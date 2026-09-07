set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

python_dir := if os_family() == "windows" { "./.venv/Scripts" } else { "./.venv/bin" }
python := python_dir + if os_family() == "windows" { "/python.exe" } else { "/python3" }
system_python := if os_family() == "windows" { "py.exe" } else { "python" }
release_runtime := if os_family() == "windows" { "./target/release/gaanim-core.exe" } else { "./target/release/gaanim-core" }

# All development Cargo commands select the same opt-in dynamic-linking feature.
dev +args:
    {{ system_python }} scripts/dev.py {{ args }}

# Run an existing dev binary/harness with Bevy and Rust DLL search paths; no build.
dev-exec +args:
    {{ system_python }} scripts/dev.py exec {{ args }}

default:
    @just --choose

# Create .venv (needed by PyO3 to locate Python at build time).
[windows]
bootstrap:
    if (-not (Test-Path .venv)) { {{ system_python }} -m venv .venv }
    {{ python }} -m pip install --upgrade pip build hatchling

[unix]
bootstrap:
    if test ! -e .venv; then {{ system_python }} -m venv .venv; fi
    {{ python }} -m pip install --upgrade pip build hatchling

# Wipe the local .venv (forces a fresh `just bootstrap`).
[windows]
clean-venv:
    Remove-Item -Recurse -Force .venv -ErrorAction SilentlyContinue

[unix]
clean-venv:
    rm -rf .venv

# Wipe cargo build artifacts and the .venv.
clean: clean-venv
    cargo clean

# ---- Build ------------------------------------------------------------------

# Type-check the entire workspace (no codegen, fastest feedback).
check:
    {{ system_python }} scripts/dev.py check --workspace

# Type-check one package and its dependencies. Usage: just check-package gaanim_math
check-package package:
    {{ system_python }} scripts/dev.py check -p "{{ package }}"

# Test one package; optional arguments select a target, test name, or harness flags.
test-package package *args:
    {{ system_python }} scripts/dev.py test -p "{{ package }}" {{ args }}

# Lint the entire workspace.
clippy:
    {{ system_python }} scripts/dev.py clippy --workspace

# Build the `gaanim` application binary (debug mode).
build:
    {{ system_python }} scripts/dev.py build -p gaanim_editor -p gaanim_launcher

# Build the runtime and write target/cargo-timings/cargo-timing.html.
build-timings:
    {{ system_python }} scripts/dev.py build -p gaanim_editor --bin gaanim-core --timings

# Build the `gaanim` application binary (release mode).
build-release:
    cargo build -p gaanim_editor -p gaanim_launcher --release

[windows]
build-release-install: build-release wheel
    New-Item -ItemType Directory -Force -Path "C:\Tools\gaanim" | Out-Null
    Copy-Item -Path "./target/release/gaanim.exe" -Destination "C:\Tools\gaanim\" -Force
    Copy-Item -Path "./target/release/gaanim-core.exe" -Destination "C:\Tools\gaanim\" -Force
    Copy-Item -Path (Get-ChildItem "./target/wheels/gaanim-*-py3-none-any.whl" | Select-Object -First 1).FullName -Destination "C:\Tools\gaanim\" -Force

[unix]
build-release-install: build-release wheel
    mkdir -p "$HOME/.local/bin" "$HOME/.local/share/gaanim"
    install -m 755 ./target/release/gaanim ./target/release/gaanim-core "$HOME/.local/bin/"
    install -m 644 ./target/wheels/gaanim-*-py3-none-any.whl "$HOME/.local/share/gaanim/"

# Install the lightweight authoring package in the local virtual environment.
python-develop:
    {{ python }} -m pip install --editable crates/gaanim_python

# Build the universal authoring wheel in target/wheels/.
wheel:
    {{ python }} -m build --wheel --no-isolation --outdir target/wheels crates/gaanim_python
    {{ python }} tests/validate_authoring_wheel.py target/wheels

# Check that the embedded extension exports every public stub declaration.
validate-python-api:
    {{ system_python }} scripts/dev.py run -p gaanim_editor --bin gaanim-core -- --validate-python-api tests/validate_python_api.py

# Export every supported format plus one isolated 3D MP4 and inspect their contracts.
test-exports encoder="libx264":
    {{ system_python }} scripts/dev.py build -p gaanim_editor --bin gaanim-core
    {{ system_python }} scripts/dev.py exec {{ system_python }} tests/validate_exports.py --output target/export-smoke --encoder {{ encoder }}

# Measure runtime p50/p95, throughput, and peak memory using the native release executable.
# Profiles: smoke (fast wiring check) or standard (300-frame export and stable sample counts).
benchmark profile="smoke" encoder="libx264":
    cargo build -p gaanim_editor --bin gaanim-core --release
    {{ python }} tests/benchmark_runtime.py --executable {{ release_runtime }} --profile {{ profile }} --encoder {{ encoder }}

# ---- Run --------------------------------------------------------------------

# Run an example script inside the Gaanim application. Usage: just run my_example
[windows]
run EX: build
    {{ system_python }} scripts/dev.py exec ./target/debug/gaanim.exe examples/{{ EX }}.py

[unix]
run EX: build
    {{ system_python }} scripts/dev.py exec ./target/debug/gaanim examples/{{ EX }}.py

# Build documentation site and PDF (one-shot).
docs:
    {{ system_python }} scripts/dev.py build -p gaanim_editor --bin gaanim-core
    {{ system_python }} scripts/dev.py run -p docs -- compile

# Build documentation PDF explicitly to custom output path.
docs-pdf output="documentation.pdf":
    {{ system_python }} scripts/dev.py run -p docs -- compile --pdf-output {{ output }}

# Build documentation site and open in browser.
docs-open:
    {{ system_python }} scripts/dev.py run -p docs -- compile --open

# Watch mode for documentation with live reload.
docs-watch:
    {{ system_python }} scripts/dev.py run -p docs -- watch

# ---- Doctor -----------------------------------------------------------------

# Sanity check: the workspace compiles and the `gaanim` binary responds.
doctor: check build
    {{ system_python }} scripts/dev.py run -p gaanim_launcher -- --help
