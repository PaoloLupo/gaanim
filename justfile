set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

python_dir := if os_family() == "windows" { "./.venv/Scripts" } else { "./.venv/bin" }
python := python_dir + if os_family() == "windows" { "/python.exe" } else { "/python3" }
system_python := if os_family() == "windows" { "py.exe" } else { "python" }

default:
    @just --choose

# Create .venv (needed by PyO3 to locate Python at build time).
[windows]
bootstrap:
    if (-not (Test-Path .venv)) { {{ system_python }} -m venv .venv }
    {{ python }} -m pip install --upgrade pip

[unix]
bootstrap:
    if test ! -e .venv; then {{ system_python }} -m venv .venv; fi
    {{ python }} -m pip install --upgrade pip

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

# Build the `gaanim` application binary (debug mode).
build:
    cargo build -p gaanim_editor

# Build the `gaanim` application binary (release mode).
build-release:
    cargo build -p gaanim_editor --release

# ---- Run --------------------------------------------------------------------

# Run an example script inside the Gaanim application. Usage: just run my_example
[windows]
run EX:
    $pyBase = & "{{ python }}" -c "import sys; print(sys.base_prefix)"; $env:PATH = "$pyBase;$env:PATH"; cargo run -p gaanim_editor -- examples/{{ EX }}.py

[unix]
run EX:
    cargo run -p gaanim_editor -- examples/{{ EX }}.py

# Build documentation site (one-shot).
docs:
    cargo run -p docs -- compile

# Build documentation site and open in browser.
docs-open:
    cargo run -p docs -- compile --open

# Watch mode for documentation with live reload.
docs-watch:
    cargo run -p docs -- watch

# ---- Doctor -----------------------------------------------------------------

# Sanity check: the workspace compiles and the `gaanim` binary responds.
[windows]
doctor: check
    cargo build -p gaanim_editor 2>&1
    $pyBase = & "{{ python }}" -c "import sys; print(sys.base_prefix)"; $env:PATH = "$pyBase;$env:PATH"; cargo run -p gaanim_editor -- --help

[unix]
doctor: check
    cargo build -p gaanim_editor 2>&1
    cargo run -p gaanim_editor -- --help
