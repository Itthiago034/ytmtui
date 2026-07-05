//! Wraps a decoded audio `Source` to also forward its samples to the Home
//! screen's spectrum visualizer, without altering what actually gets played.
//!
//! `next()` runs on whatever thread pulls samples from the `Sink` (rodio's
//! internal mixer, driven by the `cpal` device callback) — not the app's
//! `ytmtui-audio` command thread. Forwarding must stay cheap and
//! non-blocking: samples are batched locally and handed off via a bounded
//! channel's `try_send`, which drops a chunk under backpressure instead of
//! blocking playback.

use std::sync::mpsc::SyncSender;

use rodio::Source;

use crate::visualizer::{SampleChunk, CHUNK_SAMPLES};

pub struct SpectrumTap<S> {
    inner: S,
    tx: SyncSender<SampleChunk>,
    buf: [i16; CHUNK_SAMPLES],
    buf_len: usize,
    channels: u16,
    sample_rate: u32,
}

impl<S> SpectrumTap<S>
where
    S: Source<Item = i16>,
{
    pub fn new(inner: S, tx: SyncSender<SampleChunk>) -> Self {
        let channels = inner.channels();
        let sample_rate = inner.sample_rate();
        Self {
            inner,
            tx,
            buf: [0; CHUNK_SAMPLES],
            buf_len: 0,
            channels,
            sample_rate,
        }
    }

    fn flush(&mut self) {
        if self.buf_len == 0 {
            return;
        }
        let chunk = SampleChunk {
            data: self.buf,
            len: self.buf_len,
            channels: self.channels,
            sample_rate: self.sample_rate,
        };
        let _ = self.tx.try_send(chunk);
        self.buf_len = 0;
    }
}

impl<S> Iterator for SpectrumTap<S>
where
    S: Source<Item = i16>,
{
    type Item = i16;

    fn next(&mut self) -> Option<i16> {
        match self.inner.next() {
            Some(sample) => {
                self.buf[self.buf_len] = sample;
                self.buf_len += 1;
                if self.buf_len == self.buf.len() {
                    self.flush();
                }
                Some(sample)
            }
            None => {
                self.flush();
                None
            }
        }
    }
}

impl<S> Source for SpectrumTap<S>
where
    S: Source<Item = i16>,
{
    fn current_frame_len(&self) -> Option<usize> {
        self.inner.current_frame_len()
    }
    fn channels(&self) -> u16 {
        self.inner.channels()
    }
    fn sample_rate(&self) -> u32 {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<std::time::Duration> {
        self.inner.total_duration()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeSource {
        data: std::vec::IntoIter<i16>,
    }

    impl Iterator for FakeSource {
        type Item = i16;
        fn next(&mut self) -> Option<i16> {
            self.data.next()
        }
    }

    impl Source for FakeSource {
        fn current_frame_len(&self) -> Option<usize> {
            None
        }
        fn channels(&self) -> u16 {
            2
        }
        fn sample_rate(&self) -> u32 {
            44_100
        }
        fn total_duration(&self) -> Option<std::time::Duration> {
            None
        }
    }

    #[test]
    fn forwards_samples_unchanged_and_emits_matching_chunks() {
        let samples: Vec<i16> = (0..2500).map(|i| (i % 100) as i16).collect();
        let fake = FakeSource {
            data: samples.clone().into_iter(),
        };
        let (tx, rx) = std::sync::mpsc::sync_channel(16);
        let tap = SpectrumTap::new(fake, tx);

        let forwarded: Vec<i16> = tap.collect();
        assert_eq!(forwarded, samples);

        let mut received = Vec::new();
        while let Ok(chunk) = rx.try_recv() {
            received.extend_from_slice(&chunk.data[..chunk.len]);
        }
        assert_eq!(received, samples);
    }
}
