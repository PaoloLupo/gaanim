"""Runs playground scripts in Pyodide and records their scene.

``runner.js`` loads the ``gaanim`` wheel built for Pyodide, runs this module
and calls :func:`run` for each execution. The script runs as ``__main__`` from
the project folder, where the playground's files are, so relative paths such
as ``scene.media.image("foto.png")`` find them. Its ``scene.render()``
submits the scene to the playground host (``gaanim_python::playground``),
which records it into a playback bundle for the web player.
"""

import importlib.machinery
import importlib.util
import os
import runpy
import sys
import time
import traceback
from pathlib import Path

PROJECT = Path("/home/pyodide/proyecto")
SCRIPT = PROJECT / "main.py"
BUNDLE = Path("/tmp/escena.gaanim")

PROJECT.mkdir(parents=True, exist_ok=True)


def _load_core():
    """Import ``gaanim.gaanim_core`` before ``gaanim`` itself.

    The package refuses to import without its native module, which the
    application registers before running a script; here the module is the
    extension the wheel installs next to the package's Python files.
    """
    package = importlib.util.find_spec("gaanim")
    folder = Path(package.submodule_search_locations[0])
    library = next(folder.glob("gaanim_core*.so"))
    loader = importlib.machinery.ExtensionFileLoader("gaanim.gaanim_core", str(library))
    spec = importlib.util.spec_from_file_location("gaanim.gaanim_core", library, loader=loader)
    module = importlib.util.module_from_spec(spec)
    sys.modules["gaanim.gaanim_core"] = module
    loader.exec_module(module)
    return module


core = _load_core()
import gaanim  # noqa: E402  (needs the native module above)


def sync_files(names):
    """Remove project files the playground no longer has."""
    keep = set(names) | {"main.py"}
    for entry in PROJECT.iterdir():
        if entry.is_file() and entry.name not in keep:
            entry.unlink()


def _relative(text):
    return text.replace(f'"{SCRIPT}"', '"main.py"').replace(str(PROJECT) + "/", "")


def _failure(error, phase):
    """Describe `error` with the frames of the script and what it called."""
    summary = traceback.TracebackException.from_exception(error)
    stack = list(summary.stack)
    first = next((i for i, frame in enumerate(stack) if frame.filename == str(SCRIPT)), None)
    if first is not None:
        summary.stack = traceback.StackSummary.from_list(stack[first:])
    elif phase == "run":
        # A syntax error never entered the script: only runpy's frames.
        summary.stack = traceback.StackSummary.from_list([])
    line = None
    for frame in stack:
        if frame.filename == str(SCRIPT):
            line = frame.lineno
    if isinstance(error, SyntaxError) and error.filename == str(SCRIPT):
        line = error.lineno
    return {
        "phase": phase,
        "kind": type(error).__name__,
        "message": _relative(str(error)),
        "traceback": _relative("".join(summary.format())),
        "line": line,
    }


def run(fps, assets_changed, report):
    """Run ``main.py`` and record its scene into ``BUNDLE``.

    ``report(phase)`` tells the page when the recording starts. Returns the
    timings, or the failure of the phase that failed.
    """
    os.chdir(PROJECT)
    sys.argv = [str(SCRIPT)]
    if BUNDLE.exists():
        BUNDLE.unlink()
    core._playground_begin(assets_changed)
    started = time.perf_counter()
    try:
        runpy.run_path(str(SCRIPT), run_name="__main__")
    except SystemExit as exit:
        if exit.code not in (None, 0):
            return {"error": _failure(exit, "run")}
    except BaseException as error:
        return {"error": _failure(error, "run")}
    ran = time.perf_counter()
    report("record")
    try:
        core._playground_record(str(BUNDLE), fps)
    except BaseException as error:
        return {"error": _failure(error, "record")}
    return {"run": ran - started, "record": time.perf_counter() - ran}
