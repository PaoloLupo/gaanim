---
name: gaanim-sync-docs
description: Synchronize Gaanim public Rust and Python APIs with the correct Typst documentation page, signatures, examples, and navigation. Use when API methods, constructors, arguments, defaults, return types, errors, compatibility aliases, examples, or docs/content/**/*.typ files change, or when auditing documentation drift after a feature change.
---

# Synchronize Gaanim API documentation

Document the runtime API that exists, not an aspirational design.
Resolve `PLUGIN_ROOT` as the directory two levels above this `SKILL.md`; invoke
bundled scripts by absolute path while keeping the Gaanim repository as cwd.

## Workflow

1. Read `../../references/api-doc-map.md` and
   `../../references/python-bridge.md`.
2. Inspect the PyO3 binding, `.pyi`, `gaanim/__init__.py`, executable examples,
   and relevant Rust facade. Resolve contradictions in favor of tested runtime
   behavior, then flag stale sources. For every added or changed public Python
   declaration, add or refresh its `.pyi` docstring with behavior, relevant
   units/defaults, return or chaining behavior, and observable errors; Typst is
   complementary, not a substitute.
3. Run `python <PLUGIN_ROOT>/scripts/impact.py --format json` and select the narrowest
   existing `.typ` page. Add navigation only for a genuinely new API concept.
4. Follow neighboring `api-entry` structure. Match the callable signature,
   units, defaults, chaining/return behavior, and observable errors.
5. Prefer a short executable Python example. Update an existing example when
   it is the canonical demonstration; avoid ornamental examples that cannot be
   run.
6. Run `just docs`. Execute or validate the example when its correctness is
   not already covered by a test.
7. Run `python <PLUGIN_ROOT>/scripts/audit.py` and resolve objective errors. Treat its
   semantic warnings as review prompts, not automatic failures.

Do not claim full API coverage from a successful Typst build. Finish by naming
the implementation source used, the `.pyi` declarations documented, the `.typ`
page changed, and the validations performed.
