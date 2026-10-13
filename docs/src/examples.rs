//! Playable bundles of the example catalog (`examples/examples.toml`), which
//! the examples pages play in the embed player over each example's poster.
//!
//! Before the pages compile, every cataloged example is recorded once into
//! `assets/generated/examples/`, keyed by its script, the assets it may load
//! and the runtime, so a later build records only what changed.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use serde::Deserialize;
use typst::foundations::{Dict, IntoValue, Value, func};

use crate::execution::{
    FINGERPRINT, gaanim_binary, is_bundle, project_root, strip_ansi_escape_codes,
};

/// The preview frame rate keeps the bundles small, as for `# player` cells.
const RECORD_ARGS: [&str; 2] = ["--fps", "30"];

/// Bump to record every example again, e.g. when the recording settings
/// change or bundles recorded before were wrong (3: 3D and glass, #501;
/// 4: traced paths' ends, #507).
const REVISION: u32 = 4;

/// The longest an example may take to record.
const TIMEOUT: Duration = Duration::from_secs(600);

/// How `export` refuses content a bundle cannot hold yet (video): such an
/// example keeps only its poster.
const UNSUPPORTED: &str = "playback bundles cannot record";

#[derive(Deserialize)]
struct Catalog {
    example: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    path: String,
}

/// A recorded example: its bundle, relative to the docs root, and its size.
#[derive(Clone, Debug)]
struct Recorded {
    bundle: String,
    bytes: u64,
}

/// What [`prepare`] recorded or reused, by catalog path: `None` for an
/// example a bundle cannot hold, an error when recording failed.
type Outcome = Result<Option<Recorded>, String>;

static RECORDED: LazyLock<Mutex<BTreeMap<String, Outcome>>> = LazyLock::new(Default::default);

fn examples_dir() -> PathBuf {
    project_root().join("../examples")
}

/// The bundle of the example at catalog `path`, under the docs root.
fn bundle_path(path: &str) -> String {
    let id = path.trim_end_matches(".py").replace('/', "__");
    format!("assets/generated/examples/{id}.gaanim")
}

fn cache_path(path: &str) -> PathBuf {
    let id = path.trim_end_matches(".py").replace('/', "__");
    project_root()
        .join("target/code_cache/examples")
        .join(format!("{id}.json"))
}

/// Every file under `dir`, recursively, in a stable order.
fn files_under(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    entries.sort();
    for entry in entries {
        if entry.is_dir() {
            files_under(&entry, found);
        } else {
            found.push(entry);
        }
    }
}

/// What a recording of the example at `script` depends on besides the
/// runtime: the script and the assets next to it, or every file of its
/// project when it is one.
fn inputs(script: &Path) -> Vec<PathBuf> {
    let dir = script.parent().unwrap_or(Path::new("."));
    let mut found = Vec::new();
    if dir.join("gaanim.toml").is_file() {
        files_under(dir, &mut found);
    } else {
        found.push(script.to_path_buf());
        files_under(&dir.join("assets"), &mut found);
    }
    found
}

fn cache_key(script: &Path) -> String {
    let mut hashed: Vec<(String, Vec<u8>)> = vec![(
        format!("{REVISION} {} {}", *FINGERPRINT, RECORD_ARGS.join(" ")),
        Vec::new(),
    )];
    for file in inputs(script) {
        let name = file.to_string_lossy().replace('\\', "/");
        hashed.push((name, fs::read(&file).unwrap_or_default()));
    }
    format!("{:032x}", typst_utils::hash128(&hashed))
}

/// The cached recording of `path`, if it was made from the same inputs.
fn cached(path: &str, key: &str) -> Option<Option<Recorded>> {
    let cache: serde_json::Value =
        serde_json::from_slice(&fs::read(cache_path(path)).ok()?).ok()?;
    if cache["key"] != key {
        return None;
    }
    if cache["unsupported"] == true {
        return Some(None);
    }
    let bundle = bundle_path(path);
    let file = project_root().join(&bundle);
    is_bundle(&file).then(|| {
        Some(Recorded {
            bytes: fs::metadata(&file)
                .map(|meta| meta.len())
                .unwrap_or_default(),
            bundle,
        })
    })
}

fn write_cache(path: &str, cache: serde_json::Value) {
    let file = cache_path(path);
    if let Some(parent) = file.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&file, cache.to_string());
}

/// Record the example at catalog `path` into its bundle.
fn record(path: &str, key: &str) -> Outcome {
    let script = examples_dir().join(path);
    let bundle = bundle_path(path);
    let output = project_root().join(&bundle);
    // `export` picks the format from the extension.
    let partial = output.with_extension("partial.gaanim");
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let _ = fs::remove_file(&partial);
    let mut child = Command::new(gaanim_binary())
        .arg("export")
        .arg(&script)
        .arg("--output")
        .arg(&partial)
        .args(RECORD_ARGS)
        .current_dir(script.parent().unwrap_or(Path::new(".")))
        .env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not start gaanim: {error}"))?;
    let stderr = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut stderr) = stderr {
            let _ = std::io::Read::read_to_string(&mut stderr, &mut text);
        }
        text
    });
    let started = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        if started.elapsed() > TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "recording took longer than {} s",
                TIMEOUT.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let stderr = strip_ansi_escape_codes(&reader.join().unwrap_or_default());
    if stderr.contains(UNSUPPORTED) {
        let _ = fs::remove_file(&partial);
        write_cache(path, serde_json::json!({ "key": key, "unsupported": true }));
        return Ok(None);
    }
    if !status.success() || !is_bundle(&partial) {
        let _ = fs::remove_file(&partial);
        let tail: Vec<&str> = stderr.trim().lines().rev().take(12).collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        return Err(format!("recording failed ({status}):\n{}", tail.join("\n")));
    }
    fs::rename(&partial, &output).map_err(|error| error.to_string())?;
    write_cache(path, serde_json::json!({ "key": key }));
    Ok(Some(Recorded {
        bytes: fs::metadata(&output)
            .map(|meta| meta.len())
            .unwrap_or_default(),
        bundle,
    }))
}

/// Record every cataloged example whose bundle is missing or stale, `jobs`
/// at a time, and return how many could not be recorded.
pub fn prepare(jobs: usize) -> usize {
    let catalog = match fs::read_to_string(examples_dir().join("examples.toml"))
        .map_err(|error| error.to_string())
        .and_then(|text| toml::from_str::<Catalog>(&text).map_err(|error| error.to_string()))
    {
        Ok(catalog) => catalog,
        Err(error) => {
            eprintln!("Could not read examples/examples.toml: {error}");
            return 1;
        }
    };
    let mut results = BTreeMap::new();
    let mut stale = Vec::new();
    for entry in catalog.example {
        let key = cache_key(&examples_dir().join(&entry.path));
        match cached(&entry.path, &key) {
            Some(recorded) => {
                results.insert(entry.path, Ok(recorded));
            }
            None => stale.push((entry.path, key)),
        }
    }
    if !stale.is_empty() {
        eprintln!(
            "Recording {} example bundles, {} at a time...",
            stale.len(),
            jobs
        );
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(jobs.max(1))
            .build()
            .expect("example bundle thread pool");
        let recorded: Vec<_> = pool.install(|| {
            stale
                .par_iter()
                .map(|(path, key)| (path.clone(), record(path, key)))
                .collect()
        });
        results.extend(recorded);
    }
    let failed = results.values().filter(|result| result.is_err()).count();
    for (path, result) in &results {
        if let Err(error) = result {
            eprintln!("Example bundle of examples/{path}: {error}");
        }
    }
    *RECORDED.lock().unwrap() = results;
    failed
}

/// The bundle of the example at catalog `path`: `(path: <under the docs
/// root>, bytes: <size>)`, or `none` when it has none.
#[func]
pub fn example_bundle(path: ecow::EcoString) -> Value {
    match RECORDED.lock().unwrap().get(path.as_str()) {
        Some(Ok(Some(recorded))) => {
            let mut dict = Dict::new();
            dict.insert("path".into(), recorded.bundle.as_str().into_value());
            dict.insert("bytes".into(), (recorded.bytes as i64).into_value());
            Value::Dict(dict)
        }
        _ => Value::None,
    }
}
