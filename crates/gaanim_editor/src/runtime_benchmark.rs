//! Internal runtime benchmark entrypoints used by the repository harness.

use bevy::prelude::World;
use pyo3::prelude::*;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReloadBenchmarkArgs {
    script: PathBuf,
    output: PathBuf,
}

#[derive(Debug, Serialize)]
struct ReloadBenchmarkReport {
    schema_version: u32,
    python_ms: f64,
    replay_ms: f64,
    total_ms: f64,
    width: u32,
    height: u32,
}

fn parse_reload_benchmark_args(args: &[String]) -> Result<ReloadBenchmarkArgs, String> {
    let mut script = None;
    let mut output = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--output" => {
                index += 1;
                output = args.get(index).map(PathBuf::from);
                if output.is_none() {
                    return Err("--output requires a JSON path".to_string());
                }
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown option `{value}`"));
            }
            value if script.is_none() => script = Some(PathBuf::from(value)),
            value => return Err(format!("unexpected argument `{value}`")),
        }
        index += 1;
    }
    Ok(ReloadBenchmarkArgs {
        script: script.ok_or_else(|| "missing <SCRIPT_OR_PROJECT>".to_string())?,
        output: output.ok_or_else(|| "--output is required".to_string())?,
    })
}

fn benchmark_reload(script: &Path, output: &Path) -> Result<ReloadBenchmarkReport, String> {
    let script = gaanim_project::resolve_entry(script)?;
    let probe = gaanim_project::EnvironmentProbe::detect(Some(&script));
    let venv_root = gaanim_project::activate_environment(&probe)?;
    gaanim_python::register_inittab();
    Python::initialize();
    if let Some(ref venv) = venv_root {
        crate::python_home::inject_venv_site_packages(venv);
    }

    // Prime the same interpreter and ECS world that the measured reload will
    // reuse. Startup, environment discovery, and the first import are excluded.
    let initial_canvas = crate::script_runner::load_script_canvas(&script)?;
    let mut world = World::new();
    world.insert_resource(gaanim_timeline::timeline::Timeline::default());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    world.insert_resource(gaanim_renderer::pipeline::GaanimRenderCache::default());
    crate::hot_reload::reload_with(&mut world, initial_canvas);

    let python_started = Instant::now();
    let canvas = crate::script_runner::load_script_canvas(&script)?;
    let python_ms = python_started.elapsed().as_secs_f64() * 1000.0;

    let (width, height) = canvas.frame.preview_pixel_size();
    let replay_started = Instant::now();
    crate::hot_reload::reload_with(&mut world, canvas);
    let replay_ms = replay_started.elapsed().as_secs_f64() * 1000.0;
    let report = ReloadBenchmarkReport {
        schema_version: 1,
        python_ms,
        replay_ms,
        total_ms: python_ms + replay_ms,
        width,
        height,
    };

    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("could not create benchmark output directory: {error}"))?;
    }
    let encoded = serde_json::to_vec_pretty(&report)
        .map_err(|error| format!("could not encode reload benchmark: {error}"))?;
    std::fs::write(output, encoded)
        .map_err(|error| format!("could not write {}: {error}", output.display()))?;
    Ok(report)
}

pub fn dispatch_reload_benchmark_mode() -> bool {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("--benchmark-reload") {
        return false;
    }
    let parsed = parse_reload_benchmark_args(&args[1..]).unwrap_or_else(|error| {
        eprintln!("gaanim reload benchmark: {error}");
        eprintln!(
            "usage: gaanim-core --benchmark-reload <SCRIPT_OR_PROJECT> --output <REPORT.json>"
        );
        std::process::exit(2);
    });
    match benchmark_reload(&parsed.script, &parsed.output) {
        Ok(report) => println!(
            "Persistent reload: Python {:.2}ms + replay {:.2}ms = {:.2}ms",
            report.python_ms, report.replay_ms, report.total_ms
        ),
        Err(error) => {
            eprintln!("gaanim reload benchmark: {error}");
            std::process::exit(1);
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_reload_benchmark_paths_in_any_order() {
        let args = ["--output", "target/reload.json", "examples/demo.py"].map(str::to_string);
        assert_eq!(
            parse_reload_benchmark_args(&args).unwrap(),
            ReloadBenchmarkArgs {
                script: PathBuf::from("examples/demo.py"),
                output: PathBuf::from("target/reload.json"),
            }
        );
    }

    #[test]
    fn reload_benchmark_requires_script_and_output() {
        assert!(parse_reload_benchmark_args(&[]).is_err());
        assert!(parse_reload_benchmark_args(&["scene.py".to_string()]).is_err());
        assert!(
            parse_reload_benchmark_args(&["scene.py".to_string(), "--bad".to_string()]).is_err()
        );
    }
}
