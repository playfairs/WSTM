use anyhow::Context;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
    Aurora,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PresetKind {
    Aurora,
    Plasma,
    Feedback,
    Nebula,
    Vortex,
    Pulse,
    Prism,
}

impl Default for PresetKind {
    fn default() -> Self {
        Self::Aurora
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PresetParams {
    pub intensity: f32,
    pub flow: f32,
    pub chaos: f32,
    pub density: f32,
    pub phase: f32,
    pub hue_shift: f32,
    pub feedback: f32,
    pub energy_scale: f32,
}

impl Default for PresetParams {
    fn default() -> Self {
        Self {
            intensity: 0.9,
            flow: 0.55,
            chaos: 0.35,
            density: 0.7,
            phase: 0.25,
            hue_shift: 0.15,
            feedback: 0.6,
            energy_scale: 0.75,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub fullscreen: bool,
    pub bloom_intensity: f32,
    pub motion_blur: f32,
    pub particle_count: f32,
    pub vsync: bool,
    pub theme: Theme,
    pub audio_device: String,
    pub preset: PresetKind,
    pub preset_params: PresetParams,
    pub export_path: String,
    pub import_path: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            fullscreen: false,
            bloom_intensity: 0.72,
            motion_blur: 0.45,
            particle_count: 220.0,
            vsync: true,
            theme: Theme::Aurora,
            audio_device: String::new(),
            preset: PresetKind::Aurora,
            preset_params: PresetParams::default(),
            export_path: String::new(),
            import_path: String::new(),
        }
    }
}

impl AppConfig {
    pub fn config_path() -> anyhow::Result<PathBuf> {
        let dirs = ProjectDirs::from("dev", "femboy", "wstm")
            .context("Unable to resolve a config directory for WTSM")?;
        let config_dir = dirs.config_dir();
        if !config_dir.exists() {
            fs::create_dir_all(config_dir)?;
        }
        Ok(config_dir.join("config.json"))
    }

    pub fn load() -> anyhow::Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(&path)?;
        let config: Self = serde_json::from_str(&raw)?;
        Ok(config)
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::config_path()?;
        let raw = serde_json::to_string_pretty(self)?;
        fs::write(path, raw)?;
        Ok(())
    }

    pub fn export_preset(&self, path: &Path) -> anyhow::Result<()> {
        let payload = serde_json::json!({
            "preset": self.preset,
            "params": self.preset_params,
        });
        fs::write(path, serde_json::to_string_pretty(&payload)?)?;
        Ok(())
    }

    pub fn import_preset(&mut self, path: &Path) -> anyhow::Result<()> {
        let raw = fs::read_to_string(path)?;
        let payload: serde_json::Value = serde_json::from_str(&raw)?;
        self.preset = serde_json::from_value(payload["preset"].clone())?;
        self.preset_params = serde_json::from_value(payload["params"].clone())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trips_to_json() {
        let config = AppConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let reloaded: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(reloaded.preset, PresetKind::Aurora);
        assert!(reloaded.vsync);
    }
}
