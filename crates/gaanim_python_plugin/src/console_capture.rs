//! What the script prints and the warnings Python shows, kept in the console
//! log with their script lines so the editor's console lists them. Output
//! still reaches stdout and stderr as before.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use gaanim_core::console::{self, Level, LogEntry, ScriptLocation};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString};

/// Keeps each warning Python shows; `_record(label, message, file, line)`.
const WARNINGS_HOOK: &str = r#"
import warnings

_show_warning = warnings.showwarning

def showwarning(message, category, filename, lineno, file=None, line=None):
    try:
        _record(category.__name__, str(message), filename, lineno)
    except Exception:
        pass
    _show_warning(message, category, filename, lineno, file, line)

warnings.showwarning = showwarning
"#;

/// Route `sys.stdout` through a [`ConsoleTee`] and keep Python's warnings.
pub(crate) fn install(py: Python<'_>) -> PyResult<()> {
    let record = pyo3::types::PyCFunction::new_closure(
        py,
        None,
        None,
        |args: &Bound<'_, pyo3::types::PyTuple>, _kwargs: Option<&Bound<'_, PyDict>>| {
            let (label, message, file, line): (String, String, Option<String>, Option<u32>) =
                args.extract()?;
            let location = file
                .zip(line)
                .filter(|(file, _)| !file.starts_with('<'))
                .map(|(file, line)| ScriptLocation::new(file, line));
            console::record(LogEntry::new(Level::Warn, &label, message.trim_end()).at(location));
            PyResult::Ok(())
        },
    )?;
    let globals = PyDict::new(py);
    globals.set_item("_record", record)?;
    let source = std::ffi::CString::new(WARNINGS_HOOK).expect("no NUL in the hook source");
    py.run(&source, Some(&globals), None)?;

    let sys = py.import("sys")?;
    let stdout = sys.getattr("stdout")?;
    let stream = (!stdout.is_none()).then(|| stdout.unbind());
    sys.setattr("stdout", Bound::new(py, ConsoleTee::new(stream))?)
}

/// `sys.stdout` while the editor runs a script: writes go to the wrapped
/// stream, and each complete line is kept in the log with the script line
/// that printed it. `print()` writes each argument, separator and end apart,
/// so a write only gathers text; a line is logged at its newline.
#[pyclass(module = "gaanim_console")]
pub(crate) struct ConsoleTee {
    /// The stream it replaced; `None` where Python has no stdout, as in a
    /// Windows GUI process, so prints still reach the console.
    stream: Option<Py<PyAny>>,
    line: Mutex<PendingLine>,
}

#[derive(Default)]
struct PendingLine {
    text: String,
    /// Where the line's first write came from.
    origin: Option<Option<ScriptLocation>>,
}

impl ConsoleTee {
    fn new(stream: Option<Py<PyAny>>) -> Self {
        Self {
            stream,
            line: Mutex::default(),
        }
    }

    fn pending(&self) -> std::sync::MutexGuard<'_, PendingLine> {
        self.line
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn keep_line(text: &str, origin: Option<ScriptLocation>) {
    console::record(LogEntry::new(Level::Info, "print", text).at(origin));
}

#[pymethods]
impl ConsoleTee {
    fn write(&self, py: Python<'_>, text: &Bound<'_, PyString>) -> PyResult<Py<PyAny>> {
        let chunk = text.to_str()?;
        {
            let mut pending = self.pending();
            if pending.origin.is_none() {
                pending.origin = Some(caller_location(py));
            }
            let mut rest = chunk;
            while let Some(newline) = rest.find('\n') {
                let origin = pending.origin.take().flatten();
                if pending.text.is_empty() {
                    keep_line(&rest[..newline], origin);
                } else {
                    pending.text.push_str(&rest[..newline]);
                    keep_line(&pending.text, origin);
                    pending.text.clear();
                }
                rest = &rest[newline + 1..];
            }
            if !rest.is_empty() {
                pending.text.push_str(rest);
                if pending.origin.is_none() {
                    pending.origin = Some(caller_location(py));
                }
            }
        }
        match &self.stream {
            Some(stream) => stream.call_method1(py, pyo3::intern!(py, "write"), (text,)),
            None => Ok(chunk.chars().count().into_pyobject(py)?.into_any().unbind()),
        }
    }

    /// Keeps a line still waiting for its newline, such as the one
    /// `print(..., end="")` leaves; the runner flushes at the end of a run.
    fn flush(&self, py: Python<'_>) -> PyResult<()> {
        {
            let mut pending = self.pending();
            if !pending.text.is_empty() {
                let origin = pending.origin.take().flatten();
                keep_line(&pending.text, origin);
                pending.text.clear();
            }
        }
        if let Some(stream) = &self.stream {
            stream.call_method0(py, pyo3::intern!(py, "flush"))?;
        }
        Ok(())
    }

    fn __getattr__(&self, py: Python<'_>, name: &Bound<'_, PyString>) -> PyResult<Py<PyAny>> {
        match &self.stream {
            Some(stream) => stream.getattr(py, name),
            None => Err(pyo3::exceptions::PyAttributeError::new_err(
                name.clone().unbind(),
            )),
        }
    }
}

/// Each code object seen, kept alive so its address is not reused by another
/// one while it is a key, and its file when it has a real one.
type CodeFiles = HashMap<usize, (Py<PyAny>, Option<Arc<Path>>)>;

/// The script line running now: the frame that called `print`, which has
/// no frame of its own. Files are resolved once per code object.
fn caller_location(py: Python<'_>) -> Option<ScriptLocation> {
    /// Code objects kept resolved before the cache starts over.
    const MAX_CACHED_CODES: usize = 4096;
    static FILES: Mutex<Option<CodeFiles>> = Mutex::new(None);

    // SAFETY: the thread holds the GIL; the frame is borrowed and
    // `from_borrowed_ptr` takes its own reference.
    let frame = unsafe { ffi::PyEval_GetFrame() };
    if frame.is_null() {
        return None;
    }
    let frame: Bound<'_, PyAny> = unsafe { Bound::from_borrowed_ptr(py, frame.cast()) };
    // SAFETY: `frame` is a live frame object; PyFrame_GetCode returns a new
    // reference that `from_owned_ptr` adopts.
    let code: Bound<'_, PyAny> =
        unsafe { Bound::from_owned_ptr(py, ffi::PyFrame_GetCode(frame.as_ptr().cast()).cast()) };
    let mut files = FILES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let files = files.get_or_insert_with(HashMap::new);
    if files.len() > MAX_CACHED_CODES {
        files.clear();
    }
    let key = code.as_ptr() as usize;
    let file = match files.get(&key) {
        Some((_, file)) => file.clone(),
        None => {
            let file = code
                .getattr(pyo3::intern!(py, "co_filename"))
                .and_then(|file| file.extract::<String>())
                .ok()
                .filter(|file| !file.starts_with('<'))
                .map(|file| Arc::from(console::plain_path(Path::new(&file))));
            files.insert(key, (code.clone().unbind(), file.clone()));
            file
        }
    }?;
    // SAFETY: as above, a live frame object.
    let line = unsafe { ffi::PyFrame_GetLineNumber(frame.as_ptr().cast()) };
    Some(ScriptLocation {
        file,
        line: line.max(0) as u32,
    })
}
