#[test]
fn image_overlays_are_downscaled_and_backgrounds_keep_the_image() {
    let directory = std::env::temp_dir().join(format!("gaanim-overlay-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("wide.png");
    image::RgbaImage::from_pixel(3000, 20, image::Rgba([10, 20, 30, 255]))
        .save(&path)
        .unwrap();
    let overlay = image_overlay(&path).unwrap();
    assert_eq!((overlay.width, overlay.height), (2048, 14));
    let Ok(gaanim_renderer::background::BackgroundPaint::Brush(gaanim_core::peniko::Brush::Image(
        brush,
    ))) = background_image(&path)
    else {
        panic!("an image background is an image brush");
    };
    assert_eq!((brush.image.width, brush.image.height), (3000, 20));
    assert!(image_overlay(directory.join("missing.png")).is_err());
    let _ = std::fs::remove_dir_all(&directory);
}

use super::*;

#[test]
fn forgetting_an_image_reads_it_again_and_keeps_the_others() {
    let temp = tempfile::tempdir().unwrap();
    let write = |name: &str, shade: u8| {
        let path = temp.path().join(name);
        image::RgbaImage::from_pixel(2, 2, image::Rgba([shade, 0, 0, 255]))
            .save(&path)
            .unwrap();
        path
    };
    let cover = write("cover.png", 10);
    let logo = write("logo.png", 20);
    let id = |path: &Path| load_image(path).unwrap().data.id();
    let (cover_id, logo_id) = (id(&cover), id(&logo));
    assert_eq!(id(&cover), cover_id, "cached");

    write("cover.png", 30);
    forget_assets(std::slice::from_ref(&cover));
    assert_ne!(id(&cover), cover_id, "read again");
    assert_eq!(load_image(&cover).unwrap().data.data()[0], 30);
    assert_eq!(id(&logo), logo_id, "another file stays cached");
}

#[test]
fn image_cache_forgets_the_least_recently_used_images_past_its_budget() {
    let image = |bytes: usize| gaanim_core::peniko::ImageData {
        data: gaanim_core::peniko::Blob::from(vec![0; bytes]),
        format: gaanim_core::peniko::ImageFormat::Rgba8,
        alpha_type: gaanim_core::peniko::ImageAlphaType::Alpha,
        width: 1,
        height: (bytes / 4) as u32,
    };
    let mut cache = ImageCache::default();
    for name in ["a", "b", "c"] {
        cache.insert(PathBuf::from(name), image(400), 1000);
    }
    // Only two fit: "a", the oldest, made room for "c".
    assert!(cache.get(Path::new("a")).is_none());
    assert!(cache.get(Path::new("b")).is_some());
    cache.insert(PathBuf::from("d"), image(400), 1000);
    // "b" was used after "c", so "c" goes.
    assert!(cache.get(Path::new("c")).is_none());
    assert!(cache.get(Path::new("b")).is_some() && cache.get(Path::new("d")).is_some());
    assert_eq!(cache.bytes, 800);
    cache.clear();
    assert_eq!((cache.bytes, cache.entries.len()), (0, 0));
}

#[test]
fn dimension_label_style_reaches_label_value_and_unit() {
    let mut canvas = SceneModel::new(16, 9);
    let style = gaanim_text::prelude::TextStyle {
        font: Some("Cascadia Mono".into()),
        weight: Some(700),
        color: Some(Color::from_rgb8(200, 30, 30)),
        ..Default::default()
    };
    let dimension = canvas
        .dimension_between_with_options(
            CanvasEndpoint::Static(DVec3::ZERO),
            CanvasEndpoint::Static(DVec3::new(0.0, 2.0, 0.0)),
            0.5,
            DimensionOptions {
                label: Some("h".into()),
                show_value: true,
                unit: Some("m".into()),
                side: Some(gaanim_animation::DimensionSide::Right),
                font_size: Some(0.4),
                label_style: style,
                color: Some(Color::BLACK),
                ..Default::default()
            },
        )
        .unwrap();
    for part in [&dimension.label, &dimension.unit] {
        let spec = part.as_ref().unwrap().spec.lock().unwrap();
        let SpawnKind::Text(text) = &spec.kind else {
            panic!("expected a text part");
        };
        assert_eq!(text.style.font.as_deref(), Some("Cascadia Mono"));
        assert_eq!(text.style.weight, Some(700));
        assert_eq!(text.style.size, Some(0.4));
        assert_eq!(text.style.color, Some(Color::from_rgb8(200, 30, 30)));
    }
    let number = dimension.number.unwrap();
    let spec = number.spec.lock().unwrap();
    let SpawnKind::ReactiveReadout {
        font_family,
        font_weight,
        font_size,
        ..
    } = &spec.kind
    else {
        panic!("expected a reactive readout");
    };
    assert_eq!(font_family.as_deref(), Some("Cascadia Mono"));
    assert_eq!(*font_weight, Some(700));
    assert_eq!(*font_size, Some(0.4));
    drop(spec);
    let state = canvas.state.lock().unwrap();
    let sides: Vec<_> = state
        .active()
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::AttachTrackingDimension { side, .. }
            | Op::AttachDimensionLabelPlacement { side, .. } => Some(*side),
            _ => None,
        })
        .collect();
    assert_eq!(sides, [Some(gaanim_animation::DimensionSide::Right); 2]);
}

#[test]
fn dimensioned_arrow_caps_head_and_rejects_invalid_inputs() {
    let mut canvas = SceneModel::new(16, 9);
    let arrow = canvas
        .arrow_with_dimensions((0.0, 0.0), (0.2, 0.0), 0.18, 0.15, 0.036, Some(0.3))
        .unwrap();
    let spec = arrow.spec.lock().unwrap();
    let SpawnKind::SizedArrow {
        head_length,
        head_width,
        body_width,
        ..
    } = spec.kind
    else {
        panic!("expected dimensioned arrow");
    };
    assert!((head_length - 0.06).abs() < 1e-12);
    assert!((head_width - 0.05).abs() < 1e-12);
    assert_eq!(body_width, 0.036);
    drop(spec);
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(
            canvas
                .arrow_with_dimensions((0.0, 0.0), (1.0, 0.0), bad, 0.15, 0.036, None)
                .is_err()
        );
        assert!(
            canvas
                .arrow_with_dimensions((0.0, 0.0), (1.0, 0.0), 0.18, bad, 0.036, None)
                .is_err()
        );
        assert!(
            canvas
                .arrow_with_dimensions((0.0, 0.0), (1.0, 0.0), 0.18, 0.15, bad, None)
                .is_err()
        );
        assert!(
            canvas
                .arrow_with_dimensions((0.0, 0.0), (1.0, 0.0), 0.18, 0.15, 0.036, Some(bad))
                .is_err()
        );
    }
    assert!(
        canvas
            .arrow_with_dimensions((f64::NAN, 0.0), (1.0, 0.0), 0.18, 0.15, 0.036, None)
            .is_err()
    );
    assert!(
        canvas
            .arrow_with_dimensions((0.0, 0.0), (1.0, 0.0), 0.18, 0.15, 0.036, Some(1.1))
            .is_err()
    );
}

#[test]
fn curved_arrows_accept_arrow_dimensions_capped_by_arc_length() {
    let mut canvas = SceneModel::new(16, 9);
    let dimensions = |handle: &DrawableHandle| match handle.spec.lock().unwrap().kind {
        SpawnKind::CurvedArrow {
            head_length,
            head_width,
            body_width,
            ..
        }
        | SpawnKind::CurvedArrowArc {
            head_length,
            head_width,
            body_width,
            ..
        } => (head_length, head_width, body_width),
        _ => panic!("expected a curved arrow"),
    };
    let default = canvas.curved_arrow(-1.0, 0.0, 1.0, 0.0, 0.9);
    assert_eq!(
        dimensions(&default),
        (
            DEFAULT_ARROW_HEAD_LENGTH,
            DEFAULT_ARROW_HEAD_WIDTH,
            DEFAULT_ARROW_BODY_WIDTH
        )
    );
    let bold = canvas
        .curved_arrow_with_dimensions((-1.0, 0.0), (1.0, 0.0), 0.9, 0.3, 0.24, 0.06, None)
        .unwrap();
    assert_eq!(dimensions(&bold), (0.3, 0.24, 0.06));
    // A quarter turn of radius 0.4 is about 0.63 long; 30 % of it is 0.19.
    let arc_length = 0.4 * std::f64::consts::FRAC_PI_2;
    let capped = canvas
        .curved_arrow_arc_with_dimensions(
            (0.0, 0.0),
            0.4,
            0.0,
            std::f64::consts::FRAC_PI_2,
            0.4,
            0.2,
            0.05,
            Some(0.3),
        )
        .unwrap();
    let (head_length, head_width, body_width) = dimensions(&capped);
    assert!((head_length - arc_length * 0.3).abs() < 1e-12);
    assert!((head_width - 0.2 * arc_length * 0.3 / 0.4).abs() < 1e-12);
    assert_eq!(body_width, 0.05);

    assert!(
        canvas
            .curved_arrow_with_dimensions((0.0, 0.0), (1.0, 0.0), 0.5, 0.0, 0.2, 0.05, None)
            .is_err()
    );
    assert!(
        canvas
            .curved_arrow_with_dimensions((0.0, 0.0), (1.0, 0.0), f64::NAN, 0.3, 0.2, 0.05, None)
            .is_err()
    );
    assert!(
        canvas
            .curved_arrow_arc_with_dimensions((0.0, 0.0), 1.0, 0.0, 1.0, 0.3, 0.2, 0.05, Some(0.0))
            .is_err()
    );
}

#[test]
fn typst_asset_uses_the_configured_asset_root() {
    let root = std::env::temp_dir().join(format!(
        "gaanim_typst_asset_canvas_test_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("title.typ"), "= Asset-backed Typst").unwrap();

    let mut canvas = SceneModel::new(640, 360);
    canvas.set_asset_root(&root).unwrap();
    let handle = canvas.typst_asset("title.typ").unwrap();
    let spec = handle.spec.lock().unwrap();
    let SpawnKind::Typst { source, .. } = &spec.kind else {
        panic!("typst asset should create a Typst drawable");
    };
    assert_eq!(source, "= Asset-backed Typst");
}

#[test]
fn a_typst_document_carries_the_project_files_it_reads() {
    let temp = tempfile::tempdir().unwrap();
    let (assets, shared) = (temp.path().join("assets"), temp.path().join("brand"));
    std::fs::create_dir_all(assets.join("slides")).unwrap();
    std::fs::create_dir_all(&shared).unwrap();
    std::fs::write(
        assets.join("slides/title.typ"),
        "#import \"parts.typ\": heading\n#import \"/palette.typ\": accent\n#heading",
    )
    .unwrap();
    std::fs::write(assets.join("slides/parts.typ"), "#let heading = [Hello]\n").unwrap();
    std::fs::write(shared.join("palette.typ"), "#let accent = blue\n").unwrap();

    let mut canvas = SceneModel::new(640, 360);
    canvas.set_asset_root(&assets).unwrap();
    canvas.shared_asset_roots = vec![shared.clone()];
    let files = |handle: &DrawableHandle| {
        let spec = handle.spec.lock().unwrap();
        let SpawnKind::Typst { files, .. } = &spec.kind else {
            panic!("a Typst drawable");
        };
        files.clone()
    };
    // The asset's own folder first, then the scene's and the workspace's.
    let title = canvas.typst_asset("slides/title.typ").unwrap();
    assert_eq!(
        files(&title).paths().collect::<Vec<_>>(),
        ["/palette.typ", "/parts.typ"]
    );
    let inline = canvas.typst("#import \"/palette.typ\": accent\n#text(fill: accent)[x]");
    assert_eq!(files(&inline).paths().collect::<Vec<_>>(), ["/palette.typ"]);
    assert!(files(&canvas.typst("= Plain")).is_empty());

    // An edited import changes what the replay fingerprints.
    let before = format!("{:?}", files(&title));
    std::fs::write(shared.join("palette.typ"), "#let accent = red\n").unwrap();
    let again = canvas.typst_asset("slides/title.typ").unwrap();
    assert_ne!(format!("{:?}", files(&again)), before);
}

#[test]
fn shared_asset_roots_come_after_the_scene_folders() {
    let temp = tempfile::tempdir().unwrap();
    let (own, shared, brand) = (
        temp.path().join("own"),
        temp.path().join("shared"),
        temp.path().join("brand"),
    );
    for folder in [&own, &shared, &brand] {
        std::fs::create_dir_all(folder).unwrap();
    }
    std::fs::write(own.join("logo.svg"), "").unwrap();
    std::fs::write(shared.join("logo.svg"), "").unwrap();
    std::fs::write(shared.join("mascot.svg"), "").unwrap();
    std::fs::write(brand.join("mascot.svg"), "").unwrap();
    std::fs::write(brand.join("font.ttf"), "").unwrap();

    let mut canvas = SceneModel::new(640, 360);
    canvas.shared_asset_roots = vec![shared.clone(), brand.clone()];
    canvas.asset_search = vec![own.clone()];
    assert_eq!(canvas.resolve_asset_path("logo.svg"), own.join("logo.svg"));
    assert_eq!(
        canvas.resolve_asset_path("mascot.svg"),
        shared.join("mascot.svg")
    );
    assert_eq!(
        canvas.resolve_asset_path("font.ttf"),
        brand.join("font.ttf")
    );
    // Missing everywhere, the path stays as written.
    assert_eq!(
        canvas.resolve_asset_path("missing.png"),
        PathBuf::from("missing.png")
    );

    canvas.set_asset_root(&own).unwrap();
    let own = canvas.asset_root.clone().unwrap();
    assert_eq!(canvas.resolve_asset_path("logo.svg"), own.join("logo.svg"));
    assert_eq!(
        canvas.resolve_asset_path("font.ttf"),
        brand.join("font.ttf")
    );
    assert_eq!(
        canvas.resolve_asset_path("missing.png"),
        own.join("missing.png")
    );
}

#[test]
fn surrounding_rect_validates_geometry_and_tracks_retarget_cursor() {
    let mut canvas = SceneModel::new(640, 360);
    let left = canvas.circle(20.0);
    let right = canvas.rect(80.0, 30.0);
    assert!(matches!(
        canvas.surrounding_rect(vec![], [12.0; 4], 8.0),
        Err(SurroundingRectError::NoTargets)
    ));
    let frame = canvas
        .surrounding_rect(vec![left.bounds_target()], [8.0, 12.0, 8.0, 12.0], 6.0)
        .unwrap();
    let animation = frame
        .retarget(vec![right.bounds_target()], Some(0.75))
        .unwrap();
    assert_eq!(animation.inner.duration, 0.75);
    assert_eq!(*frame.targets.lock().unwrap(), vec![right.bounds_target()]);
    assert!(frame.drawable.is_live_derived_geometry());
}
use crate::canvas::ops::Op;
use bevy::prelude::World;
use gaanim_math::SpatialTransform;
use gaanim_scene::{MobjectId, Opacity};
use gaanim_timeline::clip::{ClipPayload, PropertyLensSpec};
use gaanim_timeline::scene::SceneMember;
use gaanim_timeline::snapshot::WorldSnapshot;
use gaanim_timeline::timeline::Timeline;

trait UnifiedTextFixture {
    fn math_text(&mut self, source: &str) -> DrawableHandle;
    fn test_title(&mut self, source: &str) -> DrawableHandle;
}

impl UnifiedTextFixture for SceneModel {
    fn math_text(&mut self, source: &str) -> DrawableHandle {
        self.text(&format!("${source}$"))
    }

    fn test_title(&mut self, source: &str) -> DrawableHandle {
        let spec = gaanim_text::prelude::TextSpec::new(
            vec![source.into()],
            Some(gaanim_text::prelude::TextRole::Title),
            gaanim_text::prelude::TextStyle::default(),
            gaanim_text::prelude::TextFlow::default(),
        )
        .expect("valid title fixture");
        self.text_spec(spec)
    }
}

fn compile_updater_count(canvas: &SceneModel) -> usize {
    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    world
        .query_filtered::<bevy::prelude::Entity, bevy::prelude::With<gaanim_animation::Updater>>()
        .iter(&world)
        .count()
}

#[test]
fn text_transform_to_text_is_a_semantic_text_transition() {
    let mut canvas = SceneModel::new(320, 180);
    let source = canvas
        .math_text("x + 3 = 7")
        .define_tag("lhs", "x", Some(0));
    let target = canvas.math_text("x = 4").define_tag("lhs", "x", Some(0));
    let circle = canvas.circle(1.0);
    // Text to text keeps glyph (part) fills: a structural transition,
    // not a flattened single-color path morph.
    let anim = source.animate().transform_to(&target).unwrap();
    match &anim.inner.anim_type {
        AnimationType::TextTransition {
            target: to,
            copy,
            semantic_pairs,
        } => {
            assert_eq!(*to, target.id);
            assert!(!copy);
            assert_eq!(
                semantic_pairs,
                &vec![("x".to_string(), Some(0), "x".to_string(), Some(0))]
            );
        }
        other => panic!("expected a text transition, got {other:?}"),
    }
    // Other drawables keep the generic morph.
    let shape = circle.animate().transform_to(&target).unwrap();
    assert!(matches!(
        shape.inner.anim_type,
        AnimationType::Transform { .. }
    ));
}

#[test]
fn step_equation_preserves_explicit_tag_mapping_and_occurrence() {
    let mut canvas = SceneModel::new(320, 180);
    let source = canvas
        .math_text("x + x = 2x")
        .define_tag("right_x", "x", Some(1));
    let target = canvas
        .math_text("x = y")
        .define_tag("renamed", "y", Some(0));
    let step = source
        .step_to(
            &target,
            Some(vec![("right_x".to_string(), "renamed".to_string())]),
            0.8,
        )
        .unwrap();
    canvas.play(vec![step]);

    let state = canvas.state.lock().expect("canvas state poisoned");
    let pairs = state
        .segments
        .iter()
        .flat_map(|segment| &segment.ops)
        .find_map(|op| match op {
            Op::Play(anims) | Op::Launch(anims) => {
                anims.iter().find_map(|anim| match &anim.anim_type {
                    AnimationType::TextTransition { semantic_pairs, .. } => Some(semantic_pairs),
                    _ => None,
                })
            }
            _ => None,
        })
        .expect("step equation op should be queued");
    assert_eq!(
        pairs,
        &vec![("x".to_string(), Some(1), "y".to_string(), Some(0),)]
    );
}

#[test]
fn text_selection_compound_properties_seek_only_selected_glyphs() {
    let mut canvas = SceneModel::new(320, 180);
    let text = canvas.text("ABC");
    let selection = text.select("B");
    let red = Color::from_rgb8(255, 0, 0);
    canvas.play(vec![
        selection
            .animate_properties()
            .fill(red)
            .opacity(0.25)
            .duration(1.0),
    ]);

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    let mut timeline = world.remove_resource::<Timeline>().expect("timeline");
    let animated_targets = timeline
        .clips
        .values()
        .filter_map(|clip| match &clip.payload {
            ClipPayload::Animation(animation)
                if matches!(
                    animation.lens,
                    PropertyLensSpec::FillColor { .. } | PropertyLensSpec::Opacity { .. }
                ) =>
            {
                Some(animation.target)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        animated_targets.len(),
        2,
        "one selected glyph must receive two channels"
    );
    assert_eq!(animated_targets[0], animated_targets[1]);
    let entity = world
        .query::<(bevy::prelude::Entity, &MobjectId)>()
        .iter(&world)
        .find_map(|(entity, id)| (id.0 == animated_targets[0]).then_some(entity))
        .expect("selected glyph entity");

    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    timeline.seek(&mut world, 0.0);
    let start_opacity = world.get::<Opacity>(entity).expect("start opacity").0;
    timeline.seek(&mut world, 0.5);
    let mid_opacity = world.get::<Opacity>(entity).expect("mid opacity").0;
    timeline.seek(&mut world, 1.0);
    let end_opacity = world.get::<Opacity>(entity).expect("end opacity").0;
    assert!((start_opacity - 1.0).abs() < 1e-6);
    assert!(mid_opacity < start_opacity && mid_opacity > end_opacity);
    assert!((end_opacity - 0.25).abs() < 1e-6);
    assert!(matches!(
        world.get::<gaanim_scene::FillBrush>(entity).and_then(|fill| fill.0.as_ref()),
        Some(Brush::Solid(color)) if *color == red
    ));
}

#[test]
fn custom_updater_survives_preview_and_export_recompilation() {
    let mut canvas = SceneModel::new(320, 180);
    let dot = canvas.dot(8.0);
    dot.add_custom_updater(gaanim_animation::Updater::new(
        |_dt, _elapsed, _entity, _world| true,
    ));

    assert_eq!(compile_updater_count(&canvas), 1);
    assert_eq!(compile_updater_count(&canvas), 1);
}

#[test]
fn reactive_objects_do_not_run_before_their_authored_cursor() {
    let mut canvas = SceneModel::new(320, 180);
    canvas.wait(2.0);
    let dot = canvas.dot(8.0);
    dot.add_custom_updater(gaanim_animation::Updater::new(
        |_dt, elapsed, entity, world| {
            if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
                transform.translation.x = elapsed;
            }
            true
        },
    ));
    let _trail = canvas.traced_path(&dot);
    canvas.wait(1.0);

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_animation::PlaybackState::default());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    let snapshot = WorldSnapshot::capture(&mut world);
    let mut timeline = world.remove_resource::<Timeline>().unwrap();
    timeline.add_keyframe(0.0, snapshot);
    timeline.seek(&mut world, 1.0);

    let mut updater_query = world.query_filtered::<
            (bevy::prelude::Entity, &SpatialTransform),
            bevy::prelude::With<gaanim_animation::Updater>,
        >();
    let (_, transform) = updater_query.single(&world).unwrap();
    assert_eq!(transform.translation.x, 0.0);

    let mut trail_query = world.query::<&gaanim_animation::TracedPath>();
    let traced_path = trail_query.single(&world).unwrap();
    assert!(traced_path.points.is_empty());
}

#[test]
fn traced_path_requires_an_explicit_entry_animation() {
    let mut hidden_canvas = SceneModel::new(320, 180);
    let hidden_dot = hidden_canvas.dot(8.0);
    let _hidden_trail = hidden_canvas.traced_path(&hidden_dot);
    hidden_canvas.wait(1.0);

    let mut hidden_world = World::new();
    hidden_world.insert_resource(Timeline::new());
    hidden_world.insert_resource(gaanim_animation::PlaybackState::default());
    hidden_world.insert_resource(gaanim_text::font::FontRegistry::new());
    hidden_world.insert_resource(gaanim_text::prelude::TextConfig::default());
    hidden_canvas.compile(&mut hidden_world);
    hidden_world.flush();

    let hidden_opacity = hidden_world
        .query_filtered::<&Opacity, bevy::prelude::With<gaanim_animation::TracedPath>>()
        .single(&hidden_world)
        .unwrap();
    assert_eq!(hidden_opacity.0, 0.0);

    let mut animated_canvas = SceneModel::new(320, 180);
    let animated_dot = animated_canvas.dot(8.0);
    let animated_trail = animated_canvas.traced_path(&animated_dot);
    animated_canvas.play(vec![animated_trail.fade_in(1.0)]);

    let mut animated_world = World::new();
    animated_world.insert_resource(Timeline::new());
    animated_world.insert_resource(gaanim_animation::PlaybackState::default());
    animated_world.insert_resource(gaanim_text::font::FontRegistry::new());
    animated_world.insert_resource(gaanim_text::prelude::TextConfig::default());
    animated_canvas.compile(&mut animated_world);
    animated_world.flush();

    let snapshot = WorldSnapshot::capture(&mut animated_world);
    let mut timeline = animated_world.remove_resource::<Timeline>().unwrap();
    timeline.add_keyframe(0.0, snapshot);
    timeline.seek(&mut animated_world, 0.5);
    let halfway = animated_world
        .query_filtered::<&Opacity, bevy::prelude::With<gaanim_animation::TracedPath>>()
        .single(&animated_world)
        .unwrap()
        .0;
    assert!(halfway > 0.0 && halfway < 1.0);
}

#[test]
fn reactive_visuals_require_their_own_play_entry() {
    let mut canvas = SceneModel::new(320, 180);
    let anchor = canvas.dot(8.0).move_to(-60.0, 0.0);
    let mass = canvas.dot(8.0).move_to(60.0, 0.0);
    let spring = canvas.spring_between_with_crossing(
        CanvasEndpoint::Entity(anchor.id),
        CanvasEndpoint::Entity(mass.id),
        6,
        10.0,
        1.0,
    );
    let dimension = canvas.dimension_between(
        CanvasEndpoint::Entity(anchor.id),
        CanvasEndpoint::Entity(mass.id),
        -24.0,
    );
    let label = canvas.text("mass");
    label.follow_to(&mass, 0.0, 28.0);
    canvas.play(vec![
        spring.fade_in(1.0),
        dimension.create(1.0),
        label.fade_in(1.0),
    ]);

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_animation::PlaybackState::default());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    let opacity_for = |world: &mut World, id: gaanim_core::ObjectId| {
        // SceneBuilder's id counter starts at zero while SceneModel ids start
        // at one; the test resolves the corresponding compiled root.
        let compiled_id = gaanim_core::ObjectId::from_raw(id.as_raw() - 1);
        let mut query = world.query::<(&gaanim_scene::MobjectId, &Opacity)>();
        query
            .iter(world)
            .find(|(object_id, _)| object_id.0 == compiled_id)
            .map(|(_, opacity)| opacity.0)
            .expect("reactive visual should compile into a mobject")
    };

    assert_eq!(opacity_for(&mut world, anchor.id), 1.0);
    assert_eq!(opacity_for(&mut world, spring.id), 0.0);
    // A dimension is an annotation: its `create` draws it in, but it is
    // not hidden by opacity like the spring.
    assert_eq!(opacity_for(&mut world, dimension.id), 1.0);
    assert_eq!(opacity_for(&mut world, label.id), 0.0);

    let snapshot = WorldSnapshot::capture(&mut world);
    let mut timeline = world.remove_resource::<Timeline>().unwrap();
    timeline.add_keyframe(0.0, snapshot);
    timeline.seek(&mut world, 0.5);

    let spring_opacity = opacity_for(&mut world, spring.id);
    let dimension_opacity = opacity_for(&mut world, dimension.id);
    let label_opacity = opacity_for(&mut world, label.id);
    assert!(spring_opacity > 0.0);
    assert!(dimension_opacity > 0.0);
    assert!(label_opacity > 0.0);
}

#[test]
fn length_lines_preserve_size_and_orientation_with_next_to() {
    use gaanim_layout::Direction;
    let mut canvas = SceneModel::new(640, 360);
    let card = canvas.rect(4.0, 2.0).move_to(1.0, 1.0);
    let horizontal = canvas
        .line_with_length(3.0, Direction::Right)
        .unwrap()
        .next_to(&card, Direction::Down, 0.5);
    let vertical = canvas
        .line_with_length(3.0, Direction::Up)
        .unwrap()
        .next_to(&card, Direction::Right, 0.5);
    let diagonal = canvas
        .line_with_length(5.0, Direction::Custom(DVec3::new(3.0, 4.0, 0.0)))
        .unwrap();
    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    for (handle, expected_start, expected_end) in [
        (horizontal, (-0.5, -0.5), (2.5, -0.5)),
        (vertical, (3.5, -0.5), (3.5, 2.5)),
        (diagonal, (-1.5, -2.0), (1.5, 2.0)),
    ] {
        let id = gaanim_core::ObjectId::from_raw(handle.id.as_raw() - 1);
        let (_, path, transform) = world
            .query::<(
                &gaanim_scene::MobjectId,
                &gaanim_scene::Path2D,
                &gaanim_math::SpatialTransform,
            )>()
            .iter(&world)
            .find(|(object, _, _)| object.0 == id)
            .unwrap();
        let affine = transform.to_affine_2d();
        let elements = path.0.elements();
        let [
            gaanim_core::kurbo::PathEl::MoveTo(start),
            gaanim_core::kurbo::PathEl::LineTo(end),
        ] = elements
        else {
            panic!("expected a single line segment");
        };
        assert!((affine * *start).distance(expected_start.into()) < 1e-9);
        assert!((affine * *end).distance(expected_end.into()) < 1e-9);
    }
}

#[test]
fn length_lines_reject_invalid_geometry() {
    use gaanim_layout::Direction;
    let mut canvas = SceneModel::new(640, 360);
    for length in [0.0, -1.0, f64::INFINITY, f64::NAN] {
        assert!(canvas.line_with_length(length, Direction::Right).is_err());
    }
    for vector in [
        DVec3::ZERO,
        DVec3::Z,
        DVec3::new(1.0, 0.0, 1.0),
        DVec3::splat(f64::NAN),
    ] {
        assert!(
            canvas
                .line_with_length(2.0, Direction::Custom(vector))
                .is_err()
        );
    }
}

#[test]
fn line_between_two_fixed_points_moves_with_its_group() {
    let mut canvas = SceneModel::new(320, 180);
    let bar = canvas.line_between(
        CanvasEndpoint::Static(DVec3::new(-3.0, -1.5, 0.0)),
        CanvasEndpoint::Static(DVec3::new(-3.0, 1.5, 0.0)),
    );
    let group = canvas.group(&[&bar]);
    canvas.play(vec![group.animate().shift_by(4.0, 0.0).duration(1.0)]);

    let mut app = bevy::prelude::App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin);
    canvas.compile(app.world_mut());
    app.finish();
    app.cleanup();
    app.update();
    assert!(
        app.world_mut()
            .query_filtered::<(), With<gaanim_animation::TrackingLine>>()
            .iter(app.world())
            .next()
            .is_none(),
        "fixed endpoints need no world-space tracking"
    );

    app.world_mut().resource_mut::<Timeline>().seek_request = Some(2.0);
    app.update();
    app.update();
    let world_x = app
        .world_mut()
        .query::<(&gaanim_scene::Path2D, &gaanim_math::GlobalSpatialTransform)>()
        .iter(app.world())
        .find_map(|(path, transform)| match path.0.elements().first() {
            Some(gaanim_core::kurbo::PathEl::MoveTo(start)) => {
                Some((transform.affine_2d * *start).x)
            }
            _ => None,
        })
        .expect("line leaf");
    assert!(
        (world_x - 1.0).abs() < 1e-6,
        "line followed its group to {world_x}"
    );
}

#[test]
fn line_between_accepts_static_and_anchor_endpoints() {
    let mut canvas = SceneModel::new(320, 180);
    let reference = canvas.rect(100.0, 40.0).move_to(30.0, 40.0);
    let anchor = reference.anchor_point(Anchor::TopRight, DVec3::ZERO);
    canvas.line_between(
        CanvasEndpoint::Static(DVec3::new(-10.0, -20.0, 0.0)),
        anchor.into(),
    );

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    gaanim_animation::tracking_line_system(&mut world);

    let path = world
        .query_filtered::<&gaanim_scene::PathSource, With<gaanim_animation::TrackingLine>>()
        .single(&world)
        .expect("one endpoint line");
    let elements = path.0.elements();
    assert!(matches!(
        elements.first(),
        Some(gaanim_core::kurbo::PathEl::MoveTo(point))
            if (point.x + 10.0).abs() < 1e-9 && (point.y + 20.0).abs() < 1e-9
    ));
    assert!(matches!(
        elements.last(),
        Some(gaanim_core::kurbo::PathEl::LineTo(point))
            if (point.x - 80.0).abs() < 1e-9 && (point.y - 60.0).abs() < 1e-9
    ));
}

/// A dashed line takes `line`'s endpoints and follows a moving one with
/// its dashes (#302); between fixed points it is a fixed dashed line.
#[test]
fn dashed_lines_follow_their_endpoints() {
    let mut canvas = SceneModel::new(320, 180);
    let fixed = canvas.dashed_line_between(
        CanvasEndpoint::Static(DVec3::ZERO),
        CanvasEndpoint::Static(DVec3::new(1.0, 0.0, 0.0)),
        0.2,
        0.1,
    );
    assert!(matches!(
        fixed.spec.lock().unwrap().kind,
        SpawnKind::DashedLine { .. }
    ));
    let reference = canvas.rect(10.0, 10.0).move_to(100.0, 0.0);
    canvas.dashed_line_between(
        CanvasEndpoint::Static(DVec3::ZERO),
        reference.anchor_point(Anchor::Center, DVec3::ZERO).into(),
        10.0,
        10.0,
    );

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    gaanim_animation::tracking_line_system(&mut world);

    let path = world
        .query_filtered::<&gaanim_scene::PathSource, With<gaanim_animation::TrackingLine>>()
        .single(&world)
        .expect("one dashed endpoint line");
    // 100 long in dashes of 10 every 20: five dashes.
    let dashes = path
        .0
        .elements()
        .iter()
        .filter(|element| matches!(element, gaanim_core::kurbo::PathEl::MoveTo(_)))
        .count();
    assert_eq!(dashes, 5);
}

/// `hidden()` keeps a drawable created after a play out of sight until
/// an animation shows it, and refuses one already shown (#301).
#[test]
fn hidden_waits_for_an_entrance() {
    let mut canvas = SceneModel::new(16.0, 9.0);
    let shown = canvas.circle(1.0);
    canvas.play(vec![shown.animate().shift_by(1.0, 0.0).duration(0.5)]);
    assert!(shown.hidden().is_err(), "already shown");
    let late = canvas.circle(0.5).hidden().unwrap();
    assert!(late.spec.lock().unwrap().defer_visibility_until_play);
}

#[test]
fn deferred_group_fade_in_reveals_deferred_children() {
    let mut canvas = SceneModel::new(320, 180);
    let anchor = canvas.dot(8.0).move_to(-60.0, 0.0);
    let child = canvas.dot(8.0);
    child.follow_to(&anchor, 0.0, 24.0);
    let group = canvas.group(&[&child]);
    canvas.play(vec![group.fade_in(1.0)]);

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_animation::PlaybackState::default());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    let opacity_for = |world: &mut World, id: gaanim_core::ObjectId| {
        let compiled_id = gaanim_core::ObjectId::from_raw(id.as_raw() - 1);
        let mut query = world.query::<(&gaanim_scene::MobjectId, &Opacity)>();
        query
            .iter(world)
            .find(|(object_id, _)| object_id.0 == compiled_id)
            .map(|(_, opacity)| opacity.0)
            .expect("group member should compile into a mobject")
    };

    assert_eq!(opacity_for(&mut world, group.id), 0.0);
    assert_eq!(opacity_for(&mut world, child.id), 0.0);

    let snapshot = WorldSnapshot::capture(&mut world);
    let mut timeline = world.remove_resource::<Timeline>().unwrap();
    timeline.add_keyframe(0.0, snapshot);
    timeline.seek(&mut world, 0.5);

    assert!(opacity_for(&mut world, group.id) > 0.0);
    assert!(opacity_for(&mut world, child.id) > 0.0);
}

#[test]
fn moving_a_group_does_not_reveal_unentered_deferred_children() {
    let mut canvas = SceneModel::new(320, 180);
    let anchor = canvas.dot(8.0).move_to(-60.0, 0.0);
    let mass = canvas.dot(8.0).move_to(60.0, 0.0);
    let spring = canvas.spring_between(
        CanvasEndpoint::Entity(anchor.id),
        CanvasEndpoint::Entity(mass.id),
        6,
        10.0,
    );
    let group = canvas.group(&[&anchor, &mass, &spring]);
    canvas.play(vec![group.animate().shift_by(40.0, 0.0).duration(1.0)]);
    canvas.play(vec![spring.fade_in(1.0)]);

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_animation::PlaybackState::default());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    let group_id = gaanim_core::ObjectId::from_raw(group.id.as_raw() - 1);
    let group_entity = world
        .query::<(bevy::prelude::Entity, &MobjectId)>()
        .iter(&world)
        .find_map(|(entity, id)| (id.0 == group_id).then_some(entity))
        .expect("compiled group");
    assert_eq!(
        world.get::<Opacity>(group_entity).unwrap().0,
        1.0,
        "a deferred child must not hide the group or its already-visible siblings"
    );

    let spring_id = gaanim_core::ObjectId::from_raw(spring.id.as_raw() - 1);
    let spring_entity = world
        .query::<(bevy::prelude::Entity, &MobjectId)>()
        .iter(&world)
        .find_map(|(entity, id)| (id.0 == spring_id).then_some(entity))
        .expect("compiled deferred spring");
    let snapshot = WorldSnapshot::capture(&mut world);
    let mut timeline = world.remove_resource::<Timeline>().unwrap();
    timeline.add_keyframe(0.0, snapshot);

    timeline.seek(&mut world, 0.5);
    assert_eq!(
        world.get::<Opacity>(spring_entity).unwrap().0,
        0.0,
        "the parent movement must not serve as the spring entry animation"
    );

    timeline.seek(&mut world, 1.5);
    let entered = world.get::<Opacity>(spring_entity).unwrap().0;
    assert!(entered > 0.0 && entered < 1.0);
}

#[test]
fn moving_a_group_to_a_target_carries_a_fixed_support_and_ordinary_member_together() {
    let mut canvas = SceneModel::new(320, 180);
    let support = canvas.support_at(
        CanvasEndpoint::Static(DVec3::new(-60.0, 40.0, 0.0)),
        "fixed",
        DVec3::Y,
        48.0,
        70.0,
        None,
    );
    let mass = canvas.dot(8.0).move_to(60.0, -40.0);
    let group = canvas.group(&[&support.drawable, &mass]);
    canvas.play(vec![group.animate().move_to(40.0, 0.0).duration(1.0)]);

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_animation::PlaybackState::default());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    let entity_for = |world: &mut World, id: gaanim_core::ObjectId| {
        let compiled_id = gaanim_core::ObjectId::from_raw(id.as_raw() - 1);
        world
            .query::<(bevy::prelude::Entity, &MobjectId)>()
            .iter(world)
            .find_map(|(entity, object_id)| (object_id.0 == compiled_id).then_some(entity))
            .expect("compiled group member")
    };
    let support_entity = entity_for(&mut world, support.drawable.id);
    let mass_entity = entity_for(&mut world, mass.id);
    let before_support = gaanim_animation::resolve_tracking_endpoint(
        &gaanim_animation::TrackingEndpoint::Entity(support_entity),
        &world,
    )
    .unwrap();
    let before_mass = gaanim_animation::resolve_tracking_endpoint(
        &gaanim_animation::TrackingEndpoint::Entity(mass_entity),
        &world,
    )
    .unwrap();

    let snapshot = WorldSnapshot::capture(&mut world);
    let mut timeline = world.remove_resource::<Timeline>().unwrap();
    timeline.add_keyframe(0.0, snapshot);
    timeline.seek(&mut world, 0.5);
    gaanim_animation::endpoint_follow_system(&mut world);

    let after_support = gaanim_animation::resolve_tracking_endpoint(
        &gaanim_animation::TrackingEndpoint::Entity(support_entity),
        &world,
    )
    .unwrap();
    let after_mass = gaanim_animation::resolve_tracking_endpoint(
        &gaanim_animation::TrackingEndpoint::Entity(mass_entity),
        &world,
    )
    .unwrap();
    assert!((after_support.x - before_support.x) > 1.0);
    assert_eq!(
        after_support - before_support,
        after_mass - before_mass,
        "fixed support and mass must share the parent movement"
    );
}

#[test]
fn writing_or_creating_a_group_keeps_updater_coordinates_and_reveals_deferred_members() {
    for use_write in [true, false] {
        let mut canvas = SceneModel::new(320, 180);
        let mass = canvas.dot(8.0).move_to(60.0, -40.0);
        mass.add_custom_updater(gaanim_animation::Updater::new(
            |_dt, _elapsed, entity, world| {
                if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
                    transform.translation = DVec3::new(60.0, -40.0, 0.0);
                }
                true
            },
        ));
        let trace = canvas.traced_path_with_options(&mass, Some(1.0), Some(100), 0.5);
        let group = canvas.group(&[&mass, &trace]);
        let entry = if use_write {
            group.write(1.0)
        } else {
            group.create(1.0)
        };
        canvas.play(vec![entry]);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_animation::PlaybackState::default());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let entity_for = |world: &mut World, id: gaanim_core::ObjectId| {
            let compiled_id = gaanim_core::ObjectId::from_raw(id.as_raw() - 1);
            world
                .query::<(bevy::prelude::Entity, &MobjectId)>()
                .iter(world)
                .find_map(|(entity, object_id)| (object_id.0 == compiled_id).then_some(entity))
                .expect("compiled group member")
        };
        let mass_entity = entity_for(&mut world, mass.id);
        let trace_entity = entity_for(&mut world, trace.id);
        let snapshot = WorldSnapshot::capture(&mut world);
        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(0.0, snapshot);
        timeline.seek(&mut world, 0.5);
        gaanim_animation::seek_updaters(&mut world, 0.5);

        let position = gaanim_animation::resolve_tracking_endpoint(
            &gaanim_animation::TrackingEndpoint::Entity(mass_entity),
            &world,
        )
        .unwrap();
        assert_eq!(position, DVec3::new(60.0, -40.0, 0.0));
        assert!(
            world.get::<Opacity>(trace_entity).unwrap().0 > 0.0,
            "Write/Create on the group is an explicit entry for deferred descendants"
        );
    }
}

#[test]
fn paper_theme_uses_dark_text_fills() {
    use gaanim_text::prelude::TextRole;

    let mut canvas = SceneModel::new(1280, 720);
    canvas
        .set_theme("paper")
        .expect("paper is a built-in theme");
    let config = canvas.themed_text_config();

    for role in [
        TextRole::Title,
        TextRole::Subtitle,
        TextRole::Heading,
        TextRole::Body,
        TextRole::Caption,
        TextRole::Label,
        TextRole::Math,
        TextRole::Code,
    ] {
        assert_eq!(config.roles[&role].fill_color, Color::BLACK);
    }
}

#[test]
fn direct_font_overrides_win_over_themes() {
    use gaanim_text::prelude::TextRole;

    let mut canvas = SceneModel::new(1280, 720);
    canvas
        .set_fonts(
            Some("Inter".into()),
            Some("STIX Two Math".into()),
            Some("JetBrains Mono".into()),
        )
        .unwrap();
    canvas.set_theme("paper").unwrap();

    let config = canvas.themed_text_config();
    assert_eq!(config.roles[&TextRole::Body].font_family, "Inter");
    assert_eq!(config.roles[&TextRole::Math].font_family, "STIX Two Math");
    assert_eq!(config.roles[&TextRole::Code].font_family, "JetBrains Mono");
}

#[test]
fn reactive_annotations_share_one_default_text_size() {
    let mut canvas = SceneModel::new(1920, 1080);
    let origin = CanvasEndpoint::Static(DVec3::ZERO);
    let tip = CanvasEndpoint::Static(DVec3::new(120.0, 0.0, 0.0));
    let angle = canvas
        .angle_between_with_options(
            origin.clone(),
            CanvasRay::Direction(DVec3::X),
            CanvasRay::Direction(DVec3::Y),
            64.0,
            AngleDimensionOptions {
                label: Some("$theta$".to_owned()),
                show_value: true,
                ..Default::default()
            },
        )
        .unwrap();
    let vector = canvas
        .vector_between_with_parts(
            origin,
            tip,
            Some("$F$".to_owned()),
            true,
            ".1f".to_owned(),
            Some("N".to_owned()),
            1.0,
            14.0,
            None,
            None,
        )
        .unwrap();

    for part in [
        angle.label.as_ref(),
        angle.unit.as_ref(),
        vector.label.as_ref(),
        vector.unit.as_ref(),
    ] {
        assert_eq!(
            part.expect("reactive annotation text")
                .text_spec()
                .expect("reactive annotation text spec")
                .style
                .size,
            Some(DEFAULT_REACTIVE_TEXT_SIZE)
        );
    }

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    let sizes = world
        .query::<&gaanim_animation::ReactiveReadout>()
        .iter(&world)
        .map(|readout| readout.font_size)
        .collect::<Vec<_>>();
    assert_eq!(sizes.len(), 2);
    assert!(sizes.iter().all(|size| *size == DEFAULT_REACTIVE_TEXT_SIZE));
}

#[test]
fn mechanism_create_and_write_start_with_every_reactive_path_hidden() {
    let mut canvas = SceneModel::new(640, 360);
    let pivot = CanvasEndpoint::Static(DVec3::new(-120.0, 80.0, 0.0));
    let support = canvas.support_at(pivot.clone(), "fixed", DVec3::NEG_Y, 48.0, 72.0, None);
    let angle = canvas
        .angle_between_with_options(
            pivot.clone(),
            CanvasRay::Direction(DVec3::NEG_Y),
            CanvasRay::Direction(DVec3::new(0.7, -0.7, 0.0)),
            64.0,
            AngleDimensionOptions {
                show_value: true,
                ..Default::default()
            },
        )
        .expect("valid angle");
    let force = canvas
        .force_at(
            pivot,
            ScalarSource::constant(981.0),
            ScalarSource::constant(-std::f64::consts::FRAC_PI_2),
            0.1,
            Some("$P$".to_owned()),
            true,
            ".0f".to_owned(),
            Some("N".to_owned()),
            40.0,
            None,
            None,
        )
        .expect("valid force");
    canvas.play(vec![
        support.drawable.write(1.0),
        angle.drawable.create(1.0),
        force.drawable.write(1.0),
    ]);

    let mut app = bevy::prelude::App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin);
    canvas.compile(app.world_mut());
    app.finish();
    app.cleanup();
    app.update();

    let draw_paths = app
        .world_mut()
        .query::<(&gaanim_animation::PathReveal, &gaanim_scene::Path2D)>()
        .iter(app.world())
        .filter(|(reveal, _)| reveal.0 == 0.0)
        .map(|(_, path)| path.0.is_empty())
        .collect::<Vec<_>>();
    assert!(draw_paths.len() > 3, "expected composite draw leaves");
    assert!(
        draw_paths.iter().all(|empty| *empty),
        "fixed support, angle, and force paths must not appear before their entry animation"
    );

    let readout_paths = app
        .world_mut()
        .query::<(&gaanim_scene::ObjectTag, &gaanim_scene::Path2D)>()
        .iter(app.world())
        .filter(|(tag, _)| tag.0 == "SvgPath#ReactiveReadout")
        .map(|(_, path)| path.0.is_empty())
        .collect::<Vec<_>>();
    assert_eq!(readout_paths.len(), 2);
    assert!(
        readout_paths.iter().all(|empty| *empty),
        "angle and force values must remain hidden until Create/Write reveals them"
    );
}

#[test]
fn unstyled_reactive_numbers_use_the_body_text_color() {
    let technical = crate::canvas::CanvasTheme::default_builtin()
        .palette
        .foreground;
    // Themed scenes use the theme body color; unthemed ones use the host
    // text configuration (white) whatever the background, like runtime replay.
    for (themed, background, expected) in [
        (true, None, technical),
        (false, None, Color::WHITE),
        (false, Some(Color::from_rgb8(9, 11, 23)), Color::WHITE),
    ] {
        let mut canvas = SceneModel::new(640, 360);
        if !themed {
            canvas.clear_theme();
        }
        canvas.set_background(background);
        canvas.reactive_readout(
            gaanim_animation::ScalarSource::Constant(3.5),
            ".2f",
            "",
            "",
            "invalid",
            None,
        );

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let fill = world
            .query::<(&gaanim_scene::ObjectTag, &gaanim_scene::FillBrush)>()
            .iter(&world)
            .find_map(|(tag, fill)| (tag.0 == "SvgPath#ReactiveReadout").then_some(fill))
            .expect("compiled readout number");
        assert!(
            matches!(fill.0.as_ref(), Some(Brush::Solid(color)) if *color == expected),
            "{:?} on {background:?}",
            fill.0
        );
    }
}

#[test]
fn angle_color_applies_to_the_reactive_numeric_value() {
    let mut canvas = SceneModel::new(640, 360);
    let gold = Color::from_rgb8(255, 200, 0);
    let angle = canvas
        .angle_between_with_options(
            CanvasEndpoint::Static(DVec3::ZERO),
            CanvasRay::Direction(DVec3::X),
            CanvasRay::Direction(DVec3::Y),
            64.0,
            AngleDimensionOptions {
                label: Some("$theta$".to_owned()),
                show_value: true,
                color: Some(gold),
                ..Default::default()
            },
        )
        .unwrap();
    for text_part in [angle.label.as_ref(), angle.unit.as_ref()] {
        assert_eq!(
            text_part
                .expect("angle annotation text")
                .text_spec()
                .expect("angle annotation text spec")
                .style
                .color,
            Some(gold),
            "the explicit angle color must override the theme for symbols and units"
        );
    }
    let _number = angle.number.as_ref().expect("numeric angle readout");

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    let mut style_schedule = bevy::prelude::Schedule::default();
    style_schedule.add_systems(gaanim_scene::systems::style_propagation_system);
    style_schedule.run(&mut world);

    let fill = world
        .query::<(&gaanim_scene::ObjectTag, &gaanim_scene::FillBrush)>()
        .iter(&world)
        .find_map(|(tag, fill)| (tag.0 == "SvgPath#ReactiveReadout").then_some(fill))
        .expect("compiled angle number fill");
    assert!(matches!(
        fill.0.as_ref(),
        Some(Brush::Solid(color)) if *color == gold
    ));

    let snapshot = WorldSnapshot::capture(&mut world);
    let mut timeline = world.remove_resource::<Timeline>().unwrap();
    timeline.add_keyframe(0.0, snapshot);
    timeline.seek(&mut world, 0.0);
    let fill_after_seek = world
        .query::<(&gaanim_scene::ObjectTag, &gaanim_scene::FillBrush)>()
        .iter(&world)
        .find_map(|(tag, fill)| (tag.0 == "SvgPath#ReactiveReadout").then_some(fill))
        .expect("angle number fill after seek");
    assert!(matches!(
        fill_after_seek.0.as_ref(),
        Some(Brush::Solid(color)) if *color == gold
    ));
}

#[test]
fn force_color_applies_to_the_reactive_numeric_value_after_fade_and_seek() {
    let mut canvas = SceneModel::new(640, 360);
    canvas
        .set_theme("technical")
        .expect("technical is a built-in theme");
    let green = Color::from_rgb8(75, 229, 124);
    let force = canvas
        .force_at(
            CanvasEndpoint::Static(DVec3::ZERO),
            ScalarSource::constant(45.0),
            ScalarSource::constant(0.4),
            2.0,
            Some("$F$".to_owned()),
            true,
            ".1f".to_owned(),
            Some("N".to_owned()),
            28.0,
            None,
            Some(green),
        )
        .expect("valid force");
    canvas.play(vec![force.drawable.fade_in(1.0)]);

    let mut app = bevy::prelude::App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin);
    canvas.compile(app.world_mut());
    app.finish();
    app.cleanup();
    app.update();
    app.world_mut().resource_mut::<Timeline>().seek_request = Some(1.0);
    app.update();

    let (fill, stroke) = app
        .world_mut()
        .query::<(
            &gaanim_scene::ObjectTag,
            &gaanim_scene::FillBrush,
            &gaanim_scene::StrokeBrush,
        )>()
        .iter(app.world())
        .find_map(|(tag, fill, stroke)| {
            (tag.0 == "SvgPath#ReactiveReadout").then_some((fill, stroke))
        })
        .expect("compiled force number style after seek");
    assert!(matches!(
        fill.0.as_ref(),
        Some(Brush::Solid(color)) if *color == green
    ));
    assert!(
        stroke.brush.is_none(),
        "a reactive number is filled text and must not inherit the plot stroke"
    );
}

#[test]
fn presentation_theme_uses_projector_contrast() {
    use gaanim_text::prelude::TextRole;

    let mut canvas = SceneModel::new(1920, 1080);
    canvas
        .set_theme("presentation")
        .expect("presentation is a built-in theme");
    let config = canvas.themed_text_config();

    assert_eq!(canvas.background, Some(Color::from_rgb8(0x12, 0x12, 0x12)));
    assert_eq!(
        config.roles[&TextRole::Title].fill_color,
        Color::from_rgb8(0xFF, 0xD1, 0x66)
    );
    assert_eq!(
        config.roles[&TextRole::Body].fill_color,
        Color::from_rgb8(0xF2, 0xF2, 0xF2)
    );
}

#[test]
fn known_color_scheme_drives_text_and_components_from_one_palette() {
    use gaanim_text::prelude::TextRole;

    let mut canvas = SceneModel::new(1920, 1080);
    canvas.set_theme("dracula").expect("dracula is built in");
    let theme = canvas.theme_style.as_ref().unwrap();

    assert_eq!(canvas.theme.as_deref(), Some("dracula"));
    assert_eq!(canvas.background, Some(Color::from_rgb8(0x28, 0x2A, 0x36)));
    assert_eq!(
        theme.text.roles[&TextRole::Title].fill_color,
        theme.palette.title
    );
    assert_eq!(
        theme.text.roles[&TextRole::Body].fill_color,
        theme.palette.foreground
    );
}

#[test]
fn explicit_background_survives_later_theme_changes() {
    let explicit = Color::from_rgb8(0x12, 0x34, 0x56);
    let mut canvas = SceneModel::new(640, 360);
    canvas.set_background(Some(explicit));
    canvas.set_theme("paper").unwrap();
    assert_eq!(canvas.background, Some(explicit));
}

#[test]
fn gradient_background_keeps_a_representative_color_for_theme_contrast() {
    let navy = Color::from_rgb8(0x0B, 0x10, 0x20);
    let blue = Color::from_rgb8(0x25, 0x63, 0xEB);
    let gradient = gaanim_core::peniko::Gradient::new_linear((-320.0, 0.0), (320.0, 0.0))
        .with_stops([(0.0, navy), (1.0, blue)]);
    let mut canvas =
        SceneModel::new(640, 360).background_brush(gaanim_core::peniko::Brush::Gradient(gradient));

    assert_eq!(canvas.background, Some(navy));
    assert!(matches!(
        canvas.background_paint,
        Some(gaanim_renderer::background::BackgroundPaint::Brush(
            gaanim_core::peniko::Brush::Gradient(_)
        ))
    ));

    canvas.set_theme("paper").unwrap();
    assert_eq!(canvas.background, Some(navy));
}

#[test]
fn theme_classes_propagate_through_nested_groups() {
    let mut canvas = SceneModel::new(640, 360);
    let first = canvas.circle(20.0);
    let second = canvas.square(30.0);
    let inner = canvas.group(&[&first, &second]);
    let outer = canvas.group(&[&inner]);
    outer.style_class("focus").unwrap();
    assert_eq!(
        first
            .spec
            .lock()
            .expect("first spec poisoned")
            .style_classes,
        ["focus"]
    );
    assert_eq!(
        second
            .spec
            .lock()
            .expect("second spec poisoned")
            .style_classes,
        ["focus"]
    );
}

#[test]
fn derived_theme_can_override_semantic_colors_and_fonts() {
    use std::collections::HashMap;

    use gaanim_text::prelude::TextRole;

    let mut theme = CanvasTheme::builtin("nord").unwrap();
    theme.name = "research-lab".into();
    theme
        .set_colors(&HashMap::from([(
            "accent".into(),
            Color::from_rgb8(0xFF, 0x00, 0x66),
        )]))
        .unwrap();
    theme
        .set_fonts(&HashMap::from([("text".into(), "Inter".into())]))
        .unwrap();

    assert_eq!(theme.palette.accent, Color::from_rgb8(0xFF, 0x00, 0x66));
    for role in [
        TextRole::Title,
        TextRole::Subtitle,
        TextRole::Heading,
        TextRole::Body,
        TextRole::Caption,
        TextRole::Label,
    ] {
        assert_eq!(theme.text.roles[&role].font_family, "Inter");
    }
    assert_ne!(theme.text.roles[&TextRole::Math].font_family, "Inter");
}

#[test]
fn theme_tokens_are_queryable_and_low_contrast_is_reported() {
    use std::collections::HashMap;

    let mut theme = CanvasTheme::builtin("presentation").unwrap();
    assert_eq!(theme.color("border").unwrap(), theme.palette.rule);
    assert_eq!(theme.color("primary").unwrap(), theme.palette.foreground);
    assert!(theme.validate().is_empty());

    theme
        .set_colors(&HashMap::from([(
            "foreground".into(),
            theme.palette.background,
        )]))
        .unwrap();
    assert!(
        theme
            .validate()
            .iter()
            .any(|warning| warning.contains("foreground on background"))
    );
}

#[test]
fn svg_parts_are_addressable_and_group_styles_reach_descendant_paths() {
    let temp = std::env::temp_dir().join(format!(
        "gaanim_svg_parts_api_test_{}.svg",
        std::process::id()
    ));
    std::fs::write(
        &temp,
        r##"<svg width="80" height="40" xmlns="http://www.w3.org/2000/svg">
                <g id="assembly">
                  <rect id="body" width="30" height="20" fill="#0000ff"/>
                  <circle id="joint" cx="50" cy="20" r="8" fill="#00ff00"/>
                </g>
              </svg>"##,
    )
    .unwrap();

    let mut canvas = SceneModel::new(320, 180);
    let svg = canvas.svg(&temp).unwrap();
    std::fs::remove_file(temp).unwrap();

    assert_eq!(
        canvas
            .state
            .lock()
            .expect("canvas state poisoned")
            .all_drawables
            .len(),
        1,
        "only the SVG root should be a top-level drawable"
    );

    let red = Color::from_rgb8(255, 0, 0);
    svg.part("assembly").unwrap().fill(red);
    for id in ["body", "joint"] {
        let part = svg.part(id).unwrap();
        let spec = part.spec.lock().expect("SVG path spec poisoned");
        assert!(spec.fill_overridden);
        assert!(matches!(
            spec.fill,
            Some(gaanim_core::peniko::Brush::Solid(color)) if color == red
        ));
    }

    assert!(matches!(
        svg.part("missing"),
        Err(crate::canvas::SvgPartError::Unknown { available, .. })
            if available == "assembly, body, joint"
    ));
    assert!(matches!(
        canvas.circle(10.0).part("body"),
        Err(crate::canvas::SvgPartError::NotSvg)
    ));
}

#[test]
fn scaled_svg_group_stroke_remains_in_logical_scene_units() {
    let temp = std::env::temp_dir().join(format!(
        "gaanim_svg_logical_stroke_test_{}.svg",
        std::process::id()
    ));
    std::fs::write(
        &temp,
        r##"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
                <rect id="body" x="10" y="10" width="80" height="80" fill="#ffffff"/>
              </svg>"##,
    )
    .unwrap();

    let mut canvas = SceneModel::new(16.0, 9.0);
    let svg = canvas
        .svg(&temp)
        .unwrap()
        .scale_to(0.01)
        .stroke(Color::BLACK, 0.025);
    canvas.play(vec![
        svg.animate()
            .stroke(Color::from_rgb8(0x80, 0x80, 0x80), 0.0167)
            .duration(1.0),
    ]);
    std::fs::remove_file(temp).unwrap();

    let mut app = bevy::prelude::App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin);
    canvas.compile(app.world_mut());
    app.finish();
    app.cleanup();
    app.update();
    // Strokes are drawn in scene units whatever the drawable's scale.
    let effective_width = |app: &mut bevy::prelude::App| {
        app.world_mut()
            .query::<(&gaanim_scene::ObjectTag, &gaanim_scene::StrokeBrush)>()
            .iter(app.world())
            .find_map(|(tag, stroke)| (tag.0 == "SvgPath#body").then_some(stroke.style.width))
            .expect("compiled SVG body")
    };
    // The declared stroke and the animated one are both in scene units.
    app.world_mut().resource_mut::<Timeline>().seek_request = Some(0.0);
    app.update();
    let declared = effective_width(&mut app);
    assert!((declared - 0.025).abs() < 1.0e-9, "{declared}");
    app.world_mut().resource_mut::<Timeline>().seek_request = Some(1.0);
    app.update();
    let animated = effective_width(&mut app);
    assert!((animated - 0.0167).abs() < 1.0e-9, "{animated}");
}

#[test]
fn moving_an_svg_part_does_not_move_its_root() {
    let temp = std::env::temp_dir().join(format!(
        "gaanim_svg_part_layout_test_{}.svg",
        std::process::id()
    ));
    std::fs::write(
        &temp,
        r##"<svg width="300" height="100" xmlns="http://www.w3.org/2000/svg">
                <rect id="g" x="0" y="0" width="100" height="100" fill="#ffffff"/>
                <rect id="a" x="200" y="0" width="100" height="100" fill="#ffffff"/>
              </svg>"##,
    )
    .unwrap();
    let place = |shift: bool| {
        let mut canvas = SceneModel::new(16.0, 9.0);
        let svg = canvas.svg(&temp).unwrap().move_to_default(1.0, 2.0);
        if shift {
            let _ = svg.part("g").unwrap().shift_by(0.0, -0.45);
        }
        let mut app = bevy::prelude::App::new();
        app.add_plugins(bevy::prelude::MinimalPlugins)
            .add_plugins(gaanim_scene::GaanimScenePlugin)
            .add_plugins(gaanim_animation::GaanimAnimationPlugin)
            .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
            .add_plugins(gaanim_text::GaanimTextPlugin);
        canvas.compile(app.world_mut());
        app.finish();
        app.cleanup();
        app.update();
        let mut positions = std::collections::HashMap::new();
        for (tag, transform) in app
            .world_mut()
            .query::<(
                &gaanim_scene::ObjectTag,
                &gaanim_math::GlobalSpatialTransform,
            )>()
            .iter(app.world())
        {
            let [_, _, _, _, x, y] = transform.affine_2d.as_coeffs();
            positions.insert(tag.0.clone(), (x, y));
        }
        positions
    };
    let still = place(false);
    let shifted = place(true);
    std::fs::remove_file(temp).unwrap();
    // The other part, and so the root, stays where `move_to` put it.
    let (ax, ay) = still["SvgPath#a"];
    let (bx, by) = shifted["SvgPath#a"];
    assert!(
        (ax - bx).abs() < 1e-9 && (ay - by).abs() < 1e-9,
        "{still:?} {shifted:?}"
    );
    // The shifted part moves by exactly its own offset.
    let (gx, gy) = still["SvgPath#g"];
    let (hx, hy) = shifted["SvgPath#g"];
    assert!((gx - hx).abs() < 1e-9 && (hy - (gy - 0.45)).abs() < 1e-9);
}

#[test]
fn scaled_svg_part_strokes_are_in_scene_units() {
    let temp = std::env::temp_dir().join(format!(
        "gaanim_svg_part_stroke_test_{}.svg",
        std::process::id()
    ));
    std::fs::write(
            &temp,
            r##"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
                <rect id="body" x="10" y="10" width="80" height="80" fill="#ffffff" stroke="#000000" stroke-width="4"/>
                <rect id="lid" x="10" y="0" width="80" height="10" fill="#ffffff" stroke="#000000" stroke-width="4"/>
              </svg>"##,
        )
        .unwrap();

    let mut canvas = SceneModel::new(16.0, 9.0);
    let svg = canvas.svg(&temp).unwrap().scale_to(0.5);
    svg.part("body").unwrap().stroke(Color::BLACK, 0.02);
    std::fs::remove_file(temp).unwrap();

    let mut app = bevy::prelude::App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin);
    canvas.compile(app.world_mut());
    app.finish();
    app.cleanup();
    app.update();

    // Strokes are drawn in scene units whatever the drawable's scale.
    let mut widths = std::collections::HashMap::new();
    for (tag, stroke) in app
        .world_mut()
        .query::<(&gaanim_scene::ObjectTag, &gaanim_scene::StrokeBrush)>()
        .iter(app.world())
    {
        widths.insert(tag.0.clone(), stroke.style.width);
    }
    // The width set on the part is in scene units; the file's own stroke
    // (4 px = 0.04 units) scales with the drawing.
    assert!((widths["SvgPath#body"] - 0.02).abs() < 1.0e-9, "{widths:?}");
    assert!((widths["SvgPath#lid"] - 0.02).abs() < 1.0e-9, "{widths:?}");
}

#[test]
fn scaled_svg_clip_mask_inherits_the_visible_hierarchy_transform() {
    let temp = std::env::temp_dir().join(format!(
        "gaanim_svg_scaled_clip_api_test_{}.svg",
        std::process::id()
    ));
    std::fs::write(
            &temp,
            r##"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
                <defs><clipPath id="window"><rect x="10" y="10" width="80" height="80"/></clipPath></defs>
                <g clip-path="url(#window)"><rect width="100" height="100" fill="#00aaff"/></g>
              </svg>"##,
        )
        .unwrap();

    let mut canvas = SceneModel::new(320, 180);
    canvas.svg(&temp).unwrap().scale_to(2.0);
    std::fs::remove_file(temp).unwrap();

    let mut app = bevy::prelude::App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin);
    canvas.compile(app.world_mut());
    app.finish();
    app.cleanup();
    app.update();

    let (target_scale, source) = app
        .world_mut()
        .query::<(
            &gaanim_renderer::effects::ClipMask,
            &gaanim_math::GlobalSpatialTransform,
        )>()
        .iter(app.world())
        .find_map(|(mask, target)| mask.sources.first().copied().map(|source| (target, source)))
        .expect("compiled SVG clip mask and source");
    let source_scale = app
        .world()
        .get::<gaanim_math::GlobalSpatialTransform>(source)
        .expect("clip source transform");

    let target_coeffs = target_scale.affine_2d.as_coeffs();
    let source_coeffs = source_scale.affine_2d.as_coeffs();
    assert!((target_coeffs[0].abs() - source_coeffs[0].abs()).abs() < 1e-9);
    assert!((target_coeffs[3].abs() - source_coeffs[3].abs()).abs() < 1e-9);
}

#[test]
fn svg_clip_mask_is_not_consumed_by_group_create_animation() {
    let temp = std::env::temp_dir().join(format!(
        "gaanim_svg_create_clip_api_test_{}.svg",
        std::process::id()
    ));
    std::fs::write(
            &temp,
            r##"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
                <defs><clipPath id="window"><rect x="10" y="10" width="80" height="80"/></clipPath></defs>
                <g clip-path="url(#window)"><rect width="100" height="100" fill="#00aaff"/></g>
              </svg>"##,
        )
        .unwrap();

    let mut canvas = SceneModel::new(320, 180);
    let svg = canvas.svg(&temp).unwrap();
    canvas.play(vec![svg.create(1.0)]);
    std::fs::remove_file(temp).unwrap();

    let mut world = bevy::prelude::World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_animation::PlaybackState::default());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    let source = world
        .query::<&gaanim_renderer::effects::ClipMask>()
        .iter(&world)
        .find_map(|mask| mask.sources.first().copied())
        .expect("compiled SVG clip source");
    let source_path = world
        .get::<gaanim_scene::components::Path2D>(source)
        .expect("clip source path");
    assert!(!source_path.0.elements().is_empty());
    assert!(world.get::<gaanim_animation::PathReveal>(source).is_none());
    assert!(
        world
            .get::<gaanim_animation::FillDrawProgress>(source)
            .is_none()
    );
}

#[test]
fn tempo_times_beats_and_rejects_a_beat_behind_the_cursor() {
    let mut canvas = SceneModel::new(1280, 720);
    assert!(canvas.beats(1.0).is_err());
    assert!(canvas.set_tempo(0.0, 0.0, 4).is_err());
    canvas.set_tempo(120.0, 0.5, 4).unwrap();
    assert!((canvas.beats(2.0).unwrap() - 1.0).abs() < 1e-12);
    canvas.wait_until_beat(4.0).unwrap();
    assert!((canvas.current_time() - 2.5).abs() < 1e-9);
    canvas.wait(1.0);
    let error = canvas.wait_until_beat(5.0).unwrap_err();
    assert!(error.contains("too long"), "{error}");
    canvas.wait_until_beat(8.0).unwrap();
    assert!((canvas.current_time() - 4.5).abs() < 1e-9);
    let grid = canvas.tempo().unwrap();
    assert_eq!(grid.bar_times(4.5, 100), vec![0.5, 2.5, 4.5]);
}

#[test]
fn wait_until_time_reaches_an_absolute_instant_or_says_how_late_it_is() {
    let mut canvas = SceneModel::new(1280, 720);
    canvas.wait_until_time(1.5).unwrap();
    assert!((canvas.current_time() - 1.5).abs() < 1e-9);
    canvas.wait_until_time(1.5).unwrap();
    assert!((canvas.current_time() - 1.5).abs() < 1e-9);
    canvas.wait(1.0);
    let error = canvas.wait_until_time(2.0).unwrap_err();
    assert!(error.contains("0.500 s too long"), "{error}");
    assert!(canvas.wait_until_time(f64::NAN).is_err());
    assert!((canvas.current_time() - 2.5).abs() < 1e-9);
}

#[test]
fn launch_schedules_at_the_cursor_without_moving_it() {
    let mut canvas = SceneModel::new(1280, 720);
    let wheel = canvas.circle(20.0);
    let label = canvas.circle(10.0);
    canvas.wait(1.0);
    canvas
        .launch_composition_configured(
            Composition::leaf(wheel.animate().rotate_by(6.0).duration(8.0)),
            None,
            None,
        )
        .unwrap();
    assert!((canvas.current_time() - 1.0).abs() < 1e-9);
    canvas
        .play_composition_configured(Composition::leaf(label.fade_in(1.0)), None, None)
        .unwrap();
    assert!((canvas.current_time() - 2.0).abs() < 1e-9);
    let guard = canvas.state.lock().expect("canvas state poisoned");
    let ops = &guard.active().ops;
    assert!(
        ops.iter()
            .any(|op| matches!(op, Op::Launch(anims) if anims.len() == 1))
    );
}

#[test]
fn a_transform_can_fade_while_it_morphs() {
    // transform_to keeps the source's own opacity, so it leaves that
    // channel free for a fade (issue #421).
    let mut canvas = SceneModel::new(1280, 720);
    let shape = canvas.circle(20.0);
    let template = canvas.square(30.0);
    canvas
        .play_items(vec![
            shape
                .animate()
                .transform_to(&template)
                .unwrap()
                .duration(0.3)
                .into(),
            shape
                .animate()
                .opacity(0.0)
                .duration(0.15)
                .delay(0.15)
                .into(),
        ])
        .unwrap();
    // A move does share a channel with it, and since an effect cannot
    // join property targets, the error must not suggest merging them.
    let error = canvas
        .play_items(vec![
            shape
                .animate()
                .transform_to(&template)
                .unwrap()
                .duration(0.3)
                .into(),
            shape.animate().shift_by(1.0, 0.0).duration(0.3).into(),
        ])
        .unwrap_err();
    assert!(
        matches!(&error, PlayError::ConflictingChannel { channel, combinable: false, .. }
                if channel == "translation"),
        "{error}"
    );
    assert!(!error.to_string().contains("combine"), "{error}");
    let error = canvas
        .play_items(vec![
            shape.animate().shift_by(1.0, 0.0).duration(0.3).into(),
            shape.animate().shift_by(0.0, 1.0).duration(0.3).into(),
        ])
        .unwrap_err();
    assert!(
        error
            .to_string()
            .ends_with("or combine them into one animation"),
        "{error}"
    );
}

#[test]
fn a_play_cannot_drive_a_channel_a_launch_still_drives() {
    let mut canvas = SceneModel::new(1280, 720);
    let wheel = canvas.circle(20.0);
    canvas
        .launch_composition_configured(
            Composition::leaf(wheel.animate().rotate_by(6.0).duration(2.0)),
            None,
            None,
        )
        .unwrap();
    // Another channel of the same object runs under it.
    canvas
        .play_composition_configured(
            Composition::leaf(wheel.animate().shift_by(10.0, 0.0).duration(1.0)),
            None,
            None,
        )
        .unwrap();
    let error = canvas
        .play_composition_configured(
            Composition::leaf(wheel.animate().rotate_by(1.0).duration(0.5)),
            None,
            None,
        )
        .unwrap_err();
    assert!(
        matches!(&error, PlayError::ConflictingChannel { channel, first, .. }
                if channel == "rotation" && first.contains("launched at 0.00 s (until 2.00 s)")),
        "{error}"
    );
    assert!(
        (canvas.current_time() - 1.0).abs() < 1e-9,
        "a refused play keeps the cursor"
    );
    // A second launch on the channel is refused too, and once the first
    // one ends the channel is free.
    assert!(
        canvas
            .launch_composition_configured(
                Composition::leaf(wheel.animate().rotate_by(1.0).duration(0.5)),
                None,
                None,
            )
            .is_err()
    );
    canvas.wait(1.0);
    canvas
        .play_composition_configured(
            Composition::leaf(wheel.animate().rotate_by(1.0).duration(0.5)),
            None,
            None,
        )
        .unwrap();
}

#[test]
fn a_launch_past_the_end_of_the_scene_is_reported() {
    let mut canvas = SceneModel::new(1280, 720);
    let wheel = canvas.circle(20.0);
    canvas
        .launch_composition_configured(
            Composition::leaf(wheel.animate().rotate_by(6.0).duration(3.0)),
            None,
            None,
        )
        .unwrap();
    canvas.wait(1.0);
    let warning = canvas.launched_past_end_warning().expect("cut launch");
    assert!(
        warning.contains("until 3.00 s") && warning.contains("scene.wait(2.00)"),
        "{warning}"
    );
    canvas.wait(2.0);
    assert_eq!(canvas.launched_past_end_warning(), None);
}

#[test]
fn thumbnail_takes_the_cursor_or_an_instant() {
    let mut canvas = SceneModel::new(1280, 720);
    assert_eq!(canvas.thumbnail_time(), None);
    canvas.wait(2.5);
    canvas.set_thumbnail(None).unwrap();
    assert_eq!(canvas.thumbnail_time(), Some(2.5));
    canvas.set_thumbnail(Some(0.75)).unwrap();
    assert_eq!(canvas.thumbnail_time(), Some(0.75));
    assert!(canvas.set_thumbnail(Some(-1.0)).is_err());
    assert!(canvas.set_thumbnail(Some(f64::NAN)).is_err());
    assert_eq!(canvas.thumbnail_time(), Some(0.75));
}

#[test]
fn stagger_offsets_delays_and_cursor() {
    let mut canvas = SceneModel::new(1280, 720);
    let first = canvas.circle(20.0);
    let second = canvas.circle(20.0);

    let plan = Composition::stagger(
        vec![
            Composition::leaf(first.fade_in(1.0)),
            Composition::leaf(second.fade_in(1.0)),
        ],
        0.25,
    )
    .unwrap();
    canvas
        .play_composition_configured(plan, None, None)
        .unwrap();

    let guard = canvas.state.lock().expect("canvas state poisoned");
    let segment = guard.active();
    assert!((segment.cursor - 1.25).abs() < 1e-9);

    let Some(Op::Play(anims)) = segment.ops.last() else {
        panic!("expected parallel play op");
    };
    assert_eq!(anims.len(), 2);
    assert!((anims[0].delay - 0.0).abs() < 1e-9);
    assert!((anims[1].delay - 0.25).abs() < 1e-9);
}

#[test]
fn compound_property_animation_stays_pure_and_schedules_as_one_anim() {
    let mut canvas = SceneModel::new(1280, 720);
    let shape = canvas.circle(20.0).fill(Color::WHITE);

    let pending = shape.animate().duration(2.0);
    {
        let guard = canvas.state.lock().expect("canvas state poisoned");
        assert_eq!(guard.active().cursor, 0.0);
    }

    let animation = pending
        .move_to_3d(12.0, 24.0, 3.0)
        .scale_to_3d(2.0, 3.0, 4.0)
        .fill(Color::from_rgb8(20, 40, 220))
        .stroke(Color::WHITE, 5.0)
        .opacity(0.6);
    {
        let guard = canvas.state.lock().expect("canvas state poisoned");
        assert_eq!(guard.active().cursor, 0.0);
        let AnimationType::Properties(properties) = &animation.inner.anim_type else {
            panic!("expected typed property animation");
        };
        assert!(properties.translation.is_some());
        assert!(properties.scale.is_some());
        assert!(properties.fill.is_some());
        assert!(properties.stroke_color.is_some());
        assert_eq!(properties.stroke_width, Some(5.0));
        assert_eq!(properties.opacity, Some(0.6));
    }

    canvas.play(vec![animation]);
    let guard = canvas.state.lock().expect("canvas state poisoned");
    assert_eq!(guard.active().cursor, 2.0);
    let Some(Op::Play(anims)) = guard.active().ops.last() else {
        panic!("scene.play should regroup the compound animation");
    };
    assert_eq!(anims.len(), 1);
    assert!(matches!(anims[0].anim_type, AnimationType::Properties(_)));
}

#[test]
fn move_to_anchor_places_the_requested_anchor_at_the_target() {
    let mut canvas = SceneModel::new(640, 360);
    let rect = canvas.rect(100.0, 60.0).move_to(-120.0, 0.0);
    canvas.play(vec![
        rect.move_to_anchor(80.0, 40.0, Anchor::TopRight)
            .duration(1.0),
    ]);

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    let entity = world
        .query::<(bevy::prelude::Entity, &MobjectId)>()
        .iter(&world)
        .find_map(|(entity, id)| {
            (id.0 == gaanim_core::ObjectId::from_raw(rect.id.as_raw() - 1)).then_some(entity)
        })
        .expect("rectangle entity");
    let mut timeline = world.remove_resource::<Timeline>().expect("timeline");
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    timeline.seek(&mut world, 1.0);

    let translation = world
        .get::<SpatialTransform>(entity)
        .expect("rectangle transform")
        .translation;
    assert_eq!(translation, gaanim_core::glam::DVec3::new(30.0, 10.0, 0.0));
}

#[test]
fn move_to_anchor_point_animates_center_to_reference_anchor() {
    let mut canvas = SceneModel::new(640, 360);
    let reference = canvas.rect(100.0, 40.0).move_to(50.0, 20.0);
    let moving = canvas.rect(10.0, 6.0).move_to(-100.0, -80.0);
    let point = reference.anchor_point(
        Anchor::TopRight,
        gaanim_core::glam::DVec3::new(-5.0, -3.0, 0.0),
    );
    canvas.play(vec![
        moving
            .animate()
            .move_to_anchor_point(point)
            .unwrap()
            .duration(1.0),
    ]);

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    let mut timeline = world.remove_resource::<Timeline>().expect("timeline");
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    timeline.seek(&mut world, 1.0);

    let mut roots = world
        .query::<(
            &gaanim_scene::LocalBounds,
            &SpatialTransform,
            Option<&bevy::prelude::ChildOf>,
        )>()
        .iter(&world)
        .filter_map(|(bounds, transform, parent)| {
            parent.is_none().then_some((bounds.0, *transform))
        })
        .collect::<Vec<_>>();
    roots.sort_by(|(left, _), (right, _)| left.width().total_cmp(&right.width()));
    let (moving_bounds, moving_transform) = roots[0];
    let (reference_bounds, reference_transform) = roots[1];
    let expected = reference_transform.to_mat4().transform_point3(
        Anchor::TopRight.get_point(&reference_bounds)
            + gaanim_core::glam::DVec3::new(-5.0, -3.0, 0.0),
    );
    let actual = moving_transform
        .to_mat4()
        .transform_point3(moving_bounds.center());
    assert!(
        actual.distance(expected) < 1e-6,
        "expected {expected:?}, got {actual:?}"
    );
}

#[test]
fn meshes_built_from_points_or_outlines_sit_at_their_center() {
    let mut canvas = SceneModel::new(640, 360);
    let material = gaanim_scene::Material3D::default();
    canvas
        .arrow_3d(
            [1.0, 2.0, 3.0],
            [3.0, 2.0, -1.0],
            primitives3d::ArrowSizes::default(),
            8,
            material,
        )
        .expect("valid arrow");
    let path = [[-4.0, 0.0, 0.0], [0.0, 2.0, 0.0], [0.0, 2.0, 6.0]];
    canvas
        .tube(&path, 0.1, 8, false, true, material)
        .expect("valid tube");
    let square = canvas.rect(2.0, 2.0).move_to(5.0, -1.0);
    canvas
        .flat_3d(&square, 0.01, None, false)
        .expect("a square lies flat");
    assert!(canvas.flat_3d(&square, 0.0, None, false).is_err());
    assert!(
        canvas
            .tube(&path[..1], 0.1, 8, false, true, material)
            .is_err()
    );

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    let mut timeline = world.remove_resource::<Timeline>().expect("timeline");
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    timeline.seek(&mut world, 0.0);

    let mut meshes = world
        .query::<(&gaanim_scene::TriangleMeshData, &SpatialTransform)>()
        .iter(&world)
        .map(|(mesh, transform)| (transform.translation, mesh.clone()))
        .collect::<Vec<_>>();
    meshes.sort_by(|(a, _), (b, _)| a.x.total_cmp(&b.x));
    let centers: Vec<DVec3> = meshes.iter().map(|(center, _)| *center).collect();
    assert_eq!(
        centers,
        [
            DVec3::new(-2.0, 1.0, 3.0),
            DVec3::new(2.0, 2.0, 1.0),
            DVec3::new(5.0, -1.0, 0.0),
        ]
    );
    assert!(meshes.iter().all(|(_, mesh)| !mesh.indices.is_empty()));
    // The sheet lies in the plane it was drawn in, around its center.
    let sheet = &meshes[2].1;
    assert!(
        sheet
            .vertices
            .iter()
            .all(|v| v[2] == 0.0 && v[0].abs() <= 1.0 + 1e-6 && v[1].abs() <= 1.0 + 1e-6)
    );
}

#[test]
fn primitive_3d_color_property_preserves_other_material_channels() {
    let mut canvas = SceneModel::new(640, 360);
    let original = gaanim_scene::Material3D::metal(Color::WHITE);
    let cube = canvas.cube(2.0, original).expect("valid cube");
    let target_color = Color::from_rgb8(32, 96, 224);

    let animation = cube
        .animate()
        .color(target_color)
        .rotate_to_3d(0.2, 0.4, 0.6);
    let AnimationType::Properties(properties) = &animation.inner.anim_type else {
        panic!("expected typed property animation");
    };
    let Some((from, to)) = properties.material else {
        panic!("Primitive3D color should target Material3D");
    };
    assert_eq!(from, original);
    assert_eq!(to.color, target_color);
    assert_eq!(to.roughness, original.roughness);
    assert_eq!(to.metallic, original.metallic);
    assert_eq!(to.emissive, original.emissive);
    assert_eq!(to.emissive_strength, original.emissive_strength);
    assert!(properties.rotation.is_some());
    assert!(properties.fill.is_none());
}

#[test]
fn camera_animation_can_be_regrouped_with_drawables() {
    let mut canvas = SceneModel::new(1280, 720);
    let marker = canvas.circle(20.0);
    let marker_anim = marker.fade_in(2.0);
    let camera_anim = canvas
        .camera_orbit(0.5, 0.1, 2.0)
        .rate_func(gaanim_math::RateFunc::Linear)
        .delay(0.25);

    canvas.play(vec![marker_anim, camera_anim]);

    let guard = canvas.state.lock().expect("canvas state poisoned");
    let segment = guard.active();
    assert!((segment.cursor - 2.25).abs() < 1e-9);
    let Some(Op::Play(anims)) = segment.ops.last() else {
        panic!("expected parallel play op");
    };
    assert_eq!(anims.len(), 2);
    let camera = anims
        .iter()
        .find(|anim| matches!(anim.anim_type, AnimationType::CameraOrbit { .. }))
        .expect("camera orbit in parallel batch");
    assert!(matches!(camera.rate_func, gaanim_math::RateFunc::Linear));
    assert!((camera.delay - 0.25).abs() < 1e-9);
}

#[test]
fn play_builders_counts_existing_delays_in_cursor() {
    let mut canvas = SceneModel::new(1280, 720);
    let first = canvas.circle(20.0);
    let second = canvas.circle(20.0);

    canvas.play(vec![
        first.fade_in(1.0).delay(0.1),
        second.fade_in(1.0).delay(0.4),
    ]);

    let guard = canvas.state.lock().expect("canvas state poisoned");
    let segment = guard.active();
    assert!((segment.cursor - 1.4).abs() < 1e-9);
}

#[test]
fn scene_object_commands_deduplicate_and_reject_foreign_drawables() {
    let mut canvas = SceneModel::new(1280, 720);
    let title = canvas.test_title("Persistent title");
    canvas.segment("next", None).unwrap();
    canvas
        .reuse_many(&[title.clone(), title.clone()])
        .expect("same-scene drawable should be reusable");

    let guard = canvas.state.lock().expect("canvas state poisoned");
    let segment = guard.active();
    assert_eq!(
        segment
            .mobject_ids
            .iter()
            .filter(|id| **id == title.id)
            .count(),
        1
    );
    assert_eq!(
        segment
            .ops
            .iter()
            .filter(|op| matches!(op, Op::Reuse(id) if *id == title.id))
            .count(),
        1
    );
    drop(guard);

    let mut other = SceneModel::new(1280, 720);
    let foreign = other.circle(20.0);
    assert!(matches!(
        canvas.persist(&foreign),
        Err(SceneObjectError::ForeignScene)
    ));
}

#[test]
fn reuse_persist_and_release_schedule_reversible_scene_membership() {
    let mut canvas = SceneModel::new(1280, 720);
    let title = canvas.test_title("Shared title");
    canvas.wait(1.0);

    canvas
        .segment("reused", Some(TransitionType::CrossFade { duration: 0.5 }))
        .unwrap();
    canvas.reuse(&title).unwrap();
    canvas.wait(0.6);
    canvas.persist(&title).unwrap();
    canvas.wait(0.4);

    canvas
        .segment(
            "released",
            Some(TransitionType::Slide {
                duration: 0.5,
                direction: gaanim_timeline::transition::SlideDirection::Left,
            }),
        )
        .unwrap();
    canvas.release(&title).unwrap();
    canvas.wait(1.0);

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    let (initial_scene, reused_scene, released_scene) = {
        let timeline = world.resource::<Timeline>();
        let scene = |name: &str| {
            timeline
                .scenes
                .values()
                .find(|scene| scene.name == name)
                .map(|scene| scene.id)
                .unwrap()
        };
        (scene("_default"), scene("reused"), scene("released"))
    };
    let title_entity = world
        .query::<(bevy::prelude::Entity, &MobjectId)>()
        .iter(&world)
        .find_map(|(entity, id)| (id.0 == title.id).then_some(entity))
        .expect("title entity");

    let mut timeline = world.remove_resource::<Timeline>().unwrap();
    timeline.add_keyframe(
        0.0,
        gaanim_timeline::snapshot::WorldSnapshot::capture(&mut world),
    );

    timeline.seek(&mut world, 0.5);
    assert_eq!(
        world
            .get::<SceneMember>(title_entity)
            .map(|member| member.0),
        Some(initial_scene)
    );

    timeline.seek(&mut world, 1.25);
    assert_eq!(world.get::<SceneMember>(title_entity), None);
    // The renderer draws it above segment content with the same z-index.
    assert!(
        world
            .get::<gaanim_scene::SegmentContent>(title_entity)
            .is_none()
    );
    assert!(world.get::<gaanim_scene::Visible>(title_entity).is_some());
    assert_eq!(
        world.get::<gaanim_scene::Opacity>(title_entity).unwrap().0,
        1.0
    );
    let hierarchy_memberships: Vec<_> = world
        .query::<(bevy::prelude::Entity, &MobjectId)>()
        .iter(&world)
        .map(|(entity, _)| world.get::<SceneMember>(entity).copied())
        .collect();
    assert!(
        hierarchy_memberships.len() > 1,
        "title should contain glyphs"
    );
    assert!(hierarchy_memberships.iter().all(Option::is_none));

    timeline.seek(&mut world, 1.55);
    assert_eq!(
        world
            .get::<SceneMember>(title_entity)
            .map(|member| member.0),
        Some(reused_scene)
    );
    assert!(
        world
            .get::<gaanim_scene::SegmentContent>(title_entity)
            .is_some()
    );

    timeline.seek(&mut world, 1.75);
    assert_eq!(world.get::<SceneMember>(title_entity), None);

    timeline.seek(&mut world, 2.0);
    let released_start_x = world
        .get::<gaanim_math::SpatialTransform>(title_entity)
        .unwrap()
        .translation
        .x;
    timeline.seek(&mut world, 2.25);
    assert_eq!(world.get::<SceneMember>(title_entity), None);
    assert_eq!(
        world
            .get::<gaanim_math::SpatialTransform>(title_entity)
            .unwrap()
            .translation
            .x,
        released_start_x
    );

    timeline.seek(&mut world, 2.5);
    assert_eq!(
        world
            .get::<SceneMember>(title_entity)
            .map(|member| member.0),
        Some(released_scene)
    );

    timeline.seek(&mut world, 0.5);
    assert_eq!(
        world
            .get::<SceneMember>(title_entity)
            .map(|member| member.0),
        Some(initial_scene)
    );
    assert!(
        world
            .get::<gaanim_scene::SegmentContent>(title_entity)
            .is_some()
    );

    assert_ne!(reused_scene, released_scene);
}

#[test]
fn reuse_from_a_nonadjacent_segment_enters_with_the_destination() {
    let mut canvas = SceneModel::new(640, 360);
    let marker = canvas.circle(24.0);
    canvas.wait(0.5);
    canvas.segment("middle", Some(TransitionType::Cut)).unwrap();
    canvas.wait(0.5);
    canvas
        .segment("return", Some(TransitionType::CrossFade { duration: 0.4 }))
        .unwrap();
    canvas.reuse(&marker).unwrap();
    canvas.wait(0.5);

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    let return_scene = world
        .resource::<Timeline>()
        .scenes
        .values()
        .find(|scene| scene.name == "return")
        .map(|scene| scene.id)
        .unwrap();
    let marker_entity = world
        .query::<(
            bevy::prelude::Entity,
            &MobjectId,
            Option<&bevy::prelude::ChildOf>,
        )>()
        .iter(&world)
        .find_map(|(entity, _, parent)| parent.is_none().then_some(entity))
        .unwrap();
    let mut timeline = world.remove_resource::<Timeline>().unwrap();
    timeline.add_keyframe(
        0.0,
        gaanim_timeline::snapshot::WorldSnapshot::capture(&mut world),
    );

    timeline.seek(&mut world, 1.2);
    assert_eq!(
        world
            .get::<SceneMember>(marker_entity)
            .map(|member| member.0),
        Some(return_scene)
    );
    let opacity = world.get::<gaanim_scene::Opacity>(marker_entity).unwrap().0;
    assert!((opacity - 0.5).abs() < 1e-5);
}

#[test]
fn persistent_object_stays_fixed_for_the_entire_slide_transition() {
    let mut canvas = SceneModel::new(640, 360);
    let title = canvas.test_title("KEEP");
    canvas.wait(1.0);
    canvas.persist(&title).unwrap();
    canvas
        .segment(
            "next",
            Some(TransitionType::Slide {
                duration: 1.0,
                direction: gaanim_timeline::transition::SlideDirection::Left,
            }),
        )
        .unwrap();
    canvas.wait(1.0);

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    let title_entity = world
        .query::<(
            bevy::prelude::Entity,
            &MobjectId,
            Option<&bevy::prelude::ChildOf>,
        )>()
        .iter(&world)
        .find_map(|(entity, _, parent)| parent.is_none().then_some(entity))
        .unwrap();
    let mut timeline = world.remove_resource::<Timeline>().unwrap();
    timeline.add_keyframe(
        0.0,
        gaanim_timeline::snapshot::WorldSnapshot::capture(&mut world),
    );

    timeline.seek(&mut world, 1.25);
    assert_eq!(world.get::<SceneMember>(title_entity), None);
    let first_x = world
        .get::<gaanim_math::SpatialTransform>(title_entity)
        .unwrap()
        .translation
        .x;
    timeline.seek(&mut world, 1.75);
    assert_eq!(world.get::<SceneMember>(title_entity), None);
    assert_eq!(
        world
            .get::<gaanim_math::SpatialTransform>(title_entity)
            .unwrap()
            .translation
            .x,
        first_x
    );
    assert!(world.get::<gaanim_scene::Visible>(title_entity).is_some());
}

#[test]
fn late_reuse_changes_membership_at_the_current_cursor() {
    let mut canvas = SceneModel::new(640, 360);
    let marker = canvas.circle(24.0);
    canvas.wait(1.0);
    canvas
        .segment("second", Some(TransitionType::CrossFade { duration: 0.5 }))
        .unwrap();
    canvas.wait(0.25);
    canvas.reuse(&marker).unwrap();
    canvas.wait(0.5);

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    let (first_scene, second_scene) = {
        let timeline = world.resource::<Timeline>();
        let scene = |name: &str| {
            timeline
                .scenes
                .values()
                .find(|scene| scene.name == name)
                .map(|scene| scene.id)
                .unwrap()
        };
        (scene("_default"), scene("second"))
    };
    let marker_entity = world
        .query::<(
            bevy::prelude::Entity,
            &MobjectId,
            Option<&bevy::prelude::ChildOf>,
        )>()
        .iter(&world)
        .find_map(|(entity, _, parent)| parent.is_none().then_some(entity))
        .unwrap();
    let mut timeline = world.remove_resource::<Timeline>().unwrap();
    timeline.add_keyframe(
        0.0,
        gaanim_timeline::snapshot::WorldSnapshot::capture(&mut world),
    );

    timeline.seek(&mut world, 1.2);
    assert_eq!(
        world
            .get::<SceneMember>(marker_entity)
            .map(|member| member.0),
        Some(first_scene)
    );
    timeline.seek(&mut world, 1.3);
    assert_eq!(
        world
            .get::<SceneMember>(marker_entity)
            .map(|member| member.0),
        Some(second_scene)
    );
}

#[test]
fn persistence_covers_group_and_svg_descendants() {
    let temp = std::env::temp_dir().join(format!(
        "gaanim_persistent_svg_api_test_{}.svg",
        std::process::id()
    ));
    std::fs::write(
        &temp,
        r##"<svg width="80" height="40" xmlns="http://www.w3.org/2000/svg">
                <g id="assembly">
                  <rect id="body" width="30" height="20" fill="#0000ff"/>
                  <circle id="joint" cx="50" cy="20" r="8" fill="#00ff00"/>
                </g>
              </svg>"##,
    )
    .unwrap();

    let mut canvas = SceneModel::new(640, 360);
    let left = canvas.circle(20.0);
    let right = canvas.square(40.0);
    let group = canvas.group(&[&left, &right]);
    let svg = canvas.svg(&temp).unwrap();
    std::fs::remove_file(temp).unwrap();
    canvas.persist_many(&[group, svg]).unwrap();

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    let mut timeline = world.remove_resource::<Timeline>().unwrap();
    timeline.add_keyframe(
        0.0,
        gaanim_timeline::snapshot::WorldSnapshot::capture(&mut world),
    );
    timeline.seek(&mut world, 0.0);

    let memberships: Vec<_> = world
        .query::<(bevy::prelude::Entity, &MobjectId)>()
        .iter(&world)
        .map(|(entity, _)| world.get::<SceneMember>(entity).copied())
        .collect();
    assert!(memberships.len() >= 7, "expected group and SVG hierarchies");
    assert!(memberships.iter().all(Option::is_none));
}

#[test]
fn persistent_transform_does_not_localize_its_source() {
    let mut canvas = SceneModel::new(1280, 720);
    let source = canvas.test_title("Source");
    canvas.persist(&source).unwrap();
    canvas.wait(0.5);
    canvas
        .segment("target", Some(TransitionType::CrossFade { duration: 0.2 }))
        .unwrap();
    let target = canvas.test_title("Target");
    source.transform(&target).duration(0.5);

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    let mut timeline = world.remove_resource::<Timeline>().unwrap();
    timeline.seek(&mut world, 0.8);
    let source_entity = world
        .query::<(bevy::prelude::Entity, &MobjectId)>()
        .iter(&world)
        .find_map(|(entity, id)| (id.0 == source.id).then_some(entity))
        .expect("source entity");
    assert_eq!(world.get::<SceneMember>(source_entity), None);
}

#[test]
fn cross_segment_transform_does_not_retag_source_in_initial_snapshot() {
    let mut canvas = SceneModel::new(1280, 720);
    canvas.segment("first", None).unwrap();
    let circle = canvas.circle(40.0);
    let diamond = canvas.rect(80.0, 80.0);
    circle.transform(&diamond).duration(1.0);

    canvas
        .segment(
            "second",
            Some(gaanim_timeline::transition::TransitionType::CrossFade { duration: 0.2 }),
        )
        .unwrap();
    let replacement = canvas.rect(120.0, 40.0);
    circle.replacement_transform(&replacement).duration(1.0);

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    let timeline = world.resource::<Timeline>();
    let first_scene = timeline
        .scenes
        .values()
        .find(|scene| scene.name == "first")
        .map(|scene| scene.id)
        .expect("first scene");
    let mut query = world.query::<(bevy::prelude::Entity, &MobjectId)>();
    let circle_entity = query
        .iter(&world)
        .find_map(|(entity, id)| (id.0 == circle.id).then_some(entity))
        .expect("circle entity");

    assert_eq!(
        world
            .get::<SceneMember>(circle_entity)
            .map(|member| member.0),
        Some(first_scene)
    );
}

#[test]
fn tracker_arc_compiles_with_signal_and_regenerator() {
    let mut canvas = SceneModel::new(1280, 720);
    let tracker = canvas.value_tracker(0.4);
    let _arrow = canvas.always_redraw_arc(
        ScalarSource::signal(tracker.id),
        0.0,
        0.0,
        120.0,
        0.0,
        0.4,
        1.0,
        0.0,
    );
    canvas.play(vec![tracker.animate_value_to(1.2).duration(1.0)]);

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    let tracker_entity = world
        .query_filtered::<bevy::prelude::Entity, With<gaanim_animation::FloatSignal>>()
        .iter(&world)
        .next()
        .expect("tracker entity");
    let arrow_entity = world
        .query_filtered::<bevy::prelude::Entity, With<gaanim_animation::AlwaysRedrawRegen>>()
        .iter(&world)
        .next()
        .expect("arrow entity");

    assert_eq!(
        world
            .get::<gaanim_animation::FloatSignal>(tracker_entity)
            .map(|signal| signal.value),
        Some(0.4)
    );
    assert!(
        world
            .get::<gaanim_animation::AlwaysRedrawRegen>(arrow_entity)
            .is_some()
    );

    world
        .get_mut::<gaanim_animation::FloatSignal>(tracker_entity)
        .expect("tracker signal")
        .value = 1.2;
    gaanim_animation::always_redraw_regen_system(&mut world);
    assert!(
        !world
            .get::<gaanim_scene::Path2D>(arrow_entity)
            .expect("reactive path")
            .0
            .elements()
            .is_empty(),
        "the regenerated arrow path must reflect the signal value"
    );
}

#[test]
fn group_pivot_is_preserved_in_the_compiled_transform() {
    let mut canvas = SceneModel::new(1280, 720);
    let rail = canvas.line(40.0, 0.0, 140.0, 0.0);
    let _mechanism = canvas.group(&[&rail]).with_pivot(100.0, -18.0);

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    let mut query = world.query::<(&MobjectId, &gaanim_math::SpatialTransform)>();
    let transform = query
        .iter(&world)
        .find_map(|(_, transform)| {
            (transform.anchor == gaanim_core::glam::DVec3::new(100.0, -18.0, 0.0))
                .then_some(transform)
        })
        .expect("group transform");

    assert_eq!(
        transform.anchor,
        gaanim_core::glam::DVec3::new(100.0, -18.0, 0.0)
    );
}

#[test]
fn axes_compile_grid_lines_and_ticks_with_independent_styles() {
    let mut canvas = SceneModel::new(640, 360);
    // An active theme's axes rules replace `AxesConfig` colors.
    canvas.clear_theme();
    let axis_color = Color::from_rgb8(0x11, 0x22, 0x33);
    let grid_color = Color::from_rgb8(0x44, 0x55, 0x66);
    let tick_color = Color::from_rgb8(0x77, 0x88, 0x99);
    let config = crate::canvas::AxesConfig {
        y_grid: false,
        y_ticks: false,
        y_numbers: false,
        axis_color,
        grid_color,
        tick_color,
        x_label: Some("time".to_owned()),
        ..Default::default()
    };
    let _axes = canvas.axes((-200.0, 200.0, 50.0), (-100.0, 100.0, 25.0), config);

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    let mut query = world.query::<(&gaanim_scene::ObjectTag, &gaanim_scene::StrokeBrush)>();
    let layers = query
        .iter(&world)
        .filter_map(|(tag, stroke)| {
            let gaanim_core::peniko::Brush::Solid(color) = stroke.brush.as_ref()? else {
                return None;
            };
            Some((tag.0.as_str(), *color, stroke.style.width))
        })
        .collect::<Vec<_>>();

    assert!(layers.contains(&("SvgPath#AxesLines", axis_color, 0.03)));
    assert!(layers.contains(&("SvgPath#AxesGrid", grid_color, 0.01)));
    assert!(layers.contains(&("SvgPath#AxesTicks", tick_color, 0.02)));
}

#[test]
fn reactive_spring_regenerates_a_helical_path() {
    let mut canvas = SceneModel::new(1280, 720);
    let _spring = canvas.spring_between_with_crossing(
        crate::canvas::CanvasEndpoint::Static(gaanim_core::glam::DVec3::new(-80.0, 0.0, 0.0)),
        crate::canvas::CanvasEndpoint::Static(gaanim_core::glam::DVec3::new(80.0, 0.0, 0.0)),
        5,
        14.0,
        0.7,
    );

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    let spring = world
        .query_filtered::<bevy::prelude::Entity, With<gaanim_animation::AlwaysRedrawRegen>>()
        .iter(&world)
        .next()
        .expect("reactive spring entity");
    gaanim_animation::always_redraw_regen_system(&mut world);
    let path = world
        .get::<gaanim_scene::Path2D>(spring)
        .expect("spring path");
    assert!(
        path.0.elements().len() > 100,
        "spring must have enough samples for a smooth helical coil"
    );
}

#[test]
fn spring_support_uses_local_geometry_instead_of_a_tracking_spring() {
    let mut canvas = SceneModel::new(640, 360);
    let _support = canvas.support_at(
        CanvasEndpoint::Static(DVec3::new(80.0, 45.0, 0.0)),
        "spring",
        DVec3::Y,
        48.0,
        70.0,
        None,
    );

    let state = canvas.state.lock().expect("canvas state poisoned");
    assert!(
        !state
            .active()
            .ops
            .iter()
            .any(|op| matches!(op, Op::AttachTrackingSpring { .. })),
        "a support spring must remain local to its support so timeline pauses and seeks cannot place it at the scene origin"
    );
}

#[test]
fn fixed_support_uses_connection_plate_without_a_stem() {
    let mut canvas = SceneModel::new(640, 360);
    let support = canvas.support_at(
        CanvasEndpoint::Static(DVec3::ZERO),
        "fixed",
        DVec3::Y,
        48.0,
        70.0,
        None,
    );
    assert!(matches!(
        &support.body.spec.lock().expect("body spec").kind,
        SpawnKind::GroupNoCenter(children) if children.is_empty()
    ));
    let ground_children = match &support.ground.spec.lock().expect("ground spec").kind {
        SpawnKind::GroupNoCenter(children) => children.clone(),
        other => panic!("expected fixed-support ground group, got {other:?}"),
    };
    assert_eq!(
        ground_children.len(),
        1,
        "fixed support must contain one connection plate"
    );
}

#[test]
fn anchored_bar_and_labeled_dimension_compile_the_reactive_contract() {
    let mut canvas = SceneModel::new(640, 360);
    let frame = canvas.rect(180.0, 80.0).move_to(20.0, 0.0);
    let left = frame.anchor_point(Anchor::TopLeft, DVec3::ZERO);
    let right = frame.anchor_point(Anchor::TopRight, DVec3::ZERO);
    let _bar = canvas.bar_between(
        CanvasEndpoint::Static(DVec3::new(-180.0, 120.0, 0.0)),
        left.into(),
        9.0,
    );
    let dimension = canvas
        .dimension_between_with_options(
            left.into(),
            right.into(),
            45.0,
            DimensionOptions {
                label: Some("$W_f$".to_owned()),
                show_value: true,
                unit: Some("mm".to_owned()),
                scale: 0.5,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        dimension
            .label
            .as_ref()
            .and_then(DrawableHandle::text_spec)
            .and_then(|spec| spec.style.size),
        Some(0.48),
        "dimension labels use the theme's scene-unit size"
    );

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    assert!(
        world
            .query_filtered::<bevy::prelude::Entity, With<gaanim_animation::TrackingLine>>()
            .iter(&world)
            .next()
            .is_some(),
        "bar must compile as a reactive tracking line"
    );
    assert!(
        world
            .query_filtered::<bevy::prelude::Entity, With<gaanim_animation::EndpointDistance>>()
            .iter(&world)
            .next()
            .is_some(),
        "numeric dimension must compile a distance-backed signal"
    );
    assert!(
            world
                .query_filtered::<
                    bevy::prelude::Entity,
                    With<gaanim_animation::DimensionLabelPlacement>,
                >()
                .iter(&world)
                .next()
                .is_some(),
            "dimension annotation must keep a reactive placement binding"
        );
    gaanim_animation::always_redraw_regen_system(&mut world);
    let dimension_parts = world
        .query_filtered::<bevy::prelude::Entity, With<gaanim_animation::AlwaysRedrawRegen>>()
        .iter(&world)
        .collect::<Vec<_>>();
    assert_eq!(
        dimension_parts.len(),
        2,
        "dimension geometry must use two reactive parts"
    );
    let close_counts = dimension_parts
        .iter()
        .map(|entity| {
            world
                .get::<gaanim_scene::Path2D>(*entity)
                .expect("dimension part path")
                .0
                .elements()
                .iter()
                .filter(|element| matches!(element, gaanim_core::kurbo::PathEl::ClosePath))
                .count()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        close_counts.iter().sum::<usize>(),
        5,
        "dimension geometry must contain two extensions, one baseline, and two heads"
    );
    assert!(
        dimension_parts.iter().all(|entity| world
            .get::<gaanim_scene::FillBrush>(*entity)
            .and_then(|fill| fill.0.as_ref())
            .is_some()),
        "dimension silhouette must carry a fill brush"
    );
    assert!(dimension.label.is_some());
    assert!(dimension.number.is_some());
    assert!(dimension.unit.is_some());
}

#[test]
fn explicit_dimension_value_uses_parameter_without_driving_geometry() {
    let mut canvas = SceneModel::new(640, 360);
    let value = canvas.parameter(12.0).unwrap();
    let dimension = canvas
        .dimension_between_with_options(
            CanvasEndpoint::Static(DVec3::new(-80.0, 0.0, 0.0)),
            CanvasEndpoint::Static(DVec3::new(80.0, 0.0, 0.0)),
            35.0,
            DimensionOptions {
                value: Some(value.source()),
                format: ".1f".to_owned(),
                unit: Some("m".to_owned()),
                scale: 99.0,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(dimension.number.is_some(), "value must imply show_value");

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    assert!(
        world
            .query_filtered::<bevy::prelude::Entity, With<gaanim_animation::EndpointDistance>>()
            .iter(&world)
            .next()
            .is_none(),
        "an explicit semantic value must not create a distance-backed signal"
    );
    let readout = world
        .query::<&gaanim_animation::ReactiveReadout>()
        .single(&world)
        .expect("dimension readout");
    assert_eq!(readout.last_text, "12.0");
    assert_eq!(readout.parameters.len(), 1);
    let (_, signal_entity) = readout.parameters[0];
    assert_eq!(
        world
            .get::<gaanim_animation::FloatSignal>(signal_entity)
            .expect("value parameter signal")
            .value,
        12.0
    );
    assert!(
            world
                .query_filtered::<
                    bevy::prelude::Entity,
                    With<gaanim_animation::DimensionLabelPlacement>,
                >()
                .iter(&world)
                .next()
                .is_some(),
            "the annotation placement must remain endpoint-driven"
        );
}

#[test]
fn readout_part_spacing_is_in_scene_units() {
    let mut canvas = SceneModel::new(16.0, 9.0);
    canvas
        .dimension_between_with_options(
            CanvasEndpoint::Static(DVec3::new(-2.0, 0.0, 0.0)),
            CanvasEndpoint::Static(DVec3::new(2.0, 0.0, 0.0)),
            0.5,
            DimensionOptions {
                label: Some("W".to_owned()),
                show_value: true,
                unit: Some("m".to_owned()),
                ..Default::default()
            },
        )
        .unwrap();

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    // The gap between label, "=", value and unit is a fraction of the text size.
    let layout = world
        .query::<&gaanim_animation::ReactiveReadoutLayout>()
        .single(&world)
        .expect("dimension readout layout");
    assert!(layout.spacing > 0.0 && layout.spacing < DEFAULT_REACTIVE_TEXT_SIZE * 0.5);
}

#[test]
fn component_force_keeps_physical_readout_separate_from_visual_scale() {
    let mut canvas = SceneModel::new(640, 360);
    let fx = canvas.parameter(3.0).unwrap();
    let fy = canvas.parameter(4.0).unwrap();
    let force = canvas
        .force_from_components(
            CanvasEndpoint::Static(DVec3::new(20.0, -10.0, 0.0)),
            fx.source(),
            fy.source(),
            10.0,
            Some("$F$".to_owned()),
            true,
            ".1f".to_owned(),
            Some("N".to_owned()),
            14.0,
            None,
            None,
        )
        .unwrap();

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    gaanim_animation::endpoint_distance_system(&mut world);
    gaanim_animation::tracking_line_system(&mut world);
    gaanim_animation::tracking_vector_head_system(&mut world);

    let (_, distance) = world
        .query::<(
            &gaanim_animation::FloatSignal,
            &gaanim_animation::EndpointDistance,
        )>()
        .iter(&world)
        .next()
        .expect("force magnitude signal");
    assert!(matches!(
        distance.to,
        gaanim_animation::TrackingEndpoint::Offset { .. }
    ));
    let value = world
        .query::<(
            &gaanim_animation::FloatSignal,
            &gaanim_animation::EndpointDistance,
        )>()
        .iter(&world)
        .next()
        .unwrap()
        .0
        .value;
    assert!((value - 5.0).abs() < 1e-9);
    assert!(force.number.is_some());
    assert!(force.unit.is_some());
}

#[test]
fn transform_matching_advances_the_cursor_like_a_played_animation() {
    let mut canvas = SceneModel::new(1280, 720);
    let source = canvas.rect(1.0, 1.0);
    let target = canvas.circle(0.5);
    canvas.wait(0.5);
    canvas.transform_matching_shapes(&source, &target, 1.5);
    assert_eq!(canvas.current_time(), 2.0);
    canvas
        .transform_matching(&target, &source, "tex", 0.25)
        .unwrap();
    assert_eq!(canvas.current_time(), 2.25);
    assert!(
        canvas
            .transform_matching(&target, &source, "glyphs", 0.25)
            .is_err()
    );
    assert_eq!(canvas.current_time(), 2.25);
    assert_eq!(canvas.segment_manifest().duration(), 2.25);
}

#[test]
fn segments_use_absolute_ranges_and_only_explicit_stops_pause() {
    let mut canvas = SceneModel::new(1280, 720);
    let intro = canvas
        .segment_with("intro", None, Some("Opening".to_string()), None)
        .unwrap();
    canvas.wait(1.0);
    canvas.stop(Some("reveal".to_string())).unwrap();
    canvas.wait(2.0);
    let details = canvas
        .segment_with("details", None, None, Some("comparison".to_string()))
        .unwrap();
    canvas.wait(0.5);

    let manifest = canvas.segment_manifest();
    assert_eq!(manifest.segments.len(), 2);
    assert_eq!(manifest.segments[0].id, intro.id());
    assert_eq!(manifest.segments[0].name, "intro");
    assert_eq!(manifest.segments[0].notes.as_deref(), Some("Opening"));
    assert_eq!(manifest.segments[0].start_time, 0.0);
    assert_eq!(manifest.segments[0].end_time, 3.0);
    assert_eq!(manifest.segments[0].stops[0].time, 1.0);
    assert_eq!(manifest.segments[1].id, details.id());
    assert_eq!(manifest.segments[1].start_time, 3.0);
    assert_eq!(manifest.segments[1].end_time, 3.5);
    assert_eq!(canvas.current_time(), 3.5);

    let state = canvas.state.lock().expect("canvas state poisoned");
    let stops = state
        .segments
        .iter()
        .flat_map(|segment| segment.ops.iter())
        .filter(|op| matches!(op, Op::Stop))
        .count();
    assert_eq!(stops, 1);
}

#[test]
fn segment_targets_reach_the_manifest_and_bad_ones_are_refused() {
    let mut canvas = SceneModel::new(1280, 720);
    let intro = canvas.segment("intro", None).unwrap();
    canvas.set_segment_target(&intro, Some(90.0)).unwrap();
    canvas.wait(1.0);
    let outro = canvas.segment("outro", None).unwrap();
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            canvas.set_segment_target(&outro, Some(bad)),
            Err(SegmentError::InvalidTarget { .. })
        ));
    }
    let manifest = canvas.segment_manifest();
    let targets: Vec<_> = manifest
        .segments
        .iter()
        .map(|segment| segment.target)
        .collect();
    assert_eq!(targets, [Some(90.0), None]);
    canvas.set_segment_target(&intro, None).unwrap();
    assert_eq!(canvas.segment_manifest().segments[0].target, None);

    // The whole talk's.
    assert!(matches!(
        canvas.set_talk_target(Some(-5.0)),
        Err(SegmentError::InvalidTarget { .. })
    ));
    canvas.set_talk_target(Some(1200.0)).unwrap();
    assert_eq!(canvas.talk_target, Some(1200.0));
}

#[test]
fn a_stop_inside_the_entry_transition_waits_for_its_end() {
    let mut canvas = SceneModel::new(1280, 720);
    canvas.segment("cover", None).unwrap();
    canvas.wait(0.8);
    canvas.stop(None).unwrap();
    canvas
        .segment("plan", Some(TransitionType::CrossFade { duration: 0.5 }))
        .unwrap();
    canvas.wait(0.4);
    canvas.stop(Some("first".to_string())).unwrap();
    canvas.wait(0.4);
    canvas.stop(Some("second".to_string())).unwrap();

    // The first stop waits from 1.2 s to the end of the transition; the
    // second, after it, stays where it was written.
    let manifest = canvas.segment_manifest();
    let stops: Vec<f64> = manifest.segments[1]
        .stops
        .iter()
        .map(|stop| stop.time)
        .collect();
    assert_eq!(stops.len(), 2);
    assert!((stops[0] - 1.3).abs() < 1e-9, "{stops:?}");
    assert!((stops[1] - 1.7).abs() < 1e-9, "{stops:?}");
    assert!((canvas.current_time() - 1.7).abs() < 1e-9);
}

#[test]
fn declared_audio_starts_only_when_played_at_the_absolute_scene_cursor() {
    let path =
        std::env::temp_dir().join(format!("gaanim-declared-audio-{}.wav", std::process::id()));
    std::fs::write(&path, b"fixture").unwrap();

    let mut canvas = SceneModel::new(1280, 720);
    canvas.segment("intro", None).unwrap();
    canvas.wait(2.0);
    canvas.segment("audio", None).unwrap();
    canvas.wait(1.0);
    let audio = canvas.audio(&path, Some(2.0), 0.5, 0.1, 0.2).unwrap();

    assert!(canvas.audio_tracks.is_empty(), "declaration must be inert");
    canvas.play_items(vec![audio.into()]).unwrap();

    assert_eq!(canvas.audio_tracks.len(), 1);
    assert_eq!(canvas.audio_tracks[0].start_time, 3.0);
    assert_eq!(canvas.current_time(), 5.0);
    let state = canvas.state.lock().expect("canvas state poisoned");
    assert!(matches!(
        state.active().ops[state.active().ops.len() - 2],
        Op::Play(_)
    ));
    assert!(matches!(state.active().ops.last(), Some(Op::Wait(2.0))));
    drop(state);
    let _ = std::fs::remove_file(path);
}

fn sfx_fixture(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("gaanim-sfx-{name}-{}.wav", std::process::id()));
    std::fs::write(&path, b"fixture").unwrap();
    path
}

#[test]
fn sfx_places_a_short_track_at_the_cursor_or_an_explicit_time() {
    let path = sfx_fixture("placed");
    let mut canvas = SceneModel::new(1280, 720);
    canvas.wait(1.5);
    canvas.sfx(&path, None, 0.7).unwrap();
    canvas.sfx(&path, Some(0.25), 1.0).unwrap();

    assert_eq!(canvas.audio_tracks.len(), 2);
    assert_eq!(canvas.audio_tracks[0].start_time, 1.5);
    assert_eq!(canvas.audio_tracks[0].volume, 0.7);
    assert_eq!(canvas.audio_tracks[0].duration, None);
    assert_eq!(canvas.audio_tracks[1].start_time, 0.25);
    assert_eq!(canvas.current_time(), 1.5, "an sfx never moves the cursor");
    assert!(canvas.sfx(&path, Some(-1.0), 1.0).is_err());
    assert!(canvas.sfx(&path, None, f64::NAN).is_err());
    assert!(canvas.sfx("missing-sfx.wav", None, 1.0).is_err());
    assert_eq!(canvas.audio_tracks.len(), 2);
    let _ = std::fs::remove_file(path);
}

#[test]
fn animation_sound_follows_the_animation_start() {
    use gaanim_timeline::sound::SoundCue;
    let path = sfx_fixture("anchored");
    let mut canvas = SceneModel::new(1280, 720);
    let dot = canvas.circle(0.5);
    let square = canvas.square(1.0);
    canvas.wait(1.0);
    let cue = SoundCue::new(&path).unwrap();
    // The second step of a sequence starts after the first one ends,
    // plus its own delay: the sound follows it there.
    let first = Composition::leaf(dot.animate().fade_in().duration(2.0));
    let second = Composition::leaf(
        square
            .animate()
            .fade_in()
            .duration(1.0)
            .delay(0.5)
            .sound(cue.clone()),
    );
    canvas
        .play_composition_configured(
            Composition::sequence(vec![first, second], 0.0).unwrap(),
            None,
            None,
        )
        .unwrap();
    assert_eq!(canvas.audio_tracks.len(), 1);
    assert!((canvas.audio_tracks[0].start_time - 3.5).abs() < 1e-9);

    // A delayed animation in a plain play shifts its sound too, and an
    // offset anticipates it.
    let early = SoundCue::with_options(&path, 0.4, -0.25).unwrap();
    canvas.play(vec![dot.animate().fade_out().delay(0.75).sound(early)]);
    assert_eq!(canvas.audio_tracks.len(), 2);
    assert!((canvas.audio_tracks[1].start_time - (4.5 + 0.75 - 0.25)).abs() < 1e-9);
    assert_eq!(canvas.audio_tracks[1].volume, 0.4);
    let _ = std::fs::remove_file(path);
}

#[test]
fn invalid_animation_sound_rejects_the_whole_play() {
    use gaanim_timeline::sound::SoundCue;
    let mut canvas = SceneModel::new(1280, 720);
    let dot = canvas.circle(0.5);
    let result = canvas.play_items(vec![
        dot.animate()
            .fade_in()
            .sound(SoundCue::new("missing-sfx.wav").unwrap())
            .into(),
    ]);
    assert!(matches!(result, Err(PlayError::InvalidSound(_))));
    assert_eq!(
        canvas.current_time(),
        0.0,
        "a rejected play schedules nothing"
    );
    assert!(canvas.audio_tracks.is_empty());
    // A sound that would start before the timeline is rejected as well.
    let path = sfx_fixture("early");
    let early = SoundCue::with_options(&path, 1.0, -0.5).unwrap();
    let result = canvas.play_items(vec![dot.animate().fade_in().sound(early).into()]);
    assert!(matches!(result, Err(PlayError::InvalidSound(_))));
    let _ = std::fs::remove_file(path);
}

#[test]
fn transition_sound_starts_with_the_segment_it_enters() {
    use gaanim_timeline::sound::SoundCue;
    let path = sfx_fixture("transition");
    let whoosh = SoundCue::new(&path).unwrap();
    let mut canvas = SceneModel::new(1280, 720);
    let first = canvas.segment("first", None).unwrap();
    canvas.wait(2.0);
    canvas
        .segment(
            "second",
            Some(TransitionType::CrossFade { duration: 0.5 }.with_sound(whoosh.clone())),
        )
        .unwrap();
    canvas.wait(1.0);
    assert_eq!(canvas.audio_tracks.len(), 1);
    assert_eq!(canvas.audio_tracks[0].start_time, 2.0);

    let third = canvas.segment("third", None).unwrap();
    canvas.wait(1.0);
    // Relinking replaces the transition and places its sound at the
    // start of the segment it enters.
    canvas
        .link(
            &first,
            &third,
            TransitionType::Slide {
                duration: 0.5,
                direction: gaanim_timeline::transition::SlideDirection::Left,
            }
            .with_sound(whoosh.clone()),
        )
        .unwrap();
    assert_eq!(canvas.audio_tracks.len(), 2);
    assert_eq!(canvas.audio_tracks[1].start_time, 3.0);
    canvas.link(&first, &third, TransitionType::Cut).unwrap();
    assert_eq!(
        canvas.audio_tracks.len(),
        1,
        "a soundless relink drops the sound"
    );
    assert_eq!(canvas.audio_tracks[0].start_time, 2.0);

    let missing = TransitionType::Cut.with_sound(SoundCue::new("missing-sfx.wav").unwrap());
    assert!(matches!(
        canvas.segment("fourth", Some(missing)),
        Err(SegmentError::Sound(_))
    ));
    assert_eq!(
        canvas.segment_count(),
        3,
        "an invalid sound creates no segment"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn trimmed_background_audio_fades_without_extending_the_play() {
    let path =
        std::env::temp_dir().join(format!("gaanim-trimmed-audio-{}.wav", std::process::id()));
    std::fs::write(&path, b"fixture").unwrap();
    let mut canvas = SceneModel::new(1280, 720);
    canvas.wait(1.0);
    let music = canvas.audio_until(&path, 30.0, 0.8, 0.0, 1.5).unwrap();
    canvas.play_items(vec![music.into()]).unwrap();
    assert_eq!(
        canvas.current_time(),
        1.0,
        "background audio keeps the cursor"
    );
    assert_eq!(canvas.audio_tracks.len(), 1);
    let track = &canvas.audio_tracks[0];
    assert_eq!(track.start_time, 1.0);
    assert_eq!(track.duration, Some(30.0));
    assert_eq!(track.fade_out, 1.5);
    assert!(canvas.audio_until(&path, 1.0, 1.0, 0.8, 0.8).is_err());
    let _ = std::fs::remove_file(path);
}

#[test]
fn open_ended_audio_play_does_not_extend_the_timeline() {
    let path = std::env::temp_dir().join(format!(
        "gaanim-background-audio-{}.wav",
        std::process::id()
    ));
    std::fs::write(&path, b"fixture").unwrap();

    let mut canvas = SceneModel::new(1280, 720);
    canvas.wait(1.25);
    let audio = canvas.audio(&path, None, 1.0, 0.0, 0.0).unwrap();
    canvas.play_items(vec![audio.into()]).unwrap();

    assert_eq!(canvas.audio_tracks[0].start_time, 1.25);
    assert_eq!(canvas.current_time(), 1.25);
    let _ = std::fs::remove_file(path);
}

#[test]
fn composition_schedules_open_audio_without_blocking_and_rejects_stretch() {
    let path =
        std::env::temp_dir().join(format!("gaanim-composed-audio-{}.wav", std::process::id()));
    std::fs::write(&path, b"fixture").unwrap();
    let mut canvas = SceneModel::new(320, 180);
    let audio = canvas.audio(&path, None, 1.0, 0.0, 0.0).unwrap();
    let visual = canvas.circle(8.0).animate().fade_in().duration(0.5);
    let plan = Composition::sequence(
        vec![Composition::leaf(audio), Composition::leaf(visual)],
        0.0,
    )
    .unwrap();
    let schedule = plan.schedule(None).unwrap();
    assert_eq!(schedule.entries[0].duration, None);
    assert_eq!(schedule.entries[1].start, 0.0);
    assert_eq!(schedule.span, 0.5);
    assert!(matches!(
        plan.clone().stretch(2.0),
        Err(PlayError::StretchContainsMedia)
    ));
    canvas
        .play_composition_configured(plan, None, None)
        .unwrap();
    assert_eq!(canvas.current_time(), 0.5);
    let _ = std::fs::remove_file(path);
}

#[test]
fn declared_video_activates_frames_and_embedded_audio_at_one_cursor() {
    let path =
        std::env::temp_dir().join(format!("gaanim-declared-video-{}.mp4", std::process::id()));
    std::fs::write(&path, b"fixture").unwrap();

    let mut canvas = SceneModel::new(1280, 720);
    canvas.wait(1.5);
    let playback = gaanim_media::VideoPlayback {
        path: path.clone(),
        metadata: gaanim_media::VideoMetadata {
            width: 1,
            height: 1,
            duration: 4.0,
            fps: 30.0,
            has_audio: true,
        },
        scene_start: 0.0,
        source_offset: 0.0,
        source_duration: 4.0,
        looping: false,
        speed: 2.0,
        audio: true,
        volume: 0.75,
        last_frame: None,
        active: true,
        intervals: Vec::new(),
        frames: None,
    };
    let poster = gaanim_core::peniko::ImageData {
        data: gaanim_core::peniko::Blob::from(vec![0, 0, 0, 255]),
        format: gaanim_core::peniko::ImageFormat::Rgba8,
        alpha_type: gaanim_core::peniko::ImageAlphaType::Alpha,
        width: 1,
        height: 1,
    };
    let view = ImageOptions::default().resolve(1, 1).unwrap();
    let drawable = canvas.spawn(SpawnKind::Video {
        poster,
        view,
        playback,
    });
    let video = VideoClip {
        drawable: drawable.clone(),
        state: canvas.state.clone(),
        duration: Some(2.0),
        audio: Some(AudioTrack::from_media(&path, 0.0, 0.0, 4.0, 2.0, false, 0.75).unwrap()),
        activated: Arc::new(AtomicBool::new(false)),
    };

    assert_eq!(
        canvas.play_items(vec![video.clone().into(), video.clone().into()]),
        Err(PlayError::VideoAlreadyActivated)
    );
    assert_eq!(canvas.current_time(), 1.5);
    assert!(canvas.audio_tracks.is_empty());
    canvas.play_items(vec![video.clone().into()]).unwrap();

    let spec = drawable.spec.lock().expect("object spec poisoned");
    let SpawnKind::Video { playback, .. } = &spec.kind else {
        panic!("expected video spawn kind");
    };
    assert_eq!(playback.scene_start, 1.5);
    drop(spec);
    assert_eq!(canvas.audio_tracks[0].start_time, 1.5);
    assert_eq!(canvas.current_time(), 3.5);
    assert_eq!(
        canvas.play_items(vec![video.into()]),
        Err(PlayError::VideoAlreadyActivated)
    );
    assert_eq!(canvas.audio_tracks.len(), 1);
    let _ = std::fs::remove_file(path);
}

fn synthetic_video(canvas: &mut SceneModel) -> VideoClip {
    let poster = gaanim_core::peniko::ImageData {
        data: gaanim_core::peniko::Blob::from(vec![0; 400 * 200 * 4]),
        format: gaanim_core::peniko::ImageFormat::Rgba8,
        alpha_type: gaanim_core::peniko::ImageAlphaType::Alpha,
        width: 400,
        height: 200,
    };
    let options = ImageOptions {
        width: Some(8.0),
        height: Some(4.0),
        ..Default::default()
    };
    let view = options.resolve(400, 200).unwrap();
    let playback = gaanim_media::VideoPlayback {
        path: "unused.mp4".into(),
        metadata: gaanim_media::VideoMetadata {
            width: 400,
            height: 200,
            duration: 10.0,
            fps: 30.0,
            has_audio: false,
        },
        scene_start: 0.0,
        source_offset: 1.0,
        source_duration: 2.0,
        looping: true,
        speed: 2.0,
        audio: false,
        volume: 0.5,
        last_frame: None,
        active: true,
        intervals: Vec::new(),
        frames: None,
    };
    let drawable = canvas.spawn(SpawnKind::Video {
        poster,
        view,
        playback,
    });
    drawable.spec.lock().unwrap().media_frame = Some(options.media_frame(view));
    VideoClip {
        drawable,
        state: canvas.state.clone(),
        duration: None,
        audio: None,
        activated: Arc::new(AtomicBool::new(false)),
    }
}

#[test]
fn media_segments_are_finite_atomic_and_composable() {
    let mut scene = SceneModel::new(1280, 720);
    let video = synthetic_video(&mut scene);
    let a = video.segment(2.0, 5.0, None, None, None).unwrap();
    let b = video.segment(5.0, 8.0, Some(1.0), None, None).unwrap();
    assert_eq!(
        Composition::leaf(a.clone()).schedule(None).unwrap().span,
        1.5
    );
    assert!(matches!(
        Composition::leaf(a.clone()).stretch(4.0),
        Err(PlayError::StretchContainsMedia)
    ));
    assert_eq!(
        scene.play_items(vec![a.clone().into(), b.clone().into()]),
        Err(PlayError::OverlappingVideoSegments)
    );
    assert_eq!(scene.current_time(), 0.0);
    assert!(!a.consumed.load(Ordering::Acquire));
    let sequence = Composition::sequence(
        vec![Composition::leaf(a.clone()), Composition::leaf(b.clone())],
        1.0,
    )
    .unwrap();
    scene
        .play_composition_configured(sequence, None, None)
        .unwrap();
    assert_eq!(scene.current_time(), 5.5);
    assert_eq!(
        scene.play_items(vec![a.into()]),
        Err(PlayError::VideoSegmentConsumed)
    );
    assert_eq!(
        scene.play_items(vec![video.clone().into()]),
        Err(PlayError::MixedVideoPlayback)
    );
    scene
        .play_items(vec![
            video.segment(2.0, 5.0, None, None, None).unwrap().into(),
        ])
        .unwrap();
    let spec = video.drawable.spec.lock().unwrap();
    let SpawnKind::Video { playback, .. } = &spec.kind else {
        unreachable!()
    };
    assert_eq!(playback.intervals.len(), 3);
    assert_eq!(playback.intervals[1].scene_start, 2.5);
}

#[test]
fn media_segments_validate_owner_mode_and_ranges() {
    let mut scene = SceneModel::new(1280, 720);
    let video = synthetic_video(&mut scene);
    for (start, end) in [(f64::NAN, 2.0), (-1.0, 2.0), (2.0, 2.0), (0.0, 11.0)] {
        assert!(video.segment(start, end, None, None, None).is_err());
    }
    assert!(video.segment(0.0, 2.0, Some(0.0), None, None).is_err());
    assert!(video.segment(0.0, 2.0, None, None, Some(f64::NAN)).is_err());
    let segment = video.segment(0.0, 2.0, None, None, None).unwrap();
    let mut other = SceneModel::new(1280, 720);
    assert_eq!(
        other.play_items(vec![segment.clone().into()]),
        Err(PlayError::ForeignVideo)
    );
    assert_eq!(
        scene.play_items(vec![segment.clone().into(), video.clone().into()]),
        Err(PlayError::MixedVideoPlayback)
    );
    assert_eq!(scene.current_time(), 0.0);
    scene.play_items(vec![video.into()]).unwrap();
    assert_eq!(
        scene.play_items(vec![segment.into()]),
        Err(PlayError::MixedVideoPlayback)
    );
}

#[test]
fn chalk_from_a_later_write_reaches_the_frozen_spec() {
    let mut scene = SceneModel::new(1280, 720);
    let first = scene.circle(1.0);
    let title = scene.text("Pizarra");
    scene
        .play_items(vec![first.animate().write().into()])
        .unwrap();
    assert!(
        scene.state.lock().unwrap().frozen_spawn_specs[&title.id]
            .chalk
            .is_none()
    );
    let write = title.animate().write().chalk(4, 0.02);
    scene.play_items(vec![write.into()]).unwrap();
    let chalk = gaanim_renderer::effects::ChalkBrush {
        seed: 4,
        roughness: 0.02,
    };
    assert_eq!(
        scene.state.lock().unwrap().frozen_spawn_specs[&title.id].chalk,
        Some(chalk)
    );
    assert_eq!(title.spec.lock().unwrap().chalk, Some(chalk));
}

/// With z up, a cylinder stands along z and a plane lies on the floor
/// facing up; with y up they keep their y axis.
#[test]
fn primitives_stand_along_the_world_up_axis() {
    let material = gaanim_scene::Material3D::default();
    let extent = |canvas: &mut SceneModel, z_up: bool| {
        canvas.world_up = if z_up { DVec3::Z } else { DVec3::Y };
        let cylinder = canvas.cylinder(0.5, 3.0, 16, true, material).unwrap();
        let plane = canvas.plane(2.0, 2.0, (1, 1), material).unwrap();
        let mesh = |handle: &DrawableHandle| match &handle.spec.lock().unwrap().kind {
            SpawnKind::Primitive3D(mesh) => mesh.clone(),
            _ => panic!("a 3D primitive"),
        };
        let (cylinder, plane) = (mesh(&cylinder), mesh(&plane));
        let span = |axis: usize| {
            let values = cylinder.vertices.iter().map(|vertex| vertex[axis]);
            values.clone().fold(f32::MIN, f32::max) - values.fold(f32::MAX, f32::min)
        };
        let normal = plane.normals.as_ref().unwrap()[0];
        (span(1), span(2), normal)
    };
    let mut canvas = SceneModel::new(640, 360);
    let (height_y, depth_z, normal) = extent(&mut canvas, false);
    assert!((height_y - 3.0).abs() < 1e-5 && (depth_z - 1.0).abs() < 1e-5);
    assert_eq!(normal, [0.0, 1.0, 0.0]);
    let (depth_y, height_z, normal) = extent(&mut canvas, true);
    assert!((height_z - 3.0).abs() < 1e-5 && (depth_y - 1.0).abs() < 1e-5);
    assert_eq!(normal, [0.0, 0.0, 1.0]);
}

#[test]
fn media_framing_validates_and_preserves_birth_state() {
    let mut scene = SceneModel::new(1280, 720);
    let video = synthetic_video(&mut scene)
        .frame(8.0, 4.5, ImageFit::Cover)
        .unwrap();
    let image = video.drawable.clone();
    assert_eq!(image.source_width().unwrap(), 400);
    assert!(image.clone().frame(0.0, 1.0, ImageFit::Contain).is_err());
    assert!(image.clone().crop(0.5, 0.0, 1.0, 1.0, true).is_err());
    let birth = image.spec.lock().unwrap().media_frame.unwrap();
    scene.wait(1.0);
    let anim = image.animate().crop(0.25, 0.25, 0.5, 0.5, true).unwrap();
    scene.play_items(vec![anim.into()]).unwrap();
    assert_eq!(
        image.spec.lock().unwrap().media_frame.unwrap().crop,
        gaanim_core::kurbo::Rect::new(100.0, 50.0, 300.0, 150.0)
    );
    image
        .clone()
        .quality(gaanim_core::peniko::ImageQuality::High)
        .unwrap();
    assert_eq!(
        scene.state.lock().unwrap().frozen_spawn_specs[&image.id]
            .media_frame
            .unwrap(),
        birth
    );
    assert_eq!(image.spec.lock().unwrap().media_frame.unwrap().width, 8.0);
}

/// An image made without a size fits the safe area and its frame is the
/// fitted size, so it takes only its own room in a layout; one made with
/// both sizes keeps that frame, as `frame()` does, so a crop fills it.
#[test]
fn image_frames_are_the_fitted_size_unless_both_sizes_are_given() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("page.png");
    image::RgbaImage::from_pixel(100, 200, image::Rgba([10, 20, 30, 255]))
        .save(&path)
        .unwrap();
    let mut scene = SceneModel::new(1280, 720);
    let safe = scene.safe_frame();
    let fitted = scene.image(&path).unwrap();
    let frame = fitted.spec.lock().unwrap().media_frame.unwrap();
    assert_eq!(
        (frame.width, frame.height),
        (safe.height() / 2.0, safe.height())
    );
    let framed = scene
        .image_with_options(
            &path,
            ImageOptions {
                width: Some(7.0),
                height: Some(4.0),
                ..Default::default()
            },
        )
        .unwrap();
    let frame = framed.spec.lock().unwrap().media_frame.unwrap();
    assert_eq!((frame.width, frame.height), (7.0, 4.0));
}

#[test]
fn segments_validate_names_links_and_duplicate_stops() {
    let mut canvas = SceneModel::new(1280, 720);
    assert!(matches!(
        canvas.segment("  ", None),
        Err(SegmentError::EmptyName)
    ));

    let first = canvas.segment("first", None).unwrap();
    assert!(matches!(
        canvas.segment("FIRST", None),
        Err(SegmentError::DuplicateName { .. })
    ));
    canvas.wait(1.0);
    canvas.stop(None).unwrap();
    assert!(matches!(
        canvas.stop(None),
        Err(SegmentError::DuplicateStopTime { .. })
    ));
    let second = canvas.segment("second", None).unwrap();
    assert!(
        canvas
            .link(&first, &second, TransitionType::CrossFade { duration: 0.2 })
            .is_ok()
    );

    let mut foreign_canvas = SceneModel::new(1280, 720);
    let foreign = foreign_canvas.segment("foreign", None).unwrap();
    assert!(matches!(
        canvas.link(&foreign, &second, TransitionType::Cut),
        Err(SegmentError::ForeignSegment)
    ));

    let mut unicode_canvas = SceneModel::new(1280, 720);
    unicode_canvas.segment("ÁREA", None).unwrap();
    assert!(matches!(
        unicode_canvas.segment("área", None),
        Err(SegmentError::DuplicateName { .. })
    ));
}

#[test]
fn segment_template_metadata_is_preserved() {
    let mut canvas = SceneModel::new(1000, 600);
    canvas
        .segment_with("content", None, None, Some("lecture".to_string()))
        .unwrap();
    let manifest = canvas.segment_manifest();
    assert_eq!(manifest.segments[0].template.as_deref(), Some("lecture"));
}

#[test]
fn segment_backgrounds_compile_with_scene_fallback_and_terminal_stop_hold() {
    let scene_color = Color::from_rgb8(10, 20, 30);
    let segment_color = Color::from_rgb8(40, 50, 60);
    let mut canvas = SceneModel::new(640, 360);
    canvas.set_background(Some(scene_color));
    canvas
        .segment_with_background(
            "intro",
            None,
            None,
            None,
            Some(gaanim_renderer::background::BackgroundPaint::solid(
                segment_color,
            )),
        )
        .unwrap();
    canvas.wait(1.0);
    canvas.stop(None).unwrap();
    canvas.segment("details", None).unwrap();
    canvas.wait(1.0);

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    let background = world
        .get_resource::<gaanim_renderer::pipeline::CanvasBackground>()
        .expect("compiled canvas background");

    assert_eq!(background.paint_at(0.5).fallback_color(), segment_color);
    assert_eq!(background.paint_at(1.0).fallback_color(), segment_color);
    assert_eq!(background.paint_at(1.1).fallback_color(), scene_color);
}

#[test]
fn post_process_uniforms_resolve_their_parameters() {
    use gaanim_renderer::post_process::{PostProcessPass, PostProcessShader};
    let shader = PostProcessShader::with_uniforms(
        "fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {\n\
             return gaanim_scene(uv) * gaanim_uniforms.gain;\n}",
        ["gain"],
    )
    .unwrap();
    let mut canvas = SceneModel::new(640, 360);
    let gain = canvas.parameter(0.5).unwrap();
    let mut other = SceneModel::new(640, 360);
    let foreign = other.parameter(1.0).unwrap();
    assert!(
        canvas
            .set_post_process(vec![
                PostProcessPass::new(shader.clone(), vec![foreign.source()]).unwrap()
            ])
            .is_err()
    );
    assert!(
        canvas
            .set_post_process(vec![
                PostProcessPass::constant(shader.clone(), &[f64::NAN]).unwrap()
            ])
            .is_err()
    );
    canvas
        .set_post_process(vec![
            PostProcessPass::new(shader.clone(), vec![gain.source()]).unwrap(),
            PostProcessPass::constant(shader, &[2.0]).unwrap(),
        ])
        .unwrap();
    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    let post = world
        .get_resource::<gaanim_renderer::post_process::CanvasPostProcess>()
        .unwrap()
        .clone();
    assert_eq!(post.passes.len(), 2);
    assert_eq!(post.parameters.len(), 1);
    let frame = gaanim_core::kurbo::Rect::new(0.0, 0.0, 64.0, 36.0);
    let request = post
        .request_with(0.0, frame, |entity| {
            world
                .get::<gaanim_animation::FloatSignal>(entity)
                .map(|signal| signal.value)
        })
        .unwrap();
    assert_eq!(request.passes[0].1, [0.5]);
    assert_eq!(request.passes[1].1, [2.0]);
}

#[test]
fn segment_post_processes_compile_with_scene_inheritance() {
    use gaanim_renderer::post_process::{PostProcessOverride, PostProcessShader};
    let shader = |scale: &str| {
        PostProcessShader::new(format!(
            "fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {{\n\
                 return gaanim_scene(uv) * {scale};\n}}"
        ))
        .unwrap()
    };
    let mut canvas = SceneModel::new(640, 360);
    canvas.set_post_process(vec![shader("0.5").into()]).unwrap();
    canvas.segment("inherits", None).unwrap();
    canvas.wait(1.0);
    let plain = canvas.segment("plain", None).unwrap();
    canvas
        .set_segment_post_process(&plain, PostProcessOverride::Disabled)
        .unwrap();
    canvas.wait(1.0);
    let custom = canvas.segment("custom", None).unwrap();
    canvas
        .set_segment_post_process(
            &custom,
            PostProcessOverride::Passes(vec![shader("0.25").into()]),
        )
        .unwrap();
    canvas.wait(1.0);
    let foreign = SceneModel::new(640, 360).segment("other", None).unwrap();
    assert!(matches!(
        canvas.set_segment_post_process(&foreign, PostProcessOverride::Disabled),
        Err(SegmentError::ForeignSegment)
    ));

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    let post = world
        .get_resource::<gaanim_renderer::post_process::CanvasPostProcess>()
        .expect("compiled canvas post-process");

    let source_at = |time| {
        post.passes_at(time)
            .first()
            .map(|pass| pass.shader.source().to_owned())
    };
    assert!(source_at(0.5).unwrap().contains("* 0.5"));
    assert_eq!(source_at(1.5), None);
    assert!(source_at(2.5).unwrap().contains("* 0.25"));
}

#[test]
fn segment_layout_aliases_and_branding_are_reusable() {
    let mut canvas = SceneModel::new(1280, 720);
    canvas.set_branding(PresentationBrand {
        footer: Some("Slides · Research Lab".to_owned()),
        show_on_cover: false,
        ..Default::default()
    });
    let before = canvas
        .state
        .lock()
        .expect("canvas state poisoned")
        .active()
        .ops
        .len();
    canvas
        .segment_with("cover", None, None, Some("title_slide".to_string()))
        .expect("cover segment");
    let after_cover = canvas
        .state
        .lock()
        .expect("canvas state poisoned")
        .active()
        .ops
        .len();
    assert_eq!(after_cover, before, "cover branding is opt-in");

    canvas.wait(0.1);
    canvas
        .segment_with("content", None, None, Some("lecture".to_string()))
        .expect("content segment");
    let after_content = canvas
        .state
        .lock()
        .expect("canvas state poisoned")
        .active()
        .ops
        .len();
    assert_eq!(
        after_content, 2,
        "rule and numbered footer are added to the active segment"
    );
}

#[test]
fn branding_skips_the_first_segment_without_a_cover_template() {
    let mut canvas = SceneModel::new(16.0, 9.0);
    canvas.set_branding(PresentationBrand {
        footer: Some("MI CHARLA".to_owned()),
        ..Default::default()
    });
    let ops = |canvas: &SceneModel| {
        canvas
            .state
            .lock()
            .expect("canvas state poisoned")
            .active()
            .ops
            .len()
    };
    canvas
        .segment_with("Portada", None, None, None)
        .expect("first segment");
    assert_eq!(ops(&canvas), 0, "the first segment is the cover");
    canvas.wait(0.1);
    canvas
        .segment_with("Contenido", None, None, None)
        .expect("second segment");
    assert_eq!(ops(&canvas), 2);
}

/// With `number_anchor` the slide number leaves the footer for its own
/// corner of the safe frame (#304).
#[test]
fn branding_places_the_slide_number_on_its_own() {
    let mut canvas = SceneModel::new(16.0, 9.0);
    canvas.set_branding(PresentationBrand {
        footer: Some("MI CHARLA".to_owned()),
        number_anchor: Some(Anchor::BottomRight),
        ..Default::default()
    });
    canvas.segment_with("Portada", None, None, None).unwrap();
    canvas.wait(0.1);
    canvas.segment_with("Contenido", None, None, None).unwrap();
    let spawned: Vec<_> = canvas
        .state
        .lock()
        .unwrap()
        .active()
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Spawn(shared) => {
                let spec = shared.lock().unwrap();
                match &spec.kind {
                    SpawnKind::Text(text) => Some((spec.id, text.plain_text(), shared.clone())),
                    _ => None,
                }
            }
            _ => None,
        })
        .collect();
    let texts: Vec<(String, f64)> = spawned
        .into_iter()
        .map(|(id, text, spec)| {
            let mut handle =
                DrawableHandle::new(id, SpawnKind::Group(vec![]), canvas.state.clone(), 1);
            handle.spec = spec;
            (text, canvas.bounds_of(&handle).unwrap().center().x)
        })
        .collect();
    assert_eq!(texts.len(), 2, "{texts:?}");
    assert!(
        texts
            .iter()
            .any(|(text, x)| text == "MI CHARLA" && *x < 0.0),
        "{texts:?}"
    );
    assert!(
        texts.iter().any(|(text, x)| text == "02" && *x > 6.0),
        "{texts:?}"
    );
}

#[test]
fn required_constraint_conflicts_fail_during_authoring() {
    let mut canvas = SceneModel::new(320, 180);
    let object = canvas.circle(10.0);
    let left = gaanim_layout::LayoutExpression::variable(
        gaanim_layout::LayoutId(object.id.as_raw()),
        gaanim_layout::LayoutAttribute::Left,
    );
    let result = canvas.constrain_layout(
        vec![
            gaanim_layout::LayoutConstraint::equal(left.clone(), 10.0.into()),
            gaanim_layout::LayoutConstraint::equal(left, 20.0.into()),
        ],
        None,
    );
    assert!(matches!(
        result,
        Err(SceneObjectError::Layout(
            gaanim_layout::LayoutError::Unsatisfiable(_)
        ))
    ));
}

#[test]
fn weak_constraint_diagnostics_are_available_before_compile() {
    let mut canvas = SceneModel::new(320, 180);
    let object = canvas.circle(10.0);
    let left = gaanim_layout::LayoutExpression::variable(
        gaanim_layout::LayoutId(object.id.as_raw()),
        gaanim_layout::LayoutAttribute::Left,
    );
    canvas
        .constrain_layout(
            vec![
                gaanim_layout::LayoutConstraint::equal(left.clone(), 10.0.into()),
                gaanim_layout::LayoutConstraint::equal(left, 20.0.into())
                    .with_strength(gaanim_layout::ConstraintStrength::Weak),
            ],
            None,
        )
        .unwrap();
    let diagnostics = canvas.check_layout();
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].contains("residual 10.000000"));

    let mut foreign = SceneModel::new(320, 180);
    let foreign_object = foreign.circle(10.0);
    assert!(!canvas.owns_drawable(&foreign_object));
}

#[test]
fn layout_ownership_adopts_positioned_and_transformed_drawables() {
    let mut canvas = SceneModel::new(320, 180);
    let owner = canvas.group(&[]);
    // A drawable placed before joining a box is simply placed by it.
    let positioned = canvas.circle(10.0).move_to(12.0, 0.0);
    assert!(positioned.claim_layout(&owner).is_ok());

    let animated = canvas.circle(10.0);
    let _description = animated.animate().shift_by(8.0, 0.0);
    assert!(animated.claim_layout(&owner).is_ok());

    let visual = canvas.circle(10.0);
    let _ = visual.animate().scale_by(1.5).rotate_by(0.25);
    assert!(visual.claim_layout(&owner).is_ok());

    let mut foreign_canvas = SceneModel::new(320, 180);
    let foreign = foreign_canvas.circle(10.0);
    assert_eq!(
        foreign.claim_layout(&owner),
        Err(crate::canvas::LayoutOwnershipError::ForeignScene)
    );
}

fn leaf_opacities_at(canvas: &SceneModel, time: f64) -> Vec<f32> {
    let mut app = bevy::prelude::App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin);
    canvas.compile(app.world_mut());
    app.finish();
    app.cleanup();
    app.update();
    app.world_mut().resource_mut::<Timeline>().seek_request = Some(time);
    app.update();
    app.update();
    let mut opacities: Vec<f32> = app
        .world_mut()
        .query::<(&gaanim_scene::FillBrush, &gaanim_scene::GlobalOpacity)>()
        .iter(app.world())
        .filter(|(fill, _)| fill.0.is_some())
        .map(|(_, opacity)| opacity.0)
        .collect();
    opacities.sort_by(f32::total_cmp);
    opacities
}

#[test]
fn group_opacity_is_one_multiplier_for_setters_and_animations() {
    // A group hidden immediately must reappear when its opacity animates.
    let mut canvas = SceneModel::new(320, 180);
    let dot = canvas.circle(20.0).fill(Color::WHITE);
    let group = canvas.group(&[&dot]).opacity(0.0);
    canvas.play(vec![group.animate().opacity(1.0).duration(1.0)]);
    assert_eq!(leaf_opacities_at(&canvas, 0.0), vec![0.0]);
    assert_eq!(leaf_opacities_at(&canvas, 1.0), vec![1.0]);

    // A nested group applies its opacity once, not once per level.
    let mut canvas = SceneModel::new(320, 180);
    let dot = canvas.circle(20.0).fill(Color::WHITE);
    let inner = canvas.group(&[&dot]);
    canvas.group(&[&inner]).opacity(0.5);
    canvas.wait(0.5);
    assert_eq!(leaf_opacities_at(&canvas, 0.5), vec![0.5]);

    // Immediate cuts after the declaration freezes follow the same rule.
    let mut canvas = SceneModel::new(320, 180);
    let dot = canvas.circle(20.0).fill(Color::WHITE);
    let group = canvas.group(&[&dot]);
    canvas.wait(0.5);
    let group = group.opacity(0.0);
    canvas.play(vec![group.animate().opacity(0.4).duration(0.5)]);
    assert_eq!(leaf_opacities_at(&canvas, 0.5), vec![0.0]);
    let opacity = leaf_opacities_at(&canvas, 1.0)[0];
    assert!((opacity - 0.4).abs() < 1e-5, "{opacity}");
}

#[test]
fn headless_derived_geometry_resolves_fill_level_and_outline() {
    let mut canvas = SceneModel::new(320, 180);
    canvas.text("Vector fill level");
    let mask = canvas
        .circle(40.0)
        .no_fill()
        .stroke(Color::WHITE, 3.0)
        .move_to(0.0, -10.0)
        .opacity(0.0);
    let fill = canvas
        .fill_level(
            &mask,
            Brush::Solid(Color::from_rgb8(56, 189, 248)),
            0.0,
            FillLevelDirection::Up,
            true,
        )
        .unwrap();
    canvas.play(vec![fill.animate().fill_level(0.75).duration(1.2)]);

    let mut app = bevy::prelude::App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
    canvas.compile(app.world_mut());
    app.finish();
    app.cleanup();
    app.update();
    app.world_mut().resource_mut::<Timeline>().seek_request = Some(1.2);
    app.update();

    let fill_paths = app
        .world_mut()
        .query_filtered::<(
            &gaanim_scene::Path2D,
            &gaanim_renderer::effects::FillLevelBinding,
            Option<&gaanim_scene::FillBrush>,
            Option<&gaanim_scene::GlobalOpacity>,
            Option<&gaanim_scene::WorldBounds>,
            Option<&gaanim_scene::Visible>,
        ), ()>()
        .iter(app.world())
        .map(|(path, binding, fill, opacity, bounds, visible)| {
            (
                path.0.is_empty(),
                binding.sources.len(),
                fill.is_some_and(|fill| fill.0.is_some()),
                opacity.map(|opacity| opacity.0),
                bounds.map(|bounds| bounds.0.width()),
                visible.is_some(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(fill_paths.len(), 1);
    assert!(!fill_paths[0].0);
    assert_eq!(fill_paths[0].1, 1);
    assert!(fill_paths[0].2);
    assert_eq!(fill_paths[0].3, Some(1.0));
    assert!(fill_paths[0].5);
    assert!(fill_paths[0].4.is_none());

    let outline_paths = app
        .world_mut()
        .query_filtered::<(
            &gaanim_scene::Path2D,
            &gaanim_renderer::effects::VectorOutlineBinding,
            Option<&gaanim_scene::StrokeBrush>,
            Option<&gaanim_scene::GlobalOpacity>,
            Option<&gaanim_scene::WorldBounds>,
            Option<&gaanim_scene::Visible>,
        ), ()>()
        .iter(app.world())
        .map(|(path, binding, stroke, opacity, bounds, visible)| {
            (
                path.0.is_empty(),
                binding.sources.len(),
                stroke.is_some_and(|stroke| stroke.brush.is_some()),
                opacity.map(|opacity| opacity.0),
                bounds.map(|bounds| bounds.0.width()),
                visible.is_some(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(outline_paths.len(), 1);
    assert!(!outline_paths[0].0);
    assert_eq!(outline_paths[0].1, 1);
    assert!(outline_paths[0].2);
    assert_eq!(outline_paths[0].3, Some(1.0));
    assert!(outline_paths[0].5);
    assert!(outline_paths[0].4.is_none());
}

#[test]
fn custom_animation_composes_and_preserves_exact_seek_and_following_baselines() {
    use gaanim_animation::{CustomAnimation, CustomChannel, CustomValues};
    let mut canvas = SceneModel::new(320, 180);
    let dot = canvas.circle(10.0).move_to(4.0, 0.0);
    let callback = CustomAnimation::new(vec![CustomChannel::Position], |alpha| {
        Ok(CustomValues {
            position: Some(DVec3::new(4.0 + alpha * alpha * 100.0, 0.0, 0.0)),
            ..Default::default()
        })
    })
    .unwrap();
    let custom = dot
        .animate()
        .custom(callback)
        .unwrap()
        .duration(2.0)
        .rate_func(RateFunc::Linear);
    let opacity = dot
        .animate()
        .opacity(0.5)
        .duration(2.0)
        .rate_func(RateFunc::Linear);
    canvas
        .play_items(vec![custom.into(), opacity.into()])
        .unwrap();
    canvas
        .play_items(vec![
            dot.animate()
                .shift_by(10.0, 0.0)
                .duration(1.0)
                .rate_func(RateFunc::Linear)
                .into(),
        ])
        .unwrap();

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    let entity = world
        .query::<(bevy::prelude::Entity, &MobjectId)>()
        .iter(&world)
        .next()
        .unwrap()
        .0;
    let mut timeline = world.remove_resource::<Timeline>().unwrap();
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    for (time, x) in [
        (0.2469, 4.0 + 0.12345_f64.powi(2) * 100.0),
        (3.0, 114.0),
        (2.5, 109.0),
        (0.2469, 4.0 + 0.12345_f64.powi(2) * 100.0),
        (0.0, 4.0),
    ] {
        timeline.seek(&mut world, time);
        assert!((world.get::<SpatialTransform>(entity).unwrap().translation.x - x).abs() < 1e-9);
    }
}

#[test]
fn custom_animation_endpoint_uses_easing_before_following_relative_animation() {
    use gaanim_animation::{CustomAnimation, CustomChannel, CustomValues};
    let mut canvas = SceneModel::new(320, 180);
    let dot = canvas.circle(10.0).move_to(4.0, 0.0);
    let callback = CustomAnimation::new(vec![CustomChannel::Position], |alpha| {
        Ok(CustomValues {
            position: Some(DVec3::new(4.0 + alpha * 100.0, 0.0, 0.0)),
            ..Default::default()
        })
    })
    .unwrap();
    let custom = dot
        .animate()
        .custom(callback)
        .unwrap()
        .duration(1.0)
        .rate_func(RateFunc::ThereAndBack);
    let after = dot
        .animate()
        .shift_by(10.0, 0.0)
        .duration(1.0)
        .rate_func(RateFunc::Linear);
    canvas
        .play_composition_configured(
            Composition::sequence(
                vec![Composition::leaf(custom), Composition::leaf(after)],
                0.0,
            )
            .unwrap(),
            None,
            None,
        )
        .unwrap();
    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    let entity = world
        .query::<(bevy::prelude::Entity, &MobjectId)>()
        .iter(&world)
        .next()
        .unwrap()
        .0;
    let mut timeline = world.remove_resource::<Timeline>().unwrap();
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    for (time, x) in [(0.5, 104.0), (2.0, 14.0), (1.5, 9.0), (0.0, 4.0)] {
        timeline.seek(&mut world, time);
        assert!((world.get::<SpatialTransform>(entity).unwrap().translation.x - x).abs() < 1e-9);
    }
}

#[test]
fn custom_animation_rejects_conflicts_and_invalid_outputs_without_consuming() {
    use gaanim_animation::{CustomAnimation, CustomChannel, CustomValues};
    let mut scene = SceneModel::new(320, 180);
    let dot = scene.circle(10.0);
    let valid = CustomAnimation::new(vec![CustomChannel::Opacity], |alpha| {
        Ok(CustomValues {
            opacity: Some(alpha as f32),
            ..Default::default()
        })
    })
    .unwrap();
    let custom = dot.animate().custom(valid).unwrap();
    assert!(
        matches!(scene.play_items(vec![custom.clone().into(), dot.animate().opacity(0.2).into()]), Err(PlayError::ConflictingChannel { channel, .. }) if channel == "opacity")
    );
    assert_eq!(scene.current_time(), 0.0);
    scene.play_items(vec![custom.into()]).unwrap();
    let invalid =
        CustomAnimation::new(
            vec![CustomChannel::Opacity],
            |_| Ok(CustomValues::default()),
        )
        .unwrap();
    let cursor = scene.current_time();
    assert!(matches!(
        scene.play_items(vec![dot.animate().custom(invalid).unwrap().into()]),
        Err(PlayError::CustomAnimation(_))
    ));
    assert_eq!(scene.current_time(), cursor);
}

#[test]
fn animation_descriptions_are_pure_until_play() {
    let mut scene = SceneModel::new(320, 180);
    let dot = scene.circle(10.0);
    let before_ops = scene.state.lock().unwrap().active().ops.len();
    let anim = dot
        .animate()
        .shift_by(20.0, 0.0)
        .fill(Color::from_rgb8(255, 0, 0))
        .duration(0.75)
        .delay(0.25);
    assert_eq!(scene.current_time(), 0.0);
    assert_eq!(scene.state.lock().unwrap().active().ops.len(), before_ops);

    scene.play_items(vec![anim.into()]).unwrap();
    assert_eq!(scene.current_time(), 1.0);
    assert!(matches!(
        scene.state.lock().unwrap().active().ops.last(),
        Some(Op::Play(_))
    ));
}

#[test]
fn play_rejects_reuse_duplicates_foreign_owners_and_channel_conflicts() {
    let mut scene = SceneModel::new(320, 180);
    let dot = scene.circle(10.0);
    let used = dot.animate().opacity(0.5);
    scene.play_items(vec![used.clone().into()]).unwrap();
    assert_eq!(
        scene.play_items(vec![used.into()]),
        Err(PlayError::AnimationAlreadyConsumed)
    );

    let duplicate = dot.animate().fill(Color::from_rgb8(255, 0, 0));
    assert_eq!(
        scene.play_items(vec![duplicate.clone().into(), duplicate.into()]),
        Err(PlayError::DuplicateAnimation)
    );

    let left = dot.animate().shift_by(10.0, 0.0);
    let right = dot.animate().move_to(20.0, 0.0);
    assert!(matches!(
        scene.play_items(vec![left.into(), right.into()]),
        Err(PlayError::ConflictingChannel { channel, .. }) if channel == "translation"
    ));

    let line = scene.line(-1.0, 0.0, 1.0, 0.0);
    let error = scene
        .play_items(vec![
            line.animate()
                .trim(None, Some(1.0), None)
                .duration(1.0)
                .into(),
            line.animate()
                .trim(Some(1.0), None, None)
                .duration(1.0)
                .into(),
        ])
        .expect_err("overlapping trims share the effect channel");
    let message = error.to_string();
    assert!(
        message.contains("Trim (0.00–1.00 s)")
            && message.contains("same line")
            && message.contains("'effect'"),
        "{message}"
    );

    let mut foreign_scene = SceneModel::new(320, 180);
    let foreign = foreign_scene.circle(10.0).animate().fade_in();
    assert_eq!(
        scene.play_items(vec![foreign.into()]),
        Err(PlayError::ForeignAnimation)
    );
}

#[test]
fn immediate_setters_after_time_advance_record_zero_duration_cuts() {
    let mut scene = SceneModel::new(320, 180);
    let dot = scene.circle(10.0).move_to(0.0, 0.0).fill(Color::WHITE);
    scene.wait(1.0);
    let cursor = scene.current_time();
    let initial = scene
        .state
        .lock()
        .unwrap()
        .frozen_spawn_specs
        .get(&dot.id)
        .cloned()
        .unwrap();
    dot.clone()
        .shift_by(12.0, 0.0)
        .fill(Color::from_rgb8(255, 0, 0));
    let state = scene.state.lock().unwrap();
    assert_eq!(state.active().cursor, cursor);
    assert!(
        state
            .active()
            .ops
            .iter()
            .any(|op| matches!(op, Op::Immediate(_)))
    );
    assert_eq!(initial.fill, Some(Brush::Solid(Color::WHITE)));
}

#[test]
fn composition_resolves_nested_spans_without_flattening_early() {
    let mut scene = SceneModel::new(320, 180);
    let a = scene.circle(10.0).animate().fade_in().duration(1.0);
    let b = scene.circle(10.0).animate().fade_in().duration(2.0);
    let c = scene.circle(10.0).animate().fade_in().duration(1.0);
    let group = Composition::parallel(vec![Composition::leaf(b), Composition::leaf(c)]).unwrap();
    let plan = Composition::sequence(vec![Composition::leaf(a), group], -0.25).unwrap();
    let schedule = plan.schedule(None).unwrap();
    assert_eq!(schedule.span, 2.75);
    assert_eq!(schedule.entries[0].start, 0.0);
    assert_eq!(schedule.entries[1].start, 0.75);
    assert_eq!(schedule.entries[2].start, 0.75);
    assert_eq!(scene.current_time(), 0.0);
}

// -- TM-05: labels, relative positions and scene markers --

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "expected {expected}, got {actual}"
    );
}

fn fade(scene: &mut SceneModel, seconds: f64) -> Composition {
    Composition::leaf(scene.circle(10.0).animate().fade_in().duration(seconds))
}

fn at(position: &str) -> InsertPosition {
    InsertPosition::parse(position).unwrap()
}

#[test]
fn composition_labels_resolve_gsap_style_positions() {
    let mut scene = SceneModel::new(320, 180);
    let title = fade(&mut scene, 0.8);
    let subtitle = fade(&mut scene, 0.4);
    let plan = Composition::sequence(
        vec![title, Composition::label("golpe").unwrap(), subtitle],
        0.1,
    )
    .unwrap()
    .insert(fade(&mut scene, 1.0), at("golpe+0.15"))
    .unwrap()
    .insert(fade(&mut scene, 0.5), at("<"))
    .unwrap()
    .insert(fade(&mut scene, 0.2), at("-=0.3"))
    .unwrap()
    .insert(fade(&mut scene, 0.2), at(">+0.1"))
    .unwrap()
    .insert(fade(&mut scene, 0.2), at("<+=0.05"))
    .unwrap()
    .insert(fade(&mut scene, 0.2), at("+=0.5"))
    .unwrap()
    .insert(fade(&mut scene, 0.2), InsertPosition::at(0.25).unwrap())
    .unwrap();
    let schedule = plan.schedule(None).unwrap();
    // The label takes no gap: it marks where the subtitle starts.
    assert_eq!(schedule.labels.len(), 1);
    assert_eq!(schedule.labels[0].name, "golpe");
    assert_close(schedule.labels[0].time, 0.9);
    let starts: Vec<f64> = schedule.entries.iter().map(|entry| entry.start).collect();
    let expected = [0.0, 0.9, 1.05, 1.05, 1.75, 2.05, 2.1, 2.8, 0.25];
    for (actual, expected) in starts.iter().zip(expected) {
        assert_close(*actual, expected);
    }
    // Inserted items follow the children in path order.
    assert_eq!(schedule.entries[2].path, vec![3]);
    assert_close(schedule.span, 3.0);
    assert_eq!(scene.current_time(), 0.0);
}

#[test]
fn composition_labels_nest_shift_and_stretch() {
    let mut scene = SceneModel::new(320, 180);
    let inner = Composition::sequence(
        vec![
            fade(&mut scene, 1.0),
            Composition::label("mid").unwrap(),
            fade(&mut scene, 1.0),
            Composition::label("tail").unwrap(),
        ],
        0.0,
    )
    .unwrap();
    let first = fade(&mut scene, 0.5);
    let plan = Composition::sequence(
        vec![Composition::label("start").unwrap(), first, inner],
        0.0,
    )
    .unwrap()
    .insert(Composition::label("late").unwrap(), at("tail-0.25"))
    .unwrap()
    .insert(fade(&mut scene, 0.5), at("late"))
    .unwrap()
    .delay(1.0)
    .unwrap();
    let labels = plan.schedule(None).unwrap().labels;
    let times: Vec<(&str, f64)> = labels
        .iter()
        .map(|label| (label.name.as_str(), label.time))
        .collect();
    assert_eq!(
        times,
        vec![("start", 1.0), ("mid", 2.5), ("late", 3.25), ("tail", 3.5)]
    );
    let stretched = Composition::parallel(vec![
        fade(&mut scene, 2.0),
        Composition::label("half").unwrap().delay(1.0).unwrap(),
    ])
    .unwrap()
    .stretch(4.0)
    .unwrap()
    .repeat(2, 0.0)
    .unwrap();
    let schedule = stretched.schedule(None).unwrap();
    assert_eq!(schedule.labels.len(), 1);
    assert_close(schedule.labels[0].time, 2.0);
    assert_close(schedule.span, 8.0);
}

#[test]
fn composition_label_errors_are_clear() {
    let mut scene = SceneModel::new(320, 180);
    assert!(matches!(
        Composition::label("  "),
        Err(PlayError::EmptyLabel)
    ));
    assert!(matches!(
        Composition::label("1.5"),
        Err(PlayError::InvalidLabel(_))
    ));
    for malformed in [
        "", "  ", "<x", ">+", "+=abc", "-=-1", "-1", "=a", "+0.2", "nan",
    ] {
        assert!(
            matches!(
                InsertPosition::parse(malformed),
                Err(PlayError::InvalidInsertPosition(_))
            ),
            "{malformed:?} should be rejected"
        );
    }
    assert!(InsertPosition::at(-0.1).is_err());

    let base = Composition::sequence(
        vec![fade(&mut scene, 1.0), Composition::label("hit").unwrap()],
        0.0,
    )
    .unwrap();
    let unknown = base
        .clone()
        .insert(fade(&mut scene, 1.0), at("missing+0.2"))
        .unwrap();
    let error = unknown.schedule(None).unwrap_err();
    assert_eq!(
        error,
        PlayError::UnknownLabel {
            name: "missing".into(),
            known: "\"hit\"".into()
        }
    );
    assert!(error.to_string().contains("unknown label \"missing\""));
    let before_start = base
        .clone()
        .insert(fade(&mut scene, 1.0), at("hit-2"))
        .unwrap();
    assert!(matches!(
        before_start.schedule(None),
        Err(PlayError::InvalidInsertPosition(message)) if message.contains("before the composition start")
    ));
    let duplicate =
        Composition::parallel(vec![base.clone(), Composition::label("hit").unwrap()]).unwrap();
    assert_eq!(
        duplicate.schedule(None).unwrap_err(),
        PlayError::DuplicateLabel("hit".into())
    );
}

#[test]
fn composition_exact_label_names_win_over_offsets() {
    let mut scene = SceneModel::new(320, 180);
    let plan = Composition::sequence(
        vec![
            fade(&mut scene, 1.0),
            Composition::label("part-2").unwrap(),
            fade(&mut scene, 1.0),
            Composition::label("intro-end").unwrap(),
        ],
        0.0,
    )
    .unwrap()
    .insert(fade(&mut scene, 0.1), at("part-2"))
    .unwrap()
    .insert(fade(&mut scene, 0.1), at("part-2+0.5"))
    .unwrap()
    .insert(fade(&mut scene, 0.1), at("intro-end-=0.25"))
    .unwrap();
    let starts: Vec<f64> = plan
        .schedule(None)
        .unwrap()
        .entries
        .iter()
        .map(|entry| entry.start)
        .collect();
    for (actual, expected) in starts.iter().zip([0.0, 1.0, 1.0, 1.5, 1.75]) {
        assert_close(*actual, expected);
    }
}

#[test]
fn composition_labels_play_like_their_resolved_schedule() {
    let mut scene = SceneModel::new(320, 180);
    let plan = Composition::sequence(
        vec![fade(&mut scene, 1.0), Composition::label("hit").unwrap()],
        0.0,
    )
    .unwrap()
    .insert(fade(&mut scene, 0.5), at("hit+0.5"))
    .unwrap();
    let span = plan.schedule(None).unwrap().span;
    scene.play_composition_configured(plan, None, None).unwrap();
    assert_close(span, 2.0);
    assert_close(scene.current_time(), 2.0);
}

#[test]
fn scene_markers_are_absolute_unique_and_segment_aware() {
    let mut scene = SceneModel::new(320, 180);
    scene.marker("intro").unwrap();
    scene.segment("first", None).unwrap();
    scene.wait(1.5);
    scene.marker(" climax ").unwrap();
    scene.segment("second", None).unwrap();
    scene.wait(0.5);
    scene.marker("fin").unwrap();
    let markers = scene.markers();
    let summary: Vec<(&str, f64, &str)> = markers
        .iter()
        .map(|marker| (marker.name.as_str(), marker.time, marker.segment.as_str()))
        .collect();
    assert_eq!(
        summary,
        vec![
            ("intro", 0.0, "first"),
            ("climax", 1.5, "first"),
            ("fin", 2.0, "second")
        ]
    );
    assert_eq!(scene.marker_time("fin"), Some(2.0));
    assert_eq!(scene.marker_time("missing"), None);
    assert!(matches!(
        scene.marker("climax"),
        Err(SegmentError::DuplicateMarker { .. })
    ));
    assert!(matches!(
        scene.marker("12.5"),
        Err(SegmentError::NumericMarkerName { .. })
    ));
    assert!(matches!(
        scene.marker(""),
        Err(SegmentError::EmptyMarkerName)
    ));
    // Markers are metadata: they neither move the cursor nor add stops.
    assert_eq!(scene.current_time(), 2.0);
    assert!(
        scene
            .segment_manifest()
            .segments
            .iter()
            .all(|segment| segment.stops.is_empty())
    );
}

#[test]
fn staggered_sequences_that_touch_do_not_overlap_by_rounding() {
    let mut scene = SceneModel::new(320, 180);
    let trips = (0..18)
        .map(|_| {
            let packet = scene.rect(0.3, 0.1).move_to(-5.0, 0.0);
            Composition::sequence(
                vec![
                    Composition::leaf(packet.animate().opacity(1.0).duration(0.08)),
                    Composition::leaf(packet.animate().move_to(0.0, 0.0).duration(0.45)),
                    Composition::leaf(packet.animate().move_to(3.0, 1.0).duration(0.25)),
                    Composition::leaf(packet.animate().opacity(0.0).duration(0.08)),
                ],
                0.0,
            )
            .unwrap()
        })
        .collect();
    let staggered = Composition::stagger(trips, 0.13).unwrap();
    scene
        .play_composition_configured(staggered, None, None)
        .expect("adjacent sequence steps share the translation channel in turn");
}

/// A glTF model's animations write the pose of its nodes alone, so
/// they combine with effects and moves of the model.
#[test]
fn gltf_plays_write_the_pose_channel() {
    let mut scene = SceneModel::new(320, 180);
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/fixtures/assets/Wolf.glb");
    let wolf = scene.gltf(&path, &Default::default()).unwrap();
    let walk = wolf
        .animation("Walk", None, 1.0, true, false, 0.0, 0.0)
        .unwrap();
    assert_eq!(animation_channels(&walk), [GLTF_POSE]);
}

#[test]
fn composition_allows_sequential_channel_reuse_but_rejects_overlap() {
    let mut scene = SceneModel::new(320, 180);
    let dot = scene.circle(10.0);
    let sequence = Composition::sequence(
        vec![
            Composition::leaf(dot.animate().shift_by(10.0, 0.0)),
            Composition::leaf(dot.animate().shift_by(10.0, 0.0)),
        ],
        0.0,
    )
    .unwrap();
    scene
        .play_composition_configured(sequence, None, None)
        .unwrap();
    assert_eq!(scene.current_time(), 2.0);

    let overlap = Composition::sequence(
        vec![
            Composition::leaf(dot.animate().shift_by(10.0, 0.0)),
            Composition::leaf(dot.animate().shift_by(10.0, 0.0)),
        ],
        -0.25,
    )
    .unwrap();
    assert!(matches!(
        scene.play_composition_configured(overlap, None, None),
        Err(PlayError::ConflictingChannel { channel, .. }) if channel == "translation"
    ));
}

#[test]
fn composition_defaults_and_stretch_resolve_before_scheduling() {
    let mut scene = SceneModel::new(320, 180);
    let first = scene.circle(10.0).animate().fade_in();
    let second = scene.circle(10.0).animate().fade_in().duration(2.0);
    let plan = Composition::stagger(
        vec![Composition::leaf(first), Composition::leaf(second)],
        0.5,
    )
    .unwrap()
    .defaults(Some(0.75), Some(RateFunc::Linear))
    .unwrap()
    .stretch(5.0)
    .unwrap();
    let schedule = plan.schedule(None).unwrap();
    assert_eq!(schedule.span, 5.0);
    assert_eq!(schedule.entries[0].duration, Some(1.5));
    assert_eq!(schedule.entries[1].start, 1.0);
    assert_eq!(schedule.entries[1].duration, Some(4.0));
}

#[test]
fn speed_ramps_and_remaps_place_children_at_their_played_times() {
    let mut scene = SceneModel::new(320, 180);
    let fades: Vec<Composition> = (0..2)
        .map(|_| Composition::leaf(scene.circle(10.0).animate().fade_in().duration(1.0)))
        .collect();
    let slow = Composition::sequence(fades, 0.0)
        .unwrap()
        .speed_ramp(vec![(0.0, 0.5)])
        .unwrap();
    let schedule = slow.schedule(None).unwrap();
    assert!((schedule.span - 4.0).abs() < 1e-9);
    assert!((schedule.entries[1].start - 2.0).abs() < 1e-9);
    assert!((schedule.entries[1].duration.unwrap() - 2.0).abs() < 1e-9);

    // Nested: the ramp's played span is what the outer sequence steps by,
    // and a stretch afterwards scales the played times.
    let inner = Composition::sequence(
        (0..2)
            .map(|_| Composition::leaf(scene.circle(10.0).animate().fade_in()))
            .collect(),
        0.0,
    )
    .unwrap()
    .speed_ramp(vec![(0.0, 2.0)])
    .unwrap();
    let after = Composition::leaf(scene.circle(10.0).animate().fade_in());
    let outer = Composition::sequence(vec![inner, after], 0.0)
        .unwrap()
        .stretch(4.0)
        .unwrap();
    let schedule = outer.schedule(None).unwrap();
    assert!((schedule.span - 4.0).abs() < 1e-9);
    let starts: Vec<f64> = schedule.entries.iter().map(|entry| entry.start).collect();
    assert!((starts[1] - 1.0).abs() < 1e-9 && (starts[2] - 2.0).abs() < 1e-9);

    // A remap keeps the span and the order of its children.
    let remapped = Composition::stagger(
        (0..4)
            .map(|_| Composition::leaf(scene.circle(10.0).animate().fade_in()))
            .collect(),
        0.5,
    )
    .unwrap()
    .time_remap(RateFunc::EaseInOut(gaanim_math::EasingCurve::Cubic))
    .unwrap();
    let schedule = remapped.schedule(None).unwrap();
    assert!((schedule.span - 2.5).abs() < 1e-9);
    assert!(
        schedule
            .entries
            .windows(2)
            .all(|pair| pair[0].start < pair[1].start)
    );

    let bounce =
        Composition::leaf(scene.circle(10.0).animate().fade_in()).time_remap(RateFunc::Back {
            overshoot: 1.7,
            mode: gaanim_math::EaseMode::InOut,
        });
    assert!(matches!(bounce, Err(PlayError::InvalidTimeWarp(_))));
}
