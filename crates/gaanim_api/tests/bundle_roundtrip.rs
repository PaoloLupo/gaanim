//! A recorded bundle replays every frame exactly as the scene drew it.

use gaanim_api::canvas::SceneModel;
use gaanim_api::export::{BundleConfig, record_canvas};
use gaanim_bundle::Bundle;
use gaanim_core::glam::DVec2;
use gaanim_core::peniko::{self, Color};
use gaanim_timeline::transition::TransitionType;

/// A scene that exercises what the 2D renderer composites: text, gradients,
/// dashed strokes, effects, clips, blends, write reveals, echo copies,
/// camera motion, stops, markers and masked scene transitions.
fn showcase() -> SceneModel {
    let mut canvas = SceneModel::new(16.0, 9.0);
    canvas.segment("intro", None).unwrap();
    let title = canvas
        .text("Reproducible")
        .fill(Color::WHITE)
        .move_to(0.0, 2.5);
    let gradient = peniko::Gradient::new_linear((-2.0, 0.0), (2.0, 0.0)).with_stops([
        Color::from_rgb8(0x4f, 0x9d, 0xff),
        Color::from_rgb8(0xff, 0x6b, 0x6b),
    ]);
    let card = canvas
        .rect(4.0, 2.0)
        .fill_brush(peniko::Brush::Gradient(gradient))
        .shadow(Color::from_rgba8(0, 0, 0, 120), DVec2::new(0.1, -0.1), 0.08)
        .move_to(-3.0, 0.0);
    let ring = canvas
        .circle(1.0)
        .stroke(Color::from_rgb8(0xff, 0xd1, 0x66), 0.08)
        .glow(Color::from_rgb8(0xff, 0xd1, 0x66), 0.2, 1.0)
        .move_to(3.0, 0.0);
    let blurred = canvas
        .circle(0.6)
        .fill(Color::from_rgba8(120, 200, 255, 200))
        .blur(0.05)
        .blend(Some(peniko::BlendMode::new(
            peniko::Mix::Screen,
            peniko::Compose::SrcOver,
        )))
        .move_to(3.4, 0.4);
    let clipped = canvas
        .rect(3.0, 3.0)
        .fill(Color::from_rgb8(0x6b, 0xff, 0xb8))
        .clip(&ring, peniko::Fill::NonZero);
    let dashed = canvas.line(-6.0, -3.0, 6.0, -3.0);
    let dashed = dashed.stroke(Color::WHITE, 0.05);
    canvas.play(vec![
        title.animate().write().duration(0.6),
        card.animate().fade_in().duration(0.4),
    ]);
    canvas.play(vec![
        ring.animate().create().duration(0.5),
        blurred.animate().fade_in().duration(0.5),
        clipped.animate().fade_in().duration(0.5),
        dashed.animate().create().duration(0.5),
    ]);
    canvas.marker("built").unwrap();
    canvas.stop(Some("look".into())).unwrap();
    let zoom = canvas.camera_zoom_to(1.4, 0.5);
    canvas.play(vec![
        card.animate().rotate_by(0.6).duration(0.5),
        ring.animate().shift_by(-1.0, 0.5).duration(0.5),
        zoom,
    ]);
    // A callback animation: its values are only known by calling it, so the
    // bundle must carry what it produced.
    let orbit = gaanim_animation::CustomAnimation::new(
        vec![
            gaanim_animation::CustomChannel::Position,
            gaanim_animation::CustomChannel::Opacity,
        ],
        |alpha| {
            let angle = alpha * std::f64::consts::TAU;
            Ok(gaanim_animation::CustomValues {
                position: Some(gaanim_core::glam::DVec3::new(
                    2.0 * angle.cos(),
                    2.0 * angle.sin(),
                    0.0,
                )),
                opacity: Some((0.4 + 0.6 * alpha) as f32),
                ..Default::default()
            })
        },
    )
    .unwrap();
    canvas.play(vec![blurred.animate().custom(orbit).unwrap().duration(0.6)]);
    canvas.wait(0.2);

    canvas
        .segment(
            "wipe",
            Some(TransitionType::Wipe {
                duration: 0.4,
                direction: DVec2::new(1.0, 0.0),
                feather: 0.2,
            }),
        )
        .unwrap();
    let dot = canvas.circle(0.8).fill(Color::from_rgb8(0xc0, 0x84, 0xfc));
    canvas.play(vec![dot.animate().grow_from_center().duration(0.4)]);
    canvas.stop(None).unwrap();
    canvas.wait(0.2);

    canvas
        .segment("fade", Some(TransitionType::CrossFade { duration: 0.3 }))
        .unwrap();
    let square = canvas
        .rect(2.0, 2.0)
        .fill(Color::from_rgb8(0x38, 0xbd, 0xf8));
    canvas.play(vec![square.animate().rotate_by(1.0).duration(0.4)]);
    canvas
}

#[test]
fn every_recorded_frame_replays_bit_for_bit() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("showcase.gaanim");
    let mut config = BundleConfig::new(&path);
    config.fps = 24;
    record_canvas(showcase(), config).expect("record the bundle");

    let mut bundle = Bundle::open(&path).expect("open the bundle");
    assert_eq!(bundle.scene.fps, 24);
    assert_eq!(bundle.scene.segments.len(), 3);
    assert_eq!(
        bundle
            .scene
            .segments
            .iter()
            .map(|segment| segment.stops.len())
            .sum::<usize>(),
        2
    );
    assert_eq!(bundle.scene.markers.len(), 1);
    // The grid plus the instants a presentation rests on.
    let stops: Vec<f64> = bundle
        .scene
        .segments
        .iter()
        .flat_map(|segment| segment.stops.iter().map(|stop| stop.time))
        .collect();
    for stop in &stops {
        let index = bundle.frame_index_at(*stop);
        assert_eq!(
            bundle.times()[index],
            *stop,
            "a frame rests exactly on {stop}"
        );
    }
    assert!(bundle.frame_count() > (bundle.scene.duration * 24.0) as usize);

    let mismatched = bundle.verify().expect("decode every frame");
    assert!(
        mismatched.is_empty(),
        "frames differ from the recording: {mismatched:?}"
    );

    // Something is drawn, and frames change over time.
    let first = bundle.frame(bundle.frame_index_at(0.3)).unwrap();
    let later = bundle.frame(bundle.frame_index_at(1.5)).unwrap();
    assert!(!first.capture.elements.is_empty());
    assert_ne!(bundle.digest(0), bundle.digest(bundle.frame_index_at(1.5)));
    assert!(later.capture.elements.len() >= first.capture.elements.len());
}

#[test]
fn a_damaged_bundle_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("small.gaanim");
    let mut canvas = SceneModel::new(16.0, 9.0);
    let circle = canvas.circle(1.0).fill(Color::WHITE);
    canvas.play(vec![circle.animate().fade_in().duration(0.2)]);
    let mut config = BundleConfig::new(&path);
    config.fps = 10;
    record_canvas(canvas, config).unwrap();

    // Rewrite the archive with one byte of the first frame chunk flipped.
    let damaged = directory.path().join("damaged.gaanim");
    {
        use std::io::{Read, Write};
        let mut source = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let mut target = zip::ZipWriter::new(std::fs::File::create(&damaged).unwrap());
        for index in 0..source.len() {
            let mut entry = source.by_index(index).unwrap();
            let name = entry.name().to_owned();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            if name == "frames/000000.bin" {
                let middle = bytes.len() / 2;
                bytes[middle] ^= 0x5a;
            }
            target
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            target.write_all(&bytes).unwrap();
        }
        target.finish().unwrap();
    }
    let path = damaged;
    let error = match Bundle::open(&path) {
        Ok(mut bundle) => bundle.frame(0).err(),
        Err(error) => Some(error),
    };
    assert!(error.is_some(), "a corrupted frame chunk must not decode");
}

#[test]
fn a_captured_frame_composes_exactly_like_an_export_frame() {
    let canvas = showcase();
    let mut app = gaanim_export::bundle::recording_app(move |world| {
        gaanim_api::runtime::replay_canvas_into(world, canvas)
    })
    .unwrap();
    let times = {
        let timeline = app
            .world()
            .resource::<gaanim_timeline::timeline::Timeline>();
        gaanim_export::bundle::recording_times(timeline, 12)
    };
    let mut store = gaanim_renderer::fragment::FragmentStore::default();
    for time in times {
        app.world_mut()
            .resource_mut::<gaanim_timeline::timeline::Timeline>()
            .seek_request = Some(time);
        app.update();
        let camera = app.world().resource::<gaanim_math::ResolvedCamera>().camera;
        let capture = gaanim_renderer::pipeline::capture_frame(app.world_mut(), Some(&camera));
        // One capture replays at any output size exactly as the scene
        // renders at that size.
        for size in [(1920, 1080), (480, 270)] {
            app.world_mut()
                .resource_mut::<gaanim_renderer::pipeline::CanvasBackground>()
                .pixel_size = size;
            let exported =
                gaanim_renderer::pipeline::compile_scene_from_world(app.world_mut(), Some(&camera));
            let background = app
                .world()
                .get_resource::<gaanim_renderer::pipeline::CanvasBackground>()
                .map(|background| (background, background.pixel_size));
            let pixels_per_unit =
                gaanim_renderer::pipeline::output_pixels_per_unit(&camera, size.0);
            let composed = gaanim_renderer::pipeline::compose_captured(
                &capture,
                &mut store,
                background,
                pixels_per_unit,
                None,
            );
            assert!(
                gaanim_renderer::canvas::draws_same(&exported, &composed),
                "the capture at {time}s composes differently from the export at {size:?}"
            );
        }
        store.retain_shared();
    }
}

/// Motion blur, a drawable exempt from it, a stateful updater, and a stop
/// and a marker between frames of the grid.
fn blurred() -> SceneModel {
    let mut canvas = SceneModel::new(16.0, 9.0);
    canvas.set_motion_blur(Some(
        gaanim_renderer::effects::MotionBlur::new(180.0, 3, None).unwrap(),
    ));
    canvas.segment("drift", None).unwrap();
    let dot = canvas
        .circle(0.8)
        .fill(Color::from_rgb8(0xff, 0x6b, 0x6b))
        .move_to(-4.0, 0.0);
    dot.add_updater(gaanim_api::canvas::UpdaterPreset::AdvanceX { speed: 3.0 });
    let hud = canvas
        .rect(3.0, 0.6)
        .fill(Color::from_rgba8(255, 255, 255, 180))
        .move_to(0.0, 3.5)
        .motion_blur(false);
    canvas.play(vec![hud.animate().fade_in().duration(0.3)]);
    canvas.wait(0.137);
    canvas.marker("between").unwrap();
    canvas.stop(Some("off-grid".into())).unwrap();
    canvas.play(vec![dot.animate().shift_by(0.0, 2.0).duration(0.4)]);
    canvas
}

#[test]
fn a_video_exported_from_the_bundle_matches_the_scene_export() {
    if gaanim_export::prelude::GpuContext::new(16, 16).is_err() {
        eprintln!("skipped: no GPU adapter");
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let bundle_path = directory.path().join("blurred.gaanim");
    let mut config = BundleConfig::new(&bundle_path);
    // The rate of a draft export.
    config.fps = 30;
    config.width = 160;
    config.height = 90;
    record_canvas(blurred(), config).unwrap();

    let mut bundle = Bundle::open(&bundle_path).unwrap();
    assert!(bundle.verify().unwrap().is_empty());
    let first = bundle.frame(0).unwrap();
    assert_eq!(
        first.motion_blur.len(),
        3,
        "grid frames carry their sub-frames"
    );
    // The stop between grid frames is recorded exactly, blurred like a
    // snapshot of the scene at that instant.
    let stop = bundle.scene.segments[0].stops[0].time;
    let at_stop = bundle.frame(bundle.frame_index_at(stop)).unwrap();
    assert_eq!(at_stop.time, stop);
    assert_eq!(at_stop.motion_blur.len(), 3);

    let output = |name: &str| {
        let directory = directory.path().join(name);
        std::fs::create_dir_all(&directory).unwrap();
        let mut export =
            gaanim_export::prelude::ExportConfig::new(&directory.join("f.png").to_string_lossy())
                .with_quality(gaanim_export::prelude::QualityPreset::Draft);
        export.width = 160;
        export.height = 90;
        export.aspect_ratio = gaanim_export::prelude::AspectRatioPreset::Custom;
        export.format = gaanim_export::prelude::ExportFormat::PngSequence;
        export.headless = true;
        (directory, export)
    };
    let (direct, config) = output("direct");
    gaanim_api::export::export_canvas(blurred(), config).unwrap();
    let (replayed, config) = output("bundle");
    gaanim_export::prelude::export_bundle(&bundle_path, config).unwrap();

    let mut frames: Vec<_> = std::fs::read_dir(&direct)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    frames.sort();
    assert!(frames.len() > 10);
    assert_eq!(frames.len(), std::fs::read_dir(&replayed).unwrap().count());
    for frame in frames {
        assert!(
            std::fs::read(direct.join(&frame)).unwrap()
                == std::fs::read(replayed.join(&frame)).unwrap(),
            "{frame:?} differs between the scene and the bundle"
        );
    }
}
