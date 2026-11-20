use super::*;

fn frame(time: f64) -> Frame {
    let mut camera = gaanim_math::Camera::ortho_2d(1280, 720);
    camera.position.x = time;
    Frame {
        time,
        camera,
        capture: Default::default(),
        post: Vec::new(),
        motion_blur: Vec::new(),
    }
}

/// An element of the drawable `raw`: a unit square at `x`.
fn square(raw: u32, x: f64, post_exempt: bool) -> CapturedElement {
    let entity = bevy::prelude::Entity::from_raw_u32(raw).unwrap();
    CapturedElement {
        entity,
        recipe: Arc::new(gaanim_renderer::fragment::FragmentRecipe {
            path: Some(Arc::new(gaanim_core::kurbo::Shape::to_path(
                &gaanim_core::kurbo::Rect::new(x, 0.0, x + 1.0, 1.0),
                0.1,
            ))),
            ..Default::default()
        }),
        lottie: None,
        transform: gaanim_core::kurbo::Affine::IDENTITY,
        opacity: 1.0,
        opacity_bounds: gaanim_core::kurbo::Rect::new(x, 0.0, x + 1.0, 1.0),
        opacity_reach: None,
        opacity_group: entity,
        group_opacity: 1.0,
        group_shadow: None,
        render_order: Default::default(),
        clip_mask: None,
        blend: None,
        transition_side: Default::default(),
        post_exempt,
        lineage: Vec::new(),
        view_bounds: None,
        in_views: true,
        layer: None,
        screen: None,
        echo_rank: 0,
        effect_root: None,
        effect_extent: None,
        matte_root: None,
        matte_of: None,
        glass_root: None,
    }
}

/// The left edge of each element's outline in `frame`.
fn lefts(frame: &Frame) -> Vec<f64> {
    frame
        .capture
        .elements
        .iter()
        .map(|element| {
            gaanim_core::kurbo::Shape::bounding_box(&**element.recipe.path.as_ref().unwrap()).x0
        })
        .collect()
}

/// A drawable that draws several elements in one frame, as the runs of a
/// 3D mesh do, keeps each of them in every frame, not only in the first
/// frame of a chunk.
#[test]
fn drawables_with_several_elements_keep_each_one() {
    let mut writer = BundleWriter::new(std::io::Cursor::new(Vec::new()), "test");
    let frames: Vec<Vec<f64>> = vec![
        vec![0.0, 2.0, 4.0],
        vec![0.5, 2.5, 4.5],
        vec![0.5, 2.5, 4.5],
        vec![1.0, 3.0],
    ];
    for (index, xs) in frames.iter().enumerate() {
        let mut captured = frame(index as f64 / 60.0);
        // Drawable 1 draws every square but the last; drawable 2 the last.
        captured.capture.elements = xs
            .iter()
            .enumerate()
            .map(|(at, x)| square(if at + 1 == xs.len() { 2 } else { 1 }, *x, false))
            .collect();
        writer.push_frame(&captured, [0; 32]).unwrap();
    }
    let scene = SceneData {
        fps: 60,
        duration: frames.len() as f64 / 60.0,
        ..Default::default()
    };
    let bytes = writer.finish(&scene).unwrap().into_inner();
    let mut bundle = Bundle::from_bytes(bytes.into()).unwrap();
    for (index, xs) in frames.iter().enumerate() {
        assert_eq!(&lefts(&bundle.frame(index).unwrap()), xs, "frame {index}");
    }
}

#[test]
fn the_camera_an_outgoing_segment_keeps_round_trips() {
    let mut kept = gaanim_math::Camera::perspective_3d(1280, 720, 0.6);
    kept.position = gaanim_core::glam::DVec3::new(9.0, 5.0, 8.0);
    kept.lens_shift = gaanim_core::glam::DVec2::new(-3.0, 0.5);
    let view = gaanim_core::kurbo::Affine::new([1.5, 0.0, 0.0, 1.5, -2.0, 1.0]);
    let transition = |outgoing_camera| gaanim_renderer::pipeline::CapturedTransition {
        outgoing_camera,
        outgoing_view: outgoing_camera.map_or(gaanim_core::kurbo::Affine::IDENTITY, |_| view),
        ..Default::default()
    };
    let mut writer = BundleWriter::new(std::io::Cursor::new(Vec::new()), "test");
    for (index, camera) in [Some(kept), None].into_iter().enumerate() {
        let mut recorded = frame(index as f64 / 60.0);
        recorded.capture.elements = vec![square(1, 0.0, false)];
        recorded.capture.transition = Some(transition(camera));
        writer.push_frame(&recorded, [index as u8; 32]).unwrap();
    }
    let scene = SceneData {
        fps: 60,
        duration: 2.0 / 60.0,
        ..Default::default()
    };
    let mut bundle =
        Bundle::from_bytes(writer.finish(&scene).unwrap().into_inner().into()).unwrap();
    let read = |bundle: &mut Bundle, index| {
        let transition = bundle.frame(index).unwrap().capture.transition.unwrap();
        (transition.outgoing_camera, transition.outgoing_view)
    };
    assert_eq!(read(&mut bundle, 0), (Some(kept), view));
    assert_eq!(
        read(&mut bundle, 1),
        (None, gaanim_core::kurbo::Affine::IDENTITY)
    );
}

#[test]
fn drawables_left_out_of_the_post_process_round_trip() {
    let element = |raw: u32, post_exempt: bool| square(raw, 0.0, post_exempt);
    let record = |elements: Vec<CapturedElement>| {
        let mut writer = BundleWriter::new(std::io::Cursor::new(Vec::new()), "test");
        let mut first = frame(0.0);
        first.capture.elements = elements;
        writer.push_frame(&first, [0; 32]).unwrap();
        let scene = SceneData {
            fps: 60,
            duration: 1.0 / 60.0,
            ..Default::default()
        };
        Bundle::from_bytes(writer.finish(&scene).unwrap().into_inner().into()).unwrap()
    };

    let mut bundle = record(vec![element(1, false), element(2, true)]);
    assert!(bundle.manifest.entries.contains_key(UNPROCESSED));
    assert!(!bundle.has_glass());
    let decoded = bundle.frame(0).unwrap();
    let flags: Vec<bool> = decoded
        .capture
        .elements
        .iter()
        .map(|element| element.post_exempt)
        .collect();
    assert_eq!(flags, [false, true]);
    // Without such drawables the bundle has no entry for them.
    let bundle = record(vec![element(1, false)]);
    assert!(!bundle.manifest.entries.contains_key(UNPROCESSED));
}

/// Outlines filled in a 3D drawable's plane come back with their
/// outlines, placements, colors, gradients, clips and images, each
/// outline stored once.
#[test]
fn filled_outlines_of_3d_frames_round_trip() {
    use gaanim_core::kurbo::{Affine, Rect, Shape};
    use gaanim_core::peniko::Color;
    let square = Arc::new(Rect::new(0.0, 0.0, 1.0, 1.0).to_path(0.1));
    let glyph = |x: f64, color: Color| gaanim_scene::PlaneGlyph {
        outline: Arc::clone(&square),
        placement: Affine::translate((x, 0.5)) * Affine::scale(0.25),
        color,
        gradient: None,
        clip: None,
        image: None,
    };
    let (red, blue) = (Color::from_rgb8(255, 0, 0), Color::from_rgb8(0, 0, 255));
    let mut fill = gaanim_scene::PlaneFillData {
        glyphs: vec![glyph(0.0, red), glyph(1.5, blue), glyph(3.0, red)],
    };
    fill.glyphs[2].gradient = Some((
        Arc::new(
            gaanim_core::peniko::Gradient::new_radial((0.5, 0.5), 0.75)
                .with_extend(gaanim_core::peniko::Extend::Reflect)
                .with_stops([red, blue]),
        ),
        Affine::translate((3.0, 0.0)),
    ));
    fill.glyphs[1].clip = Some(gaanim_scene::PlaneClip {
        outline: Arc::clone(&square),
        offset: gaanim_core::kurbo::Vec2::new(-0.25, 0.5),
        outside: true,
    });
    let mut brush = gaanim_core::peniko::ImageBrush::new(gaanim_core::peniko::ImageData {
        data: gaanim_core::peniko::Blob::from(vec![10, 20, 30, 255, 40, 50, 60, 128]),
        format: gaanim_core::peniko::ImageFormat::Rgba8,
        alpha_type: gaanim_core::peniko::ImageAlphaType::Alpha,
        width: 2,
        height: 1,
    });
    brush.sampler.quality = gaanim_core::peniko::ImageQuality::Low;
    fill.glyphs[0].image = Some(gaanim_scene::PlaneImage {
        brush,
        to_pixels: Affine::scale_non_uniform(2.0, -1.0) * Affine::translate((0.5, -1.0)),
    });
    let mut writer = BundleWriter::new(std::io::Cursor::new(Vec::new()), "test");
    let mut first = frame(0.0);
    first.capture.three_d = Some(gaanim_renderer::pipeline::CapturedThreeD {
        sources: vec![gaanim_renderer::pipeline::CapturedThreeDSource {
            entity: bevy::prelude::Entity::from_raw_u32(1).unwrap(),
            transform: gaanim_core::glam::DMat4::IDENTITY,
            opacity: 1.0,
            mesh: None,
            lines: None,
            material: None,
            line_width: None,
            line_range: None,
            fill: Some(Arc::new(fill.clone())),
            joints: None,
            morph: None,
            glow: None,
            shadow: None,
            blur: None,
            instances: None,
            volume: None,
            composition: gaanim_renderer::pipeline::ThreeDComposition {
                in_views: true,
                post_exempt: false,
                layer: None,
                transition_side: Default::default(),
                lineage: Vec::new(),
            },
        }],
        ..Default::default()
    });
    writer.push_frame(&first, [0; 32]).unwrap();
    let scene = SceneData {
        fps: 60,
        duration: 1.0 / 60.0,
        ..Default::default()
    };
    let mut bundle =
        Bundle::from_bytes(writer.finish(&scene).unwrap().into_inner().into()).unwrap();
    let decoded = bundle.frame(0).unwrap();
    let three_d = decoded.capture.three_d.unwrap();
    let mut back = three_d.sources[0].fill.as_deref().unwrap().clone();
    // An image comes back with its texels, sampling and map.
    let (image, expected) = (
        back.glyphs[0].image.take().unwrap(),
        fill.glyphs[0].image.take().unwrap(),
    );
    assert_eq!(
        image.brush.image.data.data(),
        expected.brush.image.data.data()
    );
    assert_eq!(image.brush.sampler, expected.brush.sampler);
    assert_eq!(image.to_pixels, expected.to_pixels);
    assert_eq!(back, fill);
    // The three glyphs share one outline.
    assert!(Arc::ptr_eq(
        &back.glyphs[0].outline,
        &back.glyphs[2].outline
    ));
}

/// The glow, drop shadow and blur of a 3D drawable come back with its
/// frame, also in a frame whose 3D content has no palette before them.
#[test]
fn glows_shadows_and_blurs_of_3d_drawables_round_trip() {
    let mesh = Arc::new(gaanim_scene::TriangleMeshData {
        vertices: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        indices: vec![0, 1, 2],
        normals: None,
        uvs: None,
        color: None,
        colors: None,
        material: None,
        surface: None,
        blend: None,
    });
    let glow = gaanim_renderer::effects::Glow {
        radius: 0.3,
        intensity: 1.5,
        color: gaanim_core::peniko::Color::new([1.0, 0.5, 0.0, 0.8]),
    };
    let shadow = gaanim_renderer::effects::DropShadow {
        offset: gaanim_core::glam::DVec2::new(0.1, -0.2),
        blur_radius: 0.05,
        color: gaanim_core::peniko::Color::new([0.0, 0.0, 0.0, 0.5]),
    };
    let blur = gaanim_renderer::effects::GaussianBlur { sigma: 0.125 };
    type Effects = (
        Option<gaanim_renderer::effects::Glow>,
        Option<gaanim_renderer::effects::DropShadow>,
        Option<gaanim_renderer::effects::GaussianBlur>,
    );
    let source = |(glow, shadow, blur): Effects| gaanim_renderer::pipeline::CapturedThreeDSource {
        entity: bevy::prelude::Entity::from_raw_u32(1).unwrap(),
        transform: gaanim_core::glam::DMat4::IDENTITY,
        opacity: 1.0,
        mesh: Some(Arc::clone(&mesh)),
        lines: None,
        material: None,
        line_width: None,
        line_range: None,
        fill: None,
        joints: None,
        morph: None,
        glow,
        shadow,
        blur,
        instances: None,
        volume: None,
        composition: gaanim_renderer::pipeline::ThreeDComposition {
            in_views: true,
            post_exempt: false,
            layer: None,
            transition_side: Default::default(),
            lineage: Vec::new(),
        },
    };
    // A glow alone, a shadow and a blur alone (written after the
    // sections before them), all three, and none.
    let effects: [Effects; 4] = [
        (Some(glow.clone()), None, None),
        (None, Some(shadow.clone()), Some(blur)),
        (Some(glow), Some(shadow), Some(blur)),
        (None, None, None),
    ];
    let mut writer = BundleWriter::new(std::io::Cursor::new(Vec::new()), "test");
    for (index, each) in effects.iter().enumerate() {
        let mut captured = frame(index as f64 / 60.0);
        captured.capture.three_d = Some(gaanim_renderer::pipeline::CapturedThreeD {
            sources: vec![source(each.clone())],
            ..Default::default()
        });
        writer.push_frame(&captured, [0; 32]).unwrap();
    }
    let scene = SceneData {
        fps: 60,
        duration: 4.0 / 60.0,
        ..Default::default()
    };
    let mut bundle =
        Bundle::from_bytes(writer.finish(&scene).unwrap().into_inner().into()).unwrap();
    for (index, (glow, shadow, blur)) in effects.iter().enumerate() {
        let three_d = bundle.frame(index).unwrap().capture.three_d.unwrap();
        let back = &three_d.sources[0];
        assert_eq!(&back.glow, glow, "frame {index}");
        assert_eq!(&back.shadow, shadow, "frame {index}");
        assert_eq!(&back.blur, blur, "frame {index}");
    }
}

/// Copies of a mesh come back with every list they carry and how far
/// they have grown in, beside a source that draws no copies.
/// A volume's field and the colors of its values go to the images
/// table once, and come back with its grid, box and density.
#[test]
fn volumes_round_trip() {
    let texels: Vec<u8> = (0..60).map(|value| (value * 4) as u8).collect();
    let transfer: Vec<u8> = (0..1024).map(|value| (value % 251) as u8).collect();
    let volume = Arc::new(gaanim_scene::VolumeData {
        values: gaanim_scene::VolumeData::pack(&texels),
        size: [5, 4, 3],
        min: [-1.0, -2.0, -0.5],
        max: [1.0, 2.0, 0.5],
        transfer: gaanim_core::peniko::ImageData {
            data: gaanim_core::peniko::Blob::from(transfer.clone()),
            format: gaanim_core::peniko::ImageFormat::Rgba8,
            alpha_type: gaanim_core::peniko::ImageAlphaType::Alpha,
            width: 256,
            height: 1,
        },
        density: 3.5,
    });
    let source = gaanim_renderer::pipeline::CapturedThreeDSource {
        entity: bevy::prelude::Entity::from_raw_u32(1).unwrap(),
        transform: gaanim_core::glam::DMat4::IDENTITY,
        opacity: 0.75,
        mesh: None,
        lines: None,
        material: None,
        line_width: None,
        line_range: None,
        fill: None,
        joints: None,
        morph: None,
        glow: None,
        shadow: None,
        blur: None,
        instances: None,
        volume: Some(Arc::clone(&volume)),
        composition: gaanim_renderer::pipeline::ThreeDComposition {
            in_views: true,
            post_exempt: false,
            layer: None,
            transition_side: Default::default(),
            lineage: Vec::new(),
        },
    };
    let mut writer = BundleWriter::new(std::io::Cursor::new(Vec::new()), "test");
    for index in 0..2 {
        let mut captured = frame(index as f64 / 60.0);
        captured.capture.three_d = Some(gaanim_renderer::pipeline::CapturedThreeD {
            sources: vec![source.clone()],
            ..Default::default()
        });
        writer.push_frame(&captured, [0; 32]).unwrap();
    }
    let scene = SceneData {
        fps: 60,
        duration: 2.0 / 60.0,
        ..Default::default()
    };
    let mut bundle =
        Bundle::from_bytes(writer.finish(&scene).unwrap().into_inner().into()).unwrap();
    // The field and its colors, stored once for both frames.
    let images: u32 = bundle
        .manifest
        .table_segments
        .iter()
        .map(|segment| segment.counts.images)
        .sum();
    assert_eq!(images, 2);
    for index in 0..2 {
        let three_d = bundle.frame(index).unwrap().capture.three_d.unwrap();
        let back = three_d.sources[0].volume.as_deref().unwrap();
        assert_eq!(back.texels(), Some(&texels[..]));
        assert_eq!(back.transfer.data.data(), &transfer[..]);
        assert_eq!(
            (back.size, back.min, back.max, back.density),
            (volume.size, volume.min, volume.max, volume.density)
        );
        assert_eq!(three_d.sources[0].opacity, 0.75);
    }
}

#[test]
fn copies_of_meshes_round_trip() {
    let mesh = Arc::new(gaanim_scene::TriangleMeshData {
        vertices: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        indices: vec![0, 1, 2],
        normals: None,
        uvs: None,
        color: None,
        colors: None,
        material: Some(gaanim_scene::Material3D::default()),
        surface: None,
        blend: None,
    });
    let source = |raw: u32, instances: Option<gaanim_scene::MeshInstances>| {
        gaanim_renderer::pipeline::CapturedThreeDSource {
            entity: bevy::prelude::Entity::from_raw_u32(raw).unwrap(),
            transform: gaanim_core::glam::DMat4::IDENTITY,
            opacity: 1.0,
            mesh: Some(Arc::clone(&mesh)),
            lines: None,
            material: mesh.material,
            line_width: None,
            line_range: None,
            fill: None,
            joints: None,
            morph: None,
            glow: None,
            shadow: None,
            blur: None,
            instances,
            volume: None,
            composition: gaanim_renderer::pipeline::ThreeDComposition {
                in_views: true,
                post_exempt: false,
                layer: None,
                transition_side: Default::default(),
                lineage: Vec::new(),
            },
        }
    };
    let row = |shift: f32| {
        Arc::new(gaanim_scene::MeshCopies {
            positions: (0..50).map(|i| [i as f32 * 0.1, shift, 0.0]).collect(),
            scales: Some(vec![[0.5, 1.0, 2.0]; 50]),
            rotations: Some(vec![[0.0, 0.0, 0.6, 0.8]; 50]),
            colors: Some((0..50).map(|i| [i as f32 / 50.0, 0.5, 0.2, 1.0]).collect()),
        })
    };
    let (still, moved) = (row(0.0), row(0.25));
    let shown = [
        (Arc::clone(&still), 0.25),
        (Arc::clone(&still), 1.0),
        (Arc::clone(&moved), 1.0),
        (Arc::clone(&moved), 1.0),
    ];
    let mut writer = BundleWriter::new(std::io::Cursor::new(Vec::new()), "test");
    for (index, (copies, reveal)) in shown.iter().enumerate() {
        let mut captured = frame(index as f64 / 60.0);
        let instances = gaanim_scene::MeshInstances {
            copies: Arc::clone(copies),
            reveal: *reveal,
        };
        captured.capture.three_d = Some(gaanim_renderer::pipeline::CapturedThreeD {
            sources: vec![source(1, None), source(2, Some(instances))],
            ..Default::default()
        });
        writer.push_frame(&captured, [0; 32]).unwrap();
    }
    let scene = SceneData {
        fps: 60,
        duration: shown.len() as f64 / 60.0,
        ..Default::default()
    };
    let mut bundle =
        Bundle::from_bytes(writer.finish(&scene).unwrap().into_inner().into()).unwrap();
    for (index, (copies, reveal)) in shown.iter().enumerate() {
        let three_d = bundle.frame(index).unwrap().capture.three_d.unwrap();
        assert!(three_d.sources[0].instances.is_none(), "frame {index}");
        let back = three_d.sources[1].instances.as_ref().unwrap();
        assert_eq!(*back.copies, **copies, "frame {index}");
        assert_eq!(back.reveal, *reveal);
    }
}

/// Copies of many meshes come back however far back the copies each
/// is predicted from lie, and wherever in a frame its source is named:
/// the last of 200 sources is named in the last bytes of its frame,
/// with copies predicted from those 200 items before.
#[test]
fn copies_of_many_meshes_round_trip() {
    const SOURCES: u32 = 200;
    let mesh = Arc::new(gaanim_scene::TriangleMeshData {
        vertices: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        indices: vec![0, 1, 2],
        normals: None,
        uvs: None,
        color: None,
        colors: None,
        material: Some(gaanim_scene::Material3D::default()),
        surface: None,
        blend: None,
    });
    let copies = |frame: usize, source: u32| {
        Arc::new(gaanim_scene::MeshCopies {
            positions: (0..5)
                .map(|i| [i as f32 * 0.1, source as f32 * 0.1, frame as f32 * 0.01])
                .collect(),
            scales: Some(vec![[0.06, 0.12, 0.08]; 5]),
            rotations: None,
            colors: None,
        })
    };
    let sources = |frame: usize| -> Vec<_> {
        (1..=SOURCES)
            .map(|raw| gaanim_renderer::pipeline::CapturedThreeDSource {
                entity: bevy::prelude::Entity::from_raw_u32(raw).unwrap(),
                transform: gaanim_core::glam::DMat4::IDENTITY,
                opacity: 1.0,
                mesh: Some(Arc::clone(&mesh)),
                lines: None,
                material: mesh.material,
                line_width: None,
                line_range: None,
                fill: None,
                joints: None,
                morph: None,
                glow: None,
                shadow: None,
                blur: None,
                instances: Some(gaanim_scene::MeshInstances {
                    copies: copies(frame, raw),
                    reveal: 1.0,
                }),
                volume: None,
                composition: gaanim_renderer::pipeline::ThreeDComposition {
                    in_views: true,
                    post_exempt: false,
                    layer: None,
                    transition_side: Default::default(),
                    lineage: Vec::new(),
                },
            })
            .collect()
    };
    const FRAMES: usize = 4;
    let mut writer = BundleWriter::new(std::io::Cursor::new(Vec::new()), "test");
    for index in 0..FRAMES {
        let mut captured = frame(index as f64 / 60.0);
        captured.capture.three_d = Some(gaanim_renderer::pipeline::CapturedThreeD {
            sources: sources(index),
            ..Default::default()
        });
        writer.push_frame(&captured, [0; 32]).unwrap();
    }
    let scene = SceneData {
        fps: 60,
        duration: FRAMES as f64 / 60.0,
        ..Default::default()
    };
    let mut bundle =
        Bundle::from_bytes(writer.finish(&scene).unwrap().into_inner().into()).unwrap();
    for index in 0..FRAMES {
        let three_d = bundle.frame(index).unwrap().capture.three_d.unwrap();
        assert_eq!(three_d.sources.len(), SOURCES as usize, "frame {index}");
        for (source, raw) in three_d.sources.iter().zip(1..=SOURCES) {
            let back = source.instances.as_ref().unwrap();
            assert_eq!(
                *back.copies,
                *copies(index, raw),
                "frame {index}, source {raw}"
            );
        }
    }
}

/// An imported model's mesh comes back with its texture, normal map,
/// alpha mode, sides, skin weights and morph targets, stored once, and
/// each frame with the palette of its joints and its targets' weights,
/// how its translucent content blends, its lights' cones and its depth
/// of field.
#[test]
fn imported_meshes_and_their_palettes_and_weights_round_trip() {
    use gaanim_core::peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
    let texture = gaanim_scene::MeshTexture {
        image: ImageData {
            data: Blob::from(vec![255, 0, 0, 255, 0, 0, 255, 128]),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: 2,
            height: 1,
        },
        wrap: [
            gaanim_scene::TextureWrap::MirroredRepeat,
            gaanim_scene::TextureWrap::ClampToEdge,
        ],
        nearest: true,
    };
    let mesh = Arc::new(gaanim_scene::TriangleMeshData {
        vertices: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        indices: vec![0, 1, 2],
        normals: None,
        uvs: Some(vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]),
        color: None,
        colors: None,
        material: Some(gaanim_scene::Material3D::default()),
        surface: Some(Arc::new(gaanim_scene::MeshSurface {
            texture: Some(texture),
            normal_map: Some(gaanim_scene::NormalMap {
                texture: gaanim_scene::MeshTexture {
                    image: ImageData {
                        data: Blob::from(vec![128, 128, 255, 255]),
                        format: ImageFormat::Rgba8,
                        alpha_type: ImageAlphaType::Alpha,
                        width: 1,
                        height: 1,
                    },
                    wrap: [gaanim_scene::TextureWrap::Repeat; 2],
                    nearest: false,
                },
                scale: 0.75,
                tangents: vec![
                    [1.0, 0.0, 0.0, 1.0],
                    [0.0, 1.0, 0.0, -1.0],
                    [1.0, 0.0, 0.0, 1.0],
                ],
            }),
            alpha: gaanim_scene::AlphaMode::Mask(0.3),
            double_sided: true,
            skin: Some(gaanim_scene::SkinWeights {
                joints: vec![[0, 1, 0, 0], [1, 0, 0, 0], [300, 2, 1, 0]],
                weights: vec![
                    [0.5, 0.5, 0.0, 0.0],
                    [1.0, 0.0, 0.0, 0.0],
                    [0.2, 0.3, 0.5, 0.0],
                ],
            }),
            morph_targets: vec![
                gaanim_scene::MorphTarget {
                    positions: vec![[0.0; 3], [0.5, 0.0, 0.0], [0.0, 0.0, 0.25]],
                    normals: Some(vec![[0.0, 0.0, 1.0]; 3]),
                },
                gaanim_scene::MorphTarget {
                    positions: vec![[0.0, -1.0, 0.0]; 3],
                    normals: None,
                },
            ],
        })),
        blend: None,
    });
    let weights: [Arc<[f32]>; 2] = [Arc::from([0.0, 1.0]), Arc::from([0.25, 0.5])];
    let source = |palette: [f32; 16], weights: &Arc<[f32]>| {
        gaanim_renderer::pipeline::CapturedThreeDSource {
            entity: bevy::prelude::Entity::from_raw_u32(1).unwrap(),
            transform: gaanim_core::glam::DMat4::IDENTITY,
            opacity: 1.0,
            mesh: Some(Arc::clone(&mesh)),
            lines: None,
            material: mesh.material,
            line_width: None,
            line_range: None,
            fill: None,
            joints: Some(Arc::from(vec![palette, [1.0; 16]])),
            morph: Some(Arc::clone(weights)),
            glow: None,
            shadow: None,
            blur: None,
            instances: None,
            volume: None,
            composition: gaanim_renderer::pipeline::ThreeDComposition {
                in_views: true,
                post_exempt: false,
                layer: None,
                transition_side: Default::default(),
                lineage: Vec::new(),
            },
        }
    };
    let identity = gaanim_core::glam::Mat4::IDENTITY.to_cols_array();
    let moved =
        gaanim_core::glam::Mat4::from_translation(gaanim_core::glam::Vec3::X).to_cols_array();
    let light = |spot: bool| gaanim_renderer::three_d::Light {
        position: gaanim_core::glam::DVec3::new(0.0, 4.0, 2.0),
        directional: false,
        color: [1.0, 0.5, 0.25],
        spot: spot.then_some(gaanim_renderer::three_d::Spot {
            direction: gaanim_core::glam::DVec3::NEG_Y,
            cos_inner: 0.95,
            cos_outer: 0.9,
        }),
    };
    let focused = gaanim_scene::DepthOfField {
        focus: 0.8,
        aperture: 0.2,
        max_blur: 0.3,
    };
    let mut writer = BundleWriter::new(std::io::Cursor::new(Vec::new()), "test");
    for (index, palette) in [identity, moved].into_iter().enumerate() {
        let mut captured = frame(index as f64 / 60.0);
        // The first frame peels its translucent surfaces, under a spot
        // light and out of focus; the second blends them as layers,
        // under a point light and in focus.
        let first = index == 0;
        captured.capture.three_d = Some(gaanim_renderer::pipeline::CapturedThreeD {
            sources: vec![source(palette, &weights[index])],
            transparency: gaanim_scene::Transparency3D {
                layers: if first { 5 } else { 0 },
            },
            lights: vec![light(first)],
            depth_of_field: if first { focused } else { Default::default() },
            ..Default::default()
        });
        writer.push_frame(&captured, [0; 32]).unwrap();
    }
    let scene = SceneData {
        fps: 60,
        duration: 2.0 / 60.0,
        ..Default::default()
    };
    let mut bundle =
        Bundle::from_bytes(writer.finish(&scene).unwrap().into_inner().into()).unwrap();
    assert_eq!(bundle.manifest.version, VERSION);
    for (index, palette) in [identity, moved].into_iter().enumerate() {
        let decoded = bundle.frame(index).unwrap();
        let three_d = decoded.capture.three_d.unwrap();
        let first = index == 0;
        assert_eq!(three_d.transparency.layers, if first { 5 } else { 0 });
        assert_eq!(three_d.lights, [light(first)]);
        let in_focus = gaanim_scene::DepthOfField::default();
        assert_eq!(
            three_d.depth_of_field,
            if first { focused } else { in_focus }
        );
        let back = &three_d.sources[0];
        let back_mesh = back.mesh.as_deref().unwrap();
        assert_eq!(back_mesh.vertices, mesh.vertices, "frame {index}");
        assert_eq!(back_mesh.uvs, mesh.uvs);
        assert_eq!(back_mesh.material, mesh.material);
        // Images compare by their blob; their texels come back.
        let (back_surface, surface) = (
            back_mesh.surface.as_deref().unwrap(),
            mesh.surface.as_deref().unwrap(),
        );
        let (back_texture, texture) = (
            back_surface.texture.as_ref().unwrap(),
            surface.texture.as_ref().unwrap(),
        );
        assert_eq!(back_texture.image.data.data(), texture.image.data.data());
        assert_eq!(
            (back_texture.wrap, back_texture.nearest),
            (texture.wrap, texture.nearest)
        );
        assert_eq!(back_surface.alpha, surface.alpha);
        assert_eq!(back_surface.double_sided, surface.double_sided);
        assert_eq!(back_surface.skin, surface.skin);
        assert_eq!(back.joints.as_deref(), Some(&[palette, [1.0; 16]][..]));
        let (back_map, map) = (
            back_surface.normal_map.as_ref().unwrap(),
            surface.normal_map.as_ref().unwrap(),
        );
        assert_eq!(
            back_map.texture.image.data.data(),
            map.texture.image.data.data()
        );
        assert_eq!(
            (back_map.scale, &back_map.tangents),
            (map.scale, &map.tangents)
        );
        assert_eq!(back_surface.morph_targets, surface.morph_targets);
        assert_eq!(back.morph.as_deref(), Some(&weights[index][..]));
    }
}

/// A bundle says when its frames draw glass, which players without GPU
/// passes hand to the web player.
#[test]
fn bundles_say_when_they_draw_glass() {
    let record = |glass: bool| {
        let mut writer = BundleWriter::new(std::io::Cursor::new(Vec::new()), "test");
        let mut first = frame(0.0);
        first.capture.elements = vec![square(1, 0.0, false)];
        if glass {
            first.capture.glasses = vec![gaanim_renderer::pipeline::CapturedGlass {
                root: bevy::prelude::Entity::from_raw_u32(1).unwrap(),
                glass: Default::default(),
            }];
        }
        writer.push_frame(&first, [0; 32]).unwrap();
        let scene = SceneData {
            fps: 60,
            duration: 1.0 / 60.0,
            ..Default::default()
        };
        Bundle::from_bytes(writer.finish(&scene).unwrap().into_inner().into()).unwrap()
    };
    assert!(record(true).has_glass());
    assert!(!record(false).has_glass());
}

#[test]
fn frames_decode_in_any_order_across_passes() {
    let mut writer = BundleWriter::new(std::io::Cursor::new(Vec::new()), "test");
    let grid: Vec<f64> = (0..130).map(|index| f64::from(index) / 60.0).collect();
    for time in &grid {
        writer.push_frame(&frame(*time), [0; 32]).unwrap();
    }
    writer.start_pass().unwrap();
    let extras = [0.505, 1.255];
    for time in extras {
        writer.push_frame(&frame(time), [0; 32]).unwrap();
    }
    let scene = SceneData {
        fps: 60,
        duration: 130.0 / 60.0,
        ..Default::default()
    };
    let bytes = writer.finish(&scene).unwrap().into_inner();
    let mut bundle = Bundle::from_bytes(bytes.into()).unwrap();
    assert_eq!(bundle.manifest.chunks.len(), 4);

    // Playback through a grid chunk and the pass holding the extras,
    // then back and forth within and across chunks.
    let mut order: Vec<usize> = (0..40).collect();
    order.extend([130, 40, 41, 131, 75, 3, 129, 0, 59, 60, 131, 130]);
    for index in order {
        let decoded = bundle.frame(index).unwrap();
        let time = bundle.times()[index];
        assert_eq!(decoded.time, time, "frame {index}");
        assert_eq!(decoded.camera, frame(time).camera, "frame {index}");
    }
    assert!(bundle.cached.len() <= CACHED_CHUNKS);
}

#[test]
fn a_bundle_opens_from_its_end_and_decodes_chunks_as_they_arrive() {
    let mut writer = BundleWriter::new(std::io::Cursor::new(Vec::new()), "test");
    for index in 0..200 {
        writer
            .push_frame(&frame(f64::from(index) / 60.0), [0; 32])
            .unwrap();
    }
    let scene = SceneData {
        fps: 60,
        duration: 200.0 / 60.0,
        ..Default::default()
    };
    let bytes: Arc<[u8]> = writer.finish(&scene).unwrap().into_inner().into();
    let len = bytes.len() as u64;
    let add = |source: &BundleSource, ranges: &[Range<u64>]| {
        for range in ranges {
            source.insert(
                range.start,
                Arc::from(&bytes[range.start as usize..range.end as usize]),
            );
        }
    };

    // From the last bytes, opening asks for the directory, then for the
    // tables and the first chunk, not the later chunks.
    let source = BundleSource::new(len);
    add(&source, std::slice::from_ref(&(len - 22..len)));
    let mut bundle = None;
    for _ in 0..4 {
        match Bundle::from_source(source.clone()) {
            Ok(opened) => {
                bundle = Some(opened);
                break;
            }
            Err(BundleError::Incomplete { missing }) => add(&source, &missing),
            Err(error) => panic!("{error}"),
        }
    }
    let mut bundle = bundle.expect("opens once the asked bytes arrive");
    assert!(!source.is_complete());
    bundle.frame(0).unwrap();

    // Players never compare digests, so opening leaves them out; they
    // read once their bytes are there.
    let digests = bundle.archive.span(DIGESTS).unwrap();
    assert!(!source.missing(digests.clone()).is_empty());
    assert_eq!(bundle.digest(0), None);
    add(&source, std::slice::from_ref(&digests));
    assert_eq!(bundle.digest(0), Some([0; 32]));

    let last = bundle.frame_count() - 1;
    let ahead = bundle.missing_around(last - 70, 1);
    assert!(!ahead.is_empty());
    let Err(BundleError::Incomplete { missing }) = bundle.frame(last) else {
        panic!("the last chunk has not arrived");
    };
    add(&source, &missing);
    add(&source, &ahead);
    assert_eq!(bundle.frame(last).unwrap().time, bundle.times()[last]);
    assert!(bundle.missing_around(last - 70, 1).is_empty());
}

#[test]
fn a_lobby_and_its_gates_round_trip_without_polls() {
    let mut writer = BundleWriter::new(std::io::Cursor::new(Vec::new()), "test");
    writer.push_frame(&frame(0.0), [0; 32]).unwrap();
    let gate = StopGate {
        time: 0.5,
        until: GateCondition::Any(vec![
            GateCondition::Players { count: 5 },
            GateCondition::All(vec![
                GateCondition::AnswerShare {
                    poll: "q0".into(),
                    share: 0.8,
                    players: true,
                },
                GateCondition::TimeUp { poll: "q0".into() },
            ]),
        ]),
    };
    let scene = SceneData {
        fps: 60,
        duration: 1.0 / 60.0,
        poll_session: Some(PollSessionInfo {
            relay: Some("https://relay.example.dev".into()),
            code: "ABC234".into(),
            lobby: true,
            game_segment: Some(3),
            teams: Some(gaanim_timeline::timeline::TeamsInfo {
                names: vec!["Rojo".into(), "Azul".into()],
                colors: vec!["#ff0000".into(), "#0000ff".into()],
                choose: true,
            }),
            ask: Some(gaanim_timeline::timeline::AskInfo {
                label: "Código".into(),
                required: true,
            }),
        }),
        stop_gates: vec![gate.clone()],
        ..Default::default()
    };
    let bundle = Bundle::from_bytes(writer.finish(&scene).unwrap().into_inner().into()).unwrap();
    // No poll, yet the session and its lobby are kept.
    assert!(bundle.scene.polls.is_empty());
    assert!(bundle.scene.poll_session.as_ref().unwrap().lobby);
    assert_eq!(
        bundle.scene.poll_session.as_ref().unwrap().game_segment,
        Some(3)
    );
    let teams = bundle
        .scene
        .poll_session
        .as_ref()
        .unwrap()
        .teams
        .as_ref()
        .unwrap();
    assert_eq!(teams.names, ["Rojo", "Azul"]);
    assert!(teams.choose);
    let ask = bundle
        .scene
        .poll_session
        .as_ref()
        .unwrap()
        .ask
        .as_ref()
        .unwrap();
    assert_eq!((ask.label.as_str(), ask.required), ("Código", true));
    assert_eq!(bundle.scene.stop_gates, [gate]);
}

#[test]
fn polls_round_trip_in_their_own_entry() {
    let record = |polls: Vec<TimelinePoll>, scene: SceneData| {
        let mut writer = BundleWriter::new(std::io::Cursor::new(Vec::new()), "test");
        writer.push_frame(&frame(0.0), [0; 32]).unwrap();
        let scene = SceneData {
            fps: 60,
            duration: 1.0 / 60.0,
            poll_session: (!polls.is_empty()).then(|| PollSessionInfo {
                relay: Some("https://relay.example.dev".into()),
                code: "ABC234".into(),
                lobby: false,
                game_segment: None,
                teams: None,
                ask: None,
            }),
            polls,
            ..scene
        };
        Bundle::from_bytes(writer.finish(&scene).unwrap().into_inner().into()).unwrap()
    };
    let quiz = TimelinePoll {
        id: "q0-0badf00d".into(),
        question: "¿Cuál?".into(),
        options: vec!["A".into(), "B".into()],
        preview: vec![3, 1],
        segment: 1,
        open: 0.0,
        close: 1.0,
        quiz: Some(TimelineQuiz {
            correct: vec![0, 1],
            time: 20,
            points: 1000,
            reveal: Some(0.9),
        }),
        // Multiple choice, with a picture the bundle carries.
        multiple: true,
        image: Some(gaanim_timeline::timeline::PollImage {
            hash: "0123456789abcdef".into(),
            mime: "image/jpeg".into(),
            bytes: vec![1, 2, 3].into(),
        }),
    };
    let glyph = GlyphRecord {
        ch: 'Ñ',
        advance: 0.6,
        path: "M0 0L1 0L1 1Z".into(),
    };
    let live = SceneData {
        poll_bars: vec![PollBarRecord {
            key: 7,
            source: BarSourceRecord::Leader { rank: 1 },
            length: 4.0,
            thickness: 0.5,
            radius: 0.1,
            direction: "up".into(),
            scale: "leader".into(),
        }],
        poll_texts: vec![LiveTextRecord {
            key: 8,
            list: String::new(),
            rank: 0,
            align: "left".into(),
            glyphs: vec![glyph.clone()],
        }],
        poll_readouts: vec![LiveReadoutRecord {
            key: 9,
            source: LiveSourceRecord::Poll {
                poll: quiz.id.clone(),
                answers: 2,
                measure: "percent".into(),
                answer: 1,
                time: 0.0,
            },
            format: ".0f".into(),
            invalid: "?".into(),
            decimal_separator: ',',
            prefix: RunRecord::default(),
            suffix: RunRecord {
                advance: 0.5,
                path: "M0 0L1 1".into(),
            },
            glyphs: vec![glyph],
        }],
        ..Default::default()
    };
    let bundle = record(vec![quiz.clone()], live.clone());
    assert!(bundle.manifest.entries.contains_key(POLLS));
    assert_eq!(bundle.scene.polls, [quiz]);
    assert_eq!(bundle.scene.poll_bars, live.poll_bars);
    assert_eq!(bundle.scene.poll_texts, live.poll_texts);
    assert_eq!(bundle.scene.poll_readouts, live.poll_readouts);
    assert_eq!(bundle.scene.poll_session.as_ref().unwrap().code, "ABC234");

    // A scene without polls writes no entry, as bundles did before.
    let bundle = record(Vec::new(), SceneData::default());
    assert!(!bundle.manifest.entries.contains_key(POLLS));
    assert!(bundle.scene.polls.is_empty());
    assert!(bundle.scene.poll_session.is_none());
}

/// A bundle where a square stays put and another moves every frame:
/// each frame meets a new outline, as an animated shape does. Its table
/// segments close at `segment_bytes`.
fn moving_square_in_segments(frames: u32, segment_bytes: usize) -> Vec<u8> {
    let mut writer = BundleWriter::new(std::io::Cursor::new(Vec::new()), "test");
    writer.segment_bytes = segment_bytes;
    let still = square(1, -5.0, false);
    for index in 0..frames {
        let mut captured = frame(f64::from(index) / 60.0);
        captured.capture.elements = vec![still.clone(), square(2, f64::from(index) / 10.0, false)];
        writer.push_frame(&captured, [0; 32]).unwrap();
    }
    let scene = SceneData {
        fps: 60,
        duration: f64::from(frames) / 60.0,
        ..Default::default()
    };
    writer.finish(&scene).unwrap().into_inner()
}

fn moving_square(frames: u32) -> Vec<u8> {
    moving_square_in_segments(frames, SEGMENT_BYTES)
}

#[test]
fn a_short_bundle_keeps_its_tables_in_one_segment() {
    let bundle = Bundle::from_bytes(moving_square(200).into()).unwrap();
    assert_eq!(bundle.manifest.table_segments.len(), 1);
    for chunk in &bundle.manifest.chunks {
        assert_eq!(chunk.table_segments, vec![0]);
    }
}

#[test]
fn chunks_store_what_they_meet_first_and_list_the_segments_they_use() {
    // Every chunk fills a segment.
    let bytes = moving_square_in_segments(200, 1);
    let bundle = Bundle::from_bytes(bytes.into()).unwrap();
    let manifest = &bundle.manifest;
    assert_eq!(manifest.version, VERSION);
    // One segment a chunk: the moving outlines it met first.
    assert_eq!(manifest.chunks.len(), 4);
    assert_eq!(manifest.table_segments.len(), 4);
    assert_eq!(manifest.table_segments[1].counts.paths, CHUNK_FRAMES as u32);
    // The still square was met in the first chunk; every later one
    // refers to that segment and its own.
    assert_eq!(manifest.chunks[0].table_segments, vec![0]);
    for (index, chunk) in manifest.chunks.iter().enumerate().skip(1) {
        assert_eq!(chunk.table_segments, vec![0, index as u32], "chunk {index}");
    }
    assert!(!manifest.entries.contains_key("tables/paths.bin"));
}

#[test]
fn opening_downloads_the_first_chunk_with_its_segments_only() {
    let bytes: Arc<[u8]> = moving_square_in_segments(200, 1).into();
    let len = bytes.len() as u64;
    let source = BundleSource::new(len);
    let add = |ranges: &[Range<u64>]| {
        for range in ranges {
            source.insert(
                range.start,
                Arc::from(&bytes[range.start as usize..range.end as usize]),
            );
        }
    };
    add(std::slice::from_ref(&(len - 22..len)));
    let mut bundle = None;
    for _ in 0..4 {
        match Bundle::from_source(source.clone()) {
            Ok(opened) => {
                bundle = Some(opened);
                break;
            }
            Err(BundleError::Incomplete { missing }) => add(&missing),
            Err(error) => panic!("{error}"),
        }
    }
    let mut bundle = bundle.expect("opens once the asked bytes arrive");
    let spans: HashMap<String, Range<u64>> = bundle
        .archive
        .spans()
        .map(|(name, span)| (name.to_owned(), span))
        .collect();
    let arrived = |name: &str| source.missing(spans[name].clone()).is_empty();
    assert!(arrived("segments/000000.bin"));
    for later in [
        "segments/000001.bin",
        "segments/000002.bin",
        "segments/000003.bin",
    ] {
        assert!(!arrived(later), "{later} is not needed to open");
    }
    assert_eq!(lefts(&bundle.frame(0).unwrap()), vec![-5.0, 0.0]);

    // The last chunk asks for itself and its own segment, not the ones
    // between.
    let last = bundle.frame_count() - 1;
    let Err(BundleError::Incomplete { missing }) = bundle.frame(last) else {
        panic!("the last chunk has not arrived");
    };
    add(&missing);
    assert!(arrived("segments/000003.bin"));
    assert!(!arrived("segments/000001.bin"));
    assert_eq!(
        lefts(&bundle.frame(last).unwrap()),
        vec![-5.0, last as f64 / 10.0]
    );
}

/// A segment holds the recipes of every chunk that met them first, so it
/// can hold another chunk's recipe that refers to an earlier segment this
/// chunk does not list. Seeking straight to the chunk still decodes it.
#[test]
fn a_chunk_decodes_without_the_segments_of_recipes_it_does_not_draw() {
    let mut writer = BundleWriter::new(std::io::Cursor::new(Vec::new()), "test");
    // Every chunk closes a segment of its own.
    writer.segment_bytes = 1;
    let earlier = square(2, 3.0, false);
    // The earlier outline in a recipe of its own.
    let mut again = square(3, 3.0, false);
    again.recipe = Arc::new(gaanim_renderer::fragment::FragmentRecipe {
        path: earlier.recipe.path.clone(),
        fill_progress: Some(0.5),
        ..Default::default()
    });
    let later = square(4, 7.0, false);
    let chunks = [
        vec![square(1, -5.0, false)],
        vec![earlier],
        vec![again, later.clone()],
        vec![later],
    ];
    let mut index = 0;
    for elements in chunks {
        for _ in 0..CHUNK_FRAMES {
            let mut captured = frame(f64::from(index) / 60.0);
            captured.capture.elements = elements.clone();
            writer.push_frame(&captured, [0; 32]).unwrap();
            index += 1;
        }
    }
    let scene = SceneData {
        fps: 60,
        duration: f64::from(index) / 60.0,
        ..Default::default()
    };
    let bytes = writer.finish(&scene).unwrap().into_inner();
    let mut bundle = Bundle::from_bytes(bytes.into()).unwrap();
    // The last chunk draws only the later square, from segment 2, where
    // the other recipe refers to segment 1's outline.
    assert_eq!(bundle.manifest.chunks[3].table_segments, vec![2]);
    let last = bundle.frame_count() - 1;
    assert_eq!(lefts(&bundle.frame(last).unwrap()), vec![7.0]);
    // Segment 1 loads with chunk 1, and the recipe that needs it decodes.
    let third = 2 * CHUNK_FRAMES;
    assert_eq!(lefts(&bundle.frame(third).unwrap()), vec![3.0, 7.0]);
}

#[test]
fn format_5_bundles_still_open_and_verify() {
    let bytes = include_bytes!("../tests/fixtures/format5.gaanim");
    let mut bundle = Bundle::from_bytes(Arc::from(&bytes[..])).unwrap();
    assert_eq!(bundle.manifest.version, 5);
    assert_eq!(bundle.frame_count(), 61);
    assert_eq!(bundle.scene.output_size, (1920, 1920));
    assert!(bundle.verify().unwrap().is_empty());
}

#[test]
fn older_formats_are_refused_by_version() {
    let mut manifest: serde_json::Value = serde_json::from_slice(
        &Archive::open(BundleSource::whole(Arc::from(
            &include_bytes!("../tests/fixtures/format5.gaanim")[..],
        )))
        .unwrap()
        .read("manifest.json")
        .unwrap(),
    )
    .unwrap();
    manifest["version"] = 4.into();
    let manifest: Manifest = serde_json::from_value(manifest).unwrap();
    assert!(!(OLDEST_READABLE..=VERSION).contains(&manifest.version));
    assert!(version_message(4, "gaanim 0.8.0").contains("no longer reads"));
}

#[test]
fn frame_times_read_back_bit_for_bit() {
    // The export's grid at 30 fps, then instants between its frames, as
    // a recording in two passes stores them.
    let step = 1.0 / 30.0;
    let mut times = Vec::new();
    let mut time = 0.0;
    for _ in 0..100 {
        times.push(time);
        time += step;
    }
    times.extend([0.51, 1.234_567, 3.3]);
    let bytes = write_index(&times, 30);
    // A byte a grid frame, and the bits of each other instant.
    assert!(bytes.len() < 100 + 3 * 9 + 8);
    let read = read_index(&bytes, VERSION).unwrap();
    assert_eq!(
        read.iter().map(|t| t.to_bits()).collect::<Vec<_>>(),
        times.iter().map(|t| t.to_bits()).collect::<Vec<_>>()
    );
}

/// Bytes of `bundle`'s file that opening it needs: what `from_source`
/// waits for.
fn opening_bytes(bundle: &Bundle) -> u64 {
    let mut names: Vec<String> = bundle
        .archive
        .spans()
        .filter(|(name, _)| read_on_open(name))
        .map(|(name, _)| name.to_owned())
        .collect();
    if bundle.manifest.version >= 6 {
        let mut segments = bundle.manifest.scene_table_segments.clone();
        segments.extend(&bundle.manifest.chunks[0].table_segments);
        segments.sort_unstable();
        segments.dedup();
        names.extend(segments.iter().map(|segment| {
            bundle.manifest.table_segments[*segment as usize]
                .entry
                .clone()
        }));
    }
    names
        .iter()
        .filter_map(|name| bundle.archive.span(name))
        .map(|span| span.end - span.start)
        .sum()
}

#[test]
fn a_format_5_bundle_repacks_into_the_current_format_frame_for_frame() {
    let bytes = include_bytes!("../tests/fixtures/format5.gaanim");
    let mut old = Bundle::from_bytes(Arc::from(&bytes[..])).unwrap();
    let repacked = repack(&mut old, std::io::Cursor::new(Vec::new()), "test")
        .unwrap()
        .into_inner();
    let mut new = Bundle::from_bytes(repacked.into()).unwrap();
    assert_eq!(new.manifest.version, VERSION);
    assert_eq!(new.times(), old.times());
    assert_eq!(new.scene.segments, old.scene.segments);
    assert!(new.verify().unwrap().is_empty());
}

/// Write every bundle of the folder `GAANIM_BUNDLES` again in the current
/// format, check that each frame composes as it was recorded, and print
/// how many bytes opening each one needs before and after.
#[test]
#[ignore = "reads the bundles of the folder GAANIM_BUNDLES"]
fn bundles_of_a_folder_repack_frame_for_frame() {
    let Ok(folder) = std::env::var("GAANIM_BUNDLES") else {
        return;
    };
    let mut paths: Vec<PathBuf> = std::fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == EXTENSION)
        })
        .collect();
    paths.sort();
    // `GAANIM_BUNDLES_PART=i/n` takes every n-th bundle from the i-th, so
    // several processes share a folder.
    if let Some((part, parts)) = std::env::var("GAANIM_BUNDLES_PART").ok().and_then(|part| {
        let (part, parts) = part.split_once('/')?;
        Some((part.parse::<usize>().ok()?, parts.parse::<usize>().ok()?))
    }) {
        paths = paths.into_iter().skip(part).step_by(parts.max(1)).collect();
    }
    let (mut total_before, mut total_after, mut open_before, mut open_after) = (0, 0, 0, 0);
    let (mut total_web, mut open_web) = (0, 0);
    for path in &paths {
        let bytes = std::fs::read(path).unwrap();
        let mut old = Bundle::from_bytes(bytes.clone().into()).unwrap();
        let repacked = repack(&mut old, std::io::Cursor::new(Vec::new()), "test")
            .unwrap()
            .into_inner();
        let size = repacked.len();
        let mut new = Bundle::from_bytes(repacked.into()).unwrap();
        // `GAANIM_BUNDLES_QUICK` only measures the repacked sizes.
        if std::env::var_os("GAANIM_BUNDLES_QUICK").is_some() {
            println!(
                "{:48} {:>10} -> {:>10} bytes, opening {:>10} -> {:>10}",
                path.file_name().unwrap().to_string_lossy(),
                bytes.len(),
                size,
                opening_bytes(&old),
                opening_bytes(&new)
            );
            continue;
        }
        // Frame for frame: the repacked bundle composes what the original
        // does (some recordings differ from their own digests already).
        let background = old.scene.background.clone();
        let mut store = FragmentStore::default();
        for index in 0..old.frame_count() {
            let before = frame_digest(&old.frame(index).unwrap(), background.as_ref(), &mut store);
            store.end_frame();
            let after = frame_digest(&new.frame(index).unwrap(), background.as_ref(), &mut store);
            store.end_frame();
            assert_eq!(before, after, "{}: frame {index}", path.display());
        }
        let (before, after) = (opening_bytes(&old), opening_bytes(&new));
        let options = RepackOptions { web: true };
        let packed = repack_with(&mut old, std::io::Cursor::new(Vec::new()), "test", &options)
            .unwrap()
            .into_inner();
        let web_size = packed.len();
        let mut web = Bundle::from_bytes(packed.into()).unwrap();
        let web_open = opening_bytes(&web);
        for index in 0..web.frame_count() {
            web.frame(index).unwrap();
        }
        println!(
            "{:48} {:>10} -> {:>10} (web {:>10}) bytes, opening {:>10} -> {:>10} (web {:>10})",
            path.file_name().unwrap().to_string_lossy(),
            bytes.len(),
            size,
            web_size,
            before,
            after,
            web_open
        );
        total_before += bytes.len() as u64;
        total_after += size as u64;
        total_web += web_size as u64;
        open_before += before;
        open_after += after;
        open_web += web_open;
    }
    println!(
        "{} bundles: {total_before} -> {total_after} (web {total_web}) bytes; opening {open_before} -> {open_after} (web {open_web})",
        paths.len()
    );
}

#[test]
fn a_bundle_packed_for_the_web_is_smaller_and_within_its_grid() {
    let exact = moving_square(200);
    let mut original = Bundle::from_bytes(exact.clone().into()).unwrap();
    let options = RepackOptions { web: true };
    let packed = repack_with(
        &mut original,
        std::io::Cursor::new(Vec::new()),
        "test",
        &options,
    )
    .unwrap()
    .into_inner();
    assert!(
        packed.len() < exact.len(),
        "{} >= {}",
        packed.len(),
        exact.len()
    );
    let mut web = Bundle::from_bytes(packed.into()).unwrap();
    assert!(web.manifest.approximate);
    assert!(!web.manifest.entries.contains_key(DIGESTS));
    assert!(matches!(web.verify(), Err(BundleError::Unsupported(_))));
    // A 1280-unit-wide camera is 3 pixels a unit in a 4K export: points
    // stay within 1/64 of a pixel, a 192nd of a unit.
    for index in 0..original.frame_count() {
        let before = lefts(&original.frame(index).unwrap());
        let after = lefts(&web.frame(index).unwrap());
        for (b, a) in before.iter().zip(&after) {
            assert!((b - a).abs() <= 1.0 / 192.0, "frame {index}: {b} -> {a}");
        }
    }
}

#[test]
fn grid_paths_read_back_on_their_grid() {
    let mut path = gaanim_core::kurbo::BezPath::new();
    path.move_to((0.123_456_789, -7.5));
    path.curve_to((1.0, 2.0), (3.333_333, 4.0), (5.0, 6.000_001));
    path.close_path();
    for (exponent, relative) in [
        (-20, true),
        (-10, true),
        (0, true),
        (-20, false),
        (-4, false),
    ] {
        let mut w = codec::Writer::new();
        codec::write_grid_path(&mut w, &path, exponent, relative);
        let bytes = w.into_bytes();
        let mut r = codec::Reader::new(&bytes);
        let read = if relative {
            codec::read_grid_path(&mut r)
        } else {
            codec::read_absolute_grid_path(&mut r)
        }
        .unwrap();
        assert!(r.is_empty());
        let grid = 2f64.powi(exponent);
        assert_eq!(read.elements().len(), path.elements().len());
        for (a, b) in path.elements().iter().zip(read.elements()) {
            let points = |element: &gaanim_core::kurbo::PathEl| match *element {
                gaanim_core::kurbo::PathEl::MoveTo(p) | gaanim_core::kurbo::PathEl::LineTo(p) => {
                    vec![p]
                }
                gaanim_core::kurbo::PathEl::QuadTo(p, q) => vec![p, q],
                gaanim_core::kurbo::PathEl::CurveTo(p, q, r) => vec![p, q, r],
                gaanim_core::kurbo::PathEl::ClosePath => vec![],
            };
            for (p, q) in points(a).into_iter().zip(points(b)) {
                assert!((p.x - q.x).abs() <= grid / 2.0 && (p.y - q.y).abs() <= grid / 2.0);
                assert_eq!((q.x / grid).fract(), 0.0);
            }
        }
    }
}
