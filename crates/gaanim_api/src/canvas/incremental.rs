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
    /// The authored state that the compilation reads after the last
    /// segment, outside the segments (polls, live zones, parameter ranges).
    /// An incremental replay compiles that part again anyway.
    after_segments: Option<u64>,
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

    /// Whether every input that compiling either revision reads is provably
    /// identical, so both compile to the same scene.
    pub(crate) fn unchanged(&self, other: &Self) -> bool {
        self.segments.len() == other.segments.len()
            && self.shared_prefix(other) == self.segments.len()
            && self.after_segments.is_some()
            && self.after_segments == other.after_segments
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
        let segments = fingerprint_each(&state.segments, |segment| {
            let mut fingerprint = DebugFingerprint::new();
            add_segment(&mut fingerprint, segment, &state.frozen_spawn_specs);
            fingerprint.finish()
        });
        let mut after_segments = DebugFingerprint::new();
        after_segments.add(&state.polls);
        after_segments.add(&state.poll_session);
        after_segments.add(&state.poll_lobby);
        after_segments.add(&state.poll_lobby_at);
        after_segments.add(&state.rehearsal);
        after_segments.add(&state.poll_teams);
        after_segments.add(&state.poll_ask);
        after_segments.add(&state.stop_gates);
        after_segments.add(&state.live_zones);
        // Readouts inside layouts keep room for the values parameters take.
        add_sorted(&mut after_segments, &state.parameter_ranges);
        SceneFingerprints {
            global: global.finish(),
            segments,
            after_segments: after_segments.finish(),
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
            // What authoring cost.
            profile: _,
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

/// Feed what compiling `segment` reads from it. A declaration compiles from
/// its frozen spec plus the few live fields that apply to the whole scene
/// (see `SceneModel::compile_resumable`), so a setter that later changes
/// the live spec through a timeline cut changes the segment holding the cut,
/// not the one that declared the drawable.
fn add_segment(
    fingerprint: &mut DebugFingerprint,
    segment: &crate::canvas::ops::Segment,
    frozen: &HashMap<gaanim_core::ObjectId, crate::canvas::types::ObjectSpec>,
) {
    // Exhaustive on purpose: a new segment field must be fingerprinted.
    let crate::canvas::ops::Segment {
        id,
        name,
        notes,
        template,
        background,
        post_process,
        stops,
        markers,
        explicit,
        cursor,
        ops,
        transition,
        prev_segment,
        mobject_ids,
    } = segment;
    fingerprint.add(id);
    fingerprint.add(name);
    fingerprint.add(notes);
    fingerprint.add(template);
    fingerprint.add(background);
    fingerprint.add(post_process);
    fingerprint.add(stops);
    fingerprint.add(markers);
    fingerprint.add(explicit);
    fingerprint.add(cursor);
    fingerprint.add(transition);
    fingerprint.add(prev_segment);
    fingerprint.add(mobject_ids);
    fingerprint.add(&ops.len());
    for op in ops {
        let Op::Spawn(spec) = op else {
            fingerprint.add(op);
            continue;
        };
        let live = spec.lock().expect("object spec poisoned");
        let Some(declared) = frozen.get(&live.id) else {
            fingerprint.add(&*live);
            continue;
        };
        fingerprint.add(&"Spawn");
        fingerprint.add(declared);
        fingerprint.add(&live.z_index);
        fingerprint.add(&live.layout_background);
        fingerprint.add(&live.layout_owner);
        // Media schedules come from the live spec, and readouts in a box
        // keep room for what their live source shows.
        match &live.kind {
            crate::canvas::types::SpawnKind::Video { .. }
            | crate::canvas::types::SpawnKind::Lottie { .. }
            | crate::canvas::types::SpawnKind::ReactiveReadout { .. } => {
                fingerprint.add(&live.kind);
            }
            _ => {}
        }
    }
}

/// `fingerprint` of each of `segments`, on as many threads as the machine
/// runs, since segments are independent; the largest go first. A value that
/// prints a `Mutex` another thread holds comes out opaque, so the segments
/// that did are fingerprinted again alone: the result is the serial one.
fn fingerprint_each(
    segments: &[crate::canvas::ops::Segment],
    fingerprint: impl Fn(&crate::canvas::ops::Segment) -> Option<u64> + Sync,
) -> Vec<Option<u64>> {
    let threads = std::thread::available_parallelism()
        .map_or(1, |threads| threads.get())
        .min(segments.len());
    if threads <= 1 {
        return segments.iter().map(&fingerprint).collect();
    }
    let mut order: Vec<usize> = (0..segments.len()).collect();
    order.sort_by_key(|&index| std::cmp::Reverse(segments[index].ops.len()));
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut fingerprints = vec![None; segments.len()];
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let taken = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(&index) = order.get(taken) else {
                            return done;
                        };
                        done.push((index, fingerprint(&segments[index])));
                    }
                })
            })
            .collect();
        for worker in workers {
            for (index, value) in worker.join().expect("fingerprint worker panicked") {
                fingerprints[index] = value;
            }
        }
    });
    for (segment, value) in segments.iter().zip(&mut fingerprints) {
        if value.is_none() {
            *value = fingerprint(segment);
        }
    }
    fingerprints
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
    fn state_read_after_the_segments_decides_whether_a_scene_is_unchanged() {
        let scene = || {
            let mut scene = SceneModel::new(640, 360);
            let dot = scene.circle(0.3);
            scene.play(vec![dot.animate().shift_by(1.0, 0.0).duration(0.5)]);
            scene
        };
        let text = gaanim_text::prelude::TextConfig::default();
        let fonts = gaanim_text::font::FontRegistry::without_system_fonts();
        let first = scene().fingerprints(&text, &fonts);
        assert!(first.unchanged(&scene().fingerprints(&text, &fonts)));

        let gated = scene();
        gated.state.lock().unwrap().stop_gates.push((
            0,
            0.5,
            gaanim_timeline::timeline::GateCondition::Answers {
                poll: "votes".into(),
                count: 3,
            },
        ));
        let gated = gated.fingerprints(&text, &fonts);
        assert_eq!(gated.shared_prefix(&first), first.segment_count());
        assert!(!gated.unchanged(&first));
    }

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
