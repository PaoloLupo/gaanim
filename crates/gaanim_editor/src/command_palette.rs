//! The command palette (`Ctrl+Shift+P`), the jump list (`Ctrl+P`) and the
//! keyboard shortcut sheet (`?`). The palette holds every editor action,
//! found by typing, with the shortcut that runs it directly; the sheet reads
//! the same table, [`COMMANDS`], so it never drifts from the palette. The
//! jump list finds a segment or a stop by name and seeks to it, showing the
//! notes said there beside the list.
//!
//! Actions of the playback bar go back to it as a [`PlaybackAction`], so a
//! command does exactly what its button does; the rest run here.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy_egui::egui;
use gaanim_project::RecentEntry;
use gaanim_timeline::notes::split_notes;
use gaanim_timeline::timeline::Timeline;

use crate::animation_timeline::AnimationTimeline;
use crate::console_panel::ConsolePanel;
use crate::frame_capture::FrameCapture;
use crate::overlays::EditorOverlays;
use crate::settings::{Section, SettingsWindow};
use crate::ui_kit::{card_frame, palette};
use crate::{PlaybackAction, PresentationMode, WEB};

/// What a command does.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Command {
    PlayPause,
    GoToStart,
    GoToEnd,
    LoopScene,
    Continuous,
    FullScreen,
    PinOnTop,
    Overlays,
    Animations,
    Console,
    Prompter,
    Narration,
    Export,
    Present,
    SaveFrame,
    Settings(Section),
    Shortcuts,
    Home,
    /// A recent project or bundle, by path.
    OpenRecent(PathBuf),
    /// The jump list.
    Jump,
    /// Pause at this instant: a segment's start or a stop.
    Seek(f64),
}

/// A command as the palette and the sheet list it.
#[derive(Debug, Clone, Copy)]
struct Spec {
    group: &'static str,
    title: &'static str,
    shortcut: Option<&'static str>,
    command: fn() -> Command,
}

/// Every command, in the order the sheet groups them.
const COMMANDS: &[Spec] = &[
    Spec {
        group: "Playback",
        title: "Play / pause",
        shortcut: Some("Space"),
        command: || Command::PlayPause,
    },
    Spec {
        group: "Playback",
        title: "Go to start",
        shortcut: Some("Home"),
        command: || Command::GoToStart,
    },
    Spec {
        group: "Playback",
        title: "Go to end",
        shortcut: Some("End"),
        command: || Command::GoToEnd,
    },
    Spec {
        group: "Playback",
        title: "Loop this scene",
        shortcut: Some("L"),
        command: || Command::LoopScene,
    },
    Spec {
        group: "Playback",
        title: "Continuous playback (ignore stops)",
        shortcut: None,
        command: || Command::Continuous,
    },
    Spec {
        group: "View",
        title: "Full screen",
        shortcut: Some("F11"),
        command: || Command::FullScreen,
    },
    Spec {
        group: "View",
        title: "Pin window on top",
        shortcut: Some("P"),
        command: || Command::PinOnTop,
    },
    Spec {
        group: "View",
        title: "Overlays: bounds, grid, guides, layout",
        shortcut: Some("O"),
        command: || Command::Overlays,
    },
    Spec {
        group: "View",
        title: "Animation timeline",
        shortcut: Some("T"),
        command: || Command::Animations,
    },
    Spec {
        group: "View",
        title: "Console",
        shortcut: Some("J"),
        command: || Command::Console,
    },
    Spec {
        group: "View",
        title: "Prompter caption",
        shortcut: Some("Shift+T"),
        command: || Command::Prompter,
    },
    Spec {
        group: "Make",
        title: "Script & narration",
        shortcut: None,
        command: || Command::Narration,
    },
    Spec {
        group: "Make",
        title: "Export…",
        shortcut: None,
        command: || Command::Export,
    },
    Spec {
        group: "Make",
        title: "Present in full screen",
        shortcut: None,
        command: || Command::Present,
    },
    Spec {
        group: "Make",
        title: "Save the frame as PNG",
        shortcut: Some("Ctrl+Shift+S"),
        command: || Command::SaveFrame,
    },
    Spec {
        group: "Gaanim",
        title: "Settings",
        shortcut: Some("Ctrl+,"),
        command: || Command::Settings(Section::Recording),
    },
    Spec {
        group: "Playback",
        title: "Go to a segment or stop…",
        shortcut: Some("Ctrl+P"),
        command: || Command::Jump,
    },
    Spec {
        group: "Gaanim",
        title: "Keyboard shortcuts",
        shortcut: Some("?"),
        command: || Command::Shortcuts,
    },
    Spec {
        group: "Gaanim",
        title: "Back to Home",
        shortcut: None,
        command: || Command::Home,
    },
];

/// Keys that act without a command of their own, for the sheet.
const OTHER_KEYS: &[(&str, &str, &str)] = &[
    (
        "Playback",
        "← →",
        "Previous or next scene; at stops, go back or advance",
    ),
    (
        "Overlays",
        "B C G M",
        "With overlays on: bounds, coordinates, grid, guides",
    ),
    LAYOUT_KEYS,
    (
        "Overlays",
        "Ctrl+C",
        "Copy the coordinates under the pointer",
    ),
    (
        "Preview",
        "I",
        "Interactive view: move the camera; R resets it, F frames the scene",
    ),
    (
        "Preview",
        "Numpad 0",
        "In 3D, switch between the scene's camera and a free one",
    ),
    ("Gaanim", "Ctrl+Shift+P", "Command palette"),
    (
        "Gaanim",
        "Esc",
        "Close a window, leave full screen or the interactive view",
    ),
];

/// The layout overlay's key, left off the sheet while a bundle plays: a
/// bundle records frames, not the layout that placed them.
const LAYOUT_KEYS: (&str, &str, &str) =
    ("Overlays", "K", "With overlays on: layout boxes and zones");

/// Whether the palette and the sheet offer `spec`: a bundle records frames,
/// not the animations the timeline panel lists.
fn offered(spec: &Spec, bundle: bool) -> bool {
    !bundle || (spec.command)() != Command::Animations
}

/// `spec`'s title; while a bundle plays, the overlays have no layout.
fn spec_title(spec: &Spec, bundle: bool) -> &'static str {
    if bundle && (spec.command)() == Command::Overlays {
        "Overlays: bounds, grid, guides"
    } else {
        spec.title
    }
}

/// An entry of the palette's list.
#[derive(Debug, Clone)]
struct Item {
    title: String,
    group: String,
    shortcut: Option<&'static str>,
    command: Command,
    /// Where a jump lands, shown instead of a shortcut.
    time: Option<f64>,
    /// What the jump list shows beside the list: the notes said there.
    preview: Option<String>,
    /// A stop, listed under its segment.
    nested: bool,
    /// The preview's heading when it says more than the title.
    heading: Option<String>,
    /// The cue preview of a jump: its segment's entry or one of its stops,
    /// as Presenter View keys them.
    cue: Option<crate::presenter::ThumbnailKey>,
}

/// The palette's entries: every command (those a `bundle` can use while
/// one plays), each Settings section, and the recent projects to switch to.
fn items(recents: &[RecentEntry], bundle: bool) -> Vec<Item> {
    let mut items: Vec<Item> = COMMANDS
        .iter()
        .filter(|spec| offered(spec, bundle))
        .map(|spec| Item {
            title: spec_title(spec, bundle).to_owned(),
            group: spec.group.to_owned(),
            shortcut: spec.shortcut,
            command: (spec.command)(),
            time: None,
            preview: None,
            nested: false,
            heading: None,
            cue: None,
        })
        .collect();
    items.extend(Section::ALL.iter().map(|&section| Item {
        title: format!("Settings: {}", section.title()),
        group: "Gaanim".to_owned(),
        shortcut: None,
        command: Command::Settings(section),
        time: None,
        preview: None,
        nested: false,
        heading: None,
        cue: None,
    }));
    items.extend(recents.iter().map(|entry| {
        let (name, kind) = match entry {
            RecentEntry::Project(project) => (project.display_name(), "Project"),
            RecentEntry::Bundle(bundle) => (bundle.name.clone(), "Bundle"),
            RecentEntry::Workspace(workspace) => (workspace.name.clone(), "Workspace"),
        };
        Item {
            title: format!("Open {name}"),
            group: format!("Recent {}", kind.to_lowercase()),
            shortcut: None,
            command: Command::OpenRecent(entry.path().to_path_buf()),
            time: None,
            preview: None,
            nested: false,
            heading: None,
            cue: None,
        }
    }));
    items
}

/// With a workspace member open: the other members to switch to, in the
/// workspace's order, in place of their recent entries, and its page in
/// place of Home.
fn add_workspace_items(items: &mut Vec<Item>, project_dir: &std::path::Path) {
    let Ok(gaanim_project::Location {
        project: Some(project),
        workspace: Some(workspace),
    }) = gaanim_project::locate_folder(project_dir)
    else {
        return;
    };
    if workspace.root == project.root {
        return;
    }
    if let Some(home) = items.iter_mut().find(|item| item.command == Command::Home) {
        home.title = format!("Back to {}", workspace.name);
        home.command = Command::OpenRecent(workspace.root.clone());
    }
    let members: Vec<gaanim_project::WorkspaceMember> = workspace
        .members()
        .into_iter()
        .filter(|member| member.root != project.root && member.project.is_ok())
        .collect();
    items.retain(|item| match &item.command {
        Command::OpenRecent(path) => !members.iter().any(|member| &member.root == path),
        _ => true,
    });
    items.extend(members.iter().map(|member| Item {
        title: format!("Open {}", member.name()),
        group: if member.group.is_empty() {
            workspace.name.clone()
        } else {
            format!("{} › {}", workspace.name, member.group)
        },
        shortcut: None,
        command: Command::OpenRecent(member.root.clone()),
        time: None,
        preview: None,
        nested: false,
        heading: None,
        cue: None,
    }));
}

/// The jump list's entries: each segment, then its stops, in timeline
/// order, with the notes the speaker says from there.
fn jump_items(timeline: &Timeline) -> Vec<Item> {
    let mut items = Vec::new();
    for segment in &timeline.segments {
        let parts = segment
            .notes
            .as_deref()
            .map(split_notes)
            .unwrap_or_default();
        let said = |index: usize| {
            parts
                .get(index)
                .map(|part| part.text.clone())
                .filter(|text| !text.is_empty())
        };
        // Where the bar's segment list lands too: inside the segment, not on
        // the last stop of the one before.
        let entry = crate::presenter::segment_entry_time(timeline, segment);
        let name = crate::scene_display_name(&segment.name);
        items.push(Item {
            title: name.to_owned(),
            group: "segment".to_owned(),
            shortcut: None,
            command: Command::Seek(entry),
            time: Some(entry),
            preview: said(0),
            nested: false,
            heading: None,
            cue: Some((segment.id, crate::presenter::ThumbnailMoment::Entry)),
        });
        for (index, stop) in segment.stops.iter().enumerate() {
            let stop_name = stop
                .name
                .clone()
                .unwrap_or_else(|| format!("stop {}", index + 1));
            // Under its segment, by its own name; the segment's name still
            // finds it, through the group.
            items.push(Item {
                heading: Some(format!("{name} › {stop_name}")),
                title: stop_name,
                group: format!("stop · {name}"),
                shortcut: None,
                command: Command::Seek(stop.time),
                time: Some(stop.time),
                preview: said(index + 1),
                nested: true,
                cue: Some((
                    segment.id,
                    crate::presenter::ThumbnailMoment::Stop(index as u32),
                )),
            });
        }
    }
    items
}

/// The entry the playhead is in: the last one that starts at or before it.
fn current_jump(items: &[Item], now: f64) -> usize {
    items
        .iter()
        .rposition(|item| item.time.is_some_and(|time| time <= now + 1e-6))
        .unwrap_or(0)
}

/// How well `query` matches `text`: its characters in order, ignoring case,
/// scored higher at word starts and in runs; `None` when they are not all
/// there. An empty query matches everything equally.
fn score(query: &str, text: &str) -> Option<i32> {
    let text: Vec<char> = text.to_lowercase().chars().collect();
    let mut total = 0;
    let mut at = 0;
    let mut previous: Option<usize> = None;
    for wanted in query.to_lowercase().chars().filter(|c| !c.is_whitespace()) {
        let found = (at..text.len()).find(|&index| text[index] == wanted)?;
        let word_start = found == 0 || !text[found - 1].is_alphanumeric();
        total += if word_start { 10 } else { 1 };
        if previous.is_some_and(|previous| previous + 1 == found) {
            total += 5;
        }
        // Earlier matches read as better ones.
        total -= (found as i32).min(20) / 4;
        previous = Some(found);
        at = found + 1;
    }
    Some(total)
}

/// The items matching `query`, best first; ties keep the list's order.
fn filter<'a>(items: &'a [Item], query: &str) -> Vec<&'a Item> {
    let mut found: Vec<(i32, usize, &Item)> = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let title = score(query, &item.title);
            let group = score(query, &format!("{} {}", item.group, item.title));
            title
                .map(|score| score + 3)
                .max(group)
                .map(|score| (score, index, item))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    found.into_iter().map(|(_, _, item)| item).collect()
}

/// The palette and the sheet: which is open, what is typed, and the action
/// handed to the playback bar.
#[derive(Resource, Default)]
pub(crate) struct CommandPalette {
    open: bool,
    /// The jump list rather than the commands.
    jumping: bool,
    sheet: bool,
    query: String,
    highlight: usize,
    items: Vec<Item>,
    /// Focus the search field on the next frame.
    focus: bool,
    forwarded: Option<PlaybackAction>,
    /// Whether a bundle plays rather than a scene.
    bundle: bool,
}

impl CommandPalette {
    /// The action a command handed to the playback bar, once.
    pub(crate) fn take_forwarded(&mut self) -> Option<PlaybackAction> {
        self.forwarded.take()
    }

    /// Open the palette; `project_dir` is the open project's folder.
    fn show(&mut self, project_dir: Option<&std::path::Path>) {
        self.open = true;
        self.jumping = false;
        self.sheet = false;
        self.query.clear();
        self.highlight = 0;
        self.focus = true;
        self.items = items(
            &gaanim_project::RecentProjects::load().entries(),
            self.bundle,
        );
        if let Some(project_dir) = project_dir {
            add_workspace_items(&mut self.items, project_dir);
        }
    }

    fn show_jumps(&mut self, timeline: &Timeline) {
        self.open = true;
        self.jumping = true;
        self.sheet = false;
        self.query.clear();
        self.focus = true;
        self.items = jump_items(timeline);
        self.highlight = current_jump(&self.items, timeline.current_time);
    }
}

pub(crate) struct CommandPalettePlugin;

impl Plugin for CommandPalettePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CommandPalette>().add_systems(
            bevy_egui::EguiPrimaryContextPass,
            command_palette_system.after(crate::editor_ui_system),
        );
    }
}

const PALETTE_WIDTH: f32 = 560.0;
/// The jump list is wider: its notes sit beside the list.
const JUMP_WIDTH: f32 = 900.0;
const ROWS_SHOWN: usize = 10;
const ROW_HEIGHT: f32 = 34.0;

#[allow(clippy::too_many_arguments)]
fn command_palette_system(
    mut contexts: bevy_egui::EguiContexts,
    mut palette_state: ResMut<CommandPalette>,
    mut settings: ResMut<SettingsWindow>,
    presentation: Res<PresentationMode>,
    hub: Option<ResMut<crate::project_hub::ProjectHubState>>,
    mut timeline: ResMut<Timeline>,
    mut capture: ResMut<FrameCapture>,
    mut overlays: ResMut<EditorOverlays>,
    mut console: ResMut<ConsolePanel>,
    mut animations: ResMut<AnimationTimeline>,
    mut thumbnails: ResMut<crate::presenter::PresenterThumbnailCache>,
    stash: Res<crate::export::StashedReplay>,
    bundle: Option<Res<crate::bundle_player::BundlePlayback>>,
    paths: Option<Res<crate::export::ProjectPaths>>,
) {
    if palette_state.bundle != bundle.is_some() {
        palette_state.bundle = bundle.is_some();
    }
    #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
    let Some(mut hub) = hub else {
        return;
    };
    if presentation.active || hub.active {
        palette_state.open = false;
        palette_state.sheet = false;
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let ctx = ctx.clone();
    let state = &mut *palette_state;

    // Shift first: Ctrl+P alone would also match Ctrl+Shift+P.
    let palette_keys = egui::KeyboardShortcut::new(
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        egui::Key::P,
    );
    let jump_keys = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::P);
    if ctx.input_mut(|input| input.consume_shortcut(&palette_keys)) {
        if state.open && !state.jumping {
            state.open = false;
        } else {
            state.show(paths.as_deref().map(|paths| paths.project_dir.as_path()));
        }
    } else if ctx.input_mut(|input| input.consume_shortcut(&jump_keys)) {
        if state.open && state.jumping {
            state.open = false;
        } else {
            state.show_jumps(&timeline);
        }
    }
    // `?` comes as text, whatever the keyboard layout needs to type it.
    let question = !ctx.egui_wants_keyboard_input()
        && ctx.input(|input| {
            input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Text(text) if text == "?"))
        });
    if question && !state.open {
        state.sheet = !state.sheet;
    }

    let mut chosen = None;
    if state.open {
        let covers = Covers {
            thumbnails: &mut thumbnails,
            timeline: &timeline,
            stash: &stash,
        };
        chosen = palette_ui(&ctx, state, covers);
    }
    if state.sheet {
        sheet_ui(&ctx, state);
    }
    let Some(command) = chosen else {
        return;
    };
    state.open = false;
    match command {
        Command::PlayPause => timeline.is_playing = !timeline.is_playing,
        Command::GoToStart => {
            timeline.is_playing = false;
            timeline.seek_request = Some(0.0);
        }
        Command::GoToEnd => {
            timeline.is_playing = false;
            timeline.seek_request = Some(timeline.cached_duration.max(0.0));
        }
        Command::LoopScene => {
            if let Some(range) = crate::current_scene_loop_range(&timeline) {
                state.forwarded = Some(PlaybackAction::ToggleLoop(range));
            }
        }
        Command::Continuous => state.forwarded = Some(PlaybackAction::ToggleContinuous),
        Command::FullScreen => state.forwarded = Some(PlaybackAction::ToggleFullscreen),
        Command::PinOnTop => state.forwarded = Some(PlaybackAction::TogglePin),
        Command::Prompter => state.forwarded = Some(PlaybackAction::TogglePrompter),
        Command::Narration => state.forwarded = Some(PlaybackAction::ToggleNarration),
        Command::Export => state.forwarded = Some(PlaybackAction::OpenExport),
        Command::Present => state.forwarded = Some(PlaybackAction::Present),
        Command::Overlays => overlays.enabled = !overlays.enabled,
        Command::Animations => animations.open = !animations.open,
        Command::Console => console.open = !console.open,
        Command::SaveFrame => capture.requested = true,
        Command::Settings(section) => settings.show(section),
        Command::Shortcuts => state.sheet = true,
        Command::Jump => state.show_jumps(&timeline),
        Command::Seek(time) => {
            timeline.is_playing = false;
            timeline.seek_request = Some(time);
        }
        Command::Home => {
            #[cfg(not(target_arch = "wasm32"))]
            hub.request_home();
        }
        Command::OpenRecent(path) => {
            #[cfg(not(target_arch = "wasm32"))]
            hub.request_switch(path);
            #[cfg(target_arch = "wasm32")]
            let _ = path;
        }
    }
}

/// The palette's window; returns the command chosen this frame.
/// What the jump list draws its previews from: the cue previews Presenter
/// View and the playback bar's hover card share.
struct Covers<'a> {
    thumbnails: &'a mut crate::presenter::PresenterThumbnailCache,
    timeline: &'a Timeline,
    stash: &'a crate::export::StashedReplay,
}

impl Covers<'_> {
    /// The preview of the frame shown at `time`, asked for first; `None`
    /// until it is rendered.
    fn of(
        &mut self,
        ctx: &egui::Context,
        key: crate::presenter::ThumbnailKey,
        edge: u32,
    ) -> Option<egui::TextureHandle> {
        self.thumbnails.update(
            self.stash,
            self.timeline,
            edge.clamp(96, 640),
            &[key],
            bevy::platform::time::Instant::now(),
        );
        let texture = self.thumbnails.bar_texture(ctx, key);
        if texture.is_none() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        texture
    }
}

fn palette_ui(
    ctx: &egui::Context,
    state: &mut CommandPalette,
    mut covers: Covers,
) -> Option<Command> {
    let mut chosen = None;
    let matches: Vec<Item> = filter(&state.items, &state.query)
        .into_iter()
        .filter(|item| !WEB || !matches!(item.command, Command::Home | Command::OpenRecent(_)))
        .cloned()
        .collect();
    let pointer_moved = ctx.input(|input| input.pointer.delta() != egui::Vec2::ZERO);
    let (up, down, enter) = ctx.input_mut(|input| {
        (
            input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
            input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
            input.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
        )
    });
    if up {
        state.highlight = state.highlight.saturating_sub(1);
    }
    if down {
        state.highlight += 1;
    }
    state.highlight = state.highlight.min(matches.len().saturating_sub(1));
    if enter && let Some(item) = matches.get(state.highlight) {
        chosen = Some(item.command.clone());
    }

    let screen = ctx.viewport_rect();
    let jumping = state.jumping;
    let width = if jumping { JUMP_WIDTH } else { PALETTE_WIDTH }.min(screen.width() - 32.0);
    let list_width = if jumping { width * 0.5 } else { width };
    let preview_width = width - list_width;
    let preview = matches.get(state.highlight).map(|item| {
        let heading = item.heading.clone().unwrap_or_else(|| item.title.clone());
        // By its key, not its time: a segment's entry instant can still
        // belong to the stop that ends the segment before.
        let cover = match (jumping, item.cue) {
            (true, Some(key)) => {
                let edge = ((preview_width - 32.0) * ctx.pixels_per_point()) as u32;
                covers.of(ctx, key, edge)
            }
            _ => None,
        };
        JumpPreview {
            heading,
            time: item.time,
            notes: item.preview.clone(),
            cover,
        }
    });
    let modal = egui::Modal::new(egui::Id::new("gaanim-command-palette"))
        .frame(card_frame().inner_margin(egui::Margin::same(0)))
        .backdrop_color(egui::Color32::from_black_alpha(110))
        .show(ctx, |ui| {
            ui.set_width(width);
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(16, 12))
                .show(ui, |ui| {
                    let before = state.query.clone();
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut state.query)
                            .hint_text(if jumping {
                                "Go to a segment or stop by name"
                            } else {
                                "Type a command, a setting or a recent project"
                            })
                            .desired_width(f32::INFINITY)
                            .frame(egui::Frame::NONE)
                            .font(egui::FontId::proportional(16.0)),
                    );
                    // The modal takes focus as it opens: ask until the field
                    // has it.
                    if state.focus {
                        field.request_focus();
                        state.focus = !field.has_focus();
                    }
                    if state.query != before {
                        state.highlight = 0;
                    }
                });
            ui.painter().rect_filled(
                egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width(), 1.0)),
                0.0,
                palette::DIVIDER,
            );
            ui.add_space(1.0);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.vertical(|ui| {
                    ui.set_width(list_width);
                    // Sized from this frame's matches: a scroll area left to
                    // size itself keeps the height of a short list it showed.
                    let shown = if jumping {
                        ROWS_SHOWN
                    } else {
                        matches.len().clamp(1, ROWS_SHOWN)
                    };
                    let height = ROW_HEIGHT * shown as f32 + 8.0;
                    egui::ScrollArea::vertical()
                        .id_salt(("palette-list", jumping))
                        .max_height(height)
                        .min_scrolled_height(height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.add_space(4.0);
                            ui.spacing_mut().item_spacing.y = 0.0;
                            if matches.is_empty() {
                                ui.add_space(10.0);
                                ui.label(
                                    egui::RichText::new("   Nothing matches")
                                        .size(13.5)
                                        .color(palette::TEXT_FAINT),
                                );
                                ui.add_space(10.0);
                            }
                            for (index, item) in matches.iter().enumerate() {
                                let row = palette_row(ui, item, index == state.highlight);
                                if index == state.highlight && (up || down) {
                                    row.scroll_to_me(None);
                                }
                                // Only a pointer that moves takes the highlight:
                                // one resting on a row would undo the arrows.
                                if row.hovered() && pointer_moved {
                                    state.highlight = index;
                                }
                                if row.clicked() {
                                    chosen = Some(item.command.clone());
                                }
                            }
                            ui.add_space(4.0);
                        });
                });
                if jumping {
                    jump_preview(ui, preview_width, preview);
                }
            });
        });
    if modal.should_close() {
        state.open = false;
    }
    chosen
}

fn palette_row(ui: &mut egui::Ui, item: &Item, highlighted: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ROW_HEIGHT),
        egui::Sense::click(),
    );
    let painter = ui.painter();
    if highlighted {
        painter.rect_filled(rect, 0.0, palette::SELECTED);
        // The current item: gold, like the playhead.
        painter.rect_filled(
            egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())),
            0.0,
            palette::STOP,
        );
    }
    // Right to left: the shortcut or time, then the group, and the title
    // in what is left, cut with an ellipsis rather than over the others.
    let right = rect.max.x - 16.0;
    let right_width = if let Some(shortcut) = item.shortcut {
        key_chip(painter, egui::pos2(right, rect.center().y), shortcut).width()
    } else if let Some(time) = item.time {
        painter
            .text(
                egui::pos2(right, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                crate::format_time(time),
                egui::FontId::monospace(11.5),
                palette::TEXT_FAINT,
            )
            .width()
    } else {
        0.0
    };
    let left = rect.min.x + if item.nested { 32.0 } else { 16.0 };
    let room = (right - right_width - 14.0 - left).max(40.0);
    let group = elided(
        painter,
        &item.group,
        egui::FontId::monospace(11.0),
        palette::TEXT_FAINT,
        room * 0.4,
    );
    let title = elided(
        painter,
        &item.title,
        egui::FontId::proportional(14.0),
        if highlighted {
            palette::TEXT
        } else {
            palette::TEXT_MUTED
        },
        room - group.size().x - 10.0,
    );
    let title_width = title.size().x;
    painter.galley(
        egui::pos2(left, rect.center().y - title.size().y / 2.0),
        title,
        palette::TEXT,
    );
    painter.galley(
        egui::pos2(
            left + title_width + 10.0,
            rect.center().y - group.size().y / 2.0,
        ),
        group,
        palette::TEXT_FAINT,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// `text` on one line of at most `width` points, cut with an ellipsis.
fn elided(
    painter: &egui::Painter,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping {
        max_width: width.max(1.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    painter.layout_job(job)
}

/// Beside the jump list: the highlighted entry's name, time and notes.
/// What the jump list shows of the highlighted entry.
struct JumpPreview {
    heading: String,
    time: Option<f64>,
    notes: Option<String>,
    /// The frame shown there, once rendered.
    cover: Option<egui::TextureHandle>,
}

fn jump_preview(ui: &mut egui::Ui, width: f32, preview: Option<JumpPreview>) {
    let height = ROW_HEIGHT * ROWS_SHOWN as f32 + 8.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, palette::FIELD);
    ui.painter().line_segment(
        [rect.left_top(), rect.left_bottom()],
        egui::Stroke::new(1.0, palette::DIVIDER),
    );
    let Some(JumpPreview {
        heading,
        time,
        notes,
        cover,
    }) = preview
    else {
        return;
    };
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(16.0, 14.0)))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    // The slide as it shows there, 16:9 across the column; a dark stage
    // while it renders.
    let stage_width = child.available_width();
    let (stage, _) = child.allocate_exact_size(
        egui::vec2(stage_width, (stage_width * 9.0 / 16.0).round()),
        egui::Sense::hover(),
    );
    child
        .painter()
        .rect_filled(stage, 0.0, egui::Color32::from_rgb(10, 10, 10));
    match cover {
        Some(texture) => {
            let size = texture.size_vec2();
            let scale = (stage.width() / size.x).min(stage.height() / size.y);
            child.painter().image(
                texture.id(),
                egui::Rect::from_center_size(stage.center(), size * scale),
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        None => {
            child.painter().text(
                stage.center(),
                egui::Align2::CENTER_CENTER,
                "Rendering the preview…",
                egui::FontId::proportional(12.0),
                palette::TEXT_FAINT,
            );
        }
    }
    child.add_space(10.0);
    child.label(
        egui::RichText::new(heading)
            .size(15.0)
            .strong()
            .color(palette::TEXT),
    );
    if let Some(time) = time {
        child.label(
            egui::RichText::new(crate::format_time(time))
                .monospace()
                .size(11.5)
                .color(palette::STOP),
        );
    }
    child.add_space(8.0);
    // Long notes scroll inside the column instead of running below it.
    child.set_clip_rect(rect);
    let room = (rect.max.y - 14.0 - child.cursor().min.y).max(0.0);
    egui::ScrollArea::vertical()
        .id_salt("jump-preview")
        .max_height(room)
        .min_scrolled_height(room)
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            let (text, color) = match notes {
                Some(notes) => (notes, palette::TEXT_MUTED),
                None => ("No notes here.".to_owned(), palette::TEXT_FAINT),
            };
            ui.add(egui::Label::new(egui::RichText::new(text).size(13.5).color(color)).wrap());
        });
}

/// A shortcut as a key cap, right-aligned at `right`.
fn key_chip(painter: &egui::Painter, right: egui::Pos2, keys: &str) -> egui::Rect {
    let galley = painter.layout_no_wrap(
        keys.to_owned(),
        egui::FontId::monospace(11.5),
        palette::TEXT_MUTED,
    );
    let size = galley.size() + egui::vec2(12.0, 6.0);
    let rect =
        egui::Rect::from_min_size(egui::pos2(right.x - size.x, right.y - size.y / 2.0), size);
    painter.rect_filled(rect, 0.0, palette::FIELD);
    painter.rect_stroke(
        rect,
        0.0,
        egui::Stroke::new(1.0, palette::DIVIDER),
        egui::StrokeKind::Inside,
    );
    painter.galley(rect.min + egui::vec2(6.0, 3.0), galley, palette::TEXT_MUTED);
    rect
}

/// Every shortcut, grouped: the commands' and the keys without one.
fn sheet_ui(ctx: &egui::Context, state: &mut CommandPalette) {
    let screen = ctx.viewport_rect();
    let width = 640.0_f32.min(screen.width() - 32.0);
    let mut groups: Vec<(&str, Vec<(&str, &str)>)> = Vec::new();
    let bundle = state.bundle;
    let rows = COMMANDS
        .iter()
        .filter(|spec| offered(spec, bundle))
        .filter_map(|spec| {
            spec.shortcut
                .map(|keys| (spec.group, keys, spec_title(spec, bundle)))
        })
        .chain(
            OTHER_KEYS
                .iter()
                .copied()
                .filter(|row| !bundle || *row != LAYOUT_KEYS),
        );
    for (group, keys, what) in rows {
        match groups.iter_mut().find(|(name, _)| *name == group) {
            Some((_, entries)) => entries.push((keys, what)),
            None => groups.push((group, vec![(keys, what)])),
        }
    }
    let modal = egui::Modal::new(egui::Id::new("gaanim-shortcut-sheet"))
        .frame(card_frame())
        .backdrop_color(egui::Color32::from_black_alpha(110))
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Keyboard shortcuts")
                        .size(18.0)
                        .strong()
                        .color(palette::TEXT),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new("? or Esc to close · Ctrl+Shift+P for every command")
                            .monospace()
                            .size(11.0)
                            .color(palette::TEXT_FAINT),
                    );
                });
            });
            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .max_height((screen.height() - 160.0).max(200.0))
                .show(ui, |ui| {
                    for (group, entries) in &groups {
                        ui.add_space(6.0);
                        ui.label(crate::ui_kit::caption(&group.to_uppercase()));
                        ui.add_space(2.0);
                        for (keys, what) in entries {
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), 28.0),
                                egui::Sense::hover(),
                            );
                            key_chip(
                                ui.painter(),
                                egui::pos2(rect.min.x + 150.0, rect.center().y),
                                keys,
                            );
                            ui.painter().text(
                                egui::pos2(rect.min.x + 166.0, rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                *what,
                                egui::FontId::proportional(13.5),
                                palette::TEXT_MUTED,
                            );
                        }
                    }
                });
        });
    if modal.should_close() {
        state.sheet = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles<'a>(items: &'a [Item], query: &str) -> Vec<&'a str> {
        filter(items, query)
            .into_iter()
            .map(|item| item.title.as_str())
            .collect()
    }

    #[test]
    fn typing_finds_commands_by_their_initials_and_words() {
        let items = items(&[], false);
        assert_eq!(titles(&items, "").len(), items.len());
        assert_eq!(titles(&items, "export")[0], "Export…");
        assert_eq!(titles(&items, "fs")[0], "Full screen");
        assert_eq!(titles(&items, "set pro")[0], "Settings: Prompter");
        assert!(titles(&items, "zzz").is_empty());
    }

    #[test]
    fn a_group_name_finds_its_commands() {
        let items = items(&[], false);
        let found = titles(&items, "playback");
        assert!(found.contains(&"Play / pause"));
        assert!(found.contains(&"Go to end"));
    }

    #[test]
    fn a_bundle_is_offered_no_animations_or_layout() {
        let scene = items(&[], false);
        let recorded = items(&[], true);
        let overlays = |items: &[Item]| {
            items
                .iter()
                .find(|item| item.command == Command::Overlays)
                .map(|item| item.title.clone())
        };
        assert!(scene.iter().any(|item| item.command == Command::Animations));
        assert!(
            recorded
                .iter()
                .all(|item| item.command != Command::Animations)
        );
        assert_eq!(
            overlays(&scene).as_deref(),
            Some("Overlays: bounds, grid, guides, layout")
        );
        assert_eq!(
            overlays(&recorded).as_deref(),
            Some("Overlays: bounds, grid, guides")
        );
    }

    #[test]
    fn scores_prefer_word_starts_and_runs() {
        assert!(score("pre", "Present") > score("pre", "Compress"));
        assert_eq!(score("x", "Console"), None);
        assert_eq!(score("", "anything"), Some(0));
    }

    #[test]
    fn the_jump_list_holds_segments_and_stops_with_what_is_said_there() {
        use gaanim_timeline::timeline::{SegmentMetadata, SegmentStop};
        let mut timeline = Timeline::default();
        timeline.segments = vec![
            SegmentMetadata {
                id: 0,
                name: "intro".to_owned(),
                notes: Some("Hello.\n[stop]\nThe chart.".to_owned()),
                target: None,
                start_time: 0.0,
                end_time: 4.0,
                stops: vec![SegmentStop {
                    name: Some("chart".to_owned()),
                    time: 2.0,
                    ambient: None,
                }],
            },
            SegmentMetadata {
                id: 1,
                name: "results".to_owned(),
                notes: None,
                target: None,
                start_time: 4.0,
                end_time: 9.0,
                stops: vec![],
            },
        ];
        let items = jump_items(&timeline);
        let titles: Vec<&str> = items.iter().map(|item| item.title.as_str()).collect();
        assert_eq!(titles, ["intro", "chart", "results"]);
        assert_eq!(items[1].group, "stop · intro");
        assert!(items[1].nested);
        // The segment's name finds its stops too.
        assert!(
            filter(&items, "intro")
                .iter()
                .any(|item| item.title == "chart")
        );
        assert_eq!(items[0].preview.as_deref(), Some("Hello."));
        assert_eq!(items[1].preview.as_deref(), Some("The chart."));
        assert_eq!(items[1].command, Command::Seek(2.0));
        use crate::presenter::ThumbnailMoment;
        assert_eq!(items[0].cue, Some((0, ThumbnailMoment::Entry)));
        assert_eq!(items[1].cue, Some((0, ThumbnailMoment::Stop(0))));
        assert_eq!(items[2].cue, Some((1, ThumbnailMoment::Entry)));
        assert_eq!(current_jump(&items, 3.0), 1);
        assert_eq!(current_jump(&items, 4.0), 2);
        assert_eq!(filter(&items, "res")[0].title, "results");
    }

    #[test]
    fn every_command_with_a_shortcut_is_on_the_sheet_once() {
        let mut keys: Vec<&str> = COMMANDS.iter().filter_map(|spec| spec.shortcut).collect();
        keys.extend(OTHER_KEYS.iter().map(|(_, keys, _)| *keys));
        let total = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), total, "a shortcut is listed twice");
    }

    #[test]
    fn a_member_offers_the_other_members_and_its_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let write = |relative: &str, text: &str| {
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(
            "gaanim.toml",
            "name = \"canal\"\n[workspace]\nmembers = [{ path = \"shows/*\", group = \"Shows\" }, \"trailer\"]\n",
        );
        for member in ["shows/pilot", "shows/finale", "trailer"] {
            write(&format!("{member}/main.py"), "");
            write(
                &format!("{member}/gaanim.toml"),
                "kind = \"video\"\nentry = \"main.py\"\n",
            );
        }
        let workspace = gaanim_project::resolve_workspace(root).unwrap();
        let pilot = workspace.root.join("shows/pilot");
        // A recent entry for a member gives way to the workspace's own item.
        let mut listed = items(&[], false);
        listed.push(Item {
            title: "Open finale".into(),
            group: "Recent project".into(),
            shortcut: None,
            command: Command::OpenRecent(workspace.root.join("shows/finale")),
            time: None,
            preview: None,
            nested: false,
            heading: None,
            cue: None,
        });
        add_workspace_items(&mut listed, &pilot);
        let offered: Vec<(&str, &str)> = listed
            .iter()
            .filter(|item| matches!(item.command, Command::OpenRecent(_)))
            .map(|item| (item.title.as_str(), item.group.as_str()))
            .collect();
        assert_eq!(
            offered,
            [
                ("Back to canal", "Gaanim"),
                ("Open finale", "canal › Shows"),
                ("Open trailer", "canal"),
            ]
        );
        assert!(!listed.iter().any(|item| item.command == Command::Home));

        // The workspace root itself, or a lone project, changes nothing.
        let mut plain = items(&[], false);
        add_workspace_items(&mut plain, temp.path());
        assert!(plain.iter().any(|item| item.command == Command::Home));
    }
}
