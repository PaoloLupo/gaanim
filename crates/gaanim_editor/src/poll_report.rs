//! The results a presentation keeps for its author, such as a teacher who
//! grades the class: the relay's report of a game written as spreadsheets.
//!
//! Each game goes to its own folder, named by when it started, under the
//! results folder (`resultados/` in the project's output folder):
//!
//! - `jugadores.csv`: one row per player, best first, with its totals and
//!   what it answered on each question;
//! - `respuestas.csv`: one row per answer, for filters and pivot tables;
//! - `preguntas.csv`: one row per question, with its counts and how many
//!   got it right.
//!
//! Files are UTF-8 with a byte order mark and CRLF line ends, comma
//! separated, so spreadsheets open them with accents intact.

use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The relay's `report`: the whole game, nothing left out.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Report {
    /// Which game: the native keeper announces each one once.
    #[serde(default)]
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub game: Option<String>,
    /// When the game started, milliseconds since the epoch.
    #[serde(default)]
    pub started: Option<f64>,
    #[serde(default)]
    pub ask: Option<AskReport>,
    #[serde(default)]
    pub teams: Option<TeamsReport>,
    #[serde(default)]
    pub polls: Vec<PollReport>,
    #[serde(default)]
    pub players: Vec<PlayerReport>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AskReport {
    pub label: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TeamsReport {
    pub names: Vec<String>,
}

/// A question, in the order it first opened.
#[derive(Debug, Clone, Deserialize)]
pub struct PollReport {
    pub id: String,
    pub question: String,
    pub options: Vec<String>,
    /// The right answers of a quiz; `None` for a poll.
    #[serde(default)]
    pub correct: Option<Vec<usize>>,
    #[serde(default)]
    pub counts: Vec<u32>,
    /// Phones that answered, named or not.
    #[serde(default)]
    pub respondents: u32,
}

impl PollReport {
    fn is_quiz(&self) -> bool {
        self.correct.is_some()
    }

    /// The answers at `indexes` as text, `" | "` between them.
    fn texts(&self, indexes: &[usize]) -> String {
        indexes
            .iter()
            .filter_map(|index| self.options.get(*index).map(String::as_str))
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlayerReport {
    pub rank: u32,
    pub name: String,
    /// What joining also asked (`scene.roster`).
    #[serde(default)]
    pub extra: Option<String>,
    #[serde(default)]
    pub team: Option<usize>,
    /// When it joined, milliseconds since the epoch.
    #[serde(default)]
    pub joined: f64,
    #[serde(default)]
    pub score: u64,
    #[serde(default)]
    pub correct: u32,
    #[serde(default)]
    pub answered: u32,
    #[serde(default)]
    pub streak: u32,
    /// Its answer on each question, by question id.
    #[serde(default)]
    pub answers: HashMap<String, AnswerReport>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnswerReport {
    #[serde(default)]
    pub options: Vec<usize>,
    /// Milliseconds a quiz answer took.
    #[serde(default)]
    pub elapsed: Option<f64>,
    #[serde(default)]
    pub points: Option<u32>,
    #[serde(default)]
    pub right: Option<bool>,
}

impl Report {
    /// Nothing to keep: nobody joined and nobody voted.
    pub fn is_empty(&self) -> bool {
        self.players.is_empty() && self.polls.iter().all(|poll| poll.respondents == 0)
    }

    /// The game's folder: when it started, local time, as
    /// `2026-10-01_15-30`, or `partida` when the relay does not say.
    pub fn folder_name(&self) -> String {
        use chrono::TimeZone;
        self.started
            .and_then(|ms| chrono::Local.timestamp_millis_opt(ms as i64).single())
            .map(|time| time.format("%Y-%m-%d_%H-%M").to_string())
            .unwrap_or_else(|| "partida".to_string())
    }

    fn team_name(&self, team: Option<usize>) -> String {
        match (&self.teams, team) {
            (Some(teams), Some(team)) => teams.names.get(team).cloned().unwrap_or_default(),
            _ => String::new(),
        }
    }

    /// Columns every per-player file starts with: the nickname, what
    /// joining asked, and the team.
    fn who_headers(&self) -> Vec<String> {
        let mut headers = vec!["Apodo".to_string()];
        if let Some(ask) = &self.ask {
            headers.push(ask.label.clone());
        }
        if self.teams.is_some() {
            headers.push("Equipo".into());
        }
        headers
    }

    fn who(&self, player: &PlayerReport) -> Vec<String> {
        let mut row = vec![text(&player.name)];
        if self.ask.is_some() {
            row.push(text(player.extra.as_deref().unwrap_or("")));
        }
        if self.teams.is_some() {
            row.push(text(&self.team_name(player.team)));
        }
        row
    }

    /// `jugadores.csv`: a row per player, best first.
    pub fn players_csv(&self) -> String {
        let mut headers = vec!["Puesto".to_string()];
        headers.extend(self.who_headers());
        headers.extend(
            ["Puntos", "Aciertos", "Respondidas", "Racha", "Entró"]
                .into_iter()
                .map(String::from),
        );
        for (number, poll) in self.polls.iter().enumerate() {
            headers.push(format!("{}. {}", number + 1, poll.question));
        }
        let mut rows = vec![headers.iter().map(|header| text(header)).collect()];
        for player in &self.players {
            let mut row = vec![player.rank.to_string()];
            row.extend(self.who(player));
            row.extend([
                player.score.to_string(),
                player.correct.to_string(),
                player.answered.to_string(),
                player.streak.to_string(),
                clock(player.joined),
            ]);
            for poll in &self.polls {
                let cell = match player.answers.get(&poll.id) {
                    Some(answer) => {
                        let chosen = poll.texts(&answer.options);
                        match answer.right {
                            Some(true) => format!("{chosen} ✓"),
                            Some(false) => format!("{chosen} ✗"),
                            None => chosen,
                        }
                    }
                    None => String::new(),
                };
                row.push(text(&cell));
            }
            rows.push(row);
        }
        csv(&rows)
    }

    /// `respuestas.csv`: a row per answer.
    pub fn answers_csv(&self) -> String {
        let mut headers = self.who_headers();
        headers.extend(
            [
                "N.º",
                "Pregunta",
                "Tipo",
                "Respuesta",
                "Correcta",
                "Puntos",
                "Segundos",
            ]
            .into_iter()
            .map(String::from),
        );
        let mut rows = vec![headers.iter().map(|header| text(header)).collect()];
        for (number, poll) in self.polls.iter().enumerate() {
            for player in &self.players {
                let Some(answer) = player.answers.get(&poll.id) else {
                    continue;
                };
                let mut row = self.who(player);
                row.extend([
                    (number + 1).to_string(),
                    text(&poll.question),
                    kind(poll).into(),
                    text(&poll.texts(&answer.options)),
                    match answer.right {
                        Some(true) => "sí".into(),
                        Some(false) => "no".into(),
                        None => String::new(),
                    },
                    answer
                        .points
                        .map(|points| points.to_string())
                        .unwrap_or_default(),
                    answer.elapsed.map(seconds).unwrap_or_default(),
                ]);
                rows.push(row);
            }
        }
        csv(&rows)
    }

    /// `preguntas.csv`: a row per question.
    pub fn questions_csv(&self) -> String {
        let most = self
            .polls
            .iter()
            .map(|poll| poll.options.len())
            .max()
            .unwrap_or(0);
        let mut headers: Vec<String> = [
            "N.º",
            "Pregunta",
            "Tipo",
            "Correcta",
            "Respondieron",
            "Acertaron",
            "% de acierto",
            "Segundos en promedio",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        for index in 1..=most {
            headers.push(format!("Respuesta {index}"));
            headers.push(format!("Votos {index}"));
        }
        let mut rows = vec![headers.iter().map(|header| text(header)).collect()];
        for (number, poll) in self.polls.iter().enumerate() {
            let answers: Vec<&AnswerReport> = self
                .players
                .iter()
                .filter_map(|player| player.answers.get(&poll.id))
                .collect();
            let mut row = vec![
                (number + 1).to_string(),
                text(&poll.question),
                kind(poll).into(),
                text(
                    &poll
                        .correct
                        .as_deref()
                        .map(|right| poll.texts(right))
                        .unwrap_or_default(),
                ),
                poll.respondents.to_string(),
            ];
            if poll.is_quiz() {
                let right = answers
                    .iter()
                    .filter(|answer| answer.right == Some(true))
                    .count();
                let times: Vec<f64> = answers.iter().filter_map(|answer| answer.elapsed).collect();
                row.push(right.to_string());
                row.push(if answers.is_empty() {
                    String::new()
                } else {
                    (100 * right / answers.len()).to_string()
                });
                row.push(if times.is_empty() {
                    String::new()
                } else {
                    seconds(times.iter().sum::<f64>() / times.len() as f64)
                });
            } else {
                row.extend([String::new(), String::new(), String::new()]);
            }
            for index in 0..most {
                row.push(text(
                    poll.options.get(index).map(String::as_str).unwrap_or(""),
                ));
                row.push(
                    poll.options
                        .get(index)
                        .map(|_| poll.counts.get(index).copied().unwrap_or(0).to_string())
                        .unwrap_or_default(),
                );
            }
            rows.push(row);
        }
        csv(&rows)
    }

    /// The game's three files in a ZIP archive named after its folder, for
    /// the web player to download: the archive's file name and bytes.
    #[cfg(any(test, target_arch = "wasm32"))]
    pub fn archive(&self) -> Result<(String, Vec<u8>), String> {
        use std::io::Write;
        let folder = self.folder_name();
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        for (name, contents) in [
            ("jugadores.csv", self.players_csv()),
            ("respuestas.csv", self.answers_csv()),
            ("preguntas.csv", self.questions_csv()),
        ] {
            zip.start_file(format!("{folder}/{name}"), options)
                .and_then(|()| zip.write_all(contents.as_bytes()).map_err(Into::into))
                .map_err(|error| format!("could not pack {name}: {error}"))?;
        }
        let bytes = zip
            .finish()
            .map_err(|error| format!("could not pack the results: {error}"))?
            .into_inner();
        Ok((format!("resultados-{folder}.zip"), bytes))
    }

    /// Write the game's three files into `results/<folder_name>/`, each
    /// replaced whole, and return that folder.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn write(&self, results: &Path) -> Result<PathBuf, String> {
        let folder = results.join(self.folder_name());
        std::fs::create_dir_all(&folder)
            .map_err(|error| format!("could not create {}: {error}", folder.display()))?;
        for (name, contents) in [
            ("jugadores.csv", self.players_csv()),
            ("respuestas.csv", self.answers_csv()),
            ("preguntas.csv", self.questions_csv()),
        ] {
            let path = folder.join(name);
            let partial = folder.join(format!(".{name}.tmp"));
            std::fs::write(&partial, contents)
                .map_err(|error| format!("could not write {}: {error}", partial.display()))?;
            std::fs::rename(&partial, &path).map_err(|error| {
                let _ = std::fs::remove_file(&partial);
                format!(
                    "could not update {} (is it open in a spreadsheet?): {error}",
                    path.display()
                )
            })?;
        }
        Ok(folder)
    }
}

fn kind(poll: &PollReport) -> &'static str {
    if poll.is_quiz() {
        "cuestionario"
    } else {
        "encuesta"
    }
}

/// A local time of day, `15:04:05`, from milliseconds since the epoch.
fn clock(ms: f64) -> String {
    use chrono::TimeZone;
    if ms <= 0.0 {
        return String::new();
    }
    chrono::Local
        .timestamp_millis_opt(ms as i64)
        .single()
        .map(|time| time.format("%H:%M:%S").to_string())
        .unwrap_or_default()
}

/// Milliseconds as seconds with one decimal.
fn seconds(ms: f64) -> String {
    format!("{:.1}", ms / 1000.0)
}

/// Text that came from the audience or the author, made safe for a
/// spreadsheet: one that starts like a formula is kept as text.
fn text(value: &str) -> String {
    if value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{value}")
    } else {
        value.to_string()
    }
}

/// Rows as CSV: a byte order mark, CRLF line ends, and quotes around a
/// field with a comma, quote, line break or edge space.
fn csv(rows: &[Vec<String>]) -> String {
    let mut out = String::from('\u{feff}');
    for row in rows {
        let fields: Vec<String> = row
            .iter()
            .map(|field| {
                let quote = field.contains([',', '"', '\n', '\r'])
                    || field.starts_with(' ')
                    || field.ends_with(' ');
                if quote {
                    format!("\"{}\"", field.replace('"', "\"\""))
                } else {
                    field.clone()
                }
            })
            .collect();
        out.push_str(&fields.join(","));
        out.push_str("\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> Report {
        serde_json::from_value(serde_json::json!({
            "game": "g1",
            "started": 0,
            "ask": {"label": "Código", "required": true},
            "teams": {"names": ["Rojo", "Azul"], "colors": ["#ff0000", "#0000ff"]},
            "polls": [
                {"id": "p", "question": "¿Té o café?", "options": ["Té", "Café"],
                 "correct": null, "counts": [1, 2], "respondents": 3},
                {"id": "q", "question": "¿2 + 2, o 5?", "options": ["3", "4", "5"],
                 "correct": [1], "counts": [0, 1, 1], "respondents": 2}
            ],
            "players": [
                {"rank": 1, "name": "Ana", "extra": "2026-0042", "team": 1, "score": 900,
                 "correct": 1, "answered": 1, "streak": 1,
                 "answers": {"p": {"options": [1]},
                             "q": {"options": [1], "elapsed": 2600, "points": 900, "right": true}}},
                {"rank": 2, "name": "Beto", "extra": "=HYPERLINK(\"x\")", "team": 0, "score": 0,
                 "correct": 0, "answered": 1, "streak": 0,
                 "answers": {"q": {"options": [2], "elapsed": 4000, "points": 0, "right": false}}}
            ]
        }))
        .unwrap()
    }

    #[test]
    fn the_archive_holds_the_three_spreadsheets_in_the_game_folder() {
        use std::io::Read;
        let report = report();
        let (name, bytes) = report.archive().unwrap();
        let folder = report.folder_name();
        assert_eq!(name, format!("resultados-{folder}.zip"));
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        for (file, expected) in [
            ("jugadores.csv", report.players_csv()),
            ("respuestas.csv", report.answers_csv()),
            ("preguntas.csv", report.questions_csv()),
        ] {
            let mut contents = String::new();
            archive
                .by_name(&format!("{folder}/{file}"))
                .unwrap()
                .read_to_string(&mut contents)
                .unwrap();
            assert_eq!(contents, expected, "{file}");
        }
    }

    fn lines(csv: &str) -> Vec<&str> {
        csv.trim_start_matches('\u{feff}')
            .split("\r\n")
            .filter(|line| !line.is_empty())
            .collect()
    }

    #[test]
    fn players_list_their_totals_and_each_answer() {
        let csv = report().players_csv();
        assert!(csv.starts_with('\u{feff}'));
        assert_eq!(
            lines(&csv),
            [
                "Puesto,Apodo,Código,Equipo,Puntos,Aciertos,Respondidas,Racha,Entró,1. ¿Té o café?,\"2. ¿2 + 2, o 5?\"",
                "1,Ana,2026-0042,Azul,900,1,1,1,,Café,4 ✓",
                "2,Beto,\"'=HYPERLINK(\"\"x\"\")\",Rojo,0,0,1,0,,,5 ✗",
            ]
        );
    }

    #[test]
    fn answers_have_a_row_each() {
        assert_eq!(
            lines(&report().answers_csv()),
            [
                "Apodo,Código,Equipo,N.º,Pregunta,Tipo,Respuesta,Correcta,Puntos,Segundos",
                "Ana,2026-0042,Azul,1,¿Té o café?,encuesta,Café,,,",
                "Ana,2026-0042,Azul,2,\"¿2 + 2, o 5?\",cuestionario,4,sí,900,2.6",
                "Beto,\"'=HYPERLINK(\"\"x\"\")\",Rojo,2,\"¿2 + 2, o 5?\",cuestionario,5,no,0,4.0",
            ]
        );
    }

    #[test]
    fn questions_count_votes_and_right_answers() {
        assert_eq!(
            lines(&report().questions_csv()),
            [
                "N.º,Pregunta,Tipo,Correcta,Respondieron,Acertaron,% de acierto,Segundos en promedio,Respuesta 1,Votos 1,Respuesta 2,Votos 2,Respuesta 3,Votos 3",
                "1,¿Té o café?,encuesta,,3,,,,Té,1,Café,2,,",
                "2,\"¿2 + 2, o 5?\",cuestionario,4,2,1,50,3.3,3,0,4,1,5,1",
            ]
        );
    }

    #[test]
    fn without_a_roster_or_teams_the_columns_are_left_out() {
        let mut report = report();
        report.ask = None;
        report.teams = None;
        assert!(lines(&report.players_csv())[0].starts_with("Puesto,Apodo,Puntos,"));
        assert!(lines(&report.answers_csv())[0].starts_with("Apodo,N.º,"));
    }

    #[test]
    fn a_game_is_written_to_its_own_folder_and_rewritten_whole() {
        let root = std::env::temp_dir().join(format!("gaanim-report-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let report = report();
        let folder = report.write(&root).unwrap();
        assert_eq!(folder, root.join(report.folder_name()));
        let players = std::fs::read_to_string(folder.join("jugadores.csv")).unwrap();
        assert_eq!(players, report.players_csv());
        report.write(&root).unwrap();
        let mut names: Vec<_> = std::fs::read_dir(&folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, ["jugadores.csv", "preguntas.csv", "respuestas.csv"]);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn an_empty_game_is_not_worth_keeping() {
        assert!(Report::default().is_empty());
        assert!(!report().is_empty());
    }
}
