"""Editor services of the Gaanim playground.

Jedi analyses the script against the type stubs of the ``gaanim`` package
(``stubs.zip``), so completions, signatures and hovers show the documented
API without loading the engine. ``language.js`` runs this module in its own
Pyodide worker and calls these functions with the editor's text.
"""

import glob
import os
import re

import jedi

STUBS = "/home/pyodide/stubs"
PROJECT = "/home/pyodide/proyecto"
SCRIPT = f"{PROJECT}/main.py"

os.makedirs(PROJECT, exist_ok=True)

# Jedi does not understand `typing.Self` (PEP 673), which every fluent setter
# returns (`circle.fill(BLUE).` would complete nothing). The stubs are
# rewritten to the self-typed TypeVar it does follow, which also keeps the
# subclass: `def fill(self, ...) -> Self` becomes
# `def fill(self: _GaanimSelf, ...) -> _GaanimSelf`.
_SELF_METHOD = re.compile(
    r"def (\w+)\((\s*)self(?=\s*[,)])((?:(?!\bdef\b).)*?)\)(\s*)->(\s*)Self(\s*):", re.DOTALL
)


def _jedi_self_types():
    for path in glob.glob(f"{STUBS}/gaanim/*.pyi"):
        with open(path, encoding="utf-8") as file:
            text = file.read()
        if "Self" not in text:
            continue
        text = _SELF_METHOD.sub(
            lambda m: f"def {m[1]}({m[2]}self: _GaanimSelf{m[3]}){m[4]}->{m[5]}_GaanimSelf{m[6]}:",
            text,
        )
        text = re.sub(
            r"^(from typing import .*)$",
            r'\1\nfrom typing import TypeVar as _TypeVar\n_GaanimSelf = _TypeVar("_GaanimSelf")',
            text,
            count=1,
            flags=re.MULTILINE,
        )
        with open(path, "w", encoding="utf-8") as file:
            file.write(text)


_jedi_self_types()
_project = jedi.Project(PROJECT, added_sys_path=[STUBS], smart_sys_path=False)
# Completions of the last request, which `resolve` details by index.
_completions = []


def _shown(text):
    """`text` with the rewritten self type shown as the stubs declare it."""
    return text.replace("_GaanimSelf", "Self")


def _script(code):
    return jedi.Script(code, path=SCRIPT, project=_project)


def _doc(name):
    """The docstring of `name` without the signature Jedi prepends."""
    try:
        return name.docstring(raw=True).strip()
    except Exception:
        return ""


def _signatures(name):
    try:
        return [_shown(signature.to_string()) for signature in name.get_signatures()[:3]]
    except Exception:
        return []


def set_files(names):
    """Mirror the playground's files, so paths in strings complete."""
    keep = set(names)
    for entry in os.listdir(PROJECT):
        if entry != "main.py" and entry not in keep:
            os.remove(os.path.join(PROJECT, entry))
    for name in keep:
        path = os.path.join(PROJECT, name)
        if not os.path.exists(path):
            open(path, "wb").close()


def complete(code, line, column):
    global _completions
    try:
        _completions = _script(code).complete(line, column)
    except Exception:
        _completions = []
    return [
        {
            "label": completion.name_with_symbols,
            "kind": completion.type,
            "typed": len(completion.name_with_symbols) - len(completion.complete),
        }
        for completion in _completions[:400]
    ]


def resolve(index, label):
    """Details of completion `index`, if it is still the one named `label`."""
    if not 0 <= index < len(_completions):
        return None
    completion = _completions[index]
    if completion.name_with_symbols != label:
        return None
    return {
        "detail": _shown(completion.description),
        "signatures": _signatures(completion),
        "doc": _doc(completion),
    }


def hover(code, line, column):
    try:
        names = _script(code).help(line, column)
    except Exception:
        return None
    if not names:
        return None
    name = names[0]
    title = name.description
    if name.type == "statement" or name.type == "param":
        # A variable: show what it holds.
        try:
            inferred = name.infer()
        except Exception:
            inferred = []
        if inferred:
            name = inferred[0]
            title = f"{names[0].name}: {_shown(name.name)}"
    return {
        "title": title,
        "signatures": _signatures(name),
        "doc": _doc(name),
    }


def signatures(code, line, column):
    try:
        found = _script(code).get_signatures(line, column)
    except Exception:
        return None
    result = []
    for signature in found[:5]:
        label = _shown(signature.to_string())
        # Monaco highlights parameters by their offsets in the label.
        params, start = [], label.find("(") + 1
        for param in signature.params:
            text = _shown(param.to_string())
            at = label.find(text, start)
            if at < 0:
                params.append([0, 0])
                continue
            params.append([at, at + len(text)])
            start = at + len(text)
        result.append(
            {
                "label": label,
                "params": params,
                "index": signature.index,
                "doc": _doc(signature),
            }
        )
    return result


def errors(code):
    try:
        found = _script(code).get_syntax_errors()
    except Exception:
        return []
    return [
        {
            "line": error.line,
            "column": error.column,
            "until_line": error.until_line,
            "until_column": error.until_column,
            "message": error.get_message(),
        }
        for error in found
    ]
