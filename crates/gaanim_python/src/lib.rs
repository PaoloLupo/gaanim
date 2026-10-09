use ::gaanim_core as engine_core;
use pyo3::prelude::*;

/// Python's values of a `Copy` class extract as copies: PyO3's
/// `from_py_object` clones them, which clippy flags in every class it marks.
macro_rules! copy_from_py_object {
    ($class:ty) => {
        impl<'a, 'py> pyo3::FromPyObject<'a, 'py> for $class {
            type Error = pyo3::pyclass::PyClassGuardError<'a, 'py>;

            fn extract(obj: pyo3::Borrowed<'a, 'py, pyo3::PyAny>) -> Result<Self, Self::Error> {
                Ok(*obj.extract::<pyo3::PyClassGuard<'_, $class>>()?)
            }
        }
    };
}

pyo3::create_exception!(
    gaanim_core,
    LayoutOwnershipError,
    pyo3::exceptions::PyException
);

mod annotations;
mod bar_race;
mod brush;
mod callback_recipe;
mod character;
mod color;
mod composition;
mod custom;
mod easing;
mod falloff;
mod live;
mod magic_move;
mod motion;
mod particles;
mod path_modifiers;
mod poll;
mod procedural;
mod progress_ring;
mod py3d;
mod pyaxonometric;
mod pycamera_view;
mod pycanvas;
mod pydrawable;
mod pyduplicate;
mod pyemphasis;
mod pylayout;
mod pymatrix;
mod pystyle;
mod pytext;
mod pytext_animator;
mod pyzones;
mod rolling_number;
mod text_motion;
mod transition;
mod updater;
mod visualization;

/// Register the `gaanim_core` builtin module.
/// The error of the glTF API kept for existing scripts.
pub(crate) fn gltf_unsupported() -> PyErr {
    pyo3::exceptions::PyNotImplementedError::new_err(
        "glTF models are no longer supported: Gaanim draws 3D with Vello from its own \
         primitives, surfaces and lines (scene.geometry.cube, sphere, surface, polyline_3d)",
    )
}

pub fn register_inittab() {
    pyo3::append_to_inittab!(gaanim_core);
}

/// The lines of the script that called into Gaanim, innermost first: the
/// Python frames outside the `gaanim` package, at most four, up to the
/// first top-level module code. Called for every drawable the editor's scripts create,
/// so it reads frames through the C API and classifies each code object once.
fn script_call_stack() -> Vec<engine_core::console::ScriptLocation> {
    use pyo3::ffi;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    /// Enough to name a helper and the lines that called it.
    const MAX_SCRIPT_FRAMES: usize = 4;
    /// Code objects kept classified before the cache starts over.
    const MAX_CACHED_CODES: usize = 4096;

    /// Where a code object's file sits.
    #[derive(Clone)]
    enum Origin {
        /// A script file; `module` for its top-level code, above which only
        /// the runner or an import remains.
        Script { file: Arc<Path>, module: bool },
        /// The `gaanim` package, or Python's own frozen and generated code.
        Library,
        /// `runpy`, which runs the script: no script frame lies above it.
        Runner,
    }

    thread_local! {
        /// Each code object a frame ran, kept alive so its address is not
        /// reused by another one while it stays a key.
        static CODES: RefCell<HashMap<usize, (Py<PyAny>, Origin)>> = RefCell::default();
    }

    fn classify(py: Python<'_>, code: &Bound<'_, PyAny>) -> Origin {
        static PACKAGE: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
        let Ok(file) = code
            .getattr(pyo3::intern!(py, "co_filename"))
            .and_then(|file| file.extract::<String>())
        else {
            return Origin::Library;
        };
        if file == "<frozen runpy>" {
            return Origin::Runner;
        }
        let package = PACKAGE.get_or_init(|| {
            py.import("gaanim")
                .and_then(|gaanim| gaanim.getattr("__file__"))
                .and_then(|file| file.extract::<PathBuf>())
                .ok()
                .and_then(|file| file.parent().map(engine_core::console::plain_path))
        });
        let path = engine_core::console::plain_path(Path::new(&file));
        // The editor runs the package from memory, under relative names
        // such as `gaanim/live.py`.
        let library = file.starts_with('<')
            || file.starts_with("gaanim/")
            || file.starts_with("gaanim\\")
            || package
                .as_deref()
                .is_some_and(|package| path.starts_with(package));
        if library {
            return Origin::Library;
        }
        let module = code
            .getattr(pyo3::intern!(py, "co_name"))
            .and_then(|name| name.extract::<String>())
            .is_ok_and(|name| name == "<module>");
        Origin::Script {
            file: Arc::from(path),
            module,
        }
    }

    Python::attach(|py| {
        let mut stack = Vec::new();
        // SAFETY: the thread holds the GIL; the frame is borrowed and
        // `from_borrowed_ptr` takes its own reference.
        let current = unsafe { ffi::PyEval_GetFrame() };
        if current.is_null() {
            return stack;
        }
        let mut frame: Bound<'_, PyAny> = unsafe { Bound::from_borrowed_ptr(py, current.cast()) };
        CODES.with_borrow_mut(|codes| {
            if codes.len() > MAX_CACHED_CODES {
                codes.clear();
            }
            while stack.len() < MAX_SCRIPT_FRAMES {
                // SAFETY: `frame` is a live frame object; PyFrame_GetCode
                // returns a new reference that `from_owned_ptr` adopts.
                let code: Bound<'_, PyAny> = unsafe {
                    Bound::from_owned_ptr(py, ffi::PyFrame_GetCode(frame.as_ptr().cast()).cast())
                };
                let key = code.as_ptr() as usize;
                let origin = match codes.get(&key) {
                    Some((_, origin)) => origin.clone(),
                    None => {
                        let origin = classify(py, &code);
                        codes.insert(key, (code.clone().unbind(), origin.clone()));
                        origin
                    }
                };
                match origin {
                    Origin::Runner => break,
                    Origin::Library => {}
                    Origin::Script { file, module } => {
                        // SAFETY: as above, a live frame object.
                        let line = unsafe { ffi::PyFrame_GetLineNumber(frame.as_ptr().cast()) };
                        stack.push(engine_core::console::ScriptLocation {
                            file,
                            line: line.max(0) as u32,
                        });
                        if module {
                            break;
                        }
                    }
                }
                match frame.getattr(pyo3::intern!(py, "f_back")) {
                    Ok(back) if !back.is_none() => frame = back,
                    _ => break,
                }
            }
        });
        stack
    })
}

#[pymodule]
pub fn gaanim_core(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    gaanim_api::canvas::set_call_site_provider(script_call_stack);
    m.add(
        "LayoutOwnershipError",
        _py.get_type::<LayoutOwnershipError>(),
    )?;
    m.add_class::<transition::PyTransitionType>()?;
    m.add_class::<transition::PyOverlay>()?;
    m.add_class::<color::PyColor>()?;
    m.add_class::<color::PyColorMap>()?;
    m.add_class::<brush::PyBrush>()?;
    m.add_class::<brush::PyBackground>()?;
    m.add_class::<pyduplicate::PyDistribution>()?;
    m.add_class::<particles::PyEmitter>()?;
    m.add_class::<brush::PyPostProcess>()?;
    m.add_class::<pylayout::PyAnchor>()?;
    m.add_class::<pylayout::PyDirection>()?;
    m.add_class::<pylayout::PyLayoutExpression>()?;
    m.add_class::<pylayout::PyLayoutConstraint>()?;
    m.add_class::<pylayout::PyConstraintSet>()?;
    m.add_class::<pycanvas::PyTheme>()?;
    m.add_class::<pycanvas::PyCanvas>()?;
    m.add_class::<pycanvas::PyCamera>()?;
    m.add_class::<pycanvas::PyCameraAnimation>()?;
    m.add_class::<pycanvas::PyCameraState>()?;
    m.add_class::<pycanvas::PyCameraConstraint>()?;
    m.add_class::<pycamera_view::PyCameraView>()?;
    m.add_class::<pyaxonometric::PyAxonometric>()?;
    m.add_class::<pycamera_view::PyCameraViewAnimation>()?;
    m.add_class::<pycanvas::PyScene>()?;
    m.add_class::<pycanvas::PySceneStop>()?;
    m.add_class::<pydrawable::PyBounds>()?;
    m.add_class::<pycanvas::PyGeometry>()?;
    m.add_class::<pycanvas::PyTypography>()?;
    m.add_class::<pycanvas::PyLayoutBuilder>()?;
    m.add_class::<pycanvas::PyMediaLibrary>()?;
    m.add_class::<pycanvas::PyVisualization>()?;
    m.add_class::<pycanvas::PySlideKit>()?;
    m.add_class::<pycanvas::PyMechanics>()?;
    m.add_class::<pycanvas::PyFx>()?;
    m.add_class::<pycanvas::PyAssetManager>()?;
    m.add_class::<pycanvas::PySegment>()?;
    m.add_class::<pycanvas::PyAudio>()?;
    m.add_class::<pycanvas::PyAudioTempo>()?;
    m.add_class::<pycanvas::PyAudioData>()?;
    m.add_class::<pycanvas::PyVoiceover>()?;
    m.add_class::<composition::PyComposition>()?;
    m.add_class::<composition::PySchedule>()?;
    m.add_class::<composition::PyScheduleEntry>()?;
    m.add_class::<easing::PyEasingCurve>()?;
    m.add_class::<easing::PyEasing>()?;
    m.add_class::<procedural::PyRandom>()?;
    m.add_function(wrap_pyfunction!(composition::parallel, m)?)?;
    m.add_function(wrap_pyfunction!(composition::sequence, m)?)?;
    m.add_function(wrap_pyfunction!(composition::stagger, m)?)?;
    m.add_function(wrap_pyfunction!(composition::distribute, m)?)?;
    m.add_function(wrap_pyfunction!(composition::label, m)?)?;
    m.add_function(wrap_pyfunction!(magic_move::magic_move, m)?)?;
    m.add_class::<pycanvas::PySceneMarker>()?;
    m.add_class::<pydrawable::PyCanvasAnim>()?;
    m.add_class::<pydrawable::PyAnchorPoint>()?;
    m.add_class::<pydrawable::PyDrawable>()?;
    m.add_class::<pydrawable::PyImage>()?;
    m.add_class::<pycanvas::PyVideo>()?;
    m.add_class::<pydrawable::PyVideoSegment>()?;
    m.add_class::<pycanvas::PyLottie>()?;
    m.add_class::<pycanvas::PyPointRef>()?;
    m.add_class::<pycanvas::PyDimension>()?;
    m.add_class::<pycanvas::PyAngleDimension>()?;
    m.add_class::<pycanvas::PySurroundingRect>()?;
    m.add_class::<pycanvas::PyParallaxLayer>()?;
    m.add_class::<pycanvas::PyForceVector>()?;
    m.add_class::<pycanvas::PySupport>()?;
    m.add_class::<py3d::PyMaterial3D>()?;
    m.add_class::<py3d::PyPrimitive3D>()?;
    m.add_class::<pytext::PyTextStyle>()?;
    m.add_class::<pytext::PyTextAnchor>()?;
    m.add_class::<pystyle::PyStrokeStyle>()?;
    m.add_class::<pystyle::PyStyle>()?;
    m.add_class::<pystyle::PyAxesStyle>()?;
    m.add_class::<pytext::PyTextFlow>()?;
    m.add_class::<pytext::PyTextPart>()?;
    m.add_class::<pytext::PyTextParts>()?;
    m.add_class::<pytext::PyTextQuery>()?;
    m.add_class::<pytext::PyTextSelection>()?;
    m.add_class::<pytext::PyTextSelectionAnimation>()?;
    m.add_class::<pytext::PyText>()?;
    m.add_class::<pytext_animator::PyTextAnimator>()?;
    m.add_class::<pytext_animator::PyTextAnimatorAnimation>()?;
    m.add_class::<path_modifiers::PyPathModifiers>()?;
    m.add_class::<annotations::PyAnnotations>()?;
    m.add_class::<path_modifiers::PyPathModifier>()?;
    m.add_class::<path_modifiers::PyPathModifierAnimation>()?;
    m.add_function(wrap_pyfunction!(pytext::text_part, m)?)?;
    m.add_function(wrap_pyfunction!(pytext::quantity, m)?)?;
    m.add_function(wrap_pyfunction!(pytext::text_parts, m)?)?;
    m.add_class::<pylayout::PyBox>()?;
    m.add_class::<pylayout::PyBoxCascade>()?;
    m.add_class::<pylayout::PyBoxStyle>()?;
    m.add_class::<pyzones::PyZones>()?;
    m.add_class::<pyzones::PyZoneSet>()?;
    m.add_class::<pyzones::PyZone>()?;
    m.add_class::<pymatrix::PyMatrixOrder>()?;
    m.add_class::<updater::PyUpdater>()?;
    m.add_class::<falloff::PyFalloff>()?;
    m.add_class::<falloff::PyFalloffColor>()?;
    m.add_class::<visualization::PyAxis>()?;
    m.add_class::<visualization::PyScale>()?;
    m.add_class::<visualization::PyField>()?;
    m.add_class::<visualization::PyValue>()?;
    m.add_class::<visualization::PyGuide>()?;
    m.add_class::<visualization::PyChartSpec>()?;
    m.add_class::<visualization::PyChart>()?;
    m.add_class::<visualization::PyComputed>()?;
    m.add_class::<visualization::PyTimeInput>()?;
    m.add_function(wrap_pyfunction!(visualization::computed, m)?)?;
    m.add_class::<visualization::PyParameter>()?;
    m.add_class::<poll::PyPoll>()?;
    m.add_class::<poll::PyLeaderboard>()?;
    m.add_class::<poll::PyAudience>()?;
    m.add_class::<poll::PyTeams>()?;
    m.add_class::<poll::PyCondition>()?;
    m.add_class::<character::PyCharacter>()?;
    m.add_class::<live::PyLiveZone>()?;
    m.add_class::<visualization::PyReadout>()?;
    m.add_class::<visualization::PyVariable>()?;
    m.add_class::<rolling_number::PyRollingNumber>()?;
    m.add_class::<progress_ring::PyProgressRing>()?;
    m.add_class::<bar_race::PyBarRace>()?;
    m.add_class::<bar_race::PyBarRaceAnimation>()?;
    m.add_class::<visualization::PyCoordinateRef>()?;
    m.add_class::<visualization::PyCoordinateSpace>()?;
    m.add_class::<visualization::PyCoordinateSpaceAnimation>()?;
    m.add_class::<visualization::PyChartAnimation>()?;
    m.add_class::<visualization::PyCoordinateSpace3D>()?;
    m.add_class::<visualization::PyVectorField>()?;
    m.add_class::<visualization::PyArrowVectorField>()?;
    m.add_class::<visualization::PyStreamLines>()?;
    m.add_class::<visualization::PyFlowParticles>()?;
    m.add_class::<visualization::PyNumberLine>()?;
    m.add_class::<visualization::PyPolarSpace>()?;
    m.add_class::<visualization::PyDataTable>()?;
    m.add_class::<visualization::PyDataSource>()?;
    m.add(
        "Cartesian2D",
        _py.get_type::<visualization::PyCoordinateSpace>(),
    )?;
    m.add(
        "Cartesian3D",
        _py.get_type::<visualization::PyCoordinateSpace3D>(),
    )?;
    m.add(
        "ComplexSpace",
        _py.get_type::<visualization::PyCoordinateSpace>(),
    )?;

    m.add(
        "GOLD",
        color::PyColor(engine_core::peniko::Color::from_rgb8(0xFF, 0xD7, 0x00)),
    )?;
    m.add(
        "CORAL",
        color::PyColor(engine_core::peniko::Color::from_rgb8(0xFF, 0x64, 0x64)),
    )?;
    m.add(
        "BLUE",
        color::PyColor(engine_core::peniko::Color::from_rgb8(0x19, 0x32, 0x64)),
    )?;
    m.add("WHITE", color::PyColor(engine_core::peniko::Color::WHITE))?;
    m.add("BLACK", color::PyColor(engine_core::peniko::Color::BLACK))?;
    m.add(
        "RED",
        color::PyColor(engine_core::peniko::Color::from_rgb8(0xE5, 0x4B, 0x4B)),
    )?;
    m.add(
        "GREEN",
        color::PyColor(engine_core::peniko::Color::from_rgb8(0x4B, 0xE5, 0x7C)),
    )?;
    m.add(
        "YELLOW",
        color::PyColor(engine_core::peniko::Color::from_rgb8(0xF5, 0xD0, 0x4B)),
    )?;
    m.add(
        "ORANGE",
        color::PyColor(engine_core::peniko::Color::from_rgb8(0xFF, 0x9F, 0x43)),
    )?;
    m.add(
        "PURPLE",
        color::PyColor(engine_core::peniko::Color::from_rgb8(0x9B, 0x59, 0xB6)),
    )?;
    m.add(
        "PINK",
        color::PyColor(engine_core::peniko::Color::from_rgb8(0xFF, 0x7A, 0xB6)),
    )?;
    m.add(
        "GRAY",
        color::PyColor(engine_core::peniko::Color::from_rgb8(0x80, 0x80, 0x80)),
    )?;
    m.add(
        "CYAN",
        color::PyColor(engine_core::peniko::Color::from_rgb8(0x4B, 0xE5, 0xE5)),
    )?;
    m.add(
        "NAVY",
        color::PyColor(engine_core::peniko::Color::from_rgb8(0x1B, 0x1F, 0x3B)),
    )?;
    m.add(
        "TEAL",
        color::PyColor(engine_core::peniko::Color::from_rgb8(0x2E, 0x86, 0xAB)),
    )?;
    Ok(())
}
