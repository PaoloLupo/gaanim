use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::sync::{Arc, Mutex};

use gaanim_api::anim::AnimationBuilder;
use gaanim_core::peniko;
use gaanim_core::ObjectId;
use gaanim_math::RateFunc;

use crate::animation::PyAnimationSpec;
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
    inner: Mutex<SceneInner>,
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
}
