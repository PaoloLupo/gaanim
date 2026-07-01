//! Thin PyDrawable wrapper over gaanim_api DrawableHandle.

use pyo3::prelude::*;

use crate::color::PyColor;

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
    fn fill(&self, color: &PyColor) -> Self {
        Self(self.0.clone().fill(color.0))
    }
    fn no_fill(&self) -> Self {
        Self(self.0.clone().no_fill())
    }
    fn stroke(&self, color: &PyColor, width: f64) -> Self {
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
    fn fade_in(&self) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.fade_in(),
        }
    }
    fn fade_out(&self) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.fade_out(),
        }
    }
    fn fade_to(&self, alpha: f32) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.fade_to(alpha),
        }
    }
    fn write(&self) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.write(),
        }
    }
    fn create(&self) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.create(),
        }
    }
    fn unwrite(&self) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.unwrite(),
        }
    }
    fn uncreate(&self) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.uncreate(),
        }
    }
    fn grow_from_center(&self) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.grow_from_center(),
        }
    }
    fn shrink_to_center(&self) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.shrink_to_center(),
        }
    }
    fn spin_in_from_nothing(&self) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.spin_in_from_nothing(),
        }
    }
    fn draw_border_then_fill(&self) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.draw_border_then_fill(),
        }
    }
    fn indicate(&self) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.indicate(),
        }
    }
    fn wiggle(&self) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.wiggle(),
        }
    }
}
