//! Timeline lenses for typewriter and scramble text motion.
//!
//! Each lens rewrites one glyph's (or the cursor's) `Path2D` from its clip
//! progress alone, so playback, exact seeks and export agree on every frame.
//! Scheduling (keystrokes, settle order, charsets) lives in
//! [`gaanim_text::motion`].

use std::sync::Arc;

use bevy::prelude::{Entity, World};
use gaanim_core::kurbo::{Affine, BezPath, Shape};
use gaanim_scene::Path2D;
use gaanim_text::motion::scramble_pick;

use crate::tween::AnimatableLens;

fn write_path(world: &mut World, entity: Entity, path: &Arc<BezPath>) {
    if let Some(mut current) = world.get_mut::<Path2D>(entity)
        && !Arc::ptr_eq(&current.0, path)
        && current.0.elements() != path.elements()
    {
        current.0 = path.clone();
    }
}

fn write_owned(world: &mut World, entity: Entity, path: BezPath) {
    if let Some(mut current) = world.get_mut::<Path2D>(entity)
        && current.0.elements() != path.elements()
    {
        current.0 = Arc::new(path);
    }
}

/// Shows a glyph outline only while `appear <= t < vanish` (either bound
/// optional), hiding it with an empty path otherwise.
#[derive(Debug, Clone)]
pub struct GlyphVisibilityLens {
    pub path: Arc<BezPath>,
    pub appear: Option<f64>,
    pub vanish: Option<f64>,
}

impl GlyphVisibilityLens {
    pub fn visible_at(&self, t: f64) -> bool {
        self.appear.is_none_or(|appear| t >= appear) && self.vanish.is_none_or(|vanish| t < vanish)
    }
}

impl AnimatableLens for GlyphVisibilityLens {
    fn interpolate(&self, world: &mut World, entity: Entity, t: f64) {
        if self.visible_at(t) {
            write_path(world, entity, &self.path);
        } else {
            write_owned(world, entity, BezPath::new());
        }
    }

    fn clone_box(&self) -> Box<dyn AnimatableLens> {
        Box::new(self.clone())
    }

    fn type_name(&self) -> &'static str {
        "Typewriter"
    }
}

/// A text cursor that jumps between pen positions `(from_t, x, baseline)`
/// (sorted by `from_t`). It stays visible after the clip unless
/// `hide_at_end` is set.
#[derive(Debug, Clone)]
pub struct CursorLens {
    /// Cursor outline with its pen origin at `(0, 0)` on the baseline.
    pub glyph: Arc<BezPath>,
    pub steps: Arc<[(f64, f64, f64)]>,
    pub hide_at_end: bool,
}

impl CursorLens {
    pub fn position_at(&self, t: f64) -> Option<(f64, f64)> {
        self.steps
            .iter()
            .take_while(|(from, _, _)| *from <= t)
            .last()
            .or(self.steps.first())
            .map(|(_, x, y)| (*x, *y))
    }

    pub fn path_at(&self, t: f64) -> BezPath {
        if self.hide_at_end && t >= 1.0 {
            return BezPath::new();
        }
        self.position_at(t)
            .map(|(x, y)| Affine::translate((x, y)) * &*self.glyph)
            .unwrap_or_default()
    }
}

impl AnimatableLens for CursorLens {
    fn interpolate(&self, world: &mut World, entity: Entity, t: f64) {
        write_owned(world, entity, self.path_at(t));
    }

    fn clone_box(&self) -> Box<dyn AnimatableLens> {
        Box::new(self.clone())
    }

    fn type_name(&self) -> &'static str {
        "TypewriterCursor"
    }
}

/// Glyph outlines of a scramble charset, shaped once per compile.
#[derive(Debug, Clone, Default)]
pub struct ScrambleCharset {
    /// Outline with its pen origin on the baseline, and its ink center x.
    pub glyphs: Vec<(BezPath, f64)>,
}

impl ScrambleCharset {
    /// Wrap pen-origin outlines, measuring each ink center.
    pub fn new(outlines: impl IntoIterator<Item = BezPath>) -> Self {
        Self {
            glyphs: outlines
                .into_iter()
                .filter(|path| !path.elements().is_empty())
                .map(|path| {
                    let center = path.bounding_box().center().x;
                    (path, center)
                })
                .collect(),
        }
    }
}

/// One scrambled position: a seeded charset glyph centered in the final
/// grapheme's cell until progress reaches `settle_at`, then the final glyph.
///
/// The glyph changes `speed` times per second of clip time and is chosen by
/// `hash(seed, position, ⌊time·speed⌋)`. Secondary glyphs of a grapheme
/// (`primary == false`) stay empty until the grapheme settles.
#[derive(Debug, Clone)]
pub struct ScrambleGlyphLens {
    pub final_path: Arc<BezPath>,
    pub charset: Arc<ScrambleCharset>,
    /// Ink center x and baseline y of the final grapheme, in glyph space.
    pub anchor: (f64, f64),
    pub settle_at: f64,
    pub duration: f64,
    pub speed: f64,
    pub seed: u64,
    pub position: usize,
    pub primary: bool,
}

impl ScrambleGlyphLens {
    /// Index of the charset glyph shown at progress `t`, if scrambling.
    pub fn pick_at(&self, t: f64) -> Option<usize> {
        if t >= self.settle_at || !self.primary || self.charset.glyphs.is_empty() {
            return None;
        }
        let step = (t * self.duration * self.speed).floor() as i64;
        Some(scramble_pick(
            self.seed,
            self.position,
            step,
            self.charset.glyphs.len(),
        ))
    }
}

impl AnimatableLens for ScrambleGlyphLens {
    fn interpolate(&self, world: &mut World, entity: Entity, t: f64) {
        if t >= self.settle_at {
            write_path(world, entity, &self.final_path);
            return;
        }
        let path = match self.pick_at(t) {
            Some(index) => {
                let (glyph, center) = &self.charset.glyphs[index];
                Affine::translate((self.anchor.0 - center, self.anchor.1)) * glyph
            }
            None => BezPath::new(),
        };
        write_owned(world, entity, path);
    }

    fn clone_box(&self) -> Box<dyn AnimatableLens> {
        Box::new(self.clone())
    }

    fn type_name(&self) -> &'static str {
        "Scramble"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_core::kurbo::Rect;

    fn square(size: f64) -> BezPath {
        Rect::new(0.0, 0.0, size, size).to_path(1e-9)
    }

    fn glyph_world() -> (World, Entity) {
        let mut world = World::new();
        let entity = world.spawn(Path2D(Arc::new(square(1.0)))).id();
        (world, entity)
    }

    #[test]
    fn glyph_visibility_follows_its_window() {
        let (mut world, entity) = glyph_world();
        let lens = GlyphVisibilityLens {
            path: Arc::new(square(1.0)),
            appear: Some(0.25),
            vanish: Some(0.75),
        };
        let empty = |world: &World| world.get::<Path2D>(entity).unwrap().0.is_empty();
        lens.interpolate(&mut world, entity, 0.1);
        assert!(empty(&world));
        lens.interpolate(&mut world, entity, 0.25);
        assert!(!empty(&world));
        lens.interpolate(&mut world, entity, 0.75);
        assert!(empty(&world));
        // Seeking back reproduces the same state.
        lens.interpolate(&mut world, entity, 0.5);
        assert!(!empty(&world));
    }

    #[test]
    fn cursor_jumps_between_pens_and_can_leave() {
        let lens = CursorLens {
            glyph: Arc::new(square(0.1)),
            steps: Arc::from(vec![(0.0, 0.0, 0.0), (0.5, 1.0, 0.0), (1.0, 2.0, -1.0)]),
            hide_at_end: false,
        };
        assert_eq!(lens.position_at(0.0), Some((0.0, 0.0)));
        assert_eq!(lens.position_at(0.49), Some((0.0, 0.0)));
        assert_eq!(lens.position_at(0.5), Some((1.0, 0.0)));
        assert_eq!(lens.position_at(1.0), Some((2.0, -1.0)));
        assert_eq!(lens.path_at(0.7).bounding_box().x0, 1.0);
        let leaving = CursorLens {
            hide_at_end: true,
            ..lens
        };
        assert!(leaving.path_at(1.0).is_empty());
        assert!(!leaving.path_at(0.9).is_empty());
    }

    #[test]
    fn scramble_cycles_seeded_glyphs_then_settles() {
        let charset = Arc::new(ScrambleCharset::new([
            square(0.2),
            square(0.4),
            square(0.6),
        ]));
        let lens = ScrambleGlyphLens {
            final_path: Arc::new(square(1.0)),
            charset,
            anchor: (5.0, 1.0),
            settle_at: 0.8,
            duration: 2.0,
            speed: 10.0,
            seed: 3,
            position: 4,
            primary: true,
        };
        let picks: Vec<_> = (0..16)
            .map(|step| lens.pick_at(step as f64 / 20.0))
            .collect();
        assert!(picks.iter().all(Option::is_some));
        assert!(picks.windows(2).any(|pair| pair[0] != pair[1]));
        // Within one step (0.05 s of clip time) the glyph holds.
        assert_eq!(lens.pick_at(0.1), lens.pick_at(0.124));
        assert_eq!(lens.pick_at(0.8), None);

        let (mut world, entity) = glyph_world();
        lens.interpolate(&mut world, entity, 0.3);
        let scrambled = world.get::<Path2D>(entity).unwrap().0.bounding_box();
        assert!((scrambled.center().x - 5.0).abs() < 1e-9);
        assert_eq!(scrambled.y0, 1.0);
        lens.interpolate(&mut world, entity, 0.9);
        assert_eq!(
            world.get::<Path2D>(entity).unwrap().0.bounding_box(),
            Rect::new(0.0, 0.0, 1.0, 1.0)
        );
        let secondary = ScrambleGlyphLens {
            primary: false,
            ..lens
        };
        secondary.interpolate(&mut world, entity, 0.3);
        assert!(world.get::<Path2D>(entity).unwrap().0.is_empty());
    }
}
