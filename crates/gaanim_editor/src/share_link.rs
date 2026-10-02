//! Links to a moment of a presentation, as the web player shares them: the
//! fragment of `…/reproductor/?src=…#segmento=5&pausa=2` (or `#t=83.5`).
//!
//! - `segmento`: a segment by its 1-based number or its name.
//! - `pausa`: a 1-based stop: within `segmento` when both are given,
//!   otherwise counted over the whole presentation.
//! - `t`: seconds from the start, used when neither of the above is.
//!
//! The page handles `present` itself, since only a click may open the
//! Presenter View window.

use bevy::prelude::*;
use gaanim_timeline::timeline::Timeline;

/// Resource: where a link asks playback to start, applied once the bundle
/// has opened.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct LinkTarget {
    pub segment: Option<String>,
    pub stop: Option<usize>,
    pub time: Option<f64>,
}

/// Where a link lands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Landing {
    pub time: f64,
    /// Rest there, as on a stop, instead of playing on.
    pub rest: bool,
}

impl LinkTarget {
    /// Parse a URL fragment, with or without its `#`. Unknown keys are
    /// ignored; `None` when it names no moment.
    pub fn parse(fragment: &str) -> Option<Self> {
        let mut target = Self::default();
        for pair in fragment.trim_start_matches('#').split('&') {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            let value = percent_decode(value);
            match percent_decode(key).as_str() {
                "segmento" | "segment" if !value.is_empty() => target.segment = Some(value),
                "pausa" | "stop" => target.stop = value.parse().ok().filter(|stop| *stop > 0),
                "t" => target.time = parse_seconds(&value),
                _ => {}
            }
        }
        (target != Self::default()).then_some(target)
    }

    /// Where the link lands in `timeline`, or why it cannot.
    pub fn resolve(&self, timeline: &Timeline) -> Result<Landing, String> {
        let segment = match &self.segment {
            None => None,
            Some(name) => {
                let by_number = name
                    .parse::<usize>()
                    .ok()
                    .and_then(|number| timeline.segments.get(number.checked_sub(1)?));
                let found = by_number.or_else(|| {
                    timeline
                        .segments
                        .iter()
                        .find(|segment| segment.name == *name)
                        .or_else(|| {
                            let name = name.to_lowercase();
                            timeline
                                .segments
                                .iter()
                                .find(|segment| segment.name.to_lowercase() == name)
                        })
                });
                Some(found.ok_or_else(|| {
                    format!(
                        "El enlace apunta al segmento «{name}», que esta presentación no tiene."
                    )
                })?)
            }
        };
        match (segment, self.stop) {
            (Some(segment), Some(stop)) => segment
                .stops
                .get(stop - 1)
                .map(|stop| Landing {
                    time: stop.time,
                    rest: true,
                })
                .ok_or_else(|| {
                    format!(
                        "El enlace apunta a la pausa {stop} del segmento «{}», que tiene {}.",
                        segment.name,
                        segment.stops.len()
                    )
                }),
            (Some(segment), None) => Ok(Landing {
                time: segment.start_time,
                rest: false,
            }),
            (None, Some(stop)) => {
                let stops = stop_times(timeline);
                stops
                    .get(stop - 1)
                    .map(|time| Landing {
                        time: *time,
                        rest: true,
                    })
                    .ok_or_else(|| {
                        format!(
                            "El enlace apunta a la pausa {stop}, pero esta presentación tiene {}.",
                            stops.len()
                        )
                    })
            }
            (None, None) => Ok(Landing {
                time: self
                    .time
                    .unwrap_or(0.0)
                    .clamp(0.0, timeline.cached_duration),
                rest: false,
            }),
        }
    }

    /// Move `timeline` to where the link lands.
    pub fn apply(&self, timeline: &mut Timeline) -> Result<(), String> {
        let landing = self.resolve(timeline)?;
        if landing.rest {
            timeline.rest_at(landing.time);
        } else {
            timeline.seek_request = Some(landing.time);
        }
        Ok(())
    }
}

/// Every stop of the presentation, in order, as `pausa` counts them.
fn stop_times(timeline: &Timeline) -> Vec<f64> {
    let mut times: Vec<f64> = timeline
        .segments
        .iter()
        .flat_map(|segment| segment.stops.iter().map(|stop| stop.time))
        .collect();
    times.sort_by(f64::total_cmp);
    times.dedup_by(|a, b| (*a - *b).abs() < 1e-5);
    times
}

/// The fragment of a link to what `timeline` shows now: the stop it rests
/// on, or the playhead's time. Empty at the start.
pub fn fragment_for(timeline: &Timeline) -> String {
    if let Some(time) = timeline.resting_stop() {
        let at_stop =
            |stop: &gaanim_timeline::timeline::SegmentStop| (stop.time - time).abs() < 1e-5;
        // A stop that ends one segment and starts the next belongs to the
        // segment shown there.
        let shown = timeline
            .segment_position_at(time)
            .map(|position| position.segment_id);
        let segment = timeline
            .segments
            .iter()
            .filter(|segment| segment.stops.iter().any(at_stop))
            .max_by_key(|segment| Some(segment.id) == shown);
        if let Some(segment) = segment
            && let Some(stop) = segment.stops.iter().position(at_stop)
        {
            let number = timeline
                .segments
                .iter()
                .position(|candidate| candidate.id == segment.id)
                .unwrap_or(0)
                + 1;
            let name = if is_link_safe(&segment.name) && !segment.name.is_empty() {
                percent_encode(&segment.name)
            } else {
                number.to_string()
            };
            return format!("segmento={name}&pausa={}", stop + 1);
        }
    }
    let time = timeline.seek_request.unwrap_or(timeline.current_time);
    if time <= 0.0 {
        return String::new();
    }
    let text = format!("{time:.2}");
    format!("t={}", text.trim_end_matches('0').trim_end_matches('.'))
}

/// Whether a segment name reads back as itself: not a number, which
/// `segmento` would take as a position.
fn is_link_safe(name: &str) -> bool {
    name.parse::<usize>().is_err()
}

fn parse_seconds(value: &str) -> Option<f64> {
    let value = value.trim_end_matches('s');
    // `1:23.5` as well as `83.5`.
    let seconds = match value.split_once(':') {
        Some((minutes, seconds)) => {
            minutes.parse::<f64>().ok()? * 60.0 + seconds.parse::<f64>().ok()?
        }
        None => value.parse().ok()?,
    };
    (seconds.is_finite() && seconds >= 0.0).then_some(seconds)
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        let hex = |at: usize| {
            bytes
                .get(at)
                .and_then(|digit| char::from(*digit).to_digit(16))
        };
        match (byte, hex(index + 1), hex(index + 2)) {
            (b'%', Some(high), Some(low)) => {
                decoded.push((high * 16 + low) as u8);
                index += 3;
            }
            (b'+', ..) => {
                decoded.push(b' ');
                index += 1;
            }
            _ => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn percent_encode(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// System: move the playhead where the link points once a bundle's
/// timeline exists.
pub(crate) fn apply_link_target_system(
    mut commands: Commands,
    target: Option<Res<LinkTarget>>,
    timeline: Option<ResMut<Timeline>>,
    page: Option<Res<crate::host::WebPage>>,
) {
    let (Some(target), Some(mut timeline)) = (target, timeline) else {
        return;
    };
    commands.remove_resource::<LinkTarget>();
    if let Err(message) = target.apply(&mut timeline) {
        gaanim_core::console::warn("link", message.clone());
        if let Some(page) = page {
            (page.notify)(&message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_timeline::timeline::{SegmentMetadata, SegmentStop};

    fn stop(time: f64) -> SegmentStop {
        SegmentStop {
            name: None,
            time,
            ambient: None,
        }
    }

    fn presentation() -> Timeline {
        let mut timeline = Timeline::default();
        timeline.set_segments(vec![
            SegmentMetadata {
                id: 0,
                name: "intro".into(),
                start_time: 0.0,
                end_time: 4.0,
                notes: None,
                stops: vec![stop(1.0), stop(4.0)],
            },
            SegmentMetadata {
                id: 1,
                name: "Resultados finales".into(),
                start_time: 4.0,
                end_time: 9.0,
                notes: None,
                stops: vec![stop(6.0), stop(9.0)],
            },
        ]);
        timeline.cached_duration = 9.0;
        timeline
    }

    fn land(fragment: &str) -> Result<Landing, String> {
        LinkTarget::parse(fragment)
            .expect("names a moment")
            .resolve(&presentation())
    }

    #[test]
    fn segments_by_number_or_name_and_stops_within_or_overall() {
        let at = |time, rest| Ok(Landing { time, rest });
        assert_eq!(land("#segmento=2"), at(4.0, false));
        assert_eq!(land("segmento=Resultados%20finales"), at(4.0, false));
        assert_eq!(land("segmento=resultados+finales&pausa=1"), at(6.0, true));
        assert_eq!(land("pausa=3"), at(6.0, true));
        assert_eq!(land("t=1:30"), at(9.0, false));
        assert_eq!(land("t=2.5&present"), at(2.5, false));
        assert!(land("segmento=cierre").unwrap_err().contains("«cierre»"));
        assert!(land("segmento=1&pausa=5").is_err());
        assert!(land("pausa=9").is_err());
        assert_eq!(LinkTarget::parse("present"), None);
        assert_eq!(LinkTarget::parse(""), None);
    }

    #[test]
    fn the_fragment_names_the_stop_shown_or_the_time() {
        let mut timeline = presentation();
        assert_eq!(fragment_for(&timeline), "");

        timeline.current_time = 2.25;
        timeline.is_playing = true;
        assert_eq!(fragment_for(&timeline), "t=2.25");

        // The stop that ends "intro" also starts the next segment's range.
        timeline.current_time = 4.0;
        timeline.is_playing = false;
        assert_eq!(fragment_for(&timeline), "segmento=intro&pausa=2");

        timeline.current_time = 6.0;
        let fragment = fragment_for(&timeline);
        assert_eq!(fragment, "segmento=Resultados%20finales&pausa=1");
        assert_eq!(
            LinkTarget::parse(&fragment).unwrap().resolve(&timeline),
            Ok(Landing {
                time: 6.0,
                rest: true
            })
        );
    }
}
