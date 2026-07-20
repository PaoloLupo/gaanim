//! Thin PyDrawable wrapper over gaanim_api DrawableHandle.

use pyo3::prelude::*;

use crate::color::PyColor;
use crate::pylayout::{PyAnchor, PyDirection};
use crate::updater::PyUpdater;

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
}

#[pyclass(name = "Drawable", module = "gaanim_core", from_py_object)]
#[derive(Clone)]
pub struct PyDrawable(pub gaanim_api::canvas::DrawableHandle);

#[pymethods]
impl PyDrawable {
    fn fill(&self, color: PyColor) -> Self {
        Self(self.0.clone().fill(color.0))
    }
    fn no_fill(&self) -> Self {
        Self(self.0.clone().no_fill())
    }
    fn stroke(&self, color: PyColor, width: f64) -> Self {
        Self(self.0.clone().stroke(color.0, width))
    }
    fn no_stroke(&self) -> Self {
        Self(self.0.clone().no_stroke())
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
    fn scaled(&self, factor: f64) -> Self {
        Self(self.0.clone().scaled(factor))
    }
    fn rotated(&self, radians: f64) -> Self {
        Self(self.0.clone().rotated(radians))
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
    fn rotate(&self, rad: f64) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.rotate(rad),
        }
    }
    #[pyo3(signature = (duration=None))]
    fn fade_in(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.fade_in(duration),
        }
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

    /// Remove any updater attached to this entity.
    fn remove_updater(&self) {
        self.0.remove_updater();
    }

    /// Copy the source entity's Y position each frame.
    fn bind_y_from(&self, source: &PyDrawable) {
        self.0.bind_y_from(&source.0);
    }
}
