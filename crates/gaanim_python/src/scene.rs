use pyo3::prelude::*;
use pyo3::exceptions::PyValueError;
use std::sync::{Arc, Mutex};

use gaanim_api::anim::AnimationBuilder;
use gaanim_core::peniko;
use gaanim_core::ObjectId;
use gaanim_math::RateFunc;

use crate::animation::PyAnimationSpec;
use crate::color::PyColor;
use crate::mobject::{MobjectSpec, PyMobject, TextRoleKind};
use crate::runtime;
use crate::selection::PySelection;

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
            Err(_) => return Err(pyo3::exceptions::PyRuntimeError::new_err("Scene mutex is poisoned")),
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
    #[pyo3(signature = (width=1280, height=720, title=None))]
    fn new(width: u32, height: u32, title: Option<String>) -> Self {
        let title = title.unwrap_or_else(|| "Gaanim Scene".to_string());
        Self {
            inner: Mutex::new(SceneInner {
                ops: Vec::new(),
                id_counter: 0,
                width,
                height,
                background: None,
                title,
            }),
        }
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
        let spec = Arc::new(Mutex::new(MobjectSpec::Circle {
            radius,
            fill: Some(peniko::Color::WHITE),
            stroke: None,
            z_index: 0,
            opacity: 1.0,
            transform: gaanim_math::SpatialTransform::default(),
            next_to: None,
        }));
        let order = id.index() as u64;
        inner.ops.push(DeferredOp::Spawn {
            id,
            spec: spec.clone(),
            creation_order: order,
        });
        Ok(PyMobject { id, spec, creation_order: order })
    }

    fn rectangle(&self, width: f64, height: f64) -> PyResult<PyMobject> {
        self.spawn_with(MobjectSpec::Rectangle {
            width,
            height,
            fill: Some(peniko::Color::WHITE),
            stroke: None,
            z_index: 0,
            opacity: 1.0,
            transform: gaanim_math::SpatialTransform::default(),
            next_to: None,
        })
    }

    fn rounded_rect(&self, width: f64, height: f64, radius: f64) -> PyResult<PyMobject> {
        self.spawn_with(MobjectSpec::RoundedRect {
            width,
            height,
            radius,
            fill: Some(peniko::Color::WHITE),
            stroke: None,
            z_index: 0,
            opacity: 1.0,
            transform: gaanim_math::SpatialTransform::default(),
            next_to: None,
        })
    }

    fn square(&self, side: f64) -> PyResult<PyMobject> {
        self.spawn_with(MobjectSpec::Square {
            side,
            fill: Some(peniko::Color::WHITE),
            stroke: None,
            z_index: 0,
            opacity: 1.0,
            transform: gaanim_math::SpatialTransform::default(),
            next_to: None,
        })
    }

    fn dot(&self, radius: f64) -> PyResult<PyMobject> {
        self.spawn_with(MobjectSpec::Dot {
            radius,
            fill: Some(peniko::Color::WHITE),
            stroke: None,
            z_index: 0,
            opacity: 1.0,
            transform: gaanim_math::SpatialTransform::default(),
            next_to: None,
        })
    }

    fn ellipse(&self, rx: f64, ry: f64) -> PyResult<PyMobject> {
        self.spawn_with(MobjectSpec::Ellipse {
            rx,
            ry,
            fill: Some(peniko::Color::WHITE),
            stroke: None,
            z_index: 0,
            opacity: 1.0,
            transform: gaanim_math::SpatialTransform::default(),
            next_to: None,
        })
    }

    fn line(&self, x1: f64, y1: f64, x2: f64, y2: f64) -> PyResult<PyMobject> {
        self.spawn_with(MobjectSpec::Line {
            start: (x1, y1),
            end: (x2, y2),
            stroke: Some((peniko::Color::WHITE, 2.0)),
            z_index: 0,
            opacity: 1.0,
            transform: gaanim_math::SpatialTransform::default(),
            next_to: None,
        })
    }

    fn arrow(&self, x1: f64, y1: f64, x2: f64, y2: f64) -> PyResult<PyMobject> {
        self.spawn_with(MobjectSpec::Arrow {
            start: (x1, y1),
            end: (x2, y2),
            stroke: Some((peniko::Color::WHITE, 2.0)),
            fill: Some(peniko::Color::WHITE),
            z_index: 0,
            opacity: 1.0,
            transform: gaanim_math::SpatialTransform::default(),
            next_to: None,
        })
    }

    fn polygon(&self, points: Vec<(f64, f64)>) -> PyResult<PyMobject> {
        self.spawn_with(MobjectSpec::Polygon {
            points,
            fill: Some(peniko::Color::WHITE),
            stroke: None,
            z_index: 0,
            opacity: 1.0,
            transform: gaanim_math::SpatialTransform::default(),
            next_to: None,
        })
    }

    fn star(&self, n_points: u32, outer_radius: f64, inner_radius: f64) -> PyResult<PyMobject> {
        self.spawn_with(MobjectSpec::Star {
            n_points,
            outer_radius,
            inner_radius,
            fill: Some(peniko::Color::WHITE),
            stroke: None,
            z_index: 0,
            opacity: 1.0,
            transform: gaanim_math::SpatialTransform::default(),
            next_to: None,
        })
    }

    fn checkmark(&self, size: f64) -> PyResult<PyMobject> {
        self.spawn_with(MobjectSpec::Checkmark {
            size,
            stroke: Some((peniko::Color::WHITE, 4.0)),
            z_index: 0,
            opacity: 1.0,
            transform: gaanim_math::SpatialTransform::default(),
            next_to: None,
        })
    }

    fn regular_polygon(&self, n_sides: u32, radius: f64) -> PyResult<PyMobject> {
        self.spawn_with(MobjectSpec::RegularPolygon {
            n_sides,
            radius,
            fill: Some(peniko::Color::WHITE),
            stroke: None,
            z_index: 0,
            opacity: 1.0,
            transform: gaanim_math::SpatialTransform::default(),
            next_to: None,
        })
    }

    fn text(&self, content: &str, role: Option<&str>) -> PyResult<PyMobject> {
        let role = TextRoleKind::from_str(role.unwrap_or("body"));
        self.spawn_with(MobjectSpec::Text {
            content: content.to_string(),
            role,
            fill: None,
            z_index: 0,
            opacity: 1.0,
            transform: gaanim_math::SpatialTransform::default(),
            next_to: None,
        })
    }

    fn title(&self, content: &str) -> PyResult<PyMobject> {
        self.spawn_with(MobjectSpec::Text {
            content: content.to_string(),
            role: TextRoleKind::Title,
            fill: None,
            z_index: 0,
            opacity: 1.0,
            transform: gaanim_math::SpatialTransform::default(),
            next_to: None,
        })
    }

    fn subtitle(&self, content: &str) -> PyResult<PyMobject> {
        self.spawn_with(MobjectSpec::Text {
            content: content.to_string(),
            role: TextRoleKind::Subtitle,
            fill: None,
            z_index: 0,
            opacity: 1.0,
            transform: gaanim_math::SpatialTransform::default(),
            next_to: None,
        })
    }

    fn body(&self, content: &str) -> PyResult<PyMobject> {
        self.spawn_with(MobjectSpec::Text {
            content: content.to_string(),
            role: TextRoleKind::Body,
            fill: None,
            z_index: 0,
            opacity: 1.0,
            transform: gaanim_math::SpatialTransform::default(),
            next_to: None,
        })
    }

    fn caption(&self, content: &str) -> PyResult<PyMobject> {
        self.spawn_with(MobjectSpec::Text {
            content: content.to_string(),
            role: TextRoleKind::Caption,
            fill: None,
            z_index: 0,
            opacity: 1.0,
            transform: gaanim_math::SpatialTransform::default(),
            next_to: None,
        })
    }

    fn equation(&self, formula: &str) -> PyResult<PyMobject> {
        self.spawn_with(MobjectSpec::Equation {
            formula: formula.to_string(),
            fill: None,
            z_index: 0,
            opacity: 1.0,
            transform: gaanim_math::SpatialTransform::default(),
            next_to: None,
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
        lock_inner!(self.inner).ops.push(DeferredOp::Wait { duration });
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
        let inner = lock_inner!(self.inner);
        let ops = inner.ops.clone();
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

// Helpers (internal use by Selection).
impl PyScene {
    pub(crate) fn push_selection_fill(&self, selection: ObjectId, color: peniko::Color) -> PyResult<()> {
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
        lock_inner!(self.inner).ops.push(DeferredOp::SelectionShift {
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
