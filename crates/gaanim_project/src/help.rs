//! Help screens of the `gaanim` command line, shared by the launcher and the
//! core binary so both print the same text.

use gaanim_core::console::{self, Style};

/// A block of a help screen.
enum Block {
    /// Two-column rows, such as options and their descriptions. A `\n` in a
    /// description continues it on the next line, aligned with the column.
    Rows(&'static str, &'static [(&'static str, &'static str)]),
    /// Free-standing lines, such as usage forms.
    Lines(&'static str, &'static [&'static str]),
}

struct Page {
    /// One sentence under the title, saying what the command is for.
    summary: &'static str,
    blocks: &'static [Block],
    footer: &'static [&'static str],
}

/// Which help screen to print.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Topic {
    General,
    Init,
    Export,
    Check,
    Diff,
}

const DOCS: &str = "Documentation: https://paololupo.github.io/gaanim/";

const GENERAL: Page = Page {
    summary: "Write animations in Python, preview them live, and export videos or slides.",
    blocks: &[
        Block::Rows(
            "Usage",
            &[
                ("gaanim", "Open the Home screen"),
                (
                    "gaanim [OPTIONS] <SCRIPT_OR_PROJECT>",
                    "Preview a script or project; it reloads on every save",
                ),
                ("gaanim <COMMAND> [ARGS]", "Run one of the commands below"),
            ],
        ),
        Block::Rows(
            "Commands",
            &[
                (
                    "init <video|slides> [DIR]",
                    "Create a project with a minimal main.py and its\nPython environment",
                ),
                (
                    "check <SCRIPT_OR_PROJECT>",
                    "Find problems before you export or present",
                ),
                (
                    "export <SCRIPT_OR_PROJECT>",
                    "Render to MP4, WebM, WebP, GIF, or a PNG sequence",
                ),
                (
                    "--diff --example <SCRIPT>",
                    "Compare snapshots with approved baselines",
                ),
            ],
        ),
        Block::Rows(
            "Preview options",
            &[
                (
                    "--present",
                    "Present fullscreen, with a presenter window for notes",
                ),
                (
                    "--monitor <INDEX>",
                    "Monitor used by --present (default: the primary one)",
                ),
                (
                    "--sections <LIST>",
                    "Play only these segments or Section keys, e.g. intro,results",
                ),
                (
                    "--from <NAME>",
                    "Start at this segment or Section key and play to the end;\nthe whole scene is still built, so earlier state is kept",
                ),
                (
                    "-h, --help",
                    "Print help (`gaanim <COMMAND> --help` for a command)",
                ),
                ("-V, --version", "Print the version"),
            ],
        ),
        Block::Rows(
            "Presentation keys",
            &[
                ("→  Enter", "Next step"),
                ("←  Backspace", "Previous step"),
                ("Space", "Play or pause the current step"),
                ("Home  End", "First or last step"),
                ("O", "Overview of every slide, with search"),
                ("B  W", "Black or white screen for the audience"),
                ("P", "Reopen the presenter window"),
            ],
        ),
        Block::Rows(
            "Examples",
            &[
                (
                    "gaanim init video intro",
                    "Create a video project in ./intro",
                ),
                ("gaanim intro", "Preview it; save main.py to see changes"),
                ("gaanim check intro", "Look for problems in the scene"),
                (
                    "gaanim export intro -o intro.mp4",
                    "Render a 1080p, 60 fps MP4",
                ),
                (
                    "gaanim --present --monitor 1 talk",
                    "Present the slides project in ./talk",
                ),
            ],
        ),
        Block::Rows(
            "Environment",
            &[
                (
                    "RUST_LOG=info",
                    "Also show engine messages (Bevy, wgpu, winit), which are\nhidden unless they are errors",
                ),
                ("NO_COLOR=1", "Print without colours"),
                (
                    "GAANIM_INCREMENTAL=0",
                    "Rebuild the whole scene on every reload",
                ),
            ],
        ),
    ],
    footer: &[
        "Run `gaanim <COMMAND> --help` for the options of a command.",
        DOCS,
    ],
};

const INIT: Page = Page {
    summary: "Create a runnable project: gaanim.toml, a minimal main.py, and a Python 3.14\nenvironment with the authoring package, so editors autocomplete the API.",
    blocks: &[
        Block::Lines("Usage", &["gaanim init <KIND> [DIRECTORY] [--force]"]),
        Block::Rows(
            "Arguments",
            &[
                ("video", "An animation rendered to a video file"),
                (
                    "slides",
                    "A presentation: each segment is a slide and stop() waits\nfor you to advance",
                ),
                ("DIRECTORY", "Where to create it (default: gaanim-<KIND>)"),
            ],
        ),
        Block::Rows(
            "Options",
            &[
                (
                    "--force",
                    "Refresh the scaffold files of an existing project; your\nscripts' assets are kept",
                ),
                ("-h, --help", "Print this help"),
            ],
        ),
        Block::Rows(
            "Examples",
            &[
                ("gaanim init video", "Create ./gaanim-video"),
                ("gaanim init slides talk", "Create a presentation in ./talk"),
            ],
        ),
    ],
    footer: &["Then preview it with `gaanim <DIRECTORY>`.", DOCS],
};

const EXPORT: Page = Page {
    summary: "Render a script or project to a video, an animated image, or PNG frames.",
    blocks: &[
        Block::Lines(
            "Usage",
            &["gaanim export <SCRIPT_OR_PROJECT> --output <FILE> [OPTIONS]"],
        ),
        Block::Rows(
            "Formats (chosen by the --output extension)",
            &[
                (".mp4", "H.264 video with audio (needs FFmpeg)"),
                (
                    ".webm",
                    "VP9 video with audio (needs FFmpeg; supports --transparent)",
                ),
                (
                    ".webp",
                    "Animated WebP (needs FFmpeg; supports --transparent)",
                ),
                (".gif", "Animated GIF (needs FFmpeg)"),
                (
                    ".png",
                    "One PNG per frame (no FFmpeg; supports --transparent). A %d\nor %0Nd in the name becomes the frame number\n(frames/f_%04d.png -> f_0000.png); otherwise it is appended\n(frame.png -> frame_00000.png)",
                ),
            ],
        ),
        Block::Rows(
            "Options",
            &[
                ("-o, --output <FILE>", "Output file (required)"),
                (
                    "--quality <PRESET>",
                    "draft (30 fps, fast), standard (60 fps, default), or\nproduction (60 fps, best encode)",
                ),
                ("--width <PX>", "Output width in pixels (default 1920)"),
                ("--height <PX>", "Output height in pixels (default 1080)"),
                (
                    "--fit <MODE>",
                    "When the output aspect differs from the scene frame:\nerror (default), contain (letterbox), or cover (crop)",
                ),
                (
                    "--encoder <ENCODER>",
                    "MP4 only: auto (default), libx264, nvenc, amf, qsv, or\nvaapi. auto tries hardware encoders and falls back to\nlibx264; the others never fall back",
                ),
                (
                    "--transparent",
                    "Keep the alpha channel (WebM, WebP, PNG); the scene needs\na transparent background",
                ),
                (
                    "--from <SECONDS|MARKER>",
                    "Start of the range (default 0); a name uses the time of\nscene.marker(name)",
                ),
                (
                    "--to <SECONDS|MARKER>",
                    "End of the range (default: the end of the scene). Audio\nis trimmed to the range and PNG frames start at 0",
                ),
                ("-h, --help", "Print this help"),
            ],
        ),
        Block::Rows(
            "Examples",
            &[
                (
                    "gaanim export . -o exports/video.mp4",
                    "Export the project in the current folder",
                ),
                (
                    "gaanim export main.py -o preview.mp4 --quality draft",
                    "Quick, smaller render",
                ),
                (
                    "gaanim export . -o clip.webm --transparent",
                    "Keep transparency for compositing",
                ),
                (
                    "gaanim export . -o intro.gif --from 0 --to intro_end",
                    "Only up to scene.marker(\"intro_end\")",
                ),
            ],
        ),
    ],
    footer: &[DOCS],
};

const CHECK: Page = Page {
    summary: "Validate a scene or presentation without opening a window.",
    blocks: &[
        Block::Lines("Usage", &["gaanim check <SCRIPT_OR_PROJECT> [--strict]"]),
        Block::Lines(
            "Checks",
            &[
                "the entry script runs and the timeline has a duration",
                "segments, speaker notes and named stops, when present",
                "a 16:9 frame for presentations",
                "unthemed scenes whose default white objects match the background",
                "unresolved template placeholders",
            ],
        ),
        Block::Rows(
            "Options",
            &[
                (
                    "--strict",
                    "Also fail when there are warnings (useful in CI)",
                ),
                ("-h, --help", "Print this help"),
            ],
        ),
        Block::Rows(
            "Exit status",
            &[
                ("0", "No errors (and, with --strict, no warnings)"),
                ("1", "The report found problems"),
                ("2", "The script or project could not be loaded"),
            ],
        ),
    ],
    footer: &[DOCS],
};

const DIFF: Page = Page {
    summary: "Capture snapshots of a scene and compare them with approved baselines.",
    blocks: &[
        Block::Lines(
            "Usage",
            &[
                "gaanim --diff --example <SCRIPT_OR_PROJECT> [OPTIONS]",
                "gaanim --diff --baseline <DIR> --current <DIR> [OPTIONS]",
            ],
        ),
        Block::Rows(
            "Capture",
            &[
                (
                    "-e, --example <SCRIPT>",
                    "Capture and compare one script; it calls\nscene.snapshots(...) when GAANIM_SNAPSHOTS is set",
                ),
                (
                    "--tests-root <DIR>",
                    "Snapshot root (default: tests/visual)",
                ),
                (
                    "--bless",
                    "Capture as the new baseline and exit (approve first!)",
                ),
                ("--capture-only", "Capture into current/ and exit"),
                ("--no-capture", "Reuse the existing current/ snapshots"),
                (
                    "--capture-stops",
                    "Capture the frame at every scene.stop() instead",
                ),
                (
                    "--stops <LIST>",
                    "With --capture-stops, only these 1-based stops, e.g. 3-7,12",
                ),
                (
                    "--sections <LIST>",
                    "With --capture-stops, only stops in these segments",
                ),
                (
                    "--from <NAME>",
                    "With --capture-stops, stops from this segment on",
                ),
            ],
        ),
        Block::Rows(
            "Compare",
            &[
                ("-b, --baseline <DIR>", "Known-good snapshot directory"),
                ("-c, --current <DIR>", "Candidate snapshot directory"),
                ("-o, --output <DIR>", "Report directory"),
                (
                    "--pixel-threshold <0..255>",
                    "Ignored per-channel difference (default 2)",
                ),
                (
                    "--max-changed-ratio <0..1>",
                    "Allowed fraction of changed pixels (default 0)",
                ),
                ("-h, --help", "Print this help"),
            ],
        ),
    ],
    footer: &[DOCS],
};

fn page(topic: Topic) -> &'static Page {
    match topic {
        Topic::General => &GENERAL,
        Topic::Init => &INIT,
        Topic::Export => &EXPORT,
        Topic::Check => &CHECK,
        Topic::Diff => &DIFF,
    }
}

/// Colours what a person types and dims `<PLACEHOLDERS>` inside it.
fn command(text: &str, color: bool) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        let end = rest[start..]
            .find('>')
            .map_or(rest.len(), |end| start + end + 1);
        out.push_str(&console::style(&rest[..start], Style::Command, color));
        out.push_str(&console::style(&rest[start..end], Style::Muted, color));
        rest = &rest[end..];
    }
    out.push_str(&console::style(rest, Style::Command, color));
    out
}

/// Renders a help screen. The title line is omitted: callers print the
/// banner, or [`title`] when stderr is not a terminal.
pub fn render(topic: Topic, color: bool) -> String {
    let page = page(topic);
    let mut out = String::new();
    for line in page.summary.lines() {
        out.push_str(&console::style(line, Style::Strong, color));
        out.push('\n');
    }
    for block in page.blocks {
        out.push('\n');
        match block {
            Block::Lines(heading, lines) => {
                out.push_str(&console::style(
                    &heading.to_uppercase(),
                    Style::Heading,
                    color,
                ));
                out.push('\n');
                for line in *lines {
                    if *heading == "Usage" {
                        out.push_str(&format!("  {}\n", command(line, color)));
                    } else {
                        out.push_str(&format!("  • {line}\n"));
                    }
                }
            }
            Block::Rows(heading, rows) => {
                out.push_str(&console::style(
                    &heading.to_uppercase(),
                    Style::Heading,
                    color,
                ));
                out.push('\n');
                let width = rows
                    .iter()
                    .map(|(name, _)| name.chars().count())
                    .filter(|&width| width <= 36)
                    .max()
                    .unwrap_or(0);
                for (name, description) in *rows {
                    let name_width = name.chars().count();
                    let mut lines = description.lines();
                    let first = lines.next().unwrap_or("");
                    if name_width > width {
                        // Too long for the column: the description goes below.
                        out.push_str(&format!("  {}\n", command(name, color)));
                        out.push_str(&format!("  {:width$}  {first}\n", ""));
                    } else {
                        let padding = " ".repeat(width - name_width);
                        out.push_str(&format!("  {}{padding}  {first}\n", command(name, color)));
                    }
                    for line in lines {
                        out.push_str(&format!("  {:width$}  {line}\n", ""));
                    }
                }
            }
        }
    }
    if !page.footer.is_empty() {
        out.push('\n');
        for line in page.footer {
            out.push_str(&console::style(line, Style::Muted, color));
            out.push('\n');
        }
    }
    out
}

/// The one-line title printed instead of the banner outside a terminal.
pub fn title(topic: Topic) -> &'static str {
    match topic {
        Topic::General => "gaanim — GPU-accelerated vector animation engine",
        Topic::Init => "gaanim init — create a project",
        Topic::Export => "gaanim export — render a scene to a file",
        Topic::Check => "gaanim check — validate a scene or presentation",
        Topic::Diff => "gaanim --diff — visual regression snapshots",
    }
}

/// Prints a help screen to stdout, with the banner on a terminal.
pub fn print(topic: Topic) {
    let subtitle = match topic {
        Topic::General => "GPU-accelerated vector animation engine",
        Topic::Init => "init · create a project",
        Topic::Export => "export · render to a file",
        Topic::Check => "check · validate a scene",
        Topic::Diff => "--diff · visual regression",
    };
    if !console::banner(subtitle) {
        println!("{}\n", title(topic));
    }
    print!(
        "{}",
        render(topic, console::color_enabled(console::Stream::Stdout))
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOPICS: [Topic; 5] = [
        Topic::General,
        Topic::Init,
        Topic::Export,
        Topic::Check,
        Topic::Diff,
    ];

    #[test]
    fn plain_help_has_no_escapes_and_keeps_every_option() {
        for topic in TOPICS {
            let text = render(topic, false);
            assert!(!text.contains('\x1b'), "{topic:?}");
            assert!(text.contains("USAGE"), "{topic:?}");
        }
        let export = render(Topic::Export, false);
        for option in [
            "--output",
            "--quality",
            "--fit",
            "--encoder",
            "--transparent",
            "--from",
            "--to",
        ] {
            assert!(export.contains(option), "{option}");
        }
        let general = render(Topic::General, false);
        for word in [
            "init",
            "check",
            "export",
            "--diff",
            "--present",
            "RUST_LOG",
            "EXAMPLES",
        ] {
            assert!(general.contains(word), "{word}");
        }
    }

    #[test]
    fn descriptions_align_in_one_column() {
        let text = render(Topic::Check, false);
        let strict = text
            .lines()
            .find(|line| line.contains("--strict  "))
            .unwrap();
        let help = text
            .lines()
            .find(|line| line.contains("-h, --help"))
            .unwrap();
        assert_eq!(strict.find("Also"), help.find("Print"));
    }

    #[test]
    fn placeholders_are_dimmed_inside_commands() {
        let colored = command("init <video|slides>", true);
        assert!(colored.contains("init "));
        assert!(colored.contains("\x1b[2m<video|slides>"));
    }
}
