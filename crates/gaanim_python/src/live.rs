//! `scene.live_zone`: a part of the scene where the audience's characters
//! play while presenting.

use gaanim_api::canvas::{LiveEvent, LiveZoneError, LiveZoneHandle, live_express};
use pyo3::{exceptions::PyValueError, prelude::*};

pub(crate) fn live_error(error: LiveZoneError) -> PyErr {
    PyValueError::new_err(error.to_string())
}

/// A live zone: while presenting, each player arrives in it as their
/// character, launched, landing, taking a podium place or running a race,
/// and reacting by the zone's rules. Previews and exports replay its
/// preview players. It is data, so a presented `.gaanim` runs it without
/// Python.
#[pyclass(name = "LiveZone", module = "gaanim_core", frozen)]
pub struct PyLiveZone {
    pub(crate) inner: LiveZoneHandle,
}

fn point((x, y): (f64, f64)) -> [f64; 2] {
    [x, y]
}

impl PyLiveZone {
    fn rule(&self, on: LiveEvent, express: &str, looped: bool) -> PyResult<()> {
        crate::custom::ensure_authoring_allowed()?;
        let express = live_express(express, looped).map_err(live_error)?;
        self.inner.rule(on, express);
        Ok(())
    }
}

#[pymethods]
impl PyLiveZone {
    /// Nicknames replayed outside a live presentation.
    #[getter]
    fn preview(&self) -> Vec<String> {
        self.inner.zone().preview
    }

    /// A segment characters land on, tagged for rules; `sink` is how deep
    /// they sink into it, in character heights.
    #[pyo3(signature = (start, end, *, tag, sink=0.0))]
    fn surface<'py>(
        slf: PyRef<'py, Self>,
        start: (f64, f64),
        end: (f64, f64),
        tag: &str,
        sink: f64,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        slf.inner
            .surface(point(start), point(end), tag, sink)
            .map_err(live_error)?;
        Ok(slf)
    }

    /// Launch arriving characters from `start`, one every `every` seconds,
    /// at an angle sweeping between `angle` degrees every `period` seconds.
    #[pyo3(signature = (start, *, angle=(15.0, 65.0), period=3.0, speed=8.0, every=0.8, toward="right"))]
    fn launcher<'py>(
        slf: PyRef<'py, Self>,
        start: (f64, f64),
        angle: (f64, f64),
        period: f64,
        speed: f64,
        every: f64,
        toward: &str,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        let direction = match toward {
            "right" => 1.0,
            "left" => -1.0,
            other => {
                return Err(PyValueError::new_err(format!(
                    "toward must be \"right\" or \"left\", got {other:?}"
                )));
            }
        };
        slf.inner
            .launcher(point(start), angle, period, speed, every, direction)
            .map_err(live_error)?;
        Ok(slf)
    }

    /// The player at `rank` (0 for the leader) stands at `at`, playing
    /// `express` while there.
    #[pyo3(signature = (rank, at, *, express=None, r#loop=true))]
    fn place<'py>(
        slf: PyRef<'py, Self>,
        rank: usize,
        at: (f64, f64),
        express: Option<&str>,
        r#loop: bool,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        let express = express
            .map(|name| live_express(name, r#loop))
            .transpose()
            .map_err(live_error)?;
        slf.inner
            .place(rank, point(at), express)
            .map_err(live_error)?;
        Ok(slf)
    }

    /// The first `count` players run along bars: rank `r` stands at
    /// `origin + step * r + direction * length * score / leader's score`.
    #[pyo3(signature = (origin, *, step=(0.0, -1.0), direction=(1.0, 0.0), length=10.0, count=5))]
    fn race<'py>(
        slf: PyRef<'py, Self>,
        origin: (f64, f64),
        step: (f64, f64),
        direction: (f64, f64),
        length: f64,
        count: usize,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        slf.inner
            .race(point(origin), point(step), point(direction), length, count)
            .map_err(live_error)?;
        Ok(slf)
    }

    /// A character that arrives plays `express`.
    #[pyo3(signature = (express, *, r#loop=false))]
    fn on_join<'py>(
        slf: PyRef<'py, Self>,
        express: &str,
        r#loop: bool,
    ) -> PyResult<PyRef<'py, Self>> {
        slf.rule(LiveEvent::Join, express, r#loop)?;
        Ok(slf)
    }

    /// A character that lands plays `express`: on surfaces tagged `tag`,
    /// or on any surface; the zone's bottom is tagged "bottom".
    #[pyo3(signature = (express, *, tag=None, r#loop=false))]
    fn on_land<'py>(
        slf: PyRef<'py, Self>,
        express: &str,
        tag: Option<String>,
        r#loop: bool,
    ) -> PyResult<PyRef<'py, Self>> {
        slf.rule(LiveEvent::Land { tag }, express, r#loop)?;
        Ok(slf)
    }

    /// A character that goes up the leaderboard plays `express`.
    #[pyo3(signature = (express, *, r#loop=false))]
    fn on_rank_up<'py>(
        slf: PyRef<'py, Self>,
        express: &str,
        r#loop: bool,
    ) -> PyResult<PyRef<'py, Self>> {
        slf.rule(LiveEvent::RankUp, express, r#loop)?;
        Ok(slf)
    }

    /// A character that goes down the leaderboard plays `express`.
    #[pyo3(signature = (express, *, r#loop=false))]
    fn on_rank_down<'py>(
        slf: PyRef<'py, Self>,
        express: &str,
        r#loop: bool,
    ) -> PyResult<PyRef<'py, Self>> {
        slf.rule(LiveEvent::RankDown, express, r#loop)?;
        Ok(slf)
    }

    /// A character that becomes the leader plays `express`.
    #[pyo3(signature = (express, *, r#loop=false))]
    fn on_leader<'py>(
        slf: PyRef<'py, Self>,
        express: &str,
        r#loop: bool,
    ) -> PyResult<PyRef<'py, Self>> {
        slf.rule(LiveEvent::Leader, express, r#loop)?;
        Ok(slf)
    }

    /// Stop the zone at the cursor instead of at the end of the segment
    /// where it opened.
    fn close(&self) -> PyResult<()> {
        crate::custom::ensure_authoring_allowed()?;
        self.inner.close().map_err(live_error)
    }

    fn __repr__(&self) -> String {
        let zone = self.inner.zone();
        format!(
            "LiveZone(bounds={:?}, surfaces={}, launchers={}, places={}, race={}, rules={})",
            zone.bounds,
            zone.surfaces.len(),
            zone.launchers.len(),
            zone.places.len(),
            zone.race.is_some(),
            zone.rules.len()
        )
    }
}
