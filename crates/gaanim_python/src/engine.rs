use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::sync::{Arc, Mutex};

use gaanim_core::peniko;
use gaanim_core::ObjectId;

use crate::animation::PyAnimationSpec;
use crate::color::PyColor;
use crate::mobject::{CommonSpec, MobjectSpec, PyMobject, TextRoleKind};
use crate::runtime;
use crate::scene::DeferredOp;
use crate::selection::PySelection;
use crate::theme::PyTheme;
use crate::transition::PyTransitionType;

/// Connection between two scenes with a transition.
#[derive(Debug, Clone)]
struct SceneConnection {
    from: usize, // index into scenes vec
    to: usize,   // index into scenes vec
    transition: PyTransitionType,
}

/// Per-scene state.
#[derive(Debug, Clone)]
struct SceneOps {
    name: String,
    ops: Vec<DeferredOp>,
    id_counter: u32,
}

impl SceneOps {
    fn next_id(&mut self) -> ObjectId {
        self.id_counter += 1;
        ObjectId::from_parts(self.id_counter, 1)
    }
}

#[derive(Debug, Clone)]
struct EngineInner {
    scenes: Vec<SceneOps>,
    connections: Vec<SceneConnection>,
    width: u32,
    height: u32,
    background: Option<peniko::Color>,
    title: String,
    theme: PyTheme,
}

macro_rules! lock_engine {
    ($inner:expr) => {
        match $inner.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return Err(pyo3::exceptions::PyRuntimeError::new_err(
                    "Engine mutex is poisoned",
                ))
            }
        }
    };
}

/// Multi-scene animation engine.
///
/// Create scenes, add mobjects and animations to each, connect them with
/// transitions, then call `render()` or `export()`.
///
/// ```python
/// engine = Engine(width=1920, height=1080)
/// intro = engine.scene("intro")
/// intro.title("Welcome")
/// intro.play(...)
///
/// demo = engine.scene("demo")
/// demo.circle(1.0)
/// demo.play(...)
///
/// engine.sequence([intro, demo], Transition.cross_fade(0.5))
/// engine.render()
/// ```
#[pyclass(name = "Engine", module = "gaanim_core", frozen)]
pub struct PyEngine {
    inner: Arc<Mutex<EngineInner>>,
}

#[pymethods]
impl PyEngine {
    #[new]
    #[pyo3(signature = (width=1280, height=720, title=None, theme=None))]
    fn new(width: u32, height: u32, title: Option<String>, theme: Option<PyTheme>) -> Self {
        let title = title.unwrap_or_else(|| "Gaanim".to_string());
        let active_theme = theme.unwrap_or_else(PyTheme::DARK);
        let background = Some(active_theme.0.background);
        Self {
            inner: Arc::new(Mutex::new(EngineInner {
                scenes: Vec::new(),
                connections: Vec::new(),
                width,
                height,
                background,
                title,
                theme: active_theme,
            })),
        }
    }

    #[getter]
    fn width(&self) -> PyResult<u32> {
        Ok(lock_engine!(self.inner).width)
    }

    #[getter]
    fn height(&self) -> PyResult<u32> {
        Ok(lock_engine!(self.inner).height)
    }

    fn __repr__(&self) -> PyResult<String> {
        let inner = lock_engine!(self.inner);
        Ok(format!(
            "Engine({}x{}, \"{}\", scenes={})",
            inner.width,
            inner.height,
            inner.title,
            inner.scenes.len(),
        ))
    }

    /// Create a new scene and return a SceneBuilder for it.
    fn scene(&self, name: &str) -> PyResult<PySceneBuilder> {
        let mut inner = lock_engine!(self.inner);
        let index = inner.scenes.len();
        inner.scenes.push(SceneOps {
            name: name.to_string(),
            ops: Vec::new(),
            id_counter: 0,
        });
        Ok(PySceneBuilder {
            engine: self.inner.clone(),
            scene_index: index,
        })
    }

    /// Connect two scenes with a transition.
    fn connect(
        &self,
        from: &PySceneBuilder,
        to: &PySceneBuilder,
        transition: Option<PyTransitionType>,
    ) -> PyResult<()> {
        let mut inner = lock_engine!(self.inner);
        let trans = transition.unwrap_or(PyTransitionType::cut_transition());
        inner.connections.push(SceneConnection {
            from: from.scene_index,
            to: to.scene_index,
            transition: trans,
        });
        Ok(())
    }

    /// Connect a list of scenes sequentially with the same transition.
    #[pyo3(signature = (scenes, transition=None))]
    fn sequence(
        &self,
        scenes: Vec<PySceneBuilder>,
        transition: Option<PyTransitionType>,
    ) -> PyResult<()> {
        if scenes.len() < 2 {
            return Err(PyValueError::new_err(
                "sequence() requires at least 2 scenes",
            ));
        }
        let trans = transition.unwrap_or(PyTransitionType::cut_transition());
        let mut inner = lock_engine!(self.inner);
        for window in scenes.windows(2) {
            inner.connections.push(SceneConnection {
                from: window[0].scene_index,
                to: window[1].scene_index,
                transition: trans.clone(),
            });
        }
        Ok(())
    }

    /// Submit all scenes to the Gaanim host application (see `Scene.render`).
    fn render(&self) -> PyResult<()> {
        let mut inner = lock_engine!(self.inner);
        let all_ops = Self::drain_all_ops(&mut inner);
        let width = inner.width;
        let height = inner.height;
        let background = inner.background;
        drop(inner);

        let payload = crate::host::ReloadPayload {
            ops: all_ops,
            width,
            height,
            background,
        };
        if crate::host::send_to_host(payload) {
            Ok(())
        } else {
            Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Gaanim scenes can only be rendered inside the Gaanim application. \
                 Run your script with:  gaanim <script.py>",
            ))
        }
    }

    /// Export all scenes to a video file. Blocking.
    #[pyo3(signature = (
        output_path,
        fps = 60,
        width = None,
        height = None,
        transparent = None,
        aspect_ratio = None,
        quality = None,
        start_time = None,
        end_time = None,
        headless = true,
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
        headless: bool,
    ) -> PyResult<()> {
        let mut inner = lock_engine!(self.inner);
        let all_ops = Self::drain_all_ops(&mut inner);
        let w = width.unwrap_or(inner.width);
        let h = height.unwrap_or(inner.height);
        let background = inner.background;
        drop(inner);

        use gaanim_export::prelude::*;

        let mut config = ExportConfig::new(&output_path);
        config.fps = fps;
        config.width = w;
        config.height = h;
        config.headless = headless;

        if let Some(t) = transparent {
            config.transparent = t;
        }
        if let Some(ar) = aspect_ratio {
            let preset = match ar.to_lowercase().as_str() {
                "youtube" | "16:9" | "16_9" => AspectRatioPreset::Youtube,
                "tiktok" | "9:16" | "9_16" => AspectRatioPreset::TikTok,
                "instagram" | "1:1" | "1_1" => AspectRatioPreset::Instagram,
                _ => {
                    return Err(PyValueError::new_err(format!(
                        "Invalid aspect ratio preset: {}",
                        ar
                    )))
                }
            };
            config = config.with_aspect_ratio(preset);
        }
        if let Some(q) = quality {
            let preset = match q.to_lowercase().as_str() {
                "draft" => QualityPreset::Draft,
                "standard" => QualityPreset::Standard,
                "production" => QualityPreset::Production,
                _ => {
                    return Err(PyValueError::new_err(format!(
                        "Invalid quality preset: {}",
                        q
                    )))
                }
            };
            config = config.with_quality(preset);
        }

        config.start_time = start_time;
        config.end_time = end_time;

        py.detach(move || {
            let setup_world = move |world: &mut bevy::prelude::World| {
                runtime::replay_into(world, all_ops, w, h, background);
            };
            let result = if headless {
                export_scene_direct(config, setup_world)
            } else {
                export_scene(config, setup_world)
            };
            if let Err(e) = result {
                bevy::prelude::error!("Export error: {}", e);
            }
        });

        Ok(())
    }
}

impl PyEngine {
    /// Drains all scene ops into a single flat Vec, inserting SceneBegin/SceneEnd markers
    /// and SceneConnect ops for transitions between scenes.
    fn drain_all_ops(inner: &mut EngineInner) -> Vec<DeferredOp> {
        let mut all_ops = Vec::new();
        for scene in &mut inner.scenes {
            all_ops.push(DeferredOp::SceneBegin {
                name: scene.name.clone(),
            });
            all_ops.append(&mut scene.ops);
            all_ops.push(DeferredOp::SceneEnd);
        }
        // Emit connection ops after all scenes
        for conn in &inner.connections {
            all_ops.push(DeferredOp::SceneConnect {
                from_index: conn.from,
                to_index: conn.to,
                transition: conn.transition.0.clone(),
            });
        }
        all_ops
    }
}

/// A scene-scoped builder that accumulates deferred ops for one scene in an Engine.
#[pyclass(name = "SceneBuilder", module = "gaanim_core", frozen, from_py_object)]
#[derive(Debug, Clone)]
pub struct PySceneBuilder {
    engine: Arc<Mutex<EngineInner>>,
    scene_index: usize,
}

macro_rules! lock_scene {
    ($builder:expr) => {
        match $builder.engine.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return Err(pyo3::exceptions::PyRuntimeError::new_err(
                    "Engine mutex is poisoned",
                ))
            }
        }
    };
}

#[pymethods]
impl PySceneBuilder {
    fn __repr__(&self) -> PyResult<String> {
        let inner = lock_scene!(self);
        let scene = &inner.scenes[self.scene_index];
        Ok(format!(
            "SceneBuilder('{}', ops={})",
            scene.name,
            scene.ops.len()
        ))
    }

    #[getter]
    fn name(&self) -> PyResult<String> {
        let inner = lock_scene!(self);
        Ok(inner.scenes[self.scene_index].name.clone())
    }

    // ====== spawn helpers ======

    fn circle(&self, radius: f64) -> PyResult<PyMobject> {
        let mut inner = lock_scene!(self);
        let primary = inner.theme.0.primary;
        let scene = &mut inner.scenes[self.scene_index];
        let id = scene.next_id();
        let common = Self::default_common(primary);
        let spec = Arc::new(Mutex::new(MobjectSpec::Circle { common, radius }));
        let order = id.index() as u64;
        scene.ops.push(DeferredOp::Spawn {
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
        let primary = lock_scene!(self).theme.0.primary;
        self.spawn_with(MobjectSpec::Rectangle {
            common: Self::default_common(primary),
            width,
            height,
        })
    }

    fn square(&self, side: f64) -> PyResult<PyMobject> {
        let primary = lock_scene!(self).theme.0.primary;
        self.spawn_with(MobjectSpec::Square {
            common: Self::default_common(primary),
            side,
        })
    }

    fn dot(&self, radius: f64) -> PyResult<PyMobject> {
        let primary = lock_scene!(self).theme.0.primary;
        self.spawn_with(MobjectSpec::Dot {
            common: Self::default_common(primary),
            radius,
        })
    }

    fn ellipse(&self, rx: f64, ry: f64) -> PyResult<PyMobject> {
        let primary = lock_scene!(self).theme.0.primary;
        self.spawn_with(MobjectSpec::Ellipse {
            common: Self::default_common(primary),
            rx,
            ry,
        })
    }

    fn line(&self, x1: f64, y1: f64, x2: f64, y2: f64) -> PyResult<PyMobject> {
        let primary = lock_scene!(self).theme.0.primary;
        self.spawn_with(MobjectSpec::Line {
            common: CommonSpec {
                fill: None,
                stroke: Some((primary, 2.0)),
                z_index: 0,
                opacity: 1.0,
                transform: gaanim_math::SpatialTransform::default(),
                next_to: None,
                positioning_ops: Vec::new(),
            },
            start: (x1, y1),
            end: (x2, y2),
        })
    }

    fn arrow(&self, x1: f64, y1: f64, x2: f64, y2: f64) -> PyResult<PyMobject> {
        let primary = lock_scene!(self).theme.0.primary;
        self.spawn_with(MobjectSpec::Arrow {
            common: CommonSpec {
                fill: Some(primary),
                stroke: Some((primary, 2.0)),
                z_index: 0,
                opacity: 1.0,
                transform: gaanim_math::SpatialTransform::default(),
                next_to: None,
                positioning_ops: Vec::new(),
            },
            start: (x1, y1),
            end: (x2, y2),
        })
    }

    fn polygon(&self, points: Vec<(f64, f64)>) -> PyResult<PyMobject> {
        let primary = lock_scene!(self).theme.0.primary;
        self.spawn_with(MobjectSpec::Polygon {
            common: Self::default_common(primary),
            points,
        })
    }

    fn star(&self, n_points: u32, outer_radius: f64, inner_radius: f64) -> PyResult<PyMobject> {
        let primary = lock_scene!(self).theme.0.primary;
        self.spawn_with(MobjectSpec::Star {
            common: Self::default_common(primary),
            n_points,
            outer_radius,
            inner_radius,
        })
    }

    fn text(&self, content: &str, role: Option<&str>) -> PyResult<PyMobject> {
        let role = TextRoleKind::from_str(role.unwrap_or("body"));
        let primary = lock_scene!(self).theme.0.primary;
        self.spawn_with(MobjectSpec::Text {
            common: Self::default_common(primary),
            content: content.to_string(),
            role,
        })
    }

    fn title(&self, content: &str) -> PyResult<PyMobject> {
        let primary = lock_scene!(self).theme.0.primary;
        self.spawn_with(MobjectSpec::Text {
            common: Self::default_common(primary),
            content: content.to_string(),
            role: TextRoleKind::Title,
        })
    }

    fn subtitle(&self, content: &str) -> PyResult<PyMobject> {
        let primary = lock_scene!(self).theme.0.primary;
        self.spawn_with(MobjectSpec::Text {
            common: Self::default_common(primary),
            content: content.to_string(),
            role: TextRoleKind::Subtitle,
        })
    }

    fn body(&self, content: &str) -> PyResult<PyMobject> {
        let primary = lock_scene!(self).theme.0.primary;
        self.spawn_with(MobjectSpec::Text {
            common: Self::default_common(primary),
            content: content.to_string(),
            role: TextRoleKind::Body,
        })
    }

    fn equation(&self, formula: &str) -> PyResult<PyMobject> {
        let primary = lock_scene!(self).theme.0.primary;
        self.spawn_with(MobjectSpec::Equation {
            common: Self::default_common(primary),
            formula: formula.to_string(),
        })
    }

    fn background_rectangle(&self, width: f64, height: f64) -> PyResult<PyMobject> {
        let primary = lock_scene!(self).theme.0.primary;
        self.spawn_with(MobjectSpec::BackgroundRectangle {
            common: CommonSpec {
                fill: Some(primary),
                stroke: None,
                z_index: -10,
                opacity: 1.0,
                transform: gaanim_math::SpatialTransform::default(),
                next_to: None,
                positioning_ops: Vec::new(),
            },
            width,
            height,
        })
    }

    fn group(&self, children: Vec<PyMobject>) -> PyResult<PyMobject> {
        let mut inner = lock_scene!(self);
        let primary = inner.theme.0.primary;
        let scene = &mut inner.scenes[self.scene_index];
        let id = scene.next_id();
        let common = Self::default_common(primary);
        let children_specs = children
            .iter()
            .map(|c| (c.id, c.spec.clone(), c.creation_order))
            .collect();
        let spec = Arc::new(Mutex::new(MobjectSpec::Group {
            common,
            children: children_specs,
            layout_op: None,
        }));
        let order = id.index() as u64;
        scene.ops.push(DeferredOp::Spawn {
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

    fn ungroup(&self, group: &PyMobject) -> PyResult<()> {
        let mut inner = lock_scene!(self);
        inner.scenes[self.scene_index]
            .ops
            .push(DeferredOp::Ungroup { group: group.id });
        Ok(())
    }

    fn open_path(&self, points: Vec<(f64, f64)>) -> PyResult<PyMobject> {
        let primary = lock_scene!(self).theme.0.primary;
        self.spawn_with(MobjectSpec::OpenPath {
            common: CommonSpec {
                fill: None,
                stroke: Some((primary, 2.0)),
                z_index: 0,
                opacity: 1.0,
                transform: gaanim_math::SpatialTransform::default(),
                next_to: None, positioning_ops: Vec::new(),
            },
            points,
        })
    }

    fn curved_arrow(&self, x1: f64, y1: f64, x2: f64, y2: f64, angle: f64) -> PyResult<PyMobject> {
        let primary = lock_scene!(self).theme.0.primary;
        self.spawn_with(MobjectSpec::CurvedArrow {
            common: CommonSpec {
                fill: Some(primary),
                stroke: Some((primary, 2.5)),
                z_index: 0,
                opacity: 1.0,
                transform: gaanim_math::SpatialTransform::default(),
                next_to: None, positioning_ops: Vec::new(),
            },
            start: (x1, y1),
            end: (x2, y2),
            angle,
        })
    }

    fn vector(&self, x: f64, y: f64) -> PyResult<PyMobject> {
        self.arrow(0.0, 0.0, x, y)
    }

    fn brace(&self, x1: f64, y1: f64, x2: f64, y2: f64, height: f64) -> PyResult<PyMobject> {
        let primary = lock_scene!(self).theme.0.primary;
        self.spawn_with(MobjectSpec::Brace {
            common: CommonSpec {
                fill: None,
                stroke: Some((primary, 2.0)),
                z_index: 0,
                opacity: 1.0,
                transform: gaanim_math::SpatialTransform::default(),
                next_to: None, positioning_ops: Vec::new(),
            },
            start: (x1, y1),
            end: (x2, y2),
            height,
        })
    }

    fn number_line(&self, x_range: (f64, f64, f64), include_labels: bool, vertical: bool) -> PyResult<PyMobject> {
        let primary = lock_scene!(self).theme.0.primary;
        self.spawn_with(MobjectSpec::NumberLine {
            common: CommonSpec {
                fill: None,
                stroke: Some((primary, 2.0)),
                z_index: 0,
                opacity: 1.0,
                transform: gaanim_math::SpatialTransform::default(),
                next_to: None, positioning_ops: Vec::new(),
            },
            x_range,
            include_labels,
            vertical,
        })
    }

    fn axes(&self, x_range: (f64, f64, f64), y_range: (f64, f64, f64), include_labels: bool) -> PyResult<PyMobject> {
        let primary = lock_scene!(self).theme.0.primary;
        self.spawn_with(MobjectSpec::Axes {
            common: CommonSpec {
                fill: None,
                stroke: Some((primary, 2.0)),
                z_index: 0,
                opacity: 1.0,
                transform: gaanim_math::SpatialTransform::default(),
                next_to: None, positioning_ops: Vec::new(),
            },
            x_range,
            y_range,
            include_labels,
        })
    }

    fn parametric_curve(&self, _py: Python<'_>, t_range: (f64, f64), steps: usize, f: &Bound<'_, PyAny>) -> PyResult<PyMobject> {
        let (t_min, t_max) = t_range;
        let mut points = Vec::with_capacity(steps + 1);
        for i in 0..=steps {
            let t = t_min + (t_max - t_min) * (i as f64 / steps as f64);
            let res = f.call1((t,))?;
            let point: (f64, f64) = res.extract()?;
            points.push(point);
        }
        self.open_path(points)
    }

    fn function_graph(&self, _py: Python<'_>, x_range: (f64, f64), steps: usize, f: &Bound<'_, PyAny>) -> PyResult<PyMobject> {
        let (x_min, x_max) = x_range;
        let mut points = Vec::with_capacity(steps + 1);
        for i in 0..=steps {
            let x = x_min + (x_max - x_min) * (i as f64 / steps as f64);
            let res = f.call1((x,))?;
            let y: f64 = res.extract()?;
            points.push((x, y));
        }
        self.open_path(points)
    }

    fn labeled_arrow(&self, x1: f64, y1: f64, x2: f64, y2: f64, label: String, spacing: f64) -> PyResult<PyMobject> {
        let arrow_obj = self.arrow(x1, y1, x2, y2)?;

        let dx = x2 - x1;
        let dy = y2 - y1;
        let len = (dx * dx + dy * dy).sqrt();

        let mut label_pos = ( (x1 + x2) * 0.5, (y1 + y2) * 0.5 );
        if len > 1e-6 {
            let nx = -dy / len;
            let ny = dx / len;
            label_pos.0 += nx * spacing;
            label_pos.1 += ny * spacing;
        }

        let label_obj = self.text(&label, None)?;
        if let Ok(mut spec_guard) = label_obj.spec.lock() {
            spec_guard.common_mut().transform = spec_guard.common_mut().transform.shift_2d(label_pos.0, label_pos.1);
        }

        self.group(vec![arrow_obj, label_obj])
    }

    fn labeled_brace(&self, x1: f64, y1: f64, x2: f64, y2: f64, label: String, height: f64, spacing: f64) -> PyResult<PyMobject> {
        let brace_obj = self.brace(x1, y1, x2, y2, height)?;

        let dx = x2 - x1;
        let dy = y2 - y1;
        let len = (dx * dx + dy * dy).sqrt();

        let mut label_pos = ( (x1 + x2) * 0.5, (y1 + y2) * 0.5 );
        if len > 1e-6 {
            let nx = -dy / len;
            let ny = dx / len;
            label_pos.0 += nx * (height + spacing);
            label_pos.1 += ny * (height + spacing);
        }

        let label_obj = self.text(&label, None)?;
        if let Ok(mut spec_guard) = label_obj.spec.lock() {
            spec_guard.common_mut().transform = spec_guard.common_mut().transform.shift_2d(label_pos.0, label_pos.1);
        }

        self.group(vec![brace_obj, label_obj])
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
            if let Ok(spec) = item.extract::<PyAnimationSpec>() {
                specs.push(spec.inner);
            } else if item.is_none() {
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
            let mut inner = lock_scene!(self);
            inner.scenes[self.scene_index]
                .ops
                .push(DeferredOp::Play { specs });
        }
        Ok(())
    }

    fn wait(&self, duration: f64) -> PyResult<()> {
        let mut inner = lock_scene!(self);
        inner.scenes[self.scene_index]
            .ops
            .push(DeferredOp::Wait { duration });
        Ok(())
    }

    fn slide(&self) -> PyResult<()> {
        let mut inner = lock_scene!(self);
        inner.scenes[self.scene_index].ops.push(DeferredOp::Slide);
        Ok(())
    }

    // ====== selection ======

    fn select(&self, parent: &PyMobject, query: &str) -> PyResult<PySelection> {
        let mut inner = lock_scene!(self);
        let scene = &mut inner.scenes[self.scene_index];
        let selection_id = scene.next_id();
        scene.ops.push(DeferredOp::Select {
            parent: parent.id,
            query: query.to_string(),
            selection: selection_id,
        });
        Ok(PySelection {
            parent: parent.id,
            query: query.to_string(),
            id: selection_id,
        })
    }

    fn fill_selection(&self, selection: &PySelection, color: &PyColor) -> PyResult<()> {
        let mut inner = lock_scene!(self);
        inner.scenes[self.scene_index]
            .ops
            .push(DeferredOp::SelectionFill {
                selection: selection.id,
                color: color.0,
            });
        Ok(())
    }

    fn set_stroke_selection(
        &self,
        selection: &PySelection,
        color: &PyColor,
        width: f64,
    ) -> PyResult<()> {
        let mut inner = lock_scene!(self);
        inner.scenes[self.scene_index]
            .ops
            .push(DeferredOp::SelectionStroke {
                selection: selection.id,
                color: color.0,
                width,
            });
        Ok(())
    }

    // ====== boolean ops ======

    fn union(&self, a: &PyMobject, b: &PyMobject) -> PyResult<PyMobject> {
        self.boolean_op(a, b, gaanim_objects::boolean::BooleanOp::Union)
    }

    fn intersection(&self, a: &PyMobject, b: &PyMobject) -> PyResult<PyMobject> {
        self.boolean_op(a, b, gaanim_objects::boolean::BooleanOp::Intersection)
    }

    fn difference(&self, a: &PyMobject, b: &PyMobject) -> PyResult<PyMobject> {
        self.boolean_op(a, b, gaanim_objects::boolean::BooleanOp::Difference)
    }

    fn exclusion(&self, a: &PyMobject, b: &PyMobject) -> PyResult<PyMobject> {
        self.boolean_op(a, b, gaanim_objects::boolean::BooleanOp::Exclusion)
    }
}

impl PySceneBuilder {
    fn default_common(primary_color: peniko::Color) -> CommonSpec {
        CommonSpec {
            fill: Some(primary_color),
            stroke: None,
            z_index: 0,
            opacity: 1.0,
            transform: gaanim_math::SpatialTransform::default(),
            next_to: None,
            positioning_ops: Vec::new(),
        }
    }

    fn spawn_with(&self, spec: MobjectSpec) -> PyResult<PyMobject> {
        let mut inner = lock_scene!(self);
        let scene = &mut inner.scenes[self.scene_index];
        let id = scene.next_id();
        let order = id.index() as u64;
        let spec = Arc::new(Mutex::new(spec));
        scene.ops.push(DeferredOp::Spawn {
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

    fn boolean_op(
        &self,
        a: &PyMobject,
        b: &PyMobject,
        op: gaanim_objects::boolean::BooleanOp,
    ) -> PyResult<PyMobject> {
        use crate::scene::{bezpath_to_contours, contours_to_bezpath};
        let path_a = contours_to_bezpath(&a.spec.lock().unwrap().to_contours());
        let path_b = contours_to_bezpath(&b.spec.lock().unwrap().to_contours());
        let result = gaanim_objects::boolean::apply(&path_a, &path_b, op);
        let mut combined: Vec<Vec<[f64; 2]>> = Vec::new();
        for path in &result.paths {
            for ring in bezpath_to_contours(path) {
                combined.push(ring);
            }
        }
        let primary = lock_scene!(self).theme.0.primary;
        self.spawn_with(MobjectSpec::BooleanResult {
            common: CommonSpec {
                fill: Some(primary),
                stroke: Some((primary, 2.0)),
                z_index: 0,
                opacity: 1.0,
                transform: gaanim_math::SpatialTransform::default(),
                next_to: None,
                positioning_ops: Vec::new(),
            },
            contours: combined,
        })
    }
}
