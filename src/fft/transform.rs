extern crate num;
use std::f32::{self, consts::PI};

use num::complex::Complex;

const I: Complex<f32> = Complex { re: 0.0, im: 1.0 };

pub fn fft(input: &[Complex<f32>]) -> Vec<Complex<f32>> {
    fn fft_inner(buf_a: &mut [Complex<f32>], buf_b: &mut [Complex<f32>], n: usize, step: usize) {
        if step >= n {
            return;
        }

        fft_inner(buf_b, buf_a, n, step * 2);
        fft_inner(&mut buf_b[step..], &mut buf_a[step..], n, step * 2);

        let (left, right) = buf_a.split_at_mut(n / 2);

        for i in (0..n).step_by(step * 2) {
            let t = (-I * PI * (i as f32) / (n as f32)).exp() * buf_b[i + step];
            left[i / 2] = buf_b[i] + t;
            right[i / 2] = buf_b[i] - t;
        }
    }

    let n_orig = input.len();
    let n = n_orig.next_power_of_two();

    let mut buf_a = input.to_vec();
    buf_a.append(&mut vec![Complex { re: 0.0, im: 0.0 }; n - n_orig]);
    let mut buf_b = buf_a.clone();
    fft_inner(&mut buf_a, &mut buf_b, n, 1);
    buf_a
}

// pub struct Fft {
//     planner: FftPlanner<f32>,
// }

// impl Fft {
//     pub fn new() -> Fft {
//         Fft {
//             planner: FftPlanner::new(),
//         }
//     }

//     pub fn process(&mut self, samples: Vec<Vec<f32>>) -> Option<(Vec<Vec<Complex<f32>>>, usize)> {
//         if samples.is_empty() {
//             return None;
//         }

//         let mut transformed_samples = Vec::new();

//         let fft_size = samples[0].len();
//         let fft = self.planner.plan_fft_forward(fft_size);

//         for sample in samples {
//             let mut buffer: Vec<Complex<f32>> =
//                 sample.iter().map(|s| Complex { re: *s, im: 0.0 }).collect();

//             fft.process(&mut buffer);
//             transformed_samples.push(buffer);
//         }
//         Some((transformed_samples, fft_size))
//     }

//     #[allow(dead_code)]
//     pub fn get_audio_bands(
//         buffer: &[rustfft::num_complex::Complex<f32>],
//         sample_rate: u32,
//         fft_size: usize,
//     ) -> AudioBands {
//         let bin_width = sample_rate as f32 / fft_size as f32;

//         let mut audio_bands = AudioBands::new();
//         let num_bins = fft_size / 2;
//         for (k, item) in buffer.iter().enumerate().take(num_bins) {
//             let frequency = k as f32 * bin_width;
//             let amplitude = item.norm();
//             let _amplitude_db = 20.0 * (amplitude + 1e-10).log10();

//             if (20.0..250.0).contains(&frequency) {
//                 audio_bands.bass += amplitude;
//             } else if (250.0..4000.0).contains(&frequency) {
//                 audio_bands.mids += amplitude;
//             } else if (4000.0..20000.0).contains(&frequency) {
//                 audio_bands.treble += amplitude;
//             }
//         }

//         audio_bands
//     }
// }

// #[allow(dead_code)]
// #[derive(Debug)]
// pub struct AudioBands {
//     pub bass: f32,
//     pub mids: f32,
//     pub treble: f32,
// }

// impl AudioBands {
//     #[allow(dead_code)]
//     pub fn new() -> AudioBands {
//         AudioBands {
//             bass: 0.0_f32,
//             mids: 0.0_f32,
//             treble: 0.0_f32,
//         }
//     }
// }
