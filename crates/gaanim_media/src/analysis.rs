//! Audio analysis for audio-reactive animation.
//!
//! A file is decoded once with FFmpeg to mono samples and reduced to
//! per-frame data: the loudness envelope, a log-spaced spectrum and the
//! onsets (where a sound starts: a drum hit, a note, a syllable). Signals
//! built from it are a lookup by time, so a seek or an export reads exactly
//! the value that playback reads.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

/// Sample rate the analysis decodes to.
pub const SAMPLE_RATE: u32 = 22_050;
/// Samples between analysis frames (about 43 frames per second).
const HOP: usize = 512;
/// FFT window, in samples (about 93 ms).
const WINDOW: usize = 2048;
/// Log-spaced spectrum bins between [`MIN_HZ`] and the Nyquist frequency.
pub const BINS: usize = 96;
pub const MIN_HZ: f64 = 20.0;
/// Shortest time between two onsets, in seconds.
const ONSET_GAP: f64 = 0.08;
/// How early the spectral flux peaks before a sound starts: a centered
/// window already hears it half a window ahead. Measured on clicks and
/// drum hits; added back to every onset.
const ONSET_LATENCY: f64 = 0.025;
/// The band whose hits place the beat's phase: kick drum and bass.
const BEAT_LOW_HZ: (f64, f64) = (40.0, 150.0);

#[derive(Debug, thiserror::Error)]
pub enum AnalysisError {
    #[error("could not run ffmpeg; install FFmpeg and make ffmpeg available in PATH: {0}")]
    Unavailable(#[source] std::io::Error),
    #[error("ffmpeg could not decode audio from '{path}': {message}")]
    DecodeFailed { path: PathBuf, message: String },
    #[error("'{path}' has no audio")]
    NoAudio { path: PathBuf },
    #[error("a band needs 0 <= low < high and low below {nyquist:.0} Hz, got {low} to {high} Hz")]
    InvalidBand { low: f64, high: f64, nyquist: f64 },
    #[error("smoothing must be in [0, 1), got {0}")]
    InvalidSmoothing(f64),
    #[error("a tempo range needs 0 < min_bpm < max_bpm <= 400, got {min} to {max} BPM")]
    InvalidTempoRange { min: f64, max: f64 },
}

/// A steady tempo estimated from the onsets of a file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoEstimate {
    /// Beats per minute.
    pub bpm: f64,
    /// Source second of the first beat: every beat falls at
    /// `offset + k * 60 / bpm`.
    pub offset: f64,
    /// How clearly the onsets repeat at that period, 0 to 1: above about
    /// 0.3 a steady beat; near 0 no regular pulse.
    pub confidence: f64,
}

/// The analysis of one audio file, in source seconds.
#[derive(Debug, Clone)]
pub struct AudioAnalysis {
    /// Analysis frames per source second.
    pub frame_rate: f64,
    /// Length of the decoded audio, in seconds.
    pub duration: f64,
    rms: Vec<f32>,
    /// `frames × BINS` band powers.
    spectrum: Vec<f32>,
    /// `BINS + 1` bin edges, in Hz.
    edges: Vec<f64>,
    onsets: Vec<f64>,
}

/// Values sampled at `frame_rate` frames per source second from second 0.
#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    pub frame_rate: f64,
    pub values: Arc<[f32]>,
}

impl Series {
    /// The value at source second `time`, interpolated between frames; 0
    /// outside the series.
    pub fn at(&self, time: f64) -> f64 {
        if !time.is_finite() || time < 0.0 || self.values.is_empty() {
            return 0.0;
        }
        let position = time * self.frame_rate;
        let index = position.floor() as usize;
        if index + 1 >= self.values.len() {
            return if index < self.values.len() {
                f64::from(self.values[index])
            } else {
                0.0
            };
        }
        let fraction = position - index as f64;
        let (a, b) = (
            f64::from(self.values[index]),
            f64::from(self.values[index + 1]),
        );
        a + (b - a) * fraction
    }

    /// Smoothed with a one-pole filter: each frame keeps `smoothing` of the
    /// previous value, like Web Audio's `smoothingTimeConstant`.
    pub fn smoothed(&self, smoothing: f64) -> Result<Self, AnalysisError> {
        if !(0.0..1.0).contains(&smoothing) {
            return Err(AnalysisError::InvalidSmoothing(smoothing));
        }
        if smoothing == 0.0 {
            return Ok(self.clone());
        }
        let keep = smoothing as f32;
        let mut previous = 0.0f32;
        let values: Vec<f32> = self
            .values
            .iter()
            .map(|&value| {
                previous = keep * previous + (1.0 - keep) * value;
                previous
            })
            .collect();
        Ok(Self {
            frame_rate: self.frame_rate,
            values: values.into(),
        })
    }
}

impl AudioAnalysis {
    /// Analyze mono `samples` at `sample_rate`.
    pub fn from_samples(samples: &[f32], sample_rate: u32) -> Self {
        let sample_rate = sample_rate.max(1);
        let rate = f64::from(sample_rate);
        let frames = samples.len().div_ceil(HOP).max(1);
        let edges = bin_edges(rate / 2.0);
        // Each FFT bin's log bin, and for log bins narrower than the FFT
        // resolution, the FFT bin nearest their center.
        let resolution = rate / WINDOW as f64;
        let fft_bins = WINDOW / 2 + 1;
        let mut owner = vec![usize::MAX; fft_bins];
        for (k, owner) in owner.iter_mut().enumerate().skip(1) {
            let frequency = k as f64 * resolution;
            if let Some(bin) = edges
                .windows(2)
                .position(|edge| frequency >= edge[0] && frequency < edge[1])
            {
                *owner = bin;
            }
        }
        let mut covered = [false; BINS];
        for &bin in owner.iter().filter(|&&bin| bin != usize::MAX) {
            covered[bin] = true;
        }
        let nearest: Vec<(usize, f32)> = (0..BINS)
            .map(|bin| {
                let center = (edges[bin] * edges[bin + 1]).sqrt();
                let k = ((center / resolution).round() as usize).clamp(1, fft_bins - 1);
                let share = ((edges[bin + 1] - edges[bin]) / resolution) as f32;
                (k, share)
            })
            .collect();

        let window: Vec<f32> = (0..WINDOW)
            .map(|n| {
                let phase = std::f32::consts::TAU * n as f32 / WINDOW as f32;
                0.5 - 0.5 * phase.cos()
            })
            .collect();
        let mut rms = Vec::with_capacity(frames);
        let mut spectrum = vec![0.0f32; frames * BINS];
        let mut buffer = vec![(0.0f32, 0.0f32); WINDOW];
        let mut power = vec![0.0f32; fft_bins];
        for frame in 0..frames {
            // Frames are centered on `frame * HOP`.
            let center = frame * HOP;
            let start = center as isize - (WINDOW / 2) as isize;
            let sample = |n: isize| -> f32 {
                if n < 0 {
                    0.0
                } else {
                    samples.get(n as usize).copied().unwrap_or(0.0)
                }
            };
            let half = (HOP / 2) as isize;
            let energy: f32 = (center as isize - half..center as isize + half)
                .map(|n| sample(n).powi(2))
                .sum();
            rms.push((energy / HOP as f32).sqrt());

            for (n, slot) in buffer.iter_mut().enumerate() {
                *slot = (sample(start + n as isize) * window[n], 0.0);
            }
            fft(&mut buffer);
            for (k, value) in power.iter_mut().enumerate() {
                let (re, im) = buffer[k];
                *value = (re * re + im * im) / WINDOW as f32;
            }
            let row = &mut spectrum[frame * BINS..(frame + 1) * BINS];
            for (k, &bin) in owner.iter().enumerate() {
                if bin != usize::MAX {
                    row[bin] += power[k];
                }
            }
            for bin in 0..BINS {
                if !covered[bin] {
                    let (k, share) = nearest[bin];
                    row[bin] = power[k] * share;
                }
            }
        }
        let frame_rate = rate / HOP as f64;
        let onsets = pick_onsets(
            &spectral_flux(&spectrum, frames),
            frame_rate,
            ONSET_LATENCY,
            0.1,
        );
        Self {
            frame_rate,
            duration: samples.len() as f64 / rate,
            rms,
            spectrum,
            edges,
            onsets,
        }
    }

    pub fn frames(&self) -> usize {
        self.rms.len()
    }

    /// The upper end of the spectrum, in Hz.
    pub fn nyquist(&self) -> f64 {
        *self.edges.last().unwrap_or(&0.0)
    }

    /// Loudness envelope (RMS), 1 at the loud end of the file.
    pub fn level(&self) -> Series {
        self.normalized(self.rms.clone())
    }

    /// Amplitude of the frequencies between `low` and `high` Hz, 1 at the
    /// loud end of the file.
    pub fn band(&self, low: f64, high: f64) -> Result<Series, AnalysisError> {
        Ok(self.normalized(self.band_amplitudes(low, high)?))
    }

    /// How much of each bin lies between `low` and `high` Hz.
    fn band_weights(&self, low: f64, high: f64) -> Result<Vec<(usize, f32)>, AnalysisError> {
        let nyquist = self.nyquist();
        if !(low.is_finite() && high.is_finite()) || low < 0.0 || high <= low || low >= nyquist {
            return Err(AnalysisError::InvalidBand { low, high, nyquist });
        }
        let high = high.min(nyquist);
        Ok((0..BINS)
            .filter_map(|bin| {
                let (a, b) = (self.edges[bin], self.edges[bin + 1]);
                let overlap = (high.min(b) - low.max(a)).max(0.0);
                (overlap > 0.0).then(|| (bin, (overlap / (b - a)) as f32))
            })
            .collect())
    }

    /// Amplitude of the frequencies between `low` and `high` Hz per frame,
    /// before normalization.
    pub fn band_amplitudes(&self, low: f64, high: f64) -> Result<Vec<f32>, AnalysisError> {
        let weights = self.band_weights(low, high)?;
        Ok((0..self.frames())
            .map(|frame| {
                let row = &self.spectrum[frame * BINS..(frame + 1) * BINS];
                weights
                    .iter()
                    .map(|&(bin, weight)| row[bin] * weight)
                    .sum::<f32>()
                    .sqrt()
            })
            .collect())
    }

    /// Source seconds where a sound starts between `low` and `high` Hz, so a
    /// kick drum (40-120 Hz) does not pick up the hi-hats.
    pub fn band_onsets(&self, low: f64, high: f64) -> Result<Vec<f64>, AnalysisError> {
        // A band's amplitude rises in the frame centered on the hit, without
        // the log flux's half-window lead. A hit lifts it at once; the
        // bounces of a sweeping kick or a beating bass rise far less.
        Ok(pick_onsets(
            &self.band_rise(low, high)?,
            self.frame_rate,
            0.0,
            0.3,
        ))
    }

    /// How much the band's amplitude rises into every frame, 1 at the
    /// largest rise. Linear, unlike the log spectral flux: a hi-hat's faint
    /// bass leaking into a quiet kick band must not count as a hit.
    fn band_rise(&self, low: f64, high: f64) -> Result<Vec<f32>, AnalysisError> {
        let amplitudes = self.band_amplitudes(low, high)?;
        let mut rise: Vec<f32> = std::iter::once(0.0)
            .chain(
                amplitudes
                    .windows(2)
                    .map(|pair| (pair[1] - pair[0]).max(0.0)),
            )
            .collect();
        rise.truncate(amplitudes.len());
        let peak = rise.iter().copied().fold(0.0f32, f32::max);
        if peak > 0.0 {
            for value in &mut rise {
                *value /= peak;
            }
        }
        Ok(rise)
    }

    /// Loudness envelope (RMS) per frame, before normalization.
    pub fn rms(&self) -> &[f32] {
        &self.rms
    }

    /// The power of every bin in frame `frame`, lowest first: [`BINS`]
    /// values, empty past the end.
    pub fn spectrum_row(&self, frame: usize) -> &[f32] {
        self.spectrum
            .get(frame * BINS..(frame + 1) * BINS)
            .unwrap_or(&[])
    }

    /// The `BINS + 1` bin edges, in Hz.
    pub fn bin_edges(&self) -> &[f64] {
        &self.edges
    }

    /// The steady tempo between `min_bpm` and `max_bpm` that best explains
    /// the onsets: the autocorrelation peak of the spectral flux, refined
    /// between frames, and the phase whose beats land on the most flux.
    /// Meant for music with a steady pulse; a rubato or a tempo change
    /// gives an average and a low confidence.
    pub fn tempo(&self, min_bpm: f64, max_bpm: f64) -> Result<TempoEstimate, AnalysisError> {
        if !(min_bpm.is_finite() && max_bpm.is_finite())
            || min_bpm <= 0.0
            || max_bpm <= min_bpm
            || max_bpm > 400.0
        {
            return Err(AnalysisError::InvalidTempoRange {
                min: min_bpm,
                max: max_bpm,
            });
        }
        let flux = spectral_flux(&self.spectrum, self.frames());
        // The beat is where the kick and the bass hit: the off-beat hi-hats
        // carry as much broadband flux and would shift the phase half a beat.
        let low = self
            .band_rise(BEAT_LOW_HZ.0, BEAT_LOW_HZ.1)
            .ok()
            .filter(|low| low.iter().any(|&value| value > 0.0));
        // The period weighs the low hits too, or a hi-hat as loud as the
        // kick would make the half beat the period.
        let periodic: Vec<f32> = match &low {
            Some(low) => low.iter().zip(&flux).map(|(l, f)| l + 0.5 * f).collect(),
            None => flux.clone(),
        };
        // The log flux leads a hit by half a window; a band's rise does not.
        let (phase_flux, latency) = match &low {
            Some(low) => (low.as_slice(), 0.0),
            None => (flux.as_slice(), ONSET_LATENCY),
        };
        Ok(estimate_tempo(
            &periodic,
            phase_flux,
            latency,
            self.frame_rate,
            min_bpm,
            max_bpm,
        ))
    }

    /// Amplitudes of `bars` log-spaced bands between `low` and `high` Hz at
    /// source second `time`, each 1 at its loud end of the file.
    pub fn spectrum_at(&self, time: f64, bars: usize, low: f64, high: f64) -> Vec<f64> {
        let high = high.min(self.nyquist());
        if bars == 0 || low <= 0.0 || high <= low {
            return Vec::new();
        }
        let ratio = (high / low).powf(1.0 / bars as f64);
        (0..bars)
            .map(|bar| {
                let a = low * ratio.powi(bar as i32);
                self.band(a, a * ratio)
                    .map_or(0.0, |series| series.at(time))
            })
            .collect()
    }

    /// Source seconds where a sound starts, in order.
    pub fn onsets(&self) -> &[f64] {
        &self.onsets
    }

    /// The 99th percentile of the positive `values`, the loud end a few
    /// peaks do not move; 1 without any.
    pub fn loud_end(&self, values: impl Iterator<Item = f32>) -> f32 {
        let mut sorted: Vec<f32> = values.filter(|v| *v > 0.0).collect();
        if sorted.is_empty() {
            return 1.0;
        }
        sorted.sort_by(f32::total_cmp);
        sorted[((sorted.len() - 1) as f64 * 0.99).round() as usize]
    }

    /// `values` divided by their 99th percentile and clamped to `[0, 1]`, so
    /// a few peaks do not flatten the rest.
    fn normalized(&self, mut values: Vec<f32>) -> Series {
        let reference = self.loud_end(values.iter().copied());
        if reference > 0.0 {
            for value in &mut values {
                *value = (*value / reference).clamp(0.0, 1.0);
            }
        }
        Series {
            frame_rate: self.frame_rate,
            values: values.into(),
        }
    }
}

/// `BINS + 1` log-spaced edges from [`MIN_HZ`] to `nyquist`.
fn bin_edges(nyquist: f64) -> Vec<f64> {
    let ratio = (nyquist / MIN_HZ).max(1.0 + 1e-9).powf(1.0 / BINS as f64);
    (0..=BINS)
        .map(|index| MIN_HZ * ratio.powi(index as i32))
        .collect()
}

/// The spectral flux of every frame: the rise of the log spectrum from the
/// previous frame summed over every bin, normalized to its peak.
fn spectral_flux(spectrum: &[f32], frames: usize) -> Vec<f32> {
    let mut flux = vec![0.0f32; frames];
    if frames < 2 {
        return flux;
    }
    let compress = |power: f32| (1.0 + 1000.0 * power.sqrt()).ln();
    for frame in 1..frames {
        let row = &spectrum[frame * BINS..(frame + 1) * BINS];
        let previous = &spectrum[(frame - 1) * BINS..frame * BINS];
        flux[frame] = row
            .iter()
            .zip(previous)
            .map(|(&now, &before)| (compress(now) - compress(before)).max(0.0))
            .sum();
    }
    let peak = flux.iter().copied().fold(0.0f32, f32::max);
    if peak > 0.0 {
        for value in &mut flux {
            *value /= peak;
        }
    }
    flux
}

/// Onsets from a normalized spectral flux: peaks above a local mean.
/// A peak counts from `floor` of the largest one.
fn pick_onsets(flux: &[f32], frame_rate: f64, latency: f64, floor: f32) -> Vec<f64> {
    let frames = flux.len();
    if frames < 3 || flux.iter().all(|&value| value <= 0.0) {
        return Vec::new();
    }
    let near = ((0.03 * frame_rate).round() as usize).max(1);
    let around = ((0.15 * frame_rate).round() as usize).max(1);
    let mut onsets = Vec::new();
    let mut last = f64::NEG_INFINITY;
    for frame in 1..frames {
        let value = flux[frame];
        let lo = frame.saturating_sub(near);
        let hi = (frame + near).min(frames - 1);
        if flux[lo..=hi].iter().any(|&other| other > value) {
            continue;
        }
        let (lo, hi) = (
            frame.saturating_sub(around),
            (frame + around).min(frames - 1),
        );
        let mean = flux[lo..=hi].iter().sum::<f32>() / (hi - lo + 1) as f32;
        if value < floor || value < mean + 0.05 {
            continue;
        }
        let time = frame as f64 / frame_rate + latency;
        if time - last >= ONSET_GAP {
            onsets.push(time);
            last = time;
        }
    }
    onsets
}

/// The tempo whose period `flux` repeats at best, between `min_bpm` and
/// `max_bpm`, and the phase whose beats land on the most `phase_flux`,
/// which leads the sound by `latency` seconds.
fn estimate_tempo(
    flux: &[f32],
    phase_flux: &[f32],
    latency: f64,
    frame_rate: f64,
    min_bpm: f64,
    max_bpm: f64,
) -> TempoEstimate {
    let silent = TempoEstimate {
        bpm: (min_bpm * max_bpm).sqrt(),
        offset: 0.0,
        confidence: 0.0,
    };
    // Mean-free, so a loud passage does not favor every lag.
    let mean = flux.iter().map(|&v| f64::from(v)).sum::<f64>() / flux.len().max(1) as f64;
    let signal: Vec<f64> = flux.iter().map(|&v| f64::from(v) - mean).collect();
    let energy: f64 = signal.iter().map(|v| v * v).sum();
    let longest = 60.0 * frame_rate / min_bpm;
    if energy <= 0.0 || 2.0 * longest + 2.0 >= signal.len() as f64 {
        return silent;
    }
    // A beat rarely lands on a frame: a slightly blurred flux keeps the
    // correlation at a fractional lag close to its peak.
    let blurred: Vec<f64> = (0..signal.len())
        .map(|index| {
            let at = |offset: isize| {
                signal
                    .get(index.wrapping_add_signed(offset))
                    .copied()
                    .unwrap_or(0.0)
            };
            0.25 * at(-1) + 0.5 * at(0) + 0.25 * at(1)
        })
        .collect();
    let correlation = |lag: usize| -> f64 {
        if lag >= blurred.len() {
            return 0.0;
        }
        blurred[lag..]
            .iter()
            .zip(&blurred)
            .map(|(a, b)| a * b)
            .sum::<f64>()
            / (blurred.len() - lag) as f64
    };
    let interpolated = |lag: f64| -> f64 {
        let below = lag.floor() as usize;
        let fraction = lag - below as f64;
        correlation(below) * (1.0 - fraction) + correlation(below + 1) * fraction
    };
    // A beat also repeats at two and three periods; crediting them, and a
    // gentle preference for tempos near 120 BPM, settles whether the felt
    // beat is a period or its double.
    let score = |bpm: f64| -> f64 {
        let lag = 60.0 * frame_rate / bpm;
        let prior = (-0.5 * ((bpm / 120.0).log2() / 1.5).powi(2)).exp();
        prior * (interpolated(lag) + 0.5 * interpolated(2.0 * lag) + 0.33 * interpolated(3.0 * lag))
    };
    let steps = ((max_bpm - min_bpm) / 0.25).ceil() as usize;
    let coarse_bpm = (0..=steps)
        .map(|step| (min_bpm + step as f64 * 0.25).min(max_bpm))
        .max_by(|&a, &b| score(a).total_cmp(&score(b)))
        .unwrap_or(120.0);
    let coarse = 60.0 * frame_rate / coarse_bpm;
    // An error of a tenth of a frame drifts a beat off within a minute:
    // the period that also fits its later multiples is much sharper.
    let multiples = ((signal.len() as f64 / coarse) as usize / 2).clamp(1, 16);
    let period = (-25..=25)
        .map(|step| coarse + f64::from(step) * 0.02)
        .filter(|&period| period >= 1.0)
        .max_by(|&a, &b| {
            let fit = |period: f64| -> f64 {
                (1..=multiples)
                    .map(|k| interpolated(period * k as f64))
                    .sum()
            };
            fit(a).total_cmp(&fit(b))
        })
        .unwrap_or(coarse);
    let center = interpolated(period);
    let bpm = (60.0 * frame_rate / period).clamp(min_bpm, max_bpm);
    // The phase whose beats land on the most flux.
    let flux = phase_flux;
    let steps = period.ceil() as usize * 4;
    let phase = (0..steps)
        .map(|step| step as f64 * period / steps as f64)
        .max_by(|&a, &b| {
            let sum = |phase: f64| -> f64 {
                let mut total = 0.0;
                let mut position = phase;
                while (position as usize) < flux.len() {
                    total += f64::from(flux[position.round() as usize % flux.len()]);
                    position += period;
                }
                total
            };
            sum(a).total_cmp(&sum(b))
        })
        .unwrap_or(0.0);
    let zero_lag = correlation(0);
    TempoEstimate {
        bpm,
        offset: phase / frame_rate + latency,
        confidence: (center / zero_lag).clamp(0.0, 1.0),
    }
}

/// In-place iterative radix-2 FFT; `data.len()` must be a power of two.
fn fft(data: &mut [(f32, f32)]) {
    let n = data.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            data.swap(i, j);
        }
    }
    let mut length = 2;
    while length <= n {
        let angle = -std::f64::consts::TAU / length as f64;
        for start in (0..n).step_by(length) {
            for k in 0..length / 2 {
                let (sin, cos) = (angle * k as f64).sin_cos();
                let (wr, wi) = (cos as f32, sin as f32);
                let (ar, ai) = data[start + k];
                let (br, bi) = data[start + k + length / 2];
                let (tr, ti) = (br * wr - bi * wi, br * wi + bi * wr);
                data[start + k] = (ar + tr, ai + ti);
                data[start + k + length / 2] = (ar - tr, ai - ti);
            }
        }
        length <<= 1;
    }
}

/// Decode the first audio stream of `path` to mono samples at
/// [`SAMPLE_RATE`]: WAV natively, any other format through FFmpeg.
pub fn decode_mono(path: &Path) -> Result<Vec<f32>, AnalysisError> {
    if let Some(samples) = decode_wav(path) {
        return Ok(samples);
    }
    decode_with_ffmpeg(path)
}

/// A PCM WAV file mixed to mono and resampled to [`SAMPLE_RATE`]; `None`
/// when hound cannot read it, so FFmpeg tries.
fn decode_wav(path: &Path) -> Option<Vec<f32>> {
    let is_wav = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("wav"));
    if !is_wav {
        return None;
    }
    let mut reader = hound::WavReader::open(path).ok()?;
    let spec = reader.spec();
    let channels = usize::from(spec.channels.max(1));
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>().ok()?,
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample.clamp(1, 32) - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|value| value as f32 * scale))
                .collect::<Result<_, _>>()
                .ok()?
        }
    };
    let mono: Vec<f32> = interleaved
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect();
    Some(resample(&mono, spec.sample_rate, SAMPLE_RATE))
}

/// Linear resampling from `from` to `to` samples per second.
fn resample(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || from == 0 || samples.is_empty() {
        return samples.to_vec();
    }
    let step = f64::from(from) / f64::from(to);
    let count = ((samples.len() as f64) / step).floor() as usize;
    (0..count)
        .map(|index| {
            let position = index as f64 * step;
            let base = position.floor() as usize;
            let fraction = (position - base as f64) as f32;
            let a = samples[base];
            let b = samples.get(base + 1).copied().unwrap_or(a);
            a + (b - a) * fraction
        })
        .collect()
}

fn decode_with_ffmpeg(path: &Path) -> Result<Vec<f32>, AnalysisError> {
    let output = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(path)
        .args(["-map", "0:a:0", "-ac", "1", "-ar"])
        .arg(SAMPLE_RATE.to_string())
        .args(["-f", "f32le", "pipe:1"])
        .output()
        .map_err(AnalysisError::Unavailable)?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if message.contains("matches no streams") {
            return Err(AnalysisError::NoAudio {
                path: path.to_path_buf(),
            });
        }
        return Err(AnalysisError::DecodeFailed {
            path: path.to_path_buf(),
            message,
        });
    }
    Ok(output
        .stdout
        .chunks_exact(4)
        .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .collect())
}

type AnalysisKey = (PathBuf, u64, Option<SystemTime>);

/// The analysis of `path`, computed once per file size and modification.
pub fn analyze_file(path: &Path) -> Result<Arc<AudioAnalysis>, AnalysisError> {
    static CACHE: OnceLock<Mutex<HashMap<AnalysisKey, Arc<AudioAnalysis>>>> = OnceLock::new();
    let metadata = std::fs::metadata(path).map_err(|error| AnalysisError::DecodeFailed {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    let key = (path.to_path_buf(), metadata.len(), metadata.modified().ok());
    let cache = CACHE.get_or_init(Default::default);
    if let Some(analysis) = cache
        .lock()
        .expect("audio analysis cache poisoned")
        .get(&key)
    {
        return Ok(Arc::clone(analysis));
    }
    let samples = decode_mono(path)?;
    let analysis = Arc::new(AudioAnalysis::from_samples(&samples, SAMPLE_RATE));
    cache
        .lock()
        .expect("audio analysis cache poisoned")
        .insert(key, Arc::clone(&analysis));
    Ok(analysis)
}

/// A fingerprint of `path`'s contents for hot-reload recipes: its size and
/// modification time.
pub fn file_fingerprint(path: &Path) -> String {
    match std::fs::metadata(path) {
        Ok(metadata) => {
            let modified = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map_or(0, |time| time.as_nanos());
            format!("{}:{}:{modified}", path.display(), metadata.len())
        }
        Err(_) => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = SAMPLE_RATE;

    fn tone(frequency: f64, seconds: f64, amplitude: f32) -> Vec<f32> {
        let count = (seconds * f64::from(RATE)) as usize;
        (0..count)
            .map(|n| {
                amplitude
                    * (std::f64::consts::TAU * frequency * n as f64 / f64::from(RATE)).sin() as f32
            })
            .collect()
    }

    #[test]
    fn the_fft_finds_a_pure_tone() {
        let mut data: Vec<(f32, f32)> = (0..64)
            .map(|n| ((std::f32::consts::TAU * 4.0 * n as f32 / 64.0).cos(), 0.0))
            .collect();
        fft(&mut data);
        let magnitude = |k: usize| (data[k].0.powi(2) + data[k].1.powi(2)).sqrt();
        assert!((magnitude(4) - 32.0).abs() < 1e-3);
        assert!(magnitude(5) < 1e-3 && magnitude(0) < 1e-3);
    }

    #[test]
    fn bands_hear_only_their_frequencies() {
        // One second of 60 Hz, then one second of 3 kHz.
        let mut samples = tone(60.0, 1.0, 0.8);
        samples.extend(tone(3000.0, 1.0, 0.8));
        let analysis = AudioAnalysis::from_samples(&samples, RATE);
        let bass = analysis.band(20.0, 150.0).unwrap();
        let treble = analysis.band(2000.0, 5000.0).unwrap();
        assert!(
            bass.at(0.5) > 0.9 && bass.at(1.5) < 0.05,
            "{} {}",
            bass.at(0.5),
            bass.at(1.5)
        );
        assert!(treble.at(1.5) > 0.9 && treble.at(0.5) < 0.05);
    }

    #[test]
    fn the_level_follows_loudness_and_silence_is_zero() {
        let mut samples = tone(440.0, 1.0, 0.1);
        samples.extend(tone(440.0, 1.0, 0.8));
        samples.extend(vec![0.0; RATE as usize]);
        let level = AudioAnalysis::from_samples(&samples, RATE).level();
        assert!(level.at(1.5) > 0.95);
        assert!((level.at(0.5) - 0.125).abs() < 0.03, "{}", level.at(0.5));
        assert!(level.at(2.5) < 1e-3);
        assert_eq!(level.at(-1.0), 0.0);
        assert_eq!(level.at(10.0), 0.0);
    }

    #[test]
    fn onsets_land_on_the_hits() {
        // Four short noise bursts on a quiet bed.
        let mut samples = vec![0.0f32; 2 * RATE as usize];
        let mut state = 1u32;
        for hit in [0.25, 0.75, 1.1, 1.6] {
            let start = (hit * f64::from(RATE)) as usize;
            for n in 0..800 {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = (state >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
                samples[start + n] = noise * (1.0 - n as f32 / 800.0);
            }
        }
        let onsets = AudioAnalysis::from_samples(&samples, RATE)
            .onsets()
            .to_vec();
        assert_eq!(onsets.len(), 4, "{onsets:?}");
        for (onset, hit) in onsets.iter().zip([0.25, 0.75, 1.1, 1.6]) {
            assert!((onset - hit).abs() < 0.02, "{onset} vs {hit}");
        }
    }

    /// A beat at `bpm` from `first` seconds: a 60 Hz kick on every beat and
    /// a short high noise burst (a hi-hat) halfway between.
    fn kick_and_hat(bpm: f64, first: f64, seconds: f64) -> Vec<f32> {
        let mut samples = vec![0.0f32; (seconds * f64::from(RATE)) as usize];
        let beat = 60.0 / bpm;
        let mut state = 7u32;
        let mut time = first;
        while time < seconds - beat {
            let kick = (time * f64::from(RATE)) as usize;
            for n in 0..2400 {
                let envelope = (-(n as f32) / 600.0).exp();
                let phase = std::f64::consts::TAU * 60.0 * n as f64 / f64::from(RATE);
                samples[kick + n] += 0.9 * envelope * phase.sin() as f32;
            }
            // Twice-differenced noise: almost nothing below a few kHz.
            let hat = ((time + beat / 2.0) * f64::from(RATE)) as usize;
            let (mut before, mut last) = (0.0f32, 0.0f32);
            for n in 0..500 {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = (state >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
                let high = noise - 2.0 * last + before;
                (before, last) = (last, noise);
                samples[hat + n] += 0.15 * high * (1.0 - n as f32 / 500.0);
            }
            time += beat;
        }
        samples
    }

    #[test]
    fn band_onsets_hear_only_the_kick() {
        let samples = kick_and_hat(120.0, 0.5, 6.0);
        let analysis = AudioAnalysis::from_samples(&samples, RATE);
        let all = analysis.onsets().len();
        let kicks = analysis.band_onsets(30.0, 120.0).unwrap();
        assert!(all > kicks.len() + 5, "{all} onsets, {} kicks", kicks.len());
        assert!((10..=11).contains(&kicks.len()), "{kicks:?}");
        for (index, kick) in kicks.iter().enumerate() {
            let expected = 0.5 + 0.5 * index as f64;
            assert!(
                (kick - expected).abs() < 0.03,
                "{kick} vs {expected} in {kicks:?}"
            );
        }
        assert!(analysis.band_onsets(200.0, 100.0).is_err());
    }

    #[test]
    fn tempo_finds_the_beat_and_its_phase() {
        for (bpm, first) in [(124.0, 0.37), (90.0, 0.2), (150.0, 0.1)] {
            let samples = kick_and_hat(bpm, first, 12.0);
            let analysis = AudioAnalysis::from_samples(&samples, RATE);
            let tempo = analysis.tempo(60.0, 200.0).unwrap();
            assert!((tempo.bpm - bpm).abs() < 1.0, "{bpm}: {tempo:?}");
            // The first beat, up to one beat earlier or later.
            let beat = 60.0 / bpm;
            let phase = (tempo.offset - first).rem_euclid(beat);
            let error = phase.min(beat - phase);
            assert!(error < 0.03, "{bpm}: {tempo:?}");
            assert!(tempo.confidence > 0.3, "{bpm}: {tempo:?}");
        }
        let silence = AudioAnalysis::from_samples(&vec![0.0; RATE as usize * 4], RATE);
        assert_eq!(silence.tempo(60.0, 200.0).unwrap().confidence, 0.0);
        let analysis = AudioAnalysis::from_samples(&tone(100.0, 1.0, 0.5), RATE);
        assert!(analysis.tempo(0.0, 200.0).is_err());
        assert!(analysis.tempo(120.0, 100.0).is_err());
        assert!(analysis.tempo(60.0, 1000.0).is_err());
    }

    #[test]
    fn smoothing_and_invalid_bands() {
        let analysis = AudioAnalysis::from_samples(&tone(100.0, 1.0, 0.5), RATE);
        let level = analysis.level();
        let smooth = level.smoothed(0.9).unwrap();
        assert!(smooth.at(0.05) < level.at(0.05));
        assert!(level.smoothed(1.0).is_err() && level.smoothed(-0.1).is_err());
        assert!(analysis.band(200.0, 100.0).is_err());
        assert!(analysis.band(20_000.0, 30_000.0).is_err());
        assert!(analysis.band(f64::NAN, 100.0).is_err());
        assert_eq!(analysis.spectrum_at(0.5, 8, 30.0, 8000.0).len(), 8);
    }

    #[test]
    fn wav_files_decode_without_ffmpeg() {
        let directory =
            std::env::temp_dir().join(format!("gaanim-analysis-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("stereo.wav");
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for n in 0..44_100 {
            let value =
                ((std::f64::consts::TAU * 440.0 * n as f64 / 44_100.0).sin() * 16_000.0) as i16;
            writer.write_sample(value).unwrap();
            writer.write_sample(value).unwrap();
        }
        writer.finalize().unwrap();
        let samples = decode_mono(&path).unwrap();
        assert!((samples.len() as i64 - i64::from(RATE)).abs() <= 1);
        let peak = samples.iter().copied().fold(0.0f32, f32::max);
        assert!((peak - 16_000.0 / 32_768.0).abs() < 0.01, "{peak}");
        let analysis = analyze_file(&path).unwrap();
        assert!((analysis.duration - 1.0).abs() < 1e-3);
        assert!(Arc::ptr_eq(&analysis, &analyze_file(&path).unwrap()));
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn an_empty_file_analyzes_to_silence() {
        let analysis = AudioAnalysis::from_samples(&[], RATE);
        assert_eq!(analysis.level().at(0.0), 0.0);
        assert!(analysis.onsets().is_empty());
    }
}
