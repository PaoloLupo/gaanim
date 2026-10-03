//! Signals read from an audio clip's analysis, for audio-reactive animation.
//!
//! Each signal is a native function of the scene time: it finds where the
//! clip is playing (its start, offset, speed and loop) and looks the
//! analysis up there, so a seek or an export reads what playback reads. A
//! clip that has not played yet, or has stopped, reads 0.

use std::sync::{Arc, Mutex};

use gaanim_animation::{ReactiveFunction, ReactiveInput, ScalarSource};
use gaanim_media::AudioTrack;
use gaanim_media::analysis::{
    AnalysisError, AudioAnalysis, Series, analyze_file, file_fingerprint,
};

use super::AudioClip;

impl AudioClip {
    /// The analysis of the clip's file, computed once per file.
    pub fn analysis(&self) -> Result<Arc<AudioAnalysis>, AnalysisError> {
        analyze_file(&self.track.path)
    }

    /// Scene second of the clip's latest play, or `None` before it plays.
    pub fn start_time(&self) -> Option<f64> {
        self.plays
            .lock()
            .expect("audio plays poisoned")
            .last()
            .map(|track| track.start_time)
    }

    /// Scene seconds the clip plays for: its duration, or the rest of the
    /// file from its offset at its speed.
    pub fn length(&self) -> Result<f64, AnalysisError> {
        if let Some(duration) = self.track.duration {
            return Ok(duration);
        }
        let analysis = self.analysis()?;
        let speed = if self.track.speed > 0.0 {
            self.track.speed
        } else {
            1.0
        };
        Ok((analysis.duration - self.track.source_offset).max(0.0) / speed)
    }

    /// Loudness, 0 to 1, smoothed by `smoothing` in `[0, 1)`.
    pub fn level(&self, smoothing: f64) -> Result<ScalarSource, AnalysisError> {
        let series = self.analysis()?.level().smoothed(smoothing)?;
        Ok(self.series_source(series, format!("level:{smoothing}")))
    }

    /// Amplitude of the frequencies between `low` and `high` Hz, 0 to 1,
    /// smoothed by `smoothing` in `[0, 1)`.
    pub fn band(&self, low: f64, high: f64, smoothing: f64) -> Result<ScalarSource, AnalysisError> {
        let series = self.analysis()?.band(low, high)?.smoothed(smoothing)?;
        Ok(self.series_source(series, format!("band:{low}:{high}:{smoothing}")))
    }

    /// 1 at each onset, decaying to 0 with time constant `decay` seconds.
    pub fn pulse(&self, decay: f64) -> Result<ScalarSource, AnalysisError> {
        let analysis = self.analysis()?;
        let onsets: Arc<[f64]> = analysis.onsets().into();
        let duration = analysis.duration;
        let plays = Arc::clone(&self.plays);
        let decay = decay.max(1e-6);
        let sample = move |time: f64| {
            each_play(&plays, |track| {
                let source = track.source_time(time, duration)?;
                let index = onsets.partition_point(|&onset| onset <= source);
                let onset = *onsets.get(index.checked_sub(1)?)?;
                // An onset before the clip's offset, or in an earlier loop
                // cycle, does not pulse now.
                if onset < track.source_offset {
                    return None;
                }
                let elapsed = (source - onset) / track.speed.max(1e-9);
                Some((-elapsed / decay).exp())
            })
        };
        Ok(self.source(sample, format!("pulse:{decay}")))
    }

    /// `bands` amplitudes over log-spaced frequency bands from `low` to
    /// `high` Hz (the analysis's top frequency when `None`), lowest first.
    /// Each band is 0 to 1 against its own loud end, like [`Self::band`].
    pub fn spectrum(
        &self,
        bands: usize,
        low: f64,
        high: Option<f64>,
        smoothing: f64,
    ) -> Result<Vec<ScalarSource>, AnalysisError> {
        let nyquist = self.analysis()?.nyquist();
        let high = high.unwrap_or(nyquist);
        if !(low.is_finite() && low > 0.0 && high > low && high <= nyquist) || bands == 0 {
            return Err(AnalysisError::InvalidBand { low, high, nyquist });
        }
        let ratio = (high / low).powf(1.0 / bands as f64);
        (0..bands)
            .map(|index| {
                let from = low * ratio.powi(index as i32);
                let to = if index + 1 == bands {
                    high
                } else {
                    low * ratio.powi(index as i32 + 1)
                };
                self.band(from, to, smoothing)
            })
            .collect()
    }

    /// The loudness (or the band `low`-`high` Hz) over the last `span`
    /// seconds at `points` evenly spaced instants, oldest first: point `i`
    /// reads it `span * (1 - i / (points - 1))` seconds ago, so the values
    /// scroll from the last point to the first as the clip plays.
    pub fn waveform(
        &self,
        points: usize,
        span: f64,
        band: Option<(f64, f64)>,
        smoothing: f64,
    ) -> Result<Vec<ScalarSource>, AnalysisError> {
        let analysis = self.analysis()?;
        let (series, name) = match band {
            Some((low, high)) => (
                analysis.band(low, high)?.smoothed(smoothing)?,
                format!("band:{low}:{high}:{smoothing}"),
            ),
            None => (
                analysis.level().smoothed(smoothing)?,
                format!("level:{smoothing}"),
            ),
        };
        let last = points.saturating_sub(1).max(1) as f64;
        Ok((0..points)
            .map(|index| {
                let ago = if points > 1 {
                    span * (1.0 - index as f64 / last)
                } else {
                    0.0
                };
                self.delayed_series_source(series.clone(), ago, format!("{name}:ago:{ago}"))
            })
            .collect())
    }

    /// Seconds from the clip's start at which a sound starts (a drum hit, a
    /// note, a syllable), following the clip's offset, speed and length.
    pub fn onsets(&self) -> Result<Vec<f64>, AnalysisError> {
        let analysis = self.analysis()?;
        let mut track = self.track.clone();
        track.start_time = 0.0;
        Ok(track.scene_times(analysis.onsets(), analysis.duration, f64::MAX))
    }

    fn series_source(&self, series: Series, recipe: String) -> ScalarSource {
        self.delayed_series_source(series, 0.0, recipe)
    }

    /// `series` where the clip played `ago` seconds before the scene time.
    fn delayed_series_source(&self, series: Series, ago: f64, recipe: String) -> ScalarSource {
        let duration = series.values.len() as f64 / series.frame_rate;
        let plays = Arc::clone(&self.plays);
        let sample = move |time: f64| {
            each_play(&plays, |track| {
                track
                    .source_time(time - ago, duration)
                    .map(|source| series.at(source))
            })
        };
        self.source(sample, recipe)
    }

    fn source(
        &self,
        sample: impl Fn(f64) -> f64 + Send + Sync + 'static,
        recipe: String,
    ) -> ScalarSource {
        let fingerprint = file_fingerprint(&self.track.path);
        let plays = Arc::clone(&self.plays);
        let function = ReactiveFunction::new(0, 1, vec![ReactiveInput::Time], move |arguments| {
            Ok(vec![sample(arguments[0])])
        })
        // The plays place the analysis on the timeline, so they are part of
        // what the callback computes.
        .with_recipe_fn(move || {
            let plays = plays.lock().ok()?;
            let placed: Vec<String> = plays.iter().map(play_recipe).collect();
            Some(format!("audio:{fingerprint}:{recipe}:[{}]", placed.join(";")).into())
        });
        ScalarSource::function(function).expect("an audio signal is a scalar of time")
    }
}

/// The largest value of `read` over the clip's plays; 0 when none plays.
fn each_play(plays: &Mutex<Vec<AudioTrack>>, read: impl Fn(&AudioTrack) -> Option<f64>) -> f64 {
    plays
        .lock()
        .expect("audio plays poisoned")
        .iter()
        .filter_map(read)
        .fold(0.0, f64::max)
}

fn play_recipe(track: &AudioTrack) -> String {
    format!(
        "{}:{:?}:{}:{:?}:{}:{}",
        track.start_time,
        track.duration,
        track.source_offset,
        track.source_duration,
        track.speed,
        track.looping
    )
}

#[cfg(test)]
mod tests {
    use super::super::{PlayItem, SceneModel};
    use gaanim_animation::ScalarSource;

    /// A WAV of `seconds` of silence with a loud 80 Hz tone in `[on, off)`.
    fn write_wav(path: &std::path::Path, seconds: f64, on: f64, off: f64) {
        let rate = 22_050u32;
        let samples: Vec<i16> = (0..(seconds * f64::from(rate)) as usize)
            .map(|n| {
                let t = n as f64 / f64::from(rate);
                if (on..off).contains(&t) {
                    ((std::f64::consts::TAU * 80.0 * t).sin() * 20_000.0) as i16
                } else {
                    0
                }
            })
            .collect();
        let data_len = (samples.len() * 2) as u32;
        let mut bytes = Vec::with_capacity(44 + data_len as usize);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
        bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
        bytes.extend_from_slice(&rate.to_le_bytes());
        bytes.extend_from_slice(&(rate * 2).to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        std::fs::write(path, bytes).unwrap();
    }

    fn at(source: &ScalarSource, time: f64) -> f64 {
        source.evaluate(time, |_| None).unwrap()
    }

    #[test]
    fn signals_follow_the_clip_on_the_timeline() {
        let directory =
            std::env::temp_dir().join(format!("gaanim-audio-signals-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("hit.wav");
        write_wav(&path, 2.0, 0.5, 1.0);

        let mut scene = SceneModel::new(16.0, 9.0);
        let clip = scene.audio(&path, None, 1.0, 0.0, 0.0).unwrap();
        let bass = clip.band(20.0, 150.0, 0.0).unwrap();
        let treble = clip.band(2000.0, 8000.0, 0.0).unwrap();
        let pulse = clip.pulse(0.1).unwrap();
        let level = clip.level(0.0).unwrap();
        // Before the clip plays, every signal reads 0.
        assert_eq!(at(&bass, 1.7), 0.0);
        assert_eq!(clip.start_time(), None);

        scene.wait(1.0);
        scene
            .play_items(vec![PlayItem::Audio(clip.clone())])
            .unwrap();
        assert_eq!(clip.start_time(), Some(1.0));
        // The tone sounds from scene second 1.5 to 2.0.
        assert!(at(&bass, 1.75) > 0.9, "{}", at(&bass, 1.75));
        assert!(at(&level, 1.75) > 0.9);
        assert!(at(&bass, 1.2) < 0.01 && at(&bass, 2.4) < 0.01);
        assert!(at(&treble, 1.75) < 0.05);
        assert_eq!(at(&bass, 3.5), 0.0, "silent once the clip has ended");
        // The onset at the tone's start pulses at 1.5 and decays.
        let onsets = clip.onsets().unwrap();
        assert!(
            onsets.iter().any(|onset| (onset - 0.5).abs() < 0.04),
            "{onsets:?}"
        );
        assert!(at(&pulse, 1.52) > 0.4, "{}", at(&pulse, 1.52));
        assert!(at(&pulse, 1.9) < at(&pulse, 1.6));
        assert_eq!(at(&pulse, 1.3), 0.0);
        // Evaluating again at the same time reads the same value.
        assert_eq!(at(&bass, 1.75), at(&bass, 1.75));
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn spectrum_and_waveform_follow_the_clip() {
        let directory =
            std::env::temp_dir().join(format!("gaanim-audio-spectrum-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("tone.wav");
        // An 80 Hz tone from 0.5 s to 1.0 s of the file.
        write_wav(&path, 2.0, 0.5, 1.0);
        let mut scene = SceneModel::new(16.0, 9.0);
        let clip = scene.audio(&path, None, 1.0, 0.0, 0.0).unwrap();
        let spectrum = clip.spectrum(6, 40.0, None, 0.0).unwrap();
        assert_eq!(spectrum.len(), 6);
        let wave = clip.waveform(5, 1.0, None, 0.0).unwrap();
        assert!(clip.spectrum(4, 100.0, Some(50.0), 0.0).is_err());
        assert!(clip.spectrum(0, 40.0, None, 0.0).is_err());
        scene
            .play_items(vec![PlayItem::Audio(clip.clone())])
            .unwrap();
        // The lowest band holds the 80 Hz tone; the top ones stay quiet.
        assert!(at(&spectrum[0], 0.75) > 0.5, "{}", at(&spectrum[0], 0.75));
        assert!(at(&spectrum[5], 0.75) < 0.1);
        // At 1.25 s the newest point is past the tone and the one a half
        // second older is inside it.
        assert!(at(&wave[4], 1.25) < 0.05);
        assert!(at(&wave[2], 1.25) > 0.5, "{}", at(&wave[2], 1.25));
        assert_eq!(at(&wave[0], 0.2), 0.0, "before the clip played");
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn the_recipe_changes_with_the_play() {
        let directory =
            std::env::temp_dir().join(format!("gaanim-audio-recipe-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("tone.wav");
        write_wav(&path, 1.0, 0.0, 1.0);
        let mut scene = SceneModel::new(16.0, 9.0);
        let clip = scene.audio(&path, None, 1.0, 0.0, 0.0).unwrap();
        let ScalarSource::Function(function) = clip.level(0.5).unwrap() else {
            panic!("audio signals are functions of time");
        };
        let before = function.recipe();
        scene.play_items(vec![PlayItem::Audio(clip)]).unwrap();
        assert_ne!(before, function.recipe());
        std::fs::remove_dir_all(&directory).ok();
    }
}
