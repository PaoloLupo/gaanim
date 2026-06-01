# justfile for gaanim (Windows / PowerShell)
#
# Install just:        choco install just
#                      winget install --id Casey.Just -e
#                      scoop install just
#
# Quick start:
#   just bootstrap     # create .venv and install maturin
#   just build         # compile the Python extension (release) into the venv
#   just run-math      # build + run examples/math_animation.py
#   just clippy        # lint the whole workspace

set shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

# ---- Configuration ----------------------------------------------------------

py := "py"
venv := ".venv"
venvpy := ".venv\\Scripts\\python.exe"
crate := "crates\\gaanim_python"
wheels := "target\\wheels"

# ---- Default -----------------------------------------------------------------

# Show the list of available recipes.
default:
    @just --choose

# ---- Bootstrap ---------------------------------------------------------------

# Create .venv and install maturin into it.
bootstrap:
    if (-not (Test-Path {{ venv }})) { {{ py }} -m venv {{ venv }} }
    {{ venvpy }} -m pip install --upgrade pip
    {{ venvpy }} -m pip install maturin

# Wipe the local .venv (forces a fresh `just bootstrap`).
clean-venv:
    Remove-Item -Recurse -Force {{ venv }} -ErrorAction SilentlyContinue

# Wipe cargo build artifacts and the .venv.
clean: clean-venv
    cargo clean

# ---- Build ------------------------------------------------------------------

# Type-check the entire workspace (no codegen, fastest feedback).
check:
    cargo check --workspace

# Lint the entire workspace.
clippy:
    cargo clippy --workspace

# Build the Python extension in release mode and install it into the venv.

# This is the recommended dev loop: edit Rust -> `just build` -> re-run example.
build:
    maturin develop --release -m {{ crate }}\Cargo.toml

# Build the Python extension in debug mode (faster compile, slower Python bridge).
build-debug:
    maturin develop -m {{ crate }}\Cargo.toml

# Produce a standalone .whl without installing it (output: target\wheels\).
wheel:
    maturin build --release -m {{ crate }}\Cargo.toml

# ---- Run --------------------------------------------------------------------

# Build (release) then run the math animation example.
run-math: build
    {{ venvpy }} examples\math_animation.py

# Build (release) then run the write smoke test.
run-smoke: build
    {{ venvpy }} examples\write_smoke.py

# Run an example by name without rebuilding. Usage: just run my_example
run EX:
    {{ venvpy }} examples\{{ EX }}.py

# ---- Doctor -----------------------------------------------------------------

# Sanity check: the workspace compiles AND the compiled extension is importable.
doctor: check
    {{ venvpy }} -c "import gaanim.gaanim_core as g; print('import ok, attrs:', [a for a in dir(g) if not a.startswith('_')][:8])"
