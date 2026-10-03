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
        let onsets = detect_onsets(&spectrum, frames, frame_rate);
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
        let nyquist = self.nyquist();
        if !(low.is_finite() && high.is_finite()) || low < 0.0 || high <= low || low >= nyquist {
            return Err(AnalysisError::InvalidBand { low, high, nyquist });
        }
        let high = high.min(nyquist);
        let weights: Vec<(usize, f32)> = (0..BINS)
            .filter_map(|bin| {
                let (a, b) = (self.edges[bin], self.edges[bin + 1]);
                let overlap = (high.min(b) - low.max(a)).max(0.0);
                (overlap > 0.0).then(|| (bin, (overlap / (b - a)) as f32))
            })
            .collect();
        let values = (0..self.frames())
            .map(|frame| {
                let row = &self.spectrum[frame * BINS..(frame + 1) * BINS];
                weights
                    .iter()
                    .map(|&(bin, weight)| row[bin] * weight)
                    .sum::<f32>()
                    .sqrt()
            })
            .collect();
        Ok(self.normalized(values))
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

    /// `values` divided by their 99th percentile and clamped to `[0, 1]`, so
    /// a few peaks do not flatten the rest.
    fn normalized(&self, mut values: Vec<f32>) -> Series {
        let mut sorted: Vec<f32> = values.iter().copied().filter(|v| *v > 0.0).collect();
        if !sorted.is_empty() {
            sorted.sort_by(f32::total_cmp);
            let reference = sorted[((sorted.len() - 1) as f64 * 0.99).round() as usize];
            if reference > 0.0 {
                for value in &mut values {
                    *value = (*value / reference).clamp(0.0, 1.0);
                }
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

/// Onsets by spectral flux: the rise of the log spectrum from one frame to
/// the next, peak-picked above a local mean.
fn detect_onsets(spectrum: &[f32], frames: usize, frame_rate: f64) -> Vec<f64> {
    if frames < 3 {
        return Vec::new();
    }
    let compress = |power: f32| (1.0 + 1000.0 * power.sqrt()).ln();
    let mut flux = vec![0.0f32; frames];
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
    if peak <= 0.0 {
        return Vec::new();
    }
    for value in &mut flux {
        *value /= peak;
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
        if value < 0.1 || value < mean + 0.05 {
            continue;
        }
        let time = frame as f64 / frame_rate + ONSET_LATENCY;
        if time - last >= ONSET_GAP {
            onsets.push(time);
            last = time;
        }
    }
    onsets
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
