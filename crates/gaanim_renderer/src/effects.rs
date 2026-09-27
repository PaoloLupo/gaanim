use bevy::prelude::{Component, Entity};
use gaanim_core::peniko::Color;

/// Component: Adds a soft drop shadow effect to a 2D Mobject.
#[derive(Component, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DropShadow {
    /// Horizontal and vertical offset of the shadow in world units.
    pub offset: gaanim_core::glam::DVec2,
    /// Standard deviation of the Gaussian blur filter.
    pub blur_radius: f64,
    /// Shadow color (usually semi-transparent black).
    pub color: Color,
}

impl Default for DropShadow {
    fn default() -> Self {
        Self {
            offset: gaanim_core::glam::DVec2::new(0.08, -0.08),
            blur_radius: 0.06,
            color: Color::from_rgba8(0, 0, 0, 128),
        }
    }
}

/// Component: composites the element onto what is drawn beneath it with a
/// blend mode instead of plain source-over (e.g. a multiply highlighter).
///
/// The element is drawn in its own layer, so inside an isolated group (an
/// opacity group, a clip or a transition reveal) it blends with that group's
/// content only.
/// The default (normal) mode paints plainly; it lets a group member opt out
/// of the mode its group gives its members.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct ElementBlend(pub gaanim_core::peniko::BlendMode);

/// Component: where a stroke sits relative to a closed contour.
///
/// Without it a closed contour keeps its stroke inside the shape, so a
/// `write` draws a constant width, and an open path centers its stroke.
/// `Outside` draws the whole width beyond the contour, e.g. for a text halo.
/// Open paths always center their stroke.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum StrokeAlign {
    #[default]
    Inside,
    Center,
    Outside,
}

/// Component: Adds an outer glow outline effect to a 2D Mobject.
#[derive(Component, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Glow {
    /// Total spread radius of the glow outline.
    pub radius: f64,
    /// Intensity multiplier of the glow color.
    pub intensity: f32,
    /// Color of the outer glow.
    pub color: Color,
}

impl Default for Glow {
    fn default() -> Self {
        Self {
            radius: 0.16,
            intensity: 1.0,
            color: Color::from_rgba8(255, 255, 255, 255),
        }
    }
}

/// Component: Applies a Gaussian blur filter to the Mobject.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GaussianBlur {
    /// Standard deviation of the Gaussian filter.
    pub sigma: f64,
}

impl Default for GaussianBlur {
    fn default() -> Self {
        Self { sigma: 2.0 }
    }
}

/// Component: Clips the rendering of this Mobject and all its children using a vector path.
#[derive(Component, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ClipMask {
    /// The geometric clipping outline path.
    pub path: gaanim_core::kurbo::BezPath,
    /// The fill rule (NonZero or EvenOdd) used to interpret path interior.
    pub rule: gaanim_core::peniko::Fill,
    /// Vector leaves that define this mask.  They are resolved each frame in
    /// `SceneSet::DerivedGeometry`, after hierarchy transforms have settled.
    pub sources: Vec<Entity>,
    /// Interpret the mask as its complement. Inversion is represented as an
    /// even-odd layer with a deliberately large enclosing rectangle.
    pub invert: bool,
}

/// How the region framed by a [`CameraView`] source fills its screen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CameraViewFit {
    /// Scale uniformly so the whole framed region stays visible.
    #[default]
    Contain,
    /// Scale uniformly so the framed region covers the whole screen.
    Cover,
    /// Scale each axis independently so the region matches the screen.
    Stretch,
}

/// Paint behind the content of a [`CameraView`], inside its screen.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CameraViewBackground {
    /// The canvas background as the view's camera sees it.
    #[default]
    Canvas,
    /// No backdrop: the screen's own fill shows behind the content.
    None,
    /// A brush in the screen's local coordinates.
    Brush(gaanim_core::peniko::Brush),
}

/// Component: turns a closed vector leaf into a screen that shows the scene
/// as a second camera sees it.
///
/// The camera frames `source`: its local bounds under its world transform, so
/// moving, scaling or rotating the source pans, zooms or rotates the view.
/// With an explicit `zoom` the source only sets where the camera looks (its
/// center) and its rotation, and `zoom` scales world units into the screen's
/// local units. The screen draws its shadow, glow and fill, then the backdrop
/// and the scene clipped to its outline, then its stroke on top. The screen,
/// the source and every `exclude` subtree are left out of the view, and views
/// never show the content of other views.
#[derive(Component, Debug, Clone)]
pub struct CameraView {
    /// Drawable whose bounds frame the captured region.
    pub source: Entity,
    pub fit: CameraViewFit,
    pub background: CameraViewBackground,
    /// Roots of subtrees the view does not show.
    pub exclude: Vec<Entity>,
    /// Explicit magnification; `None` derives it from the source's size.
    pub zoom: Option<gaanim_animation::TrackingScalar>,
    /// View layers shown besides the drawables on no layer.
    pub layers: Vec<std::sync::Arc<str>>,
}

/// Component: puts a drawable on a named view layer. The main camera does not
/// draw it; only camera views that list the layer show it.
#[derive(Component, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ViewLayer(pub std::sync::Arc<str>);

/// System: the tips of a stroked path (see [`gaanim_animation::StrokeTips`])
/// draw in their path's view layer and, for HUD paths, over the screen.
pub fn sync_stroke_tip_layers_system(
    mut commands: bevy::prelude::Commands,
    tips: bevy::prelude::Query<
        (
            Entity,
            &gaanim_scene::prelude::ChildOf,
            Option<&ViewLayer>,
            bevy::prelude::Has<gaanim_scene::HudOverlay>,
        ),
        bevy::prelude::With<gaanim_animation::StrokeTip>,
    >,
    paths: bevy::prelude::Query<(
        Option<&ViewLayer>,
        bevy::prelude::Has<gaanim_scene::HudOverlay>,
    )>,
) {
    for (tip, child_of, layer, hud) in &tips {
        let Ok((path_layer, path_hud)) = paths.get(child_of.parent()) else {
            continue;
        };
        if layer != path_layer {
            match path_layer {
                Some(path_layer) => {
                    commands.entity(tip).insert(path_layer.clone());
                }
                None => {
                    commands.entity(tip).remove::<ViewLayer>();
                }
            }
        }
        if hud != path_hud {
            if path_hud {
                commands.entity(tip).insert(gaanim_scene::HudOverlay);
            } else {
                commands.entity(tip).remove::<gaanim_scene::HudOverlay>();
            }
        }
    }
}

/// A retained reactive vector boolean. Sources are vector leaves in world
/// space; the result path is rebuilt in `SceneSet::DerivedGeometry`.
#[derive(Component, Debug, Clone)]
pub struct BooleanBinding {
    pub sources: Vec<Entity>,
    pub op: gaanim_objects::boolean::BooleanOp,
    pub tolerance: f64,
    pub rule: gaanim_objects::boolean::BooleanFillRule,
}

#[derive(Component, Debug, Clone)]
pub struct FillLevelBinding {
    pub sources: Vec<Entity>,
    pub direction: gaanim_scene::FillDirection,
}

/// A stroke-only live copy of a fill-level source silhouette.
#[derive(Component, Debug, Clone)]
pub struct VectorOutlineBinding {
    pub sources: Vec<Entity>,
}

impl Default for ClipMask {
    fn default() -> Self {
        Self {
            path: gaanim_core::kurbo::BezPath::default(),
            rule: gaanim_core::peniko::Fill::NonZero,
            sources: Vec::new(),
            invert: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{IntoSystem, System, World};

    #[test]
    fn stroke_tips_follow_their_path_into_hud_and_view_layers() {
        let mut world = World::new();
        let path = world
            .spawn((ViewLayer("xray".into()), gaanim_scene::HudOverlay))
            .id();
        let tip = world
            .spawn((
                gaanim_animation::StrokeTip,
                gaanim_scene::prelude::ChildOf(path),
            ))
            .id();
        let mut sync = IntoSystem::into_system(sync_stroke_tip_layers_system);
        sync.initialize(&mut world);
        sync.run((), &mut world).unwrap();
        sync.apply_deferred(&mut world);
        assert_eq!(world.get::<ViewLayer>(tip), Some(&ViewLayer("xray".into())));
        assert!(world.get::<gaanim_scene::HudOverlay>(tip).is_some());

        world
            .entity_mut(path)
            .remove::<(ViewLayer, gaanim_scene::HudOverlay)>();
        sync.run((), &mut world).unwrap();
        sync.apply_deferred(&mut world);
        assert!(world.get::<ViewLayer>(tip).is_none());
        assert!(world.get::<gaanim_scene::HudOverlay>(tip).is_none());
    }
}

/// Motion blur of exported frames and snapshots: each frame averages
/// `samples` sub-frames spread over the time its shutter is open.
#[derive(bevy::prelude::Resource, Debug, Clone, Copy, PartialEq)]
pub struct MotionBlur {
    /// Fraction of a frame's duration the shutter stays open, times 360;
    /// 180 is the film standard.
    shutter_angle: f64,
    /// Sub-frames averaged per frame.
    samples: u32,
    /// Where the shutter opens relative to the frame time, in degrees of a
    /// frame; `-shutter_angle / 2` centers the blur on the frame.
    phase: f64,
}

/// Most sub-frames [`MotionBlur`] averages per frame.
pub const MAX_MOTION_BLUR_SAMPLES: u32 = 64;

impl MotionBlur {
    /// A shutter of `shutter_angle` degrees (0 to 720 exclusive of 0) sampled
    /// `samples` times (2 to [`MAX_MOTION_BLUR_SAMPLES`]). `phase` defaults
    /// to centering the shutter on the frame time.
    pub fn new(shutter_angle: f64, samples: u32, phase: Option<f64>) -> Result<Self, String> {
        if !(shutter_angle.is_finite() && shutter_angle > 0.0 && shutter_angle <= 720.0) {
            return Err(format!(
                "shutter_angle must be in (0, 720] degrees, got {shutter_angle}"
            ));
        }
        if !(2..=MAX_MOTION_BLUR_SAMPLES).contains(&samples) {
            return Err(format!(
                "samples must be between 2 and {MAX_MOTION_BLUR_SAMPLES}, got {samples}"
            ));
        }
        let phase = phase.unwrap_or(-shutter_angle / 2.0);
        if !(phase.is_finite() && phase.abs() <= 720.0) {
            return Err(format!("phase must be in [-720, 720] degrees, got {phase}"));
        }
        Ok(Self {
            shutter_angle,
            samples,
            phase,
        })
    }

    pub fn shutter_angle(&self) -> f64 {
        self.shutter_angle
    }

    pub fn samples(&self) -> u32 {
        self.samples
    }

    pub fn phase(&self) -> f64 {
        self.phase
    }

    /// Sub-frame times of the frame at `time` in a `fps` export, evenly
    /// spread over the open shutter and clamped to `[start, end]`, e.g. the
    /// segment that shows the frame.
    pub fn sample_times(&self, time: f64, fps: f64, start: f64, end: f64) -> Vec<f64> {
        let frame = 1.0 / fps.max(1.0);
        let open = time + self.phase / 360.0 * frame;
        let shutter = self.shutter_angle / 360.0 * frame;
        let samples = f64::from(self.samples);
        (0..self.samples)
            .map(|index| {
                let sample = open + (f64::from(index) + 0.5) / samples * shutter;
                sample.clamp(start, end.max(start))
            })
            .collect()
    }
}

/// Keeps a drawable sharp under [`MotionBlur`]: every sub-frame draws it as
/// it is at the frame's own time.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MotionBlurExempt;

#[cfg(test)]
mod motion_blur_tests {
    use super::*;

    #[test]
    fn sub_frames_center_on_the_frame_and_stay_in_bounds() {
        let blur = MotionBlur::new(180.0, 4, None).unwrap();
        let times = blur.sample_times(1.0, 10.0, 0.0, 10.0);
        // A 0.05 s shutter centered on 1.0.
        let expected = [0.98125, 0.99375, 1.00625, 1.01875];
        for (time, expected) in times.iter().zip(expected) {
            assert!((time - expected).abs() < 1e-12, "{times:?}");
        }
        let clamped = blur.sample_times(1.0, 10.0, 1.0, 1.01);
        assert_eq!(clamped.first(), Some(&1.0));
        assert_eq!(clamped.last(), Some(&1.01));
        let trailing = MotionBlur::new(360.0, 2, Some(0.0)).unwrap();
        assert_eq!(
            trailing.sample_times(0.0, 4.0, 0.0, 9.0),
            vec![0.0625, 0.1875]
        );
    }

    #[test]
    fn invalid_shutters_are_rejected() {
        assert!(MotionBlur::new(0.0, 8, None).is_err());
        assert!(MotionBlur::new(f64::NAN, 8, None).is_err());
        assert!(MotionBlur::new(900.0, 8, None).is_err());
        assert!(MotionBlur::new(180.0, 1, None).is_err());
        assert!(MotionBlur::new(180.0, MAX_MOTION_BLUR_SAMPLES + 1, None).is_err());
        assert!(MotionBlur::new(180.0, 8, Some(f64::INFINITY)).is_err());
    }
}
