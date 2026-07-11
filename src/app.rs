use crate::{audio::AudioAnalyzer, config::AppConfig, renderer::Renderer, ui::UiController};
use anyhow::Result;
use tracing::info;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

pub struct WstmApp {
    window: Option<Window>,
    config: AppConfig,
    renderer: Option<Renderer>,
    audio: AudioAnalyzer,
    ui: Option<UiController>,
    last_frame: std::time::Instant,
    fps: f32,
    elapsed: f32,
}

impl WstmApp {
    pub fn new() -> Result<Self> {
        let mut config = AppConfig::load().unwrap_or_default();
        let mut audio = AudioAnalyzer::new();
        if config.audio_device.is_empty() {
            if let Some(default_output) = audio.available_devices().iter().find(|name| name.starts_with("output: ")) {
                config.audio_device = default_output.clone();
            }
        }
        let _ = audio.start(&config.audio_device);
        Ok(Self {
            window: None,
            config: config.clone(),
            renderer: None,
            audio,
            ui: None,
            last_frame: std::time::Instant::now(),
            fps: 60.0,
            elapsed: 0.0,
        })
    }

    pub fn run(mut self) -> Result<()> {
        let event_loop = EventLoop::new()?;
        event_loop.run_app(&mut self)?;
        Ok(())
    }
}

impl ApplicationHandler for WstmApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = event_loop.create_window(Window::default_attributes().with_title("WTSM")).unwrap();
        self.window = Some(window);
        let window = self.window.as_ref().unwrap();
        let renderer = pollster::block_on(Renderer::new(
            window,
            crate::renderer::RendererConfig::new(
                1280,
                720,
                self.config.bloom_intensity,
                self.config.motion_blur,
                self.config.particle_count,
                self.config.vsync,
            ),
        ))
        .unwrap();
        let mut audio_state = crate::audio::AudioState::default();
        audio_state.devices = self.audio.available_devices();
        self.renderer = Some(renderer);
        self.ui = Some(UiController::new(self.config.clone(), audio_state));
        info!("WTSM renderer initialized");
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::KeyboardInput { event: KeyEvent { physical_key: winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape), state: ElementState::Pressed, .. }, .. } => {
                let window = self.window.as_ref().unwrap();
                if self.config.fullscreen {
                    window.set_fullscreen(None);
                    self.config.fullscreen = false;
                } else {
                    window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
                    self.config.fullscreen = true;
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = std::time::Instant::now();
        let dt = (now - self.last_frame).as_secs_f32();
        self.last_frame = now;
        self.elapsed += dt;
        self.fps = if dt > 0.0 { 1.0 / dt } else { self.fps };
        self.audio.update(dt);
        let metrics = self.audio.metrics();
        if let Some(ui) = self.ui.as_mut() {
            ui.audio_state.smoothed = metrics;
            let ctx = egui::Context::default();
            let mut raw_input = egui::RawInput::default();
            if let Some(window) = self.window.as_ref() {
                let size = window.inner_size();
                raw_input.screen_rect = Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(size.width as f32, size.height as f32),
                ));
            }
            ctx.begin_pass(raw_input);
            ui.draw(&ctx, self.elapsed, self.fps);
            let _output = ctx.end_pass();
        }
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.render(self.elapsed, metrics);
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
        event_loop.set_control_flow(ControlFlow::Poll);
    }
}
