//! Fingerprints that decide which segments an incremental replay may reuse.
//!
//! A segment compiles from its own ops, the frozen spawn specs of the objects
//! it declares, the state carried from earlier segments, and scene-wide
//! inputs. Two revisions whose scene-wide inputs and leading segments have
//! equal fingerprints therefore compile those segments identically.

use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::{DefaultHasher, Hasher};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use gaanim_core::fingerprint::DebugFingerprint;

use crate::canvas::canvas_impl::SceneModel;
use crate::canvas::ops::Op;
use crate::canvas::theme::CanvasTheme;

/// Content fingerprints of one scene revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SceneFingerprints {
    /// Scene-wide inputs; `None` when some input was not observable.
    global: Option<u64>,
    /// One entry per segment; `None` marks a segment that always recompiles.
    segments: Vec<Option<u64>>,
}

impl SceneFingerprints {
    /// Number of leading segments provably identical in both revisions.
    pub(crate) fn shared_prefix(&self, other: &Self) -> usize {
        if self.global.is_none() || self.global != other.global {
            return 0;
        }
        self.segments
            .iter()
            .zip(&other.segments)
            .take_while(|(mine, theirs)| mine.is_some() && mine == theirs)
            .count()
    }

    pub(crate) fn segment_count(&self) -> usize {
        self.segments.len()
    }
}

impl SceneModel {
    /// Fingerprint every input that the compilation of this scene reads,
    /// given the text configuration and fonts it will compile with.
    pub(crate) fn fingerprints(
        &self,
        text_config: &gaanim_text::prelude::TextConfig,
        font_registry: &gaanim_text::font::FontRegistry,
    ) -> SceneFingerprints {
        let mut global = self.scene_wide_fingerprint();
        add_text_config(&mut global, text_config);
        global.add(&gaanim_text::typst_compiler::registered_fonts_fingerprint(
            font_registry,
        ));

        let state = self.state.lock().expect("canvas state poisoned");
        let segments = state
            .segments
            .iter()
            .map(|segment| {
                let mut fingerprint = DebugFingerprint::new();
                fingerprint.add(segment);
                for op in &segment.ops {
                    if let Op::Spawn(spec) = op {
                        let id = spec.lock().expect("object spec poisoned").id;
                        fingerprint.add(&state.frozen_spawn_specs.get(&id));
                    }
                }
                fingerprint.finish()
            })
            .collect();
        SceneFingerprints {
            global: global.finish(),
            segments,
        }
    }

    /// Fingerprint the scene-wide inputs of the compilation: every field of
    /// the scene outside its authored state.
    pub(crate) fn scene_wide_fingerprint(&self) -> DebugFingerprint {
        // Exhaustive on purpose: a new scene-wide field must be fingerprinted.
        let SceneModel {
            frame,
            background,
            background_paint,
            background_overridden,
            post_process,
            motion_blur,
            theme,
            theme_style,
            font_family_override,
            math_font_family_override,
            code_font_family_override,
            margin,
            // Read only while authoring, to convert `px` lengths.
            design_resolution: _,
            asset_root,
            asset_search,
            audio_tracks,
            // Positions inside `audio_tracks`, which is fingerprinted.
            transition_sounds: _,
            tempo,
            // Editor metadata: its timing already lives in the segment waits
            // and `audio_tracks`, and it is never compiled.
            launched_channels: _,
            thumbnail_time: _,
            narration: _,
            branding,
            camera_position,
            lighting_3d,
            // Authored state, fingerprinted per segment by `fingerprints`.
            state: _,
            // Derived from the rest.
            measured: _,
        } = self;
        let mut global = DebugFingerprint::new();
        global.add(frame);
        global.add(background);
        global.add(background_paint);
        global.add(background_overridden);
        global.add(post_process);
        global.add(motion_blur);
        global.add(theme);
        match theme_style {
            Some(theme) => add_theme(&mut global, theme),
            None => global.add(&"no theme"),
        }
        global.add(font_family_override);
        global.add(math_font_family_override);
        global.add(code_font_family_override);
        global.add(margin);
        global.add(asset_root);
        global.add(asset_search);
        global.add(audio_tracks);
        global.add(tempo);
        global.add(branding);
        global.add(camera_position);
        global.add(lighting_3d);
        global
    }
}

/// Feed map entries in a stable order; `HashMap` iteration order differs
/// between otherwise equal maps.
fn add_sorted<K: Debug, V: Debug>(
    fingerprint: &mut DebugFingerprint,
    entries: impl IntoIterator<Item = (K, V)>,
) {
    let mut entries = entries
        .into_iter()
        .map(|(key, value)| (format!("{key:?}"), value))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    fingerprint.add(&entries.len());
    for (key, value) in entries {
        fingerprint.add(&key);
        fingerprint.add(&value);
    }
}

fn add_text_config(
    fingerprint: &mut DebugFingerprint,
    text_config: &gaanim_text::prelude::TextConfig,
) {
    let mut rest = text_config.clone();
    let roles = std::mem::take(&mut rest.roles);
    fingerprint.add(&rest);
    add_sorted(fingerprint, roles);
}

fn add_theme(fingerprint: &mut DebugFingerprint, theme: &CanvasTheme) {
    let mut rest = theme.clone();
    // `TextConfig::default()` is not empty; leave an empty role map behind.
    let text = std::mem::replace(
        &mut rest.text,
        gaanim_text::prelude::TextConfig {
            roles: Default::default(),
        },
    );
    let text_styles = std::mem::take(&mut rest.text_styles);
    let colors = std::mem::take(&mut rest.colors);
    let styles = std::mem::take(&mut rest.styles);
    // Font files are hashed by content instead of printed byte by byte.
    let fonts = std::mem::take(&mut rest.fonts);
    fingerprint.add(&rest);
    add_text_config(fingerprint, &text);
    add_sorted(fingerprint, text_styles);
    add_sorted(fingerprint, colors);
    add_sorted(fingerprint, styles);
    for font in fonts {
        fingerprint.add(&(
            font.family,
            font.bytes.len(),
            font_content_hash(&font.bytes),
        ));
    }
}

/// Hash of a font file's bytes, computed once per allocation: the bytes
/// behind an `Arc` never change while it lives, and scenes fingerprint their
/// theme on every reload and measurement.
fn font_content_hash(bytes: &Arc<[u8]>) -> u64 {
    type Hashes = HashMap<usize, (Weak<[u8]>, u64)>;
    static HASHES: OnceLock<Mutex<Hashes>> = OnceLock::new();
    let address = Arc::as_ptr(bytes).cast::<u8>() as usize;
    let mut hashes = HASHES
        .get_or_init(Default::default)
        .lock()
        .expect("font hashes poisoned");
    if let Some((allocation, hash)) = hashes.get(&address)
        && allocation
            .upgrade()
            .is_some_and(|allocation| Arc::ptr_eq(&allocation, bytes))
    {
        return *hash;
    }
    let mut hasher = DefaultHasher::new();
    hasher.write(bytes);
    let hash = hasher.finish();
    hashes.retain(|_, (allocation, _)| allocation.strong_count() > 0);
    hashes.insert(address, (Arc::downgrade(bytes), hash));
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_hashes_follow_content_across_allocations() {
        let hash = |bytes: &[u8]| {
            let mut hasher = DefaultHasher::new();
            hasher.write(bytes);
            hasher.finish()
        };
        let font: Arc<[u8]> = Arc::from(&b"font one"[..]);
        assert_eq!(font_content_hash(&font), hash(b"font one"));
        assert_eq!(font_content_hash(&font.clone()), hash(b"font one"));
        let copy: Arc<[u8]> = Arc::from(&b"font one"[..]);
        assert_eq!(font_content_hash(&copy), hash(b"font one"));
        drop((font, copy));
        // A new allocation, possibly at a freed address, hashes its own bytes.
        for _ in 0..8 {
            let other: Arc<[u8]> = Arc::from(&b"font two"[..]);
            assert_eq!(font_content_hash(&other), hash(b"font two"));
        }
    }
}
