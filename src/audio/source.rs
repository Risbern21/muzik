use std::sync::mpsc;

use rodio::Source;

pub struct PcmCaptureSource<S>
where
    S: Source,
    S::Item: Into<f32>,
{
    input: S,
    sender: mpsc::Sender<f32>,
}

impl<S> PcmCaptureSource<S>
where
    S: Source,
    S::Item: Into<f32>,
{
    pub fn new(input: S, sender: mpsc::Sender<f32>) -> PcmCaptureSource<S> {
        PcmCaptureSource { input, sender }
    }
}

impl<S> Iterator for PcmCaptureSource<S>
where
    S: Source,
    S::Item: Into<f32>,
{
    type Item = S::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(sample) = self.input.next() {
            let raw_sample = sample;
            let _ = self.sender.send(raw_sample);

            Some(sample)
        } else {
            None
        }
    }
}

impl<S> Source for PcmCaptureSource<S>
where
    S: Source,
    S::Item: Into<f32>,
{
    fn current_span_len(&self) -> Option<usize> {
        self.input.current_span_len()
    }

    fn channels(&self) -> rodio::ChannelCount {
        self.input.channels()
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        self.input.sample_rate()
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        self.input.total_duration()
    }
}
