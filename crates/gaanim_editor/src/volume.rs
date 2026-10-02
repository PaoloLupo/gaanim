//! The preview's volume, remembered between sessions: in a file beside the
//! overlay preferences on the desktop. The web player's page keeps it in
//! the browser and hands it over as it starts.

use bevy::platform::time::Instant;
use bevy::prelude::*;
use gaanim_media::PreviewVolume;
use serde::{Deserialize, Serialize};

const FILE: &str = "volume.json";
/// A volume dragged on a slider is written once it rests this long.
const SETTLE_SECONDS: f32 = 0.5;

#[derive(Serialize, Deserialize)]
struct Saved {
    level: f32,
    muted: bool,
}

fn path() -> Option<std::path::PathBuf> {
    gaanim_project::user_data_dir().map(|dir| dir.join(FILE))
}

/// The saved volume, or full volume when none is saved or it cannot be read.
pub fn load() -> PreviewVolume {
    path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|source| serde_json::from_str::<Saved>(&source).ok())
        .map(|saved| PreviewVolume {
            level: saved.level.clamp(0.0, 1.0),
            muted: saved.muted,
        })
        .unwrap_or_default()
}

fn save(volume: PreviewVolume) -> Result<(), String> {
    let path = path().ok_or("no se encontró la carpeta de datos de Gaanim")?;
    let saved = Saved {
        level: volume.level,
        muted: volume.muted,
    };
    let source = serde_json::to_string_pretty(&saved).map_err(|error| error.to_string())?;
    gaanim_media::narration::write_atomically(&path, source.as_bytes())
        .map_err(|error| error.to_string())
}

/// Write the volume once it changed and settled.
pub(crate) fn save_volume_system(
    volume: Res<PreviewVolume>,
    mut pending: Local<Option<(PreviewVolume, Instant)>>,
    mut saved: Local<Option<PreviewVolume>>,
) {
    if crate::WEB {
        return;
    }
    let Some(previous) = *saved else {
        // The first run records what was loaded; nothing changed yet.
        *saved = Some(*volume);
        return;
    };
    if *volume != previous && pending.is_none_or(|(waiting, _)| waiting != *volume) {
        *pending = Some((*volume, Instant::now()));
    }
    if let Some((waiting, since)) = *pending
        && since.elapsed().as_secs_f32() >= SETTLE_SECONDS
    {
        *pending = None;
        *saved = Some(waiting);
        if let Err(error) = save(waiting) {
            gaanim_core::console::warn("audio", format!("no se guardó el volumen: {error}"));
        }
    }
}
