use crate::config::{PresetKind, PresetParams};
use glam::Vec3;

#[derive(Debug, Clone)]
pub struct PresetState {
    pub kind: PresetKind,
    pub params: PresetParams,
}

impl Default for PresetState {
    fn default() -> Self {
        Self {
            kind: PresetKind::Aurora,
            params: PresetParams::default(),
        }
    }
}

impl PresetState {
    pub fn for_kind(kind: PresetKind, params: PresetParams) -> Self {
        Self { kind, params }
    }

    pub fn palette(&self, audio_energy: f32) -> Vec3 {
        let hue = (self.params.hue_shift + audio_energy * 0.35 + self.params.phase) % 1.0;
        let base = hsv_to_rgb(hue, 0.7, 0.95);
        let accent = hsv_to_rgb((hue + 0.22) % 1.0, 0.65, 0.9);
        let mixed = base.lerp(accent, self.params.intensity * 0.5 + 0.2);
        mixed * (0.7 + audio_energy * 0.7)
    }

    pub fn preset_index(&self) -> f32 {
        match self.kind {
            PresetKind::Aurora => 0.0,
            PresetKind::Plasma => 1.0,
            PresetKind::Feedback => 2.0,
            PresetKind::Nebula => 3.0,
            PresetKind::Vortex => 4.0,
            PresetKind::Pulse => 5.0,
            PresetKind::Prism => 6.0,
        }
    }
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> Vec3 {
    let h = (h % 1.0).max(0.0);
    let i = (h * 6.0).floor() as i32;
    let f = h * 6.0 - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match i % 6 {
        0 => Vec3::new(v, t, p),
        1 => Vec3::new(q, v, p),
        2 => Vec3::new(p, v, t),
        3 => Vec3::new(p, q, v),
        4 => Vec3::new(t, p, v),
        _ => Vec3::new(v, p, q),
    }
}
