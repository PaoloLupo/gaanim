//! Camera views: closed shapes that show what a second camera sees.
//!
//! A view is a screen drawable plus a frame drawable that plays the camera:
//! the frame's center and rotation set where the camera looks, and either its
//! size or an explicit zoom sets the magnification. Both are ordinary
//! drawables, so the timeline, bindings and seeking drive views unchanged.

use std::sync::{Arc, Mutex};

use gaanim_animation::{FollowOffsetSpace, ReactiveFunction, ReactiveInput, ScalarSource};
use gaanim_core::ObjectId;
use gaanim_core::glam::{DVec2, DVec3};
use gaanim_core::peniko::Color;
use gaanim_layout::{Anchor, Direction};
use gaanim_renderer::effects::{CameraViewBackground, CameraViewFit};

use super::canvas_impl::spawn_in;
use super::ops::{AnchorPoint, CameraViewSpec, CanvasEndpoint, Op};
use super::types::{Anim, SpawnKind};
use super::visualization::{Parameter, parameter_in};
use super::{DrawableHandle, SceneModel};
use crate::anim::{AnimationBuilder, AnimationType};

/// Magnification of a camera view.
#[derive(Debug, Clone)]
pub enum CameraViewZoom {
    /// A starting zoom owned by the view. Zooming animates it at a constant
    /// perceived speed.
    Value(f64),
    /// A parameter that holds the zoom; zooming animates it.
    Parameter(Parameter),
    /// A computed zoom the view follows but cannot set.
    Source(ScalarSource),
}

/// Options for a camera view shown inside a drawable.
#[derive(Debug, Clone, Default)]
pub struct CameraViewOptions {
    /// How the frame's region fills the screen when the frame's size sets the
    /// zoom.
    pub fit: CameraViewFit,
    /// Paint behind the view's content, inside the screen.
    pub background: CameraViewBackground,
    /// Drawables (with their subtrees) the view leaves out.
    pub exclude: Vec<DrawableHandle>,
    /// View layers the view shows besides the drawables on no layer.
    pub layers: Vec<String>,
    /// Explicit magnification; `None` derives it from the frame's size.
    pub zoom: Option<CameraViewZoom>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CameraViewError {
    #[error("camera view drawables and zoom sources must belong to the same Scene")]
    ForeignScene,
    #[error(
        "a camera view screen must be a single closed 2D shape: a rect, square, circle, dot, ellipse, polygon, star, sector, annulus, curve, SVG path or boolean"
    )]
    UnsupportedScreen,
    #[error("a drawable cannot frame its own camera view")]
    OwnSource,
    #[error("camera view zoom must be finite and greater than zero")]
    InvalidZoom,
    #[error("view layer names must not be empty")]
    InvalidLayer,
    #[error("this camera view follows a computed zoom and cannot set it")]
    ComputedZoom,
    #[error("camera inset size must be finite and greater than zero")]
    InvalidSize,
    #[error(
        "a camera inset can only be placed at a point, a drawable or an anchor point; pass follow=True for other endpoints"
    )]
    UnsupportedTarget,
}

/// Whether a drawable of `kind` is one closed 2D vector leaf that can show a
/// camera view.
pub(crate) fn is_camera_view_screen(kind: &SpawnKind) -> bool {
    matches!(
        kind,
        SpawnKind::Circle(_)
            | SpawnKind::Rect(..)
            | SpawnKind::RoundedRect(..)
            | SpawnKind::SurroundingRect
            | SpawnKind::Square(_)
            | SpawnKind::Dot(_)
            | SpawnKind::Ellipse(..)
            | SpawnKind::Polygon(_)
            | SpawnKind::Star { .. }
            | SpawnKind::RegularPolygon { .. }
            | SpawnKind::Sector { .. }
            | SpawnKind::Annulus { .. }
            | SpawnKind::Bezier { .. }
            | SpawnKind::Curve(_)
            | SpawnKind::SvgPath(_)
            | SpawnKind::Boolean { .. }
    )
}

/// How a view's magnification is controlled.
#[derive(Debug, Clone)]
enum ViewZoom {
    /// The frame's size sets it.
    Frame,
    /// A parameter owned by the view that holds the natural logarithm of the
    /// zoom, so linear animation zooms at a constant perceived speed.
    Owned(Parameter),
    /// A user parameter that holds the zoom.
    Parameter(Parameter),
    /// A computed zoom.
    Source(ScalarSource),
}

fn scene_id(handle: &DrawableHandle) -> u64 {
    handle.state.lock().expect("canvas state poisoned").scene_id
}

/// `map` applied to the value of `parameter`, described by `recipe`.
fn parameter_function(parameter: &Parameter, recipe: &str, map: fn(f64) -> f64) -> ScalarSource {
    ScalarSource::Function(
        ReactiveFunction::new(
            0,
            1,
            vec![ReactiveInput::Signal(parameter.drawable().id)],
            move |values| Ok(vec![map(values[0])]),
        )
        .with_recipe(recipe)
        .with_scene_owner(scene_id(parameter.drawable())),
    )
}

impl ViewZoom {
    /// The zoom as a reactive source; `None` when the frame's size sets it.
    fn source(&self) -> Option<ScalarSource> {
        match self {
            Self::Frame => None,
            Self::Owned(log) => Some(parameter_function(log, "camera view zoom", f64::exp)),
            Self::Parameter(parameter) => Some(parameter.source()),
            Self::Source(source) => Some(source.clone()),
        }
    }

    /// The reciprocal zoom, which scales a frame sized like its screen to the
    /// region the screen shows.
    fn inverse(&self) -> Option<ScalarSource> {
        match self {
            Self::Frame => None,
            Self::Owned(log) => Some(parameter_function(log, "camera view frame scale", |log| {
                (-log).exp()
            })),
            Self::Parameter(parameter) => Some(parameter_function(
                parameter,
                "camera view frame scale",
                f64::recip,
            )),
            Self::Source(source) => Some(ScalarSource::Function(ReactiveFunction::from_sources(
                0,
                1,
                vec![source.clone()],
                |values| Ok(vec![values[0].recip()]),
            ))),
        }
    }

    /// The parameter holding the zoom and whether it holds its logarithm.
    fn signal(&self) -> Option<(ObjectId, bool)> {
        match self {
            Self::Owned(log) => Some((log.drawable().id, true)),
            Self::Parameter(parameter) => Some((parameter.drawable().id, false)),
            Self::Frame | Self::Source(_) => None,
        }
    }
}

/// Anchor a following frame sits on: object, normalized bounds point and
/// local offset.
type ViewFocus = (ObjectId, DVec3, DVec3);

/// A second camera shown inside a screen drawable.
#[derive(Debug, Clone)]
pub struct CameraViewHandle {
    screen: DrawableHandle,
    frame: DrawableHandle,
    zoom: ViewZoom,
    fit: CameraViewFit,
    connectors: Vec<DrawableHandle>,
    /// Shared by every clone, so animation proxies see a later `follow`.
    focus: Arc<Mutex<Option<ViewFocus>>>,
}

impl CameraViewHandle {
    /// The drawable that shows the view.
    pub fn screen(&self) -> &DrawableHandle {
        &self.screen
    }

    /// The drawable that plays the camera.
    pub fn frame(&self) -> &DrawableHandle {
        &self.frame
    }

    /// Lines joining the frame to the screen, created by
    /// [`SceneModel::camera_inset`].
    pub fn connectors(&self) -> &[DrawableHandle] {
        &self.connectors
    }

    /// The zoom as a reactive source, or `None` when the frame's size sets it.
    pub fn zoom_source(&self) -> Option<ScalarSource> {
        self.zoom.source()
    }

    /// The user parameter that holds the zoom, if one was given.
    pub fn zoom_parameter(&self) -> Option<&Parameter> {
        match &self.zoom {
            ViewZoom::Parameter(parameter) => Some(parameter),
            _ => None,
        }
    }

    /// The drawable of the parameter behind the zoom, which owns it.
    pub fn zoom_owner(&self) -> Option<&DrawableHandle> {
        match &self.zoom {
            ViewZoom::Owned(parameter) | ViewZoom::Parameter(parameter) => {
                Some(parameter.drawable())
            }
            ViewZoom::Frame | ViewZoom::Source(_) => None,
        }
    }

    /// Set the zoom at the current cursor.
    pub fn zoom_to(&self, zoom: f64) -> Result<(), CameraViewError> {
        if !zoom.is_finite() || zoom <= 0.0 {
            return Err(CameraViewError::InvalidZoom);
        }
        match &self.zoom {
            ViewZoom::Frame => {
                let anim_type = AnimationType::CameraViewZoomTo {
                    screen: self.screen.id,
                    zoom,
                    fit: self.fit,
                };
                let rate_func = anim_type.default_rate_func();
                self.frame
                    .state
                    .lock()
                    .expect("canvas state poisoned")
                    .push_immediate(AnimationBuilder {
                        target: self.frame.id,
                        anim_type,
                        duration: 0.0,
                        delay: 0.0,
                        rate_func,
                    });
            }
            ViewZoom::Owned(log) => log
                .set(zoom.ln())
                .map_err(|_| CameraViewError::InvalidZoom)?,
            ViewZoom::Parameter(parameter) => parameter
                .set(zoom)
                .map_err(|_| CameraViewError::InvalidZoom)?,
            ViewZoom::Source(_) => return Err(CameraViewError::ComputedZoom),
        }
        Ok(())
    }

    /// Animate the zoom. A zoom owned by the view changes at a constant
    /// perceived speed; a zoom parameter animates linearly.
    pub fn animate_zoom_to(&self, zoom: f64) -> Result<Anim, CameraViewError> {
        if !zoom.is_finite() || zoom <= 0.0 {
            return Err(CameraViewError::InvalidZoom);
        }
        match &self.zoom {
            ViewZoom::Frame => {
                Ok(self
                    .frame
                    .animate()
                    .camera_view_zoom_to(self.screen.id, zoom, self.fit))
            }
            ViewZoom::Owned(log) => Ok(log.animate().set(zoom.ln())),
            ViewZoom::Parameter(parameter) => Ok(parameter.animate().set(zoom)),
            ViewZoom::Source(_) => Err(CameraViewError::ComputedZoom),
        }
    }

    fn pop(&self, out: bool) -> Anim {
        let focus = *self.focus.lock().expect("camera view focus poisoned");
        self.screen
            .animate()
            .camera_view_pop(self.frame.id, focus, self.zoom.signal(), out)
    }

    /// Grow the screen out of the region the camera sees into its place.
    pub fn pop_out(&self) -> Anim {
        self.pop(true)
    }

    /// Shrink the screen back into the region the camera sees.
    pub fn pop_in(&self) -> Anim {
        self.pop(false)
    }

    /// Keep the camera centered on `endpoint` from now on.
    pub fn follow(&self, endpoint: CanvasEndpoint, offset: DVec3) {
        *self.focus.lock().expect("camera view focus poisoned") = match &endpoint {
            CanvasEndpoint::Entity(object) => Some((*object, DVec3::ZERO, offset)),
            CanvasEndpoint::Anchor(point) => {
                Some((point.object, point.normalized, point.offset + offset))
            }
            _ => None,
        };
        self.frame
            .state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachEndpointFollow {
                target: self.frame.id,
                endpoint,
                offset,
                offset_space: FollowOffsetSpace::World,
            });
    }
}

fn validate_layer(layer: &str) -> Result<Arc<str>, CameraViewError> {
    let layer = layer.trim();
    if layer.is_empty() {
        return Err(CameraViewError::InvalidLayer);
    }
    Ok(Arc::from(layer))
}

impl DrawableHandle {
    /// Show inside this shape what a second camera framing `source` sees.
    ///
    /// See [`Self::camera_view_with`].
    pub fn camera_view(self, source: &DrawableHandle) -> Result<CameraViewHandle, CameraViewError> {
        self.camera_view_with(Some(source), CameraViewOptions::default())
    }

    /// Make this closed shape a screen for a second camera.
    ///
    /// The camera looks through `source`: its center and rotation set where
    /// the camera looks, and its size sets the zoom unless `options.zoom`
    /// gives one. Without `source` the camera looks through an invisible
    /// frame at the origin, with a zoom of 1 unless one is given. The screen
    /// draws its fill, then the view clipped to its outline, then its stroke.
    /// The screen, the frame and `options.exclude` stay out of the view. A
    /// later call or [`Self::no_camera_view`] replaces it.
    pub fn camera_view_with(
        self,
        source: Option<&DrawableHandle>,
        options: CameraViewOptions,
    ) -> Result<CameraViewHandle, CameraViewError> {
        let foreign = |handle: &DrawableHandle| !self.same_canvas(handle);
        if source.is_some_and(foreign) || options.exclude.iter().any(foreign) {
            return Err(CameraViewError::ForeignScene);
        }
        if source.is_some_and(|source| source.id == self.id) {
            return Err(CameraViewError::OwnSource);
        }
        if !is_camera_view_screen(&self.spec.lock().expect("object spec poisoned").kind) {
            return Err(CameraViewError::UnsupportedScreen);
        }
        let layers = options
            .layers
            .iter()
            .map(|layer| validate_layer(layer))
            .collect::<Result<Vec<_>, _>>()?;
        let zoom = match options.zoom {
            None if source.is_some() => ViewZoom::Frame,
            None => ViewZoom::Owned(
                parameter_in(&self.state, 0.0).map_err(|_| CameraViewError::InvalidZoom)?,
            ),
            Some(CameraViewZoom::Value(zoom)) => {
                if !zoom.is_finite() || zoom <= 0.0 {
                    return Err(CameraViewError::InvalidZoom);
                }
                ViewZoom::Owned(
                    parameter_in(&self.state, zoom.ln())
                        .map_err(|_| CameraViewError::InvalidZoom)?,
                )
            }
            Some(CameraViewZoom::Parameter(parameter)) => {
                if foreign(parameter.drawable()) {
                    return Err(CameraViewError::ForeignScene);
                }
                ViewZoom::Parameter(parameter)
            }
            Some(CameraViewZoom::Source(source)) => {
                let owners = source.scene_owners();
                if !owners.is_empty() && !owners.contains(&scene_id(&self)) {
                    return Err(CameraViewError::ForeignScene);
                }
                ViewZoom::Source(source)
            }
        };
        let frame = match source {
            Some(source) => source.clone(),
            // Only the center and rotation of an explicit-zoom frame count.
            None => spawn_in(&self.state, SpawnKind::Rect(1.0, 1.0), true)
                .no_fill()
                .no_stroke(),
        };
        let view = CameraViewSpec {
            source: frame.id,
            fit: options.fit,
            background: options.background,
            exclude: options.exclude.iter().map(|excluded| excluded.id).collect(),
            zoom: zoom.source(),
            layers,
        };
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::SetCameraView {
                target: self.id,
                view: Some(view),
            });
        Ok(CameraViewHandle {
            screen: self,
            frame,
            zoom,
            fit: options.fit,
            connectors: Vec::new(),
            focus: Arc::new(Mutex::new(None)),
        })
    }

    /// Remove the camera view shown inside this drawable.
    pub fn no_camera_view(self) -> Self {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::SetCameraView {
                target: self.id,
                view: None,
            });
        self
    }

    /// Put this drawable (and its children) on a view layer, or back on none.
    ///
    /// The main camera does not draw drawables on a layer; only camera views
    /// that list the layer show them.
    pub fn view_layer(self, layer: Option<&str>) -> Result<Self, CameraViewError> {
        let layer = layer.map(validate_layer).transpose()?;
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::SetViewLayer {
                target: self.id,
                layer,
            });
        Ok(self)
    }
}

/// Where a camera inset's screen goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CameraInsetPlacement {
    /// Against this corner or edge of the scene frame; `Center` centers it.
    Anchor(Anchor),
    /// Centered on a point.
    Point(f64, f64),
}

/// Outline shared by a camera inset's screen and frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CameraInsetShape {
    Rect,
    #[default]
    Rounded,
    Circle,
}

/// Options for [`SceneModel::camera_inset`].
#[derive(Debug, Clone)]
pub struct CameraInsetOptions {
    pub zoom: CameraViewZoom,
    pub placement: CameraInsetPlacement,
    /// Screen width, or diameter for a circle; `None` uses 30% of the scene
    /// width (20% for a circle).
    pub size: Option<f64>,
    pub shape: CameraInsetShape,
    /// Keep the frame on the target as it moves.
    pub follow: bool,
    /// Join the frame to the screen with two lines.
    pub connectors: bool,
    /// Stroke of the screen, frame and connectors; the theme accent by default.
    pub color: Option<Color>,
    /// Pin the screen to the output frame, like a HUD overlay.
    pub fixed: bool,
    pub background: CameraViewBackground,
    pub exclude: Vec<DrawableHandle>,
    pub layers: Vec<String>,
}

impl Default for CameraInsetOptions {
    fn default() -> Self {
        Self {
            zoom: CameraViewZoom::Value(2.0),
            placement: CameraInsetPlacement::Anchor(Anchor::TopRight),
            size: None,
            shape: CameraInsetShape::default(),
            follow: false,
            connectors: true,
            color: None,
            fixed: false,
            background: CameraViewBackground::default(),
            exclude: Vec::new(),
            layers: Vec::new(),
        }
    }
}

/// Distance between an inset and the scene frame edge, in scene units.
const INSET_MARGIN: f64 = 0.35;

/// Anchors joined by the two connector lines, on the frame and on the screen,
/// for a screen placed toward `direction` from the frame. Each line joins the
/// same anchor of both shapes along the outside of the pair.
fn connector_anchors(direction: DVec2) -> [Anchor; 2] {
    let (dx, dy) = (direction.x, direction.y);
    let horizontal = dy.abs() < 0.4 * dx.abs();
    let vertical = dx.abs() < 0.4 * dy.abs();
    match (horizontal, vertical) {
        (true, _) if dx > 0.0 => [Anchor::TopLeft, Anchor::BottomLeft],
        (true, _) => [Anchor::TopRight, Anchor::BottomRight],
        (_, true) if dy > 0.0 => [Anchor::BottomLeft, Anchor::BottomRight],
        (_, true) => [Anchor::TopLeft, Anchor::TopRight],
        _ if dx * dy > 0.0 => [Anchor::TopLeft, Anchor::BottomRight],
        _ => [Anchor::TopRight, Anchor::BottomLeft],
    }
}

impl SceneModel {
    /// Show an enlarged detail of the scene in an inset screen.
    ///
    /// Creates the screen, a frame on `target` outlining the region the screen
    /// shows, and optionally two connector lines, and returns the view. The
    /// frame's scale follows the zoom, so it always outlines what the screen
    /// shows at its resting size.
    pub fn camera_inset(
        &mut self,
        target: CanvasEndpoint,
        options: CameraInsetOptions,
    ) -> Result<CameraViewHandle, CameraViewError> {
        let frame_size = self.frame;
        let circle = options.shape == CameraInsetShape::Circle;
        let size = options.size.unwrap_or(if circle {
            frame_size.width * 0.2
        } else {
            frame_size.width * 0.3
        });
        if !size.is_finite() || size <= 0.0 {
            return Err(CameraViewError::InvalidSize);
        }
        let (width, height) = if circle {
            (size, size)
        } else {
            (size, size * frame_size.height / frame_size.width)
        };
        let color = options
            .color
            .or_else(|| self.theme_color("accent").ok())
            .unwrap_or(Color::WHITE);
        let static_target = match &target {
            CanvasEndpoint::Static(point) => Some(point.truncate()),
            _ => None,
        };
        if !options.follow
            && !matches!(
                target,
                CanvasEndpoint::Static(_) | CanvasEndpoint::Entity(_) | CanvasEndpoint::Anchor(_)
            )
        {
            return Err(CameraViewError::UnsupportedTarget);
        }

        let screen = match options.shape {
            CameraInsetShape::Rect => self.rect(width, height),
            CameraInsetShape::Rounded => self.rounded_rect(width, height, width.min(height) * 0.06),
            CameraInsetShape::Circle => self.circle(width / 2.0),
        }
        .stroke(color, 0.05);
        let (screen, direction) = match options.placement {
            CameraInsetPlacement::Anchor(Anchor::Center) => (
                screen.at_anchor(0.0, 0.0, Anchor::Center),
                static_target.map_or(DVec2::ONE, |target| -target),
            ),
            CameraInsetPlacement::Anchor(
                corner @ (Anchor::TopLeft
                | Anchor::TopRight
                | Anchor::BottomLeft
                | Anchor::BottomRight),
            ) => (screen.to_corner(corner, INSET_MARGIN), corner.to_offset()),
            CameraInsetPlacement::Anchor(edge) => {
                let direction = match edge {
                    Anchor::Top => Direction::Up,
                    Anchor::Bottom => Direction::Down,
                    Anchor::Left => Direction::Left,
                    _ => Direction::Right,
                };
                (screen.to_edge(direction, INSET_MARGIN), edge.to_offset())
            }
            CameraInsetPlacement::Point(x, y) => (
                screen.at_anchor(x, y, Anchor::Center),
                DVec2::new(x, y) - static_target.unwrap_or(DVec2::ZERO),
            ),
        };
        let direction = if direction.length_squared() > 1e-12 {
            direction
        } else {
            DVec2::ONE
        };
        let screen = if options.fixed {
            screen.hud()
        } else {
            screen.z_index(1)
        };

        let frame = if circle {
            self.circle(width / 2.0)
        } else {
            self.rect(width, height)
        }
        .no_fill()
        .stroke(color, 0.03);
        let scene = self.state.lock().expect("canvas state poisoned").scene_id;
        let frame = match &target {
            CanvasEndpoint::Static(point) => frame.at_anchor(point.x, point.y, Anchor::Center),
            CanvasEndpoint::Entity(object) => frame.at_anchor_point(AnchorPoint {
                scene_id: scene,
                object: *object,
                normalized: DVec3::ZERO,
                offset: DVec3::ZERO,
            }),
            CanvasEndpoint::Anchor(point) => frame.at_anchor_point(*point),
            _ => frame,
        };

        let mut connectors = Vec::new();
        if options.connectors {
            let ends = |anchor: Anchor, sign: f64| -> (AnchorPoint, AnchorPoint) {
                if circle {
                    // Points where the outer tangents touch both circles.
                    let perpendicular = DVec2::new(-direction.y, direction.x).normalize();
                    let offset = (perpendicular * sign * width / 2.0).extend(0.0);
                    (
                        frame.anchor_point(Anchor::Center, offset),
                        screen.anchor_point(Anchor::Center, offset),
                    )
                } else {
                    (
                        frame.anchor_point(anchor, DVec3::ZERO),
                        screen.anchor_point(anchor, DVec3::ZERO),
                    )
                }
            };
            for (anchor, sign) in connector_anchors(direction).into_iter().zip([1.0, -1.0]) {
                let (from, to) = ends(anchor, sign);
                let line = self
                    .endpoint_line(
                        CanvasEndpoint::Anchor(from),
                        CanvasEndpoint::Anchor(to),
                        false,
                    )
                    .no_fill()
                    .stroke(color, 0.02)
                    .opacity(0.8);
                connectors.push(line);
            }
        }

        let mut exclude = options.exclude;
        exclude.extend(connectors.iter().cloned());
        let mut view = screen.camera_view_with(
            Some(&frame),
            CameraViewOptions {
                fit: CameraViewFit::Contain,
                background: options.background,
                exclude,
                layers: options.layers,
                zoom: Some(options.zoom),
            },
        )?;
        if let Some(scale) = view.zoom.inverse() {
            view.frame = view.frame.clone().scale_to(scale);
        }
        if options.follow {
            view.follow(target, DVec3::ZERO);
        }
        view.connectors = connectors;
        Ok(view)
    }
}
