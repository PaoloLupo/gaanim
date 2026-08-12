use gaanim_core::ObjectId;
use gaanim_core::glam::{DQuat, DVec3};
use gaanim_core::peniko::Color;
use gaanim_expr::Expr;
use gaanim_math::RateFunc;

use crate::canvas::CanvasEndpoint;

#[derive(Debug, Clone, Copy, Default)]
pub struct DrawAnimationConfig {
    pub stroke_width: Option<f64>,
    pub lag_ratio: Option<f64>,
    pub pen_tip: bool,
}

/// Typed targets collected by `DrawableHandle::animate()`.
///
/// Each populated channel is expanded into an ordinary timeline animation at
/// compile time, so compound animations retain deterministic seek behavior.
#[derive(Debug, Clone, Default)]
pub struct PropertyAnimation {
    pub translation: Option<PropertyTranslation>,
    pub rotation: Option<PropertyRotation>,
    pub scale: Option<PropertyScale>,
    pub opacity: Option<f32>,
    pub fill: Option<Color>,
    pub stroke_color: Option<Color>,
    pub stroke_width: Option<f64>,
    /// Manim-style color shortcut for currently visible vector paints.
    pub visible_color: Option<Color>,
    pub material: Option<(gaanim_scene::Material3D, gaanim_scene::Material3D)>,
}

impl PropertyAnimation {
    pub fn is_empty(&self) -> bool {
        self.translation.is_none()
            && self.rotation.is_none()
            && self.scale.is_none()
            && self.opacity.is_none()
            && self.fill.is_none()
            && self.stroke_color.is_none()
            && self.stroke_width.is_none()
            && self.visible_color.is_none()
            && self.material.is_none()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum PropertyTranslation {
    To(DVec3),
    By(DVec3),
}

#[derive(Debug, Clone, Copy)]
pub enum PropertyRotation {
    To(DQuat),
    By2D { radians: f64, pivot: Option<DVec3> },
    By3D(DQuat),
}

#[derive(Debug, Clone, Copy)]
pub enum PropertyScale {
    To(DVec3),
    Uniform(f64),
}

/// High-level, developer-friendly animation types that do not require explicitly defining
/// the initial "from" properties (resolved dynamically at timeline playback scheduling).
#[derive(Debug, Clone)]
pub enum AnimationType {
    /// Several typed property targets evaluated concurrently.
    Properties(PropertyAnimation),
    CameraPosition {
        to: DVec3,
    },
    CameraPositionSource {
        target: CanvasEndpoint,
    },
    CameraZoom {
        to: f64,
    },
    CameraZoomSource {
        to: Expr,
    },
    CameraRotation {
        to: DQuat,
    },
    CameraRotationSource {
        to: Expr,
    },
    CameraFrame {
        target: ObjectId,
        margin: f64,
    },
    CameraFrameMany {
        targets: Vec<ObjectId>,
        margins: [f64; 4],
        dynamic: bool,
    },
    CameraFollow {
        target: ObjectId,
    },
    CameraFollowEndpoint {
        target: CanvasEndpoint,
        offset: DVec3,
        offset_space: gaanim_animation::FollowOffsetSpace,
        lag: f64,
    },
    CameraShake {
        amplitude: f64,
        frequency: f64,
    },
    CameraLookAt {
        eye: DVec3,
        target: DVec3,
        up: DVec3,
    },
    CameraLookAtSource {
        eye: CanvasEndpoint,
        target: CanvasEndpoint,
        up: DVec3,
    },
    CameraOrbit {
        delta_yaw: f64,
        delta_pitch: f64,
    },
    CameraPerspective {
        fov_y: f64,
        near: f64,
        far: f64,
    },
    CameraOrthographic {
        zoom: f64,
    },
    CameraReset,
    CameraDolly {
        factor: f64,
    },
    /// A Blender Action embedded in an imported glTF model.
    GltfAnimation {
        animation_index: usize,
        source_duration: f64,
        speed: f64,
        looped: bool,
        reverse: bool,
        transition: f64,
        start_time: f64,
    },
    TranslateTo {
        to: DVec3,
    },
    TranslateBy {
        delta: DVec3,
    },
    RotateTo {
        to: DQuat,
    },
    RotateBy {
        angle_radians: f64,
        pivot: Option<DVec3>,
    },
    RotateBy3D {
        delta: DQuat,
    },
    ScaleTo {
        to: DVec3,
    },
    ScaleUniform {
        factor: f64,
    },
    FadeTo {
        to: f32,
    },
    FadeIn,
    FadeOut,
    /// Fade in while translating from an offset to the drawable's position.
    FadeInFrom {
        offset: DVec3,
    },
    FillColorTo {
        to: Color,
    },
    StrokeColorTo {
        to: Color,
    },
    StrokeWidthTo {
        to: f64,
    },
    Material3DTo {
        from: gaanim_scene::Material3D,
        to: gaanim_scene::Material3D,
    },
    /// Manim-style `Write`: progressively draw the target's path(s) along
    /// their arc length, then cross-fade the fill in once the outline is
    /// complete. This produces the characteristic "pen draws the object,
    /// then ink fills it" effect that Manim's `Write` is known for.
    ///
    /// Internally, each item gets a triplet of clips scheduled on the
    /// default track:
    ///
    /// 1. A `FillDrawProgress` hold (`0 -> 0`) spanning the draw phase.
    ///    This is the authoritative reset that guarantees the fill is
    ///    hidden from the very first frame of every Write — even if a
    ///    previous Write left the component at 1.0, the hold lens runs
    ///    at `item_start` and overwrites it to 0.0.
    /// 2. A `PathCompletion` (`0 -> 1`) over the draw phase. The lens
    ///    trims the cached `PathSource` along its arc length, producing
    ///    the progressive pen-stroke effect.
    /// 3. A `FillDrawProgress` cross-fade (`0 -> 1`) over the fade
    ///    phase, starting right after the draw completes. The renderer
    ///    reads this component and modulates the fill brush's color
    ///    alpha uniformly (works for Solid / Gradient / Image brushes).
    ///
    /// - If the target has children (text root, equation root, group),
    ///   each child gets its own staggered sub-clip triplet. The
    ///   per-item timeline math is
    ///   `item_duration = duration / (1 + (n - 1) * lag_ratio)` and the
    ///   lag step is `item_duration * lag_ratio`, so the next child
    ///   starts when the previous one is `lag_ratio` (default 0.5)
    ///   through its total duration.
    /// - If the target has no children, a single sub-clip triplet is
    ///   scheduled on the target itself.
    /// - The draw phase occupies ~70% of each item's duration; the
    ///   fill cross-fade occupies the remaining ~30%.
    /// - The `stroke_width` controls the outline thickness used during the
    ///   draw phase (defaults to the target's existing stroke width).
    Write {
        config: DrawAnimationConfig,
    },
    Create {
        config: DrawAnimationConfig,
    },
    /// Mesh-friendly creation: fade and grow from zero scale in parallel.
    Create3D,
    Uncreate {
        config: DrawAnimationConfig,
    },
    Unwrite {
        config: DrawAnimationConfig,
    },
    GrowFromCenter,
    ShrinkToCenter,
    SpinInFromNothing,
    Indicate {
        color: Option<Color>,
        scale_factor: f64,
    },
    /// Structured transition between complete Text objects.
    TextTransition {
        target: ObjectId,
        copy: bool,
        semantic_pairs: Vec<(String, Option<usize>, String, Option<usize>)>,
    },
    /// Deferred operation over selected glyphs without changing hierarchy.
    TextSelection {
        fragment: String,
        occurrence: Option<usize>,
        effect: TextSelectionEffect,
    },
    /// Morph or copy between two local selections.
    TextSelectionTransform {
        target: ObjectId,
        source_fragment: String,
        source_occurrence: Option<usize>,
        target_fragment: String,
        target_occurrence: Option<usize>,
        copy: bool,
    },
    /// Fade out the source and fade in the target concurrently.
    FadeTransform {
        target: ObjectId,
    },
    /// Morph the source into the target's visual state while preserving the
    /// source ObjectId, so later animations continue from the morphed result.
    Transform {
        target: ObjectId,
    },
    /// Morph the source into the target, then hide the source and reveal the
    /// actual target object at the end of the animation.
    ReplacementTransform {
        target: ObjectId,
    },
    /// Oscillating wiggle vibration (horizontal).
    Wiggle,
    /// Scale from 0 at a specific anchor point, growing to full size.
    GrowFromPoint {
        px: f64,
        py: f64,
    },
    /// Scale from 0 at a specific edge direction.
    GrowFromEdge {
        direction: String,
    },
    /// Draw the outline first (like Write) then fill in.
    DrawBorderThenFill {
        config: DrawAnimationConfig,
    },
    /// Lines radiating outward from a point (flash of insight effect).
    Flash {
        color: Option<Color>,
        n_lines: u32,
        radius: f64,
    },
    /// A rectangle/circle that appears around the target, grows, and fades.
    Circumscribe {
        color: Option<Color>,
    },
    /// Move the target's translation along a Bézier path. The path is
    /// sampled at the rate-function-eased `t` and the target's world
    /// translation is set to the sampled point. Rotation and scale
    /// are unaffected.
    MoveAlongPath {
        path: gaanim_core::kurbo::BezPath,
        path_target: Option<ObjectId>,
    },
    /// Specialized Create animation for `Arrow` mobjects. Draws the
    /// outline first, then finishes with a brief scale "punch" that
    /// emphasizes the arrowhead's appearance at the end.
    GrowArrow,
    /// Interpolate a float signal value to a target.
    SignalFloat {
        to: f64,
    },
    /// ShowPassingFlash progressively draws a sliding window of the path.
    ShowPassingFlash {
        time_width: f64,
    },
}

#[derive(Debug, Clone)]
pub enum TextSelectionEffect {
    Indicate,
    Pulse,
    Wiggle,
    Wave,
    Highlight,
    Focus,
    Cancel,
    RevealFade,
    RevealWipe,
    RevealFromBelow,
    ColorTo(Color),
    Brace { label: String, above: bool },
    Annotate { label: String, offset: DVec3 },
}

/// A fluent builder for an animation tween clip.
#[derive(Debug, Clone)]
pub struct AnimationBuilder {
    pub target: ObjectId,
    pub anim_type: AnimationType,
    pub duration: f64,
    pub delay: f64,
    pub rate_func: RateFunc,
}

impl AnimationBuilder {
    /// Sets the animation duration in seconds.
    pub fn duration(mut self, sec: f64) -> Self {
        self.duration = sec;
        self
    }

    /// Configures the pacing curve or rate function.
    pub fn rate_func(mut self, f: RateFunc) -> Self {
        self.rate_func = f;
        self
    }

    /// Sets ease curve to a fluid, premium physical Spring simulation.
    pub fn spring(self) -> Self {
        self.rate_func(RateFunc::Spring {
            stiffness: 90.0,
            damping: 12.0,
        })
    }

    /// Sets ease curve to standard cubic-bezier Double Smooth.
    pub fn smooth(self) -> Self {
        self.rate_func(RateFunc::Smooth)
    }

    /// Sets pacing to flat Linear.
    pub fn linear(self) -> Self {
        self.rate_func(RateFunc::Linear)
    }

    pub fn lag_ratio(mut self, lag_ratio: f64) -> Self {
        let lag_ratio = lag_ratio.max(0.0);
        match &mut self.anim_type {
            AnimationType::Write { config }
            | AnimationType::Create { config }
            | AnimationType::Uncreate { config }
            | AnimationType::Unwrite { config }
            | AnimationType::DrawBorderThenFill { config } => {
                config.lag_ratio = Some(lag_ratio);
            }
            _ => {}
        }
        self
    }

    pub fn stroke_width(mut self, stroke_width: f64) -> Self {
        let stroke_width = stroke_width.max(0.0);
        match &mut self.anim_type {
            AnimationType::Write { config }
            | AnimationType::Create { config }
            | AnimationType::Uncreate { config }
            | AnimationType::Unwrite { config }
            | AnimationType::DrawBorderThenFill { config } => {
                config.stroke_width = Some(stroke_width);
            }
            _ => {}
        }
        self
    }

    pub fn with_pen_tip(mut self) -> Self {
        match &mut self.anim_type {
            AnimationType::Write { config }
            | AnimationType::Create { config }
            | AnimationType::Uncreate { config }
            | AnimationType::Unwrite { config }
            | AnimationType::DrawBorderThenFill { config } => {
                config.pen_tip = true;
            }
            _ => {}
        }
        self
    }

    pub fn pivot(mut self, x: f64, y: f64) -> Self {
        if let AnimationType::RotateBy { ref mut pivot, .. } = self.anim_type {
            *pivot = Some(DVec3::new(x, y, 0.0));
        }
        self
    }

    pub fn about_point(self, x: f64, y: f64) -> Self {
        self.pivot(x, y)
    }
}

impl AnimationType {
    pub fn is_empty_properties(&self) -> bool {
        matches!(self, Self::Properties(properties) if properties.is_empty())
    }

    pub(crate) fn is_camera(&self) -> bool {
        matches!(
            self,
            Self::CameraPosition { .. }
                | Self::CameraPositionSource { .. }
                | Self::CameraZoom { .. }
                | Self::CameraZoomSource { .. }
                | Self::CameraRotation { .. }
                | Self::CameraRotationSource { .. }
                | Self::CameraFrame { .. }
                | Self::CameraFrameMany { .. }
                | Self::CameraFollow { .. }
                | Self::CameraFollowEndpoint { .. }
                | Self::CameraShake { .. }
                | Self::CameraLookAt { .. }
                | Self::CameraLookAtSource { .. }
                | Self::CameraOrbit { .. }
                | Self::CameraPerspective { .. }
                | Self::CameraOrthographic { .. }
                | Self::CameraReset
                | Self::CameraDolly { .. }
        )
    }

    pub fn default_rate_func(&self) -> RateFunc {
        match self {
            Self::CameraFollow { .. }
            | Self::CameraFollowEndpoint { .. }
            | Self::CameraShake { .. }
            | Self::GltfAnimation { .. }
            | Self::Write { .. }
            | Self::Create { .. }
            | Self::Unwrite { .. }
            | Self::Uncreate { .. }
            | Self::ShowPassingFlash { .. }
            | Self::Wiggle => RateFunc::Linear,
            Self::DrawBorderThenFill { .. } => RateFunc::DoubleSmooth,
            Self::Indicate { .. } | Self::Flash { .. } | Self::Circumscribe { .. } => {
                RateFunc::ThereAndBack
            }
            _ => RateFunc::Smooth,
        }
    }
}

use crate::builder::MobjectRef;

impl MobjectRef {
    pub fn translate_to(self, to: DVec3) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::TranslateTo { to },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
            delay: 0.0,
        }
    }

    pub fn translate_to_2d(self, x: f64, y: f64) -> AnimationBuilder {
        self.translate_to(DVec3::new(x, y, 0.0))
    }

    pub fn shift(self, delta: DVec3) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::TranslateBy { delta },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
            delay: 0.0,
        }
    }

    pub fn shift_2d(self, x: f64, y: f64) -> AnimationBuilder {
        self.shift(DVec3::new(x, y, 0.0))
    }

    pub fn rotate_to(self, to: DQuat) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::RotateTo { to },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
            delay: 0.0,
        }
    }

    pub fn rotate_to_2d(self, angle_radians: f64) -> AnimationBuilder {
        self.rotate_to(DQuat::from_rotation_z(angle_radians))
    }

    pub fn rotate_by(self, angle_radians: f64) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::RotateBy {
                angle_radians,
                pivot: None,
            },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
            delay: 0.0,
        }
    }

    pub fn scale_to(self, to: DVec3) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::ScaleTo { to },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
            delay: 0.0,
        }
    }

    pub fn scale_uniform(self, factor: f64) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::ScaleUniform { factor },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
            delay: 0.0,
        }
    }

    pub fn fade_to(self, to: f32) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::FadeTo { to },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
            delay: 0.0,
        }
    }

    pub fn fade_in(self) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::FadeIn,
            duration: 1.0,
            rate_func: RateFunc::Smooth,
            delay: 0.0,
        }
    }

    pub fn fade_out(self) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::FadeOut,
            duration: 1.0,
            rate_func: RateFunc::Smooth,
            delay: 0.0,
        }
    }

    pub fn fill_color_to(self, to: Color) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::FillColorTo { to },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
            delay: 0.0,
        }
    }

    pub fn stroke_color_to(self, to: Color) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::StrokeColorTo { to },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
            delay: 0.0,
        }
    }

    pub fn stroke_width_to(self, to: f64) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::StrokeWidthTo { to },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
            delay: 0.0,
        }
    }

    /// Manim-style **Write**: progressively draws the Mobject's path(s) along
    /// their arc length, then cross-fades from the outline to the final
    /// fill/stroke. If the Mobject has children (e.g. a text root or an
    /// equation), each child is drawn in a staggered sequence.
    pub fn write(self, duration: f64) -> AnimationBuilder {
        self.write_with_stroke_width(duration, None)
    }

    /// Same as [`write`](Self::write) but with an explicit outline stroke
    /// width used during the draw phase.
    pub fn write_with_stroke_width(
        self,
        duration: f64,
        stroke_width: Option<f64>,
    ) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::Write {
                config: DrawAnimationConfig {
                    stroke_width,
                    ..Default::default()
                },
            },
            duration,
            rate_func: AnimationType::Write {
                config: DrawAnimationConfig::default(),
            }
            .default_rate_func(),
            delay: 0.0,
        }
    }

    /// Manim-style **Create**: progressively draws the Mobject's path(s) along
    /// their arc length in parallel (without character/element stagger).
    pub fn create(self, duration: f64) -> AnimationBuilder {
        self.create_with_stroke_width(duration, None)
    }

    /// Same as [`create`](Self::create) but with an explicit outline stroke width.
    pub fn create_with_stroke_width(
        self,
        duration: f64,
        stroke_width: Option<f64>,
    ) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::Create {
                config: DrawAnimationConfig {
                    stroke_width,
                    ..Default::default()
                },
            },
            duration,
            rate_func: AnimationType::Create {
                config: DrawAnimationConfig::default(),
            }
            .default_rate_func(),
            delay: 0.0,
        }
    }

    /// Progressive erasure of the Mobject's path(s) and fill in parallel.
    pub fn uncreate(self, duration: f64) -> AnimationBuilder {
        self.uncreate_with_stroke_width(duration, None)
    }

    /// Same as [`uncreate`](Self::uncreate) but with an explicit outline stroke width.
    pub fn uncreate_with_stroke_width(
        self,
        duration: f64,
        stroke_width: Option<f64>,
    ) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::Uncreate {
                config: DrawAnimationConfig {
                    stroke_width,
                    ..Default::default()
                },
            },
            duration,
            rate_func: AnimationType::Uncreate {
                config: DrawAnimationConfig::default(),
            }
            .default_rate_func(),
            delay: 0.0,
        }
    }

    /// Staggered sequential erasure of the Mobject's path(s) and fill in reverse order (e.g. right-to-left).
    pub fn unwrite(self, duration: f64) -> AnimationBuilder {
        self.unwrite_with_stroke_width(duration, None)
    }

    /// Same as [`unwrite`](Self::unwrite) but with an explicit outline stroke width.
    pub fn unwrite_with_stroke_width(
        self,
        duration: f64,
        stroke_width: Option<f64>,
    ) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::Unwrite {
                config: DrawAnimationConfig {
                    stroke_width,
                    ..Default::default()
                },
            },
            duration,
            rate_func: AnimationType::Unwrite {
                config: DrawAnimationConfig::default(),
            }
            .default_rate_func(),
            delay: 0.0,
        }
    }

    /// Scale up from 0.0 to original size centered at current local position.
    pub fn grow_from_center(self) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::GrowFromCenter,
            duration: 1.0,
            rate_func: RateFunc::Smooth,
            delay: 0.0,
        }
    }

    /// Scale down from current size to 0.0 centered at current local position.
    pub fn shrink_to_center(self) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::ShrinkToCenter,
            duration: 1.0,
            rate_func: RateFunc::Smooth,
            delay: 0.0,
        }
    }

    /// Scale up from 0.0 and rotate 360 degrees concurrently.
    pub fn spin_in_from_nothing(self) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::SpinInFromNothing,
            duration: 1.0,
            rate_func: RateFunc::Smooth,
            delay: 0.0,
        }
    }

    /// Make a subtle upward hop, pulse around the visual center, and highlight with GOLD.
    pub fn indicate(self) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::Indicate {
                color: Some(Color::from_rgb8(255, 215, 0)),
                scale_factor: 1.1,
            },
            duration: 1.0,
            rate_func: RateFunc::ThereAndBack,
            delay: 0.0,
        }
    }

    /// Temporarily scale up and highlight with custom parameters before returning to baseline.
    pub fn indicate_with_color_and_scale(
        self,
        color: Option<Color>,
        scale_factor: f64,
    ) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::Indicate {
                color,
                scale_factor,
            },
            duration: 1.0,
            rate_func: RateFunc::ThereAndBack,
            delay: 0.0,
        }
    }

    pub fn fade_transform(self, target: ObjectId) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::FadeTransform { target },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
            delay: 0.0,
        }
    }

    pub fn wiggle(self) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::Wiggle,
            duration: 1.0,
            rate_func: RateFunc::Linear,
            delay: 0.0,
        }
    }

    pub fn grow_from_point(self, px: f64, py: f64) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::GrowFromPoint { px, py },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
            delay: 0.0,
        }
    }

    pub fn grow_from_edge(self, direction: &str) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::GrowFromEdge {
                direction: direction.to_string(),
            },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
            delay: 0.0,
        }
    }

    pub fn draw_border_then_fill(self) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::DrawBorderThenFill {
                config: DrawAnimationConfig::default(),
            },
            duration: 1.5,
            rate_func: AnimationType::DrawBorderThenFill {
                config: DrawAnimationConfig::default(),
            }
            .default_rate_func(),
            delay: 0.0,
        }
    }

    pub fn flash(self, color: Option<Color>, n_lines: u32, radius: f64) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::Flash {
                color,
                n_lines,
                radius,
            },
            duration: 1.0,
            rate_func: RateFunc::ThereAndBack,
            delay: 0.0,
        }
    }

    pub fn circumscribe(self, color: Option<Color>) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::Circumscribe { color },
            duration: 1.5,
            rate_func: RateFunc::ThereAndBack,
            delay: 0.0,
        }
    }

    /// Move the target along a Bézier path. The path is sampled at the
    /// rate-function-eased `t` (use `.linear()` for uniform parametric
    /// motion, or a smooth easing for an accelerated start/end).
    pub fn move_along_path(self, path: gaanim_core::kurbo::BezPath) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::MoveAlongPath {
                path,
                path_target: None,
            },
            duration: 2.0,
            rate_func: RateFunc::Linear,
            delay: 0.0,
        }
    }

    /// Specialized arrow draw: traces the outline and finishes with a
    /// scale "punch" that emphasizes the arrowhead's arrival.
    pub fn grow_arrow(self) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::GrowArrow,
            duration: 1.5,
            rate_func: RateFunc::Smooth,
            delay: 0.0,
        }
    }

    /// Manim-style ShowPassingFlash: progressively draws a sliding window of the path
    /// along its arc length.
    pub fn show_passing_flash(self, duration: f64, time_width: f64) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::ShowPassingFlash { time_width },
            duration,
            rate_func: RateFunc::Linear,
            delay: 0.0,
        }
    }
}

/// Handle reference to a ValueTracker signal in the Scene.
#[derive(Clone, Copy, Debug)]
pub struct ValueTrackerRef {
    pub id: ObjectId,
}

impl ValueTrackerRef {
    /// Animate this ValueTracker's float value to a target value.
    pub fn animate_to(self, to: f64) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::SignalFloat { to },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
            delay: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draw_animation_defaults_match_manim_style_rate_funcs() {
        assert!(matches!(
            AnimationType::Write {
                config: DrawAnimationConfig::default()
            }
            .default_rate_func(),
            RateFunc::Linear
        ));
        assert!(matches!(
            AnimationType::Create {
                config: DrawAnimationConfig::default()
            }
            .default_rate_func(),
            RateFunc::Linear
        ));
        assert!(matches!(
            AnimationType::DrawBorderThenFill {
                config: DrawAnimationConfig::default()
            }
            .default_rate_func(),
            RateFunc::DoubleSmooth
        ));
    }

    #[test]
    fn animation_builder_updates_draw_config_fields() {
        let builder = AnimationBuilder {
            target: ObjectId::from_parts(1, 1),
            anim_type: AnimationType::Write {
                config: DrawAnimationConfig::default(),
            },
            duration: 1.0,
            delay: 0.0,
            rate_func: RateFunc::Linear,
        }
        .lag_ratio(0.15)
        .stroke_width(3.5)
        .with_pen_tip();

        let AnimationType::Write { config } = builder.anim_type else {
            panic!("expected write animation");
        };
        assert_eq!(config.lag_ratio, Some(0.15));
        assert_eq!(config.stroke_width, Some(3.5));
        assert!(config.pen_tip);
    }
}
