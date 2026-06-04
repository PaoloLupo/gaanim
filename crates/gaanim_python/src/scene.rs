use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::sync::{Arc, Mutex};

use gaanim_api::anim::AnimationBuilder;
use gaanim_core::peniko;
use gaanim_core::ObjectId;
use gaanim_math::RateFunc;

use crate::animation::{PyAnimationSpec, PyValueTracker};
use crate::color::PyColor;
use crate::mobject::{CommonSpec, MobjectSpec, PyMobject, TextRoleKind};
use crate::runtime;
use crate::selection::PySelection;
use crate::theme::PyTheme;

/// A single deferred operation. The runtime replays these in order during
/// the Bevy `Startup` system.
#[derive(Debug, Clone)]
pub(crate) enum DeferredOp {
    /// Spawn a mobject. `id` is the Python-side handle (used to map back to
    /// the actual Bevy `ObjectId` that the `SceneBuilder` allocates).
    /// The `spec` is shared (Arc<Mutex<>>) with the returned PyMobject so
    /// that chain mutations like `.fill(BLUE).z_index(-10)` propagate to
    /// the deferred op before replay.
    Spawn {
        id: ObjectId,
        spec: Arc<Mutex<MobjectSpec>>,
        creation_order: u64,
    },
    /// Play one or more animations in parallel. The timeline cursor advances
    /// by `max(duration)`.
    Play {
        specs: Vec<AnimationBuilder>,
    },
    Wait {
        duration: f64,
    },
    /// Bind a selection to a (parent, query) pair. The runtime resolves
    /// the matched child ids at replay time and registers them in
    /// `selection_map`.
    Select {
        parent: ObjectId,
        query: String,
        selection: ObjectId,
    },
    /// Apply a fill to all glyphs in a selection.
    SelectionFill {
        selection: ObjectId,
        color: peniko::Color,
    },
    /// Apply a stroke to all glyphs in a selection.
    SelectionStroke {
        selection: ObjectId,
        color: peniko::Color,
        width: f64,
    },
    /// Shift all glyphs in a selection in parallel on the timeline.
    SelectionShift {
        selection: ObjectId,
        dx: f64,
        dy: f64,
        duration: f64,
        rate_func: RateFunc,
    },
    Ungroup {
        group: ObjectId,
    },
    /// Marks the beginning of a new scene scope (multi-scene Engine).
    SceneBegin {
        name: String,
    },
    /// Marks the end of the current scene scope (multi-scene Engine).
    SceneEnd,
    /// Connect two scenes by their replay order index with a transition.
    SceneConnect {
        from_index: usize,
        to_index: usize,
        transition: gaanim_timeline::transition::TransitionType,
    },
    SpawnValueTracker {
        id: ObjectId,
        initial: f64,
    },
    AddUpdater {
        target: ObjectId,
        updater_type: String, // "bob" | "rotate" | "orbit" | "pulse" | "follow"
        params: Vec<f64>,
        follow_target: Option<ObjectId>,
    },
    RemoveUpdater {
        target: ObjectId,
    },
    SpawnTracedPath {
        id: ObjectId,
        source: ObjectId,
        color: peniko::Color,
        width: f64,
        min_distance: f64,
        max_points: Option<usize>,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct SceneInner {
    pub ops: Vec<DeferredOp>,
    pub id_counter: u32,
    pub width: u32,
    pub height: u32,
    pub background: Option<peniko::Color>,
    pub title: String,
    pub theme: PyTheme,
}

impl SceneInner {
    pub fn next_id(&mut self) -> ObjectId {
        self.id_counter += 1;
        ObjectId::from_parts(self.id_counter, 1)
    }
}

macro_rules! lock_inner {
    ($inner:expr) => {
        match $inner.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return Err(pyo3::exceptions::PyRuntimeError::new_err(
                    "Scene mutex is poisoned",
                ))
            }
        }
    };
}

/// The Python entry point for authoring a scene.
///
/// Holds a deferred op queue. Calling `render()` drains the queue into a
/// Bevy `App` and runs the Vello GPU pipeline.
#[pyclass(name = "Scene", module = "gaanim_core", frozen)]
pub struct PyScene {
    pub(crate) inner: Mutex<SceneInner>,
}

#[pymethods]
impl PyScene {
    #[new]
    #[pyo3(signature = (width=1280, height=720, title=None, theme=None))]
    fn new(width: u32, height: u32, title: Option<String>, theme: Option<PyTheme>) -> Self {
        let title = title.unwrap_or_else(|| "Gaanim Scene".to_string());
        let active_theme = theme.unwrap_or_else(PyTheme::DARK);
        let background = Some(active_theme.0.background);
        Self {
            inner: Mutex::new(SceneInner {
                ops: Vec::new(),
                id_counter: 0,
                width,
                height,
                background,
                title,
                theme: active_theme,
            }),
        }
    }

    #[getter]
    fn theme(&self) -> PyResult<PyTheme> {
        Ok(lock_inner!(self.inner).theme.clone())
    }

    fn set_theme(&self, theme: PyTheme) -> PyResult<()> {
        let mut inner = lock_inner!(self.inner);
        inner.background = Some(theme.0.background);
        inner.theme = theme;
        Ok(())
    }

    #[getter]
    fn width(&self) -> PyResult<u32> {
        Ok(lock_inner!(self.inner).width)
    }

    #[getter]
    fn height(&self) -> PyResult<u32> {
        Ok(lock_inner!(self.inner).height)
    }

    #[getter]
    fn title_str(&self) -> PyResult<String> {
        Ok(lock_inner!(self.inner).title.clone())
    }

    fn __repr__(&self) -> PyResult<String> {
        let inner = lock_inner!(self.inner);
        Ok(format!(
            "Scene({}x{}, \"{}\", ops={})",
            inner.width,
            inner.height,
            inner.title,
            inner.ops.len(),
        ))
    }

    fn background(&self, color: &PyColor) -> PyResult<()> {
        lock_inner!(self.inner).background = Some(color.0);
        Ok(())
    }

    fn group(&self, children: Vec<PyMobject>) -> PyResult<PyMobject> {
        let mut inner = lock_inner!(self.inner);
        let id = inner.next_id();
        let common = Self::default_common(inner.theme.0.primary);
        
        let children_specs = children.iter()
            .map(|c| (c.id, c.spec.clone(), c.creation_order))
            .collect();
            
        let spec = Arc::new(Mutex::new(MobjectSpec::Group {
            common,
            children: children_specs,
        }));
        
        let order = id.index() as u64;
        inner.ops.push(DeferredOp::Spawn {
            id,
            spec: spec.clone(),
            creation_order: order,
        });
        
        Ok(PyMobject {
            id,
            spec,
            creation_order: order,
        })
    }

    fn value_tracker(&self, initial: f64) -> PyResult<PyValueTracker> {
        let mut inner = lock_inner!(self.inner);
        let id = inner.next_id();
        inner.ops.push(DeferredOp::SpawnValueTracker { id, initial });
        Ok(PyValueTracker { id })
    }

    #[pyo3(signature = (source, color=None, width=2.0, min_distance=5.0, max_points=None))]
    fn traced_path(
        &self,
        source: &PyMobject,
        color: Option<&PyColor>,
        width: f64,
        min_distance: f64,
        max_points: Option<usize>,
    ) -> PyResult<PyMobject> {
        let mut inner = lock_inner!(self.inner);
        let id = inner.next_id();
        let c = color.map(|c| c.0).unwrap_or(peniko::Color::from_rgb8(255, 215, 0));
        
        inner.ops.push(DeferredOp::SpawnTracedPath {
            id,
            source: source.id,
            color: c,
            width,
            min_distance,
            max_points,
        });

        let common = Self::default_common(inner.theme.0.primary);
        let spec = Arc::new(Mutex::new(MobjectSpec::Line {
            common,
            start: (0.0, 0.0),
            end: (0.0, 0.0),
        }));
        let order = id.index() as u64;

        Ok(PyMobject {
            id,
            spec,
            creation_order: order,
        })
    }

    fn ungroup(&self, group: &PyMobject) -> PyResult<()> {
        let mut inner = lock_inner!(self.inner);
        inner.ops.push(DeferredOp::Ungroup { group: group.id });
        Ok(())
    }

    // ====== spawn helpers ======

    fn circle(&self, radius: f64) -> PyResult<PyMobject> {
        let mut inner = lock_inner!(self.inner);
        let id = inner.next_id();
        let common = Self::default_common(inner.theme.0.primary);
        let spec = Arc::new(Mutex::new(MobjectSpec::Circle {
            common,
            radius,
        }));
        let order = id.index() as u64;
        inner.ops.push(DeferredOp::Spawn {
            id,
            spec: spec.clone(),
            creation_order: order,
        });
        Ok(PyMobject {
            id,
            spec,
            creation_order: order,
        })
    }

    fn rectangle(&self, width: f64, height: f64) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::Rectangle {
            common: Self::default_common(primary_color),
            width,
            height,
        })
    }

    fn rounded_rect(&self, width: f64, height: f64, radius: f64) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::RoundedRect {
            common: Self::default_common(primary_color),
            width,
            height,
            radius,
        })
    }

    fn square(&self, side: f64) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::Square {
            common: Self::default_common(primary_color),
            side,
        })
    }

    fn dot(&self, radius: f64) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::Dot {
            common: Self::default_common(primary_color),
            radius,
        })
    }

    fn ellipse(&self, rx: f64, ry: f64) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::Ellipse {
            common: Self::default_common(primary_color),
            rx,
            ry,
        })
    }

    fn line(&self, x1: f64, y1: f64, x2: f64, y2: f64) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::Line {
            common: CommonSpec {
                fill: None,
                stroke: Some((primary_color, 2.0)),
                z_index: 0,
                opacity: 1.0,
                transform: gaanim_math::SpatialTransform::default(),
                next_to: None,
            },
            start: (x1, y1),
            end: (x2, y2),
        })
    }

    fn arrow(&self, x1: f64, y1: f64, x2: f64, y2: f64) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::Arrow {
            common: CommonSpec {
                fill: Some(primary_color),
                stroke: Some((primary_color, 2.0)),
                z_index: 0,
                opacity: 1.0,
                transform: gaanim_math::SpatialTransform::default(),
                next_to: None,
            },
            start: (x1, y1),
            end: (x2, y2),
        })
    }

    fn polygon(&self, points: Vec<(f64, f64)>) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::Polygon {
            common: Self::default_common(primary_color),
            points,
        })
    }

    fn star(&self, n_points: u32, outer_radius: f64, inner_radius: f64) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::Star {
            common: Self::default_common(primary_color),
            n_points,
            outer_radius,
            inner_radius,
        })
    }

    fn checkmark(&self, size: f64) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::Checkmark {
            common: CommonSpec {
                fill: None,
                stroke: Some((primary_color, 4.0)),
                z_index: 0,
                opacity: 1.0,
                transform: gaanim_math::SpatialTransform::default(),
                next_to: None,
            },
            size,
        })
    }

    fn regular_polygon(&self, n_sides: u32, radius: f64) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::RegularPolygon {
            common: Self::default_common(primary_color),
            n_sides,
            radius,
        })
    }

    fn text(&self, content: &str, role: Option<&str>) -> PyResult<PyMobject> {
        let role = TextRoleKind::from_str(role.unwrap_or("body"));
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::Text {
            common: Self::default_common(primary_color),
            content: content.to_string(),
            role,
        })
    }

    #[pyo3(signature = (signal_tracker, num_decimals=2, prefix="", suffix="", font_family="Inter", font_size=36.0))]
    fn decimal_number(
        &self,
        signal_tracker: &crate::animation::PyValueTracker,
        num_decimals: usize,
        prefix: &str,
        suffix: &str,
        font_family: &str,
        font_size: f64,
    ) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::DecimalNumber {
            common: Self::default_common(primary_color),
            signal_id: signal_tracker.id,
            num_decimals,
            prefix: prefix.to_string(),
            suffix: suffix.to_string(),
            font_family: font_family.to_string(),
            font_size,
        })
    }

    fn title(&self, content: &str) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::Text {
            common: Self::default_common(primary_color),
            content: content.to_string(),
            role: TextRoleKind::Title,
        })
    }

    fn subtitle(&self, content: &str) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::Text {
            common: Self::default_common(primary_color),
            content: content.to_string(),
            role: TextRoleKind::Subtitle,
        })
    }

    fn body(&self, content: &str) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::Text {
            common: Self::default_common(primary_color),
            content: content.to_string(),
            role: TextRoleKind::Body,
        })
    }

    fn caption(&self, content: &str) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::Text {
            common: Self::default_common(primary_color),
            content: content.to_string(),
            role: TextRoleKind::Caption,
        })
    }

    fn equation(&self, formula: &str) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::Equation {
            common: Self::default_common(primary_color),
            formula: formula.to_string(),
        })
    }

    fn dashed_line(
        &self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        dash_length: f64,
        gap_length: f64,
    ) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::DashedLine {
            common: CommonSpec {
                fill: None,
                stroke: Some((primary_color, 2.0)),
                z_index: 0,
                opacity: 1.0,
                transform: gaanim_math::SpatialTransform::default(),
                next_to: None,
            },
            start: (x1, y1),
            end: (x2, y2),
            dash_length,
            gap_length,
        })
    }

    fn arc(
        &self,
        cx: f64,
        cy: f64,
        rx: f64,
        ry: f64,
        start_angle: f64,
        sweep_angle: f64,
    ) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::Arc {
            common: CommonSpec {
                fill: None,
                stroke: Some((primary_color, 2.0)),
                z_index: 0,
                opacity: 1.0,
                transform: gaanim_math::SpatialTransform::default(),
                next_to: None,
            },
            center: (cx, cy),
            rx,
            ry,
            start_angle,
            sweep_angle,
        })
    }

    fn arc_between_points(
        &self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        angle: f64,
    ) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::ArcBetweenPoints {
            common: CommonSpec {
                fill: None,
                stroke: Some((primary_color, 2.0)),
                z_index: 0,
                opacity: 1.0,
                transform: gaanim_math::SpatialTransform::default(),
                next_to: None,
            },
            start: (x1, y1),
            end: (x2, y2),
            angle,
        })
    }

    fn double_arrow(
        &self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        head_len: Option<f64>,
        head_width: Option<f64>,
    ) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::DoubleArrow {
            common: CommonSpec {
                fill: Some(primary_color),
                stroke: Some((primary_color, 2.0)),
                z_index: 0,
                opacity: 1.0,
                transform: gaanim_math::SpatialTransform::default(),
                next_to: None,
            },
            start: (x1, y1),
            end: (x2, y2),
            head_len,
            head_width,
        })
    }

    fn sector(
        &self,
        cx: f64,
        cy: f64,
        radius: f64,
        start_angle: f64,
        sweep_angle: f64,
    ) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::Sector {
            common: Self::default_common(primary_color),
            center: (cx, cy),
            radius,
            start_angle,
            sweep_angle,
        })
    }

    fn annulus(&self, outer_radius: f64, inner_radius: f64) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::Annulus {
            common: Self::default_common(primary_color),
            outer_radius,
            inner_radius,
        })
    }

    fn surrounding_rectangle(
        &self,
        width: f64,
        height: f64,
        corner_radius: f64,
    ) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::SurroundingRectangle {
            common: CommonSpec {
                fill: None,
                stroke: Some((primary_color, 2.0)),
                z_index: 0,
                opacity: 1.0,
                transform: gaanim_math::SpatialTransform::default(),
                next_to: None,
            },
            width,
            height,
            corner_radius,
        })
    }

    fn background_rectangle(&self, width: f64, height: f64) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::BackgroundRectangle {
            common: CommonSpec {
                fill: Some(primary_color),
                stroke: None,
                z_index: -10,
                opacity: 1.0,
                transform: gaanim_math::SpatialTransform::default(),
                next_to: None,
            },
            width,
            height,
        })
    }

    fn cross(&self, size: f64) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::Cross {
            common: CommonSpec {
                fill: None,
                stroke: Some((primary_color, 3.0)),
                z_index: 0,
                opacity: 1.0,
                transform: gaanim_math::SpatialTransform::default(),
                next_to: None,
            },
            size,
        })
    }

    fn right_angle(&self, arm_length: f64) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::RightAngle {
            common: CommonSpec {
                fill: None,
                stroke: Some((primary_color, 2.0)),
                z_index: 0,
                opacity: 1.0,
                transform: gaanim_math::SpatialTransform::default(),
                next_to: None,
            },
            arm_length,
        })
    }

    /// Tangent line to a polyline curve at fractional position `t`.
    /// `curve` is a list of `(x, y)` waypoints; `length` is the
    /// half-length of the line on either side of the tangent point.
    fn tangent_line(
        &self,
        curve: Vec<(f64, f64)>,
        t: f64,
        length: f64,
    ) -> PyResult<PyMobject> {
        if curve.len() < 2 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "tangent_line requires at least 2 waypoints",
            ));
        }
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::TangentLine {
            common: CommonSpec {
                fill: None,
                stroke: Some((primary_color, 2.0)),
                z_index: 0,
                opacity: 1.0,
                transform: gaanim_math::SpatialTransform::default(),
                next_to: None,
            },
            curve,
            t,
            length,
        })
    }

    /// Cartesian number plane with axes and grid.
    /// `x_range`, `y_range` are `(min, max, step)` tuples.
    fn number_plane(
        &self,
        x_range: (f64, f64, f64),
        y_range: (f64, f64, f64),
        axis_stroke: f64,
        grid_stroke: f64,
    ) -> PyResult<PyMobject> {
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::NumberPlane {
            common: CommonSpec {
                fill: None,
                stroke: Some((primary_color, axis_stroke)),
                z_index: 0,
                opacity: 1.0,
                transform: gaanim_math::SpatialTransform::default(),
                next_to: None,
            },
            x_range,
            y_range,
            axis_stroke,
            grid_stroke,
        })
    }

    /// Boolean union: returns a mobject whose filled area is the union of
    /// the two source mobjects' geometries. The source mobjects are left
    /// untouched and can be animated or removed independently.
    fn union(&self, a: &PyMobject, b: &PyMobject) -> PyResult<PyMobject> {
        self.boolean_op(a, b, gaanim_objects::boolean::BooleanOp::Union)
    }

    /// Boolean intersection: filled area is the overlap of A and B.
    fn intersection(&self, a: &PyMobject, b: &PyMobject) -> PyResult<PyMobject> {
        self.boolean_op(a, b, gaanim_objects::boolean::BooleanOp::Intersection)
    }

    /// Boolean difference: filled area is A minus B.
    fn difference(&self, a: &PyMobject, b: &PyMobject) -> PyResult<PyMobject> {
        self.boolean_op(a, b, gaanim_objects::boolean::BooleanOp::Difference)
    }

    /// Boolean exclusion: filled area is (A ∪ B) \ (A ∩ B), the symmetric
    /// difference (XOR).
    fn exclusion(&self, a: &PyMobject, b: &PyMobject) -> PyResult<PyMobject> {
        self.boolean_op(a, b, gaanim_objects::boolean::BooleanOp::Exclusion)
    }

    // ====== animations / waits ======

    #[pyo3(signature = (*anims))]
    fn play(&self, py: Python<'_>, anims: &Bound<'_, pyo3::types::PyTuple>) -> PyResult<()> {
        if anims.is_empty() {
            return Err(PyValueError::new_err(
                "play() requires at least one animation",
            ));
        }
        let mut specs = Vec::with_capacity(anims.len());
        for item in anims.iter() {
            // AnimSpec or SelectionAnim.build() result
            if let Ok(spec) = item.extract::<PyAnimationSpec>() {
                specs.push(spec.inner);
            } else if item.is_none() {
                // Allow passing None as a placeholder
                continue;
            } else {
                return Err(PyValueError::new_err(format!(
                    "play() expects AnimSpec objects, got {}",
                    item.get_type().name()?
                )));
            }
        }
        let _ = py;
        if !specs.is_empty() {
            lock_inner!(self.inner).ops.push(DeferredOp::Play { specs });
        }
        Ok(())
    }

    fn wait(&self, duration: f64) -> PyResult<()> {
        lock_inner!(self.inner)
            .ops
            .push(DeferredOp::Wait { duration });
        Ok(())
    }

    fn slide(&self) -> PyResult<()> {
        // Slides are markers in the timeline; the runtime translates them
        // into a Breakpoint clip on the default track.
        lock_inner!(self.inner)
            .ops
            .push(DeferredOp::Play { specs: Vec::new() });
        Ok(())
    }

    // ====== selection ======

    fn select(&self, parent: &PyMobject, query: &str) -> PyResult<PySelection> {
        let mut inner = lock_inner!(self.inner);
        let selection_id = inner.next_id();
        let parent_id = parent.id;
        inner.ops.push(DeferredOp::Select {
            parent: parent_id,
            query: query.to_string(),
            selection: selection_id,
        });
        Ok(PySelection {
            parent: parent_id,
            query: query.to_string(),
            id: selection_id,
        })
    }

    /// Apply a fill to all glyphs in a selection (instant, at build time).
    fn fill_selection(&self, selection: &PySelection, color: &PyColor) -> PyResult<()> {
        self.push_selection_fill(selection.id, color.0)
    }

    /// Apply a stroke to all glyphs in a selection (instant, at build time).
    fn set_stroke_selection(
        &self,
        selection: &PySelection,
        color: &PyColor,
        width: f64,
    ) -> PyResult<()> {
        self.push_selection_stroke(selection.id, color.0, width)
    }

    /// Begin a coordinated per-glyph shift animation across a selection.
    fn selection_anim(
        &self,
        selection: &PySelection,
        dx: f64,
        dy: f64,
    ) -> crate::selection::PySelectionAnim {
        crate::selection::PySelectionAnim {
            selection: selection.id,
            parent: selection.parent,
            dx,
            dy,
            duration: 1.0,
            rate_func: RateFunc::Smooth,
        }
    }

    // ====== render ======

    /// Drain the deferred op queue into a Bevy `App` and run the GPU window.
    /// Blocking: returns when the window is closed.
    fn render(&self, py: Python<'_>) -> PyResult<()> {
        let mut inner = lock_inner!(self.inner);
        let ops = std::mem::take(&mut inner.ops);
        let width = inner.width;
        let height = inner.height;
        let title = inner.title.clone();
        let background = inner.background;
        drop(inner);

        // Release the GIL while we drive Bevy.
        py.detach(|| {
            runtime::run(ops, width, height, title, background);
        });
        Ok(())
    }

    /// Exposes a premium offline render and export engine for video, image sequence, and GIF.
    /// Supports transparent WebM, segment/time-range rendering, and multiple presets.
    #[pyo3(signature = (
        output_path,
        fps = 60,
        width = None,
        height = None,
        transparent = None,
        aspect_ratio = None,
        quality = None,
        start_time = None,
        end_time = None
    ))]
    fn export(
        &self,
        py: Python<'_>,
        output_path: String,
        fps: u32,
        width: Option<u32>,
        height: Option<u32>,
        transparent: Option<bool>,
        aspect_ratio: Option<String>,
        quality: Option<String>,
        start_time: Option<f64>,
        end_time: Option<f64>,
    ) -> PyResult<()> {
        let mut inner = lock_inner!(self.inner);
        let ops = std::mem::take(&mut inner.ops);
        let w = width.unwrap_or(inner.width);
        let h = height.unwrap_or(inner.height);
        let background = inner.background;
        drop(inner);

        use gaanim_export::prelude::*;

        let mut config = ExportConfig::new(&output_path);
        config.fps = fps;
        config.width = w;
        config.height = h;

        if let Some(t) = transparent {
            config.transparent = t;
        }

        if let Some(ar) = aspect_ratio {
            let preset = match ar.to_lowercase().as_str() {
                "youtube" | "16:9" | "16_9" => AspectRatioPreset::Youtube,
                "tiktok" | "9:16" | "9_16" => AspectRatioPreset::TikTok,
                "instagram" | "1:1" | "1_1" => AspectRatioPreset::Instagram,
                _ => return Err(PyValueError::new_err(format!("Invalid aspect ratio preset: {}. Choose from: youtube, tiktok, instagram", ar))),
            };
            config = config.with_aspect_ratio(preset);
        }

        if let Some(q) = quality {
            let preset = match q.to_lowercase().as_str() {
                "draft" => QualityPreset::Draft,
                "standard" => QualityPreset::Standard,
                "production" => QualityPreset::Production,
                _ => return Err(PyValueError::new_err(format!("Invalid quality preset: {}. Choose from: draft, standard, production", q))),
            };
            config = config.with_quality(preset);
        }

        // Custom overrides after presets
        if let Some(width_val) = width {
            config.width = width_val;
        }
        if let Some(height_val) = height {
            config.height = height_val;
        }

        config.start_time = start_time;
        config.end_time = end_time;

        py.detach(move || {
            let setup_world = move |world: &mut bevy::prelude::World| {
                runtime::replay_into(world, ops, w, h, background);
            };

            if let Err(e) = export_scene(config, setup_world) {
                bevy::prelude::error!("Export error: {}", e);
            }
        });

        Ok(())
    }

    // ====== edit ======

    /// Drain the deferred op queue into a Bevy `App` with the interactive
    /// editor overlay (inspector, hierarchy, playback controls).
    /// Blocking: returns when the window is closed.
    fn edit(&self, py: Python<'_>) -> PyResult<()> {
        let mut inner = lock_inner!(self.inner);
        let ops = std::mem::take(&mut inner.ops);
        let width = inner.width;
        let height = inner.height;
        let title = inner.title.clone();
        let background = inner.background;
        drop(inner);

        py.detach(|| {
            runtime::run_editor(ops, width, height, title, background);
        });
        Ok(())
    }
}

// Helpers (internal use by Selection, constructors).
impl PyScene {
    fn default_common(primary_color: peniko::Color) -> CommonSpec {
        CommonSpec {
            fill: Some(primary_color),
            stroke: None,
            z_index: 0,
            opacity: 1.0,
            transform: gaanim_math::SpatialTransform::default(),
            next_to: None,
        }
    }
    pub(crate) fn push_selection_fill(
        &self,
        selection: ObjectId,
        color: peniko::Color,
    ) -> PyResult<()> {
        lock_inner!(self.inner)
            .ops
            .push(DeferredOp::SelectionFill { selection, color });
        Ok(())
    }

    pub(crate) fn push_selection_stroke(
        &self,
        selection: ObjectId,
        color: peniko::Color,
        width: f64,
    ) -> PyResult<()> {
        lock_inner!(self.inner)
            .ops
            .push(DeferredOp::SelectionStroke {
                selection,
                color,
                width,
            });
        Ok(())
    }

    pub(crate) fn push_selection_shift(
        &self,
        selection: ObjectId,
        dx: f64,
        dy: f64,
        duration: f64,
        rate_func: RateFunc,
    ) -> PyResult<()> {
        lock_inner!(self.inner)
            .ops
            .push(DeferredOp::SelectionShift {
                selection,
                dx,
                dy,
                duration,
                rate_func,
            });
        Ok(())
    }

    fn spawn_with(&self, spec: MobjectSpec) -> PyResult<PyMobject> {
        let mut inner = lock_inner!(self.inner);
        let id = inner.next_id();
        let order = id.index() as u64;
        let spec = Arc::new(Mutex::new(spec));
        inner.ops.push(DeferredOp::Spawn {
            id,
            spec: spec.clone(),
            creation_order: order,
        });
        Ok(PyMobject {
            id,
            spec,
            creation_order: order,
        })
    }

    /// Shared implementation of the four boolean ops exposed to Python.
    /// Reconstructs the source mobjects' geometry from their specs, runs
    /// the operation via `gaanim_objects::boolean`, and stores the result
    /// as `MobjectSpec::BooleanResult` for replay.
    fn boolean_op(
        &self,
        a: &PyMobject,
        b: &PyMobject,
        op: gaanim_objects::boolean::BooleanOp,
    ) -> PyResult<PyMobject> {
        let path_a = contours_to_bezpath(&a.spec.lock().unwrap().to_contours());
        let path_b = contours_to_bezpath(&b.spec.lock().unwrap().to_contours());
        let result = gaanim_objects::boolean::apply(&path_a, &path_b, op);
        let mut combined: Vec<Vec<[f64; 2]>> = Vec::new();
        for path in &result.paths {
            for ring in bezpath_to_contours(path) {
                combined.push(ring);
            }
        }
        let primary_color = lock_inner!(self.inner).theme.0.primary;
        self.spawn_with(MobjectSpec::BooleanResult {
            common: CommonSpec {
                fill: Some(primary_color),
                stroke: Some((primary_color, 2.0)),
                z_index: 0,
                opacity: 1.0,
                transform: gaanim_math::SpatialTransform::default(),
                next_to: None,
            },
            contours: combined,
        })
    }
}

pub(crate) fn contours_to_bezpath(contours: &[Vec<[f64; 2]>]) -> gaanim_core::kurbo::BezPath {
    let mut path = gaanim_core::kurbo::BezPath::new();
    for ring in contours {
        if ring.is_empty() {
            continue;
        }
        path.move_to(gaanim_core::kurbo::Point::new(ring[0][0], ring[0][1]));
        for p in &ring[1..] {
            path.line_to(gaanim_core::kurbo::Point::new(p[0], p[1]));
        }
        path.close_path();
    }
    path
}

pub(crate) fn bezpath_to_contours(path: &gaanim_core::kurbo::BezPath) -> Vec<Vec<[f64; 2]>> {
    let mut out: Vec<Vec<[f64; 2]>> = Vec::new();
    let mut current: Option<Vec<[f64; 2]>> = None;
    for el in path.iter() {
        match el {
            gaanim_core::kurbo::PathEl::MoveTo(p) => {
                if let Some(buf) = current.take() {
                    if buf.len() >= 3 {
                        out.push(buf);
                    }
                }
                current = Some(vec![[p.x, p.y]]);
            }
            gaanim_core::kurbo::PathEl::LineTo(p) => {
                if let Some(buf) = current.as_mut() {
                    buf.push([p.x, p.y]);
                }
            }
            gaanim_core::kurbo::PathEl::ClosePath => {
                if let Some(buf) = current.take() {
                    if buf.len() >= 3 {
                        out.push(buf);
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(buf) = current.take() {
        if buf.len() >= 3 {
            out.push(buf);
        }
    }
    out
}
