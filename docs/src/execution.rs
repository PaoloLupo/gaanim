use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration as StdDuration,
};

use typst::{
    diag::{At, SourceDiagnostic, SourceResult, bail, eco_format},
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
        {
            continue;
        }

        let mut replaced = line.to_string();
        if (line.contains(&file_name) || line.contains(temp_file))
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
) -> SourceResult<Value> {
    let span = raw.span();

    if lang.as_str() != "python" && lang.as_str() != "py" {
        bail!(
            span,
            "Only Python is supported in gaanim docs, got: {}",
            lang
        );
    }

    let mut cmd = "python".to_string();
    let ext = "py";

    // Auto-detect venv (make absolute without resolving symlinks, so the venv
    // python is used even when current_dir is set to a subdirectory like docs/).
    if let Ok(cwd) = std::env::current_dir() {
        let venv_unix = cwd.join(".venv/bin/python");
        let venv_win = cwd.join(".venv/Scripts/python.exe");
        if venv_unix.exists() {
            cmd = venv_unix.to_string_lossy().to_string();
        } else if venv_win.exists() {
            cmd = venv_win.to_string_lossy().to_string();
        }
    }

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
    let mut timeout_secs: u64 = 120; // gaanim animations can take longer
    let mut caption = String::new();
    let mut target_cell: Option<String> = None;
    let mut cell_id_override: Option<String> = None;
    let mut expected_webp: Option<String> = None;

    for (line, _) in code_lines {
        let trimmed = line.trim();

        if trimmed == "# show-code: true" || trimmed == "# show-code" {
            show_code = true;
            continue;
        }
        if trimmed == "# show-code: false" || trimmed == "# hide-code" {
            show_code = false;
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

    // Hash and cell ID
    let cell_hash = typst_utils::hash128(code_to_execute.as_bytes());
    let cell_id = if !id.is_empty() {
        id.to_string()
    } else if let Some(ref override_id) = cell_id_override {
        override_id.clone()
    } else if let Some(ref name) = target_cell {
        format!("{}_{:x}", name, cell_hash)
    } else {
        format!("cell_{:x}", cell_hash)
    };
    let hex_hash = format!("{:x}", cell_hash);

    // Resolve project root for all file operations
    let project_root: PathBuf = {
        let guard = PROJECT_ROOT.read().unwrap();
        guard.clone().unwrap_or_else(|| PathBuf::from("."))
    };

    // Cache
    let cache_dir = project_root.join("target/code_cache");
    let cache_file = cache_dir.join(format!("{}.json", cell_id));
    fs::create_dir_all(&cache_dir).unwrap();

    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut webp_path = String::new();
    let mut executed = false;

    // Check cache
    if cache_file.exists()
        && let Ok(data) = fs::read_to_string(&cache_file)
        && let Ok(cache) = serde_json::from_str::<serde_json::Value>(&data)
        && cache["hash"].as_str() == Some(hex_hash.as_str())
    {
        let cached_webp = cache["webp"].as_str().unwrap_or("");
        // Cache hit only if webp file still exists on disk
        if cached_webp.is_empty() || project_root.join(cached_webp).exists() {
            stdout = strip_ansi_escape_codes(cache["stdout"].as_str().unwrap_or(""));
            stderr = strip_ansi_escape_codes(cache["stderr"].as_str().unwrap_or(""));
            webp_path = cached_webp.to_string();
            executed = true;
        }
    }

    // Execute if not cached
    if !executed {
        eprintln!("Executing Python cell (id: {})...", cell_id);

        let mut full_script = String::new();
        full_script.push_str(PYTHON_PRELUDE);
        full_script.push_str(&code_to_execute);

        let temp_file = project_root.join(format!("target/temp_{}.py", cell_id));
        fs::write(&temp_file, &full_script).unwrap();

        struct TempFileCleaner {
            path: PathBuf,
        }
        impl Drop for TempFileCleaner {
            fn drop(&mut self) {
                let _ = fs::remove_file(&self.path);
            }
        }
        let _cleaner = TempFileCleaner {
            path: temp_file.clone(),
        };

        let prelude_lines = PYTHON_PRELUDE.lines().count();

        let mut command = Command::new(&cmd);
        command
            .arg(&temp_file)
            .arg(&cell_id)
            .current_dir(&project_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("PYTHONUTF8", "1")
            .env("PYTHONIOENCODING", "utf-8");

        let child = command
            .spawn()
            .map_err(|e| eco_format!("Error executing Python: {}", e))
            .at(span)?;

        let pid = child.id();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(child.wait_with_output());
        });

        match rx.recv_timeout(StdDuration::from_secs(timeout_secs)) {
            Ok(Ok(output)) => {
                stdout = strip_ansi_escape_codes(&String::from_utf8_lossy(&output.stdout));
                let err_str = String::from_utf8_lossy(&output.stderr);
                stderr = adjust_stderr_line_numbers(
                    &err_str,
                    &temp_file.to_string_lossy(),
                    prelude_lines,
                );
            }
            Ok(Err(e)) => {
                bail!(span, "Error executing Python: {}", e);
            }
            Err(_) => {
                #[cfg(target_os = "windows")]
                {
                    let _ = Command::new("taskkill")
                        .args(["/F", "/T", "/PID", &pid.to_string()])
                        .output();
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let _ = Command::new("kill").args(["-9", &pid.to_string()]).output();
                }
                bail!(
                    span,
                    "Timeout: cell exceeded {} seconds limit.",
                    timeout_secs
                );
            }
        }

        // Detect output WebP
        if let Some(ref webp_name) = expected_webp {
            let src_path = project_root.join(webp_name);
            if src_path.exists() {
                let img_dir = project_root.join("assets/generated");
                fs::create_dir_all(&img_dir).unwrap();
                let dest_name = format!("{}_anim.webp", cell_id);
                let dest = img_dir.join(&dest_name);
                let _ = fs::rename(&src_path, &dest);
                webp_path = format!("assets/generated/{}", dest_name);
            } else {
                eprintln!(
                    "Warning: expected WebP '{}' not found after execution",
                    webp_name
                );
            }
        }

        // Save cache
        let cache = serde_json::json!({
            "hash": hex_hash,
            "webp": webp_path,
            "stdout": stdout,
            "stderr": stderr,
        });
        let _ = fs::write(&cache_file, serde_json::to_string(&cache).unwrap());
    }

    // Build result for Typst
    let mut result = Dict::new();
    result.insert("code".into(), Value::Str(code_to_display.trim_end().into()));
    result.insert("show_code".into(), Value::Bool(show_code));
    result.insert("stdout".into(), Value::Str(stdout.trim_end().into()));
    result.insert("stderr".into(), Value::Str(stderr.trim_end().into()));
    result.insert("caption".into(), Value::Str(caption.as_str().into()));
    result.insert("webp".into(), Value::Str(webp_path.as_str().into()));
    result.insert("vars".into(), Value::Dict(Dict::new()));

    // Emit warning on stderr
    if !stderr.is_empty() {
        engine.sink.warn(SourceDiagnostic::warning(
            span,
            eco_format!("Execution error in Python cell:\n{}", stderr),
        ));
    }

    Ok(Value::Dict(result))
}
