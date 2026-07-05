//! Real-time audio spectrum visualizer for the Home screen.
//!
//! Playback samples are tapped as they're decoded (see `player::tap`),
//! downmixed to mono, windowed and run through an FFT, then bucketed into a
//! fixed number of logarithmically-spaced bars (bass to treble, Cava-style)
//! with an attack/release envelope so the bars rise quickly but fall with a
//! bit of "gravity" instead of jittering.

use std::collections::VecDeque;
use std::sync::Arc;

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

/// Interleaved samples batched from the audio thread per send. Fixed-size
/// and `Copy` so a chunk can cross the sample channel without allocating.
pub const CHUNK_SAMPLES: usize = 1024;

#[derive(Clone, Copy)]
pub struct SampleChunk {
    pub data: [i16; CHUNK_SAMPLES],
    pub len: usize,
    pub channels: u16,
    pub sample_rate: u32,
}

impl Default for SampleChunk {
    fn default() -> Self {
        Self {
            data: [0; CHUNK_SAMPLES],
            len: 0,
            channels: 2,
            sample_rate: 44_100,
        }
    }
}

const FFT_SIZE: usize = 2048;
/// Number of visualizer bars rendered on the Home screen.
pub const BAR_COUNT: usize = 32;
/// Bass floor for the log-spaced bar frequency edges, matching Cava's feel.
const MIN_FREQ_HZ: f32 = 40.0;
/// Fast rise when a bar's energy increases.
const ATTACK: f32 = 0.55;
/// Slower "gravity" fall when a bar's energy decreases.
const RELEASE: f32 = 0.12;

/// Downmixes interleaved samples to mono `f32` in `[-1.0, 1.0]`.
fn downmix_to_mono(data: &[i16], channels: u16) -> Vec<f32> {
    let channels = channels.max(1) as usize;
    data.chunks(channels)
        .map(|frame| {
            let sum: i32 = frame.iter().map(|&s| s as i32).sum();
            (sum as f32 / frame.len() as f32) / i16::MAX as f32
        })
        .collect()
}

/// Attack/release envelope: moves `prev` toward `raw` faster when rising,
/// slower when falling, and always stays within `[0.0, 1.0]`.
fn smooth(prev: f32, raw: f32, attack: f32, release: f32) -> f32 {
    let rate = if raw > prev { attack } else { release };
    (prev + (raw - prev) * rate).clamp(0.0, 1.0)
}

/// Builds `bar_count` log-spaced, non-overlapping, contiguous bin ranges
/// `[start, end)` over the usable FFT bins (bin 0, the DC offset, is never
/// included). Edges are nudged to stay strictly increasing where possible,
/// so bars don't collapse onto the exact same bin in the sparse low-bass
/// region; at the real `FFT_SIZE`/44.1kHz configuration there are enough
/// bins for every bar. Only in a degenerate config with far fewer usable
/// bins than `bar_count` can trailing edges still tie at `max_bin - 1` —
/// `bucket_bins` treats an empty range as zero energy rather than panicking.
fn build_bar_bins(fft_size: usize, sample_rate: u32, bar_count: usize) -> Vec<(usize, usize)> {
    let nyquist = sample_rate as f32 / 2.0;
    let max_bin = (fft_size / 2).max(2);
    let bin_hz = sample_rate as f32 / fft_size as f32;

    let mut edges = Vec::with_capacity(bar_count + 1);
    for i in 0..=bar_count {
        let t = i as f32 / bar_count as f32;
        let freq = MIN_FREQ_HZ * (nyquist / MIN_FREQ_HZ).powf(t);
        let bin = ((freq / bin_hz).round() as usize).clamp(1, max_bin - 1);
        edges.push(bin);
    }
    for i in 1..edges.len() {
        if edges[i] <= edges[i - 1] {
            edges[i] = (edges[i - 1] + 1).min(max_bin - 1);
        }
    }
    edges.windows(2).map(|w| (w[0], w[1])).collect()
}

/// Averages the magnitude spectrum within each bar's bin range, compresses
/// it (music energy spans orders of magnitude), and clamps to `[0.0, 1.0]`.
fn bucket_bins(magnitudes: &[f32], bar_bins: &[(usize, usize)]) -> Vec<f32> {
    bar_bins
        .iter()
        .map(|&(start, end)| {
            let end = end.max(start + 1).min(magnitudes.len());
            if start >= end {
                return 0.0;
            }
            let avg = magnitudes[start..end].iter().sum::<f32>() / (end - start) as f32;
            (avg.ln_1p() / 4.0).clamp(0.0, 1.0)
        })
        .collect()
}

/// Rolling FFT-based spectrum analyzer feeding the Home screen's bars.
pub struct SpectrumAnalyzer {
    ring: VecDeque<f32>,
    hann: Vec<f32>,
    fft: Arc<dyn Fft<f32>>,
    scratch: Vec<Complex<f32>>,
    bar_bins: Vec<(usize, usize)>,
    cached_sample_rate: u32,
    bars: [f32; BAR_COUNT],
}

impl SpectrumAnalyzer {
    pub fn new() -> Self {
        let hann: Vec<f32> = (0..FFT_SIZE)
            .map(|i| {
                0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (FFT_SIZE - 1) as f32).cos()
            })
            .collect();
        let fft = FftPlanner::new().plan_fft_forward(FFT_SIZE);
        Self {
            ring: VecDeque::with_capacity(FFT_SIZE),
            hann,
            fft,
            scratch: vec![Complex::new(0.0, 0.0); FFT_SIZE],
            bar_bins: build_bar_bins(FFT_SIZE, 44_100, BAR_COUNT),
            cached_sample_rate: 44_100,
            bars: [0.0; BAR_COUNT],
        }
    }

    /// Feeds one drained chunk in: downmixes, slides it into the rolling
    /// window, and (once the window is full) recomputes one spectrum frame.
    pub fn push_samples(&mut self, chunk: &SampleChunk) {
        if chunk.sample_rate != self.cached_sample_rate && chunk.sample_rate > 0 {
            self.bar_bins = build_bar_bins(FFT_SIZE, chunk.sample_rate, BAR_COUNT);
            self.cached_sample_rate = chunk.sample_rate;
        }

        for sample in downmix_to_mono(&chunk.data[..chunk.len], chunk.channels) {
            if self.ring.len() == FFT_SIZE {
                self.ring.pop_front();
            }
            self.ring.push_back(sample);
        }

        if self.ring.len() < FFT_SIZE {
            return;
        }

        for (i, sample) in self.ring.iter().enumerate() {
            self.scratch[i] = Complex::new(sample * self.hann[i], 0.0);
        }
        self.fft.process(&mut self.scratch);

        let magnitudes: Vec<f32> = self.scratch[..FFT_SIZE / 2]
            .iter()
            .map(|c| (c.re * c.re + c.im * c.im).sqrt())
            .collect();
        let raw = bucket_bins(&magnitudes, &self.bar_bins);
        for (bar, &r) in self.bars.iter_mut().zip(raw.iter()) {
            *bar = smooth(*bar, r, ATTACK, RELEASE);
        }
    }

    /// Relaxes bars toward zero when nothing is being fed in (paused, or the
    /// Home screen isn't the active section) so they settle instead of
    /// freezing mid-frame.
    pub fn decay_idle(&mut self) {
        for bar in self.bars.iter_mut() {
            *bar = smooth(*bar, 0.0, ATTACK, RELEASE);
        }
    }

    /// Current smoothed bar heights, each in `[0.0, 1.0]`.
    pub fn bars(&self) -> &[f32; BAR_COUNT] {
        &self.bars
    }

    /// Clears the rolling sample window and bar heights. Called on track
    /// change so the previous track's residual spectrum can't blend into
    /// the next track's first frames.
    pub fn reset(&mut self) {
        self.ring.clear();
        self.bars = [0.0; BAR_COUNT];
    }
}

impl Default for SpectrumAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_cancels_out_of_phase_stereo() {
        assert_eq!(downmix_to_mono(&[1000, -1000], 2), vec![0.0]);
    }

    #[test]
    fn downmix_scales_mono_passthrough() {
        let out = downmix_to_mono(&[16_384], 1);
        assert_eq!(out.len(), 1);
        assert!((out[0] - 0.5).abs() < 0.001);
    }

    #[test]
    fn downmix_handles_empty_input() {
        assert!(downmix_to_mono(&[], 2).is_empty());
    }

    #[test]
    fn smooth_rises_faster_than_it_falls() {
        let risen = smooth(0.0, 1.0, ATTACK, RELEASE);
        let fallen = smooth(1.0, 0.0, ATTACK, RELEASE);
        assert!((risen - ATTACK).abs() < 1e-6);
        assert!((fallen - (1.0 - RELEASE)).abs() < 1e-6);
    }

    #[test]
    fn smooth_output_always_stays_in_unit_range() {
        assert_eq!(smooth(0.0, -5.0, ATTACK, RELEASE), 0.0);
        assert_eq!(smooth(0.0, 5.0, ATTACK, RELEASE), 1.0);
    }

    #[test]
    fn bar_bins_are_strictly_increasing_for_a_realistic_config() {
        let bins = build_bar_bins(FFT_SIZE, 44_100, BAR_COUNT);
        assert_eq!(bins.len(), BAR_COUNT);
        assert!(bins[0].0 >= 1, "must exclude the DC bin");
        for pair in bins.windows(2) {
            assert!(pair[0].1 <= pair[1].0);
            assert!(pair[0].0 < pair[0].1);
        }
    }

    #[test]
    fn bar_bins_never_panic_when_bars_exceed_usable_bins() {
        let bins = build_bar_bins(8, 44_100, BAR_COUNT);
        assert_eq!(bins.len(), BAR_COUNT);
        for &(start, end) in &bins {
            assert!(start <= end);
        }
    }

    #[test]
    fn bucket_bins_lights_up_the_bar_containing_the_spike() {
        let mut magnitudes = vec![0.0f32; 100];
        magnitudes[55] = 10.0;
        let bar_bins = vec![(0, 50), (50, 60), (60, 100)];
        let bars = bucket_bins(&magnitudes, &bar_bins);
        assert!(bars[1] > bars[0]);
        assert!(bars[1] > bars[2]);
    }

    #[test]
    fn decay_idle_settles_bars_toward_zero() {
        let mut analyzer = SpectrumAnalyzer::new();
        analyzer.bars = [0.8; BAR_COUNT];
        for _ in 0..50 {
            analyzer.decay_idle();
        }
        assert!(analyzer.bars().iter().all(|&b| b < 0.01));
    }

    #[test]
    fn reset_clears_bars_and_the_rolling_window() {
        let mut analyzer = SpectrumAnalyzer::new();
        analyzer.bars = [0.8; BAR_COUNT];
        analyzer.ring.extend([0.1, 0.2, 0.3]);
        analyzer.reset();
        assert_eq!(*analyzer.bars(), [0.0; BAR_COUNT]);
        assert!(analyzer.ring.is_empty());
    }
}
