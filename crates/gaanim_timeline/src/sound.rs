//! Sound cues anchored to timeline events (transitions and animations).
//!
//! A [`SoundCue`] only names a file and its gain. It carries no absolute
//! time: the owner that schedules the event (a transition entering a
//! segment, an animation starting inside a play batch) resolves the cue into
//! an audio track at the event's resolved start, so the sound moves together
//! with the event it decorates.

use std::path::PathBuf;

/// A short sound played when a timeline event starts.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SoundCue {
    /// Audio file; relative paths are resolved by the scene that schedules it.
    pub path: PathBuf,
    /// Linear gain, finite and non-negative.
    pub volume: f64,
    /// Seconds from the event start to the sound start. It may be negative
    /// to anticipate the event, as long as the sound starts at or after 0 s.
    pub offset: f64,
}

/// Invalid [`SoundCue`] parameters.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SoundCueError {
    #[error("sound path must not be empty")]
    EmptyPath,
    #[error("sound volume must be a finite non-negative number")]
    InvalidVolume,
    #[error("sound offset must be finite")]
    InvalidOffset,
}

impl SoundCue {
    /// A cue at the event start with unit gain.
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, SoundCueError> {
        Self::with_options(path, 1.0, 0.0)
    }

    /// A cue with an explicit gain and start offset.
    pub fn with_options(
        path: impl Into<PathBuf>,
        volume: f64,
        offset: f64,
    ) -> Result<Self, SoundCueError> {
        let path = path.into();
        if path.as_os_str().is_empty() {
            return Err(SoundCueError::EmptyPath);
        }
        if !volume.is_finite() || volume < 0.0 {
            return Err(SoundCueError::InvalidVolume);
        }
        if !offset.is_finite() {
            return Err(SoundCueError::InvalidOffset);
        }
        Ok(Self {
            path,
            volume,
            offset,
        })
    }

    /// Absolute start of the sound for an event starting at `event_start`.
    pub fn start_for(&self, event_start: f64) -> f64 {
        event_start + self.offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cue_validates_path_volume_and_offset() {
        assert_eq!(SoundCue::new("").unwrap_err(), SoundCueError::EmptyPath);
        assert_eq!(
            SoundCue::with_options("a.wav", -0.1, 0.0).unwrap_err(),
            SoundCueError::InvalidVolume
        );
        assert_eq!(
            SoundCue::with_options("a.wav", f64::NAN, 0.0).unwrap_err(),
            SoundCueError::InvalidVolume
        );
        assert_eq!(
            SoundCue::with_options("a.wav", 1.0, f64::INFINITY).unwrap_err(),
            SoundCueError::InvalidOffset
        );
        let cue = SoundCue::new("a.wav").unwrap();
        assert_eq!(cue.volume, 1.0);
        assert_eq!(cue.offset, 0.0);
    }

    #[test]
    fn cue_start_follows_the_event_start() {
        let cue = SoundCue::with_options("whoosh.wav", 0.5, -0.1).unwrap();
        assert!((cue.start_for(2.0) - 1.9).abs() < 1e-12);
        // Shifting the event shifts the sound by the same amount.
        assert!((cue.start_for(3.5) - cue.start_for(2.0) - 1.5).abs() < 1e-12);
    }
}
