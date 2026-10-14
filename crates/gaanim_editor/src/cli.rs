//! `gaanim export` arguments.
//!
//! Scripts and projects export through the Python plugin; playback bundles
//! (`.gaanim`) export through [`export_bundle_video`] and
//! [`export_bundle_pdf`] without Python.

use std::path::{Path, PathBuf};

use gaanim_core::console;
use gaanim_export::encoder::{ExportFormat, VideoEncoder};
use gaanim_export::prelude::{
    AspectRatioPreset, ExportConfig, FrameRate, OutputFit, QualityPreset,
};

/// One end of an export range, or a `--frame` instant: seconds, or a name
/// resolved once the scene's markers, segments and stops are known.
#[derive(Debug, Clone, PartialEq)]
pub enum ExportBound {
    Seconds(f64),
    Name(String),
}

impl std::fmt::Display for ExportBound {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Seconds(seconds) => write!(formatter, "{seconds}"),
            Self::Name(name) => write!(formatter, "{name}"),
        }
    }
}

/// `--from` / `--to` / a `--frame` instant: finite non-negative seconds, or
/// a name.
pub fn parse_export_seconds(flag: &str, value: Option<&String>) -> Result<ExportBound, String> {
    let value = value
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{flag} requires seconds or a name"))?;
    match value.parse::<f64>() {
        Ok(seconds) if seconds.is_finite() && seconds >= 0.0 => Ok(ExportBound::Seconds(seconds)),
        Ok(_) => Err(format!(
            "{flag} requires a non-negative number of seconds or a name"
        )),
        Err(_) if value.starts_with('-') => Err(format!(
            "{flag} requires a non-negative number of seconds or a name"
        )),
        Err(_) => Ok(ExportBound::Name(value.to_string())),
    }
}

pub fn validate_export_range(
    from: Option<&ExportBound>,
    to: Option<&ExportBound>,
) -> Result<(), String> {
    match (from, to) {
        (Some(ExportBound::Seconds(from)), Some(ExportBound::Seconds(to))) if to <= from => {
            Err(format!("--to ({to}) must be greater than --from ({from})"))
        }
        _ => Ok(()),
    }
}

/// Which instant a segment name stands for: `--from` takes its start,
/// `--to` its end, and `--frame` the first instant it rests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundRole {
    From,
    To,
    Frame,
}

impl BoundRole {
    pub fn flag(self) -> &'static str {
        match self {
            Self::From => "--from",
            Self::To => "--to",
            Self::Frame => "--frame",
        }
    }
}

/// A segment as export names see it: its time range and its stops.
#[derive(Debug, Clone, PartialEq)]
pub struct NamedSegment {
    pub name: String,
    pub start: f64,
    pub end: f64,
    /// `(name, time)` of each stop, in timeline order.
    pub stops: Vec<(Option<String>, f64)>,
}

/// The names an export instant can take, tried in this order: a
/// `scene.marker` name, a segment name or `Section` key, and a stop name.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TimelineNames {
    pub markers: Vec<(String, f64)>,
    pub segments: Vec<NamedSegment>,
}

impl TimelineNames {
    /// The names a compiled scene defines.
    pub fn of_canvas(canvas: &gaanim_api::canvas::SceneModel) -> Self {
        Self {
            markers: canvas
                .markers()
                .into_iter()
                .map(|marker| (marker.name, marker.time))
                .collect(),
            segments: canvas
                .segment_manifest()
                .segments
                .into_iter()
                .map(|segment| NamedSegment {
                    name: segment.name,
                    start: segment.start_time,
                    end: segment.end_time,
                    stops: segment
                        .stops
                        .into_iter()
                        .map(|stop| (stop.name, stop.time))
                        .collect(),
                })
                .collect(),
        }
    }

    /// The names a recorded timeline (a playback bundle's) defines.
    pub fn of_timeline(
        markers: &[gaanim_timeline::timeline::TimelineMarker],
        segments: &[gaanim_timeline::timeline::SegmentMetadata],
    ) -> Self {
        Self {
            markers: markers
                .iter()
                .map(|marker| (marker.name.clone(), marker.time))
                .collect(),
            segments: segments
                .iter()
                .map(|segment| NamedSegment {
                    name: segment.name.clone(),
                    start: segment.start_time,
                    end: segment.end_time,
                    stops: segment
                        .stops
                        .iter()
                        .map(|stop| (stop.name.clone(), stop.time))
                        .collect(),
                })
                .collect(),
        }
    }

    /// The time `name` stands for as `role`.
    ///
    /// A marker is its time. A segment name or `Section` key (ignoring case,
    /// as the preview's `--from`) starts a range at its first segment and
    /// ends it at its last; as a frame it is the first stop among its
    /// segments, where a deck's first page of it rests, or without stops
    /// the end of its first segment. A stop name (ignoring case) is the time
    /// the stop rests; one shared by several stops is ambiguous.
    pub fn resolve(&self, role: BoundRole, name: &str) -> Result<f64, String> {
        if let Some((_, time)) = self.markers.iter().find(|(marker, _)| marker == name) {
            return Ok(*time);
        }
        let segments = gaanim_timeline::selection::segments_named(
            name,
            self.segments.iter().map(|segment| segment.name.as_str()),
        );
        if let (Some(&first), Some(&last)) = (segments.first(), segments.last()) {
            return Ok(match role {
                BoundRole::From => self.segments[first].start,
                BoundRole::To => self.segments[last].end,
                BoundRole::Frame => segments
                    .iter()
                    .find_map(|&index| self.segments[index].stops.first())
                    .map_or(self.segments[first].end, |(_, time)| *time),
            });
        }
        let lowercase = name.to_lowercase();
        let stops: Vec<(&str, f64)> = self
            .segments
            .iter()
            .flat_map(|segment| {
                segment
                    .stops
                    .iter()
                    .filter(|(stop, _)| {
                        stop.as_ref()
                            .is_some_and(|stop| stop.to_lowercase() == lowercase)
                    })
                    .map(|(_, time)| (segment.name.as_str(), *time))
            })
            .collect();
        match stops.as_slice() {
            [(_, time)] => Ok(*time),
            [] => Err(self.unknown(role, name)),
            several => Err(format!(
                "{}: {name:?} names {} stops ({}); pass the seconds of the one you mean",
                role.flag(),
                several.len(),
                several
                    .iter()
                    .map(|(segment, time)| format!("{time}s in {segment:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }

    /// The error for a name that matches nothing, listing every name the
    /// scene defines.
    fn unknown(&self, role: BoundRole, name: &str) -> String {
        let quoted = |names: Vec<String>| {
            names
                .iter()
                .map(|name| format!("{name:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut stops: Vec<String> = Vec::new();
        for stop in self
            .segments
            .iter()
            .flat_map(|segment| &segment.stops)
            .filter_map(|(stop, _)| stop.clone())
        {
            if !stops.contains(&stop) {
                stops.push(stop);
            }
        }
        let lists = [
            (
                "markers",
                self.markers
                    .iter()
                    .map(|(marker, _)| marker.clone())
                    .collect(),
            ),
            (
                "segments",
                gaanim_timeline::selection::segment_choices(
                    self.segments.iter().map(|segment| segment.name.as_str()),
                ),
            ),
            ("stops", stops),
        ]
        .into_iter()
        .filter(|(_, names)| !names.is_empty())
        .collect::<Vec<_>>();
        // `--frame` splits its list at commas.
        let hint = if role == BoundRole::Frame
            && lists
                .iter()
                .any(|(_, names)| names.iter().any(|name| name.contains(',')))
        {
            "; write a comma inside a name as \\,"
        } else {
            ""
        };
        let lists = lists
            .into_iter()
            .map(|(kind, names)| format!("{kind}: {}", quoted(names)))
            .collect::<Vec<_>>();
        format!(
            "{}: no marker, segment or stop named {name:?}; {}{hint}",
            role.flag(),
            if lists.is_empty() {
                "the scene defines none (use scene.marker(\"name\"), scene.segment or \
                 scene.stop(\"name\"))"
                    .to_string()
            } else {
                format!("the scene defines {}", lists.join("; "))
            }
        )
    }
}

/// Resolve a bound against the names the scene defines.
pub fn resolve_export_bound(
    role: BoundRole,
    bound: Option<&ExportBound>,
    names: &TimelineNames,
) -> Result<Option<f64>, String> {
    match bound {
        None => Ok(None),
        Some(ExportBound::Seconds(seconds)) => Ok(Some(*seconds)),
        Some(ExportBound::Name(name)) => names.resolve(role, name).map(Some),
    }
}

/// Resolve both ends of an export range and check their order.
pub fn resolve_export_range(
    from: Option<&ExportBound>,
    to: Option<&ExportBound>,
    names: &TimelineNames,
) -> Result<(Option<f64>, Option<f64>), String> {
    let start = resolve_export_bound(BoundRole::From, from, names)?;
    let end = resolve_export_bound(BoundRole::To, to, names)?;
    if let (Some(start), Some(end)) = (start, end)
        && end <= start
    {
        return Err(format!(
            "--to ({}) resolves to {end}s, which must be after --from ({}) at {start}s",
            to.map(ToString::to_string).unwrap_or_default(),
            from.map(ToString::to_string).unwrap_or_default(),
        ));
    }
    Ok((start, end))
}

/// Output formats `gaanim export` writes, by file extension.
pub const EXPORT_EXTENSIONS: [&str; 8] =
    ["mp4", "webm", "mov", "webp", "gif", "png", "gaanim", "pdf"];

/// The extension of a slide deck exported to PDF.
pub const PDF_EXTENSION: &str = "pdf";

/// Video formats that keep the alpha channel with `--transparent`.
pub const TRANSPARENT_EXTENSIONS: [&str; 4] = ["webm", "mov", "webp", "png"];

/// `gaanim export <INPUT> --output <FILE> [options]`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportCommand {
    /// Script, project directory, or playback bundle.
    pub input: Option<PathBuf>,
    pub output: Option<String>,
    pub quality: String,
    pub encoder: VideoEncoder,
    pub transparent: bool,
    pub width: u32,
    pub height: u32,
    pub fit: OutputFit,
    pub from: Option<ExportBound>,
    pub to: Option<ExportBound>,
    /// Frame rate of a video, or recording rate of a playback bundle;
    /// `None` takes the quality preset's.
    pub fps: Option<FrameRate>,
    /// `--frame`: instants written as single PNGs instead of a range.
    pub frames: Vec<ExportBound>,
    /// `--notes`: a PDF deck prints each slide's speaker notes.
    pub notes: bool,
}

impl Default for ExportCommand {
    fn default() -> Self {
        Self {
            input: None,
            output: None,
            quality: "standard".to_string(),
            encoder: VideoEncoder::Auto,
            transparent: false,
            width: 1920,
            height: 1080,
            fit: OutputFit::Error,
            from: None,
            to: None,
            fps: None,
            frames: Vec::new(),
            notes: false,
        }
    }
}

impl ExportCommand {
    /// Parse the arguments that follow `export`.
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let mut command = Self::default();
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--fps" => {
                    index += 1;
                    let value = args.get(index).ok_or_else(|| {
                        "--fps requires a frame rate, e.g. 24 or 24000/1001".to_string()
                    })?;
                    command.fps = Some(value.parse().map_err(|error| format!("--fps: {error}"))?);
                }
                "--frame" => {
                    index += 1;
                    // Names may contain commas written as `\,`, as in `--sections`.
                    let list = args
                        .get(index)
                        .and_then(|list| {
                            gaanim_timeline::selection::SegmentSelection::parse_list(list).ok()
                        })
                        .ok_or_else(|| {
                            "--frame requires seconds or names separated by commas, e.g. \
                             3,11.4,Results (write \\, for a comma inside a name)"
                                .to_string()
                        })?;
                    for value in list {
                        command
                            .frames
                            .push(parse_export_seconds("--frame", Some(&value))?);
                    }
                }
                flag @ ("--from" | "--to") => {
                    index += 1;
                    let bound = parse_export_seconds(flag, args.get(index))?;
                    if flag == "--from" {
                        command.from = Some(bound);
                    } else {
                        command.to = Some(bound);
                    }
                }
                "--output" | "-o" => {
                    index += 1;
                    command.output = args.get(index).cloned();
                }
                "--quality" => {
                    index += 1;
                    command.quality = args.get(index).cloned().unwrap_or_default();
                }
                "--encoder" => {
                    index += 1;
                    command.encoder = args
                        .get(index)
                        .and_then(|value| VideoEncoder::parse_arg(value))
                        .ok_or_else(|| {
                            format!("encoder must be {}", VideoEncoder::ARG_VALUES.join(", "))
                        })?;
                }
                "--transparent" => command.transparent = true,
                "--notes" => command.notes = true,
                flag @ ("--width" | "--height") => {
                    index += 1;
                    let value = args
                        .get(index)
                        .and_then(|value| value.parse().ok())
                        .filter(|value| *value > 0)
                        .ok_or_else(|| format!("{flag} requires a positive integer"))?;
                    if flag == "--width" {
                        command.width = value;
                    } else {
                        command.height = value;
                    }
                }
                "--fit" => {
                    index += 1;
                    command.fit = match args.get(index).map(String::as_str) {
                        Some("error") => OutputFit::Error,
                        Some("contain") => OutputFit::Contain,
                        Some("cover") => OutputFit::Cover,
                        _ => return Err("--fit must be error, contain, or cover".to_string()),
                    };
                }
                value if value.starts_with('-') => {
                    return Err(format!("unknown option `{value}`"));
                }
                value if command.input.is_none() => command.input = Some(PathBuf::from(value)),
                value => return Err(format!("unexpected argument `{value}`")),
            }
            index += 1;
        }
        if !matches!(
            command.quality.as_str(),
            "draft" | "standard" | "production"
        ) {
            return Err("quality must be draft, standard, or production".to_string());
        }
        Ok(command)
    }

    /// The output path and its lowercase format extension.
    pub fn output_format(&self) -> Result<(String, String), String> {
        let output = self
            .output
            .clone()
            .ok_or_else(|| "--output is required".to_string())?;
        let format = Path::new(&output)
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .filter(|format| EXPORT_EXTENSIONS.contains(&format.as_str()))
            .ok_or_else(|| {
                "output extension must be mp4, webm, mov, webp, gif, png, gaanim, or pdf"
                    .to_string()
            })?;
        Ok((output, format))
    }

    /// Frames per second a playback bundle records: a whole rate.
    pub fn recording_fps(&self) -> Result<Option<u32>, String> {
        self.fps
            .map(|fps| {
                fps.as_whole().ok_or_else(|| {
                    format!(
                        "--fps: a playback bundle records a whole number of frames per second, \
                         not {fps}; export a video at {fps} fps instead"
                    )
                })
            })
            .transpose()
    }

    /// Check the options that apply to a video `format`.
    pub fn validate_video(&self, format: &str) -> Result<(), String> {
        if self.notes {
            return Err(
                "--notes prints speaker notes in a PDF; use an output ending in .pdf".to_string(),
            );
        }
        if self.transparent && !TRANSPARENT_EXTENSIONS.contains(&format) {
            return Err("--transparent requires WebM, MOV, WebP, or PNG output".to_string());
        }
        if self.fps.is_some() && !self.frames.is_empty() {
            return Err("--frame writes single instants; drop --fps".to_string());
        }
        if format != "mp4" && self.encoder != VideoEncoder::Auto {
            return Err("--encoder requires MP4 output".to_string());
        }
        if !self.frames.is_empty() {
            if format != "png" {
                return Err("--frame writes PNG files; use an output ending in .png".to_string());
            }
            if self.from.is_some() || self.to.is_some() {
                return Err("--frame exports single instants; drop --from and --to".to_string());
            }
        }
        validate_export_range(self.from.as_ref(), self.to.as_ref())
    }

    /// Check the options that apply to a PDF deck: a page for each point
    /// where the presentation rests, so no range, rate or encoding.
    pub fn validate_pdf(&self) -> Result<(), String> {
        let ignored = [
            (self.from.is_some() || self.to.is_some(), "--from and --to"),
            (!self.frames.is_empty(), "--frame"),
            (self.fps.is_some(), "--fps"),
            (self.transparent, "--transparent"),
            (self.encoder != VideoEncoder::Auto, "--encoder"),
        ];
        match ignored.iter().find(|(given, _)| *given) {
            Some((_, flags)) => Err(format!(
                "a PDF has a page for each point where the presentation rests; drop {flags}"
            )),
            None => Ok(()),
        }
    }

    /// The PNG each `--frame` instant is written to: `output` itself for one
    /// instant, otherwise `output` with the instant's 1-based position
    /// appended (`still.png` -> `still_1.png`, `still_2.png`).
    pub fn frame_outputs(&self, output: &str) -> Vec<PathBuf> {
        let path = Path::new(output);
        if self.frames.len() <= 1 {
            return vec![path.to_path_buf()];
        }
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("frame");
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("png");
        (1..=self.frames.len())
            .map(|position| path.with_file_name(format!("{stem}_{position}.{extension}")))
            .collect()
    }
}

pub fn quality_preset(quality: &str) -> QualityPreset {
    match quality {
        "draft" => QualityPreset::Draft,
        "production" => QualityPreset::Production,
        _ => QualityPreset::Standard,
    }
}

pub fn export_format(format: &str) -> Option<ExportFormat> {
    Some(match format {
        "mp4" => ExportFormat::Mp4,
        "webm" => ExportFormat::Webm,
        "mov" => ExportFormat::Mov,
        "webp" => ExportFormat::Webp,
        "gif" => ExportFormat::Gif,
        "png" => ExportFormat::PngSequence,
        _ => return None,
    })
}

/// The export configuration a video export of `command` uses.
pub fn video_config(
    command: &ExportCommand,
    output: &str,
    format: ExportFormat,
    range: (Option<f64>, Option<f64>),
) -> ExportConfig {
    let mut config = ExportConfig::new(output);
    // The preset sets fps, CRF and speed for the format; an explicit --fps
    // replaces its rate.
    config.format = format;
    let mut config = config.with_quality(quality_preset(&command.quality));
    if let Some(fps) = command.fps {
        config.fps = fps;
    }
    config.width = command.width;
    config.height = command.height;
    config.fit = command.fit;
    config.start_time = range.0;
    config.end_time = range.1;
    config.aspect_ratio = AspectRatioPreset::Custom;
    config.video_encoder = command.encoder;
    config.transparent = command.transparent;
    config.headless = true;
    config
}

/// Render each `--frame` instant of `canvas` as exactly one PNG, seeking the
/// timeline to it as `gaanim --diff` does, and return the files written.
pub fn export_frames(
    canvas: gaanim_api::canvas::SceneModel,
    command: &ExportCommand,
    output: &str,
) -> Result<Vec<PathBuf>, String> {
    let names = TimelineNames::of_canvas(&canvas);
    let times = command
        .frames
        .iter()
        .map(|bound| {
            resolve_export_bound(BoundRole::Frame, Some(bound), &names)
                .map(|time| time.expect("a --frame bound is present"))
        })
        .collect::<Result<Vec<f64>, String>>()?;
    let mut config = ExportConfig::new(output).with_quality(quality_preset(&command.quality));
    config.width = command.width;
    config.height = command.height;
    config.fit = command.fit;
    config.aspect_ratio = AspectRatioPreset::Custom;
    config.transparent = command.transparent;
    config.headless = true;
    let paths = command.frame_outputs(output);
    for path in &paths {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
    }
    let transparent = command.transparent;
    let mut written = Vec::with_capacity(paths.len());
    let mut result = Ok(());
    gaanim_export::prelude::capture_scene_direct_streaming(
        config,
        &times,
        move |world| gaanim_api::runtime::replay_canvas_into(world, canvas),
        |frame| {
            let path = &paths[written.len()];
            match gaanim_export::prelude::write_png_frame(
                path,
                frame.rgba,
                frame.width,
                frame.height,
                transparent,
            ) {
                Ok(()) => {
                    written.push(path.clone());
                    std::ops::ControlFlow::Continue(())
                }
                Err(error) => {
                    result = Err(error.to_string());
                    std::ops::ControlFlow::Break(())
                }
            }
        },
    )
    .map_err(|error| error.to_string())?;
    result.map(|()| written)
}

/// The slides of a deck: each segment's name, speaker notes and stop times.
pub fn deck_slides<'a>(
    segments: impl IntoIterator<Item = (&'a str, Option<&'a str>, Vec<f64>)>,
) -> Vec<gaanim_export::deck::DeckSlide> {
    segments
        .into_iter()
        .map(|(name, notes, stops)| gaanim_export::deck::DeckSlide {
            name: name.to_owned(),
            notes: notes.map(str::to_owned),
            stops,
        })
        .collect()
}

/// The times of the frames among `pages`, in order.
#[cfg(not(target_arch = "wasm32"))]
fn frame_times(pages: &[gaanim_export::deck::DeckPage]) -> Vec<f64> {
    pages
        .iter()
        .filter_map(|page| match page {
            gaanim_export::deck::DeckPage::Frame { time, .. } => Some(*time),
            gaanim_export::deck::DeckPage::Notes { .. } => None,
        })
        .collect()
}

/// Keep a captured `frame` as a deck page and count it among `total`.
#[cfg(not(target_arch = "wasm32"))]
fn capture_page(
    frames: &mut Vec<Vec<u8>>,
    frame: &gaanim_export::prelude::CapturedFrame,
    total: usize,
    telemetry: &Option<gaanim_export::prelude::ExportTelemetry>,
) -> std::ops::ControlFlow<()> {
    frames.push(gaanim_export::pdf::frame_pdf(
        &frame.rgba,
        frame.width,
        frame.height,
    ));
    gaanim_export::pdf::report_frames(telemetry, frames.len() as u64, total as u64);
    std::ops::ControlFlow::Continue(())
}

/// Lay out `pages` around the captured `frames` and write the PDF to
/// `output`; returns the number of pages.
#[cfg(not(target_arch = "wasm32"))]
fn write_deck(
    output: &str,
    pages: &[gaanim_export::deck::DeckPage],
    slides: &[gaanim_export::deck::DeckSlide],
    frames: Vec<Vec<u8>>,
    size: (u32, u32),
    title: &str,
) -> Result<usize, String> {
    use gaanim_export::deck::DeckPage;
    use gaanim_export::pdf::PdfPage;
    let mut frames = frames.into_iter();
    let pages = pages
        .iter()
        .map(|page| match *page {
            DeckPage::Frame { .. } => frames.next().map(PdfPage::Frame),
            DeckPage::Notes { slide } => Some(PdfPage::Notes {
                title: slides[slide].name.clone(),
                // A `[stop]` mark prints as an arrow where the speaker advances.
                notes: gaanim_timeline::notes::printable(
                    slides[slide].notes.as_deref().unwrap_or_default().trim(),
                ),
            }),
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| "the renderer produced fewer frames than the deck has pages".to_owned())?;
    let count = pages.len();
    let pdf =
        gaanim_export::pdf::deck_pdf(pages, size, title).map_err(|error| error.to_string())?;
    if let Some(parent) = Path::new(output)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(output, pdf).map_err(|error| format!("{output}: {error}"))?;
    Ok(count)
}

/// Export `canvas` as a PDF deck at `output`, titled after the project
/// `script` belongs to (or the script): a page at the export size for every
/// point where its presentation rests, and with `--notes` each slide's
/// speaker notes after its last page. Captured frames count in `telemetry`.
/// Returns the number of pages.
#[cfg(not(target_arch = "wasm32"))]
pub fn export_slides_pdf(
    canvas: gaanim_api::canvas::SceneModel,
    command: &ExportCommand,
    output: &str,
    script: Option<&Path>,
    telemetry: Option<gaanim_export::prelude::ExportTelemetry>,
) -> Result<usize, String> {
    let manifest = canvas.segment_manifest();
    let slides = deck_slides(manifest.segments.iter().map(|segment| {
        (
            segment.name.as_str(),
            segment.notes.as_deref(),
            segment.stops.iter().map(|stop| stop.time).collect(),
        )
    }));
    let pages = gaanim_export::deck::deck_pages(&slides, manifest.duration(), command.notes);
    let mut config = ExportConfig::new(output).with_quality(quality_preset(&command.quality));
    config.width = command.width;
    config.height = command.height;
    config.fit = command.fit;
    config.aspect_ratio = AspectRatioPreset::Custom;
    config.headless = true;
    let times = frame_times(&pages);
    let mut frames = Vec::with_capacity(times.len());
    gaanim_export::prelude::capture_scene_direct_streaming(
        config,
        &times,
        move |world| gaanim_api::runtime::replay_canvas_into(world, canvas),
        |frame| capture_page(&mut frames, &frame, times.len(), &telemetry),
    )
    .map_err(|error| error.to_string())?;
    let title = script.and_then(bundle_title).unwrap_or_default();
    write_deck(
        output,
        &pages,
        &slides,
        frames,
        (command.width, command.height),
        &title,
    )
}

/// Export a playback bundle as a PDF deck, without Python: every page is
/// the frame the bundle recorded where its presentation rests, as
/// [`export_slides_pdf`] lays out a script's. Returns the number of pages.
#[cfg(not(target_arch = "wasm32"))]
pub fn export_bundle_pdf(
    command: &ExportCommand,
    telemetry: Option<gaanim_export::prelude::ExportTelemetry>,
) -> Result<usize, CommandError> {
    let input = command
        .input
        .as_deref()
        .ok_or_else(|| CommandError::Usage("a bundle to export is required".to_string()))?;
    let (output, _) = command.output_format().map_err(CommandError::Usage)?;
    command.validate_pdf().map_err(CommandError::Usage)?;
    let bundle = gaanim_bundle::Bundle::open(input)
        .map_err(|error| CommandError::Failed(format!("{}: {error}", input.display())))?;
    gaanim_export::prelude::check_bundle_fit(
        bundle.scene.output_size,
        (command.width, command.height),
        command.fit,
    )
    .map_err(|error| CommandError::Usage(error.to_string()))?;
    let slides = deck_slides(bundle.scene.segments.iter().map(|segment| {
        (
            segment.name.as_str(),
            segment.notes.as_deref(),
            segment.stops.iter().map(|stop| stop.time).collect(),
        )
    }));
    let pages = gaanim_export::deck::deck_pages(&slides, bundle.scene.duration, command.notes);
    let title = bundle.scene.title.clone();
    drop(bundle);
    let times = frame_times(&pages);
    let mut frames = Vec::with_capacity(times.len());
    gaanim_export::prelude::capture_bundle_streaming(
        input,
        command.width,
        command.height,
        command.fit,
        &times,
        |frame| capture_page(&mut frames, &frame, times.len(), &telemetry),
    )
    .map_err(|error| CommandError::Failed(error.to_string()))?;
    write_deck(
        &output,
        &pages,
        &slides,
        frames,
        (command.width, command.height),
        &title,
    )
    .map_err(CommandError::Failed)
}

/// Frames per second a bundle records for an export `quality`: the rate a
/// video of that quality has.
pub fn bundle_fps(quality: &str) -> u32 {
    match quality {
        "draft" => 30,
        _ => 60,
    }
}

/// The recording of `canvas` into a bundle at `output`, named after the
/// project `script` belongs to (or the script) and sized like the scene's
/// preview.
pub fn bundle_config(
    script: Option<&Path>,
    canvas: &gaanim_api::canvas::SceneModel,
    output: &str,
    fps: Option<u32>,
) -> gaanim_api::export::BundleConfig {
    let mut config = gaanim_api::export::BundleConfig::new(output);
    if let Some(title) = script.and_then(bundle_title) {
        config.title = title;
    }
    if let Some(fps) = fps {
        config.fps = fps;
    }
    (config.width, config.height) = canvas.frame.preview_pixel_size();
    // Verification switch: record a second world regardless of the scene, to
    // compare it with the single-world recording.
    config.force_second_world = std::env::var_os("GAANIM_BUNDLE_SECOND_WORLD").is_some();
    config
}

/// The title a bundle recorded from `script` carries: its project's name,
/// which the player shows in the window title, or the script's name outside
/// a project. Every project's entry is `main.py`, so its name says nothing.
fn bundle_title(script: &Path) -> Option<String> {
    let project = gaanim_project::find_project_for_script(script)
        .map(|project| project.manifest.name.trim().to_owned())
        .filter(|name| !name.is_empty());
    project.or_else(|| {
        script
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::to_owned)
    })
}

/// Why a command stopped: bad arguments (exit status 2) or a failed run (1).
#[derive(Debug, Clone, PartialEq)]
pub enum CommandError {
    Usage(String),
    Failed(String),
}

impl CommandError {
    /// Report the error and exit with its status.
    pub fn exit(self, scope: &str) -> ! {
        let (status, message) = match self {
            Self::Usage(message) => (2, message),
            Self::Failed(message) => (1, message),
        };
        console::error(scope, message);
        std::process::exit(status);
    }
}

/// Export a playback bundle to video, without Python: every frame is
/// composed from the recorded frame shown at that instant.
pub fn export_bundle_video(command: &ExportCommand) -> Result<(), CommandError> {
    let input = command
        .input
        .as_deref()
        .ok_or_else(|| CommandError::Usage("a bundle to export is required".to_string()))?;
    let (output, format) = command.output_format().map_err(CommandError::Usage)?;
    if !command.frames.is_empty() {
        return Err(CommandError::Usage(
            "--frame renders a script or project; for a bundle, export a short range with --from and --to"
                .to_string(),
        ));
    }
    if format == gaanim_bundle::EXTENSION {
        return Err(CommandError::Usage(
            "the input is already a playback bundle; export it to mp4, webm, mov, webp, gif, or png"
                .to_string(),
        ));
    }
    command
        .validate_video(&format)
        .map_err(CommandError::Usage)?;
    let bundle = gaanim_bundle::Bundle::open(input)
        .map_err(|error| CommandError::Failed(format!("{}: {error}", input.display())))?;
    let names = TimelineNames::of_timeline(&bundle.scene.markers, &bundle.scene.segments);
    let range = resolve_export_range(command.from.as_ref(), command.to.as_ref(), &names)
        .map_err(CommandError::Usage)?;
    // Every frame of the video is a recorded frame.
    let recorded = FrameRate::whole(bundle.scene.fps);
    if let Some(fps) = command.fps.filter(|fps| *fps != recorded) {
        return Err(CommandError::Usage(format!(
            "--fps: a bundle exports at the rate it was recorded at ({recorded} fps); \
             export its script with --fps {fps}{}",
            if fps.as_whole().is_some() {
                ", or record the bundle again with it"
            } else {
                ""
            }
        )));
    }
    let format = export_format(&format).expect("validated export format");
    drop(bundle);
    // The video runs at the bundle's recorded rate.
    gaanim_export::prelude::export_bundle(input, video_config(command, &output, format, range))
        .map_err(|error| CommandError::Failed(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn bundles_are_titled_after_their_project() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project = gaanim_project::create_project(&gaanim_project::CreateProjectOptions {
            kind: gaanim_project::ProjectKind::Slides,
            directory: temp.path().join("tesis"),
            force: false,
        })
        .expect("project");
        assert_eq!(bundle_title(&project.entry).as_deref(), Some("tesis"));
        let loose = temp.path().join("demo.py");
        assert_eq!(bundle_title(&loose).as_deref(), Some("demo"));
    }

    #[test]
    fn frame_exports_single_instants_as_png() {
        let one = ExportCommand::parse(&args(&["scene.py", "-o", "still.png", "--frame", "11.4"]))
            .unwrap();
        assert_eq!(one.frames, vec![ExportBound::Seconds(11.4)]);
        assert!(one.validate_video("png").is_ok());
        assert_eq!(
            one.frame_outputs("out/still.png"),
            vec![PathBuf::from("out/still.png")]
        );

        let many = ExportCommand::parse(&args(&[
            "scene.py",
            "-o",
            "still.png",
            "--frame",
            "3, climax",
        ]))
        .unwrap();
        assert_eq!(
            many.frames,
            vec![
                ExportBound::Seconds(3.0),
                ExportBound::Name("climax".into())
            ]
        );
        assert_eq!(
            many.frame_outputs("out/still.png"),
            vec![
                PathBuf::from("out/still_1.png"),
                PathBuf::from("out/still_2.png")
            ]
        );

        assert!(one.validate_video("mp4").is_err());
        let ranged = ExportCommand::parse(&args(&[
            "scene.py",
            "-o",
            "still.png",
            "--frame",
            "3",
            "--from",
            "1",
        ]))
        .unwrap();
        assert!(ranged.validate_video("png").is_err());
        assert!(ExportCommand::parse(&args(&["scene.py", "--frame", "-1"])).is_err());
        assert!(ExportCommand::parse(&args(&["scene.py", "--frame"])).is_err());
        assert!(ExportCommand::parse(&args(&["scene.py", "--frame", "3,,4"])).is_err());
        // A comma inside a name is written `\,`, as in `--sections`.
        let comma = ExportCommand::parse(&args(&[
            "scene.py",
            "-o",
            "still.png",
            "--frame",
            r"Tiempo\, lugar, 2",
        ]))
        .unwrap();
        assert_eq!(
            comma.frames,
            vec![
                ExportBound::Name("Tiempo, lugar".into()),
                ExportBound::Seconds(2.0)
            ]
        );
    }

    /// A deck: markers, a plain segment, a `Section` with two steps and a
    /// segment without stops.
    fn deck_names() -> TimelineNames {
        let segment = |name: &str, start, end, stops: &[(Option<&str>, f64)]| NamedSegment {
            name: name.to_string(),
            start,
            end,
            stops: stops
                .iter()
                .map(|(name, time)| (name.map(str::to_string), *time))
                .collect(),
        };
        TimelineNames {
            markers: vec![("climax".into(), 2.5), ("Problema".into(), 9.0)],
            segments: vec![
                segment("Portada", 0.0, 2.0, &[(None, 1.0)]),
                segment(
                    "Problema",
                    2.0,
                    6.0,
                    &[(Some("pregunta"), 3.0), (Some("respuesta"), 5.0)],
                ),
                segment("results · 1 · 1 · Datos", 6.0, 8.0, &[]),
                segment(
                    "results · 1 · 2 · Curvas",
                    8.0,
                    10.0,
                    &[(Some("respuesta"), 9.5)],
                ),
                segment("Cierre", 10.0, 12.0, &[]),
            ],
        }
    }

    #[test]
    fn export_bounds_resolve_markers_segments_and_stops() {
        let names = deck_names();
        let resolve = |role, name: &str| names.resolve(role, name);
        // Markers come first, even over a segment with the same name.
        assert_eq!(resolve(BoundRole::From, "climax"), Ok(2.5));
        assert_eq!(resolve(BoundRole::Frame, "Problema"), Ok(9.0));
        // A segment starts a range, ends it, and as a frame rests at its
        // first stop; a name ignores case.
        let without_marker = TimelineNames {
            markers: Vec::new(),
            ..deck_names()
        };
        assert_eq!(without_marker.resolve(BoundRole::From, "problema"), Ok(2.0));
        assert_eq!(without_marker.resolve(BoundRole::To, "Problema"), Ok(6.0));
        assert_eq!(
            without_marker.resolve(BoundRole::Frame, "Problema"),
            Ok(3.0)
        );
        // A Section key spans all its steps; its frame is the first stop
        // among them, as a deck's first page of it.
        assert_eq!(resolve(BoundRole::From, "RESULTS"), Ok(6.0));
        assert_eq!(resolve(BoundRole::To, "results"), Ok(10.0));
        assert_eq!(resolve(BoundRole::Frame, "results"), Ok(9.5));
        // A step title names its step; one without stops rests at its end.
        assert_eq!(resolve(BoundRole::Frame, "datos"), Ok(8.0));
        assert_eq!(resolve(BoundRole::Frame, "Cierre"), Ok(12.0));
        // A stop is the instant it rests, whatever the role.
        assert_eq!(resolve(BoundRole::Frame, "Pregunta"), Ok(3.0));
        assert_eq!(resolve(BoundRole::To, "pregunta"), Ok(3.0));
        // A stop name shared by several stops is ambiguous.
        let error = resolve(BoundRole::Frame, "respuesta").unwrap_err();
        assert!(
            error.contains("names 2 stops") && error.contains("5s in \"Problema\""),
            "{error}"
        );
        // An unknown name lists every kind of name, Section keys once.
        let error = resolve(BoundRole::To, "fin").unwrap_err();
        assert!(error.starts_with("--to: no marker, segment or stop named \"fin\""));
        for listed in [
            "markers: \"climax\", \"Problema\"",
            "segments: \"Portada\", \"Problema\", \"results\", \"Cierre\"",
            "stops: \"pregunta\", \"respuesta\"",
        ] {
            assert!(error.contains(listed), "{error}");
        }
        let error = TimelineNames::default()
            .resolve(BoundRole::Frame, "fin")
            .unwrap_err();
        assert!(error.contains("the scene defines none"), "{error}");
        // A frame list split inside a name hints at the escape.
        let comma = TimelineNames {
            segments: vec![NamedSegment {
                name: "Tiempo, lugar".into(),
                start: 0.0,
                end: 1.0,
                stops: Vec::new(),
            }],
            ..TimelineNames::default()
        };
        let error = comma.resolve(BoundRole::Frame, "Tiempo").unwrap_err();
        assert!(
            error.ends_with(r"write a comma inside a name as \,"),
            "{error}"
        );
    }

    #[test]
    fn export_ranges_resolve_names_in_order() {
        let names = deck_names();
        let climax = ExportBound::Name("climax".into());
        assert_eq!(
            resolve_export_bound(BoundRole::To, Some(&ExportBound::Seconds(4.0)), &names),
            Ok(Some(4.0))
        );
        assert_eq!(
            resolve_export_range(
                Some(&ExportBound::Name("Portada".into())),
                Some(&ExportBound::Name("results".into())),
                &names
            ),
            Ok((Some(0.0), Some(10.0)))
        );
        // Named ranges are only ordered once the names are known.
        assert!(validate_export_range(Some(&climax), Some(&ExportBound::Seconds(0.1))).is_ok());
        assert!(
            resolve_export_range(Some(&climax), Some(&ExportBound::Seconds(0.1)), &names).is_err()
        );
    }

    #[test]
    fn parses_export_commands() {
        let command = ExportCommand::parse(&args(&[
            "deck.gaanim",
            "-o",
            "talk.mp4",
            "--quality",
            "draft",
            "--from",
            "intro",
            "--to",
            "12",
            "--width",
            "1280",
            "--height",
            "720",
        ]))
        .unwrap();
        assert_eq!(command.input, Some(PathBuf::from("deck.gaanim")));
        assert_eq!(
            command.output_format().unwrap(),
            ("talk.mp4".to_string(), "mp4".to_string())
        );
        assert_eq!(command.from, Some(ExportBound::Name("intro".into())));
        assert_eq!(command.to, Some(ExportBound::Seconds(12.0)));
        assert_eq!((command.width, command.height), (1280, 720));
        assert!(command.validate_video("mp4").is_ok());

        let bundle =
            ExportCommand::parse(&args(&["scene.py", "-o", "x.gaanim", "--fps", "30"])).unwrap();
        assert_eq!(bundle.fps, Some(FrameRate::whole(30)));
        assert_eq!(bundle.recording_fps(), Ok(Some(30)));

        for invalid in [
            &["scene.py", "--fps", "0"][..],
            &["scene.py", "--fps", "241"],
            &["scene.py", "--fps", "24/0"],
            &["scene.py", "--fps"],
            &["scene.py", "--quality", "ultra"],
            &["scene.py", "--fit", "stretch"],
            &["scene.py", "--from", "-1"],
            &["scene.py", "other.py"],
            &["scene.py", "--frobnicate"],
        ] {
            assert!(ExportCommand::parse(&args(invalid)).is_err(), "{invalid:?}");
        }
        let avi = ExportCommand::parse(&args(&["scene.py", "-o", "x.avi"])).unwrap();
        assert!(avi.output_format().is_err());
        let transparent =
            ExportCommand::parse(&args(&["scene.py", "-o", "x.mp4", "--transparent"])).unwrap();
        assert!(transparent.validate_video("mp4").is_err());
        for format in ["webm", "mov", "webp", "png"] {
            assert!(transparent.validate_video(format).is_ok(), "{format}");
        }
    }

    #[test]
    fn decks_export_to_pdf_with_or_without_notes() {
        let deck = ExportCommand::parse(&args(&["talk.py", "-o", "talk.pdf", "--notes"])).unwrap();
        assert!(deck.notes);
        assert_eq!(
            deck.output_format().unwrap(),
            ("talk.pdf".to_string(), PDF_EXTENSION.to_string())
        );
        assert!(deck.validate_pdf().is_ok());
        // Notes print only in a PDF.
        assert!(deck.validate_video("mp4").is_err());
        // A PDF rests where the presentation does: no range, rate or encoding.
        for flags in [
            &["--from", "2"][..],
            &["--to", "intro"],
            &["--frame", "3"],
            &["--fps", "24"],
            &["--transparent"],
            &["--encoder", "libx264"],
        ] {
            let mut arguments = vec!["talk.py", "-o", "talk.pdf"];
            arguments.extend_from_slice(flags);
            let command = ExportCommand::parse(&args(&arguments)).unwrap();
            assert!(command.validate_pdf().is_err(), "{flags:?}");
        }
    }

    #[test]
    fn videos_export_at_an_explicit_frame_rate() {
        let command = ExportCommand::parse(&args(&[
            "scene.py",
            "-o",
            "film.mp4",
            "--fps",
            "24000/1001",
        ]))
        .unwrap();
        let ntsc = FrameRate::new(24_000, 1001).unwrap();
        assert_eq!(command.fps, Some(ntsc));
        assert!(command.validate_video("mp4").is_ok());
        let config = video_config(&command, "film.mp4", ExportFormat::Mp4, (None, None));
        // The exporter applies the presets again; the explicit rate survives.
        assert_eq!(config.apply_presets().fps, ntsc);
        // A bundle cannot record a fractional rate.
        assert!(command.recording_fps().unwrap_err().contains("23.976"));

        // Without --fps the preset decides, and GIF stays at 30.
        let preset = ExportCommand::parse(&args(&["scene.py", "-o", "loop.gif"])).unwrap();
        let config = video_config(&preset, "loop.gif", ExportFormat::Gif, (None, None));
        assert_eq!(config.apply_presets().fps, FrameRate::whole(30));
        let explicit =
            ExportCommand::parse(&args(&["scene.py", "-o", "loop.gif", "--fps", "50"])).unwrap();
        let config = video_config(&explicit, "loop.gif", ExportFormat::Gif, (None, None));
        assert_eq!(config.apply_presets().fps, FrameRate::whole(50));

        let still = ExportCommand::parse(&args(&[
            "scene.py",
            "-o",
            "still.png",
            "--frame",
            "2",
            "--fps",
            "24",
        ]))
        .unwrap();
        assert!(still.validate_video("png").is_err());
    }

    #[test]
    fn mov_outputs_export_prores() {
        let command = ExportCommand::parse(&args(&["lower.py", "-o", "lower.MOV"])).unwrap();
        assert_eq!(
            command.output_format().unwrap(),
            ("lower.MOV".to_string(), "mov".to_string())
        );
        assert_eq!(export_format("mov"), Some(ExportFormat::Mov));
        let config = video_config(&command, "lower.MOV", ExportFormat::Mov, (None, None));
        assert_eq!(config.format, ExportFormat::Mov);
        let encoder =
            ExportCommand::parse(&args(&["a.py", "-o", "a.mov", "--encoder", "nvenc"])).unwrap();
        assert!(encoder.validate_video("mov").is_err());
    }
}
