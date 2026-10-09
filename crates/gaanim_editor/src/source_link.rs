//! Opening a line of the scene script in the person's code editor, from the
//! inspector, the console and the script error panel.
//!
//! `GAANIM_EDITOR` names the command, with `{file}` and `{line}` where the
//! path and the line go (`code --goto {file}:{line}`); a command without them
//! gets the path appended. Without it, the first code editor found on `PATH`
//! opens the line, and otherwise the system opens the file with its default
//! application, which cannot jump to the line.

use std::io;
use std::path::{Path, PathBuf};

use gaanim_core::console::ScriptLocation;

/// The environment variable that names the code editor command.
pub const EDITOR_VARIABLE: &str = "GAANIM_EDITOR";

/// Code editors that open a file at a line, in the order they are looked for,
/// with how each is told the line.
const KNOWN_EDITORS: &[(&str, &[&str])] = &[
    ("code", &["--goto", "{file}:{line}"]),
    ("cursor", &["--goto", "{file}:{line}"]),
    ("codium", &["--goto", "{file}:{line}"]),
    ("zed", &["{file}:{line}"]),
    ("subl", &["{file}:{line}"]),
];

/// Whether this build can open files at all; the web player cannot.
pub const AVAILABLE: bool = !crate::WEB;

/// Open `location` in the person's code editor.
pub fn open(location: &ScriptLocation) -> io::Result<()> {
    let configured = std::env::var(EDITOR_VARIABLE).ok();
    if let Some(command) = configured
        .as_deref()
        .and_then(|template| editor_command(template, &location.file, location.line))
    {
        return spawn(command);
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    for (program, arguments) in KNOWN_EDITORS {
        if let Some(program) = find_program(program, &path) {
            let mut command = vec![program.to_string_lossy().into_owned()];
            command.extend(
                arguments
                    .iter()
                    .map(|argument| fill(argument, &location.file, location.line)),
            );
            return spawn(command);
        }
    }
    crate::platform::open_detached(&*location.file)
}

/// [`open`], reporting a failure in the console.
pub fn open_or_report(location: &ScriptLocation) {
    if let Err(error) = open(location) {
        gaanim_core::console::warn("editor", format!("could not open {location} · {error}"));
    }
}

/// How a person learns to choose the editor, for tooltips.
pub fn hint() -> String {
    format!("Elige el editor con {EDITOR_VARIABLE}, por ejemplo \"code --goto {{file}}:{{line}}\".")
}

/// The command `template` runs for `file` at `line`, split like a shell
/// would split it; `None` when it names no program.
fn editor_command(template: &str, file: &Path, line: u32) -> Option<Vec<String>> {
    let mut words = split_words(template);
    if words.is_empty() {
        return None;
    }
    let placeholders = words
        .iter()
        .any(|word| word.contains("{file}") || word.contains("{line}"));
    for word in &mut words {
        *word = fill(word, file, line);
    }
    if !placeholders {
        words.push(file.to_string_lossy().into_owned());
    }
    Some(words)
}

fn fill(word: &str, file: &Path, line: u32) -> String {
    word.replace("{file}", &file.to_string_lossy())
        .replace("{line}", &line.to_string())
}

/// Words of a command line: whitespace separates them, and single or double
/// quotes keep spaces inside one.
fn split_words(command: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut started = false;
    for ch in command.chars() {
        match (quote, ch) {
            (Some(open), ch) if ch == open => quote = None,
            (Some(_), ch) => word.push(ch),
            (None, '"' | '\'') => {
                quote = Some(ch);
                started = true;
            }
            (None, ch) if ch.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            (None, ch) => {
                word.push(ch);
                started = true;
            }
        }
    }
    if started {
        words.push(word);
    }
    words
}

/// The executable `name` in one of the folders of `path`.
fn find_program(name: &str, path: &std::ffi::OsStr) -> Option<PathBuf> {
    let names: Vec<String> = if cfg!(windows) {
        ["cmd", "exe"]
            .iter()
            .map(|extension| format!("{name}.{extension}"))
            .collect()
    } else {
        vec![name.to_owned()]
    };
    std::env::split_paths(path).find_map(|folder| {
        names
            .iter()
            .map(|name| folder.join(name))
            .find(|candidate| candidate.is_file())
    })
}

fn spawn(command: Vec<String>) -> io::Result<()> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let (program, arguments) = command
            .split_first()
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        let mut child = std::process::Command::new(program)
            .args(arguments)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        // Reap the launcher, which usually hands the file to a running editor
        // and exits, so it does not linger as a zombie process.
        std::thread::spawn(move || child.wait());
        Ok(())
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = command;
        Err(io::ErrorKind::Unsupported.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_template_receives_the_file_and_line() {
        let file = Path::new("/work/my scene.py");
        assert_eq!(
            editor_command("code --goto {file}:{line}", file, 42),
            Some(vec![
                "code".to_owned(),
                "--goto".to_owned(),
                "/work/my scene.py:42".to_owned()
            ])
        );
        assert_eq!(
            editor_command("\"C:\\Program Files\\Editor\\ed.exe\" -n", file, 3),
            Some(vec![
                "C:\\Program Files\\Editor\\ed.exe".to_owned(),
                "-n".to_owned(),
                "/work/my scene.py".to_owned()
            ])
        );
        assert_eq!(
            editor_command("idea --line {line} '{file}'", file, 7),
            Some(vec![
                "idea".to_owned(),
                "--line".to_owned(),
                "7".to_owned(),
                "/work/my scene.py".to_owned()
            ])
        );
        assert_eq!(editor_command("   ", file, 1), None);
    }

    #[test]
    fn programs_are_found_in_the_folders_of_path() {
        let folder =
            std::env::temp_dir().join(format!("gaanim-source-link-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let name = if cfg!(windows) { "zed.exe" } else { "zed" };
        std::fs::write(folder.join(name), "").unwrap();
        let path = std::env::join_paths([Path::new("/nonexistent-gaanim"), &folder]).unwrap();
        assert_eq!(find_program("zed", &path), Some(folder.join(name)));
        assert_eq!(find_program("subl", &path), None);
        let _ = std::fs::remove_dir_all(&folder);
    }
}
