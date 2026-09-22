use rustfft::{FftPlanner, num_complex::Complex};

#[allow(dead_code)]
pub fn fft(samples: Vec<Vec<f32>>) -> Option<(Vec<Vec<Complex<f32>>>, usize)> {
    let mut fft_planner = FftPlanner::new();

    if samples.is_empty() {
        return None;
    }

    let mut transformed_samples = Vec::new();

    let fft_size = samples[0].len();
    let fft = fft_planner.plan_fft_forward(fft_size);

    for sample in samples {
        let mut buffer: Vec<Complex<f32>> =
            sample.iter().map(|s| Complex { re: *s, im: 0.0 }).collect();

        fft.process(&mut buffer);
        transformed_samples.push(buffer);
    }
    Some((transformed_samples, fft_size))
}

#[allow(dead_code)]
pub fn get_audio_bands(buffer: &[Complex<f32>], sample_rate: u32, fft_size: usize) -> AudioBands {
    let bin_width = sample_rate as f32 / fft_size as f32;

    let mut audio_bands = AudioBands::new();
    let num_bins = fft_size / 2;
    for (k, item) in buffer.iter().enumerate().take(num_bins) {
        let frequency = k as f32 * bin_width;
        let amplitude = item.norm();
        let _amplitude_db = 20.0 * (amplitude + 1e-10).log10();

        if (20.0..250.0).contains(&frequency) {
            audio_bands.bass += amplitude;
        } else if (250.0..4000.0).contains(&frequency) {
            audio_bands.mids += amplitude;
        } else if (4000.0..20000.0).contains(&frequency) {
            audio_bands.treble += amplitude;
        }
    }

    audio_bands
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct AudioBands {
    pub bass: f32,
    pub mids: f32,
    pub treble: f32,
}

impl AudioBands {
    #[allow(dead_code)]
    pub fn new() -> AudioBands {
        AudioBands {
            bass: 0.0_f32,
            mids: 0.0_f32,
            treble: 0.0_f32,
        }
    }
}

use std::f32::consts::PI;

#[allow(dead_code)]
pub fn calculate_dynamic_window_size(sample_rate: u32, target_duration: f32) -> usize {
    let exact_samples = sample_rate as f32 * target_duration;

    let base_two = 2.0_f32;
    let power = exact_samples.log2().round();
    let dynamic_window_size = base_two.powf(power) as usize;

    dynamic_window_size.clamp(512, 8192)
}

#[allow(dead_code)]
pub fn apply_hann_window(samples: &[f32], window_size: usize, hop_size: usize) -> Vec<Vec<f32>> {
    let hann_window = create_hann_window(window_size);
    let mut windowed_frames = Vec::new();

    let mut start = 0;
    while start + window_size <= samples.len() {
        let chunk = &samples[start..start + window_size];
        let windowed_chunk: Vec<f32> = chunk
            .iter()
            .zip(hann_window.iter())
            .map(|(&s, &w)| s * w)
            .collect();

        windowed_frames.push(windowed_chunk);
        start += hop_size;
    }

    windowed_frames
}

#[allow(dead_code)]
fn create_hann_window(window_size: usize) -> Vec<f32> {
    let mut window = Vec::with_capacity(window_size);
    for i in 0..window_size {
        let value = 0.5 * (1.0 - ((2.0 * PI * i as f32) / (window_size - 1) as f32).cos());
        window.push(value);
    }
    window
}
