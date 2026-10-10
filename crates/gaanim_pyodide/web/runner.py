"""Runs playground scripts in Pyodide and records their scene.

``runner.js`` loads the ``gaanim`` wheel built for Pyodide, runs this module
and calls :func:`run` for each execution. The playground's project (its
``main.py``, its modules and its assets, in folders) is written to the
project folder, from which ``main.py`` runs as ``__main__``: relative paths
such as ``scene.media.image("assets/foto.png")`` find their files, and the
project's modules import from it, fresh on every run. ``scene.render()``
submits the scene to the playground host (``gaanim_python::playground``),
which records it into a playback bundle for the web player.
"""

import importlib
import importlib.machinery
import importlib.util
import os
import re
import runpy
import sys
import time
import tomllib
import traceback
from pathlib import Path

PROJECT = Path("/home/pyodide/proyecto")
SCRIPT = PROJECT / "main.py"
BUNDLE = Path("/tmp/escena.gaanim")

PROJECT.mkdir(parents=True, exist_ok=True)


def requirements():
    """The packages the project asks for: ``[project] dependencies`` of its
    ``pyproject.toml`` (the file of an application project) and the lines of
    its ``requirements.txt``. ``gaanim`` itself, which the playground has, is
    left out; so are comments and pip options."""
    found = []
    pyproject = PROJECT / "pyproject.toml"
    if pyproject.is_file():
        try:
            found += tomllib.loads(pyproject.read_text(encoding="utf-8")).get("project", {}).get("dependencies", [])
        except (tomllib.TOMLDecodeError, UnicodeDecodeError) as error:
            print(f"pyproject.toml no se pudo leer: {error}", file=sys.stderr)
    listed = PROJECT / "requirements.txt"
    if listed.is_file():
        for line in listed.read_text(encoding="utf-8").splitlines():
            line = line.split("#", 1)[0].strip()
            if line and not line.startswith("-"):
                found.append(line)
    wanted = []
    for requirement in found:
        name = re.match(r"[A-Za-z0-9._-]+", requirement.strip())
        if name and name.group(0).lower() != "gaanim" and requirement not in wanted:
            wanted.append(requirement.strip())
    return wanted


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


def sync_files(paths, folders=()):
    """Remove the project files and folders the playground no longer has.

    ``paths`` are the project's files relative to the project folder
    (``main.py``, ``lib/util.py``, ``assets/fonts/Inter.ttf``); ``folders``
    its folders, empty ones included.
    """
    keep = {PROJECT / path for path in paths} | {SCRIPT}
    keep_folders = {PROJECT / folder for folder in folders}
    for path in keep | keep_folders:
        keep_folders.update(path.parents)
    for entry in sorted(PROJECT.rglob("*"), key=lambda entry: len(entry.parts), reverse=True):
        if entry.is_file() and entry not in keep:
            entry.unlink()
        elif entry.is_dir() and entry not in keep_folders and not any(entry.iterdir()):
            entry.rmdir()
    for folder in folders:
        (PROJECT / folder).mkdir(parents=True, exist_ok=True)


def _forget_project_modules():
    """Drop the project's modules imported by an earlier run, so the script
    imports them as they are now."""
    prefix = str(PROJECT) + "/"
    for name, module in list(sys.modules.items()):
        if (getattr(module, "__file__", None) or "").startswith(prefix):
            del sys.modules[name]
    importlib.invalidate_caches()


def _relative(text):
    return text.replace(str(PROJECT) + "/", "")


def _in_project(filename):
    return filename.startswith(str(PROJECT) + "/")


def _failure(error, phase):
    """Describe `error` with the frames of the project and what it called,
    and where in the project it happened: ``file`` (relative to the project
    folder) and ``line``."""
    summary = traceback.TracebackException.from_exception(error)
    stack = list(summary.stack)
    first = next((i for i, frame in enumerate(stack) if frame.filename == str(SCRIPT)), None)
    if first is not None:
        summary.stack = traceback.StackSummary.from_list(stack[first:])
    elif phase == "run":
        # A syntax error never entered the script: only runpy's frames.
        summary.stack = traceback.StackSummary.from_list([])
    where = None
    for frame in stack:
        if _in_project(frame.filename):
            where = (frame.filename, frame.lineno)
    if isinstance(error, SyntaxError) and error.filename and _in_project(error.filename):
        where = (error.filename, error.lineno)
    return {
        "phase": phase,
        "kind": type(error).__name__,
        "message": _relative(str(error)),
        "traceback": _relative("".join(summary.format())),
        "file": _relative(where[0]) if where else None,
        "line": where[1] if where else None,
    }


def run(fps, assets_changed, report):
    """Run ``main.py`` and record its scene into ``BUNDLE``.

    ``report(phase, done, total)`` tells the page when the recording starts
    and how far it went. Returns the timings and the recording's size and
    duration, or the failure of the phase that failed.
    """
    os.chdir(PROJECT)
    sys.argv = [str(SCRIPT)]
    # The project's modules import from its folder, as a script's do.
    if sys.path[0] != str(PROJECT):
        sys.path.insert(0, str(PROJECT))
    _forget_project_modules()
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
    report("record", 0, 0)
    try:
        width, height, duration = core._playground_record(
            str(BUNDLE), fps, lambda done, total: report("record", done, total)
        )
    except BaseException as error:
        return {"error": _failure(error, "record")}
    return {
        "run": ran - started,
        "record": time.perf_counter() - ran,
        "width": width,
        "height": height,
        "duration": duration,
    }
