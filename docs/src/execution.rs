use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        LazyLock, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration as StdDuration,
};

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use typst::{
    diag::{SourceDiagnostic, SourceResult, bail, eco_format},
    ecow::EcoString,
    engine::Engine,
    foundations::{Dict, Packed, Value, func},
    text::{RawContent, RawElem},
};

use crate::world::PROJECT_ROOT;

const PYTHON_PRELUDE: &str = r#"import sys as _sys
_sys.path.append(".")
_cell_id = _sys.argv[1] if len(_sys.argv) > 1 else "output"
import warnings as _warnings
_warnings.filterwarnings("ignore")
"#;

fn is_valid_webp(path: &Path) -> bool {
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut header = [0_u8; 12];
    file.read_exact(&mut header).is_ok() && has_valid_webp_signature(&header)
}

fn has_valid_webp_signature(header: &[u8]) -> bool {
    header.len() >= 12 && &header[0..4] == b"RIFF" && &header[8..12] == b"WEBP"
}

fn strip_ansi_escape_codes(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if let Some(&'[') = chars.peek() {
                let _ = chars.next();
                while let Some(&next_c) = chars.peek() {
                    let _ = chars.next();
                    if next_c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn adjust_stderr_line_numbers(stderr: &str, temp_file: &str, prelude_lines: usize) -> String {
    let mut lines = Vec::new();
    let file_name = Path::new(temp_file)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    for line in stderr.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Precompiling packages")
            || trimmed.contains("dependency successfully precompiled")
            || trimmed.contains("dependencies successfully precompiled")
            || trimmed.contains("UserWarning:")
            || trimmed.eq_ignore_ascii_case("Typst warning: unknown font family: consolas")
        {
            continue;
        }

        let mut replaced = line.to_string();
        if !temp_file.is_empty()
            && (line.contains(&file_name) || line.contains(temp_file))
            && let Some(byte_idx) = line.find("line ")
        {
            let start_byte = byte_idx + 5;
            if start_byte < line.len() {
                let mut end_byte = start_byte;
                while end_byte < line.len() && line.as_bytes()[end_byte].is_ascii_digit() {
                    end_byte += 1;
                }
                if start_byte < end_byte
                    && let Ok(line_num) = line[start_byte..end_byte].parse::<usize>()
                    && line_num > prelude_lines
                {
                    let adjusted = line_num - prelude_lines;
                    let prefix = line[..start_byte]
                        .replace(&file_name, "code cell")
                        .replace(temp_file, "code cell");
                    replaced = format!("{}{}{}", prefix, adjusted, &line[end_byte..]);
                }
            }
        }
        lines.push(replaced);
    }
    lines.join("\n")
}

fn extract_cells_from_file(path: &Path) -> std::io::Result<Vec<(String, String)>> {
    let content = fs::read_to_string(path)?;
    let mut cells = Vec::new();
    let mut current_cell_name = String::new();
    let mut current_cell_code = String::new();

    for line in content.lines() {
        let trimmed_start = line.trim_start();

        if let Some(name) = trimmed_start.strip_prefix("# %%") {
            if !current_cell_code.trim().is_empty() || !current_cell_name.is_empty() {
                cells.push((current_cell_name.clone(), current_cell_code.clone()));
            }
            current_cell_name = name.trim().to_string();
            current_cell_code = String::new();
        } else {
            current_cell_code.push_str(line);
            current_cell_code.push('\n');
        }
    }

    if !current_cell_code.trim().is_empty() || !current_cell_name.is_empty() {
        cells.push((current_cell_name, current_cell_code));
    }

    Ok(cells)
}

fn find_companion_file_by_cell(root: &Path, cell_name: &str, ext: &str) -> Option<PathBuf> {
    fn scan_dir(dir: &Path, cell_name: &str, ext: &str) -> Option<PathBuf> {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.is_dir() {
                    let name = path.file_name().and_then(|n| n.to_str());
                    if name == Some("target") || name == Some(".git") || name == Some("dist") {
                        continue;
                    }
                    if let Some(found) = scan_dir(&path, cell_name, ext) {
                        return Some(found);
                    }
                } else if path.is_file()
                    && path.extension().and_then(|e| e.to_str()) == Some(ext)
                    && let Ok(content) = fs::read_to_string(&path)
                {
                    for line in content.lines() {
                        let trimmed = line.trim_start();
                        if trimmed.starts_with("# %%")
                            && trimmed["# %%".len()..].trim() == cell_name
                        {
                            return Some(path);
                        }
                    }
                }
            }
        }
        None
    }
    scan_dir(root, cell_name, ext)
}

/// A cell whose process fails without printing anything (for example a host
/// that cannot load its DLLs on Windows) must not pass for a cell that simply
/// has no output: report the exit status as the cell's error instead.
fn silent_failure(program: &str, success: bool, status: &str, stderr: &str) -> Option<String> {
    (!success && stderr.trim().is_empty()).then(|| {
        format!(
            "{program} exited with {status} and printed no error. Check that it starts \
             outside the docs build (on Windows, the Python base directory must be on PATH)."
        )
    })
}

/// What the cell printed, without the report `gaanim check` appends: readers
/// care about their `print()` output, not the builder's validation.
fn without_preflight_report(stdout: &str) -> &str {
    match stdout.find("Scene preflight:") {
        Some(start) if start == 0 || stdout[..start].ends_with('\n') => &stdout[..start],
        _ => stdout,
    }
}

/// Drop the runtime's tracing lines (`2026-01-01T00:00:00.000Z  INFO ...`)
/// and ALSA's complaints about a build machine without a sound card.
fn without_runtime_logs(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| !line.starts_with("ALSA lib "))
        .filter(|line| {
            let mut fields = line.split_whitespace();
            let timestamp = fields.next().unwrap_or("");
            let level = fields.next().unwrap_or("");
            !(timestamp.len() > 20
                && timestamp.ends_with('Z')
                && timestamp.as_bytes()[4] == b'-'
                && timestamp.as_bytes()[10] == b'T'
                && matches!(level, "TRACE" | "DEBUG" | "INFO" | "WARN" | "ERROR"))
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Source of a cell as a later `# continue` cell replays it: every line except
/// the `render()` call, which only the last cell of a chain may make.
fn without_render_calls(code: &str) -> String {
    code.lines()
        .filter(|line| !line.trim().ends_with(".render()"))
        .map(|line| format!("{line}\n"))
        .collect()
}

#[func]
pub fn compile_code_cell(
    engine: &mut Engine,
    raw: Packed<RawElem>,
    #[named]
    #[default(EcoString::inline("python"))]
    lang: EcoString,
    #[named]
    #[default(EcoString::inline(""))]
    id: EcoString,
    #[named]
    #[default(EcoString::inline(""))]
    prelude: EcoString,
) -> SourceResult<Value> {
    let span = raw.span();

    if lang.as_str() != "python" && lang.as_str() != "py" {
        bail!(
            span,
            "Only Python is supported in gaanim docs, got: {}",
            lang
        );
    }

    let ext = "py";

    // Parse lines: >>> / <<< markers and magic comments
    let mut code_lines = Vec::new();
    match &raw.text {
        RawContent::Text(text) => {
            for line in text.lines() {
                code_lines.push((line, span));
            }
        }
        RawContent::Lines(lines) => {
            for (line, s) in lines {
                code_lines.push((line.as_str(), *s));
            }
        }
    }

    let mut code_to_execute = String::new();
    let mut code_to_display = String::new();
    let mut show_code = false;
    let mut hide_code = false;
    let mut timeout_secs: u64 = 120; // gaanim animations can take longer
    let mut caption = String::new();
    let mut target_cell: Option<String> = None;
    let mut cell_id_override: Option<String> = None;
    let mut expected_webp: Option<String> = None;

    for (line, _) in code_lines {
        let trimmed = line.trim();

        if trimmed == "# continue" {
            continue;
        }
        if trimmed.starts_with("# show-code: true") || trimmed == "# show-code" {
            show_code = true;
            continue;
        }
        if trimmed.starts_with("# show-code: false") || trimmed == "# hide-code" {
            show_code = false;
            hide_code = true;
            continue;
        }
        if let Some(t) = trimmed.strip_prefix("# timeout:") {
            if let Ok(t) = t.trim().parse::<u64>() {
                timeout_secs = t;
            }
            continue;
        }
        if let Some(c) = trimmed.strip_prefix("# caption:") {
            caption = c.trim().to_string();
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# cell:") {
            target_cell = Some(rest.trim().to_string());
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# id:") {
            cell_id_override = Some(rest.trim().to_string());
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# output:") {
            expected_webp = Some(rest.trim().to_string());
            continue;
        }

        // <<< = display but don't execute
        if line.starts_with("<<< ") {
            code_to_display.push_str(line.strip_prefix("<<< ").unwrap());
            code_to_display.push('\n');
            continue;
        }
        if line.trim() == "<<<" {
            code_to_display.push('\n');
            continue;
        }

        // >>> = execute but don't display
        if line.starts_with(">>>") {
            code_to_execute.push_str(line.strip_prefix(">>>").unwrap());
            code_to_execute.push('\n');
            continue;
        }

        // Normal: both
        code_to_execute.push_str(line);
        code_to_execute.push('\n');
        code_to_display.push_str(line);
        code_to_display.push('\n');
    }

    // Legacy cache compatibility: infer output from old scene.export snippets.
    if expected_webp.is_none() && code_to_execute.contains(".export(") {
        // extrae primer argumento entre comillas de .export("...") o .export('...')
        let mut inferred: Option<String> = None;
        if let Some(start) = code_to_execute.find(".export(") {
            let rest = &code_to_execute[start + ".export(".len()..];
            if let Some(q) = rest.find('"') {
                let after = &rest[q + 1..];
                if let Some(end) = after.find('"') {
                    inferred = Some(after[..end].to_string());
                }
            } else if let Some(q) = rest.find('\'') {
                let after = &rest[q + 1..];
                if let Some(end) = after.find('\'') {
                    inferred = Some(after[..end].to_string());
                }
            }
        }
        if let Some(p) = inferred {
            if !p.is_empty() {
                expected_webp = Some(p);
            }
        } else if code_to_execute.contains("preview.webp") {
            expected_webp = Some("preview.webp".to_string());
        }
    }

    // Resolve companion file cell
    if let Some(ref cell_name) = target_cell {
        let file_id = span.id();
        let typ_path = if let Some(id) = file_id {
            PROJECT_ROOT
                .read()
                .unwrap()
                .as_ref()
                .map(|root| root.join(Path::new(id.vpath().get_without_slash())))
        } else {
            None
        };

        let mut companion_path = typ_path.map(|p| p.with_extension(ext));

        if companion_path.as_ref().map(|p| !p.exists()).unwrap_or(true) {
            let found = {
                let guard = PROJECT_ROOT.read().unwrap();
                if let Some(ref root) = *guard {
                    find_companion_file_by_cell(root, cell_name, ext)
                } else {
                    None
                }
            };
            if let Some(f) = found {
                companion_path = Some(f);
            }
        }

        if let Some(path) = companion_path {
            if path.exists() {
                let cells = match extract_cells_from_file(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        bail!(
                            span,
                            "Error reading companion file {}: {}",
                            path.display(),
                            e
                        )
                    }
                };

                let target_idx = cells.iter().position(|(name, _)| name == cell_name);
                if let Some(idx) = target_idx {
                    code_to_display = cells[idx].1.clone();

                    let mut exec_code = String::new();
                    exec_code.push_str("import sys as _sys\n_real_stdout = _sys.stdout\nclass _NullWriter:\n    def write(self, x): pass\n    def flush(self): pass\n_sys.stdout = _NullWriter()\n");

                    for i in cells.iter().take(idx) {
                        exec_code.push_str(&i.1);
                        exec_code.push('\n');
                    }

                    exec_code.push_str("\n_sys.stdout = _real_stdout\n");
                    exec_code.push_str(&cells[idx].1);
                    code_to_execute = exec_code;
                } else {
                    bail!(
                        span,
                        "Cell '{}' not found in companion file '{}'. Available cells: {:?}",
                        cell_name,
                        path.display(),
                        cells.iter().map(|(n, _)| n).collect::<Vec<_>>()
                    );
                }
            } else {
                bail!(
                    span,
                    "Companion file not found for cell '{}' (extension .{}).",
                    cell_name,
                    ext
                );
            }
        } else {
            bail!(
                span,
                "Could not determine current Typst file path, and no .py file found containing cell '{}'.",
                cell_name
            );
        }
    }

    // `# continue`: replay the page's previous cell first, hidden and with its
    // output muted, so a fragment runs with the names it builds on. Each cell
    // hands Typst its own code without `render()` calls (`own`); Typst
    // concatenates a run of continued cells into the next cell's prelude.
    let own = without_render_calls(&code_to_execute);
    let mut chain_lines = 0;
    if !prelude.trim().is_empty() {
        let replay = format!(
            "import sys as _sys\n_real_stdout = _sys.stdout\nclass _NullWriter:\n    def write(self, x): pass\n    def flush(self): pass\n_sys.stdout = _NullWriter()\n{}\n_sys.stdout = _real_stdout\n",
            prelude
        );
        chain_lines = replay.lines().count();
        code_to_execute = format!("{}{}", replay, code_to_execute);
    }

    // Hash and cell ID. An export's preview settings are part of its identity,
    // so changing them re-renders the previews instead of reusing old files.
    let cell_hash = if expected_webp.is_some() {
        typst_utils::hash128(format!("{code_to_execute}{}", PREVIEW_ARGS.join(" ")).as_bytes())
    } else {
        typst_utils::hash128(code_to_execute.as_bytes())
    };
    let cell_id = if !id.is_empty() {
        id.to_string()
    } else if let Some(ref override_id) = cell_id_override {
        override_id.clone()
    } else if let Some(ref name) = target_cell {
        format!("{}_{:x}", name, cell_hash)
    } else {
        format!("cell_{:x}", cell_hash)
    };

    let mode = if let Some(output) = expected_webp {
        CellMode::Export(output)
    } else if code_to_execute.contains(".render(") {
        CellMode::Check
    } else {
        CellMode::Validate
    };
    let job = CellJob {
        hash: format!("{:x}", cell_hash),
        script: format!("{PYTHON_PRELUDE}{code_to_execute}"),
        prelude_lines: PYTHON_PRELUDE.lines().count() + chain_lines,
        cell_id,
        mode,
        timeout_secs,
        cached_webp: None,
    };

    let outcome = match lookup_cache(&job) {
        Lookup::Hit(outcome) => {
            record(&job.cell_id, false);
            outcome
        }
        lookup => {
            let job = match lookup {
                Lookup::Revalidate(webp) => CellJob { cached_webp: Some(webp), ..job },
                _ => job,
            };
            if COLLECTING.load(Ordering::SeqCst) {
                // First pass: queue the cell and render a placeholder. The
                // builder runs the queue in parallel and compiles again.
                PENDING.lock().unwrap().entry(job.cell_id.clone()).or_insert(job);
                CellOutcome::default()
            } else {
                eprintln!("Running example {}...", job.cell_id);
                let outcome = run_cell(&job);
                record(&job.cell_id, true);
                outcome
            }
        }
    };

    // Build result for Typst
    let mut result = Dict::new();
    result.insert("code".into(), Value::Str(code_to_display.trim_end().into()));
    result.insert("show_code".into(), Value::Bool(show_code));
    result.insert("hide_code".into(), Value::Bool(hide_code));
    result.insert(
        "stdout".into(),
        Value::Str(without_preflight_report(&outcome.stdout).trim_end().into()),
    );
    result.insert("stderr".into(), Value::Str(outcome.stderr.trim_end().into()));
    result.insert("caption".into(), Value::Str(caption.as_str().into()));
    result.insert("webp".into(), Value::Str(outcome.webp.as_str().into()));
    result.insert("vars".into(), Value::Dict(Dict::new()));
    result.insert("own".into(), Value::Str(own.as_str().into()));

    if !outcome.stderr.is_empty() {
        engine.sink.warn(SourceDiagnostic::warning(
            span,
            eco_format!("{EXAMPLE_ERROR}\n{}", outcome.stderr),
        ));
    }

    Ok(Value::Dict(result))
}

/// Export options for the animated previews: small and light enough for a web
/// page (they sit next to the code), and fast to render on a CPU rasterizer.
/// Scenes with another aspect ratio are letterboxed.
const PREVIEW_ARGS: [&str; 7] = [
    "--quality", "draft", "--width", "960", "--height", "540", "--fit",
];
const PREVIEW_FIT: &str = "contain";

/// Prefix of the diagnostic a failing example emits; the builder counts them.
pub const EXAMPLE_ERROR: &str = "Execution error in Python cell:";

/// How the runtime runs a cell.
#[derive(Clone, Debug, PartialEq)]
enum CellMode {
    /// Export the scene and show the animation (`# output:`).
    Export(String),
    /// Build the scene without rendering (the cell calls `render()`).
    Check,
    /// Run authoring code against the embedded module; no scene is submitted.
    Validate,
}

/// Everything needed to run one cell, independent of Typst.
#[derive(Clone, Debug)]
struct CellJob {
    cell_id: String,
    hash: String,
    script: String,
    prelude_lines: usize,
    mode: CellMode,
    timeout_secs: u64,
    /// Preview from an earlier runtime. When set, the cell only needs a
    /// `check` to prove it still runs; the animation is kept.
    cached_webp: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct CellOutcome {
    stdout: String,
    stderr: String,
    webp: String,
}

enum Lookup {
    Hit(CellOutcome),
    /// Succeeded with the same code under another runtime fingerprint.
    Revalidate(String),
    Miss,
}

static COLLECTING: AtomicBool = AtomicBool::new(false);
static PENDING: LazyLock<Mutex<BTreeMap<String, CellJob>>> = LazyLock::new(Default::default);
static STATS: LazyLock<Mutex<RunStats>> = LazyLock::new(Default::default);

/// Identifies this build: a failure is reused only within the run that saw it.
static RUN_ID: LazyLock<String> = LazyLock::new(|| {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("{}-{nanos}", std::process::id())
});

/// The public API and runtime version a cached result was produced against.
/// When either changes, cached successes are proved again before reuse.
static FINGERPRINT: LazyLock<String> = LazyLock::new(|| {
    let stub = fs::read(project_root().join("../crates/gaanim_python/gaanim/gaanim_core.pyi"))
        .unwrap_or_default();
    format!(
        "{}-{:x}",
        env!("CARGO_PKG_VERSION"),
        typst_utils::hash128(&stub)
    )
});

/// Which examples ran or came from the cache during one compilation. Sets,
/// because Typst may evaluate a cell more than once per pass. Failures are
/// counted from the final diagnostics instead: early layout iterations can
/// evaluate a `# continue` cell against a chain that has not converged yet.
#[derive(Clone, Debug, Default)]
pub struct RunStats {
    pub executed: BTreeSet<String>,
    pub cached: BTreeSet<String>,
}

impl RunStats {
    /// Cells served from the cache that did not also run in this compilation.
    pub fn only_cached(&self) -> usize {
        self.cached.difference(&self.executed).count()
    }
}

fn record(cell_id: &str, executed: bool) {
    let mut stats = STATS.lock().unwrap();
    let set = if executed { &mut stats.executed } else { &mut stats.cached };
    set.insert(cell_id.to_string());
}

/// Start a compilation: forget the previous one's statistics.
pub fn reset_stats() {
    *STATS.lock().unwrap() = RunStats::default();
}

pub fn stats() -> RunStats {
    STATS.lock().unwrap().clone()
}

/// From now on, cells that need running are queued instead of run inline.
pub fn start_collecting() {
    PENDING.lock().unwrap().clear();
    COLLECTING.store(true, Ordering::SeqCst);
}

/// Stop queueing and run every queued cell, `jobs` at a time. Returns how
/// many ran; the caller must compile again (with memoization evicted) to show
/// their results.
pub fn run_collected(jobs: usize) -> usize {
    COLLECTING.store(false, Ordering::SeqCst);
    let pending: Vec<CellJob> = std::mem::take(&mut *PENDING.lock().unwrap())
        .into_values()
        .collect();
    if pending.is_empty() {
        return 0;
    }
    eprintln!("Running {} examples, {} at a time...", pending.len(), jobs);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(jobs.max(1))
        .build()
        .expect("example thread pool");
    pool.install(|| {
        pending.par_iter().for_each(|job| {
            run_cell(job);
            record(&job.cell_id, true);
        });
    });
    pending.len()
}

/// Delete cached results and previews that no cell of the last compilation
/// used, so the cache (and CI's saved copy of it) does not grow forever.
/// Returns how many files were removed.
pub fn prune_unused() -> usize {
    let stats = stats();
    let used = |id: &str| stats.executed.contains(id) || stats.cached.contains(id);
    let root = project_root();
    let mut removed = 0;
    let dirs = [
        (root.join("target/code_cache"), ".json"),
        (root.join("assets/generated"), "_anim.webp"),
    ];
    for (dir, suffix) in dirs {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(id) = name.strip_suffix(suffix)
                && !used(id)
                && fs::remove_file(entry.path()).is_ok()
            {
                removed += 1;
            }
        }
    }
    removed
}

fn project_root() -> PathBuf {
    PROJECT_ROOT
        .read()
        .unwrap()
        .clone()
        .unwrap_or_else(|| PathBuf::from("."))
}

fn cache_file(cell_id: &str) -> PathBuf {
    project_root()
        .join("target/code_cache")
        .join(format!("{cell_id}.json"))
}

fn lookup_cache(job: &CellJob) -> Lookup {
    let Ok(data) = fs::read_to_string(cache_file(&job.cell_id)) else {
        return Lookup::Miss;
    };
    let Ok(cache) = serde_json::from_str::<serde_json::Value>(&data) else {
        return Lookup::Miss;
    };
    let text = |key: &str| cache[key].as_str().unwrap_or("").to_string();
    if text("hash") != job.hash {
        return Lookup::Miss;
    }
    let outcome = CellOutcome {
        stdout: strip_ansi_escape_codes(&text("stdout")),
        stderr: strip_ansi_escape_codes(&text("stderr")),
        webp: text("webp"),
    };
    // A failure is final for this build (both passes and the PDF see it) but
    // is retried by the next build.
    if !outcome.stderr.trim().is_empty() {
        return if text("run") == *RUN_ID { Lookup::Hit(outcome) } else { Lookup::Miss };
    }
    let root = project_root();
    let webp_is_valid = !outcome.webp.is_empty() && is_valid_webp(&root.join(&outcome.webp));
    let exports = matches!(job.mode, CellMode::Export(_));
    if exports && !webp_is_valid || !exports && !outcome.webp.is_empty() && !webp_is_valid {
        return Lookup::Miss;
    }
    if text("fingerprint") == *FINGERPRINT {
        Lookup::Hit(outcome)
    } else if exports {
        Lookup::Revalidate(outcome.webp)
    } else {
        Lookup::Miss
    }
}

fn core_binary() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| project_root().join("target/debug"))
        .join(if cfg!(windows) { "gaanim-core.exe" } else { "gaanim-core" })
}

/// Run one cell and cache its outcome. Never fails: problems become the
/// cell's error text so the page shows them next to the code.
fn run_cell(job: &CellJob) -> CellOutcome {
    let root = project_root();
    // Each cell needs its own working directory: cells run in parallel and
    // most export to the same name (`preview.webp`).
    let work_dir = root.join("target/code_cells").join(&job.cell_id);
    let _ = fs::create_dir_all(&work_dir);
    link_fixtures(&root.join("fixtures"), &work_dir);
    // The script lives next to the fixtures, as `main.py` does in a project,
    // so calls that resolve paths from the script (`load_project()`) work.
    let temp_file = work_dir.join("main.py");
    let _ = fs::write(&temp_file, &job.script);

    let mut outcome = execute(job, &work_dir, &temp_file);

    if outcome.stderr.is_empty() {
        if let Some(webp) = &job.cached_webp {
            outcome.webp = webp.clone();
        } else if let CellMode::Export(name) = &job.mode {
            match collect_webp(&root, &work_dir.join(name), &job.cell_id) {
                Some(path) => outcome.webp = path,
                None => {
                    outcome.stderr = format!(
                        "The export finished without writing a valid `{name}` preview."
                    )
                }
            }
        }
    }

    let cache = serde_json::json!({
        "hash": job.hash,
        "fingerprint": *FINGERPRINT,
        "run": *RUN_ID,
        "webp": outcome.webp,
        "stdout": outcome.stdout,
        "stderr": outcome.stderr,
    });
    let _ = fs::create_dir_all(root.join("target/code_cache"));
    let _ = fs::write(cache_file(&job.cell_id), cache.to_string());
    let _ = fs::remove_dir_all(&work_dir);
    outcome
}

fn execute(job: &CellJob, work_dir: &Path, temp_file: &Path) -> CellOutcome {
    let program = "gaanim-core";
    let mut command = Command::new(core_binary());
    match &job.mode {
        // A preview that only needs revalidating is checked, not rendered.
        CellMode::Export(_) if job.cached_webp.is_some() => {
            command.arg("check").arg(temp_file);
        }
        CellMode::Export(output) => {
            command
                .arg("export")
                .arg(temp_file)
                .arg("--output")
                .arg(output)
                .args(PREVIEW_ARGS)
                .arg(PREVIEW_FIT);
        }
        CellMode::Check => {
            command.arg("check").arg(temp_file);
        }
        // Fragments run against the embedded module without submitting a scene.
        CellMode::Validate => {
            command.arg("--validate-python-api").arg(temp_file);
        }
    }
    command
        .current_dir(work_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8");

    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return CellOutcome {
                stderr: format!("Could not start {program}: {error}"),
                ..Default::default()
            };
        }
    };
    let pid = child.id();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });

    match rx.recv_timeout(StdDuration::from_secs(job.timeout_secs)) {
        Ok(Ok(output)) => {
            let raw_stderr = strip_ansi_escape_codes(&String::from_utf8_lossy(&output.stderr));
            let mut stderr = adjust_stderr_line_numbers(
                &raw_stderr,
                &temp_file.to_string_lossy(),
                job.prelude_lines,
            );
            if let Some(message) = silent_failure(
                program,
                output.status.success(),
                &output.status.to_string(),
                &stderr,
            ) {
                stderr = message;
            }
            // A run that succeeded may still log (the windowed 3D export does);
            // only other output, such as a Typst warning, marks it as failed.
            if output.status.success() {
                stderr = without_runtime_logs(&stderr);
            }
            CellOutcome {
                stdout: strip_ansi_escape_codes(&String::from_utf8_lossy(&output.stdout)),
                stderr: stderr.trim().to_string(),
                webp: String::new(),
            }
        }
        Ok(Err(error)) => CellOutcome {
            stderr: format!("Could not wait for {program}: {error}"),
            ..Default::default()
        },
        Err(_) => {
            #[cfg(target_os = "windows")]
            let _ = Command::new("taskkill")
                .args(["/F", "/T", "/PID", &pid.to_string()])
                .output();
            #[cfg(not(target_os = "windows"))]
            let _ = Command::new("kill").args(["-9", &pid.to_string()]).output();
            CellOutcome {
                stderr: format!(
                    "Timeout: the example ran longer than {} seconds (`# timeout:` raises the limit).",
                    job.timeout_secs
                ),
                ..Default::default()
            }
        }
    }
}

/// Make the sample project in `docs/fixtures` (manifest, images, fonts…)
/// visible from a cell's working directory, so examples that load files by
/// relative path run as they would in a reader's project.
fn link_fixtures(fixtures: &Path, work_dir: &Path) {
    let Ok(entries) = fs::read_dir(fixtures) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let target = work_dir.join(entry.file_name());
        #[cfg(unix)]
        let _ = std::os::unix::fs::symlink(entry.path(), &target);
        #[cfg(not(unix))]
        let _ = copy_recursively(&entry.path(), &target);
    }
}

#[cfg(not(unix))]
fn copy_recursively(source: &Path, target: &Path) -> std::io::Result<()> {
    if source.is_dir() {
        fs::create_dir_all(target)?;
        for entry in fs::read_dir(source)?.filter_map(Result::ok) {
            copy_recursively(&entry.path(), &target.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(source, target).map(|_| ())
    }
}

/// Move a finished export into `assets/generated` and return its site path.
fn collect_webp(root: &Path, source: &Path, cell_id: &str) -> Option<String> {
    // Wait for the exporter to finish flushing (Windows keeps a file lock).
    for _ in 0..15 {
        let size = || fs::metadata(source).map(|m| m.len()).unwrap_or(0);
        if source.exists() {
            let before = size();
            std::thread::sleep(StdDuration::from_millis(120));
            if before == size() && before > 1024 {
                break;
            }
        } else {
            std::thread::sleep(StdDuration::from_millis(80));
        }
    }
    if !is_valid_webp(source) {
        return None;
    }
    let dir = root.join("assets/generated");
    fs::create_dir_all(&dir).ok()?;
    let name = format!("{cell_id}_anim.webp");
    let dest = dir.join(&name);
    // Windows: rename fails while the source is locked; fall back to a copy.
    if fs::rename(source, &dest).is_err() {
        fs::copy(source, &dest).ok()?;
        let _ = fs::remove_file(source);
    }
    dest.exists().then(|| format!("assets/generated/{name}"))
}

#[cfg(test)]
mod tests {
    use super::{
        adjust_stderr_line_numbers, has_valid_webp_signature, silent_failure,
        strip_ansi_escape_codes, without_preflight_report, without_render_calls,
        without_runtime_logs,
    };

    #[test]
    fn the_check_report_is_not_part_of_what_a_cell_printed() {
        let stdout = "hola\nScene preflight: /tmp/main.py\n  PASS with 1 warning\n";
        assert_eq!(without_preflight_report(stdout), "hola\n");
        assert_eq!(without_preflight_report("Scene preflight: x\n"), "");
        assert_eq!(without_preflight_report("no report"), "no report");
    }

    #[test]
    fn runtime_logs_are_not_errors_but_other_output_is() {
        let stderr = "2026-09-25T22:46:49.427640Z  WARN winit: error setting XSETTINGS\n\
                      2026-09-25T22:46:50.753327Z ERROR bevy_render: slab\n";
        assert_eq!(without_runtime_logs(stderr), "");
        let colored = "\u{1b}[2m2026-09-25T22:57:48.171032Z\u{1b}[0m \u{1b}[33m WARN\u{1b}[0m bevy_audio";
        assert_eq!(without_runtime_logs(&strip_ansi_escape_codes(colored)), "");
        assert_eq!(
            without_runtime_logs("ALSA lib pcm.c:2721:(snd_pcm_open_noupdate) Unknown PCM default"),
            ""
        );
        assert_eq!(
            without_runtime_logs("Typst warning: unknown font family: cascadia mono"),
            "Typst warning: unknown font family: cascadia mono"
        );
    }

    #[test]
    fn a_replayed_cell_keeps_its_code_but_not_its_render_call() {
        let code = "scene = Scene()\ncircle = scene.geometry.circle(1)\n  scene.render()\n";
        assert_eq!(
            without_render_calls(code),
            "scene = Scene()\ncircle = scene.geometry.circle(1)\n"
        );
    }

    #[test]
    fn a_process_that_fails_silently_reports_its_exit_status() {
        let message = silent_failure("gaanim-core", false, "exit code: 0xc0000135", "").unwrap();
        assert!(message.starts_with("gaanim-core exited with exit code: 0xc0000135"));
        assert_eq!(
            silent_failure("gaanim-core", true, "exit code: 0", ""),
            None
        );
        assert_eq!(
            silent_failure(
                "python",
                false,
                "exit code: 1",
                "Traceback (most recent call last)"
            ),
            None
        );
    }

    #[test]
    fn cached_diagnostics_do_not_replace_empty_paths_between_every_character() {
        let message = "File code cell, line 7, in <module>\nValueError: invalid input";
        assert_eq!(adjust_stderr_line_numbers(message, "", 0), message);
    }

    #[test]
    fn validates_the_riff_webp_signature() {
        assert!(has_valid_webp_signature(b"RIFF\x04\x00\x00\x00WEBP"));
        assert!(!has_valid_webp_signature(b"not a webp file"));
        assert!(!has_valid_webp_signature(b"RIFF"));
    }

    #[test]
    fn suppresses_the_optional_consolas_fallback_warning() {
        assert_eq!(
            adjust_stderr_line_numbers(
                "Typst warning: unknown font family: consolas",
                "temp.py",
                0,
            ),
            ""
        );
    }
}
