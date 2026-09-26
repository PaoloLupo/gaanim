//! Log output of the Gaanim applications.
//!
//! Engine libraries (Bevy, wgpu, winit, egui, Vello) report adapter details,
//! shader compilation and window-system quirks at `info`/`warn`. None of it is
//! actionable for someone previewing a scene, so only their errors are shown,
//! while Gaanim's own messages keep `info`. `RUST_LOG` still overrides any of
//! this (for example `RUST_LOG=info` shows everything again).

use bevy::log::tracing::{Event, Level as TracingLevel, Subscriber};
use bevy::log::tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
use bevy::log::tracing_subscriber::fmt::{FmtContext, Layer as FmtLayer};
use bevy::log::tracing_subscriber::registry::LookupSpan;
use bevy::log::{Level, LogPlugin};
use gaanim_core::console::{self, Stream};

/// Crates whose warnings are noise for people using Gaanim: they describe the
/// machine (software adapter, no audio device, X11 settings) rather than the
/// scene. Their errors are still shown.
const QUIET_CRATES: &[&str] = &[
    "bevy_audio",
    "bevy_egui",
    "bevy_pbr",
    "bevy_render",
    "bevy_winit",
    "calloop",
    "cosmic_text",
    "naga",
    "symphonia",
    "vello",
    "wgpu",
    "wgpu_core",
    "wgpu_hal",
    "winit",
];

/// Filter directives on top of the default level (`warn`).
pub fn log_filter() -> String {
    std::iter::once("gaanim=info".to_string())
        .chain(QUIET_CRATES.iter().map(|name| format!("{name}=error")))
        .collect::<Vec<_>>()
        .join(",")
}

/// Bevy's log plugin configured for Gaanim: quiet engine crates and the same
/// compact, coloured lines as the rest of the command-line output.
pub fn log_plugin() -> LogPlugin {
    LogPlugin {
        level: Level::WARN,
        filter: log_filter(),
        fmt_layer: |_| {
            Some(Box::new(
                FmtLayer::default()
                    .with_writer(std::io::stderr)
                    .with_ansi(console::color_enabled(Stream::Stderr))
                    .event_format(ConsoleFormat),
            ))
        },
        ..Default::default()
    }
}

/// Formats events like every other status line, naming the crate for messages
/// that do not come from Gaanim.
struct ConsoleFormat;

impl<S, N> FormatEvent<S, N> for ConsoleFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> std::fmt::Result {
        let metadata = event.metadata();
        let mut message = String::new();
        if let Some(krate) = event_scope(metadata.target()) {
            message.push_str(krate);
            message.push_str(": ");
        }
        ctx.format_fields(Writer::new(&mut message), event)?;
        let level = console_level(*metadata.level());
        let line = console::format_line(
            level,
            level_label(level),
            &message,
            writer.has_ansi_escapes(),
        );
        writeln!(writer, "{line}")
    }
}

/// Engine and library messages are labelled by their level; the crate that
/// logged them opens the message.
fn level_label(level: console::Level) -> &'static str {
    match level {
        console::Level::Error => "error",
        console::Level::Warn => "warning",
        console::Level::Debug => "debug",
        console::Level::Info | console::Level::Success => "info",
    }
}

fn console_level(level: TracingLevel) -> console::Level {
    match level {
        TracingLevel::ERROR => console::Level::Error,
        TracingLevel::WARN => console::Level::Warn,
        TracingLevel::INFO => console::Level::Info,
        _ => console::Level::Debug,
    }
}

/// The crate name of a log target, or `None` for Gaanim's own crates.
fn event_scope(target: &str) -> Option<&str> {
    let krate = target.split("::").next().unwrap_or(target);
    (!krate.starts_with("gaanim")).then_some(krate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_crates_only_report_errors() {
        let filter = log_filter();
        assert!(filter.starts_with("gaanim=info,"));
        for name in [
            "wgpu=error",
            "winit=error",
            "bevy_render=error",
            "bevy_audio=error",
        ] {
            assert!(
                filter.split(',').any(|directive| directive == name),
                "{name}"
            );
        }
        assert_eq!(log_plugin().level, Level::WARN);
    }

    #[test]
    fn gaanim_messages_carry_no_scope() {
        assert_eq!(event_scope("gaanim_api::canvas::compile"), None);
        assert_eq!(event_scope("bevy_asset::server"), Some("bevy_asset"));
        assert_eq!(event_scope("wgpu"), Some("wgpu"));
    }
}
