//! `gaanim-play`: plays and exports playback bundles (`.gaanim`).
//!
//! A bundle holds every frame a scene drew, so this binary never runs
//! Python and does not link it: a shared presentation or video opens on a
//! machine with nothing but Gaanim installed.

use std::path::PathBuf;

use gaanim_core::console;
use gaanim_editor::bundle_player;
use gaanim_editor::cli::{CommandError, ExportCommand};
use gaanim_editor::host::{HostOptions, host_app};

const USAGE: &str = "\
Plays a Gaanim playback bundle, without Python.

Usage:
  gaanim-play [--present [--monitor N]] [--sections A,B | --from A] <BUNDLE.gaanim>
  gaanim-play export <BUNDLE.gaanim> --output <FILE> [--quality Q] [--width W]
              [--height H] [--fit error|contain|cover] [--from T] [--to T]
              [--transparent] [--encoder E]
  gaanim-play check <BUNDLE.gaanim>   Recompose every frame and compare it
                                      with the digest recorded for it

Record a bundle with `gaanim export scene.py --output scene.gaanim`.";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if matches!(args.first().map(String::as_str), Some("--version" | "-V")) {
        println!("gaanim-play {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{USAGE}");
        return;
    }
    if args.first().map(String::as_str) == Some("export") {
        let command = ExportCommand::parse(&args[1..])
            .map_err(CommandError::Usage)
            .unwrap_or_else(|error| error.exit("export"));
        console::banner("Export");
        if let Err(error) = gaanim_editor::cli::export_bundle_video(&command) {
            error.exit("export");
        }
        return;
    }
    if args.first().map(String::as_str) == Some("check") {
        match args.get(1..).unwrap_or_default() {
            [bundle] if bundle_player::is_bundle_path(std::path::Path::new(bundle)) => {
                check_bundle(std::path::Path::new(bundle))
                    .unwrap_or_else(|error| error.exit("check"));
            }
            _ => {
                CommandError::Usage("usage: gaanim-play check <BUNDLE.gaanim>".into()).exit("check")
            }
        }
        return;
    }
    let (bundle, options) = parse_play_args(&args).unwrap_or_else(|error| {
        console::error("usage", error);
        console::hint("Run `gaanim-play --help` for usage.");
        std::process::exit(2);
    });

    #[cfg(target_os = "linux")]
    gaanim_editor::alsa_errors::route_alsa_errors();
    console::banner(if options.present {
        "Presentation"
    } else {
        "Playback"
    });
    let mut app = host_app(&options);
    if let Err(error) = bundle_player::open_bundle(app.world_mut(), &bundle) {
        console::error("bundle", error);
        std::process::exit(2);
    }
    app.run();
}

/// Open `path`, describe it, and recompose every frame against its digest.
fn check_bundle(path: &std::path::Path) -> Result<(), CommandError> {
    let mut bundle = gaanim_bundle::Bundle::open(path)
        .map_err(|error| CommandError::Failed(format!("{}: {error}", path.display())))?;
    let scene = &bundle.scene;
    let stops: usize = scene
        .segments
        .iter()
        .map(|segment| segment.stops.len())
        .sum();
    console::info("check", format!("{} · {}", path.display(), scene.title));
    console::detail(
        "Frames",
        format!(
            "{} at {} fps · {:.2} seconds · {}×{}",
            bundle.frame_count(),
            scene.fps,
            scene.duration,
            scene.output_size.0,
            scene.output_size.1
        ),
    );
    console::detail(
        "Structure",
        format!(
            "{} segments · {stops} stops · {} markers · {} audio tracks",
            scene.segments.len(),
            scene.markers.len(),
            scene.audio.len()
        ),
    );
    let mismatched = bundle
        .verify()
        .map_err(|error| CommandError::Failed(error.to_string()))?;
    if let Some((index, time)) = mismatched.first() {
        return Err(CommandError::Failed(format!(
            "{} of {} frames differ from the recording, first frame {index} at {time:.3}s",
            mismatched.len(),
            bundle.frame_count()
        )));
    }
    console::success("pass", "every frame composes as it was recorded");
    Ok(())
}

fn parse_play_args(args: &[String]) -> Result<(PathBuf, HostOptions), String> {
    let mut bundle = None;
    let mut options = HostOptions::default();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        index += 1;
        match arg.as_str() {
            "--present" => options.present = true,
            "--sections" | "--from" => {
                let value = args
                    .get(index)
                    .ok_or_else(|| format!("{arg} requires a segment or section name"))?;
                index += 1;
                if arg == "--from" {
                    options.selection.from = Some(value.clone());
                } else {
                    options.selection.sections =
                        gaanim_timeline::selection::SegmentSelection::parse_list(value)?;
                }
            }
            "--monitor" => {
                let value = args
                    .get(index)
                    .ok_or_else(|| "--monitor requires a zero-based monitor index".to_string())?;
                index += 1;
                options.monitor = Some(
                    value
                        .parse()
                        .map_err(|_| "--monitor must be a non-negative integer".to_string())?,
                );
            }
            value if value.starts_with('-') => return Err(format!("unknown option `{value}`")),
            value if bundle.is_none() => bundle = Some(PathBuf::from(value)),
            value => return Err(format!("unexpected argument `{value}`")),
        }
    }
    if options.monitor.is_some() && !options.present {
        return Err("--monitor requires --present".to_string());
    }
    let bundle = bundle.ok_or_else(|| "a bundle (.gaanim) to play is required".to_string())?;
    if !bundle_player::is_bundle_path(&bundle) {
        return Err(format!(
            "{} is not a playback bundle; scripts and projects open with `gaanim`",
            bundle.display()
        ));
    }
    Ok((bundle, options))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn parses_presentation_launches() {
        let (bundle, options) =
            parse_play_args(&args(&["--present", "--monitor", "1", "talk.gaanim"])).unwrap();
        assert_eq!(bundle, PathBuf::from("talk.gaanim"));
        assert!(options.present);
        assert_eq!(options.monitor, Some(1));
        let (_, options) = parse_play_args(&args(&["--from", "demo", "talk.GAANIM"])).unwrap();
        assert_eq!(options.selection.from.as_deref(), Some("demo"));

        for invalid in [
            &[][..],
            &["scene.py"],
            &["--monitor", "1", "talk.gaanim"],
            &["talk.gaanim", "other.gaanim"],
            &["--loop", "talk.gaanim"],
        ] {
            assert!(parse_play_args(&args(invalid)).is_err(), "{invalid:?}");
        }
    }
}
