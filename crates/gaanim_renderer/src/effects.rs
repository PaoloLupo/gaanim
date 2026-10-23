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
/// `Inside` draws the whole authored width within the contour, `Center`
/// half on each side and `Outside` all of it beyond, e.g. for a text halo.
/// Open paths always center their stroke.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum StrokeAlign {
    #[default]
    Inside,
    Center,
    Outside,
}

/// Component: the stroke pen follows the scale the drawable accumulates from
/// itself, its groups and coordinate views, like a stroke drawn on paper
/// that is then enlarged. Without it a stroke keeps its width in scene units.
/// A non-uniform scale widens the pen along its axis.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StrokeScalesWithObject;

/// Component: draw the fill and stroke as chalk on a blackboard: the outline
/// trembles by up to `roughness` scene units and the paint is broken by a
/// grain. Both follow `seed`, so every frame and export draws the same chalk.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct ChalkBrush {
    pub seed: u64,
    pub roughness: f64,
}

impl Default for ChalkBrush {
    fn default() -> Self {
        Self {
            seed: 0,
            roughness: 0.01,
        }
    }
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

impl ClipMask {
    /// The contour that keeps what the target draws, and whether it keeps
    /// what lies outside it instead: an inverted mask's own contour, without
    /// the rectangle `resolve_dynamic_clip_masks_system` encloses it in.
    pub fn kept_contour(&self) -> (gaanim_core::kurbo::BezPath, bool) {
        if !self.invert || self.sources.is_empty() {
            return (self.path.clone(), self.invert);
        }
        // The enclosing rectangle comes first: a move, three lines and a close.
        let elements = self.path.elements();
        let own = elements.get(5..).unwrap_or_default();
        (gaanim_core::kurbo::BezPath::from_vec(own.to_vec()), true)
    }
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
    /// Timeline time the view appears at, the cursor where it was set.
    pub since: f64,
    /// Timeline time `no_camera_view` removed it at, if it did.
    pub until: Option<f64>,
    /// A 3D camera of the view's own, which draws the scene's 3D content
    /// from where it stands instead of magnifying the canvas.
    pub camera_3d: Option<ViewCamera3D>,
}

/// The 3D camera of a camera view that looks at the scene's 3D content from
/// a place of its own: its eye and target, which may move, its up direction
/// and its vertical field of view in radians. The view's screen shows the
/// 3D content through it as the main camera would show it on a frame the
/// screen's shape, with the main camera's lens limits; the canvas's 2D
/// drawables stay out of it.
#[derive(Debug, Clone)]
pub struct ViewCamera3D {
    pub eye: gaanim_animation::TrackingEndpoint,
    pub target: gaanim_animation::TrackingEndpoint,
    pub up: gaanim_core::glam::DVec3,
    pub fov_y: f64,
}

/// Component: where the [`ViewCamera3D`] of a camera view's screen stands
/// this frame, resolved by [`resolve_view_cameras_system`].
#[derive(Component, Debug, Clone, Copy)]
pub struct ViewCamera(pub gaanim_math::Camera);

/// System: resolve the eye and target of the 3D cameras of camera views
/// where they are this frame, into a perspective camera on a frame as tall
/// as the main camera's and shaped like each screen.
pub fn resolve_view_cameras_system(world: &mut bevy::prelude::World) {
    let mut screens = world.query::<(
        Entity,
        &CameraView,
        &gaanim_scene::LocalBounds,
        Option<&gaanim_scene::PathSource>,
    )>();
    let views: Vec<(Entity, ViewCamera3D, gaanim_core::kurbo::Rect)> = screens
        .iter(world)
        .filter_map(|(entity, view, bounds, source)| {
            Some((
                entity,
                view.camera_3d.clone()?,
                crate::pipeline::geometry_rect(bounds, source),
            ))
        })
        .collect();
    if views.is_empty() {
        return;
    }
    let Some(main) = world
        .get_resource::<gaanim_math::ResolvedCamera>()
        .map(|resolved| resolved.camera)
        .or_else(|| world.get_resource::<gaanim_math::Camera>().copied())
    else {
        return;
    };
    let (near, far) = match main.projection {
        gaanim_math::Projection::Perspective { near, far, .. } => (near, far),
        gaanim_math::Projection::Orthographic { .. } => (0.1, 1000.0),
    };
    let mut resolved = Vec::with_capacity(views.len());
    for (entity, camera_3d, rect) in views {
        let (Some(eye), Some(target)) = (
            gaanim_animation::resolve_tracking_endpoint(&camera_3d.eye, world),
            gaanim_animation::resolve_tracking_endpoint(&camera_3d.target, world),
        ) else {
            continue;
        };
        if !(rect.width() > 0.0 && rect.height() > 0.0) {
            continue;
        }
        let aspect = rect.width() / rect.height();
        let height = main.frame_height;
        let pixels = main.viewport_height.max(1);
        let mut camera = gaanim_math::Camera::ortho_2d_frame(
            height * aspect,
            height,
            ((f64::from(pixels) * aspect).round() as u32).max(1),
            pixels,
        );
        if camera.set_perspective(camera_3d.fov_y, near, far).is_err()
            || camera.look_at(eye, target, camera_3d.up).is_err()
        {
            continue;
        }
        resolved.push((entity, camera));
    }
    for (entity, camera) in resolved {
        world.entity_mut(entity).insert(ViewCamera(camera));
    }
}

impl CameraView {
    /// Whether the screen shows the view at timeline time `time`; outside
    /// that window it draws as a plain drawable.
    pub fn shown_at(&self, time: f64) -> bool {
        time + 1e-9 >= self.since && self.until.is_none_or(|until| time + 1e-9 < until)
    }
}

/// Component: turns a closed vector leaf into a snapshot of a [`CameraView`]:
/// it shows, frozen, what the view of `screen` showed at timeline time `at`,
/// in its own outline and wherever it moves. The scene builder freezes it
/// into [`crate::pipeline::FrozenViews`] under `key`; until then, or when
/// the view showed nothing, the drawable draws as a plain shape.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct CameraSnapshot {
    /// The screen whose view the snapshot freezes.
    pub screen: Entity,
    pub at: f64,
    /// The authored drawable, the same in every world built from one scene.
    pub key: gaanim_core::ObjectId,
}

/// System: the frame of a camera view that follows 3D content stays where
/// the main camera draws that content on the canvas, so that the view shows
/// it. The canvas a camera at an angle or in perspective draws 2D content
/// on is not where it draws a 3D point, so a frame following a mesh, a 3D
/// line, a model, one of its nodes or a billboard moves from the point
/// `follow` put it at to the canvas point drawn over it. The lines of an
/// inset follow it there.
pub fn project_camera_view_follows_system(world: &mut bevy::prelude::World) {
    use gaanim_animation::{EndpointFollow, TrackingEndpoint};
    let Some(camera) = world
        .get_resource::<gaanim_math::ResolvedCamera>()
        .map(|resolved| resolved.camera)
    else {
        return;
    };
    // A flat camera draws a point where its x and y put it.
    if camera.is_flat() {
        return;
    }
    let time = world
        .get_resource::<gaanim_animation::PlaybackState>()
        .map_or(0.0, |state| state.current_time);
    let frames: Vec<Entity> = world
        .query::<&CameraView>()
        .iter(world)
        .map(|view| view.source)
        .collect();
    let mut placed = Vec::new();
    for frame in frames {
        let Some(follow) = world
            .get::<EndpointFollow>(frame)
            .filter(|follow| follow.holds_at(time))
        else {
            continue;
        };
        let target = match &follow.endpoint {
            TrackingEndpoint::Entity(entity) | TrackingEndpoint::EntityAnchor { entity, .. } => {
                gaanim_animation::stand_in_at(*entity, time, world)
            }
            _ => continue,
        };
        if !drawn_through_camera(target, world) {
            continue;
        }
        let Some(point) = gaanim_animation::resolve_tracking_endpoint_with_offset(
            &follow.endpoint,
            follow.offset,
            follow.offset_space,
            world,
        ) else {
            continue;
        };
        let at = camera.canvas_point(point).extend(0.0);
        let local = world
            .get::<gaanim_scene::prelude::ChildOf>(frame)
            .and_then(|relation| gaanim_animation::entity_world_matrix(relation.parent(), world))
            .filter(|matrix| matrix.determinant().abs() > f64::EPSILON)
            .map_or(at, |matrix| matrix.inverse().transform_point3(at));
        placed.push((frame, local));
    }
    if placed.is_empty() {
        return;
    }
    for (frame, local) in placed {
        if let Some(mut transform) = world.get_mut::<gaanim_math::SpatialTransform>(frame) {
            transform.translation = local;
        }
    }
    gaanim_animation::tracking_line_system(world);
}

/// Whether the main camera draws `entity` through its projection: no part
/// of it is a 2D drawable of the canvas, or it is a billboard, which faces
/// the camera where its 3D point is drawn.
fn drawn_through_camera(entity: Entity, world: &bevy::prelude::World) -> bool {
    if world.get::<gaanim_scene::Billboard>(entity).is_some() {
        return true;
    }
    let mut pending = vec![entity];
    while let Some(node) = pending.pop() {
        let flat = world.get::<gaanim_scene::Path2D>(node).is_some()
            || world.get::<gaanim_scene::RasterImage>(node).is_some()
            || world.get::<crate::lottie::LottiePlayer>(node).is_some();
        if flat && world.get::<gaanim_scene::DrawnIn3D>(node).is_none() {
            return false;
        }
        if let Some(children) = world.get::<bevy::prelude::Children>(node) {
            pending.extend(children.iter());
        }
    }
    true
}

/// Component: puts a drawable on a named view layer. The main camera does not
/// draw it; only camera views that list the layer show it.
#[derive(Component, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ViewLayer(pub std::sync::Arc<str>);

/// System: the tips of a stroked path (see [`gaanim_animation::StrokeTips`])
/// draw in their path's view layer and, for HUD paths, over the screen.
#[allow(clippy::type_complexity)]
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

/// Time ranges with their own [`MotionBlur`], such as a whip pan's: a frame
/// inside one is blurred even when the scene has no motion blur of its own.
#[derive(bevy::prelude::Resource, Debug, Clone, PartialEq, Default)]
pub struct MotionBlurWindows(pub Vec<(f64, f64, MotionBlur)>);

impl MotionBlurWindows {
    /// The blur of the first window that holds `time`.
    pub fn at(&self, time: f64) -> Option<MotionBlur> {
        self.0
            .iter()
            .find(|(start, end, _)| *start - 1e-9 <= time && time <= *end + 1e-9)
            .map(|(_, _, blur)| *blur)
    }
}

/// Keeps a drawable sharp under [`MotionBlur`]: every sub-frame draws it as
/// it is at the frame's own time.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MotionBlurExempt;

/// Leaves a drawable out of the scene post-process: it draws above the
/// processed frame, such as a label that pixelation would make unreadable.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PostProcessExempt;

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

/// Width of a stroke along its visible path: `(position, factor)` pairs with
/// positions in `[0, 1]` of the arc length, sorted, and factors that scale
/// the stroke width, interpolated linearly. The stroke is drawn as a filled
/// outline instead of a pen, so dashes do not apply.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct StrokeProfile(pub std::sync::Arc<[(f64, f64)]>);

impl StrokeProfile {
    /// Width factor at `position` along the path.
    pub fn factor_at(&self, position: f64) -> f64 {
        let points = &self.0;
        let Some(&(first_at, first)) = points.first() else {
            return 1.0;
        };
        if position <= first_at {
            return first;
        }
        for pair in points.windows(2) {
            let ((a, wa), (b, wb)) = (pair[0], pair[1]);
            if position <= b {
                let span = b - a;
                return if span <= f64::EPSILON {
                    wb
                } else {
                    wa + (wb - wa) * (position - a) / span
                };
            }
        }
        points.last().map_or(1.0, |&(_, last)| last)
    }
}

#[cfg(test)]
mod stroke_profile_tests {
    use super::*;

    #[test]
    fn factors_interpolate_between_sorted_points() {
        let profile = StrokeProfile(vec![(0.0, 0.0), (0.5, 1.0), (1.0, 0.25)].into());
        assert_eq!(profile.factor_at(-1.0), 0.0);
        assert_eq!(profile.factor_at(0.25), 0.5);
        assert_eq!(profile.factor_at(0.75), 0.625);
        assert_eq!(profile.factor_at(2.0), 0.25);
        assert_eq!(StrokeProfile(Vec::new().into()).factor_at(0.3), 1.0);
    }
}

/// Which pairs of points a [`ConnectBinding`] joins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConnectMode {
    /// Every pair closer than the maximum distance.
    Range,
    /// Each point to its `neighbors` nearest points within the distance.
    Nearest,
    /// Each point to the next one in order, within the distance.
    Sequential,
}

/// Live plexus lines between the positions of `sources`, rebuilt in
/// `SceneSet::DerivedGeometry` as they move.
///
/// A connection draws in the binding whose `layer` covers its length: the
/// lengths up to `max_distance` are split into `layers` equal ranges, so
/// layers with decreasing opacity fade long links out.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct ConnectBinding {
    pub sources: Vec<Entity>,
    pub max_distance: f64,
    pub mode: ConnectMode,
    pub neighbors: usize,
    pub layer: usize,
    pub layers: usize,
}

impl ConnectBinding {
    /// Index pairs to join among `points`, in a deterministic order, with
    /// their lengths.
    pub fn connections(
        mode: ConnectMode,
        points: &[Option<gaanim_core::kurbo::Point>],
        max_distance: f64,
        neighbors: usize,
    ) -> Vec<(usize, usize, f64)> {
        let distance = |a: usize, b: usize| Some(points[a]?.distance(points[b]?));
        let within = |length: f64| length <= max_distance;
        let mut links = Vec::new();
        match mode {
            ConnectMode::Range => {
                for a in 0..points.len() {
                    for b in a + 1..points.len() {
                        if let Some(length) = distance(a, b).filter(|length| within(*length)) {
                            links.push((a, b, length));
                        }
                    }
                }
            }
            ConnectMode::Sequential => {
                for a in 0..points.len().saturating_sub(1) {
                    if let Some(length) = distance(a, a + 1).filter(|length| within(*length)) {
                        links.push((a, a + 1, length));
                    }
                }
            }
            ConnectMode::Nearest => {
                let mut chosen = std::collections::BTreeMap::new();
                for a in 0..points.len() {
                    let mut candidates: Vec<(f64, usize)> = (0..points.len())
                        .filter(|&b| b != a)
                        .filter_map(|b| Some((distance(a, b)?, b)))
                        .filter(|(length, _)| within(*length))
                        .collect();
                    candidates.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
                    for (length, b) in candidates.into_iter().take(neighbors) {
                        chosen.insert((a.min(b), a.max(b)), length);
                    }
                }
                links.extend(chosen.into_iter().map(|((a, b), length)| (a, b, length)));
            }
        }
        links
    }

    /// Whether a connection of `length` draws in this binding's layer.
    pub fn owns(&self, length: f64) -> bool {
        let layers = self.layers.max(1);
        let share = if self.max_distance > 0.0 && self.max_distance.is_finite() {
            (length / self.max_distance).clamp(0.0, 1.0)
        } else {
            0.0
        };
        ((share * layers as f64) as usize).min(layers - 1) == self.layer
    }
}

#[cfg(test)]
mod connect_tests {
    use super::*;
    use gaanim_core::kurbo::Point;

    #[test]
    fn modes_join_deterministic_pairs() {
        let points = [
            Some(Point::new(0.0, 0.0)),
            Some(Point::new(1.0, 0.0)),
            Some(Point::new(3.0, 0.0)),
            None,
            Some(Point::new(1.0, 1.5)),
        ];
        let pairs = |links: Vec<(usize, usize, f64)>| {
            links
                .into_iter()
                .map(|(a, b, _)| (a, b))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            pairs(ConnectBinding::connections(
                ConnectMode::Range,
                &points,
                2.0,
                1
            )),
            [(0, 1), (0, 4), (1, 2), (1, 4)]
        );
        assert_eq!(
            pairs(ConnectBinding::connections(
                ConnectMode::Sequential,
                &points,
                2.0,
                1
            )),
            [(0, 1), (1, 2)]
        );
        assert_eq!(
            pairs(ConnectBinding::connections(
                ConnectMode::Nearest,
                &points,
                10.0,
                1
            )),
            [(0, 1), (1, 2), (1, 4)]
        );
        let layer = |layer| ConnectBinding {
            sources: Vec::new(),
            max_distance: 2.0,
            mode: ConnectMode::Range,
            neighbors: 1,
            layer,
            layers: 4,
        };
        assert!(layer(0).owns(0.1) && layer(1).owns(0.6) && layer(3).owns(2.0));
        assert!(!layer(0).owns(1.9));
    }
}

#[cfg(test)]
mod camera_view_follow_tests {
    use super::*;
    use gaanim_core::glam::DVec3;
    use gaanim_math::SpatialTransform;

    /// A screen whose view looks through a frame following `target`.
    fn following_frame(world: &mut bevy::prelude::World, target: Entity) -> Entity {
        let frame = world
            .spawn((
                SpatialTransform::default(),
                gaanim_animation::EndpointFollow {
                    endpoint: gaanim_animation::TrackingEndpoint::Entity(target),
                    offset: DVec3::ZERO,
                    offset_space: gaanim_animation::FollowOffsetSpace::World,
                    since: 0.0,
                },
            ))
            .id();
        world.spawn(CameraView {
            source: frame,
            fit: CameraViewFit::Contain,
            background: CameraViewBackground::None,
            exclude: Vec::new(),
            zoom: None,
            layers: Vec::new(),
            since: 0.0,
            until: None,
            camera_3d: None,
        });
        frame
    }

    #[test]
    fn view_frames_follow_3d_content_where_the_camera_draws_it() {
        let mut world = bevy::prelude::World::new();
        let mut camera = gaanim_math::Camera::ortho_2d_frame(16.0, 9.0, 1280, 720);
        camera
            .look_at(DVec3::new(0.0, 4.0, 10.0), DVec3::ZERO, DVec3::Y)
            .unwrap();
        camera.set_perspective(0.8, 0.1, 100.0).unwrap();
        world.insert_resource(gaanim_math::ResolvedCamera::new(
            camera,
            gaanim_math::CameraViewport::default(),
        ));
        let point = DVec3::new(2.0, 1.0, -3.0);
        let mesh = world
            .spawn((
                SpatialTransform::new_3d(point.x, point.y, point.z),
                gaanim_scene::Mesh3DMarker,
            ))
            .id();
        let shape = world
            .spawn((
                SpatialTransform::new_3d(point.x, point.y, point.z),
                gaanim_scene::Path2D(std::sync::Arc::new(
                    gaanim_core::kurbo::BezPath::from_svg("M0 0 L1 0 L1 1 Z").unwrap(),
                )),
            ))
            .id();
        let on_mesh = following_frame(&mut world, mesh);
        let on_shape = following_frame(&mut world, shape);
        project_camera_view_follows_system(&mut world);

        // Drawn on the still canvas of the perspective camera, over the mesh.
        let placed = world.get::<SpatialTransform>(on_mesh).unwrap().translation;
        let scale = camera.pixels_per_unit();
        let drawn = gaanim_core::kurbo::Affine::translate((640.0, 360.0))
            * gaanim_core::kurbo::Affine::scale_non_uniform(scale, -scale)
            * gaanim_core::kurbo::Point::new(placed.x, placed.y);
        let screen = camera.world_to_screen(point);
        assert!(
            (drawn.x - screen.x).abs() < 1e-6 && (drawn.y - screen.y).abs() < 1e-6,
            "{drawn:?} {screen:?}"
        );
        assert_eq!(placed.z, 0.0);
        // A 2D drawable stays on the canvas where its x and y put it, so
        // the frame `follow` placed is left alone.
        assert_eq!(
            world.get::<SpatialTransform>(on_shape).unwrap().translation,
            DVec3::ZERO
        );

        // A flat camera leaves every frame where `follow` put it.
        world.insert_resource(gaanim_math::ResolvedCamera::default());
        world
            .get_mut::<SpatialTransform>(on_mesh)
            .unwrap()
            .translation = DVec3::ZERO;
        project_camera_view_follows_system(&mut world);
        assert_eq!(
            world.get::<SpatialTransform>(on_mesh).unwrap().translation,
            DVec3::ZERO
        );
    }
}
