//! `Anim.typewriter/backspace/retype/scramble/scramble_to`: typing and
//! decoding motion for whole Text objects, evaluated natively per glyph.

use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;

use crate::pydrawable::PyCanvasAnim;

impl PyCanvasAnim {
    fn text_motion_slot(&self, name: &str) -> PyResult<()> {
        crate::custom::ensure_authoring_allowed()?;
        if !self.inner.inner.anim_type.is_empty_properties() {
            return Err(PyValueError::new_err(format!(
                "{name}() cannot be combined with property targets or another effect in one Anim; combine separate animations with parallel()"
            )));
        }
        if !self.inner.is_text_motion_target() {
            return Err(PyTypeError::new_err(format!(
                "{name}() requires the animate proxy of a Text"
            )));
        }
        Ok(())
    }

    fn with_motion(
        &self,
        name: &str,
        build: impl FnOnce(gaanim_api::canvas::Anim) -> Result<gaanim_api::canvas::Anim, String>,
    ) -> PyResult<Self> {
        self.text_motion_slot(name)?;
        build(self.inner.clone())
            .map(|inner| Self { inner })
            .map_err(PyValueError::new_err)
    }
}

#[pymethods]
impl PyCanvasAnim {
    #[pyo3(signature = (cps=18.0, cursor=Some(gaanim_api::text_motion::DEFAULT_CURSOR.to_string()), blink=2.0, jitter=0.2, seed=0, keep_cursor=true))]
    fn typewriter(
        &self,
        cps: f64,
        cursor: Option<String>,
        blink: f64,
        jitter: f64,
        seed: u64,
        keep_cursor: bool,
    ) -> PyResult<Self> {
        self.with_motion("typewriter", |anim| {
            anim.typewriter(cps, cursor.as_deref(), blink, jitter, seed, keep_cursor)
        })
    }

    #[pyo3(signature = (count=None, cps=24.0))]
    fn backspace(&self, count: Option<usize>, cps: f64) -> PyResult<Self> {
        self.with_motion("backspace", |anim| anim.backspace(count, cps))
    }

    #[pyo3(signature = (text, cps=18.0, jitter=0.2, seed=0))]
    fn retype(&self, text: &str, cps: f64, jitter: f64, seed: u64) -> PyResult<Self> {
        self.with_motion("retype", |anim| anim.retype(text, cps, jitter, seed))
    }

    #[pyo3(signature = (charset="upper", reveal_delay=0.3, speed=20.0, seed=0))]
    fn scramble(&self, charset: &str, reveal_delay: f64, speed: f64, seed: u64) -> PyResult<Self> {
        self.with_motion("scramble", |anim| {
            anim.scramble(charset, reveal_delay, speed, seed)
        })
    }

    #[pyo3(signature = (text, charset="upper", reveal_delay=0.3, speed=20.0, seed=0))]
    fn scramble_to(
        &self,
        text: &str,
        charset: &str,
        reveal_delay: f64,
        speed: f64,
        seed: u64,
    ) -> PyResult<Self> {
        self.with_motion("scramble_to", |anim| {
            anim.scramble_to(text, charset, reveal_delay, speed, seed)
        })
    }
}
