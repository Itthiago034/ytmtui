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
/// Queda por frame dos "peak caps" (marcadores do pico recente de cada
/// barra) — bem mais lenta que a das barras, no estilo Winamp/Cava.
const PEAK_FALL: f32 = 0.02;

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
    /// Buffer de magnitudes reutilizado entre frames (evita uma alocação
    /// de `FFT_SIZE/2` floats por frame).
    magnitudes: Vec<f32>,
    bar_bins: Vec<(usize, usize)>,
    cached_sample_rate: u32,
    bars: [f32; BAR_COUNT],
    /// Pico recente de cada barra, caindo devagar (ver [`PEAK_FALL`]).
    peaks: [f32; BAR_COUNT],
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
            magnitudes: vec![0.0; FFT_SIZE / 2],
            bar_bins: build_bar_bins(FFT_SIZE, 44_100, BAR_COUNT),
            cached_sample_rate: 44_100,
            bars: [0.0; BAR_COUNT],
            peaks: [0.0; BAR_COUNT],
        }
    }

    /// Feeds one drained chunk into the rolling window (downmix only — no
    /// FFT). Several chunks arrive per UI tick but only the final window
    /// matters for the frame that gets drawn, so the caller pushes them all
    /// and then runs [`Self::compute_frame`] once.
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
    }

    /// Recomputes one spectrum frame from the current window: FFT, bucket
    /// into bars, apply the attack/release envelope and let the peak caps
    /// fall. One call per UI tick — the FFT runs once per drawn frame
    /// instead of once per drained chunk.
    pub fn compute_frame(&mut self) {
        if self.ring.len() < FFT_SIZE {
            return;
        }

        for (i, sample) in self.ring.iter().enumerate() {
            self.scratch[i] = Complex::new(sample * self.hann[i], 0.0);
        }
        self.fft.process(&mut self.scratch);

        for (slot, c) in self.magnitudes.iter_mut().zip(&self.scratch[..FFT_SIZE / 2]) {
            *slot = (c.re * c.re + c.im * c.im).sqrt();
        }
        let raw = bucket_bins(&self.magnitudes, &self.bar_bins);
        for (i, &r) in raw.iter().enumerate().take(BAR_COUNT) {
            self.bars[i] = smooth(self.bars[i], r, ATTACK, RELEASE);
            self.peaks[i] = self.bars[i].max(self.peaks[i] - PEAK_FALL);
        }
    }

    /// Relaxes bars toward zero when nothing is being fed in (paused, or the
    /// Home screen isn't the active section) so they settle instead of
    /// freezing mid-frame.
    pub fn decay_idle(&mut self) {
        for (bar, peak) in self.bars.iter_mut().zip(self.peaks.iter_mut()) {
            *bar = smooth(*bar, 0.0, ATTACK, RELEASE);
            *peak = bar.max(*peak - PEAK_FALL);
        }
    }

    /// Current smoothed bar heights, each in `[0.0, 1.0]`.
    pub fn bars(&self) -> &[f32; BAR_COUNT] {
        &self.bars
    }

    /// Recent peak per bar, each in `[0.0, 1.0]`, falling slowly.
    pub fn peaks(&self) -> &[f32; BAR_COUNT] {
        &self.peaks
    }

    /// Clears the rolling sample window, bar heights and peak caps. Called
    /// on track change so the previous track's residual spectrum can't
    /// blend into the next track's first frames.
    pub fn reset(&mut self) {
        self.ring.clear();
        self.bars = [0.0; BAR_COUNT];
        self.peaks = [0.0; BAR_COUNT];
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
    fn peaks_fall_slower_than_bars_and_never_below_them() {
        let mut analyzer = SpectrumAnalyzer::new();
        analyzer.bars = [0.8; BAR_COUNT];
        analyzer.peaks = [0.8; BAR_COUNT];
        analyzer.decay_idle();
        let (bar, peak) = (analyzer.bars()[0], analyzer.peaks()[0]);
        assert!(peak > bar, "cap lags behind the falling bar");
        assert!((peak - (0.8 - PEAK_FALL)).abs() < 1e-6);

        // A rising bar drags its cap along instead of passing through it.
        analyzer.bars = [0.9; BAR_COUNT];
        analyzer.peaks = [0.5; BAR_COUNT];
        analyzer.decay_idle();
        assert!(analyzer.peaks()[0] >= analyzer.bars()[0]);
    }

    #[test]
    fn compute_frame_lights_bars_from_a_sine_window() {
        let mut analyzer = SpectrumAnalyzer::new();
        // 440Hz a 44.1kHz, chunks mono cheios até encher a janela da FFT.
        let mut phase = 0.0f32;
        let mut fed = 0;
        while fed < FFT_SIZE {
            let mut chunk = SampleChunk {
                len: CHUNK_SAMPLES,
                channels: 1,
                sample_rate: 44_100,
                ..Default::default()
            };
            for slot in chunk.data.iter_mut() {
                *slot = ((phase * std::f32::consts::TAU).sin() * 20_000.0) as i16;
                phase += 440.0 / 44_100.0;
            }
            analyzer.push_samples(&chunk);
            fed += CHUNK_SAMPLES;
        }
        assert_eq!(*analyzer.bars(), [0.0; BAR_COUNT], "no FFT before compute_frame");
        analyzer.compute_frame();
        assert!(
            analyzer.bars().iter().any(|&b| b > 0.1),
            "a pure tone must light up at least one bar"
        );
        assert!(
            analyzer.peaks().iter().zip(analyzer.bars()).all(|(p, b)| p >= b),
            "caps sit at or above their bars"
        );
    }

    #[test]
    fn reset_clears_bars_and_the_rolling_window() {
        let mut analyzer = SpectrumAnalyzer::new();
        analyzer.bars = [0.8; BAR_COUNT];
        analyzer.peaks = [0.9; BAR_COUNT];
        analyzer.ring.extend([0.1, 0.2, 0.3]);
        analyzer.reset();
        assert_eq!(*analyzer.bars(), [0.0; BAR_COUNT]);
        assert_eq!(*analyzer.peaks(), [0.0; BAR_COUNT]);
        assert!(analyzer.ring.is_empty());
    }
}
