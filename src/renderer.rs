use anyhow::Result;
use std::borrow::Cow;
use wgpu::util::DeviceExt;
use winit::dpi::PhysicalSize;

pub const SAMPLE_COUNT: usize = 1024;
const PATTERN_MODE_COUNT: u32 = 20;

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

#[derive(Debug, Clone, Copy)]
struct PatternTransition {
    current_mode: u32,
    next_mode: u32,
    transition: f32,
    transition_started_at: f32,
    next_switch_at: f32,
    transition_duration: f32,
    hold_duration: f32,
    rng_state: u64,
    seed: f32,
    flow_seed: f32,
    chaos_seed: f32,
    twist_seed: f32,
    drift_seed: f32,
    pulse_seed: f32,
    motion_speed: f32,
    warp_scale: f32,
}

impl Default for PatternTransition {
    fn default() -> Self {
        Self::new()
    }
}

impl PatternTransition {
    fn new() -> Self {
        let mut state = Self {
            current_mode: 0,
            next_mode: 1,
            transition: 1.0,
            transition_started_at: 0.0,
            next_switch_at: 0.0,
            transition_duration: 1.2,
            hold_duration: 3.8,
            rng_state: 0x6a09e667f3bcc909u64,
            seed: 0.0,
            flow_seed: 0.0,
            chaos_seed: 0.0,
            twist_seed: 0.0,
            drift_seed: 0.0,
            pulse_seed: 0.0,
            motion_speed: 1.0,
            warp_scale: 1.0,
        };
        state.next_mode = state.pick_next_mode(0);
        state.reseed();
        state
    }

    fn update(&mut self, time: f32, beat: f32) {
        let beat_push = if beat > 0.7 { 0.95 + self.random_f32() * 0.65 } else { 0.0 };
        let should_switch = time >= self.next_switch_at - beat_push;
        if should_switch {
            self.current_mode = self.next_mode;
            self.next_mode = self.pick_next_mode(self.current_mode);
            self.transition = 0.0;
            self.transition_started_at = time;
            self.transition_duration = 0.8 + self.random_f32() * 1.4;
            self.hold_duration = 1.8 + self.random_f32() * 3.2;
            self.next_switch_at = time + self.transition_duration + self.hold_duration;
            self.reseed();
        }

        let elapsed = (time - self.transition_started_at).max(0.0);
        self.transition = (elapsed / self.transition_duration).clamp(0.0, 1.0);
    }

    fn reseed(&mut self) {
        self.seed = self.random_f32();
        self.flow_seed = self.random_f32() * 2.0 - 1.0;
        self.chaos_seed = self.random_f32() * 2.0 - 1.0;
        self.twist_seed = self.random_f32() * 2.0 - 1.0;
        self.drift_seed = self.random_f32() * 2.0 - 1.0;
        self.pulse_seed = self.random_f32() * 2.0 - 1.0;
        self.motion_speed = 0.6 + self.random_f32() * 2.8;
        self.warp_scale = 0.7 + self.random_f32() * 1.8;
    }

    fn pick_next_mode(&mut self, current_mode: u32) -> u32 {
        let mut candidate = (self.rng_state % PATTERN_MODE_COUNT as u64) as u32;
        let mut attempts = 0;
        while candidate == current_mode && attempts < PATTERN_MODE_COUNT as i32 {
            self.rng_state = self.rng_state.wrapping_mul(1664525).wrapping_add(1013904223);
            candidate = (self.rng_state % PATTERN_MODE_COUNT as u64) as u32;
            attempts += 1;
        }
        self.rng_state = self.rng_state.wrapping_mul(1664525).wrapping_add(1013904223);
        candidate
    }

    fn random_f32(&mut self) -> f32 {
        self.rng_state = self.rng_state.wrapping_mul(1664525).wrapping_add(1013904223);
        ((self.rng_state & 0x00ff_ffff) as f32 / 16_777_215.0).clamp(0.0, 1.0)
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
    pub sim_pipelines: Vec<wgpu::RenderPipeline>,
    pub display_pipelines: Vec<wgpu::RenderPipeline>,
    pub uniform_buffers: [wgpu::Buffer; 2],
    pub uniform_bind_group_layout: wgpu::BindGroupLayout,
    pub uniform_bind_groups: [wgpu::BindGroup; 2],
    pub texture_bind_group_layout: wgpu::BindGroupLayout,
    pub feedback_bind_groups: [wgpu::BindGroup; 2],
    pub feedback_textures: [wgpu::Texture; 2],
    pub feedback_views: [wgpu::TextureView; 2],
    pub feedback_sampler: wgpu::Sampler,
    pub current_feedback: usize,
    pub frame: usize,
    pub sample_buffer: wgpu::Buffer,
    pub sample_bind_group_layout: wgpu::BindGroupLayout,
    pub sample_bind_group: wgpu::BindGroup,
    pattern_state: PatternTransition,
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

        let uniform_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("uniform-bind-group-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });

        let uniform_buffers = [
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("uniform-buffer-0"),
                contents: bytemuck::cast_slice(&[Uniforms::default()]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }),
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("uniform-buffer-1"),
                contents: bytemuck::cast_slice(&[Uniforms::default()]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }),
        ];

        let uniform_bind_groups = [
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("uniform-bind-group-0"),
                layout: &uniform_bind_group_layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform_buffers[0].as_entire_binding() }],
            }),
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("uniform-bind-group-1"),
                layout: &uniform_bind_group_layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform_buffers[1].as_entire_binding() }],
            }),
        ];

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

        let sample_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sample-bind-group-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline-layout"),
            bind_group_layouts: &[&uniform_bind_group_layout, &texture_bind_group_layout, &sample_bind_group_layout],
            push_constant_ranges: &[],
        });

        let shader_sources = [
            include_str!("shaders/visualizer.wgsl"),
            // include_str!("shaders/variants/00.wgsl"),
            // include_str!("shaders/variants/01.wgsl"),
            // include_str!("shaders/variants/02.wgsl"),
            // include_str!("shaders/variants/03.wgsl"),
            // include_str!("shaders/variants/04.wgsl"),
            // include_str!("shaders/variants/05.wgsl"),
            // include_str!("shaders/variants/06.wgsl"),
            // include_str!("shaders/variants/07.wgsl"),
            // include_str!("shaders/variants/08.wgsl"),
            // include_str!("shaders/variants/09.wgsl"),
            // include_str!("shaders/variants/10.wgsl"),
            // include_str!("shaders/variants/11.wgsl"),
            // include_str!("shaders/variants/12.wgsl"),
            // include_str!("shaders/variants/13.wgsl"),
            // include_str!("shaders/variants/14.wgsl"),
            // include_str!("shaders/variants/15.wgsl"),
            // include_str!("shaders/variants/16.wgsl"),
            // include_str!("shaders/variants/17.wgsl"),
            // include_str!("shaders/variants/18.wgsl"),
            // include_str!("shaders/variants/19.wgsl"),
        ];
        let mut sim_pipelines = Vec::with_capacity(shader_sources.len());
        let mut display_pipelines = Vec::with_capacity(shader_sources.len());
        for (index, source) in shader_sources.iter().enumerate() {
            let label = format!("wstm-shader-{}", index);
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label.as_str()),
                source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(source)),
            });
            sim_pipelines.push(device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            }));
            display_pipelines.push(device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
            }));
        }

        const SAMPLE_COUNT: usize = 1024;
        let sample_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sample-buffer"),
            size: (SAMPLE_COUNT * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let sample_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sample-bind-group"),
            layout: &sample_bind_group_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: sample_buffer.as_entire_binding() }],
        });

        Ok(Self {
            surface,
            device,
            queue,
            config,
            size,
            adapter,
            sim_pipelines,
            display_pipelines,
            uniform_buffers,
            uniform_bind_group_layout,
            uniform_bind_groups,
            texture_bind_group_layout,
            feedback_bind_groups,
            feedback_textures,
            feedback_views,
            feedback_sampler,
            sample_buffer,
            sample_bind_group_layout,
            sample_bind_group,
            current_feedback: 0,
            frame: 0,
            pattern_state: PatternTransition::new(),
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

    pub fn render(&mut self, time: f32, audio: crate::audio::AudioMetrics, samples: &[f32]) {
        self.frame += 1;
        self.pattern_state.update(time, audio.beat);
        let base_uniforms = Uniforms::from_audio(
            time,
            audio,
            self.frame as f32,
            self.size.width as f32,
            self.size.height as f32,
            self.pattern_state.current_mode,
            self.pattern_state.next_mode,
            self.pattern_state.transition,
            self.pattern_state.seed,
            self.pattern_state.flow_seed,
            self.pattern_state.chaos_seed,
            self.pattern_state.twist_seed,
            self.pattern_state.drift_seed,
            self.pattern_state.pulse_seed,
            self.pattern_state.motion_speed,
            self.pattern_state.warp_scale,
        );
        let current_uniforms = Uniforms { blend_alpha: 1.0 - self.pattern_state.transition, ..base_uniforms };
        let next_uniforms = Uniforms { blend_alpha: self.pattern_state.transition, ..base_uniforms };
        self.queue.write_buffer(&self.uniform_buffers[0], 0, bytemuck::cast_slice(&[current_uniforms]));
        self.queue.write_buffer(&self.uniform_buffers[1], 0, bytemuck::cast_slice(&[next_uniforms]));

        let mut clipped = vec![0.0f32; SAMPLE_COUNT];
        if !samples.is_empty() {
            let src_len = samples.len().min(SAMPLE_COUNT);
            let src_start = samples.len().saturating_sub(src_len);
            clipped[SAMPLE_COUNT - src_len..].copy_from_slice(&samples[src_start..src_start + src_len]);
        }
        self.queue.write_buffer(&self.sample_buffer, 0, bytemuck::cast_slice(&clipped));

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
            let current_sim_pipeline = &self.sim_pipelines[self.pattern_state.current_mode as usize % self.sim_pipelines.len()];
            pass.set_pipeline(current_sim_pipeline);
            pass.set_bind_group(0, &self.uniform_bind_groups[0], &[]);
            pass.set_bind_group(1, &self.feedback_bind_groups[prev], &[]);
            pass.set_bind_group(2, &self.sample_bind_group, &[]);
            pass.draw(0..3, 0..1);
            let next_sim_pipeline = &self.sim_pipelines[self.pattern_state.next_mode as usize % self.sim_pipelines.len()];
            pass.set_pipeline(next_sim_pipeline);
            pass.set_bind_group(0, &self.uniform_bind_groups[1], &[]);
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
            let display_pipeline = &self.display_pipelines[self.pattern_state.current_mode as usize % self.display_pipelines.len()];
            pass.set_pipeline(display_pipeline);
            pass.set_bind_group(0, &self.uniform_bind_groups[0], &[]);
            pass.set_bind_group(1, &self.feedback_bind_groups[next], &[]);
            pass.set_bind_group(2, &self.sample_bind_group, &[]);
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
    pattern_a: i32,
    pattern_b: i32,
    transition: f32,
    blend_alpha: f32,
    pattern_seed: f32,
    flow_seed: f32,
    chaos_seed: f32,
    twist_seed: f32,
    drift_seed: f32,
    pulse_seed: f32,
    motion_speed: f32,
    warp_scale: f32,
}

impl Default for Uniforms {
    fn default() -> Self {
        Self { time: 0.0, bass: 0.0, mid: 0.0, treble: 0.0, rms: 0.0, peak: 0.0, beat: 0.0, centroid: 0.0, rolloff: 0.0, loudness: 0.0, bloom: 0.72, motion_blur: 0.45, particle_count: 220.0, frame: 0.0, width: 1920.0, height: 1080.0, hue_shift: 0.0, pattern_a: 0, pattern_b: 1, transition: 1.0, blend_alpha: 1.0, pattern_seed: 0.0, flow_seed: 0.0, chaos_seed: 0.0, twist_seed: 0.0, drift_seed: 0.0, pulse_seed: 0.0, motion_speed: 1.0, warp_scale: 1.0 }
    }
}

impl Uniforms {
    fn from_audio(
        time: f32,
        audio: crate::audio::AudioMetrics,
        frame: f32,
        width: f32,
        height: f32,
        pattern_a: u32,
        pattern_b: u32,
        transition: f32,
        pattern_seed: f32,
        flow_seed: f32,
        chaos_seed: f32,
        twist_seed: f32,
        drift_seed: f32,
        pulse_seed: f32,
        motion_speed: f32,
        warp_scale: f32,
    ) -> Self {
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
            pattern_a: pattern_a as i32,
            pattern_b: pattern_b as i32,
            transition,
            blend_alpha: 1.0,
            pattern_seed,
            flow_seed,
            chaos_seed,
            twist_seed,
            drift_seed,
            pulse_seed,
            motion_speed,
            warp_scale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_transition_never_reuses_current_mode() {
        let mut state = PatternTransition::new();
        let next = state.pick_next_mode(0);
        assert_ne!(next, 0);
        assert!(next < PATTERN_MODE_COUNT);
    }
}
