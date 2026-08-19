# Rust to Python API contract

When exposing a user-facing feature, inspect every applicable layer:

1. Implement and test the owning Rust crate.
2. Re-export or wrap it through `gaanim_api` without leaking lower-level ECS
   details.
3. Add the PyO3 class, method, function, or enum in `gaanim_python/src` and
   register it from the native module.
4. Mirror the runtime signature and return type in `gaanim_core.pyi`. Every
   added or behaviorally changed public declaration must also have a concise
   docstring there; explain its effect, units/defaults, return or chaining
   behavior, and observable errors when relevant. Update an existing docstring
   when the contract changes. Typst documentation does not replace this stub
   documentation requirement.
5. Export the name from `gaanim/__init__.py` and `__all__` when it belongs to
   the top-level Python API.
6. Document the Python-facing behavior in the mapped `.typ` page.
7. Add a focused Rust or Python regression test and an example when the visual
   or fluent behavior is best demonstrated end-to-end.

Use `just python-develop` to install the lightweight authoring package when an
IDE needs it. `just validate-python-api` starts the Gaanim executable and
compares the stub against its builtin PyO3 module; the public wheel contains no
native extension. The validator proves that declarations exist at runtime, but
not that every runtime member is documented.

Keep compatibility aliases intentional and documented. Omit stub docstrings
only for private names, protocol/dunder members, or aliases with no independent
public behavior. Do not silently accept or ignore arguments merely to resemble
Manim unless that compatibility is part of the requested behavior.
