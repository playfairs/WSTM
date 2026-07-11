use crate::{audio::AudioState, config::{AppConfig, PresetKind, Theme}};
use egui::{Color32, Context, RichText};

#[derive(Debug)]
pub struct UiController {
    pub config: AppConfig,
    pub audio_state: AudioState,
}

impl UiController {
    pub fn new(config: AppConfig, audio_state: AudioState) -> Self {
        Self { config, audio_state }
    }

    pub fn draw(&mut self, ctx: &Context, elapsed: f32, fps: f32) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(RichText::new("WTSM").strong());
            ui.small("We See The Music");
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Preset");
                egui::ComboBox::from_label("")
                    .selected_text(format!("{:?}", self.config.preset))
                    .show_ui(ui, |ui| {
                        for preset in [PresetKind::Aurora, PresetKind::Plasma, PresetKind::Feedback, PresetKind::Nebula, PresetKind::Vortex, PresetKind::Pulse, PresetKind::Prism] {
                            ui.selectable_value(&mut self.config.preset, preset, format!("{:?}", preset));
                        }
                    });
                ui.checkbox(&mut self.config.fullscreen, "Fullscreen");
                ui.checkbox(&mut self.config.vsync, "VSync");
            });
            ui.horizontal(|ui| {
                ui.label("Bloom");
                ui.add(egui::Slider::new(&mut self.config.bloom_intensity, 0.0..=1.0));
                ui.label("Motion blur");
                ui.add(egui::Slider::new(&mut self.config.motion_blur, 0.0..=1.0));
            });
            ui.horizontal(|ui| {
                ui.label("Particles");
                ui.add(egui::Slider::new(&mut self.config.particle_count, 50.0..=800.0));
                ui.label("FPS");
                ui.label(format!("{fps:.1}"));
            });
            ui.horizontal(|ui| {
                ui.label("Theme");
                egui::ComboBox::from_label("")
                    .selected_text(format!("{:?}", self.config.theme))
                    .show_ui(ui, |ui| {
                        for theme in [Theme::Dark, Theme::Light, Theme::Aurora] {
                            ui.selectable_value(&mut self.config.theme, theme, format!("{:?}", theme));
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Audio device");
                egui::ComboBox::from_label("")
                    .selected_text(if self.config.audio_device.is_empty() { "Default" } else { &self.config.audio_device })
                    .show_ui(ui, |ui| {
                        for device in &self.audio_state.devices {
                            ui.selectable_value(&mut self.config.audio_device, device.clone(), device.as_str());
                        }
                    });
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Bass");
                ui.colored_label(Color32::from_rgb(120, 220, 255), format!("{:.2}", self.audio_state.smoothed.bass_energy));
                ui.label("Mid");
                ui.colored_label(Color32::from_rgb(255, 120, 180), format!("{:.2}", self.audio_state.smoothed.mid_energy));
                ui.label("Treble");
                ui.colored_label(Color32::from_rgb(255, 220, 100), format!("{:.2}", self.audio_state.smoothed.treble_energy));
            });
            ui.label(format!("Elapsed: {elapsed:.2}s"));
        });
    }
}
