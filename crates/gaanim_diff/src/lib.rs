//! Exact seek capture and deterministic visual regression reporting for gaanim.

mod model;
#[cfg(feature = "gui")]
pub mod viewer;

pub use model::{
    DiffReport, FrameDiff, FrameStatus, MANIFEST_FILE, REPORT_FILE, SnapshotEntry, SnapshotManifest,
};

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use gaanim_api::canvas::Canvas;
use gaanim_export::prelude::{
    AspectRatioPreset, ExportConfig, ExportError, capture_scene_direct, capture_scene_hybrid,
};
use image::{DynamicImage, ImageEncoder, Rgba, RgbaImage};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DiffError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("PNG error: {0}")]
    Image(#[from] image::ImageError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("render error: {0}")]
    Export(#[from] ExportError),
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

pub type Result<T> = std::result::Result<T, DiffError>;

#[derive(Debug, Clone, Copy)]
pub struct CompareOptions {
    /// Per-channel differences at or below this value are ignored.
    pub pixel_threshold: u8,
    /// Maximum fraction of pixels above the threshold before a frame fails.
    pub max_changed_ratio: f64,
}

impl Default for CompareOptions {
    fn default() -> Self {
        Self {
            pixel_threshold: 2,
            max_changed_ratio: 0.0,
        }
    }
}

/// Capture exact timeline timestamps into PNG files plus a stable manifest.
pub fn capture_canvas(
    canvas: Canvas,
    output_dir: impl AsRef<Path>,
    times: &[f64],
) -> Result<SnapshotManifest> {
    if times.is_empty() {
        return Err(DiffError::InvalidInput(
            "at least one seek timestamp is required".to_string(),
        ));
    }

    let output_dir = output_dir.as_ref();
    fs::create_dir_all(output_dir)?;

    let mut config = ExportConfig::new("snapshots.png");
    config.width = canvas.width;
    config.height = canvas.height;
    config.aspect_ratio = AspectRatioPreset::Custom;
    config.headless = true;

    let width = config.width;
    let height = config.height;
    let frames = if canvas.has_native_3d_content() {
        capture_scene_hybrid(config, times, move |world| {
            gaanim_api::runtime::replay_canvas_into(world, canvas)
        })?
    } else {
        capture_scene_direct(config, times, move |world| {
            gaanim_api::runtime::replay_canvas_into(world, canvas)
        })?
    };

    let mut snapshots = Vec::with_capacity(frames.len());
    for (index, frame) in frames.into_iter().enumerate() {
        let id = format!("seek_{index:04}_t_{}", time_slug(frame.time));
        let file = format!("{id}.png");
        let path = output_dir.join(&file);
        let encoder = image::codecs::png::PngEncoder::new(fs::File::create(path)?);
        encoder.write_image(
            &frame.rgba,
            frame.width,
            frame.height,
            image::ExtendedColorType::Rgba8,
        )?;
        snapshots.push(SnapshotEntry {
            id,
            time_seconds: frame.time,
            file,
        });
    }

    let manifest = SnapshotManifest {
        schema_version: 1,
        width,
        height,
        snapshots,
    };
    fs::write(
        output_dir.join(MANIFEST_FILE),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(manifest)
}

/// Compare two snapshot directories and write a portable HTML + JSON report.
pub fn compare_directories(
    baseline_dir: impl AsRef<Path>,
    current_dir: impl AsRef<Path>,
    output_dir: impl AsRef<Path>,
    options: CompareOptions,
) -> Result<DiffReport> {
    if !(0.0..=1.0).contains(&options.max_changed_ratio) {
        return Err(DiffError::InvalidInput(
            "max_changed_ratio must be between 0 and 1".to_string(),
        ));
    }

    let baseline_dir = baseline_dir.as_ref();
    let current_dir = current_dir.as_ref();
    let output_dir = output_dir.as_ref();
    let baseline = load_snapshot_set(baseline_dir)?;
    let current = load_snapshot_set(current_dir)?;

    let baseline_assets = output_dir.join("assets").join("baseline");
    let current_assets = output_dir.join("assets").join("current");
    let diff_assets = output_dir.join("assets").join("diff");
    fs::create_dir_all(&baseline_assets)?;
    fs::create_dir_all(&current_assets)?;
    fs::create_dir_all(&diff_assets)?;

    let ids: BTreeSet<_> = baseline.keys().chain(current.keys()).cloned().collect();
    let mut frames = Vec::with_capacity(ids.len());

    for (index, id) in ids.into_iter().enumerate() {
        let baseline_entry = baseline.get(&id);
        let current_entry = current.get(&id);
        let asset_name = format!("{index:04}_{}.png", file_slug(&id));

        match (baseline_entry, current_entry) {
            (Some(reference), Some(candidate)) => {
                let reference_image = image::open(&reference.path)?;
                let candidate_image = image::open(&candidate.path)?;
                fs::copy(&reference.path, baseline_assets.join(&asset_name))?;
                fs::copy(&candidate.path, current_assets.join(&asset_name))?;

                let (mut frame, diff_image) = compare_images(
                    &id,
                    candidate.time.or(reference.time),
                    &reference_image,
                    &candidate_image,
                    options,
                );
                diff_image
                    .save_with_format(diff_assets.join(&asset_name), image::ImageFormat::Png)?;
                frame.baseline_file = Some(web_path("assets/baseline", &asset_name));
                frame.current_file = Some(web_path("assets/current", &asset_name));
                frame.diff_file = Some(web_path("assets/diff", &asset_name));
                frames.push(frame);
            }
            (Some(reference), None) => {
                fs::copy(&reference.path, baseline_assets.join(&asset_name))?;
                let size = image::image_dimensions(&reference.path)?;
                frames.push(FrameDiff {
                    id,
                    time_seconds: reference.time,
                    status: FrameStatus::MissingCurrent,
                    baseline_file: Some(web_path("assets/baseline", &asset_name)),
                    current_file: None,
                    diff_file: None,
                    baseline_size: Some([size.0, size.1]),
                    current_size: None,
                    changed_pixels: 0,
                    total_pixels: u64::from(size.0) * u64::from(size.1),
                    changed_ratio: 1.0,
                    mean_absolute_error: 1.0,
                    max_channel_delta: 255,
                    change_bounds: None,
                });
            }
            (None, Some(candidate)) => {
                fs::copy(&candidate.path, current_assets.join(&asset_name))?;
                let size = image::image_dimensions(&candidate.path)?;
                frames.push(FrameDiff {
                    id,
                    time_seconds: candidate.time,
                    status: FrameStatus::MissingBaseline,
                    baseline_file: None,
                    current_file: Some(web_path("assets/current", &asset_name)),
                    diff_file: None,
                    baseline_size: None,
                    current_size: Some([size.0, size.1]),
                    changed_pixels: 0,
                    total_pixels: u64::from(size.0) * u64::from(size.1),
                    changed_ratio: 1.0,
                    mean_absolute_error: 1.0,
                    max_channel_delta: 255,
                    change_bounds: None,
                });
            }
            (None, None) => unreachable!(),
        }
    }

    let missing = frames
        .iter()
        .filter(|frame| {
            matches!(
                frame.status,
                FrameStatus::MissingBaseline | FrameStatus::MissingCurrent
            )
        })
        .count();
    let changed = frames
        .iter()
        .filter(|frame| {
            matches!(
                frame.status,
                FrameStatus::Changed | FrameStatus::DimensionMismatch
            )
        })
        .count();
    let passed = frames.iter().all(|frame| !frame.status.is_failure());
    let report = DiffReport {
        schema_version: 1,
        passed,
        pixel_threshold: options.pixel_threshold,
        max_changed_ratio: options.max_changed_ratio,
        compared: frames.len().saturating_sub(missing),
        changed,
        missing,
        frames,
    };

    fs::write(
        output_dir.join(REPORT_FILE),
        serde_json::to_vec_pretty(&report)?,
    )?;
    fs::write(output_dir.join("index.html"), render_html(&report)?)?;
    Ok(report)
}

#[derive(Debug)]
struct InputSnapshot {
    time: Option<f64>,
    path: PathBuf,
}

fn load_snapshot_set(directory: &Path) -> Result<BTreeMap<String, InputSnapshot>> {
    let manifest_path = directory.join(MANIFEST_FILE);
    if manifest_path.exists() {
        let manifest: SnapshotManifest = serde_json::from_slice(&fs::read(manifest_path)?)?;
        let mut result = BTreeMap::new();
        for entry in manifest.snapshots {
            let relative = Path::new(&entry.file);
            if relative.is_absolute()
                || relative
                    .components()
                    .any(|part| matches!(part, std::path::Component::ParentDir))
            {
                return Err(DiffError::InvalidInput(format!(
                    "manifest contains an unsafe path: {}",
                    entry.file
                )));
            }
            let path = directory.join(relative);
            if !path.is_file() {
                return Err(DiffError::InvalidInput(format!(
                    "snapshot file does not exist: {}",
                    path.display()
                )));
            }
            result.insert(
                entry.id,
                InputSnapshot {
                    time: Some(entry.time_seconds),
                    path,
                },
            );
        }
        return Ok(result);
    }

    let mut result = BTreeMap::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
        {
            let id = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or_else(|| {
                    DiffError::InvalidInput(format!("non-UTF-8 snapshot path: {}", path.display()))
                })?
                .to_string();
            result.insert(id, InputSnapshot { time: None, path });
        }
    }
    if result.is_empty() {
        return Err(DiffError::InvalidInput(format!(
            "no {MANIFEST_FILE} or PNG files found in {}",
            directory.display()
        )));
    }
    Ok(result)
}

fn compare_images(
    id: &str,
    time: Option<f64>,
    baseline: &DynamicImage,
    current: &DynamicImage,
    options: CompareOptions,
) -> (FrameDiff, RgbaImage) {
    let baseline = baseline.to_rgba8();
    let current = current.to_rgba8();
    let baseline_size = [baseline.width(), baseline.height()];
    let current_size = [current.width(), current.height()];
    let width = baseline.width().max(current.width());
    let height = baseline.height().max(current.height());
    let total_pixels = u64::from(width) * u64::from(height);
    let mut changed_pixels = 0_u64;
    let mut sum_delta = 0_u64;
    let mut max_channel_delta = 0_u8;
    let mut bounds: Option<[u32; 4]> = None;
    let mut diff = RgbaImage::new(width, height);

    for y in 0..height {
        for x in 0..width {
            let reference = pixel_at(&baseline, x, y);
            let candidate = pixel_at(&current, x, y);
            let mut pixel_delta = 0_u8;
            for channel in 0..4 {
                let delta = reference[channel].abs_diff(candidate[channel]);
                sum_delta += u64::from(delta);
                pixel_delta = pixel_delta.max(delta);
                max_channel_delta = max_channel_delta.max(delta);
            }

            if pixel_delta > options.pixel_threshold {
                changed_pixels += 1;
                extend_bounds(&mut bounds, x, y);
                let glow = 80_u8.saturating_add(pixel_delta.saturating_mul(2) / 3);
                diff.put_pixel(x, y, Rgba([255, glow / 4, glow, 255]));
            } else {
                let luma = ((u16::from(reference[0]) * 54
                    + u16::from(reference[1]) * 183
                    + u16::from(reference[2]) * 19)
                    / 256) as u8;
                let muted = luma / 4;
                diff.put_pixel(x, y, Rgba([muted, muted, muted, 255]));
            }
        }
    }

    let changed_ratio = if total_pixels == 0 {
        0.0
    } else {
        changed_pixels as f64 / total_pixels as f64
    };
    let mean_absolute_error = if total_pixels == 0 {
        0.0
    } else {
        sum_delta as f64 / (total_pixels as f64 * 4.0 * 255.0)
    };
    let status = if baseline_size != current_size {
        FrameStatus::DimensionMismatch
    } else if changed_ratio > options.max_changed_ratio {
        FrameStatus::Changed
    } else {
        FrameStatus::Unchanged
    };

    (
        FrameDiff {
            id: id.to_string(),
            time_seconds: time,
            status,
            baseline_file: None,
            current_file: None,
            diff_file: None,
            baseline_size: Some(baseline_size),
            current_size: Some(current_size),
            changed_pixels,
            total_pixels,
            changed_ratio,
            mean_absolute_error,
            max_channel_delta,
            change_bounds: bounds,
        },
        diff,
    )
}

fn pixel_at(image: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
    if x < image.width() && y < image.height() {
        image.get_pixel(x, y).0
    } else {
        [0, 0, 0, 0]
    }
}

fn extend_bounds(bounds: &mut Option<[u32; 4]>, x: u32, y: u32) {
    match bounds {
        Some([min_x, min_y, max_x, max_y]) => {
            *min_x = (*min_x).min(x);
            *min_y = (*min_y).min(y);
            *max_x = (*max_x).max(x);
            *max_y = (*max_y).max(y);
        }
        None => *bounds = Some([x, y, x, y]),
    }
}

fn time_slug(time: f64) -> String {
    format!("{time:.6}").replace('-', "m").replace('.', "_")
}

fn file_slug(value: &str) -> String {
    let slug: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect();
    if slug.is_empty() {
        "frame".to_string()
    } else {
        slug
    }
}

fn web_path(directory: &str, file: &str) -> String {
    format!("{directory}/{file}")
}

fn render_html(report: &DiffReport) -> Result<String> {
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(REPORT_HTML.replace("__REPORT_JSON__", &json))
}

const REPORT_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>gaanim visual diff</title>
<style>
:root{color-scheme:dark;--bg:#0b0d12;--panel:#151922;--line:#293140;--text:#e9edf5;--muted:#98a2b3;--ok:#37d67a;--bad:#ff5d73;--accent:#7c8cff}*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--text);font:14px/1.45 ui-sans-serif,system-ui,sans-serif}header{position:sticky;top:0;z-index:3;background:#0b0d12ee;backdrop-filter:blur(12px);border-bottom:1px solid var(--line);padding:18px 24px}.top{display:flex;gap:18px;align-items:center;flex-wrap:wrap}h1{font-size:20px;margin:0}.summary{color:var(--muted)}.pass{color:var(--ok)}.fail{color:var(--bad)}button{background:var(--panel);color:var(--text);border:1px solid var(--line);border-radius:8px;padding:7px 11px;cursor:pointer}button.active{border-color:var(--accent);background:#222941}.keys{margin-left:auto;color:var(--muted)}main{padding:22px;display:grid;grid-template-columns:repeat(auto-fit,minmax(420px,1fr));gap:18px}.card{background:var(--panel);border:1px solid var(--line);border-radius:12px;overflow:hidden}.card.changed{border-color:#713745}.meta{padding:13px 15px;display:flex;gap:10px;align-items:baseline}.meta strong{font-size:15px}.badge{margin-left:auto;border-radius:999px;padding:2px 8px;font-size:12px;background:#262d3a}.changed .badge{color:var(--bad);background:#351f27}.viewport{height:360px;background:repeating-conic-gradient(#10141b 0 25%,#141923 0 50%) 50%/20px 20px;display:grid;place-items:center;overflow:auto}.viewport img{max-width:100%;max-height:100%;object-fit:contain;image-rendering:auto}.metrics{padding:10px 15px;color:var(--muted);font-variant-numeric:tabular-nums}.empty{padding:60px;text-align:center;color:var(--muted)}@media(max-width:520px){main{grid-template-columns:1fr;padding:10px}.viewport{height:260px}.keys{display:none}}
</style></head>
<body><header><div class="top"><h1>gaanim visual diff</h1><span id="summary" class="summary"></span><button data-mode="baseline">1 Baseline</button><button data-mode="current">2 Current</button><button class="active" data-mode="diff">3 Diff</button><button id="toggle">Only failures</button><span class="keys">Keyboard: 1 / 2 / 3</span></div></header><main id="grid"></main>
<script>const report=__REPORT_JSON__;let mode='diff',onlyFailures=false;const grid=document.querySelector('#grid');const summary=document.querySelector('#summary');summary.textContent=`${report.passed?'PASS':'FAIL'} · ${report.compared} compared · ${report.changed} changed · ${report.missing} missing`;summary.classList.add(report.passed?'pass':'fail');function srcFor(frame){return frame[mode+'_file']||frame.current_file||frame.baseline_file}function draw(){grid.innerHTML='';const frames=report.frames.filter(f=>!onlyFailures||f.status!=='unchanged');if(!frames.length){grid.innerHTML='<div class="empty">No frames match this filter.</div>';return}for(const frame of frames){const card=document.createElement('article');card.className='card '+(frame.status==='unchanged'?'':'changed');const time=frame.time_seconds==null?'':` · ${frame.time_seconds.toFixed(6)}s`;const ratio=(frame.changed_ratio*100).toFixed(4);card.innerHTML=`<div class="meta"><strong></strong><span class="time"></span><span class="badge"></span></div><div class="viewport"></div><div class="metrics">${frame.changed_pixels.toLocaleString()} / ${frame.total_pixels.toLocaleString()} px (${ratio}%) · MAE ${frame.mean_absolute_error.toFixed(6)} · max Δ ${frame.max_channel_delta}</div>`;card.querySelector('strong').textContent=frame.id;card.querySelector('.time').textContent=time;card.querySelector('.badge').textContent=frame.status.replaceAll('_',' ');const src=srcFor(frame);const viewport=card.querySelector('.viewport');if(src){const img=document.createElement('img');img.src=src;img.alt=`${mode}: ${frame.id}`;viewport.appendChild(img)}else{viewport.textContent=`No ${mode} image`}grid.appendChild(card)}}function setMode(next){mode=next;document.querySelectorAll('[data-mode]').forEach(b=>b.classList.toggle('active',b.dataset.mode===mode));draw()}document.querySelectorAll('[data-mode]').forEach(b=>b.onclick=()=>setMode(b.dataset.mode));document.querySelector('#toggle').onclick=e=>{onlyFailures=!onlyFailures;e.target.classList.toggle('active',onlyFailures);draw()};addEventListener('keydown',e=>{if(e.key==='1')setMode('baseline');if(e.key==='2')setMode('current');if(e.key==='3')setMode('diff')});draw();</script></body></html>"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_changed_pixel_is_measured_and_bounded() {
        let baseline = DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, Rgba([0, 0, 0, 255])));
        let mut current = baseline.to_rgba8();
        current.put_pixel(1, 0, Rgba([10, 0, 0, 255]));
        let (frame, _) = compare_images(
            "test",
            Some(0.5),
            &baseline,
            &DynamicImage::ImageRgba8(current),
            CompareOptions {
                pixel_threshold: 2,
                max_changed_ratio: 0.0,
            },
        );

        assert_eq!(frame.status, FrameStatus::Changed);
        assert_eq!(frame.changed_pixels, 1);
        assert_eq!(frame.change_bounds, Some([1, 0, 1, 0]));
        assert_eq!(frame.changed_ratio, 0.25);
    }

    #[test]
    fn threshold_can_ignore_small_channel_noise() {
        let baseline = DynamicImage::ImageRgba8(RgbaImage::from_pixel(1, 1, Rgba([5, 5, 5, 255])));
        let current = DynamicImage::ImageRgba8(RgbaImage::from_pixel(1, 1, Rgba([7, 5, 5, 255])));
        let (frame, _) =
            compare_images("test", None, &baseline, &current, CompareOptions::default());
        assert_eq!(frame.status, FrameStatus::Unchanged);
        assert_eq!(frame.changed_pixels, 0);
    }

    #[test]
    fn directory_comparison_writes_portable_report() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("gaanim-diff-test-{}-{nonce}", std::process::id()));
        let baseline_dir = root.join("baseline");
        let current_dir = root.join("current");
        let report_dir = root.join("report");
        fs::create_dir_all(&baseline_dir).unwrap();
        fs::create_dir_all(&current_dir).unwrap();

        let baseline = RgbaImage::from_pixel(2, 2, Rgba([0, 0, 0, 255]));
        let mut current = baseline.clone();
        current.put_pixel(1, 1, Rgba([255, 0, 255, 255]));
        baseline.save(baseline_dir.join("frame.png")).unwrap();
        current.save(current_dir.join("frame.png")).unwrap();

        let report = compare_directories(
            &baseline_dir,
            &current_dir,
            &report_dir,
            CompareOptions::default(),
        )
        .unwrap();

        assert!(!report.passed);
        assert_eq!(report.changed, 1);
        assert!(report_dir.join(REPORT_FILE).is_file());
        assert!(report_dir.join("index.html").is_file());
        assert!(report_dir.join("assets/diff/0000_frame.png").is_file());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "requires a working GPU adapter"]
    fn captures_a_real_headless_seek() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "gaanim-capture-test-{}-{nonce}",
            std::process::id()
        ));
        let mut canvas = Canvas::new(64, 64);
        canvas.circle(12.0);
        canvas.wait(0.1);

        let manifest = capture_canvas(canvas, &root, &[0.0, 0.1]).unwrap();
        assert_eq!(manifest.snapshots.len(), 2);
        for snapshot in manifest.snapshots {
            assert!(root.join(snapshot.file).is_file());
        }

        fs::remove_dir_all(root).unwrap();
    }
}
