set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

python_dir := if os_family() == "windows" { "./.venv/Scripts" } else { "./.venv/bin" }
python := python_dir + if os_family() == "windows" { "/python.exe" } else { "/python3" }
system_python := if os_family() == "windows" { "py.exe" } else { "python" }

default:
    @just --choose

# Create .venv and install maturin into it.
[windows]
bootstrap:
    if (-not (Test-Path .venv)) { {{ system_python }} -m venv .venv }
    {{ python }} -m pip install --upgrade pip
    {{ python }} -m pip install maturin

[unix]
bootstrap:
    if test ! -e .venv; then {{ system_python }} -m venv .venv; fi
    {{ python }} -m pip install --upgrade pip
    {{ python }} -m pip install maturin

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
    cargo check --workspace

# Lint the entire workspace.
clippy:
    cargo clippy --workspace

# Build the Python extension in release mode and install it into the venv.

# This is the recommended dev loop: edit Rust -> `just build` -> re-run example.
[working-directory("./crates/gaanim_python")]
build-release:
    maturin develop --release

# Build the Python extension in debug mode (faster compile, slower Python bridge).
[working-directory("./crates/gaanim_python")]
build:
    maturin develop

# Produce a standalone .whl without installing it (output: target\wheels\).
[working-directory("./crates/gaanim_python")]
wheel:
    maturin build --release

# ---- Run --------------------------------------------------------------------

# Run an example by name without rebuilding. Usage: just run my_example
run EX: build
    {{ python }} examples/{{ EX }}.py

# ---- Doctor -----------------------------------------------------------------

# Sanity check: the workspace compiles AND the compiled extension is importable.
doctor: check
    {{ python }} -c "import gaanim.gaanim_core as g; print('import ok, attrs:', [a for a in dir(g) if not a.startswith('_')][:8])"
