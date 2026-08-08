//! Thin PyDrawable wrapper over gaanim_api DrawableHandle.

use std::sync::atomic::{AtomicU64, Ordering};

use pyo3::exceptions::{PyKeyError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyTuple;

use crate::brush::PyPaint;
use crate::color::PyColor;
use crate::pylayout::{PyAnchor, PyDirection};
use crate::updater::PyUpdater;

static PYTHON_UPDATER_KEY_COUNTER: AtomicU64 = AtomicU64::new(1);

#[pyclass(name = "Anim", module = "gaanim_core", from_py_object)]
#[derive(Clone, Debug)]
pub struct PyCanvasAnim {
    pub inner: gaanim_api::canvas::Anim,
}

#[pymethods]
impl PyCanvasAnim {
    fn duration(&self, d: f64) -> Self {
        Self {
            inner: self.inner.clone().duration(d),
        }
    }

    fn ease(&self, name: &str) -> Self {
        Self {
            inner: self.inner.clone().ease(name),
        }
    }

    fn rate(&self, name: &str) -> Self {
        self.ease(name)
    }

    fn delay(&self, sec: f64) -> Self {
        Self {
            inner: self.inner.clone().delay(sec),
        }
    }

    fn steps(&self, n: u32) -> Self {
        Self {
            inner: self.inner.clone().steps(n),
        }
    }

    fn spring(&self) -> Self {
        Self {
            inner: self.inner.clone().spring(),
        }
    }

    fn smooth(&self) -> Self {
        Self {
            inner: self.inner.clone().smooth(),
        }
    }

    fn linear(&self) -> Self {
        Self {
            inner: self.inner.clone().linear(),
        }
    }

    fn lag_ratio(&self, value: f64) -> Self {
        Self {
            inner: self.inner.clone().lag_ratio(value),
        }
    }

    fn stroke_width(&self, value: f64) -> Self {
        Self {
            inner: self.inner.clone().stroke_width(value),
        }
    }

    fn with_pen_tip(&self) -> Self {
        Self {
            inner: self.inner.clone().with_pen_tip(),
        }
    }

    fn pivot(&self, x: f64, y: f64) -> Self {
        Self {
            inner: self.inner.clone().pivot(x, y),
        }
    }

    fn about_point(&self, x: f64, y: f64) -> Self {
        self.pivot(x, y)
    }
}

#[pyclass(name = "Drawable", module = "gaanim_core", from_py_object)]
#[derive(Clone)]
pub struct PyDrawable(pub gaanim_api::canvas::DrawableHandle);

#[pyclass(name = "FragmentSelection", module = "gaanim_core", from_py_object)]
#[derive(Clone)]
pub struct PyFragmentSelection(pub gaanim_api::canvas::FragmentSelection);

#[pymethods]
impl PyFragmentSelection {
    /// Instantly colors the selected glyphs.
    fn fill(&self, color: PyColor) -> Self {
        Self(self.0.clone().fill(color.0))
    }

    /// Emphasizes only the selected glyphs.
    #[pyo3(signature = (duration=None))]
    fn indicate(&self, duration: Option<f64>) -> Self {
        Self(self.0.clone().indicate(duration))
    }

    /// Reveal the selected fragment with ``fade``, ``wipe``, or ``from_below``.
    #[pyo3(signature = (style="fade", duration=None))]
    fn reveal(&self, style: &str, duration: Option<f64>) -> PyResult<Self> {
        let style = match style {
            "fade" => gaanim_api::canvas::FragmentRevealStyle::Fade,
            "wipe" => gaanim_api::canvas::FragmentRevealStyle::Wipe,
            "from_below" => gaanim_api::canvas::FragmentRevealStyle::FromBelow,
            _ => {
                return Err(PyValueError::new_err(
                    "style must be 'fade', 'wipe', or 'from_below'",
                ))
            }
        };
        Ok(Self(self.0.clone().reveal(style, duration)))
    }

    /// Draw a strikethrough over this fragment and fade it out.
    #[pyo3(signature = (duration=None))]
    fn cancel(&self, duration: Option<f64>) -> Self {
        Self(self.0.clone().cancel(duration))
    }

    /// Animates the selected glyphs to `color`.
    #[pyo3(signature = (color, duration=None))]
    fn color_to(&self, color: PyColor, duration: Option<f64>) -> Self {
        Self(self.0.clone().color_to(color.0, duration))
    }

    /// Morphs this fragment into another selected fragment.
    #[pyo3(signature = (target, duration=None))]
    fn transform_to(&self, target: &PyFragmentSelection, duration: Option<f64>) -> Self {
        Self(self.0.clone().transform_to(&target.0, duration))
    }
}

#[pymethods]
impl PyDrawable {
    /// Return a named source group or path from an imported SVG or glTF.
    fn part(&self, id: &str) -> PyResult<Self> {
        if id.is_empty() {
            return Err(PyKeyError::new_err("part selector must not be empty"));
        }
        match self.0.part(id) {
            Ok(part) => Ok(Self(part)),
            Err(gaanim_api::canvas::SvgPartError::NotSvg) => Err(PyValueError::new_err(
                "this drawable has no named SVG or glTF parts",
            )),
            Err(error @ gaanim_api::canvas::SvgPartError::Unknown { .. }) => {
                Err(PyKeyError::new_err(error.to_string()))
            }
        }
    }

    fn parts(&self, py: Python<'_>) -> PyResult<Py<PyTuple>> {
        Ok(PyTuple::new(py, self.0.parts())?.unbind())
    }

    fn animations(&self, py: Python<'_>) -> PyResult<Py<PyTuple>> {
        Ok(PyTuple::new(py, self.0.animations())?.unbind())
    }

    #[pyo3(signature = (name, *, duration=None, speed=1.0, r#loop=false, reverse=false, transition=0.0, start_time=0.0))]
    fn animation(
        &self,
        name: &str,
        duration: Option<f64>,
        speed: f64,
        r#loop: bool,
        reverse: bool,
        transition: f64,
        start_time: f64,
    ) -> PyResult<PyCanvasAnim> {
        self.0
            .animation(
                name, duration, speed, r#loop, reverse, transition, start_time,
            )
            .map(|inner| PyCanvasAnim { inner })
            .map_err(|error| match error {
                gaanim_api::canvas::GltfAnimationError::Unknown { .. } => {
                    PyKeyError::new_err(error.to_string())
                }
                _ => PyValueError::new_err(error.to_string()),
            })
    }

    fn fill(&self, paint: PyPaint) -> Self {
        Self(self.0.clone().fill_brush(paint.0))
    }
    fn no_fill(&self) -> Self {
        Self(self.0.clone().no_fill())
    }
    fn stroke(&self, paint: PyPaint, width: f64) -> Self {
        Self(self.0.clone().stroke_brush(paint.0, width))
    }
    fn no_stroke(&self) -> Self {
        Self(self.0.clone().no_stroke())
    }
    /// Add a cached soft outer glow.
    #[pyo3(signature = (color, radius=16.0, intensity=1.0))]
    fn glow(&self, color: PyColor, radius: f64, intensity: f32) -> PyResult<Self> {
        if !radius.is_finite() || radius <= 0.0 {
            return Err(PyValueError::new_err("radius must be finite and positive"));
        }
        if !intensity.is_finite() || intensity <= 0.0 {
            return Err(PyValueError::new_err(
                "intensity must be finite and positive",
            ));
        }
        Ok(Self(self.0.clone().glow(color.0, radius, intensity)))
    }
    /// Apply a cached soft vector blur.
    #[pyo3(signature = (sigma=4.0))]
    fn blur(&self, sigma: f64) -> PyResult<Self> {
        if !sigma.is_finite() || sigma <= 0.0 {
            return Err(PyValueError::new_err("sigma must be finite and positive"));
        }
        Ok(Self(self.0.clone().blur(sigma)))
    }
    /// Add a cached soft shadow behind the drawable.
    #[pyo3(signature = (color, x=8.0, y=-8.0, blur=6.0))]
    fn shadow(&self, color: PyColor, x: f64, y: f64, blur: f64) -> PyResult<Self> {
        if !x.is_finite() || !y.is_finite() {
            return Err(PyValueError::new_err("shadow offset must be finite"));
        }
        if !blur.is_finite() || blur < 0.0 {
            return Err(PyValueError::new_err(
                "shadow blur must be finite and non-negative",
            ));
        }
        Ok(Self(self.0.clone().shadow(
            color.0,
            gaanim_core::glam::DVec2::new(x, y),
            blur,
        )))
    }
    /// Remove glow, blur, and shadow from the drawable.
    fn no_effects(&self) -> Self {
        Self(self.0.clone().no_effects())
    }
    /// Clip this drawable to another drawable's vector outline.
    #[pyo3(signature = (mask, rule="nonzero"))]
    fn clip(&self, mask: &PyDrawable, rule: &str) -> PyResult<Self> {
        let rule = match rule {
            "nonzero" => gaanim_core::peniko::Fill::NonZero,
            "evenodd" | "even_odd" => gaanim_core::peniko::Fill::EvenOdd,
            _ => {
                return Err(PyValueError::new_err("rule must be 'nonzero' or 'evenodd'"));
            }
        };
        Ok(Self(self.0.clone().clip(&mask.0, rule)))
    }
    /// Remove the clipping mask from this drawable.
    fn no_clip(&self) -> Self {
        Self(self.0.clone().no_clip())
    }
    /// Colors every matching fragment of a text or equation drawable.
    ///
    /// Matching is case-insensitive and ignores mathematical spacing and
    /// sub/superscript markers. Later calls take precedence on overlaps.
    fn color_by(&self, fragment: &str, color: PyColor) -> PyResult<Self> {
        if fragment.trim().is_empty() {
            return Err(PyValueError::new_err("fragment must not be empty"));
        }
        Ok(Self(self.0.clone().color_by(fragment, color.0)))
    }
    /// Select matching glyphs for styling or an isolated animation.
    #[pyo3(signature = (fragment, occurrence=None))]
    fn select(&self, fragment: &str, occurrence: Option<usize>) -> PyResult<PyFragmentSelection> {
        if fragment.trim().is_empty() {
            return Err(PyValueError::new_err("fragment must not be empty"));
        }
        let selection = match occurrence {
            Some(occurrence) => self.0.select_nth(fragment, occurrence),
            None => self.0.select(fragment),
        };
        Ok(PyFragmentSelection(selection))
    }
    /// Select a named fragment supplied through ``Scene.equation(tags=...)``.
    fn tag(&self, name: &str) -> PyResult<PyFragmentSelection> {
        self.0
            .tag(name)
            .map(PyFragmentSelection)
            .ok_or_else(|| PyValueError::new_err(format!("unknown fragment tag '{name}'")))
    }
    /// Pulse a named equation term without selecting its source fragment again.
    #[pyo3(signature = (name, duration=None))]
    fn indicate_tag(&self, name: &str, duration: Option<f64>) -> PyResult<Self> {
        let tag = self
            .0
            .tag(name)
            .ok_or_else(|| PyValueError::new_err(format!("unknown fragment tag '{name}'")))?;
        tag.indicate(duration);
        Ok(Self(self.0.clone()))
    }
    /// Strike through and remove one named semantic term.
    #[pyo3(signature = (name, duration=None))]
    fn cancel_term(&self, name: &str, duration: Option<f64>) -> PyResult<Self> {
        let tag = self
            .0
            .tag(name)
            .ok_or_else(|| PyValueError::new_err(format!("unknown fragment tag '{name}'")))?;
        tag.cancel(duration);
        Ok(Self(self.0.clone()))
    }
    /// Reveal a raw equation fragment with ``fade``, ``wipe``, or ``from_below``.
    #[pyo3(signature = (fragment, *, style="fade", duration=None, occurrence=None))]
    fn reveal_fragment(
        &self,
        fragment: &str,
        style: &str,
        duration: Option<f64>,
        occurrence: Option<usize>,
    ) -> PyResult<Self> {
        if fragment.trim().is_empty() {
            return Err(PyValueError::new_err("fragment must not be empty"));
        }
        let style = match style {
            "fade" => gaanim_api::canvas::FragmentRevealStyle::Fade,
            "wipe" => gaanim_api::canvas::FragmentRevealStyle::Wipe,
            "from_below" => gaanim_api::canvas::FragmentRevealStyle::FromBelow,
            _ => {
                return Err(PyValueError::new_err(
                    "style must be 'fade', 'wipe', or 'from_below'",
                ))
            }
        };
        let selection = match occurrence {
            Some(occurrence) => self.0.select_nth(fragment, occurrence),
            None => self.0.select(fragment),
        };
        selection.reveal(style, duration);
        Ok(Self(self.0.clone()))
    }
    fn opacity(&self, op: f32) -> Self {
        Self(self.0.clone().opacity(op))
    }
    fn z_index(&self, z: i32) -> Self {
        Self(self.0.clone().z_index(z))
    }
    fn at(&self, x: f64, y: f64) -> Self {
        Self(self.0.clone().at(x, y))
    }
    fn at_3d(&self, x: f64, y: f64, z: f64) -> Self {
        Self(self.0.clone().at_3d(x, y, z))
    }
    fn billboard(&self) -> Self {
        Self(self.0.clone().billboard())
    }
    fn hud(&self) -> Self {
        Self(self.0.clone().hud())
    }
    fn scaled(&self, factor: f64) -> Self {
        Self(self.0.clone().scaled(factor))
    }
    fn scaled_3d(&self, x: f64, y: f64, z: f64) -> Self {
        Self(self.0.clone().scaled_3d(x, y, z))
    }
    fn rotated(&self, radians: f64) -> Self {
        Self(self.0.clone().rotated(radians))
    }
    fn rotated_3d(&self, x: f64, y: f64, z: f64) -> Self {
        Self(self.0.clone().rotated_3d(x, y, z))
    }
    fn with_pivot(&self, x: f64, y: f64) -> Self {
        Self(self.0.clone().with_pivot(x, y))
    }
    fn with_pivot_3d(&self, x: f64, y: f64, z: f64) -> Self {
        Self(self.0.clone().with_pivot_3d(x, y, z))
    }
    fn pivot(&self, x: f64, y: f64) -> Self {
        Self(self.0.clone().pivot(x, y))
    }
    fn at_anchor(&self, x: f64, y: f64, anchor: &PyAnchor) -> Self {
        Self(self.0.clone().at_anchor(x, y, anchor.0))
    }
    #[pyo3(signature = (reference, direction, spacing=24.0, aligned_edge=None))]
    fn next_to(
        &self,
        reference: &PyDrawable,
        direction: &PyDirection,
        spacing: f64,
        aligned_edge: Option<&PyAnchor>,
    ) -> Self {
        let aligned_edge = aligned_edge
            .map(|anchor| anchor.0)
            .unwrap_or(gaanim_api::canvas::Anchor::Center);
        Self(
            self.0
                .clone()
                .next_to_aligned(&reference.0, direction.0, spacing, aligned_edge),
        )
    }
    #[pyo3(signature = (reference, target_anchor, reference_anchor=None))]
    fn align_to(
        &self,
        reference: &PyDrawable,
        target_anchor: &PyAnchor,
        reference_anchor: Option<&PyAnchor>,
    ) -> Self {
        let reference_anchor = reference_anchor
            .map(|anchor| anchor.0)
            .unwrap_or(target_anchor.0);
        Self(
            self.0
                .clone()
                .align_to(&reference.0, target_anchor.0, reference_anchor),
        )
    }
    #[pyo3(signature = (direction, buff=24.0))]
    fn to_edge(&self, direction: &PyDirection, buff: f64) -> Self {
        Self(self.0.clone().to_edge(direction.0, buff))
    }
    #[pyo3(signature = (corner, buff=24.0))]
    fn to_corner(&self, corner: &PyAnchor, buff: f64) -> Self {
        Self(self.0.clone().to_corner(corner.0, buff))
    }
    #[pyo3(signature = (gap=24.0, align=None))]
    fn vstack(&self, gap: f64, align: Option<&PyAnchor>) -> Self {
        Self(
            self.0.clone().vstack(
                gap,
                align
                    .map(|anchor| anchor.0)
                    .unwrap_or(gaanim_api::canvas::Anchor::Left),
            ),
        )
    }
    #[pyo3(signature = (gap=24.0, align=None))]
    fn hstack(&self, gap: f64, align: Option<&PyAnchor>) -> Self {
        Self(
            self.0.clone().hstack(
                gap,
                align
                    .map(|anchor| anchor.0)
                    .unwrap_or(gaanim_api::canvas::Anchor::Bottom),
            ),
        )
    }

    fn r#move(&self, dx: f64, dy: f64) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.r#move(dx, dy),
        }
    }
    fn move_to(&self, x: f64, y: f64) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.move_to(x, y),
        }
    }
    fn move_3d(&self, dx: f64, dy: f64, dz: f64) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.move_3d(dx, dy, dz),
        }
    }
    fn move_to_3d(&self, x: f64, y: f64, z: f64) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.move_to_3d(x, y, z),
        }
    }
    fn glide_to(&self, x: f64, y: f64) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.glide_to(x, y),
        }
    }
    fn scale(&self, factor: f64) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.scale(factor),
        }
    }
    fn scale_to_3d(&self, x: f64, y: f64, z: f64) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.scale_to_3d(x, y, z),
        }
    }
    fn rotate(&self, rad: f64) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.rotate(rad),
        }
    }
    fn rotate_by_3d(&self, axis: &str, radians: f64) -> PyResult<PyCanvasAnim> {
        self.0
            .rotate_by_3d(axis, radians)
            .map(|inner| PyCanvasAnim { inner })
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }
    fn rotate_to_3d(&self, x: f64, y: f64, z: f64) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.rotate_to_3d(x, y, z),
        }
    }
    #[pyo3(signature = (duration=None))]
    fn fade_in(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.fade_in(duration),
        }
    }
    /// Fade in while moving from ``direction`` (for example ``Direction.DOWN``).
    #[pyo3(signature = (direction, distance=48.0, duration=None))]
    fn fade_in_from(
        &self,
        direction: &PyDirection,
        distance: f64,
        duration: Option<f64>,
    ) -> PyResult<PyCanvasAnim> {
        if !distance.is_finite() || distance < 0.0 {
            return Err(PyValueError::new_err(
                "distance must be a finite non-negative number",
            ));
        }
        Ok(PyCanvasAnim {
            inner: self.0.fade_in_from(direction.0.clone(), distance, duration),
        })
    }
    #[pyo3(signature = (duration=None))]
    fn fade_out(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.fade_out(duration),
        }
    }
    fn fade_to(&self, alpha: f32) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.fade_to(alpha),
        }
    }
    #[pyo3(signature = (duration=None))]
    fn write(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.write(duration),
        }
    }
    /// Write declared equation tags one term at a time, rather than glyph by glyph.
    #[pyo3(signature = (*, tags=None, duration=1.0))]
    fn write_by_term(&self, tags: Option<Vec<String>>, duration: f64) -> PyResult<Self> {
        if !duration.is_finite() || duration <= 0.0 {
            return Err(PyValueError::new_err(
                "duration must be a finite positive number",
            ));
        }
        Ok(Self(self.0.write_by_terms(tags, duration)))
    }
    #[pyo3(signature = (duration=None))]
    fn create(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.create(duration),
        }
    }
    #[pyo3(signature = (duration=None))]
    fn unwrite(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.unwrite(duration),
        }
    }
    #[pyo3(signature = (duration=None))]
    fn uncreate(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.uncreate(duration),
        }
    }
    #[pyo3(signature = (duration=None))]
    fn grow_from_center(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.grow_from_center(duration),
        }
    }
    #[pyo3(signature = (duration=None))]
    fn shrink_to_center(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.shrink_to_center(duration),
        }
    }
    #[pyo3(signature = (duration=None))]
    fn spin_in_from_nothing(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.spin_in_from_nothing(duration),
        }
    }
    #[pyo3(signature = (duration=None))]
    fn draw_border_then_fill(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.draw_border_then_fill(duration),
        }
    }
    #[pyo3(signature = (duration=None))]
    fn indicate(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.indicate(duration),
        }
    }
    #[pyo3(signature = (duration=None))]
    fn wiggle(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.wiggle(duration),
        }
    }

    fn move_along_path(&self, target: &PyDrawable) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.move_along_drawable(&target.0),
        }
    }

    fn move_along_drawable(&self, target: &PyDrawable) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.move_along_drawable(&target.0),
        }
    }
    fn fade_transform(&self, target: &PyDrawable) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.fade_transform(&target.0),
        }
    }
    fn transform(&self, target: &PyDrawable) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.transform(&target.0),
        }
    }
    fn replacement_transform(&self, target: &PyDrawable) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.replacement_transform(&target.0),
        }
    }

    // -- Reactive methods --

    /// Attach a preset updater that runs every frame.
    fn add_updater(&self, updater: &PyUpdater) {
        self.0.add_updater(updater.0.clone());
    }

    /// Attach a generic Python callback updater.
    ///
    /// `callback` must be a callable with signature `callback(pos, dt, elapsed) -> (x,y,z)`
    /// where `pos` is a `(x,y,z)` tuple of the current world position and `dt`/`elapsed`
    /// are frame delta and total elapsed seconds. The callback should return the new
    /// world position. This is fully generic — use it for Lorenz, spirals, fields, etc.
    ///
    /// Example Lorenz:
    /// ```python
    /// def lorenz(pos, dt, t):
    ///     x,y,z = pos
    ///     dx = 10*(y-x); dy = x*(28-z)-y; dz = x*y - 8/3*z
    ///     # fixed integration step 0.01, repeat 3 substeps per frame for speed
    ///     for _ in range(3):
    ///         x += 0.01*dx; y += 0.01*dy; z += 0.01*dz
    ///         dx = 10*(y-x); dy = x*(28-z)-y; dz = x*y - 8/3*z
    ///     return (x,y,z)
    /// dot.add_updater_fn(lorenz)
    /// ```
    fn add_updater_fn(&self, callback: Py<PyAny>) -> PyResult<()> {
        if Python::attach(|py| callback.bind(py).is_callable()) == false {
            return Err(PyValueError::new_err("callback must be callable"));
        }
        let key = PYTHON_UPDATER_KEY_COUNTER.fetch_add(1, Ordering::Relaxed);
        let callback_clone = callback.clone();
        let updater = gaanim_animation::Updater::new(move |dt, elapsed, entity, world| {
            let current = world
                .get::<gaanim_math::SpatialTransform>(entity)
                .map(|t| t.translation);
            let current = match current {
                Some(p) => p,
                None => return true,
            };
            let result: PyResult<(f64, f64, f64)> = Python::attach(|py| {
                let func = callback_clone.bind(py);
                let pos = (current.x, current.y, current.z);
                // Preferred signature: callback(pos, dt, elapsed)
                let v = func.call1((pos, dt, elapsed))?;
                // Accept either (x,y,z) tuple or list
                if let Ok(tup) = v.extract::<(f64, f64, f64)>() {
                    Ok(tup)
                } else if let Ok(vec) = v.extract::<Vec<f64>>() {
                    if vec.len() == 3 {
                        Ok((vec[0], vec[1], vec[2]))
                    } else {
                        Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                            "callback must return (x,y,z) tuple of 3 floats",
                        ))
                    }
                } else {
                    Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                        "callback must return (x,y,z) tuple",
                    ))
                }
            });
            match result {
                Ok((nx, ny, nz)) => {
                    if nx.is_finite() && ny.is_finite() && nz.is_finite() {
                        if let Some(mut t) = world.get_mut::<gaanim_math::SpatialTransform>(entity)
                        {
                            t.translation = gaanim_core::glam::DVec3::new(nx, ny, nz);
                        }
                    }
                    true
                }
                Err(e) => {
                    Python::attach(|py| e.print(py));
                    true
                }
            }
        });
        gaanim_api::canvas::ops::register_python_updater(key, updater);
        self.0.add_python_updater(key);
        Ok(())
    }

    /// Remove any updater attached to this entity.
    fn remove_updater(&self) {
        self.0.remove_updater();
    }

    /// Copy the source entity's Y position each frame.
    fn bind_y_from(&self, source: &PyDrawable) {
        self.0.bind_y_from(&source.0);
    }

    /// Copy the source entity's X position each frame.
    fn bind_x_from(&self, source: &PyDrawable) {
        self.0.bind_x_from(&source.0);
    }

    /// Keep this drawable centered on ``source`` each frame.
    fn attach_to(&self, source: &PyDrawable) {
        self.0.attach_to(&source.0);
    }

    /// Follow ``source`` while keeping an ``(x, y)`` scene-space offset.
    fn follow_to(&self, source: &PyDrawable, offset: (f64, f64)) {
        self.0.follow_to(&source.0, offset.0, offset.1);
    }

    /// Copy selected source axes each frame. ``axes`` accepts ``"x"``,
    /// ``"y"``, ``"xy"`` (the default), or ``"xyz"``.
    #[pyo3(signature = (source, axes="xy"))]
    fn bind_position_from(&self, source: &PyDrawable, axes: &str) -> PyResult<()> {
        let axes = match axes {
            "x" => gaanim_api::canvas::AxisMask::X,
            "y" => gaanim_api::canvas::AxisMask::Y,
            "xy" => gaanim_api::canvas::AxisMask::XY,
            "xyz" => gaanim_api::canvas::AxisMask::XYZ,
            _ => {
                return Err(PyValueError::new_err(
                    "axes must be one of: 'x', 'y', 'xy', or 'xyz'",
                ));
            }
        };
        self.0.bind_position_from(&source.0, axes);
        Ok(())
    }

    // --- manim Axes compatibility ---
    /// manim `coords_to_point` — data coords → scene point (respects auto_fit/x_length).
    fn coords_to_point(&self, x: f64, y: f64) -> PyResult<(f64, f64)> {
        let Some(((x_min, x_max, _), (y_min, y_max, _), config)) = self.0.axes_info() else {
            return Err(PyValueError::new_err(
                "coords_to_point() only valid on axes",
            ));
        };
        let (avail_w, avail_h) = (800.0, 480.0);
        let manim_w: f64 = 14.222222222222221;
        let manim_h: f64 = 8.0;
        let (scale_x, scale_y) = match (config.x_length, config.y_length) {
            (Some(xl), Some(yl)) => (
                xl * avail_w / manim_w / (x_max - x_min).max(1e-9),
                yl * avail_h / manim_h / (y_max - y_min).max(1e-9),
            ),
            (Some(xl), None) => {
                let s = xl * avail_w / manim_w / (x_max - x_min).max(1e-9);
                (s, s)
            }
            (None, Some(yl)) => {
                let s = yl * avail_h / manim_h / (y_max - y_min).max(1e-9);
                (s, s)
            }
            (None, None) if config.auto_fit => {
                let s =
                    (avail_w / (x_max - x_min).max(1e-9)).min(avail_h / (y_max - y_min).max(1e-9));
                (s, s)
            }
            _ => (1.0, 1.0),
        };
        let x_center = (x_min + x_max) * 0.5;
        let y_center = (y_min + y_max) * 0.5;
        let (sx, sy) = if config.auto_fit || config.x_length.is_some() || config.y_length.is_some()
        {
            ((x - x_center) * scale_x, (y - y_center) * scale_y)
        } else {
            (x, y)
        };
        Ok((sx, sy))
    }

    /// manim `point_to_coords` — scene point → data coords (inverse of coords_to_point).
    fn point_to_coords(&self, point: (f64, f64)) -> PyResult<(f64, f64)> {
        let Some(((x_min, x_max, _), (y_min, y_max, _), config)) = self.0.axes_info() else {
            return Err(PyValueError::new_err(
                "point_to_coords() only valid on axes",
            ));
        };
        let (avail_w, avail_h) = (800.0, 480.0);
        let manim_w: f64 = 14.222222222222221;
        let manim_h: f64 = 8.0;
        let (scale_x, scale_y) = match (config.x_length, config.y_length) {
            (Some(xl), Some(yl)) => (
                xl * avail_w / manim_w / (x_max - x_min).max(1e-9),
                yl * avail_h / manim_h / (y_max - y_min).max(1e-9),
            ),
            (Some(xl), None) => {
                let s = xl * avail_w / manim_w / (x_max - x_min).max(1e-9);
                (s, s)
            }
            (None, Some(yl)) => {
                let s = yl * avail_h / manim_h / (y_max - y_min).max(1e-9);
                (s, s)
            }
            (None, None) if config.auto_fit => {
                let s =
                    (avail_w / (x_max - x_min).max(1e-9)).min(avail_h / (y_max - y_min).max(1e-9));
                (s, s)
            }
            _ => (1.0, 1.0),
        };
        let x_center = (x_min + x_max) * 0.5;
        let y_center = (y_min + y_max) * 0.5;
        let (x, y) = if config.auto_fit || config.x_length.is_some() || config.y_length.is_some() {
            (point.0 / scale_x + x_center, point.1 / scale_y + y_center)
        } else {
            (point.0, point.1)
        };
        Ok((x, y))
    }

    /// manim `get_x_axis` / `get_y_axis` — return the axes itself (group) for compatibility.
    fn get_x_axis(&self) -> PyResult<Self> {
        if self.0.axes_info().is_none() {
            return Err(PyValueError::new_err("get_x_axis() only valid on axes"));
        }
        Ok(Self(self.0.clone()))
    }
    fn get_y_axis(&self) -> PyResult<Self> {
        if self.0.axes_info().is_none() {
            return Err(PyValueError::new_err("get_y_axis() only valid on axes"));
        }
        Ok(Self(self.0.clone()))
    }
    fn get_axes(&self) -> PyResult<Self> {
        self.get_x_axis()
    }

    /// manim `add_coordinates` — no-op in gaanim (numbers already via config), kept for compat.
    fn add_coordinates(&self) -> Self {
        Self(self.0.clone())
    }

    /// manim `get_graph` / `plot` alias — note: use `scene.plot(axes, func, ...)` for actual graph creation.
    /// This is a compatibility shim that creates an empty polyline.
    fn get_graph(&self) -> PyResult<Self> {
        if self.0.axes_info().is_none() {
            return Err(PyValueError::new_err("get_graph() only valid on axes"));
        }
        Ok(Self(self.0.clone()))
    }
}
