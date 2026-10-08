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
