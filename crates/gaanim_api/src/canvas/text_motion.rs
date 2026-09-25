//! Fluent authoring of typewriter and scramble text motion on `Anim`, and
//! the compile-time hook that attaches the Text's typesetting context.

use std::collections::HashMap;
use std::sync::Arc;

use gaanim_core::ObjectId;
use gaanim_math::Bounds3D;
use gaanim_text::prelude::{TextRole, TextSpec};

use super::types::{Anim, ObjectSpec, SpawnKind};
use crate::anim::{AnimationBuilder, AnimationType};
use crate::text_motion::{TextMotion, TextMotionContext, TextMotionKind, TypedText, resolve};

fn positive(name: &str, value: f64) -> Result<(), String> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(format!("{name} must be finite and positive"))
    }
}

fn non_negative(name: &str, value: f64) -> Result<(), String> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(format!("{name} must be finite and non-negative"))
    }
}

fn jitter_in_range(jitter: f64) -> Result<(), String> {
    if jitter.is_finite() && (0.0..1.0).contains(&jitter) {
        Ok(())
    } else {
        Err("jitter must be within [0, 1)".into())
    }
}

fn plain_text(name: &str, text: &str) -> Result<(), String> {
    if text.is_empty() {
        Err(format!(
            "{name} must not be empty; use backspace() to clear the text"
        ))
    } else if text.chars().any(|c| c.is_control() && c != '\n') {
        Err(format!("{name} must not contain control characters"))
    } else {
        Ok(())
    }
}

impl Anim {
    /// Whether this proxy animates a whole Text (not a selection), the
    /// target typewriter and scramble motions require.
    pub fn is_text_motion_target(&self) -> bool {
        !self.property_target_is_text_selection()
            && self.text_motion_spec().is_some_and(|spec| {
                matches!(
                    spec.lock().expect("object spec poisoned").kind,
                    SpawnKind::Text(_)
                )
            })
    }

    fn text_motion(self, kind: TextMotionKind) -> Result<Self, String> {
        if !self.inner.anim_type.is_empty_properties() {
            return Err("text motions cannot be combined with property targets or another effect in one Anim; combine separate animations with parallel()".into());
        }
        if !self.is_text_motion_target() {
            return Err("typewriter and scramble motions require a Text animation proxy".into());
        }
        let spec = self
            .text_motion_spec()
            .ok_or("typewriter and scramble motions require a Text animation proxy")?;
        let before = {
            let spec = spec.lock().expect("object spec poisoned");
            let SpawnKind::Text(text) = &spec.kind else {
                return Err(
                    "typewriter and scramble motions require a Text animation proxy".into(),
                );
            };
            match &spec.typed_text {
                Some((version, typed)) if *version == text.version => typed.clone(),
                _ => TypedText::full(text.rendered_text()),
            }
        };
        let (after, _, duration) = resolve(&before, &kind);
        Ok(self.with_text_motion(
            TextMotion {
                kind,
                after,
                context: None,
            },
            duration,
        ))
    }

    /// Clear the Text and type it again: `cps` graphemes per second, each
    /// keystroke interval scaled by a seeded factor in `[1 - jitter, 1 + jitter]`.
    /// A `cursor` string (none when `None`) follows the typed text, stays
    /// solid while typing and blinks `blink` times per second afterwards;
    /// `keep_cursor = false` removes it when typing ends. Without an explicit
    /// duration the motion lasts until the last keystroke (about `n / cps`).
    pub fn typewriter(
        self,
        cps: f64,
        cursor: Option<&str>,
        blink: f64,
        jitter: f64,
        seed: u64,
        keep_cursor: bool,
    ) -> Result<Self, String> {
        positive("cps", cps)?;
        non_negative("blink", blink)?;
        jitter_in_range(jitter)?;
        let cursor = cursor.filter(|cursor| !cursor.trim().is_empty());
        if cursor.is_some_and(|cursor| cursor.chars().any(char::is_control)) {
            return Err("cursor must be a single line of visible text".into());
        }
        self.text_motion(TextMotionKind::Typewriter {
            cps,
            jitter,
            seed,
            cursor: cursor.map(str::to_owned),
            blink,
            keep_cursor,
        })
    }

    /// Delete the last `count` visible graphemes (all of them when `None`)
    /// at a steady `cps`, moving the cursor back.
    pub fn backspace(self, count: Option<usize>, cps: f64) -> Result<Self, String> {
        positive("cps", cps)?;
        self.text_motion(TextMotionKind::Backspace {
            count: count.unwrap_or(usize::MAX),
            cps,
        })
    }

    /// Delete back to the longest prefix shared with `text`, then type the
    /// rest of `text` (plain text, laid out with the Text's style).
    pub fn retype(self, text: &str, cps: f64, jitter: f64, seed: u64) -> Result<Self, String> {
        plain_text("text", text)?;
        positive("cps", cps)?;
        jitter_in_range(jitter)?;
        self.text_motion(TextMotionKind::Retype {
            text: text.to_owned(),
            cps,
            jitter,
            seed,
        })
    }

    /// Decode the Text: every grapheme cycles through seeded glyphs of
    /// `charset` (`speed` changes per second) and settles left to right
    /// after `reveal_delay` seconds. The final layout is reserved, so the
    /// width never jumps.
    pub fn scramble(
        self,
        charset: &str,
        reveal_delay: f64,
        speed: f64,
        seed: u64,
    ) -> Result<Self, String> {
        self.scramble_inner(None, charset, reveal_delay, speed, seed)
    }

    /// Like [`Self::scramble`], decoding into new plain `text`.
    pub fn scramble_to(
        self,
        text: &str,
        charset: &str,
        reveal_delay: f64,
        speed: f64,
        seed: u64,
    ) -> Result<Self, String> {
        plain_text("text", text)?;
        self.scramble_inner(Some(text), charset, reveal_delay, speed, seed)
    }

    fn scramble_inner(
        self,
        text: Option<&str>,
        charset: &str,
        reveal_delay: f64,
        speed: f64,
        seed: u64,
    ) -> Result<Self, String> {
        non_negative("reveal_delay", reveal_delay)?;
        positive("speed", speed)?;
        let charset = gaanim_text::motion::charset_graphemes(charset)?;
        self.text_motion(TextMotionKind::Scramble {
            text: text.map(str::to_owned),
            charset,
            reveal_delay,
            speed,
            seed,
        })
    }
}

/// Attach the target Text's fonts and a typesetter for new content to a
/// text motion, mirroring how the Text itself was compiled.
pub(super) fn attach_text_motion_context(
    object_specs: &HashMap<ObjectId, ObjectSpec>,
    frame_bounds: Bounds3D,
    text_config: &gaanim_text::prelude::TextConfig,
    authored_target: ObjectId,
    anim: &mut AnimationBuilder,
) {
    let AnimationType::TextMotion(motion) = &mut anim.anim_type else {
        return;
    };
    let Some(spec) = object_specs.get(&authored_target) else {
        return;
    };
    let SpawnKind::Text(text) = &spec.kind else {
        return;
    };
    let (Some(role), Some(math)) = (
        text_config.roles.get(&text.role),
        text_config.roles.get(&TextRole::Math),
    ) else {
        return;
    };
    let font_family = text
        .style
        .font
        .clone()
        .unwrap_or_else(|| role.font_family.clone());
    let font_size = text.style.size.unwrap_or(role.size).max(1.0e-6);
    let color = match &spec.fill {
        Some(gaanim_core::peniko::Brush::Solid(color)) => *color,
        _ => text.style.color.unwrap_or(role.fill_color),
    };
    let width = frame_bounds.width().max(1.0);
    let base = text.clone();
    let family = font_family.clone();
    let typeset = move |content: &str| {
        let spec = TextSpec::new_with_markup(
            vec![content.into()],
            Some(base.role),
            base.style.clone(),
            base.flow.clone(),
            false,
        )
        .ok()?;
        Some(super::compile::structured_text_typst_source(
            &spec,
            Some(width),
            font_size,
            &family,
            color,
        ))
    };
    motion.context = Some(TextMotionContext {
        rendered: text.rendered_text(),
        font_family,
        math_font: text
            .style
            .math_font
            .clone()
            .unwrap_or_else(|| math.font_family.clone()),
        font_size,
        weight: text.style.weight,
        typeset: Arc::new(typeset),
    });
}

#[cfg(test)]
mod tests {
    use super::super::SceneModel;
    use crate::anim::AnimationType;

    fn motion_duration(anim: &super::Anim) -> f64 {
        assert!(matches!(anim.inner.anim_type, AnimationType::TextMotion(_)));
        anim.inner.duration
    }

    #[test]
    fn typing_motions_default_to_their_keystroke_time() {
        let mut canvas = SceneModel::new(640, 360);
        let prompt = canvas.text("gaanim render");
        let typing = prompt
            .animate()
            .typewriter(10.0, Some("\u{258D}"), 2.0, 0.0, 0, true)
            .unwrap();
        assert!((motion_duration(&typing) - 1.3).abs() < 1e-9);
        canvas.play(vec![typing]);

        let deleting = prompt.animate().backspace(Some(6), 20.0).unwrap();
        assert!((motion_duration(&deleting) - 0.3).abs() < 1e-9);
        canvas.play(vec![deleting]);

        // The authoring state now shows "gaanim ", so retype keeps 7 graphemes.
        let retyping = prompt
            .animate()
            .retype("gaanim export", 10.0, 0.0, 0)
            .unwrap();
        assert!((motion_duration(&retyping) - 0.6).abs() < 1e-9);
        let explicit = prompt
            .animate()
            .duration(2.5)
            .retype("gaanim export", 10.0, 0.0, 0)
            .unwrap();
        assert_eq!(motion_duration(&explicit), 2.5);

        let scrambled = prompt
            .animate()
            .scramble_to("LAUNCH", "01", 0.3, 20.0, 1)
            .unwrap();
        assert!((motion_duration(&scrambled) - 0.9).abs() < 1e-9);

        assert!(
            prompt
                .animate()
                .typewriter(0.0, None, 2.0, 0.2, 0, true)
                .is_err()
        );
        assert!(
            prompt
                .animate()
                .typewriter(18.0, None, 2.0, 1.0, 0, true)
                .is_err()
        );
        assert!(prompt.animate().scramble(" ", 0.3, 20.0, 0).is_err());
        assert!(prompt.animate().retype("", 18.0, 0.2, 0).is_err());
        let circle = canvas.circle(1.0);
        assert!(
            circle
                .animate()
                .typewriter(18.0, None, 2.0, 0.2, 0, true)
                .is_err()
        );
    }

    /// Visible glyphs, visible cursor x (if any) and the cursor count after
    /// seeking the compiled timeline to `time`.
    fn typed_state(
        world: &mut bevy::prelude::World,
        timeline: &mut gaanim_timeline::timeline::Timeline,
        time: f64,
    ) -> (usize, Option<f64>) {
        use gaanim_core::kurbo::Shape;
        timeline.seek(world, time);
        // Count glyphs the renderer fills: a non-empty closed path that
        // differs from its `PathSource` is drawn as a trimmed outline only.
        let glyphs = world
            .query::<(
                &gaanim_scene::Path2D,
                Option<&gaanim_scene::PathSource>,
                &gaanim_scene::components::TextSpan,
            )>()
            .iter(world)
            .filter(|(path, source, _)| {
                !path.0.is_empty()
                    && source.is_none_or(|source| source.0.elements() == path.0.elements())
            })
            .count();
        let cursor = world
            .query_filtered::<&gaanim_scene::Path2D, (
                bevy::prelude::With<bevy::prelude::ChildOf>,
                bevy::prelude::Without<gaanim_scene::components::TextSpan>,
            )>()
            .iter(world)
            .find(|path| !path.0.is_empty())
            .map(|path| path.0.bounding_box().x0);
        (glyphs, cursor)
    }

    #[test]
    fn compiled_typing_is_a_pure_function_of_time() {
        use bevy::ecs::world::CommandQueue;
        let mut canvas = SceneModel::new(640, 360);
        let prompt = canvas.text("abc def");
        // 7 keystrokes at 10 cps: 0.0-0.7 s.
        canvas.play(vec![
            prompt
                .animate()
                .typewriter(10.0, Some("\u{258D}"), 2.0, 0.0, 0, true)
                .unwrap(),
        ]);
        // 3 deletions: 0.7-1.0 s, leaving "abc ".
        canvas.play(vec![prompt.animate().backspace(Some(3), 10.0).unwrap()]);
        // Keeps "abc ", types "xyz!": 1.0-1.4 s.
        canvas.play(vec![
            prompt.animate().retype("abc xyz!", 10.0, 0.0, 0).unwrap(),
        ]);
        // Decodes into a new word: 1.4-2.3 s.
        canvas.play(vec![
            prompt
                .animate()
                .scramble_to("READY", "upper", 0.3, 20.0, 0)
                .unwrap(),
        ]);

        let world = bevy::prelude::World::new();
        let mut queue = CommandQueue::default();
        let mut commands = bevy::prelude::Commands::new(&mut queue, &world);
        let mut timeline = gaanim_timeline::timeline::Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);
        let mut world = world;
        queue.apply(&mut world);
        timeline.add_keyframe(
            0.0,
            gaanim_timeline::snapshot::WorldSnapshot::capture(&mut world),
        );

        let expected = [
            (0.05, 0),
            (0.25, 2),
            (0.45, 3),
            (0.65, 5),
            (0.75, 6),
            (0.85, 5),
            (1.05, 3),
            (1.15, 4),
            (1.35, 6),
        ];
        let mut cursors = Vec::new();
        for (time, glyphs) in expected {
            let (visible, cursor) = typed_state(&mut world, &mut timeline, time);
            assert_eq!(visible, glyphs, "t={time}");
            cursors.push(cursor.expect("cursor visible while typing"));
        }
        // The cursor advances while typing and returns while deleting.
        assert!(cursors[1] < cursors[2] && cursors[2] < cursors[4]);
        assert!(cursors[5] < cursors[4] && cursors[6] < cursors[5]);
        // Seeking backwards reproduces earlier frames exactly.
        for (time, glyphs) in expected.iter().rev() {
            assert_eq!(
                typed_state(&mut world, &mut timeline, *time).0,
                *glyphs,
                "t={time}"
            );
        }
        // Mid-scramble every final position shows a glyph; the result is exact.
        let (scrambling, _) = typed_state(&mut world, &mut timeline, 1.6);
        assert_eq!(scrambling, 5);
        let paths = |world: &mut bevy::prelude::World| {
            world
                .query::<(&gaanim_scene::Path2D, &gaanim_scene::components::TextSpan)>()
                .iter(world)
                .filter(|(path, _)| !path.0.is_empty())
                .map(|(path, _)| path.0.elements().to_vec())
                .collect::<Vec<_>>()
        };
        let first = paths(&mut world);
        typed_state(&mut world, &mut timeline, 0.3);
        typed_state(&mut world, &mut timeline, 1.6);
        assert_eq!(paths(&mut world), first);
        assert_eq!(typed_state(&mut world, &mut timeline, 2.4).0, 5);
    }
}
