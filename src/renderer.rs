use anyhow::Result;
use std::borrow::Cow;
use wgpu::util::DeviceExt;
use winit::dpi::PhysicalSize;

#[derive(Debug, Clone, Copy)]
pub struct RendererConfig {
    pub width: u32,
    pub height: u32,
    pub bloom_intensity: f32,
    pub motion_blur: f32,
    pub particle_count: f32,
    pub vsync: bool,
}

impl RendererConfig {
    pub fn new(width: u32, height: u32, bloom_intensity: f32, motion_blur: f32, particle_count: f32, vsync: bool) -> Self {
        Self { width, height, bloom_intensity, motion_blur, particle_count, vsync }
    }
}

#[derive(Debug)]
pub struct Renderer {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub size: PhysicalSize<u32>,
    pub adapter: wgpu::Adapter,
    pub sim_pipeline: wgpu::RenderPipeline,
    pub display_pipeline: wgpu::RenderPipeline,
    pub uniform_buffer: wgpu::Buffer,
    pub uniform_bind_group_layout: wgpu::BindGroupLayout,
    pub uniform_bind_group: wgpu::BindGroup,
    pub texture_bind_group_layout: wgpu::BindGroupLayout,
    pub feedback_bind_groups: [wgpu::BindGroup; 2],
    pub feedback_textures: [wgpu::Texture; 2],
    pub feedback_views: [wgpu::TextureView; 2],
    pub feedback_sampler: wgpu::Sampler,
    pub current_feedback: usize,
    pub frame: usize,
}

impl Renderer {
    pub async fn new(window: &winit::window::Window, _config: RendererConfig) -> Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor { backends: wgpu::Backends::all(), ..Default::default() });
        let surface = instance.create_surface(window)?;
        let surface: wgpu::Surface<'static> = unsafe { std::mem::transmute(surface) };
        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }).await.ok_or_else(|| anyhow::anyhow!("No suitable GPU adapter found"))?;
        let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor {
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            label: Some("wstm-device"),
            memory_hints: wgpu::MemoryHints::Performance,
        }, None).await?;
        let size = window.inner_size();
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps.formats.iter().copied().find(|f| f.is_srgb()).unwrap_or(surface_caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("wstm-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/visualizer.wgsl"))),
        });

        let uniform_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("uniform-bind-group-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("uniform-buffer"),
            contents: bytemuck::cast_slice(&[Uniforms::default()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("uniform-bind-group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() }],
        });

        let texture_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("texture-bind-group-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let feedback_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("feedback-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let feedback_textures = [
            Self::create_feedback_texture(&device, size.width, size.height),
            Self::create_feedback_texture(&device, size.width, size.height),
        ];
        let feedback_views = [
            feedback_textures[0].create_view(&wgpu::TextureViewDescriptor::default()),
            feedback_textures[1].create_view(&wgpu::TextureViewDescriptor::default()),
        ];

        let feedback_bind_groups = [
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("feedback-bind-group-0"),
                layout: &texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&feedback_views[0]) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&feedback_sampler) },
                ],
            }),
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("feedback-bind-group-1"),
                layout: &texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&feedback_views[1]) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&feedback_sampler) },
                ],
            }),
        ];

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline-layout"),
            bind_group_layouts: &[&uniform_bind_group_layout, &texture_bind_group_layout],
            push_constant_ranges: &[],
        });

        let sim_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sim-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("sim_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let display_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("display-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("display_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Ok(Self {
            surface,
            device,
            queue,
            config,
            size,
            adapter,
            sim_pipeline,
            display_pipeline,
            uniform_buffer,
            uniform_bind_group_layout,
            uniform_bind_group,
            texture_bind_group_layout,
            feedback_bind_groups,
            feedback_textures,
            feedback_views,
            feedback_sampler,
            current_feedback: 0,
            frame: 0,
        })
    }

    fn create_feedback_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("feedback-texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.size.width = width.max(1);
        self.size.height = height.max(1);
        self.config.width = self.size.width;
        self.config.height = self.size.height;
        self.surface.configure(&self.device, &self.config);

        self.feedback_textures = [
            Self::create_feedback_texture(&self.device, self.size.width, self.size.height),
            Self::create_feedback_texture(&self.device, self.size.width, self.size.height),
        ];
        self.feedback_views = [
            self.feedback_textures[0].create_view(&wgpu::TextureViewDescriptor::default()),
            self.feedback_textures[1].create_view(&wgpu::TextureViewDescriptor::default()),
        ];
        self.feedback_bind_groups = [
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("feedback-bind-group-0"),
                layout: &self.texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&self.feedback_views[0]) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.feedback_sampler) },
                ],
            }),
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("feedback-bind-group-1"),
                layout: &self.texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&self.feedback_views[1]) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.feedback_sampler) },
                ],
            }),
        ];
    }

    pub fn render(&mut self, time: f32, audio: crate::audio::AudioMetrics) {
        self.frame += 1;
        let uniforms = Uniforms::from_audio(time, audio, self.frame as f32, self.size.width as f32, self.size.height as f32);
        self.queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let prev = self.current_feedback;
        let next = 1 - self.current_feedback;
        let output = self.surface.get_current_texture().expect("surface texture");
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("encoder") });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("simulation-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.feedback_views[next],
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store, ..Default::default() },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.sim_pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_bind_group(1, &self.feedback_bind_groups[prev], &[]);
            pass.draw(0..3, 0..1);
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("display-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store, ..Default::default() },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.display_pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_bind_group(1, &self.feedback_bind_groups[next], &[]);
            pass.draw(0..3, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        self.current_feedback = next;
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    time: f32,
    bass: f32,
    mid: f32,
    treble: f32,
    rms: f32,
    peak: f32,
    beat: f32,
    centroid: f32,
    rolloff: f32,
    loudness: f32,
    bloom: f32,
    motion_blur: f32,
    particle_count: f32,
    frame: f32,
    width: f32,
    height: f32,
    hue_shift: f32,
}

impl Default for Uniforms {
    fn default() -> Self {
        Self { time: 0.0, bass: 0.0, mid: 0.0, treble: 0.0, rms: 0.0, peak: 0.0, beat: 0.0, centroid: 0.0, rolloff: 0.0, loudness: 0.0, bloom: 0.72, motion_blur: 0.45, particle_count: 220.0, frame: 0.0, width: 1920.0, height: 1080.0, hue_shift: 0.0 }
    }
}

impl Uniforms {
    fn from_audio(time: f32, audio: crate::audio::AudioMetrics, frame: f32, width: f32, height: f32) -> Self {
        Self {
            time,
            bass: audio.bass_energy,
            mid: audio.mid_energy,
            treble: audio.treble_energy,
            rms: audio.rms,
            peak: audio.peak_level,
            centroid: audio.spectral_centroid,
            rolloff: audio.spectral_rolloff,
            loudness: audio.loudness,
            bloom: 0.72,
            motion_blur: 0.45,
            particle_count: 220.0,
            frame,
            width,
            height,
            hue_shift: (audio.spectral_centroid * 0.65 + audio.beat * 0.18),
            beat: audio.beat,
        }
    }
}
