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
fn data_entries_are_zstd_frames_and_version_1_bundles_are_refused() {
    use std::io::{Read, Write};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("small.gaanim");
    let mut canvas = SceneModel::new(16.0, 9.0);
    let circle = canvas.circle(1.0).fill(Color::WHITE);
    canvas.play(vec![circle.animate().fade_in().duration(0.3)]);
    let mut config = BundleConfig::new(&path);
    config.fps = 10;
    record_canvas(canvas, config).unwrap();
    let recorded = Bundle::open(&path).unwrap();
    assert_eq!(recorded.manifest.version, gaanim_bundle::VERSION);

    // Rewrite it the way version 1 stored it: plain entries, archive Deflate.
    let legacy = directory.path().join("legacy.gaanim");
    {
        let mut source = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let mut target = zip::ZipWriter::new(std::fs::File::create(&legacy).unwrap());
        for index in 0..source.len() {
            let mut entry = source.by_index(index).unwrap();
            let name = entry.name().to_owned();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            if name == "manifest.json" {
                let mut manifest: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                manifest["version"] = 1.into();
                bytes = serde_json::to_vec(&manifest).unwrap();
            } else if name == gaanim_bundle::THUMBNAIL {
                assert_eq!(&bytes[..4], b"\x89PNG", "the cover is a plain PNG");
            } else if !name.starts_with("media/") {
                assert_eq!(
                    entry.compression(),
                    zip::CompressionMethod::Stored,
                    "{name} holds its own compression"
                );
                assert_eq!(
                    &bytes[..4],
                    &[0x28, 0xb5, 0x2f, 0xfd],
                    "{name} is a zstd frame"
                );
                let mut decoder = ruzstd::decoding::StreamingDecoder::new(&bytes[..]).unwrap();
                let mut plain = Vec::new();
                decoder.read_to_end(&mut plain).unwrap();
                bytes = plain;
            }
            target
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            target.write_all(&bytes).unwrap();
        }
        target.finish().unwrap();
    }
    let error = Bundle::open(&legacy)
        .err()
        .expect("a version 1 bundle is refused");
    let message = error.to_string();
    assert!(
        message.contains("no longer reads") && message.contains("gaanim export"),
        "{message}"
    );
}

#[test]
fn a_bundle_from_a_newer_format_names_its_generator() {
    use std::io::{Read, Write};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("small.gaanim");
    let mut canvas = SceneModel::new(16.0, 9.0);
    let circle = canvas.circle(1.0).fill(Color::WHITE);
    canvas.play(vec![circle.animate().fade_in().duration(0.1)]);
    let mut config = BundleConfig::new(&path);
    config.fps = 10;
    record_canvas(canvas, config).unwrap();
    let future = directory.path().join("future.gaanim");
    {
        let mut source = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let mut target = zip::ZipWriter::new(std::fs::File::create(&future).unwrap());
        for index in 0..source.len() {
            let mut entry = source.by_index(index).unwrap();
            let name = entry.name().to_owned();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            if name == "manifest.json" {
                let mut manifest: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                manifest["version"] = (gaanim_bundle::VERSION + 1).into();
                manifest["generator"] = "gaanim 9.9.9".into();
                bytes = serde_json::to_vec(&manifest).unwrap();
            }
            target
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            target.write_all(&bytes).unwrap();
        }
        target.finish().unwrap();
    }
    let error = Bundle::open(&future)
        .err()
        .expect("a newer format is rejected");
    assert!(error.to_string().contains("gaanim 9.9.9"), "{error}");
}

/// A 16:9 scene where a white disc fades in over its first second.
fn fading_disc() -> SceneModel {
    let mut canvas = SceneModel::new(16.0, 9.0);
    let disc = canvas.circle(3.0).fill(Color::WHITE);
    canvas.play(vec![disc.animate().fade_in().duration(1.0)]);
    canvas.wait(0.5);
    canvas
}

fn cover_of(canvas: SceneModel, name: &str) -> image::RgbaImage {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(name);
    let mut config = BundleConfig::new(&path);
    config.fps = 10;
    config.width = 1280;
    config.height = 720;
    record_canvas(canvas, config).unwrap();
    let png = Bundle::open(&path)
        .unwrap()
        .thumbnail()
        .unwrap()
        .expect("the bundle has a cover");
    image::load_from_memory_with_format(&png, image::ImageFormat::Png)
        .unwrap()
        .into_rgba8()
}

/// Mean brightness of the red channel, 0..255.
fn brightness(image: &image::RgbaImage) -> f64 {
    image.pixels().map(|pixel| f64::from(pixel[0])).sum::<f64>() / image.pixels().len() as f64
}

#[test]
fn bundles_carry_a_cover_of_the_fullest_or_the_chosen_frame() {
    // The cover is rendered on the GPU; without an adapter a bundle is
    // recorded without one, by design.
    if gaanim_export::prelude::GpuContext::new(16, 16).is_err() {
        eprintln!("skipped: no GPU adapter");
        return;
    }
    // Without stops: the frame that shows the most, once the disc is in.
    let full = cover_of(fading_disc(), "full.gaanim");
    assert_eq!(full.dimensions(), (512, 288));
    assert!(brightness(&full) > 40.0, "{}", brightness(&full));

    // An explicit instant: the empty first frame.
    let mut canvas = fading_disc();
    canvas.set_thumbnail(Some(0.0)).unwrap();
    let empty = cover_of(canvas, "empty.gaanim");
    assert!(
        brightness(&empty) + 20.0 < brightness(&full),
        "{} {}",
        brightness(&empty),
        brightness(&full)
    );

    // A stop wins over the fullest frame: one disc shows at the stop, a
    // second one joins after it.
    let two_discs = |stop: bool| {
        let mut canvas = fading_disc();
        if stop {
            canvas.stop(None).unwrap();
        }
        let second = canvas.circle(2.0).fill(Color::WHITE).move_to(5.5, 0.0);
        canvas.play(vec![second.animate().fade_in().duration(1.0)]);
        canvas
    };
    let at_stop = cover_of(two_discs(true), "stop.gaanim");
    let fullest = cover_of(two_discs(false), "fullest.gaanim");
    assert!((brightness(&at_stop) - brightness(&full)).abs() < 2.0);
    assert!(brightness(&fullest) > brightness(&at_stop) + 10.0);
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

/// Stops, a marker, typed text and a transition, without state that depends
/// on the instants the timeline visited.
fn history_free_scene() -> SceneModel {
    let mut canvas = SceneModel::new(16.0, 9.0);
    canvas.segment("typed", None).unwrap();
    let title = canvas
        .text("Stateless")
        .fill(Color::WHITE)
        .move_to(0.0, 2.0);
    canvas.play(vec![
        title
            .animate()
            .typewriter(14.0, Some("|"), 2.0, 0.3, 7, true)
            .unwrap(),
    ]);
    canvas.marker("typed").unwrap();
    canvas.stop(Some("title".into())).unwrap();
    let dot = canvas.circle(0.6).fill(Color::from_rgb8(0xf5, 0x9e, 0x0b));
    canvas.play(vec![dot.animate().shift_by(2.0, 0.0).duration(0.37)]);
    canvas.stop(None).unwrap();
    canvas
        .segment("next", Some(TransitionType::CrossFade { duration: 0.3 }))
        .unwrap();
    let square = canvas
        .rect(1.5, 1.5)
        .fill(Color::from_rgb8(0x38, 0xbd, 0xf8));
    canvas.play(vec![square.animate().rotate_by(1.0).duration(0.41)]);
    canvas
}

#[test]
fn a_history_free_scene_records_in_one_world_like_in_two() {
    let directory = tempfile::tempdir().unwrap();
    let record = |name: &str, force_second_world: bool| {
        let path = directory.path().join(name);
        let mut config = BundleConfig::new(&path);
        config.fps = 24;
        config.force_second_world = force_second_world;
        record_canvas(history_free_scene(), config).expect("record the bundle");
        Bundle::open(&path).expect("open the bundle")
    };
    let one = record("one.gaanim", false);
    let two = record("two.gaanim", true);

    // One world records every instant in time order; two record the grid first.
    assert!(one.times().windows(2).all(|pair| pair[0] < pair[1]));
    assert!(two.times().windows(2).any(|pair| pair[0] > pair[1]));
    let by_time = |bundle: &Bundle| {
        let mut frames: Vec<(u64, [u8; 32])> = (0..bundle.frame_count())
            .map(|index| {
                (
                    bundle.times()[index].to_bits(),
                    bundle.digest(index).unwrap(),
                )
            })
            .collect();
        frames.sort_by(|a, b| f64::from_bits(a.0).total_cmp(&f64::from_bits(b.0)));
        frames
    };
    assert_eq!(by_time(&one), by_time(&two));
}

/// Chalk strokes, a drawable drawn through a shader effect that moves
/// with time, a track matte, metaballs and glass.
fn chalk_and_effects() -> SceneModel {
    let mut canvas = SceneModel::new(16.0, 9.0);
    let board = canvas
        .circle(1.5)
        .stroke(Color::WHITE, 0.08)
        .chalk(7, 0.03)
        .move_to(-3.0, 0.0);
    let shader = gaanim_renderer::post_process::PostProcessShader::new(
        "fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {\n    \
         return gaanim_scene(uv + vec2<f32>(0.0, 0.05 * sin(uv.x * 12.0 + time * 4.0)));\n}",
    )
    .unwrap();
    let wave = canvas
        .rect(4.0, 1.0)
        .fill(Color::from_rgb8(0x38, 0xbd, 0xf8))
        .shader_effect(vec![shader.into()], 0.3)
        .move_to(3.0, 0.0);
    // A matte, metaballs and glass over what is drawn before it.
    let letters = canvas.text("M").fill(Color::WHITE).move_to(-3.0, 2.5);
    let stripes = canvas
        .rect(3.0, 1.5)
        .fill(Color::from_rgb8(0xff, 0x6b, 0x6b))
        .move_to(-3.0, 2.5)
        .matte(
            Some(&letters),
            gaanim_renderer::object_effects::MatteMode::Alpha,
        )
        .unwrap();
    let drops = [
        canvas.circle(0.5).move_to(1.0, -2.5),
        canvas.circle(0.4).move_to(2.2, -2.5),
    ];
    let blob = canvas
        .metaballs(&[&drops[0], &drops[1]], 1.0, 0.6)
        .unwrap()
        .fill(Color::from_rgb8(0xff, 0xd1, 0x66));
    let glass = canvas
        .rounded_rect(3.0, 2.0, 0.3)
        .fill(Color::from_rgba8(255, 255, 255, 30))
        .move_to(1.5, -2.0)
        .glass(Some(gaanim_renderer::object_effects::Glass {
            blur: 0.2,
            ..gaanim_renderer::object_effects::Glass::LIQUID
        }));
    let _ = (stripes, blob);
    canvas.play(vec![
        board.animate().create().duration(0.5),
        wave.animate().shift_by(0.0, 1.0).duration(0.5),
        drops[1].animate().shift_by(-0.6, 0.0).duration(0.5),
        glass.animate().shift_by(-1.0, 0.0).duration(0.5),
        letters.animate().shift_by(0.5, 0.0).duration(0.5),
    ]);
    canvas
}

#[test]
fn chalk_and_shader_effects_record_and_replay() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("effects.gaanim");
    let mut config = BundleConfig::new(&path);
    config.fps = 12;
    config.width = 160;
    config.height = 90;
    record_canvas(chalk_and_effects(), config).expect("record chalk and shader effects");

    let mut bundle = Bundle::open(&path).unwrap();
    assert!(bundle.verify().unwrap().is_empty());
    let frame = bundle.frame(bundle.frame_index_at(0.4)).unwrap();
    assert!(
        frame
            .capture
            .elements
            .iter()
            .any(|element| element.recipe.chalk.is_some()),
        "the chalk survives the bundle"
    );
    assert_eq!(frame.capture.effects.len(), 1);
    let effect = &frame.capture.effects[0];
    assert_eq!(effect.margin, 0.3);
    assert_eq!(effect.passes.len(), 1);
    assert!(
        frame
            .capture
            .elements
            .iter()
            .any(|element| element.effect_root == Some(effect.root))
    );
    assert_eq!(frame.capture.mattes.len(), 1);
    assert_eq!(frame.capture.glasses.len(), 1);
    assert!(
        frame
            .capture
            .elements
            .iter()
            .any(|element| element.matte_of == Some(frame.capture.mattes[0].source))
    );
}

#[test]
fn a_video_exported_from_a_bundle_with_effects_matches_the_scene_export() {
    if gaanim_export::prelude::GpuContext::new(16, 16).is_err() {
        eprintln!("skipped: no GPU adapter");
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let bundle_path = directory.path().join("effects.gaanim");
    let mut config = BundleConfig::new(&bundle_path);
    // The rate of a draft export.
    config.fps = 30;
    config.width = 160;
    config.height = 90;
    record_canvas(chalk_and_effects(), config).unwrap();

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
    gaanim_api::export::export_canvas(chalk_and_effects(), config).unwrap();
    let (replayed, config) = output("bundle");
    gaanim_export::prelude::export_bundle(&bundle_path, config).unwrap();

    let mut frames: Vec<_> = std::fs::read_dir(&direct)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    frames.sort();
    assert!(frames.len() > 4);
    assert_eq!(frames.len(), std::fs::read_dir(&replayed).unwrap().count());
    for frame in frames {
        assert!(
            std::fs::read(direct.join(&frame)).unwrap()
                == std::fs::read(replayed.join(&frame)).unwrap(),
            "{frame:?} differs between the scene and the bundle"
        );
    }
}

/// A shader background whose uniform follows an animated parameter, then a
/// shader transition whose uniform follows another.
fn reactive_shaders() -> SceneModel {
    let mut canvas = SceneModel::new(16.0, 9.0);
    let red = canvas.parameter(0.0).unwrap();
    let background = gaanim_renderer::background::ShaderBackground::with_uniforms(
        "fn gaanim_background(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {\n    \
         return vec4<f32>(gaanim_uniforms.red, 0.1, uv.x * 0.3, 1.0);\n}",
        Color::BLACK,
        vec![("red".to_string(), red.source())],
        None,
    )
    .unwrap();
    canvas.set_background_paint(Some(gaanim_renderer::background::BackgroundPaint::Shader(
        background,
    )));
    canvas.segment("a", None).unwrap();
    canvas.circle(1.0).fill(Color::WHITE);
    canvas.play(vec![red.animate().set(1.0).duration(0.5)]);
    let blue = canvas.parameter(0.0).unwrap();
    let shader = gaanim_scene::TransitionShader {
        source: "fn transition(uv: vec2<f32>) -> vec4<f32> {\n    \
                 return mix(gaanim_from(uv), vec4<f32>(0.0, 0.0, 1.0, 1.0), gaanim_uniforms.blue);\n}"
            .to_string(),
        uniforms: vec!["blue".to_string()],
        values: vec![0.0],
        data: None,
    };
    canvas
        .segment(
            "b",
            Some(TransitionType::Shader {
                duration: 0.5,
                shader: std::sync::Arc::new(shader),
                uniforms: vec![gaanim_animation::ResolvedScalarSource {
                    source: blue.source(),
                    parameters: Vec::new(),
                }],
            }),
        )
        .unwrap();
    canvas
        .rect(2.0, 1.0)
        .fill(Color::from_rgb8(0x38, 0xbd, 0xf8));
    canvas.play(vec![blue.animate().set(1.0).duration(0.5)]);
    canvas.wait(0.25);
    canvas
}

#[test]
fn reactive_shader_uniforms_record_and_replay() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("reactive.gaanim");
    let mut config = BundleConfig::new(&path);
    config.fps = 12;
    config.width = 160;
    config.height = 90;
    record_canvas(reactive_shaders(), config).expect("record reactive shaders");

    let mut bundle = Bundle::open(&path).unwrap();
    assert!(bundle.verify().unwrap().is_empty());
    let red_at = |bundle: &mut Bundle, time: f64| {
        let frame = bundle.frame(bundle.frame_index_at(time)).unwrap();
        frame
            .capture
            .background_values
            .iter()
            .find(|entry| entry.paint == 0)
            .map(|entry| entry.values[0])
    };
    let early = red_at(&mut bundle, 0.05).expect("the background records its uniform");
    let late = red_at(&mut bundle, 0.45).unwrap();
    assert!(
        early < late,
        "{early} then {late}: the uniform follows the parameter"
    );
    // In the transition the blend's uniform follows its parameter too.
    let blue_at = |bundle: &mut Bundle, time: f64| {
        let frame = bundle.frame(bundle.frame_index_at(time)).unwrap();
        frame
            .capture
            .transition
            .as_ref()
            .and_then(|transition| transition.shader.as_ref())
            .map(|shader| shader.values[0])
    };
    let start = blue_at(&mut bundle, 0.65).expect("a shader transition frame");
    let end = blue_at(&mut bundle, 0.9).expect("a shader transition frame");
    assert!(start < end, "{start} then {end}");
}

#[test]
fn a_video_exported_from_a_bundle_with_reactive_shaders_matches_the_scene_export() {
    if gaanim_export::prelude::GpuContext::new(16, 16).is_err() {
        eprintln!("skipped: no GPU adapter");
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let bundle_path = directory.path().join("reactive.gaanim");
    let mut config = BundleConfig::new(&bundle_path);
    config.fps = 30;
    config.width = 160;
    config.height = 90;
    record_canvas(reactive_shaders(), config).unwrap();
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
    gaanim_api::export::export_canvas(reactive_shaders(), config).unwrap();
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
