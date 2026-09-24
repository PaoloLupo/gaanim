//! Fingerprints that decide which segments an incremental replay may reuse.
//!
//! A segment compiles from its own ops, the frozen spawn specs of the objects
//! it declares, the state carried from earlier segments, and scene-wide
//! inputs. Two revisions whose scene-wide inputs and leading segments have
//! equal fingerprints therefore compile those segments identically.

use std::fmt::Debug;
use std::hash::{DefaultHasher, Hasher};

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
        // Exhaustive on purpose: a new scene-wide field must be fingerprinted.
        let SceneModel {
            frame,
            background,
            background_paint,
            background_overridden,
            post_process,
            theme,
            theme_style,
            font_family_override,
            math_font_family_override,
            code_font_family_override,
            margin,
            asset_root,
            audio_tracks,
            branding,
            camera_position,
            lighting_3d,
            state,
        } = self;
        let mut global = DebugFingerprint::new();
        global.add(frame);
        global.add(background);
        global.add(background_paint);
        global.add(background_overridden);
        global.add(post_process);
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
        global.add(audio_tracks);
        global.add(branding);
        global.add(camera_position);
        global.add(lighting_3d);
        add_text_config(&mut global, text_config);
        global.add(&gaanim_text::typst_compiler::registered_fonts_fingerprint(
            font_registry,
        ));

        let state = state.lock().expect("canvas state poisoned");
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
        let mut hasher = DefaultHasher::new();
        hasher.write(&font.bytes);
        fingerprint.add(&(font.family, font.bytes.len(), hasher.finish()));
    }
}
