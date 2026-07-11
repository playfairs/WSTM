use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rustfft::{num_complex::Complex, FftPlanner};
use std::sync::{Arc, Mutex};
use tracing::{debug, warn};

#[derive(Debug, Clone, Copy)]
pub struct AudioMetrics {
    pub bass_energy: f32,
    pub mid_energy: f32,
    pub treble_energy: f32,
    pub rms: f32,
    pub peak_level: f32,
    pub beat: f32,
    pub spectral_centroid: f32,
    pub spectral_rolloff: f32,
    pub loudness: f32,
}

impl Default for AudioMetrics {
    fn default() -> Self {
        Self {
            bass_energy: 0.0,
            mid_energy: 0.0,
            treble_energy: 0.0,
            rms: 0.0,
            peak_level: 0.0,
            beat: 0.0,
            spectral_centroid: 0.0,
            spectral_rolloff: 0.0,
            loudness: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AudioState {
    pub metrics: AudioMetrics,
    pub smoothed: AudioMetrics,
    pub devices: Vec<String>,
    pub active_device: String,
    pub sample_rate: u32,
}

impl Default for AudioState {
    fn default() -> Self {
        Self {
            metrics: AudioMetrics::default(),
            smoothed: AudioMetrics::default(),
            devices: Vec::new(),
            active_device: String::new(),
            sample_rate: 44100,
        }
    }
}

#[derive(Debug)]
pub struct AudioAnalyzer {
    ring: Arc<Mutex<Vec<f32>>>,
    metrics: Arc<Mutex<AudioMetrics>>,
    state: Arc<Mutex<AudioState>>,
    fft_size: usize,
    last_peak: f32,
    beat_counter: f32,
}

impl AudioAnalyzer {
    pub fn new() -> Self {
        let state = Arc::new(Mutex::new(AudioState::default()));
        let metrics = Arc::new(Mutex::new(AudioMetrics::default()));
        let ring = Arc::new(Mutex::new(Vec::new()));
        Self {
            ring: ring.clone(),
            metrics: metrics.clone(),
            state: state.clone(),
            fft_size: 512,
            last_peak: 0.0,
            beat_counter: 0.0,
        }
    }

    pub fn available_devices(&self) -> Vec<String> {
        let host = cpal::default_host();
        let mut devices = Vec::new();

        if let Some(device) = host.default_output_device() {
            if let Ok(name) = device.name() {
                devices.push(format!("output: {name}"));
            }
        }
        if let Some(device) = host.default_input_device() {
            if let Ok(name) = device.name() {
                devices.push(format!("input: {name}"));
            }
        }

        if let Ok(output_devices) = host.output_devices() {
            for device in output_devices {
                if let Ok(name) = device.name() {
                    devices.push(format!("output: {name}"));
                }
            }
        }
        if let Ok(input_devices) = host.input_devices() {
            for device in input_devices {
                if let Ok(name) = device.name() {
                    devices.push(format!("input: {name}"));
                }
            }
        }

        devices.sort();
        devices.dedup();
        devices
    }

    pub fn start(&mut self, device_name: &str) -> Result<()> {
        let host = cpal::default_host();
        let selected_name = device_name.strip_prefix("output: ").or_else(|| device_name.strip_prefix("input: ")).map(str::trim);

        let input_device = if let Some(name) = selected_name {
            host.input_devices()
                .ok()
                .and_then(|mut iter| iter.find(|device| device.name().ok().as_deref() == Some(name)))
                .or_else(|| {
                    warn!("Selected output device '{name}' is not available as an input device; falling back to the default input device.");
                    host.default_input_device()
                })
        } else {
            host.default_input_device()
        };

        let device = input_device
            .or_else(|| host.default_input_device())
            .ok_or_else(|| anyhow::anyhow!("No audio input device was found"))?;

        let config = device
            .default_input_config()
            .unwrap_or_else(|_| device.supported_input_configs().unwrap().next().unwrap().with_max_sample_rate());
        let sample_rate = config.sample_rate().0;
        let mut state = self.state.lock().unwrap();
        state.sample_rate = sample_rate;
        state.active_device = if device_name.is_empty() {
            device.name().unwrap_or_else(|_| "Default input".to_string())
        } else {
            device_name.to_string()
        };
        state.devices = self.available_devices();
        drop(state);

        let ring = self.ring.clone();
        let _metrics = self.metrics.clone();
        let _state = self.state.clone();
        let stream = device.build_input_stream(
            &cpal::StreamConfig::from(config.clone()),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let mut samples = ring.lock().unwrap();
                samples.extend_from_slice(data);
                if samples.len() > 4096 {
                    let drain_len = samples.len().saturating_sub(4096);
                    if drain_len > 0 {
                        samples.drain(..drain_len);
                    }
                }
                drop(samples);
            },
            move |err| warn!("Audio stream error: {err}"),
            None,
        )?;
        stream.play()?;
        debug!("Audio stream started for {}", device.name().unwrap_or_else(|_| "default".to_string()));
        Ok(())
    }

    pub fn update(&mut self, dt: f32) {
        let mut samples = self.ring.lock().unwrap();
        if samples.len() < 256 {
            drop(samples);
            self.apply_smoothing(dt, AudioMetrics::default());
            return;
        }
        let take_len = samples.len().min(self.fft_size);
        let sample_slice = samples.drain(..take_len).collect::<Vec<_>>();
        drop(samples);
        let metrics = analyse_samples(&sample_slice);
        self.metrics.lock().unwrap().clone_from(&metrics);
        self.apply_smoothing(dt, metrics);
    }

    pub fn metrics(&self) -> AudioMetrics {
        self.state.lock().unwrap().smoothed
    }

    fn apply_smoothing(&mut self, dt: f32, incoming: AudioMetrics) {
        let mut state = self.state.lock().unwrap();
        let alpha = 1.0 - (-dt * 9.0).exp();
        state.smoothed.bass_energy = lerp(state.smoothed.bass_energy, incoming.bass_energy, alpha);
        state.smoothed.mid_energy = lerp(state.smoothed.mid_energy, incoming.mid_energy, alpha);
        state.smoothed.treble_energy = lerp(state.smoothed.treble_energy, incoming.treble_energy, alpha);
        state.smoothed.rms = lerp(state.smoothed.rms, incoming.rms, alpha);
        state.smoothed.peak_level = lerp(state.smoothed.peak_level, incoming.peak_level, alpha);
        state.smoothed.beat = lerp(state.smoothed.beat, incoming.beat, alpha);
        state.smoothed.spectral_centroid = lerp(state.smoothed.spectral_centroid, incoming.spectral_centroid, alpha);
        state.smoothed.spectral_rolloff = lerp(state.smoothed.spectral_rolloff, incoming.spectral_rolloff, alpha);
        state.smoothed.loudness = lerp(state.smoothed.loudness, incoming.loudness, alpha);
        if incoming.peak_level > 0.4 && self.last_peak < 0.4 {
            self.beat_counter = 1.0;
        }
        self.last_peak = incoming.peak_level;
        if self.beat_counter > 0.0 {
            self.beat_counter = (self.beat_counter - dt * 2.0).max(0.0);
        }
        state.smoothed.beat = state.smoothed.beat.max(self.beat_counter);
    }
}

fn analyse_samples(samples: &[f32]) -> AudioMetrics {
    let fft_size = samples.len().next_power_of_two().max(256);
    let mut windowed = vec![0.0; fft_size];
    for (index, sample) in samples.iter().enumerate() {
        let normalized = (index as f32 / (fft_size as f32 - 1.0)) * std::f32::consts::PI;
        let window = 0.5 - 0.5 * (normalized).cos();
        windowed[index] = sample * window;
    }
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(fft_size);
    let mut spectrum = vec![Complex::new(0.0, 0.0); fft_size];
    for (index, sample) in windowed.iter().enumerate() {
        spectrum[index] = Complex::new(*sample, 0.0);
    }
    fft.process(&mut spectrum);

    let mut sum = 0.0f32;
    let mut peak = 0.0f32;
    let mut bass = 0.0f32;
    let mut mid = 0.0f32;
    let mut treble = 0.0f32;
    let mut sum_magnitude = 0.0f32;
    let mut centroid_numerator = 0.0f32;
    let mut rolloff = 0.0f32;
    let total = fft_size as f32;
    let bins = fft_size / 2;
    for index in 0..bins {
        let magnitude = (spectrum[index].re * spectrum[index].re + spectrum[index].im * spectrum[index].im).sqrt() / total;
        let freq = index as f32 / (fft_size as f32 / 2.0);
        sum += magnitude;
        sum_magnitude += magnitude;
        peak = peak.max(magnitude);
        if freq < 0.2 {
            bass += magnitude * 1.4;
        } else if freq < 0.55 {
            mid += magnitude * 1.0;
        } else {
            treble += magnitude * 0.8;
        }
        centroid_numerator += magnitude * freq;
        if magnitude > 0.02 && rolloff == 0.0 {
            rolloff = freq;
        }
    }
    let rms = (sum / bins as f32).sqrt();
    let loudness = (sum_magnitude / bins as f32) * 4.5;
    AudioMetrics {
        bass_energy: bass.clamp(0.0, 1.0),
        mid_energy: mid.clamp(0.0, 1.0),
        treble_energy: treble.clamp(0.0, 1.0),
        rms: rms.clamp(0.0, 1.0),
        peak_level: peak.clamp(0.0, 1.0),
        beat: if peak > 0.25 { 1.0 } else { 0.0 },
        spectral_centroid: (centroid_numerator / sum_magnitude.max(1e-5)).clamp(0.0, 1.0),
        spectral_rolloff: rolloff.clamp(0.0, 1.0),
        loudness: loudness.clamp(0.0, 1.0),
    }
}

fn lerp(current: f32, target: f32, alpha: f32) -> f32 {
    current + (target - current) * alpha
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fft_analysis_returns_nonnegative_metrics() {
        let samples = (0..512).map(|i| (i as f32 * 0.01).sin()).collect::<Vec<_>>();
        let metrics = analyse_samples(&samples);
        assert!(metrics.bass_energy >= 0.0);
        assert!(metrics.treble_energy >= 0.0);
        assert!(metrics.loudness >= 0.0);
    }
}
