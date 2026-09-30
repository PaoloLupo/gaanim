//! `Emitter` and the `scene.fx.particles` / `scene.fx.confetti` helpers.

use gaanim_api::canvas::{Emitter, EmitterShape, ParticleColors, ParticleOptions, ParticleShape};
use gaanim_core::glam::DVec2;
use gaanim_core::peniko::Brush;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;

use crate::brush::PyBrush;
use crate::color::PyColor;
use crate::pydrawable::PyDrawable;

type SharedScene = std::sync::Arc<std::sync::Mutex<gaanim_api::canvas::SceneModel>>;

/// Where `scene.fx.particles` releases its particles.
#[pyclass(name = "Emitter", module = "gaanim_core", frozen, skip_from_py_object)]
#[derive(Clone, Debug)]
pub struct PyEmitter(pub Emitter);

fn checked(shape: EmitterShape) -> PyResult<PyEmitter> {
    Emitter::new(shape)
        .map(PyEmitter)
        .map_err(PyValueError::new_err)
}

#[pymethods]
impl PyEmitter {
    /// Every particle leaves from one point.
    #[staticmethod]
    fn point() -> PyResult<Self> {
        checked(EmitterShape::Point)
    }

    /// Particles leave from inside a disc, or from its rim with `edge`.
    #[staticmethod]
    #[pyo3(signature = (radius, *, edge=false))]
    fn circle(radius: f64, edge: bool) -> PyResult<Self> {
        checked(EmitterShape::Circle { radius, edge })
    }

    /// Particles leave from inside a `width` x `height` rectangle.
    #[staticmethod]
    fn rect(width: f64, height: f64) -> PyResult<Self> {
        checked(EmitterShape::Rect { width, height })
    }

    /// Particles leave from along a segment of `length`, turned by `angle`.
    #[staticmethod]
    #[pyo3(signature = (length, angle=0.0))]
    fn line(length: f64, angle: f64) -> PyResult<Self> {
        checked(EmitterShape::Line { length, angle })
    }

    /// The same emitter at a scene point, or following a drawable.
    #[pyo3(signature = (target, *, offset=(0.0, 0.0)))]
    fn at(&self, target: &Bound<'_, PyAny>, offset: (f64, f64)) -> PyResult<Self> {
        let emitter = self.0.clone();
        if let Ok(drawable) = target.cast::<PyDrawable>() {
            return emitter
                .at_drawable(&drawable.borrow().0, offset.0, offset.1)
                .map(Self)
                .map_err(PyValueError::new_err);
        }
        let (x, y) = target.extract::<(f64, f64)>().map_err(|_| {
            PyTypeError::new_err("an emitter position is an (x, y) point or a Drawable")
        })?;
        emitter
            .at_point(x + offset.0, y + offset.1)
            .map(Self)
            .map_err(PyValueError::new_err)
    }

    fn __repr__(&self) -> String {
        let place = if self.0.anchor.is_some() {
            "following a drawable".to_string()
        } else {
            format!("at ({}, {})", self.0.position.x, self.0.position.y)
        };
        format!("Emitter({:?} {place})", self.0.shape)
    }
}

/// An emitter from `Emitter`, an `(x, y)` point or a drawable to follow.
fn emitter_of(value: &Bound<'_, PyAny>) -> PyResult<Emitter> {
    if let Ok(emitter) = value.cast::<PyEmitter>() {
        return Ok(emitter.get().0.clone());
    }
    PyEmitter(Emitter::new(EmitterShape::Point).map_err(PyValueError::new_err)?)
        .at(value, (0.0, 0.0))
        .map(|emitter| emitter.0)
}

/// A `(low, high)` range from a number or a pair.
fn range_of(name: &str, value: &Bound<'_, PyAny>) -> PyResult<(f64, f64)> {
    if let Ok(value) = value.extract::<f64>() {
        return Ok((value, value));
    }
    value
        .extract::<(f64, f64)>()
        .map_err(|_| PyTypeError::new_err(format!("{name} must be a number or a (low, high) pair")))
}

/// Particle colors from a color, a sequence of colors or a gradient `Brush`.
fn colors_of(value: &Bound<'_, PyAny>) -> PyResult<ParticleColors> {
    use gaanim_core::peniko::color::Srgb;
    if let Ok(color) = value.extract::<PyColor>() {
        return Ok(ParticleColors::Palette(vec![color.0]));
    }
    if let Ok(brush) = value.cast::<PyBrush>() {
        return Ok(match &brush.borrow().0 {
            Brush::Solid(color) => ParticleColors::Palette(vec![*color]),
            Brush::Gradient(gradient) => ParticleColors::Gradient(
                gradient
                    .stops
                    .iter()
                    .map(|stop| (stop.offset, stop.color.to_alpha_color::<Srgb>()))
                    .collect(),
            ),
            _ => {
                return Err(PyValueError::new_err(
                    "particle colors need a solid or gradient Brush",
                ));
            }
        });
    }
    let colors = value.extract::<Vec<PyColor>>().map_err(|_| {
        PyTypeError::new_err("color must be a color, a sequence of colors or a gradient Brush")
    })?;
    Ok(ParticleColors::Palette(
        colors.into_iter().map(|color| color.0).collect(),
    ))
}

fn shape_of(name: &str) -> PyResult<ParticleShape> {
    ParticleShape::parse(name).ok_or_else(|| {
        PyValueError::new_err(format!(
            "unknown particle shape {name:?}; expected \"circle\", \"square\", \"rect\", \"triangle\" or \"streak\""
        ))
    })
}

/// Keyword arguments of `Fx.particles`.
pub(crate) struct ParticleArgs<'py> {
    pub rate: f64,
    pub duration: Option<f64>,
    pub lifetime: Bound<'py, PyAny>,
    pub speed: Bound<'py, PyAny>,
    pub direction: f64,
    pub spread: f64,
    pub gravity: (f64, f64),
    pub drag: f64,
    pub size: Bound<'py, PyAny>,
    pub size_end: f64,
    pub fade: f64,
    pub spin: Bound<'py, PyAny>,
    pub flutter: f64,
    pub shape: String,
    pub color: Bound<'py, PyAny>,
    pub seed: u64,
}

pub(crate) fn particles(
    scene: &SharedScene,
    emitter: &Bound<'_, PyAny>,
    args: ParticleArgs<'_>,
) -> PyResult<PyDrawable> {
    let spin = match args.spin.extract::<f64>() {
        // A single spin turns either way up to that speed.
        Ok(spin) => (-spin.abs(), spin.abs()),
        Err(_) => range_of("spin", &args.spin)?,
    };
    let options = ParticleOptions {
        rate: args.rate,
        duration: args.duration,
        lifetime: range_of("lifetime", &args.lifetime)?,
        speed: range_of("speed", &args.speed)?,
        direction: args.direction,
        spread: args.spread,
        gravity: DVec2::new(args.gravity.0, args.gravity.1),
        drag: args.drag,
        size: range_of("size", &args.size)?,
        size_end: args.size_end,
        fade: args.fade,
        spin,
        flutter: args.flutter,
        shape: shape_of(&args.shape)?,
        colors: colors_of(&args.color)?,
        seed: args.seed,
    };
    let emitter = emitter_of(emitter)?;
    scene
        .lock()
        .expect("scene canvas poisoned")
        .particles(&emitter, options)
        .map(PyDrawable)
        .map_err(PyValueError::new_err)
}

/// Keyword arguments of `Fx.confetti`.
pub(crate) struct ConfettiArgs<'py> {
    pub count: u32,
    pub seed: u64,
    pub colors: Option<Bound<'py, PyAny>>,
    pub speed: Option<Bound<'py, PyAny>>,
    pub direction: f64,
    pub spread: f64,
    pub gravity: (f64, f64),
    pub lifetime: Option<Bound<'py, PyAny>>,
    pub size: Option<Bound<'py, PyAny>>,
}

pub(crate) fn confetti(
    scene: &SharedScene,
    origin: &Bound<'_, PyAny>,
    args: ConfettiArgs<'_>,
) -> PyResult<PyDrawable> {
    let mut options = ParticleOptions::confetti();
    options.seed = args.seed;
    options.direction = args.direction;
    options.spread = args.spread;
    options.gravity = DVec2::new(args.gravity.0, args.gravity.1);
    if let Some(colors) = &args.colors {
        options.colors = colors_of(colors)?;
    }
    if let Some(speed) = &args.speed {
        options.speed = range_of("speed", speed)?;
    }
    if let Some(lifetime) = &args.lifetime {
        options.lifetime = range_of("lifetime", lifetime)?;
    }
    if let Some(size) = &args.size {
        options.size = range_of("size", size)?;
    }
    let emitter = emitter_of(origin)?;
    scene
        .lock()
        .expect("scene canvas poisoned")
        .confetti(&emitter, args.count, options)
        .map(PyDrawable)
        .map_err(PyValueError::new_err)
}
