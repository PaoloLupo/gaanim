//! What authoring a scene cost: the reload profile and its warnings.
//!
//! `GAANIM_RELOAD_PROFILE=1` prints, once a scene is rendered, how long the
//! script spent on each segment and what measuring drawables cost, with the
//! script lines that compiled the scene. A costly measurement pattern is
//! reported even without it, and by `gaanim check`.

use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::SceneModel;

/// Whether `GAANIM_RELOAD_PROFILE` asks for the full profile.
pub fn reload_profile_enabled() -> bool {
    std::env::var_os("GAANIM_RELOAD_PROFILE").is_some_and(|value| value != "0")
}

static CALL_SITE: OnceLock<fn() -> Option<String>> = OnceLock::new();

/// Let a binding name the line of its script that made an authoring call,
/// such as `sections/intro.py:42`. Only the first provider counts.
pub fn set_call_site_provider(provider: fn() -> Option<String>) {
    let _ = CALL_SITE.set(provider);
}

fn call_site() -> Option<String> {
    CALL_SITE.get().and_then(|provider| provider())
}

/// Measurements that compiled, and how long they took.
#[derive(Debug, Clone, Copy, Default)]
struct Compiles {
    count: usize,
    time: Duration,
}

impl Compiles {
    fn add(&mut self, time: Duration) {
        self.count += 1;
        self.time += time;
    }
}

/// What authoring one scene cost so far. Copies of a scene share it.
#[derive(Debug)]
pub(crate) struct AuthoringProfile {
    created: Instant,
    /// Each explicit segment and when the script started it.
    segments: Vec<(String, Instant)>,
    /// Declarations measured on their own.
    isolated: Compiles,
    /// Measurements that compiled the scene up to the cursor.
    scene: Compiles,
    /// Measurements that reused an earlier compilation.
    reused: usize,
    /// Where the scene was compiled to measure, in first-call order.
    sites: Vec<(String, Compiles)>,
    /// The slowest measurement and where it was made.
    slowest: Option<(Duration, Option<String>)>,
}

impl Default for AuthoringProfile {
    fn default() -> Self {
        Self {
            created: Instant::now(),
            segments: Vec::new(),
            isolated: Compiles::default(),
            scene: Compiles::default(),
            reused: 0,
            sites: Vec::new(),
            slowest: None,
        }
    }
}

/// Shared by every copy of a scene, so the copy a script renders carries
/// what authoring it cost.
#[derive(Clone, Default)]
pub(crate) struct SharedProfile(Arc<Mutex<AuthoringProfile>>);

impl std::fmt::Debug for SharedProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SharedProfile")
    }
}

/// How a measurement got its compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Measured {
    Isolated,
    Scene,
    Reused,
}

/// Total time of scene compiles from which measuring is worth a warning.
const COSTLY_MEASURING: Duration = Duration::from_millis(500);
/// A single measurement this slow is worth a warning.
const SLOW_MEASUREMENT: Duration = Duration::from_millis(100);

impl SharedProfile {
    fn with<R>(&self, f: impl FnOnce(&mut AuthoringProfile) -> R) -> Option<R> {
        self.0.lock().ok().map(|mut profile| f(&mut profile))
    }

    pub(crate) fn segment_started(&self, name: &str) {
        self.with(|profile| profile.segments.push((name.to_string(), Instant::now())));
    }

    /// Record a measurement that took `time`. Scene compiles and slow
    /// measurements name the line that made them.
    pub(crate) fn measured(&self, how: Measured, time: Duration) {
        let site = (how == Measured::Scene || time >= SLOW_MEASUREMENT)
            .then(call_site)
            .flatten();
        self.with(|profile| {
            match how {
                Measured::Isolated => profile.isolated.add(time),
                Measured::Reused => profile.reused += 1,
                Measured::Scene => {
                    profile.scene.add(time);
                    let label = site.clone().unwrap_or_else(|| "?".to_string());
                    match profile.sites.iter_mut().find(|(at, _)| *at == label) {
                        Some((_, compiles)) => compiles.add(time),
                        None => {
                            let mut compiles = Compiles::default();
                            compiles.add(time);
                            profile.sites.push((label, compiles));
                        }
                    }
                }
            }
            if profile
                .slowest
                .as_ref()
                .is_none_or(|(slowest, _)| time > *slowest)
            {
                profile.slowest = Some((time, site));
            }
        });
    }

    /// A warning when measuring drawables compiled the scene at a cost.
    pub(crate) fn measurement_warning(&self) -> Option<String> {
        self.with(|profile| {
            let slowest = profile
                .slowest
                .as_ref()
                .filter(|(time, _)| *time >= SLOW_MEASUREMENT);
            if profile.scene.time < COSTLY_MEASURING && slowest.is_none() {
                return None;
            }
            let mut sites = profile.sites.clone();
            sites.sort_by_key(|(_, compiles)| std::cmp::Reverse(compiles.time));
            let sites = sites
                .iter()
                .take(3)
                .map(|(site, compiles)| {
                    format!("{site} ({}×, {})", compiles.count, seconds(compiles.time))
                })
                .collect::<Vec<_>>();
            let mut warning = format!(
                "measuring drawables compiled the scene {} time{} ({}); measure before \
                 the first play, measure the boxes you need together, or compute them \
                 from your data",
                profile.scene.count,
                if profile.scene.count == 1 { "" } else { "s" },
                seconds(profile.scene.time),
            );
            if !sites.is_empty() {
                warning.push_str(&format!(": {}", sites.join(", ")));
            }
            if let Some((time, site)) = slowest {
                warning.push_str(&format!("; the slowest took {}", seconds(*time)));
                if let Some(site) = site {
                    warning.push_str(&format!(" at {site}"));
                }
            }
            Some(warning)
        })
        .flatten()
    }

    /// The lines of the full profile, as of `now`.
    pub(crate) fn report(&self, now: Instant) -> Vec<String> {
        self.with(|profile| {
            let mut lines = vec![format!(
                "script {} · measurements: {} isolated ({}), {} compiled the scene ({}), \
                 {} reused",
                seconds(now - profile.created),
                profile.isolated.count,
                seconds(profile.isolated.time),
                profile.scene.count,
                seconds(profile.scene.time),
                profile.reused,
            )];
            let starts = profile.segments.iter().map(|(_, start)| *start);
            let mut segments: Vec<(&str, Duration)> = profile
                .segments
                .iter()
                .zip(starts.skip(1).chain([now]))
                .map(|((name, start), end)| (name.as_str(), end - *start))
                .collect();
            segments.sort_by_key(|(_, time)| std::cmp::Reverse(*time));
            if let Some((_, first)) = profile.segments.first() {
                lines.push(format!(
                    "before the first segment {}",
                    seconds(*first - profile.created)
                ));
            }
            lines.extend(
                segments
                    .iter()
                    .take(5)
                    .map(|(name, time)| format!("segment `{name}` {}", seconds(*time))),
            );
            let mut sites = profile.sites.clone();
            sites.sort_by_key(|(_, compiles)| std::cmp::Reverse(compiles.time));
            lines.extend(sites.iter().take(5).map(|(site, compiles)| {
                format!(
                    "compiled the scene to measure at {site}: {}×, {}",
                    compiles.count,
                    seconds(compiles.time)
                )
            }));
            lines
        })
        .unwrap_or_default()
    }
}

fn seconds(time: Duration) -> String {
    format!("{:.2}s", time.as_secs_f64())
}

impl SceneModel {
    /// A warning when measuring drawables while authoring this scene
    /// compiled it at a cost; see [`SceneModel::bounds_of`].
    pub fn measurement_warning(&self) -> Option<String> {
        self.profile.measurement_warning()
    }

    /// Report what authoring this scene cost: the warning always, and the
    /// whole profile with `GAANIM_RELOAD_PROFILE=1`. The host calls it when
    /// a script hands it a scene.
    pub fn report_authoring(&self) {
        if let Some(warning) = self.measurement_warning() {
            gaanim_core::console::warn("measure", warning);
        }
        if reload_profile_enabled() {
            for line in self.profile.report(Instant::now()) {
                gaanim_core::console::info("profile", line);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn costly_measuring_is_reported_with_its_lines() {
        let profile = SharedProfile::default();
        profile.measured(Measured::Isolated, Duration::from_millis(3));
        profile.measured(Measured::Reused, Duration::from_millis(1));
        assert_eq!(profile.measurement_warning(), None);

        for _ in 0..3 {
            profile.measured(Measured::Scene, Duration::from_millis(200));
        }
        let warning = profile.measurement_warning().unwrap();
        assert!(
            warning.starts_with("measuring drawables compiled the scene 3 times (0.60s)"),
            "{warning}"
        );
        assert!(warning.contains("the slowest took 0.20s"), "{warning}");

        profile.segment_started("intro");
        let report = profile.report(Instant::now());
        assert!(
            report[0].contains("1 isolated") && report[0].contains("3 compiled the scene"),
            "{report:?}"
        );
        assert!(
            report
                .iter()
                .any(|line| line.starts_with("segment `intro`"))
        );
    }
}
