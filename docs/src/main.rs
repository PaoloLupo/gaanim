mod args;
mod execution;
mod world;

use std::{
    io::Write,
    path::{Path, PathBuf},
    process::ExitCode,
};

use anyhow::Result;
use clap::Parser;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use typst::{comemo, diag::Warned};
use typst_bundle::{Bundle, BundleOptions, VirtualFs};
use typst_kit::{
    diagnostics::{self, termcolor},
    server::HttpServer,
    timer::Timer,
    watcher::Watcher,
};

use crate::{
    args::{Cli, Command, CompileCommand, WatchCommand},
    world::DocWorld,
};

const SITE_PATH: &str = "dist/site";

fn main() -> Result<ExitCode> {
    let cli = Cli::parse();

    match &cli.command {
        Command::Compile(command) => compile(command),
        Command::Watch(command) => watch(command),
    }
}

fn compile(command: &CompileCommand) -> Result<ExitCode> {
    let mut timer = Timer::new_or_placeholder(command.args.timings.clone());
    let mut config = Config::new(&command.args, false);
    let mut world = DocWorld::new(&config);
    let report = timer
        .record(&mut world, |world| compile_once(world, &mut config))
        .expect("Compilation failed");

    report.print(&world);

    if report.0.output.is_err() {
        return Ok(ExitCode::FAILURE);
    }

    Ok(ExitCode::SUCCESS)
}

fn watch(command: &WatchCommand) -> ! {
    let mut timer = Timer::new_or_placeholder(command.args.timings.clone());
    let mut watcher = Watcher::new(None).unwrap();
    let mut config = Config::new(&command.args, true);
    let mut world = DocWorld::new(&config);
    let mut last_duration: Option<std::time::Duration> = None;

    loop {
        print_watch_header(&config);
        if let Some(dur) = last_duration {
            writeln!(out(), "Last compilation took: {:.2?}", dur).unwrap();
            writeln!(out()).unwrap();
        }
        writeln!(out(), "compiling ...").unwrap();

        let start = std::time::Instant::now();
        let report = timer
            .record(&mut world, |world| compile_once(world, &mut config))
            .unwrap();
        let dur = start.elapsed();
        last_duration = Some(dur);

        print_watch_header(&config);
        report.print(&world);
        writeln!(out(), "Compiled in {:.2?}", dur).unwrap();

        comemo::evict(10);
        watcher.update(world.dependencies()).unwrap();
        watcher.wait().unwrap();
        world.reset();
    }
}

fn print_watch_header(config: &Config) {
    let mut out = out();
    let clear = "\x1B[2J\x1B[1;1H";
    write!(out, "{clear}").unwrap();
    if let Some(server) = &config.server {
        writeln!(out, "serving docs on http://{}", server.addr()).unwrap();
        writeln!(out).unwrap();
    }
    if let Some(path) = &config.output {
        writeln!(out, "writing to {}", path.display()).unwrap();
        writeln!(out).unwrap();
    }
}

struct Config {
    input: Option<PathBuf>,
    output: Option<PathBuf>,
    server: Option<HttpServer>,
    open: bool,
}

impl Config {
    fn new(args: &args::CompileArgs, watching: bool) -> Self {
        Self {
            input: args.input.clone(),
            output: args.output.clone().or_else(|| {
                if watching {
                    None
                } else {
                    Some(SITE_PATH.into())
                }
            }),
            server: watching.then(|| HttpServer::new("gaanim-docs", None, true).unwrap()),
            open: args.open,
        }
    }
}

fn out() -> termcolor::StandardStream {
    termcolor::StandardStream::stderr(termcolor::ColorChoice::Auto)
}

struct Report(Warned<typst::diag::SourceResult<()>>);

impl Report {
    fn print(&self, world: &DocWorld) {
        let Warned { output, warnings } = &self.0;
        let errors = output
            .as_ref()
            .err()
            .map(|v| v.as_slice())
            .unwrap_or_default();
        diagnostics::emit(
            &mut out(),
            world,
            errors.iter().chain(warnings),
            diagnostics::DiagnosticFormat::Human,
        )
        .unwrap();
    }
}

fn compile_once(world: &DocWorld, config: &mut Config) -> Report {
    let Warned { output, warnings } = typst::compile::<Bundle>(world);
    let result = output.and_then(|bundle| export_website(bundle, config));
    let mut warned = Warned {
        output: result,
        warnings,
    };

    if config.open && warned.output.is_ok() {
        if let Some(server) = &config.server {
            let url = format!("http://{}", server.addr());
            let _ = open::that_detached(url);
        } else if let Some(output) = &config.output {
            let path_to_open = output.join("index.html");
            if let Ok(abs_path) = path_to_open.canonicalize() {
                let _ = open::that_detached(abs_path);
            } else {
                let _ = open::that_detached(path_to_open);
            }
        }
        config.open = false;
    }

    warned
        .warnings
        .retain(|diag| !diag.message.starts_with("bundle export is experimental"));

    Report(warned)
}

fn export_website(bundle: Bundle, config: &Config) -> typst::diag::SourceResult<()> {
    let options = BundleOptions::default();
    let fs = typst_bundle::export(&bundle, &options)?;

    if let Some(path) = &config.output {
        write_virtual_fs(path, &fs);
    }

    if let Some(server) = &config.server {
        server.set_bundle(bundle, fs);
    }

    Ok(())
}

fn write_virtual_fs(root: &Path, fs: &VirtualFs) {
    std::fs::create_dir_all(root).unwrap();
    fs.par_iter().for_each(|(path, data)| {
        let realized = path.realize(root).unwrap();
        if let Some(parent) = realized.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&realized, data).unwrap();
    });
}
