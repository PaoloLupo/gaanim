//! Terminal output of the Gaanim command-line tools.
//!
//! One colour policy, the startup banner and the status lines people read
//! while a scene previews or exports. Machine-readable protocol lines
//! (`GAANIM_*`) and output that scripts parse must not go through here.
//!
//! Colour follows the usual conventions: `NO_COLOR` disables it,
//! `CLICOLOR_FORCE`/`FORCE_COLOR` force it, and otherwise it is used only when
//! the stream is a terminal (and `TERM` is not `dumb`).

use std::fmt::Display;
use std::io::{IsTerminal, Write};
use std::sync::OnceLock;

/// The standard stream a line is written to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

/// Severity of a status line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Debug,
    Info,
    Success,
    Warn,
    Error,
}

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RED: &str = "\x1b[1;31m";
const GREEN: &str = "\x1b[1;32m";
const YELLOW: &str = "\x1b[1;33m";
// Brand palette (tools/generate_brand.py, dark theme).
const VIOLET: (u8, u8, u8) = (0x7C, 0x6C, 0xFF);
const HAZE: (u8, u8, u8) = (0x3F, 0x37, 0xA8);
const GOLD: (u8, u8, u8) = (0xFF, 0xC9, 0x33);
const PAPER: (u8, u8, u8) = (0xF7, 0xF6, 0xFF);

/// Whether `stream` should receive ANSI colour escapes.
pub fn color_enabled(stream: Stream) -> bool {
    static STDOUT: OnceLock<bool> = OnceLock::new();
    static STDERR: OnceLock<bool> = OnceLock::new();
    let cell = match stream {
        Stream::Stdout => &STDOUT,
        Stream::Stderr => &STDERR,
    };
    *cell.get_or_init(|| {
        let is_terminal = match stream {
            Stream::Stdout => std::io::stdout().is_terminal(),
            Stream::Stderr => std::io::stderr().is_terminal(),
        };
        let wanted = color_policy(
            |name| std::env::var_os(name).map(|value| value.to_string_lossy().into_owned()),
            is_terminal,
        );
        wanted && enable_virtual_terminal(stream)
    })
}

/// Decides colour from the environment and whether the stream is a terminal.
fn color_policy(var: impl Fn(&str) -> Option<String>, is_terminal: bool) -> bool {
    let set = |name: &str| var(name).is_some_and(|value| !value.is_empty());
    if set("NO_COLOR") {
        return false;
    }
    let forced = |name: &str| var(name).is_some_and(|value| !value.is_empty() && value != "0");
    if forced("CLICOLOR_FORCE") || forced("FORCE_COLOR") {
        return true;
    }
    is_terminal && var("TERM").as_deref() != Some("dumb")
}

fn rgb(color: (u8, u8, u8)) -> String {
    format!("\x1b[38;2;{};{};{}m", color.0, color.1, color.2)
}

fn rgb_bg(color: (u8, u8, u8)) -> String {
    format!("\x1b[48;2;{};{};{}m", color.0, color.1, color.2)
}

fn paint(text: &str, style: &str, color: bool) -> String {
    if color && !text.is_empty() {
        format!("{style}{text}{RESET}")
    } else {
        text.to_string()
    }
}

/// Colours `text` like the mark of `level`, for labels inside reports.
pub fn paint_level(text: &str, level: Level, color: bool) -> String {
    let style = match level {
        Level::Debug => DIM.to_string(),
        Level::Info => format!("{BOLD}{}", rgb(VIOLET)),
        Level::Success => GREEN.to_string(),
        Level::Warn => YELLOW.to_string(),
        Level::Error => RED.to_string(),
    };
    paint(text, &style, color)
}

/// Text roles of the help screens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// Section headings (`USAGE`, `COMMANDS`).
    Heading,
    /// Commands, options and keys a person types.
    Command,
    /// Placeholders and secondary text.
    Muted,
    /// Emphasis inside prose.
    Strong,
}

/// Paints `text` in a help-screen role.
pub fn style(text: &str, role: Style, color: bool) -> String {
    let code = match role {
        Style::Heading => format!("{BOLD}{}", rgb(VIOLET)),
        Style::Command => rgb(GOLD),
        Style::Muted => DIM.to_string(),
        Style::Strong => BOLD.to_string(),
    };
    paint(text, &code, color)
}

/// Width of the label column in status lines.
const LABEL_WIDTH: usize = 10;
/// Column where messages start: two spaces, the mark, a space and the label.
pub const MESSAGE_COLUMN: usize = 4 + LABEL_WIDTH;

fn mark(level: Level) -> &'static str {
    match level {
        Level::Debug => "·",
        Level::Info => "▸",
        Level::Success => "✓",
        Level::Warn => "!",
        Level::Error => "✗",
    }
}

/// Formats one status line, `  ▸ watch     examples/ · save a file to
/// reload`: a mark and a label coloured by level, then the message, whose
/// details after the first ` · ` are dimmed.
pub fn format_line(level: Level, label: &str, message: &str, color: bool) -> String {
    let mark = paint_level(mark(level), level, color);
    let label = paint_level(
        &format!("{label:<width$}", width = LABEL_WIDTH - 1),
        level,
        color,
    );
    let message = match message.split_once(" · ") {
        Some((head, tail)) => format!("{head}{}", paint(&format!(" · {tail}"), DIM, color)),
        None => message.to_string(),
    };
    format!("  {mark} {label} {message}")
}

/// Whether `line` is a status line written by [`format_line`] without colour.
pub fn is_status_line(line: &str) -> bool {
    let mut chars = line.chars();
    line.starts_with("  ")
        && chars.nth(2).is_some_and(|mark| "▸·✓!✗".contains(mark))
        && chars.next() == Some(' ')
}

/// The label of a status line written by [`format_line`] without colour.
pub fn status_label(line: &str) -> Option<&str> {
    if !is_status_line(line) {
        return None;
    }
    // Skip the two-space indent and the mark (which may be several bytes).
    let (_mark, rest) = line[2..].split_once(' ')?;
    rest.split_whitespace().next()
}

/// Prints a status line on stderr.
pub fn status(level: Level, label: &str, message: impl Display) {
    let line = format_line(
        level,
        label,
        &message.to_string(),
        color_enabled(Stream::Stderr),
    );
    let _ = writeln!(std::io::stderr().lock(), "{line}");
}

/// Progress a person follows: a file being watched, a frame being rendered.
pub fn info(label: &str, message: impl Display) {
    status(Level::Info, label, message);
}

/// A finished step: a scene loaded, an export written.
pub fn success(label: &str, message: impl Display) {
    status(Level::Success, label, message);
}

/// Something that works but deserves attention.
pub fn warn(label: &str, message: impl Display) {
    status(Level::Warn, label, message);
}

/// A failure the person has to act on.
pub fn error(label: &str, message: impl Display) {
    status(Level::Error, label, message);
}

/// Colours a Python traceback: frame locations dimmed, the source lines as
/// they are, and the final `Error: message` line in red.
pub fn format_traceback(traceback: &str, color: bool) -> String {
    let lines: Vec<&str> = traceback.lines().collect();
    let last = lines
        .iter()
        .rposition(|line| !line.trim().is_empty() && !line.starts_with(' '));
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            if Some(index) == last {
                paint(line, RED, color)
            } else if line.starts_with("  File ") || line.starts_with("Traceback ") {
                paint(line, DIM, color)
            } else {
                line.to_string()
            }
        })
        .map(|line| line + "\n")
        .collect()
}

/// A path as a person would type it: relative to the current directory when it
/// lies inside it, otherwise unchanged.
pub fn display_path(path: &std::path::Path) -> String {
    let relative = std::env::current_dir().ok().and_then(|cwd| {
        path.strip_prefix(cwd)
            .ok()
            .map(std::path::Path::to_path_buf)
    });
    match relative {
        Some(relative) if relative.as_os_str().is_empty() => ".".to_string(),
        Some(relative) => relative.display().to_string(),
        None => path.display().to_string(),
    }
}

/// Formats a dimmed line aligned under the messages of status lines.
pub fn format_hint(message: &str, color: bool) -> String {
    format!("{:MESSAGE_COLUMN$}{}", "", paint(message, DIM, color))
}

/// Prints a dimmed follow-up line under a status line, such as how to get help.
pub fn hint(message: impl Display) {
    let line = format_hint(&message.to_string(), color_enabled(Stream::Stderr));
    let _ = writeln!(std::io::stderr().lock(), "{line}");
}

/// Formats a `Key:  value` line aligned under the messages of status lines.
pub fn format_detail(key: &str, value: &str, color: bool) -> String {
    let key = paint(&format!("{:<12}", format!("{key}:")), DIM, color);
    format!("{:MESSAGE_COLUMN$}{key}{value}", "")
}

/// Prints a `Key:  value` line under a status line.
pub fn detail(key: &str, value: impl Display) {
    let line = format_detail(key, &value.to_string(), color_enabled(Stream::Stderr));
    let _ = writeln!(std::io::stderr().lock(), "{line}");
}

// Row widths of the three 8x8 logo frames, back to front; mirrors FRAMES in
// tools/generate_brand.py (square, squircle, circle, two units apart).
const LOGO_FRAMES: [[usize; 8]; 3] = [
    [8, 8, 8, 8, 8, 8, 8, 8],
    [6, 8, 8, 8, 8, 8, 8, 6],
    [4, 6, 8, 8, 8, 8, 6, 4],
];
const LOGO_STEP: usize = 2;
const LOGO_WIDTH: usize = 8 + LOGO_STEP * 2;
const LOGO_COLORS: [(u8, u8, u8); 3] = [HAZE, VIOLET, GOLD];

/// The frontmost logo frame covering each pixel of the 12x8 symbol.
fn logo_pixels() -> [[Option<usize>; LOGO_WIDTH]; 8] {
    let mut pixels = [[None; LOGO_WIDTH]; 8];
    for (frame, widths) in LOGO_FRAMES.iter().enumerate() {
        for (y, width) in widths.iter().enumerate() {
            let start = frame * LOGO_STEP + (8 - width) / 2;
            for pixel in &mut pixels[y][start..start + width] {
                *pixel = Some(frame);
            }
        }
    }
    pixels
}

/// The symbol drawn with half blocks: two pixel rows per text line.
fn logo_lines(color: bool) -> Vec<String> {
    let pixels = logo_pixels();
    pixels
        .chunks(2)
        .map(|rows| {
            let mut line = String::new();
            for (&top, &bottom) in rows[0].iter().zip(&rows[1]) {
                if !color {
                    // One shade per frame: the oldest frame is the faintest.
                    line.push(match top.max(bottom) {
                        None => ' ',
                        Some(0) => '░',
                        Some(1) => '▒',
                        Some(_) => '█',
                    });
                    continue;
                }
                let cell = match (top, bottom) {
                    (None, None) => " ".to_string(),
                    (Some(t), Some(b)) if t == b => format!("{}█", rgb(LOGO_COLORS[t])),
                    (Some(t), None) => format!("{}▀", rgb(LOGO_COLORS[t])),
                    (None, Some(b)) => format!("{}▄", rgb(LOGO_COLORS[b])),
                    (Some(t), Some(b)) => {
                        format!("{}{}▀", rgb(LOGO_COLORS[t]), rgb_bg(LOGO_COLORS[b]))
                    }
                };
                line.push_str(&cell);
                line.push_str(if color { RESET } else { "" });
            }
            line
        })
        .collect()
}

/// The startup banner: the logo symbol beside the name, version and `subtitle`.
pub fn banner_lines(subtitle: &str, color: bool) -> Vec<String> {
    let name = format!(
        "{} {}",
        paint("gaanim", &format!("{BOLD}{}", rgb(PAPER)), color),
        paint(concat!("v", env!("CARGO_PKG_VERSION")), DIM, color)
    );
    let subtitle = paint(subtitle, &rgb(VIOLET), color);
    let text = ["", name.as_str(), subtitle.as_str(), ""];
    logo_lines(color)
        .into_iter()
        .zip(text)
        .map(|(logo, text)| format!("  {logo}   {text}").trim_end().to_string())
        .collect()
}

/// Prints the banner to stderr, only for a person at a terminal: logs, CI and
/// scripts that capture the output never see it. Returns whether it printed.
pub fn banner(subtitle: &str) -> bool {
    if !std::io::stderr().is_terminal() {
        return false;
    }
    let color = color_enabled(Stream::Stderr);
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr);
    for line in banner_lines(subtitle, color) {
        let _ = writeln!(stderr, "{line}");
    }
    let _ = writeln!(stderr);
    true
}

/// Classic Windows consoles need virtual terminal processing switched on
/// before they interpret ANSI escapes; other platforms always do.
#[cfg(windows)]
fn enable_virtual_terminal(stream: Stream) -> bool {
    use std::ffi::c_void;
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    const STD_ERROR_HANDLE: u32 = -12i32 as u32;
    const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetStdHandle(handle: u32) -> *mut c_void;
        fn GetConsoleMode(console: *mut c_void, mode: *mut u32) -> i32;
        fn SetConsoleMode(console: *mut c_void, mode: u32) -> i32;
    }
    let id = match stream {
        Stream::Stdout => STD_OUTPUT_HANDLE,
        Stream::Stderr => STD_ERROR_HANDLE,
    };
    // SAFETY: plain Win32 console calls on this process's standard handle;
    // `mode` outlives the call that writes it.
    unsafe {
        let handle = GetStdHandle(id);
        let mut mode = 0;
        if handle.is_null() || GetConsoleMode(handle, &mut mode) == 0 {
            // Not a console (a pipe or a terminal emulator's pty): escapes pass
            // through to whatever reads them.
            return true;
        }
        mode & ENABLE_VIRTUAL_TERMINAL_PROCESSING != 0
            || SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0
    }
}

#[cfg(not(windows))]
fn enable_virtual_terminal(_stream: Stream) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            vars.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.to_string())
        }
    }

    #[test]
    fn color_follows_no_color_force_and_terminal() {
        assert!(color_policy(env(&[]), true));
        assert!(!color_policy(env(&[]), false));
        assert!(!color_policy(env(&[("NO_COLOR", "1")]), true));
        assert!(!color_policy(
            env(&[("NO_COLOR", "1"), ("CLICOLOR_FORCE", "1")]),
            true
        ));
        assert!(color_policy(env(&[("CLICOLOR_FORCE", "1")]), false));
        assert!(color_policy(env(&[("FORCE_COLOR", "true")]), false));
        assert!(!color_policy(env(&[("FORCE_COLOR", "0")]), false));
        assert!(!color_policy(env(&[("TERM", "dumb")]), true));
        // An empty NO_COLOR does not disable colour.
        assert!(color_policy(env(&[("NO_COLOR", "")]), true));
    }

    #[test]
    fn plain_lines_have_no_escapes() {
        assert_eq!(
            format_line(Level::Info, "watch", "examples", false),
            "  ▸ watch     examples"
        );
        assert_eq!(
            format_line(Level::Error, "export", "--output is required", false),
            "  ✗ export    --output is required"
        );
        assert_eq!(
            format_detail("Encoder", "CPU (libx264)", false),
            "              Encoder:    CPU (libx264)"
        );
        assert_eq!(
            format_detail("Encoder", "x", false).find('E'),
            Some(MESSAGE_COLUMN)
        );
        let colored = format_line(Level::Success, "ready", "Scene ready · 0.02s", true);
        assert!(colored.contains('\x1b') && colored.contains("Scene ready"));
        // Details after the first separator are dimmed, the headline is not.
        assert!(colored.contains(&format!("Scene ready{DIM} · 0.02s")));
    }

    #[test]
    fn tracebacks_keep_their_text_and_highlight_the_error() {
        let traceback = "Traceback (most recent call last):\n  File \"main.py\", line 4, in <module>\n    undefined_name\nNameError: boom\n";
        assert_eq!(format_traceback(traceback, false), traceback);
        let colored = format_traceback(traceback, true);
        assert!(colored.contains(&format!("{RED}NameError: boom{RESET}")));
        assert!(colored.contains("\n    undefined_name\n"));
    }

    #[test]
    fn paths_inside_the_current_directory_are_relative() {
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(display_path(&cwd), ".");
        assert_eq!(
            display_path(&cwd.join("examples").join("a.py")),
            format!("examples{}a.py", std::path::MAIN_SEPARATOR)
        );
        let outside = std::path::Path::new("/definitely/elsewhere/a.py");
        assert_eq!(display_path(outside), outside.display().to_string());
    }

    #[test]
    fn status_lines_are_recognised() {
        for level in [
            Level::Debug,
            Level::Info,
            Level::Success,
            Level::Warn,
            Level::Error,
        ] {
            assert!(is_status_line(&format_line(level, "ready", "x", false)));
        }
        assert!(!is_status_line("Traceback (most recent call last):"));
        assert!(!is_status_line("  File \"main.py\", line 4"));
        assert!(!is_status_line("hello"));
        assert_eq!(status_label("  ▸ check     main.py"), Some("check"));
        assert_eq!(status_label("  ✗ error     boom"), Some("error"));
        assert_eq!(status_label("plain"), None);
    }

    #[test]
    fn logo_matches_the_brand_symbol() {
        let pixels = logo_pixels();
        // The circle (front frame) is 4 wide on its first row, starting at x=6.
        assert_eq!(pixels[0][6..10], [Some(2); 4]);
        // The square's left column stays visible behind the other frames.
        assert!(pixels.iter().all(|row| row[0] == Some(0)));
        let plain = logo_lines(false);
        assert_eq!(plain.len(), 4);
        assert!(plain.iter().all(|line| line.chars().count() == LOGO_WIDTH));
        assert!(plain.iter().all(|line| !line.contains('\x1b')));
        assert_eq!(plain[1], "░░▒▒████████");
    }

    #[test]
    fn banner_puts_the_name_beside_the_symbol() {
        let lines = banner_lines("vector animation on the GPU", false);
        assert_eq!(lines.len(), 4);
        assert!(lines[1].ends_with(concat!("gaanim v", env!("CARGO_PKG_VERSION"))));
        assert!(lines[2].ends_with("vector animation on the GPU"));
    }
}
