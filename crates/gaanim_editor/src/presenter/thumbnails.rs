//! Asynchronous cue previews for Presenter View.
//!
//! Captures run on a worker thread against a fresh headless world, so
//! generating previews never seeks or mutates the audience's live
//! presentation. Frames stream back one by one: the cues the speaker needs
//! first (current and next) are rendered first, and previews from an older
//! revision stay visible until their replacement arrives.

use bevy::prelude::*;
use bevy_egui::egui;
use crossbeam_channel::{Receiver, TryRecvError, unbounded};
use gaanim_export::prelude::{AspectRatioPreset, ExportConfig, capture_scene_direct_streaming};
use gaanim_timeline::timeline::Timeline;
use std::collections::{HashMap, HashSet};
use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::export::StashedReplay;

/// Growing the presenter window re-renders every cue, so wait until a drag
/// resize has settled before starting that work.
const GROWTH_SETTLE: Duration = Duration::from_millis(400);
/// Bound per-frame texture uploads so a large deck never hitches the cockpit.
const MAX_UPLOADS_PER_FRAME: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ThumbnailMoment {
    Entry,
    Stop(u32),
    Complete,
}

pub(crate) type ThumbnailKey = (u32, ThumbnailMoment);

#[derive(Debug)]
struct ThumbnailPixels {
    key: ThumbnailKey,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

#[derive(Debug)]
struct CachedThumbnail {
    revision: u64,
    generation: u64,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

#[derive(Debug)]
enum WorkerEvent {
    Frame(ThumbnailPixels),
    Finished(Result<(), String>),
}

struct ThumbnailJob {
    revision: u64,
    edge: u32,
    keys: Vec<ThumbnailKey>,
    completed: usize,
    cancel: Arc<AtomicBool>,
    receiver: Receiver<WorkerEvent>,
}

impl ThumbnailJob {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }
}

/// What Presenter View should tell the speaker about cue previews.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PreviewStatus {
    Waiting,
    Rendering { done: usize, total: usize },
    Ready,
    Failed(String),
}

/// egui textures for one presenter window. They are rebuilt from the retained
/// pixels when Presenter View is reopened, without rendering again.
#[derive(Default)]
struct TextureSet {
    camera: Option<Entity>,
    entries: HashMap<ThumbnailKey, (egui::TextureHandle, u64)>,
}

#[derive(Resource, Default)]
pub(crate) struct PresenterThumbnailCache {
    thumbnails: HashMap<ThumbnailKey, CachedThumbnail>,
    job: Option<ThumbnailJob>,
    /// Revision and preview edge whose full set has been rendered.
    complete: Option<(u64, u32)>,
    /// Last automatic request and how often it has been attempted.
    requested: Option<(u64, u32)>,
    attempts: u8,
    pending_growth: Option<(u32, Instant)>,
    next_generation: u64,
    error: Option<String>,
    native_3d: Option<(u64, bool)>,
    textures: TextureSet,
}

impl PresenterThumbnailCache {
    /// Advance the preview pipeline by one frame: collect finished frames,
    /// cancel obsolete work, and start the next render when needed.
    pub(crate) fn update(
        &mut self,
        stash: &StashedReplay,
        timeline: &Timeline,
        desired_edge: u32,
        priority: &[ThumbnailKey],
        now: Instant,
    ) {
        self.poll();
        let Some(canvas) = stash.canvas.as_ref() else {
            return;
        };
        let revision = stash.revision;
        if revision == 0 || timeline.segments.is_empty() {
            return;
        }
        if self.native_3d.is_none_or(|(known, _)| known != revision) {
            self.native_3d = Some((revision, canvas.has_native_3d_content()));
        }

        let edge = self.target_edge(revision, desired_edge, now);
        if let Some(job) = &self.job {
            // Let the running worker observe cancellation and finish before a
            // replacement starts, so GPU captures never overlap.
            if job.revision != revision || job.edge < edge {
                job.cancel.store(true, Ordering::Release);
            }
            return;
        }
        if self.complete == Some((revision, edge)) {
            return;
        }
        if self.requested != Some((revision, edge)) {
            self.requested = Some((revision, edge));
            self.attempts = 0;
        }
        if self.attempts >= 2 {
            return;
        }
        self.attempts += 1;
        self.spawn(canvas.clone(), revision, edge, timeline, priority);
    }

    fn target_edge(&mut self, revision: u64, desired: u32, now: Instant) -> u32 {
        let settled = self
            .job
            .as_ref()
            .filter(|job| job.revision == revision && !job.cancelled())
            .map(|job| job.edge)
            .or_else(|| {
                self.complete
                    .filter(|(complete, _)| *complete == revision)
                    .map(|(_, edge)| edge)
            });
        let Some(settled) = settled else {
            self.pending_growth = None;
            return desired;
        };
        // Downscaling an existing preview is free; only growth re-renders.
        if desired <= settled {
            self.pending_growth = None;
            return settled;
        }
        match self.pending_growth {
            Some((pending, since)) if pending == desired => {
                if now.duration_since(since) >= GROWTH_SETTLE {
                    desired
                } else {
                    settled
                }
            }
            _ => {
                self.pending_growth = Some((desired, now));
                settled
            }
        }
    }

    fn spawn(
        &mut self,
        canvas: gaanim_api::canvas::SceneModel,
        revision: u64,
        edge: u32,
        timeline: &Timeline,
        priority: &[ThumbnailKey],
    ) {
        let (preview_width, preview_height) = canvas.frame.preview_pixel_size();
        let (width, height) = thumbnail_dimensions(preview_width, preview_height, edge);
        let captures = capture_plan(timeline, priority);
        let keys = captures
            .iter()
            .flat_map(|(_, keys)| keys.iter().copied())
            .collect::<Vec<_>>();
        let times = captures.iter().map(|(time, _)| *time).collect::<Vec<_>>();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let (sender, receiver) = unbounded();

        let spawn_result = std::thread::Builder::new()
            .name("gaanim-presenter-thumbnails".to_string())
            .spawn(move || {
                let mut config = ExportConfig::new("presenter-thumbnails.png");
                config.width = width;
                config.height = height;
                config.aspect_ratio = AspectRatioPreset::Custom;
                config.headless = true;
                let mut groups = captures.into_iter().map(|(_, keys)| keys);
                let result = capture_scene_direct_streaming(
                    config,
                    &times,
                    move |world| gaanim_api::runtime::replay_canvas_into(world, canvas),
                    |frame| {
                        let Some(keys) = groups.next() else {
                            return ControlFlow::Break(());
                        };
                        if worker_cancel.load(Ordering::Acquire) {
                            return ControlFlow::Break(());
                        }
                        let last = keys.len().saturating_sub(1);
                        let mut rgba = Some(frame.rgba);
                        for (index, key) in keys.into_iter().enumerate() {
                            let rgba = if index == last {
                                rgba.take().unwrap_or_default()
                            } else {
                                rgba.clone().unwrap_or_default()
                            };
                            let pixels = ThumbnailPixels {
                                key,
                                width: frame.width,
                                height: frame.height,
                                rgba,
                            };
                            if sender.send(WorkerEvent::Frame(pixels)).is_err() {
                                return ControlFlow::Break(());
                            }
                        }
                        ControlFlow::Continue(())
                    },
                )
                .map_err(|error| error.to_string());
                let _ = sender.send(WorkerEvent::Finished(result));
            });

        match spawn_result {
            Ok(_) => {
                self.error = None;
                self.job = Some(ThumbnailJob {
                    revision,
                    edge,
                    keys,
                    completed: 0,
                    cancel,
                    receiver,
                });
            }
            Err(error) => {
                self.error = Some(format!("could not start the preview renderer: {error}"));
            }
        }
    }

    fn poll(&mut self) {
        loop {
            let Some(job) = self.job.as_mut() else {
                return;
            };
            match job.receiver.try_recv() {
                Ok(WorkerEvent::Frame(pixels)) => {
                    if job.cancelled() {
                        continue;
                    }
                    job.completed += 1;
                    let revision = job.revision;
                    self.store(revision, pixels);
                }
                Ok(WorkerEvent::Finished(result)) => {
                    let job = self.job.take().expect("job checked above");
                    self.finish(job, result);
                    return;
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    let job = self.job.take().expect("job checked above");
                    self.finish(
                        job,
                        Err("preview renderer stopped unexpectedly".to_string()),
                    );
                    return;
                }
            }
        }
    }

    fn store(&mut self, revision: u64, pixels: ThumbnailPixels) {
        self.next_generation = self.next_generation.wrapping_add(1).max(1);
        self.thumbnails.insert(
            pixels.key,
            CachedThumbnail {
                revision,
                generation: self.next_generation,
                width: pixels.width,
                height: pixels.height,
                rgba: pixels.rgba,
            },
        );
    }

    fn finish(&mut self, job: ThumbnailJob, result: Result<(), String>) {
        if job.cancelled() {
            return;
        }
        match result {
            Ok(()) => {
                let requested = job.keys.iter().copied().collect::<HashSet<_>>();
                self.thumbnails.retain(|key, _| requested.contains(key));
                self.complete = Some((job.revision, job.edge));
                self.error = None;
            }
            // Frames delivered before the failure stay usable.
            Err(error) => self.error = Some(error),
        }
    }

    /// Forget failed attempts so the next frame starts a fresh render.
    pub(crate) fn retry(&mut self) {
        self.requested = None;
        self.attempts = 0;
        self.complete = None;
        self.error = None;
    }

    pub(crate) fn status(&self, revision: u64) -> PreviewStatus {
        if let Some(job) = self.job.as_ref().filter(|job| !job.cancelled()) {
            return PreviewStatus::Rendering {
                done: job.completed,
                total: job.keys.len(),
            };
        }
        if let Some(error) = &self.error {
            return PreviewStatus::Failed(error.clone());
        }
        if self
            .complete
            .is_some_and(|(complete, _)| complete == revision)
        {
            PreviewStatus::Ready
        } else {
            PreviewStatus::Waiting
        }
    }

    /// Whether the current scene has native 3D objects that the vector-only
    /// preview capture cannot draw.
    pub(crate) fn omits_native_3d(&self, revision: u64) -> bool {
        self.native_3d == Some((revision, true))
    }

    /// Upload new or changed previews into this presenter window's egui context.
    pub(crate) fn sync_textures(
        &mut self,
        ctx: &egui::Context,
        camera: Entity,
        priority: &[ThumbnailKey],
    ) {
        let textures = &mut self.textures;
        if textures.camera != Some(camera) {
            textures.entries.clear();
            textures.camera = Some(camera);
        }
        let thumbnails = &self.thumbnails;
        textures
            .entries
            .retain(|key, _| thumbnails.contains_key(key));

        let pending = priority
            .iter()
            .chain(thumbnails.keys())
            .filter(|key| {
                thumbnails.get(key).is_some_and(|thumbnail| {
                    textures
                        .entries
                        .get(key)
                        .is_none_or(|(_, generation)| *generation != thumbnail.generation)
                })
            })
            .copied()
            .collect::<Vec<_>>();
        let mut uploaded = HashSet::new();
        for key in pending {
            if !uploaded.insert(key) {
                continue;
            }
            if uploaded.len() > MAX_UPLOADS_PER_FRAME {
                ctx.request_repaint();
                break;
            }
            let thumbnail = &thumbnails[&key];
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [thumbnail.width as usize, thumbnail.height as usize],
                &thumbnail.rgba,
            );
            match textures.entries.get_mut(&key) {
                Some((texture, generation)) => {
                    texture.set(image, egui::TextureOptions::LINEAR);
                    *generation = thumbnail.generation;
                }
                None => {
                    let texture = ctx.load_texture(
                        format!("presenter-cue-{}-{:?}", key.0, key.1),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    textures
                        .entries
                        .insert(key, (texture, thumbnail.generation));
                }
            }
        }
    }

    /// The preview texture for `key`, and whether it predates `revision`.
    pub(crate) fn texture(
        &self,
        key: ThumbnailKey,
        revision: u64,
    ) -> Option<(egui::TextureHandle, bool)> {
        let (texture, _) = self.textures.entries.get(&key)?;
        let stale = self
            .thumbnails
            .get(&key)
            .is_some_and(|thumbnail| thumbnail.revision != revision);
        Some((texture.clone(), stale))
    }
}

/// Every cue preview for the timeline, ordered so the `priority` keys render
/// first. Keys sharing one timestamp are captured once.
fn capture_plan(timeline: &Timeline, priority: &[ThumbnailKey]) -> Vec<(f64, Vec<ThumbnailKey>)> {
    let duration = timeline.cached_duration.max(0.0);
    let mut requests = timeline
        .segments
        .iter()
        .flat_map(|segment| {
            let mut cues = Vec::with_capacity(segment.stops.len() + 2);
            cues.push((
                (segment.id, ThumbnailMoment::Entry),
                entry_segment_time(segment.start_time, segment.end_time),
            ));
            cues.extend(segment.stops.iter().enumerate().map(|(index, stop)| {
                ((segment.id, ThumbnailMoment::Stop(index as u32)), stop.time)
            }));
            cues.push((
                (segment.id, ThumbnailMoment::Complete),
                representative_segment_time(segment.start_time, segment.end_time),
            ));
            cues
        })
        // Segment metadata and clip durations accumulate floats in different
        // orders; never ask the capture for a time past the scene end.
        .map(|(key, time)| (key, time.clamp(0.0, duration)))
        .collect::<Vec<_>>();
    let rank = |key: &ThumbnailKey| {
        priority
            .iter()
            .position(|candidate| candidate == key)
            .unwrap_or(priority.len())
    };
    // Stable: non-priority cues keep timeline order.
    requests.sort_by_key(|(key, _)| rank(key));

    let mut plan: Vec<(f64, Vec<ThumbnailKey>)> = Vec::with_capacity(requests.len());
    let mut by_time: HashMap<u64, usize> = HashMap::new();
    for (key, time) in requests {
        match by_time.get(&time.to_bits()) {
            Some(&index) => plan[index].1.push(key),
            None => {
                by_time.insert(time.to_bits(), plan.len());
                plan.push((time, vec![key]));
            }
        }
    }
    plan
}

pub(crate) fn thumbnail_dimensions(
    canvas_width: u32,
    canvas_height: u32,
    max_edge: u32,
) -> (u32, u32) {
    let max_edge = max_edge.max(1) as f64;
    let width = canvas_width.max(1) as f64;
    let height = canvas_height.max(1) as f64;
    let scale = max_edge / width.max(height);
    (
        (width * scale).round().max(1.0) as u32,
        (height * scale).round().max(1.0) as u32,
    )
}

pub(crate) fn desired_thumbnail_edge(viewport_width: f32, pixels_per_point: f32) -> u32 {
    let physical_preview_width = viewport_width.max(1.0) * 0.66 * pixels_per_point.max(1.0);
    let quantized = (physical_preview_width / 160.0).ceil() * 160.0;
    quantized.clamp(960.0, 1600.0) as u32
}

pub(crate) fn representative_segment_time(start_time: f64, end_time: f64) -> f64 {
    if end_time > start_time + 1e-4 {
        (end_time - 1e-4).max(start_time)
    } else {
        start_time
    }
}

pub(crate) fn entry_segment_time(start_time: f64, end_time: f64) -> f64 {
    if end_time > start_time + 2e-4 {
        (start_time + 1e-4).min(end_time - 1e-4)
    } else {
        start_time
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_timeline::timeline::{SegmentMetadata, SegmentStop};

    fn deck() -> Timeline {
        let mut timeline = Timeline::new();
        timeline.cached_duration = 4.0;
        timeline.set_segments(vec![
            SegmentMetadata {
                id: 1,
                name: "intro".into(),
                notes: None,
                start_time: 0.0,
                end_time: 2.0,
                stops: vec![SegmentStop {
                    name: Some("ready".into()),
                    time: 2.0,
                }],
            },
            SegmentMetadata {
                id: 2,
                name: "closing".into(),
                notes: None,
                start_time: 2.0,
                end_time: 4.0 + 4.0 * f64::EPSILON,
                stops: vec![SegmentStop {
                    name: None,
                    // A terminal stop accumulated past the clip duration.
                    time: 4.0 + 4.0 * f64::EPSILON,
                }],
            },
        ]);
        timeline
    }

    fn job(revision: u64, edge: u32) -> (ThumbnailJob, crossbeam_channel::Sender<WorkerEvent>) {
        let (sender, receiver) = unbounded();
        (
            ThumbnailJob {
                revision,
                edge,
                keys: vec![(1, ThumbnailMoment::Entry), (1, ThumbnailMoment::Stop(0))],
                completed: 0,
                cancel: Arc::new(AtomicBool::new(false)),
                receiver,
            },
            sender,
        )
    }

    fn pixels(key: ThumbnailKey) -> WorkerEvent {
        WorkerEvent::Frame(ThumbnailPixels {
            key,
            width: 2,
            height: 1,
            rgba: vec![0; 8],
        })
    }

    #[test]
    fn capture_plan_never_exceeds_the_scene_duration() {
        let plan = capture_plan(&deck(), &[]);

        assert!(plan.iter().all(|(time, _)| *time <= 4.0));
        let terminal = plan
            .iter()
            .find(|(_, keys)| keys.contains(&(2, ThumbnailMoment::Stop(0))))
            .unwrap();
        assert_eq!(terminal.0, 4.0);
    }

    #[test]
    fn capture_plan_renders_priority_cues_first_and_shares_equal_times() {
        let priority = [(2, ThumbnailMoment::Stop(0)), (1, ThumbnailMoment::Entry)];
        let plan = capture_plan(&deck(), &priority);

        assert_eq!(plan[0].1[0], (2, ThumbnailMoment::Stop(0)));
        assert_eq!(plan[1].1, vec![(1, ThumbnailMoment::Entry)]);
        let keys = plan.iter().map(|(_, keys)| keys.len()).sum::<usize>();
        assert_eq!(keys, 6);
        let mut times = plan
            .iter()
            .map(|(time, _)| time.to_bits())
            .collect::<Vec<_>>();
        times.sort_unstable();
        times.dedup();
        assert_eq!(times.len(), plan.len());
    }

    #[test]
    fn streamed_frames_are_usable_before_the_render_finishes() {
        let mut cache = PresenterThumbnailCache::default();
        let (job, sender) = job(3, 960);
        cache.job = Some(job);
        sender.send(pixels((1, ThumbnailMoment::Entry))).unwrap();

        cache.poll();

        assert!(cache.thumbnails.contains_key(&(1, ThumbnailMoment::Entry)));
        assert_eq!(
            cache.status(3),
            PreviewStatus::Rendering { done: 1, total: 2 }
        );
        sender.send(pixels((1, ThumbnailMoment::Stop(0)))).unwrap();
        sender.send(WorkerEvent::Finished(Ok(()))).unwrap();
        cache.poll();
        assert_eq!(cache.status(3), PreviewStatus::Ready);
        assert!(cache.job.is_none());
    }

    #[test]
    fn a_new_revision_cancels_the_running_worker_before_starting_another() {
        let mut cache = PresenterThumbnailCache::default();
        let (job, sender) = job(1, 960);
        let cancel = job.cancel.clone();
        cache.job = Some(job);
        let stash = StashedReplay {
            canvas: Some(gaanim_api::canvas::SceneModel::new(1920, 1080)),
            revision: 2,
        };

        cache.update(&stash, &deck(), 960, &[], Instant::now());

        assert!(cancel.load(Ordering::Acquire));
        assert_eq!(cache.job.as_ref().map(|job| job.revision), Some(1));
        // Frames and results from the cancelled worker are ignored.
        sender.send(pixels((1, ThumbnailMoment::Entry))).unwrap();
        sender.send(WorkerEvent::Finished(Ok(()))).unwrap();
        cache.poll();
        assert!(cache.thumbnails.is_empty());
        assert!(cache.complete.is_none());
        assert!(cache.job.is_none());
    }

    #[test]
    fn previous_revision_previews_stay_visible_until_replaced() {
        let mut cache = PresenterThumbnailCache::default();
        cache.store(
            1,
            ThumbnailPixels {
                key: (1, ThumbnailMoment::Entry),
                width: 2,
                height: 1,
                rgba: vec![0; 8],
            },
        );
        let (job, sender) = job(2, 960);
        cache.job = Some(job);

        cache.poll();
        assert_eq!(cache.thumbnails[&(1, ThumbnailMoment::Entry)].revision, 1);
        sender.send(pixels((1, ThumbnailMoment::Entry))).unwrap();
        cache.poll();
        assert_eq!(cache.thumbnails[&(1, ThumbnailMoment::Entry)].revision, 2);
    }

    #[test]
    fn completed_render_drops_cues_that_no_longer_exist() {
        let mut cache = PresenterThumbnailCache::default();
        cache.store(
            1,
            ThumbnailPixels {
                key: (9, ThumbnailMoment::Complete),
                width: 2,
                height: 1,
                rgba: vec![0; 8],
            },
        );
        let (job, sender) = job(2, 960);
        cache.job = Some(job);
        sender.send(WorkerEvent::Finished(Ok(()))).unwrap();

        cache.poll();

        assert!(cache.thumbnails.is_empty());
    }

    #[test]
    fn failed_render_keeps_delivered_frames_and_can_be_retried() {
        let mut cache = PresenterThumbnailCache {
            requested: Some((4, 960)),
            attempts: 2,
            ..default()
        };
        let (job, sender) = job(4, 960);
        cache.job = Some(job);
        sender.send(pixels((1, ThumbnailMoment::Entry))).unwrap();
        sender
            .send(WorkerEvent::Finished(Err("adapter lost".into())))
            .unwrap();

        cache.poll();

        assert_eq!(
            cache.status(4),
            PreviewStatus::Failed("adapter lost".into())
        );
        assert_eq!(cache.thumbnails.len(), 1);
        cache.retry();
        assert_eq!(cache.attempts, 0);
        assert!(cache.requested.is_none());
        assert_eq!(cache.status(4), PreviewStatus::Waiting);
    }

    #[test]
    fn crashed_worker_reports_an_error_instead_of_waiting_forever() {
        let mut cache = PresenterThumbnailCache::default();
        let (job, sender) = job(1, 960);
        cache.job = Some(job);
        drop(sender);

        cache.poll();

        assert!(matches!(cache.status(1), PreviewStatus::Failed(_)));
    }

    #[test]
    fn shrinking_reuses_previews_and_growth_waits_for_resize_to_settle() {
        let mut cache = PresenterThumbnailCache {
            complete: Some((5, 1280)),
            ..default()
        };
        let start = Instant::now();

        assert_eq!(cache.target_edge(5, 960, start), 1280);
        assert_eq!(cache.target_edge(5, 1600, start), 1280);
        assert_eq!(
            cache.target_edge(5, 1600, start + Duration::from_millis(100)),
            1280
        );
        assert_eq!(cache.target_edge(5, 1600, start + GROWTH_SETTLE), 1600);
        // A new revision renders immediately at the requested size.
        assert_eq!(cache.target_edge(6, 960, start), 960);
    }

    #[test]
    fn thumbnail_dimensions_preserve_common_aspect_ratios() {
        assert_eq!(thumbnail_dimensions(1920, 1080, 960), (960, 540));
        assert_eq!(thumbnail_dimensions(1080, 1920, 960), (540, 960));
        assert_eq!(thumbnail_dimensions(0, 0, 960), (960, 960));
    }

    #[test]
    fn thumbnail_resolution_adapts_to_viewport_and_dpi() {
        assert_eq!(desired_thumbnail_edge(1180.0, 1.0), 960);
        assert_eq!(desired_thumbnail_edge(1180.0, 2.0), 1600);
        assert_eq!(desired_thumbnail_edge(4000.0, 2.0), 1600);
    }

    #[test]
    fn representative_time_stays_inside_the_segment() {
        assert_eq!(representative_segment_time(2.0, 2.0), 2.0);
        let time = representative_segment_time(2.0, 5.0);
        assert!((2.0..5.0).contains(&time));
    }

    #[test]
    fn entry_time_stays_inside_the_segment() {
        assert_eq!(entry_segment_time(2.0, 2.0), 2.0);
        let time = entry_segment_time(2.0, 5.0);
        assert!(time > 2.0 && time < 5.0);
    }
}
