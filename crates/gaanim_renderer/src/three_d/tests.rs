use super::*;

fn camera(projection: Projection) -> Camera {
    let mut camera = Camera::ortho_2d_frame(16.0, 9.0, 1920, 1080);
    camera.projection = projection;
    camera.position = DVec3::new(0.0, 0.0, 10.0);
    camera
}

fn quad(color: peniko::Color) -> TriangleMeshData {
    TriangleMeshData {
        vertices: vec![
            [-1.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [1.0, 1.0, 0.0],
            [-1.0, 1.0, 0.0],
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
        normals: None,
        uvs: None,
        color: Some(color),
        colors: None,
        material: None,
        surface: None,
        blend: None,
    }
}

#[test]
fn orthographic_projection_keeps_the_canvas_in_scene_units() {
    let (ndc_to_canvas, line_width) =
        canvas_projection(&camera(Projection::Orthographic { zoom: 1.0 })).unwrap();
    // The NDC square spans the 16 x 9 frame around the camera.
    let corner = ndc_to_canvas * kurbo::Point::new(1.0, 1.0);
    assert!((corner.x - 8.0).abs() < 1e-9 && (corner.y - 4.5).abs() < 1e-9);
    // 2 pixels of a 1080-pixel-tall output, in the 9-unit-tall frame.
    assert!((line_width - 9.0 * 2.0 / 1080.0).abs() < 1e-12);
}

#[test]
fn meshes_keep_their_valid_triangles_with_unit_normals() {
    let mut mesh = quad(peniko::Color::WHITE);
    mesh.indices.extend([0, 1, 9]);
    let source = MeshSource::mesh(&mesh, true, false).unwrap();
    assert_eq!(source.count, 6);
    assert_eq!(source.vertices.len(), 4 * MESH_VERTEX_BYTES);
    // The first vertex's normal, generated from the faces, faces +z.
    let normal: Vec<f32> = source.vertices[12..24]
        .chunks(4)
        .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
        .collect();
    assert_eq!(normal, [0.0, 0.0, 1.0]);
    mesh.indices.clear();
    assert!(MeshSource::mesh(&mesh, true, false).is_none());
}

#[test]
fn closed_meshes_are_told_apart_from_open_ones() {
    assert!(
        !MeshSource::mesh(&quad(peniko::Color::WHITE), true, false)
            .unwrap()
            .closed
    );
    // A tetrahedron whose faces have vertices of their own, as flat
    // shading and seams make them, encloses its volume.
    let corners = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
    ];
    let faces = [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
    let tetrahedron = TriangleMeshData {
        vertices: faces
            .iter()
            .flatten()
            .map(|&corner| corners[corner])
            .collect(),
        indices: (0..12).collect(),
        ..quad(peniko::Color::WHITE)
    };
    let solid = MeshSource::mesh(&tetrahedron, true, false).unwrap();
    assert!(solid.closed && !solid.sheet);
    // Without a face it is open, and holds nothing.
    let mut open = tetrahedron.clone();
    open.indices.truncate(9);
    let open = MeshSource::mesh(&open, true, false).unwrap();
    assert!(!open.closed && open.sheet);
    // Both sides of a quad, far from the origin, are closed but hold
    // nothing; unlit, it is neither.
    let mut sides = quad(peniko::Color::WHITE);
    for vertex in &mut sides.vertices {
        vertex[0] += 1000.0;
    }
    let back: Vec<u32> = sides
        .indices
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|[a, b, c]| [a + 4, c + 4, b + 4])
        .collect();
    sides.vertices.extend_from_within(..);
    sides.indices.extend(back);
    let two_sided = MeshSource::mesh(&sides, true, false).unwrap();
    assert!(two_sided.closed && two_sided.sheet);
    assert!(!MeshSource::mesh(&sides, false, false).unwrap().sheet);
    // Copies of a corner computed with different rounding still meet,
    // even across a cell of the grid that welds them (1e-5 here).
    let mut positions: Vec<DVec3> = tetrahedron
        .vertices
        .iter()
        .map(|v| DVec3::new(v[0].into(), v[1].into(), v[2].into()))
        .collect();
    for (index, corner) in faces.iter().flatten().enumerate() {
        if *corner == 1 && index % 2 == 0 {
            positions[index].x -= 1e-8;
        }
    }
    assert!(encloses(&positions, &tetrahedron.indices, 1.0));
}

#[test]
fn line_lists_pair_their_points_as_strips_indices_or_pairs() {
    let lines = |strip, indices| LineListData {
        points: vec![[0.0; 3], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
        indices,
        strip,
        color: peniko::Color::WHITE,
        colors: None,
        revealed: None,
    };
    assert_eq!(MeshSource::lines(&lines(true, None)).unwrap().count, 3);
    assert_eq!(MeshSource::lines(&lines(false, None)).unwrap().count, 2);
    let indexed = lines(false, Some(vec![0, 2, 1, 3, 2, 7]));
    assert_eq!(MeshSource::lines(&indexed).unwrap().count, 2);
    let mut single = lines(true, None);
    single.points.truncate(1);
    assert!(MeshSource::lines(&single).is_none());
}

#[test]
fn uploads_are_kept_until_their_revision_changes() {
    let mesh = quad(peniko::Color::WHITE);
    let mut moved = quad(peniko::Color::WHITE);
    moved.vertices[0] = [-2.0, -1.0, 0.0];
    let mut sources = MeshSources::default();
    let mut built = |revision, mesh: &TriangleMeshData| {
        sources
            .get(7, Kind::Mesh, revision, None, || {
                MeshSource::mesh(mesh, true, false)
            })
            .unwrap()
    };
    let first = built(1, &mesh);
    // The same revision keeps the source without building it again.
    assert!(Arc::ptr_eq(&first, &built(1, &moved)));
    // A new revision builds it again: the same mesh keeps its source,
    // which drawables alike share; another mesh gets a new one.
    assert!(Arc::ptr_eq(&first, &built(2, &mesh)));
    assert!(!Arc::ptr_eq(&first, &built(3, &moved)));
    sources.end_frame();
    sources.end_frame();
    assert!(sources.sources.is_empty());
    drop(first);
    sources.end_frame();
    assert!(sources.shared.is_empty());
}

#[test]
fn classes_draw_into_images_of_their_own_with_translucent_draws_last() {
    let camera = camera(Projection::Orthographic { zoom: 1.0 });
    let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
    let transform = GlobalSpatialTransform::default();
    let (opaque, faded) = (GlobalOpacity(1.0), GlobalOpacity(0.5));
    let red = quad(peniko::Color::from_rgb8(255, 0, 0));
    let content = |opacity| Content {
        line_width: None,
        line_range: None,
        fill: None,
        joints: None,
        transform: &transform,
        opacity,
        mesh: Some(&red),
        lines: None,
        material: None,
        glow: None,
        shadow: None,
        blur: None,
        instances: None,
        morph: None,
        volume: None,
    };
    let mut sources = MeshSources::default();
    let mut meshes =
        GpuMeshes::new(&mut sources, (1920, 1080), camera.to_vello_transform(), 0).unwrap();
    assert_eq!(
        meshes.push(0, [(1, 1), (1, 1)], [None, None], &content(&faded)),
        (true, false)
    );
    meshes.push(0, [(2, 1), (2, 1)], [None, None], &content(&opaque));
    meshes.push(1, [(3, 1), (3, 1)], [None, None], &content(&opaque));
    let frames = meshes.finish(
        &camera,
        |_| camera,
        ndc_to_canvas,
        line_width,
        &Illumination::default(),
    );
    assert_eq!(frames.len(), 2);
    assert_ne!(frames[0].1.image.data.id(), frames[1].1.image.data.id());
    let translucent: Vec<bool> = frames[0]
        .1
        .draws
        .iter()
        .map(|draw| draw.translucent)
        .collect();
    assert_eq!(translucent, [false, true]);
}

fn lit_quad() -> TriangleMeshData {
    TriangleMeshData {
        material: Some(Material3D::default()),
        ..quad(peniko::Color::WHITE)
    }
}

fn placed(scale: f64, translation: DVec3) -> GlobalSpatialTransform {
    GlobalSpatialTransform {
        mat4: DMat4::from_scale_rotation_translation(
            DVec3::splat(scale),
            Default::default(),
            translation,
        ),
        ..Default::default()
    }
}

#[test]
fn shadows_cover_the_meshes_from_the_first_lights() {
    let camera = camera(Projection::Orthographic { zoom: 1.0 });
    let floor = lit_quad();
    let (transform, opacity) = (placed(2.0, DVec3::ZERO), GlobalOpacity(1.0));
    let mut sources = MeshSources::default();
    let mut meshes =
        GpuMeshes::new(&mut sources, (1920, 1080), camera.to_vello_transform(), 0).unwrap();
    meshes.push(
        0,
        [(1, 1), (1, 1)],
        [None, None],
        &Content {
            transform: &transform,
            opacity: &opacity,
            mesh: Some(&floor),
            lines: None,
            material: floor.material.as_ref(),
            line_width: None,
            line_range: None,
            fill: None,
            joints: None,
            glow: None,
            shadow: None,
            blur: None,
            instances: None,
            morph: None,
            volume: None,
        },
    );
    // The fixed key light casts one map, at full size.
    let mut illumination = Illumination::default();
    let key = shadows(&illumination, &meshes.pending);
    assert_eq!(
        (key.key, key.layers.len(), key.size),
        (Some(0), 1, SHADOW_SIZE)
    );
    // The floor's middle lies in the map, its corner too.
    for point in [DVec3::ZERO, DVec3::new(2.0, 2.0, 0.0)] {
        let ndc = key.layers[0].view_proj.project_point3(point);
        assert!(ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0, "{ndc:?}");
        assert!((0.0..=1.0).contains(&ndc.z), "{ndc:?}");
    }
    // A point light casts a cube of six, a directional one a map: three
    // lights cast 13 smaller maps.
    let light = |position, directional| Light {
        position,
        directional,
        color: [1.0; 3],
        spot: None,
    };
    illumination.lights = vec![
        light(DVec3::new(0.0, 0.0, 3.0), false),
        light(DVec3::new(1.0, 1.0, 1.0), true),
        light(DVec3::new(0.0, 3.0, 0.0), false),
    ];
    let lit = shadows(&illumination, &meshes.pending);
    assert_eq!(lit.key, None);
    assert_eq!(&lit.lights[..3], [Some(0), Some(6), Some(7)]);
    assert_eq!((lit.layers.len(), lit.size), (13, SHADOW_SIZE_MANY));
    // The floor beneath the point light lies in the face looking down.
    let below = lit.layers[5]
        .view_proj
        .project_point3(DVec3::new(0.5, 0.5, 0.0));
    assert!(below.x.abs() <= 1.0 && below.y.abs() <= 1.0, "{below:?}");
    illumination.lighting.shadows = false;
    assert!(shadows(&illumination, &meshes.pending).layers.is_empty());
}

#[test]
fn lines_are_as_wide_as_their_stroke() {
    let camera = camera(Projection::Orthographic { zoom: 1.0 });
    let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
    let lines = LineListData {
        points: vec![[0.0; 3], [1.0, 0.0, 0.0]],
        indices: None,
        strip: true,
        color: peniko::Color::WHITE,
        colors: None,
        revealed: None,
    };
    let (transform, opacity) = (GlobalSpatialTransform::default(), GlobalOpacity(1.0));
    let mut sources = MeshSources::default();
    let mut meshes =
        GpuMeshes::new(&mut sources, (1920, 1080), camera.to_vello_transform(), 0).unwrap();
    for (key, width) in [(1, Some(0.05)), (2, None)] {
        meshes.push(
            0,
            [(key, 1), (key, 1)],
            [None, None],
            &Content {
                transform: &transform,
                opacity: &opacity,
                mesh: None,
                lines: Some(&lines),
                material: None,
                line_width: width,
                line_range: None,
                fill: None,
                joints: None,
                glow: None,
                shadow: None,
                blur: None,
                instances: None,
                morph: None,
                volume: None,
            },
        );
    }
    let frames = meshes.finish(
        &camera,
        |_| camera,
        ndc_to_canvas,
        line_width,
        &Illumination::default(),
    );
    let half_width = |draw: usize| {
        let start = draw * DRAW_STRIDE + LINE_ROW;
        f32::from_le_bytes(
            frames[0].1.draw_uniforms[start..start + 4]
                .try_into()
                .unwrap(),
        )
    };
    // 0.05 scene units at 120 pixels per unit, and the default 2 pixels
    // of a 1080-pixel-tall output.
    assert!((half_width(0) - 3.0).abs() < 1e-4, "{}", half_width(0));
    assert!((half_width(1) - 1.0).abs() < 1e-4, "{}", half_width(1));
}

#[test]
fn a_revealed_line_draws_only_the_segments_it_shows() {
    let camera = camera(Projection::Orthographic { zoom: 1.0 });
    let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
    let lines = LineListData {
        points: vec![
            [0.0; 3],
            [1.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [3.0, 0.0, 0.0],
            [4.0; 3],
        ],
        indices: None,
        strip: true,
        color: peniko::Color::WHITE,
        colors: None,
        revealed: None,
    };
    let (transform, opacity) = (GlobalSpatialTransform::default(), GlobalOpacity(1.0));
    let mut sources = MeshSources::default();
    let mut meshes =
        GpuMeshes::new(&mut sources, (1920, 1080), camera.to_vello_transform(), 0).unwrap();
    for (key, range) in [(1, Some([0.25, 0.6])), (2, Some([0.5, 0.5])), (3, None)] {
        meshes.push(
            0,
            [(key, 1), (key, 1)],
            [None, None],
            &Content {
                transform: &transform,
                opacity: &opacity,
                mesh: None,
                lines: Some(&lines),
                material: None,
                line_width: None,
                line_range: range,
                fill: None,
                joints: None,
                glow: None,
                shadow: None,
                blur: None,
                instances: None,
                morph: None,
                volume: None,
            },
        );
    }
    let frames = meshes.finish(
        &camera,
        |_| camera,
        ndc_to_canvas,
        line_width,
        &Illumination::default(),
    );
    let frame = &frames[0].1;
    let shown = |draw: usize| {
        let start = draw * DRAW_STRIDE + LINE_ROW + 4;
        let value = |offset: usize| {
            f32::from_le_bytes(
                frame.draw_uniforms[start + offset..start + offset + 4]
                    .try_into()
                    .unwrap(),
            )
        };
        [value(0), value(4)]
    };
    // A quarter to 60% of 4 segments: from 1 to 2.4, which draws the
    // second and third segments, the third in part.
    assert_eq!(shown(0), [1.0, 2.4]);
    assert_eq!(frame.draws[0].instances, 1..3);
    // Nothing shown draws nothing; without a reveal, every segment.
    assert!(frame.draws[1].instances.is_empty());
    assert_eq!(
        (shown(2), frame.draws[2].instances.clone()),
        ([0.0, 4.0], 0..4)
    );
}

#[test]
fn a_line_revealed_halfway_draws_its_first_half() {
    let Some(mut gpu) = crate::background::test_gpu::test_gpu() else {
        return;
    };
    let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
    camera.position = DVec3::new(0.0, 0.0, 10.0);
    let (ndc_to_canvas, _) = canvas_projection(&camera).unwrap();
    let placement = camera.to_vello_transform();
    let (transform, opacity) = (GlobalSpatialTransform::default(), GlobalOpacity(1.0));
    let lines = LineListData {
        points: vec![[-1.5, 0.0, 0.0], [0.0, 0.0, 0.0], [1.5, 0.0, 0.0]],
        indices: None,
        strip: true,
        color: peniko::Color::from_rgb8(255, 0, 0),
        colors: None,
        revealed: None,
    };
    let mut sources = MeshSources::default();
    let mut meshes = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
    meshes.push(
        0,
        [(1, 1), (1, 1)],
        [None, None],
        &Content {
            transform: &transform,
            opacity: &opacity,
            mesh: None,
            lines: Some(&lines),
            material: None,
            line_width: Some(0.25),
            line_range: Some([0.0, 0.5]),
            fill: None,
            joints: None,
            glow: None,
            shadow: None,
            blur: None,
            instances: None,
            morph: None,
            volume: None,
        },
    );
    let frames: Vec<GpuMeshFrame> = meshes
        .finish(
            &camera,
            |_| camera,
            ndc_to_canvas,
            0.1,
            &Illumination::default(),
        )
        .into_iter()
        .map(|(_, frame)| frame)
        .collect();
    let mut scene = vello::Scene::new();
    scene.append(&frames[0].scene(), Some(placement));
    let mut three_d = GpuThreeD::default();
    three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames);
    let target = gpu.target(64, 64);
    gpu.render(&scene, &target);
    let pixels = gpu.read(&target);
    let red = |x: usize| pixels[(32 * 64 + x) * 4];
    // At 16 pixels per unit, x = -1 is shown and x = 1 is not.
    assert!(red(16) > 200, "{}", red(16));
    assert_eq!(red(48), 0);
}

#[test]
fn an_opaque_mesh_shadows_the_lit_floor_beneath_it() {
    let Some(mut gpu) = crate::background::test_gpu::test_gpu() else {
        return;
    };
    let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
    camera.position = DVec3::new(0.0, 0.0, 10.0);
    let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
    let placement = camera.to_vello_transform();
    let opacity = GlobalOpacity(1.0);
    let lit = lit_quad();
    // A floor 2 units on a side, and a card a unit above it, right of the
    // middle: light from the right throws the card's shadow left of it.
    let floor = placed(1.0, DVec3::ZERO);
    let card = placed(0.25, DVec3::new(0.5, 0.0, 1.0));
    let mut sources = MeshSources::default();
    let mut meshes = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
    for (key, transform) in [(1, &floor), (2, &card)] {
        meshes.push(
            0,
            [(key, 1), (key, 1)],
            [None, None],
            &Content {
                transform,
                opacity: &opacity,
                mesh: Some(&lit),
                lines: None,
                material: lit.material.as_ref(),
                line_width: None,
                line_range: None,
                fill: None,
                joints: None,
                glow: None,
                shadow: None,
                blur: None,
                instances: None,
                morph: None,
                volume: None,
            },
        );
    }
    let illumination = Illumination {
        lighting: Lighting3D::default(),
        lights: vec![Light {
            position: DVec3::new(1.0, 0.0, 1.0),
            directional: true,
            color: [1.0; 3],
            spot: None,
        }],
        ..Default::default()
    };
    let frames: Vec<GpuMeshFrame> = meshes
        .finish(
            &camera,
            |_| camera,
            ndc_to_canvas,
            line_width,
            &illumination,
        )
        .into_iter()
        .map(|(_, frame)| frame)
        .collect();
    let mut scene = vello::Scene::new();
    scene.append(&frames[0].scene(), Some(placement));
    let mut three_d = GpuThreeD::default();
    assert!(three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames));
    let target = gpu.target(64, 64);
    gpu.render(&scene, &target);
    let pixels = gpu.read(&target);
    let pixel = |x: usize, y: usize| pixels[(y * 64 + x) * 4];
    // At 16 pixels per unit: the shadow at (-0.5, 0), the floor in the
    // light at (-0.5, 0.75).
    let (shadow, light) = (pixel(24, 32), pixel(24, 20));
    assert!(
        u16::from(shadow) + 40 < u16::from(light),
        "shadow {shadow}, light {light}"
    );
}

#[test]
fn filled_outlines_shadow_the_lit_floor_through_their_holes() {
    let Some(mut gpu) = crate::background::test_gpu::test_gpu() else {
        return;
    };
    let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
    camera.position = DVec3::new(0.0, 0.0, 10.0);
    let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
    let placement = camera.to_vello_transform();
    let opacity = GlobalOpacity(1.0);
    let lit = lit_quad();
    let floor = placed(1.0, DVec3::ZERO);
    // A square frame, half a unit on a side with a hole a fifth of a unit
    // across, a unit above the floor and right of the middle: light from
    // the right throws its shadow a unit left of it.
    let mut frame = kurbo::BezPath::new();
    frame.move_to((0.25, -0.25));
    frame.line_to((0.75, -0.25));
    frame.line_to((0.75, 0.25));
    frame.line_to((0.25, 0.25));
    frame.close_path();
    frame.move_to((0.4, -0.1));
    frame.line_to((0.4, 0.1));
    frame.line_to((0.6, 0.1));
    frame.line_to((0.6, -0.1));
    frame.close_path();
    let fill = PlaneFillData {
        glyphs: vec![gaanim_scene::PlaneGlyph {
            outline: Arc::new(frame),
            placement: kurbo::Affine::IDENTITY,
            color: peniko::Color::from_rgb8(255, 0, 0),
            gradient: None,
            clip: None,
            image: None,
        }],
    };
    let above = placed(1.0, DVec3::new(0.0, 0.0, 1.0));
    let mut sources = MeshSources::default();
    let mut meshes = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
    meshes.push(
        0,
        [(1, 1), (1, 1)],
        [None, None],
        &Content {
            transform: &floor,
            opacity: &opacity,
            mesh: Some(&lit),
            lines: None,
            material: lit.material.as_ref(),
            line_width: None,
            line_range: None,
            fill: None,
            joints: None,
            glow: None,
            shadow: None,
            blur: None,
            instances: None,
            morph: None,
            volume: None,
        },
    );
    assert!(meshes.push_fill(
        0,
        (2, 1),
        None,
        &Content {
            transform: &above,
            opacity: &opacity,
            mesh: None,
            lines: None,
            material: None,
            line_width: None,
            line_range: None,
            fill: Some(&fill),
            joints: None,
            glow: None,
            shadow: None,
            blur: None,
            instances: None,
            morph: None,
            volume: None,
        },
    ));
    let illumination = Illumination {
        lighting: Lighting3D::default(),
        lights: vec![Light {
            position: DVec3::new(1.0, 0.0, 1.0),
            directional: true,
            color: [1.0; 3],
            spot: None,
        }],
        ..Default::default()
    };
    let frames: Vec<GpuMeshFrame> = meshes
        .finish(
            &camera,
            |_| camera,
            ndc_to_canvas,
            line_width,
            &illumination,
        )
        .into_iter()
        .map(|(_, frame)| frame)
        .collect();
    let mut scene = vello::Scene::new();
    scene.append(&frames[0].scene(), Some(placement));
    let mut three_d = GpuThreeD::default();
    assert!(three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames));
    let target = gpu.target(64, 64);
    gpu.render(&scene, &target);
    let pixels = gpu.read(&target);
    let pixel = |x: usize, y: usize| u16::from(pixels[(y * 64 + x) * 4]);
    // At 16 pixels per unit: the frame's shadow at (-0.67, 0), its hole's
    // at (-0.5, 0), and the floor in the light at (-0.67, 0.75).
    let (shadow, hole, light) = (pixel(21, 32), pixel(24, 32), pixel(21, 20));
    assert!(shadow + 40 < light, "shadow {shadow}, light {light}");
    assert!(hole + 10 > light, "hole {hole}, light {light}");
}

#[test]
fn translucent_layers_behind_filled_outlines_do_not_cover_them() {
    let Some(mut gpu) = crate::background::test_gpu::test_gpu() else {
        return;
    };
    let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
    camera.position = DVec3::new(0.0, 0.0, 10.0);
    let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
    let placement = camera.to_vello_transform();
    // A half-opaque blue sheet, and a red square a unit in front of it.
    let sheet = quad(peniko::Color::from_rgb8(0, 0, 255));
    let behind = placed(1.0, DVec3::ZERO);
    let half = GlobalOpacity(0.5);
    let fill = PlaneFillData {
        glyphs: vec![gaanim_scene::PlaneGlyph {
            outline: Arc::new(kurbo::Shape::to_path(
                &kurbo::Rect::new(-0.5, -0.5, 0.5, 0.5),
                0.0,
            )),
            placement: kurbo::Affine::IDENTITY,
            color: peniko::Color::from_rgb8(255, 0, 0),
            gradient: None,
            clip: None,
            image: None,
        }],
    };
    let front = placed(1.0, DVec3::new(0.0, 0.0, 1.0));
    let opaque = GlobalOpacity(1.0);
    let mut sources = MeshSources::default();
    let mut meshes = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
    meshes.push(
        0,
        [(1, 1), (1, 1)],
        [None, None],
        &Content {
            transform: &behind,
            opacity: &half,
            mesh: Some(&sheet),
            lines: None,
            material: None,
            line_width: None,
            line_range: None,
            fill: None,
            joints: None,
            glow: None,
            shadow: None,
            blur: None,
            instances: None,
            morph: None,
            volume: None,
        },
    );
    assert!(meshes.push_fill(
        0,
        (2, 1),
        None,
        &Content {
            transform: &front,
            opacity: &opaque,
            mesh: None,
            lines: None,
            material: None,
            line_width: None,
            line_range: None,
            fill: Some(&fill),
            joints: None,
            glow: None,
            shadow: None,
            blur: None,
            instances: None,
            morph: None,
            volume: None,
        },
    ));
    let illumination = Illumination {
        lighting: Lighting3D::default(),
        lights: Vec::new(),
        ..Default::default()
    };
    let frames: Vec<GpuMeshFrame> = meshes
        .finish(
            &camera,
            |_| camera,
            ndc_to_canvas,
            line_width,
            &illumination,
        )
        .into_iter()
        .map(|(_, frame)| frame)
        .collect();
    let mut scene = vello::Scene::new();
    scene.append(&frames[0].scene(), Some(placement));
    let mut three_d = GpuThreeD::default();
    assert!(three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames));
    let target = gpu.target(64, 64);
    gpu.render(&scene, &target);
    let pixels = gpu.read(&target);
    let pixel = |x: usize, y: usize| &pixels[(y * 64 + x) * 4..(y * 64 + x) * 4 + 3];
    // The square, drawn after the sheet's layer, is not blended with it.
    let square = pixel(32, 32);
    assert!(square[0] > 240 && square[2] < 10, "square {square:?}");
    // Beside it, the sheet shows.
    let sheet = pixel(32, 18);
    assert!(sheet[2] > 60 && sheet[0] < 10, "sheet {sheet:?}");
}

/// The red channel at `pixels` of a 64 x 64 render of a lit floor 2
/// units on a side and a card a unit above it, right of the middle,
/// under `light`, seen from above at 16 pixels per unit.
fn floor_and_card(lights: Vec<Light>) -> Option<Vec<u8>> {
    let mut gpu = crate::background::test_gpu::test_gpu()?;
    let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
    camera.position = DVec3::new(0.0, 0.0, 10.0);
    let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
    let placement = camera.to_vello_transform();
    let opacity = GlobalOpacity(1.0);
    let lit = lit_quad();
    let floor = placed(1.0, DVec3::ZERO);
    let card = placed(0.25, DVec3::new(0.5, 0.0, 1.0));
    let mut sources = MeshSources::default();
    let mut meshes = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
    for (key, transform) in [(1, &floor), (2, &card)] {
        meshes.push(
            0,
            [(key, 1), (key, 1)],
            [None, None],
            &Content {
                transform,
                opacity: &opacity,
                mesh: Some(&lit),
                lines: None,
                material: lit.material.as_ref(),
                line_width: None,
                line_range: None,
                fill: None,
                joints: None,
                glow: None,
                shadow: None,
                blur: None,
                instances: None,
                morph: None,
                volume: None,
            },
        );
    }
    let illumination = Illumination {
        lighting: Lighting3D::default(),
        lights,
        ..Default::default()
    };
    let frames: Vec<GpuMeshFrame> = meshes
        .finish(
            &camera,
            |_| camera,
            ndc_to_canvas,
            line_width,
            &illumination,
        )
        .into_iter()
        .map(|(_, frame)| frame)
        .collect();
    let mut scene = vello::Scene::new();
    scene.append(&frames[0].scene(), Some(placement));
    let mut three_d = GpuThreeD::default();
    three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames);
    let target = gpu.target(64, 64);
    gpu.render(&scene, &target);
    Some(gpu.read(&target).chunks(4).map(|pixel| pixel[0]).collect())
}

#[test]
fn a_point_light_shadows_through_the_face_of_its_cube_the_floor_lies_in() {
    // From (1.5, 0, 3) through the card, the shadow falls around the
    // middle of the floor, which the light sees through its -z face.
    let Some(red) = floor_and_card(vec![Light {
        position: DVec3::new(1.5, 0.0, 3.0),
        directional: false,
        color: [1.0; 3],
        spot: None,
    }]) else {
        return;
    };
    // At 16 pixels per unit: the shadow at (-0.2, 0), the floor in the
    // light at (-0.2, 0.75).
    let (shadow, light) = (red[32 * 64 + 29], red[20 * 64 + 29]);
    assert!(
        u16::from(shadow) + 40 < u16::from(light),
        "shadow {shadow}, light {light}"
    );
}

/// A mesh drawn at many points takes one record per copy shown, in one
/// draw, each placed, sized and colored by its copy; hidden copies take
/// none.
#[test]
fn copies_of_a_mesh_draw_as_one_with_a_record_each() {
    let camera = camera(Projection::Orthographic { zoom: 1.0 });
    let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
    let mesh = lit_quad();
    let opacity = GlobalOpacity(1.0);
    let transform = placed(2.0, DVec3::new(1.0, 0.0, 0.0));
    let frame = |reveal: f32| {
        let instances = MeshInstances {
            copies: Arc::new(MeshCopies {
                positions: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 2.0, 0.0]],
                scales: Some(vec![[1.0; 3], [0.5; 3], [1.0; 3]]),
                rotations: None,
                colors: Some(vec![[1.0, 0.0, 0.0, 1.0]; 3]),
            }),
            reveal,
        };
        let mut sources = MeshSources::default();
        let mut meshes =
            GpuMeshes::new(&mut sources, (1920, 1080), camera.to_vello_transform(), 0).unwrap();
        meshes.push(
            0,
            [(1, 1), (1, 1)],
            [None, None],
            &Content {
                transform: &transform,
                opacity: &opacity,
                mesh: Some(&mesh),
                lines: None,
                material: mesh.material.as_ref(),
                line_width: None,
                line_range: None,
                fill: None,
                joints: None,
                glow: None,
                shadow: None,
                blur: None,
                instances: Some(&instances),
                morph: None,
                volume: None,
            },
        );
        meshes
            .finish(
                &camera,
                |_| camera,
                ndc_to_canvas,
                line_width,
                &Illumination::default(),
            )
            .remove(0)
            .1
    };
    let shown = frame(1.0);
    assert_eq!(shown.draws.len(), 1);
    assert_eq!(shown.draws[0].copies, 0..3);
    assert_eq!(shown.instances.len(), 3 * INSTANCE_BYTES);
    let value = |record: usize, at: usize| {
        let start = record * INSTANCE_BYTES + at * 4;
        f32::from_le_bytes(shown.instances[start..start + 4].try_into().unwrap())
    };
    // The third copy sits 2 units up in the drawable, 4 in the world;
    // the second is half the size; each takes its own color.
    assert_eq!([value(2, 12), value(2, 13)], [1.0, 4.0]);
    assert_eq!(value(1, 0), 1.0);
    assert_eq!(value(0, 28), 1.0);
    assert_eq!(value(0, 29), 0.0);
    // Before they grow in, none draws.
    let hidden = frame(0.0);
    assert_eq!(hidden.draws[0].copies, 0..0);
    assert!(hidden.instances.is_empty());
}

#[test]
fn meshes_alike_share_one_upload_and_draw_as_one() {
    let camera = camera(Projection::Orthographic { zoom: 1.0 });
    let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
    let lit = lit_quad();
    let mut other = lit_quad();
    other.vertices[2] = [2.0, 2.0, 0.0];
    let opacity = GlobalOpacity(1.0);
    let places: Vec<GlobalSpatialTransform> = (0..4)
        .map(|index| placed(0.5, DVec3::new(index as f64, 0.0, 0.0)))
        .collect();
    let mut sources = MeshSources::default();
    let mut meshes =
        GpuMeshes::new(&mut sources, (1920, 1080), camera.to_vello_transform(), 0).unwrap();
    for (index, transform) in places.iter().enumerate() {
        let mesh = if index == 2 { &other } else { &lit };
        meshes.push(
            0,
            [(index as u64, 1), (index as u64, 1)],
            [None, None],
            &Content {
                transform,
                opacity: &opacity,
                mesh: Some(mesh),
                lines: None,
                material: mesh.material.as_ref(),
                line_width: None,
                line_range: None,
                fill: None,
                joints: None,
                glow: None,
                shadow: None,
                blur: None,
                instances: None,
                morph: None,
                volume: None,
            },
        );
    }
    let frames = meshes.finish(
        &camera,
        |_| camera,
        ndc_to_canvas,
        line_width,
        &Illumination::default(),
    );
    let frame = &frames[0].1;
    assert_eq!(frame.instances.len(), 4 * INSTANCE_BYTES);
    // The three quads alike share one source and sit next to each
    // other, so they draw as one; the other quad draws apart.
    let alike = (0..4)
        .map(|reference| -> Vec<usize> {
            (0..4)
                .filter(|index| {
                    frame.draws[*index].source.vertices == frame.draws[reference].source.vertices
                })
                .collect()
        })
        .max_by_key(Vec::len)
        .unwrap();
    assert_eq!(alike.len(), 3);
    assert!(alike.windows(2).all(|pair| pair[1] == pair[0] + 1));
    let [first, .., last] = alike[..] else {
        unreachable!()
    };
    assert!(Arc::ptr_eq(
        &frame.draws[first].source,
        &frame.draws[last].source
    ));
    assert!(draw_alike(frame, first, first + 1) && draw_alike(frame, first, last));
    let apart = (0..4).find(|index| !alike.contains(index)).unwrap();
    assert!(!draw_alike(frame, first, apart));
}

#[test]
fn outlines_push_a_cube_out_along_the_mean_normal_of_its_corners() {
    // A tetrahedron whose faces have vertices of their own: each copy of
    // a corner pushes out the same way, between its three faces.
    let corners = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
    ];
    let faces = [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
    let tetrahedron = TriangleMeshData {
        vertices: faces
            .iter()
            .flatten()
            .map(|&corner| corners[corner])
            .collect(),
        indices: (0..12).collect(),
        ..quad(peniko::Color::WHITE)
    };
    let source = MeshSource::mesh(&tetrahedron, true, true).unwrap();
    assert_eq!(source.outline.len(), 12 * 12);
    let direction = |vertex: usize| -> [f32; 3] {
        std::array::from_fn(|axis| {
            let at = vertex * 12 + axis * 4;
            f32::from_le_bytes(source.outline[at..at + 4].try_into().unwrap())
        })
    };
    // The origin corner's copies (vertices 0, 3 and 6) all point away
    // from the solid, equally along the three axes.
    for vertex in [0, 3, 6] {
        let [x, y, z] = direction(vertex);
        assert!(
            x < 0.0 && (x - y).abs() < 1e-5 && (y - z).abs() < 1e-5,
            "{x} {y} {z}"
        );
    }
    // Unlit meshes have no material to outline them.
    assert!(
        MeshSource::mesh(&tetrahedron, false, true)
            .unwrap()
            .outline
            .is_empty()
    );
    // Nor do meshes whose material draws no outline.
    assert!(
        MeshSource::mesh(&tetrahedron, true, false)
            .unwrap()
            .outline
            .is_empty()
    );
}

#[test]
fn an_outlined_mesh_draws_its_outline_around_its_silhouette() {
    let Some(mut gpu) = crate::background::test_gpu::test_gpu() else {
        return;
    };
    let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
    camera.position = DVec3::new(0.0, 0.0, 10.0);
    let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
    let placement = camera.to_vello_transform();
    let opacity = GlobalOpacity(1.0);
    let mut card = lit_quad();
    card.material = Some(
        Material3D {
            color: peniko::Color::BLACK,
            emissive: peniko::Color::from_rgb8(0, 0, 255),
            emissive_strength: 1.0,
            ..Material3D::default()
        }
        .with_outline(peniko::Color::from_rgb8(255, 0, 0), 0.25)
        .unwrap(),
    );
    // A card 1 unit on a side in the middle: its outline reaches 0.25
    // units beyond its edges, 4 pixels at 16 pixels per unit.
    let transform = placed(0.5, DVec3::ZERO);
    let mut sources = MeshSources::default();
    let mut meshes = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
    meshes.push(
        0,
        [(1, 1), (1, 1)],
        [None, None],
        &Content {
            transform: &transform,
            opacity: &opacity,
            mesh: Some(&card),
            lines: None,
            material: card.material.as_ref(),
            line_width: None,
            line_range: None,
            fill: None,
            joints: None,
            glow: None,
            shadow: None,
            blur: None,
            instances: None,
            morph: None,
            volume: None,
        },
    );
    let frames: Vec<GpuMeshFrame> = meshes
        .finish(
            &camera,
            |_| camera,
            ndc_to_canvas,
            line_width,
            &Illumination::default(),
        )
        .into_iter()
        .map(|(_, frame)| frame)
        .collect();
    assert!(frames[0].draws[0].outlined);
    let mut scene = vello::Scene::new();
    scene.append(&frames[0].scene(), Some(placement));
    let mut three_d = GpuThreeD::default();
    three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames);
    let target = gpu.target(64, 64);
    gpu.render(&scene, &target);
    let pixels = gpu.read(&target);
    let pixel = |x: usize, y: usize| &pixels[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
    // The card's middle is the card; 2 pixels beyond its right edge
    // (x = 40 + 2), the outline; well beyond it, nothing.
    assert!(
        pixel(32, 32)[2] > 200 && pixel(32, 32)[0] < 100,
        "{:?}",
        pixel(32, 32)
    );
    assert!(
        pixel(42, 32)[0] > 200 && pixel(42, 32)[2] < 100,
        "{:?}",
        pixel(42, 32)
    );
    assert_eq!(pixel(50, 32)[3], 0);
}

#[test]
fn a_mesh_draws_into_the_image_the_scene_shows() {
    let Some(mut gpu) = crate::background::test_gpu::test_gpu() else {
        return;
    };
    let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
    camera.position = DVec3::new(0.0, 0.0, 10.0);
    let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
    let placement = camera.to_vello_transform();
    let transform = GlobalSpatialTransform::default();
    let opacity = GlobalOpacity(1.0);
    let red = quad(peniko::Color::from_rgb8(255, 0, 0));
    let mut sources = MeshSources::default();
    let mut meshes = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
    meshes.push(
        0,
        [(1, 1), (1, 1)],
        [None, None],
        &Content {
            line_width: None,
            line_range: None,
            fill: None,
            joints: None,
            transform: &transform,
            opacity: &opacity,
            mesh: Some(&red),
            lines: None,
            material: None,
            glow: None,
            shadow: None,
            blur: None,
            instances: None,
            morph: None,
            volume: None,
        },
    );
    let frames: Vec<GpuMeshFrame> = meshes
        .finish(
            &camera,
            |_| camera,
            ndc_to_canvas,
            line_width,
            &Illumination::default(),
        )
        .into_iter()
        .map(|(_, frame)| frame)
        .collect();
    let mut scene = vello::Scene::new();
    scene.append(&frames[0].scene(), Some(placement));
    let mut three_d = GpuThreeD::default();
    assert!(three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames));
    // The same frame again keeps the image.
    assert!(!three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames));
    let target = gpu.target(64, 64);
    gpu.render(&scene, &target);
    let pixels = gpu.read(&target);
    let pixel = |x: usize, y: usize| &pixels[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
    // The quad spans the middle half of the frame.
    assert_eq!(pixel(32, 32), [255, 0, 0, 255]);
    assert_eq!(pixel(4, 4)[3], 0);
}

/// `quad` as an imported model's primitive: its UVs from its corners
/// (v down), `texture`, `alpha`, and with `skinned` every vertex on
/// joint 0.
fn model_quad(texture: Option<MeshTexture>, alpha: AlphaMode, skinned: bool) -> TriangleMeshData {
    let mut mesh = quad(peniko::Color::WHITE);
    mesh.uvs = Some(
        mesh.vertices
            .iter()
            .map(|v| [(v[0] + 1.0) * 0.5, (1.0 - v[1]) * 0.5])
            .collect(),
    );
    let count = mesh.vertices.len();
    mesh.surface = Some(Arc::new(gaanim_scene::MeshSurface {
        texture,
        alpha,
        double_sided: false,
        skin: skinned.then(|| gaanim_scene::SkinWeights {
            joints: vec![[0; 4]; count],
            weights: vec![[1.0, 0.0, 0.0, 0.0]; count],
        }),
        ..Default::default()
    }));
    mesh
}

/// A 2x1 texture: its left texel `left`, its right one `right`, read
/// with nearest texels and clamped.
fn two_texels(left: [u8; 4], right: [u8; 4]) -> MeshTexture {
    MeshTexture {
        image: ImageData {
            data: Blob::from([left, right].concat()),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: 2,
            height: 1,
        },
        wrap: [TextureWrap::ClampToEdge; 2],
        nearest: true,
    }
}

/// The RGBA pixels of `meshes` drawn into a 64-pixel frame 4 units wide,
/// each with its palette, and whether the model pipelines were built.
fn draw_meshes(meshes: &[(TriangleMeshData, Option<Arc<[[f32; 16]]>>)]) -> Option<(Vec<u8>, bool)> {
    let meshes: Vec<_> = meshes
        .iter()
        .map(|(mesh, joints)| (mesh.clone(), joints.clone(), None))
        .collect();
    draw_morphed(&meshes)
}

/// The RGBA pixels of `meshes` drawn as [`draw_meshes`] draws them, each
/// with its palette and its morph targets' weights.
#[allow(clippy::type_complexity)]
fn draw_morphed(
    meshes: &[(
        TriangleMeshData,
        Option<Arc<[[f32; 16]]>>,
        Option<Arc<[f32]>>,
    )],
) -> Option<(Vec<u8>, bool)> {
    let mut gpu = crate::background::test_gpu::test_gpu()?;
    let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
    camera.position = DVec3::new(0.0, 0.0, 10.0);
    let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
    let placement = camera.to_vello_transform();
    let transform = GlobalSpatialTransform::default();
    let opacity = GlobalOpacity(1.0);
    let mut sources = MeshSources::default();
    let mut pending = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
    for (index, (mesh, joints, morph)) in meshes.iter().enumerate() {
        let key = index as u64 + 1;
        pending.push(
            0,
            [(key, 1), (key, 1)],
            [None, None],
            &Content {
                transform: &transform,
                opacity: &opacity,
                mesh: Some(mesh),
                lines: None,
                material: mesh.material.as_ref(),
                line_width: None,
                line_range: None,
                fill: None,
                joints: joints.as_ref(),
                glow: None,
                shadow: None,
                blur: None,
                instances: None,
                morph: morph.as_ref(),
                volume: None,
            },
        );
    }
    let frames: Vec<GpuMeshFrame> = pending
        .finish(
            &camera,
            |_| camera,
            ndc_to_canvas,
            line_width,
            &Illumination::default(),
        )
        .into_iter()
        .map(|(_, frame)| frame)
        .collect();
    let mut scene = vello::Scene::new();
    scene.append(&frames[0].scene(), Some(placement));
    let mut three_d = GpuThreeD::default();
    three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames);
    let target = gpu.target(64, 64);
    gpu.render(&scene, &target);
    Some((gpu.read(&target), three_d.models.is_some()))
}

fn pixel(pixels: &[u8], x: usize, y: usize) -> [u8; 4] {
    pixels[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4]
        .try_into()
        .unwrap()
}

/// The RGBA pixels of `meshes`, each placed by its matrix and with its
/// glow, drawn as [`draw_meshes`] draws them, and whether the glow
/// pipelines were built.
fn draw_glowing(meshes: &[(TriangleMeshData, DMat4, Option<Glow>)]) -> Option<(Vec<u8>, bool)> {
    let meshes: Vec<_> = meshes
        .iter()
        .map(|(mesh, matrix, glow)| (mesh.clone(), *matrix, glow.clone(), None, None))
        .collect();
    draw_soft(&meshes)
}

/// A mesh, where it stands, and its glow, shadow and blur.
type Soft = (
    TriangleMeshData,
    DMat4,
    Option<Glow>,
    Option<DropShadow>,
    Option<GaussianBlur>,
);

/// Draw `meshes` with their glows, shadows and blurs: the pixels, and
/// whether the glow pipelines were built.
fn draw_soft(meshes: &[Soft]) -> Option<(Vec<u8>, bool)> {
    let mut gpu = crate::background::test_gpu::test_gpu()?;
    let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
    camera.position = DVec3::new(0.0, 0.0, 10.0);
    let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
    let placement = camera.to_vello_transform();
    let opacity = GlobalOpacity(1.0);
    let transforms: Vec<GlobalSpatialTransform> = meshes
        .iter()
        .map(|(_, matrix, _, _, _)| GlobalSpatialTransform {
            mat4: *matrix,
            ..Default::default()
        })
        .collect();
    let mut sources = MeshSources::default();
    let mut pending = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
    for (index, ((mesh, _, glow, shadow, blur), transform)) in
        meshes.iter().zip(&transforms).enumerate()
    {
        let key = index as u64 + 1;
        pending.push(
            0,
            [(key, 1), (key, 1)],
            [None, None],
            &Content {
                transform,
                opacity: &opacity,
                mesh: Some(mesh),
                lines: None,
                material: mesh.material.as_ref(),
                line_width: None,
                line_range: None,
                fill: None,
                joints: None,
                glow: glow.clone(),
                shadow: shadow.clone(),
                blur: *blur,
                instances: None,
                morph: None,
                volume: None,
            },
        );
    }
    let frames: Vec<GpuMeshFrame> = pending
        .finish(
            &camera,
            |_| camera,
            ndc_to_canvas,
            line_width,
            &Illumination::default(),
        )
        .into_iter()
        .map(|(_, frame)| frame)
        .collect();
    let mut scene = vello::Scene::new();
    scene.append(&frames[0].scene(), Some(placement));
    let mut three_d = GpuThreeD::default();
    three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames);
    let target = gpu.target(64, 64);
    gpu.render(&scene, &target);
    Some((gpu.read(&target), three_d.glow.is_some()))
}

/// The RGBA pixels of `meshes`, each placed by its matrix and at its
/// opacity, drawn as [`draw_meshes`] draws them with translucent
/// content blending as `transparency` says, and whether the peel
/// pipelines were built.
fn draw_translucent(
    meshes: &[(TriangleMeshData, DMat4, f32)],
    transparency: Transparency3D,
) -> Option<(Vec<u8>, bool)> {
    let illumination = Illumination {
        transparency,
        ..Default::default()
    };
    let (pixels, three_d) = draw_illuminated(meshes, &illumination)?;
    Some((pixels, three_d.peel.is_some()))
}

/// The RGBA pixels of `meshes`, each placed by its matrix and at its
/// opacity, drawn as [`draw_meshes`] draws them under `illumination`,
/// and what drew them.
fn draw_illuminated(
    meshes: &[(TriangleMeshData, DMat4, f32)],
    illumination: &Illumination,
) -> Option<(Vec<u8>, GpuThreeD)> {
    let mut gpu = crate::background::test_gpu::test_gpu()?;
    let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
    camera.position = DVec3::new(0.0, 0.0, 10.0);
    let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
    let placement = camera.to_vello_transform();
    let placed: Vec<(GlobalSpatialTransform, GlobalOpacity)> = meshes
        .iter()
        .map(|(_, matrix, opacity)| {
            let transform = GlobalSpatialTransform {
                mat4: *matrix,
                ..Default::default()
            };
            (transform, GlobalOpacity(*opacity))
        })
        .collect();
    let mut sources = MeshSources::default();
    let mut pending = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
    for (index, ((mesh, _, _), (transform, opacity))) in meshes.iter().zip(&placed).enumerate() {
        let key = index as u64 + 1;
        pending.push(
            0,
            [(key, 1), (key, 1)],
            [None, None],
            &Content {
                transform,
                opacity,
                mesh: Some(mesh),
                lines: None,
                material: mesh.material.as_ref(),
                line_width: None,
                line_range: None,
                fill: None,
                joints: None,
                glow: None,
                shadow: None,
                blur: None,
                instances: None,
                morph: None,
                volume: None,
            },
        );
    }
    let frames: Vec<GpuMeshFrame> = pending
        .finish(&camera, |_| camera, ndc_to_canvas, line_width, illumination)
        .into_iter()
        .map(|(_, frame)| frame)
        .collect();
    let mut scene = vello::Scene::new();
    scene.append(&frames[0].scene(), Some(placement));
    let mut three_d = GpuThreeD::default();
    three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames);
    let target = gpu.target(64, 64);
    gpu.render(&scene, &target);
    Some((gpu.read(&target), three_d))
}

/// A volume filled with one value that the transfer gives full red at
/// full opacity, from -1 to 1 on each axis, absorbing `density` per
/// unit.
fn red_volume(density: f32) -> gaanim_scene::VolumeData {
    let transfer: Vec<u8> = [255, 0, 0, 255].repeat(256);
    gaanim_scene::VolumeData {
        values: gaanim_scene::VolumeData::pack(&[255; 8]),
        size: [2, 2, 2],
        min: [-1.0; 3],
        max: [1.0; 3],
        transfer: ImageData {
            data: Blob::from(transfer),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: 256,
            height: 1,
        },
        density,
    }
}

/// The RGBA pixels of `volume`, and of a white quad through its middle
/// when `quad`, drawn as [`draw_meshes`] draws them.
fn draw_volume(volume: &gaanim_scene::VolumeData, quad_inside: bool) -> Option<Vec<u8>> {
    let mut gpu = crate::background::test_gpu::test_gpu()?;
    let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
    camera.position = DVec3::new(0.0, 0.0, 10.0);
    let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
    let placement = camera.to_vello_transform();
    let transform = GlobalSpatialTransform::default();
    let opacity = GlobalOpacity(1.0);
    let white = quad(peniko::Color::WHITE);
    let mut sources = MeshSources::default();
    let mut pending = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
    let content = |mesh, volume| Content {
        transform: &transform,
        opacity: &opacity,
        mesh,
        lines: None,
        material: None,
        line_width: None,
        line_range: None,
        fill: None,
        joints: None,
        glow: None,
        shadow: None,
        blur: None,
        instances: None,
        morph: None,
        volume,
    };
    assert!(pending.push_volume(0, (1, 1), None, &content(None, Some(volume))));
    if quad_inside {
        pending.push(
            0,
            [(2, 1), (2, 1)],
            [None, None],
            &content(Some(&white), None),
        );
    }
    let frames: Vec<GpuMeshFrame> = pending
        .finish(
            &camera,
            |_| camera,
            ndc_to_canvas,
            line_width,
            &Illumination::default(),
        )
        .into_iter()
        .map(|(_, frame)| frame)
        .collect();
    let mut scene = vello::Scene::new();
    scene.append(&frames[0].scene(), Some(placement));
    let mut three_d = GpuThreeD::default();
    three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames);
    let target = gpu.target(64, 64);
    gpu.render(&scene, &target);
    Some(gpu.read(&target))
}

/// A volume absorbs by its depth through it, and stops at what is
/// opaque inside it: a quad through its middle shows through the half
/// in front of it.
#[test]
fn a_volume_absorbs_by_its_depth_and_stops_at_what_is_opaque() {
    let volume = red_volume(1.0);
    let Some(alone) = draw_volume(&volume, false) else {
        return;
    };
    // Two units deep: 1 - e^-2 of red.
    let middle = pixel(&alone, 32, 32);
    let expected = (1.0 - (-2.0_f32).exp()) * 255.0;
    assert!((f32::from(middle[3]) - expected).abs() < 6.0, "{middle:?}");
    assert!(middle[1] < 4 && middle[2] < 4, "{middle:?}");
    // Beyond the box, nothing.
    assert_eq!(pixel(&alone, 4, 4)[3], 0);
    // One unit in front of the quad: 1 - e^-1 of red over white.
    let over = pixel(&draw_volume(&volume, true).unwrap(), 32, 32);
    let white = (-1.0_f32).exp() * 255.0;
    assert_eq!(over[3], 255);
    assert!(over[0] > 250, "{over:?}");
    assert!((f32::from(over[1]) - white).abs() < 8.0, "{over:?}");
}

/// A white quad in focus on the left and one farther away on the right:
/// out of focus, the far one's edge spreads over several pixels while
/// the near one's stays sharp, and in focus both are sharp.
#[test]
fn the_depth_of_field_blurs_what_lies_away_from_the_focus() {
    let white = quad(peniko::Color::WHITE);
    // 16 pixels per unit; the camera stands 10 units in front of its
    // target, so the far quad lies at 1.5 times its distance.
    let near =
        DMat4::from_translation(DVec3::new(-1.0, 0.0, 0.0)) * DMat4::from_scale(DVec3::splat(0.5));
    let far =
        DMat4::from_translation(DVec3::new(1.0, 0.0, -5.0)) * DMat4::from_scale(DVec3::splat(0.5));
    let meshes = [(white.clone(), near, 1.0), (white, far, 1.0)];
    let blurred = Illumination {
        depth_of_field: gaanim_scene::DepthOfField {
            focus: 1.0,
            aperture: 1.0,
            max_blur: 1.0,
        },
        ..Default::default()
    };
    let Some((sharp, three_d)) = draw_illuminated(&meshes, &Illumination::default()) else {
        return;
    };
    assert!(three_d.focus.is_none(), "a frame in focus builds no blur");
    let (out, three_d) = draw_illuminated(&meshes, &blurred).unwrap();
    assert!(three_d.focus.is_some());
    // The near quad spans pixels 8 to 24 across, the far one 40 to 56.
    let alpha = |pixels: &[u8], x| pixel(pixels, x, 32)[3];
    assert_eq!((alpha(&sharp, 23), alpha(&sharp, 25)), (255, 0));
    assert_eq!((alpha(&sharp, 40), alpha(&sharp, 38)), (255, 0));
    assert_eq!(alpha(&out, 23), 255, "in focus");
    assert!(alpha(&out, 25) <= 2, "in focus: {}", alpha(&out, 25));
    // A third of a unit of blur: 5 pixels of sigma.
    let (inside, edge, outside) = (alpha(&out, 48), alpha(&out, 40), alpha(&out, 36));
    assert!(inside > 150, "{inside}");
    assert!((40..200).contains(&edge), "{edge}");
    assert!((5..130).contains(&outside), "{outside}");
}

/// A spot light lights the floor within its cone and leaves the rest
/// to the sky and ground light, where a point light at its place lights
/// all of it.
#[test]
fn a_spot_light_lights_only_within_its_cone() {
    let position = DVec3::new(-0.6, 0.6, 3.0);
    let target = DVec3::new(-0.6, 0.6, 0.0);
    let cone = gaanim_scene::SpotCone {
        target,
        angle: 0.25,
        softness: 0.2,
    };
    let spot = Light {
        position,
        directional: false,
        color: [1.0; 3],
        spot: Spot::new(&cone, position),
    };
    assert!(spot.spot.is_some());
    let point = Light { spot: None, ..spot };
    let Some(spotted) = floor_and_card(vec![spot]) else {
        return;
    };
    let lit = floor_and_card(vec![point]).unwrap();
    // At 16 pixels per unit: the cone's middle at (-0.6, 0.6), and the
    // floor well outside it at (0.75, -0.75).
    let (within, beyond) = (spotted[22 * 64 + 22], spotted[44 * 64 + 44]);
    assert!(
        u16::from(beyond) + 40 < u16::from(within),
        "within {within}, beyond {beyond}"
    );
    assert!(
        lit[44 * 64 + 44] > beyond + 40,
        "{} {beyond}",
        lit[44 * 64 + 44]
    );
    assert!(within.abs_diff(lit[22 * 64 + 22]) < 4);
    // A spot that aims at itself gives no light.
    let aimless = gaanim_scene::SpotCone {
        target: position,
        ..cone
    };
    assert!(Spot::new(&aimless, position).is_none());
}

/// Two translucent quads that cross, red nearer on the left and blue on
/// the right: peeled, each half shows the nearer one over the other;
/// sorted as layers, one lies over the other on both halves.
#[test]
fn peeled_translucent_drawables_blend_in_the_order_they_lie_where_they_cross() {
    let red = quad(peniko::Color::new([1.0, 0.0, 0.0, 1.0]));
    let blue = quad(peniko::Color::new([0.0, 0.0, 1.0, 1.0]));
    let crossing = |angle: f64| DMat4::from_rotation_y(angle);
    let meshes = [(red, crossing(0.5), 0.5), (blue, crossing(-0.5), 0.5)];
    let Some((sorted, built)) = draw_translucent(&meshes, Transparency3D::default()) else {
        return;
    };
    assert!(!built, "a frame that does not peel builds no peel pipeline");
    let (peeled, built) = draw_translucent(&meshes, Transparency3D { layers: 4 }).unwrap();
    assert!(built);
    let (left, right) = (pixel(&peeled, 24, 32), pixel(&peeled, 40, 32));
    assert!(
        left[0] > left[2] + 40,
        "red lies nearer on the left: {left:?}"
    );
    assert!(
        right[2] > right[0] + 40,
        "blue lies nearer on the right: {right:?}"
    );
    assert_eq!(left[3], right[3]);
    let (left, right) = (pixel(&sorted, 24, 32), pixel(&sorted, 40, 32));
    assert_eq!(
        left[0] > left[2],
        right[0] > right[2],
        "sorted, one quad lies over the other on both halves: {left:?} {right:?}"
    );
}

/// One translucent drawable folded over itself shows its far fold
/// through its near one when peeled, and only its nearest surface when
/// it blends as a layer; one layer peels only the nearest.
#[test]
fn peeled_layers_show_the_folds_of_one_translucent_drawable() {
    let mut folded = quad(peniko::Color::WHITE);
    folded.vertices = vec![
        [-1.0, -1.0, 0.5],
        [1.0, -1.0, 0.5],
        [1.0, 1.0, 0.5],
        [-1.0, 1.0, 0.5],
        [-1.0, -1.0, -0.5],
        [1.0, -1.0, -0.5],
        [1.0, 1.0, -0.5],
        [-1.0, 1.0, -0.5],
    ];
    folded.indices = vec![0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7];
    folded.color = None;
    let (red, blue) = ([1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]);
    folded.colors = Some(vec![red, red, red, red, blue, blue, blue, blue]);
    let meshes = [(folded, DMat4::IDENTITY, 0.5)];
    let Some((sorted, _)) = draw_translucent(&meshes, Transparency3D::default()) else {
        return;
    };
    let (peeled, _) = draw_translucent(&meshes, Transparency3D { layers: 4 }).unwrap();
    let (nearest, _) = draw_translucent(&meshes, Transparency3D { layers: 1 }).unwrap();
    let (sorted, peeled, nearest) = (
        pixel(&sorted, 32, 32),
        pixel(&peeled, 32, 32),
        pixel(&nearest, 32, 32),
    );
    assert!(
        sorted[2] < 4,
        "a layer shows its nearest surface: {sorted:?}"
    );
    assert!(peeled[2] > 40 && peeled[0] > peeled[2], "{peeled:?}");
    assert!(peeled[3] > sorted[3], "{peeled:?} {sorted:?}");
    // The peeled layer passes through 8 bits once more.
    assert!(
        nearest.iter().zip(sorted).all(|(a, b)| a.abs_diff(b) <= 2),
        "{nearest:?} {sorted:?}"
    );
}

fn red_glow() -> Option<Glow> {
    Some(Glow {
        radius: 0.5,
        intensity: 4.0,
        color: peniko::Color::new([1.0, 0.0, 0.0, 1.0]),
    })
}

/// A glowing mesh lights the pixels around it in its glow's color,
/// fading with the distance, and keeps its own.
#[test]
fn a_glow_lights_the_pixels_around_a_mesh() {
    let white = quad(peniko::Color::WHITE);
    let Some((plain, built)) = draw_glowing(&[(white.clone(), DMat4::IDENTITY, None)]) else {
        return;
    };
    assert!(!built, "a frame without glows builds no glow pipeline");
    let (glowing, built) = draw_glowing(&[(white, DMat4::IDENTITY, red_glow())]).unwrap();
    assert!(built);
    // The quad spans pixels 16 to 48; its glow blurs by 2 pixels.
    assert_eq!(pixel(&glowing, 32, 32), [255, 255, 255, 255]);
    assert_eq!(pixel(&plain, 50, 32), [0, 0, 0, 0]);
    let near = pixel(&glowing, 50, 32);
    assert!(
        near[0] > 60 && near[3] > 60 && near[1] < 8 && near[2] < 8,
        "{near:?}"
    );
    let far = pixel(&glowing, 53, 32);
    assert!(far[3] < near[3], "{far:?} {near:?}");
    assert!(pixel(&glowing, 62, 32)[3] < 4);
}

/// A mesh's drop shadow falls beside it by its offset, in its color,
/// and hides under the mesh itself.
#[test]
fn a_shadow_falls_beside_a_mesh_by_its_offset() {
    let shadow = DropShadow {
        offset: gaanim_core::glam::DVec2::new(0.5, -0.5),
        blur_radius: 0.05,
        color: peniko::Color::new([0.0, 0.0, 0.0, 1.0]),
    };
    let Some((pixels, built)) = draw_soft(&[(
        quad(peniko::Color::WHITE),
        DMat4::IDENTITY,
        None,
        Some(shadow),
        None,
    )]) else {
        return;
    };
    assert!(built);
    // The quad spans pixels 16 to 48, its shadow 24 to 56, 8 pixels
    // right and down.
    assert_eq!(pixel(&pixels, 32, 32), [255, 255, 255, 255]);
    let cast = pixel(&pixels, 52, 52);
    assert!(cast[3] > 230 && cast[0] < 8, "{cast:?}");
    assert_eq!(pixel(&pixels, 52, 20)[3], 0);
    assert_eq!(pixel(&pixels, 12, 12)[3], 0);
}

/// A blurred mesh spreads its color over what lies behind it, and what
/// lies in front of it hides its blur.
#[test]
fn a_blurred_mesh_spreads_its_color_over_what_lies_behind() {
    let blue = peniko::Color::new([0.0, 0.0, 1.0, 1.0]);
    let behind =
        DMat4::from_translation(DVec3::new(0.0, 0.0, -1.0)) * DMat4::from_scale(DVec3::splat(1.8));
    // A green strip in front of the white quad's lower edge.
    let front = DMat4::from_translation(DVec3::new(0.0, -1.0, 1.0))
        * DMat4::from_scale(DVec3::new(1.9, 0.12, 1.0));
    let blur = GaussianBlur { sigma: 0.25 };
    let Some((pixels, built)) = draw_soft(&[
        (quad(blue), behind, None, None, None),
        (
            quad(peniko::Color::WHITE),
            DMat4::IDENTITY,
            None,
            None,
            Some(blur),
        ),
        (
            quad(peniko::Color::new([0.0, 1.0, 0.0, 1.0])),
            front,
            None,
            None,
            None,
        ),
    ]) else {
        return;
    };
    assert!(built);
    // The white quad spans pixels 16 to 48, blurred by 4 pixels: white
    // in its middle, half white at its edge, blue farther out.
    let middle = pixel(&pixels, 32, 32);
    assert!(middle.iter().all(|value| *value > 245), "{middle:?}");
    let inside = pixel(&pixels, 45, 32);
    assert!(inside[0] < 245 && inside[0] > 128, "{inside:?}");
    let outside = pixel(&pixels, 51, 32);
    assert!(
        outside[0] > 40 && outside[0] < 128 && outside[2] > 200,
        "{outside:?}"
    );
    let far = pixel(&pixels, 60, 32);
    assert!(far[0] < 8 && far[2] > 245, "{far:?}");
    // The strip in front stays green across the quad's edge.
    let strip = pixel(&pixels, 32, 48);
    assert!(strip[1] > 245 && strip[0] < 8, "{strip:?}");
}

/// Outlines filled in a drawable's plane glow and cast a drop shadow as
/// meshes do.
#[test]
fn filled_outlines_glow_and_cast_shadows() {
    let Some(mut gpu) = crate::background::test_gpu::test_gpu() else {
        return;
    };
    let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
    camera.position = DVec3::new(0.0, 0.0, 10.0);
    let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
    let placement = camera.to_vello_transform();
    let opacity = GlobalOpacity(1.0);
    let at_origin = GlobalSpatialTransform::default();
    // A white square 2 units on a side: pixels 16 to 48.
    let square = kurbo::Shape::to_path(&kurbo::Rect::new(0.0, 0.0, 2.0, 2.0), 0.0);
    let fill = PlaneFillData {
        glyphs: vec![gaanim_scene::PlaneGlyph {
            outline: Arc::new(square),
            placement: kurbo::Affine::translate((-1.0, -1.0)),
            color: peniko::Color::WHITE,
            gradient: None,
            clip: None,
            image: None,
        }],
    };
    let mut draw = |glow: Option<Glow>, shadow: Option<DropShadow>| {
        let mut sources = MeshSources::default();
        let mut meshes = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
        assert!(meshes.push_fill(
            0,
            (1, 1),
            None,
            &Content {
                transform: &at_origin,
                opacity: &opacity,
                mesh: None,
                lines: None,
                material: None,
                line_width: None,
                line_range: None,
                fill: Some(&fill),
                joints: None,
                glow,
                shadow,
                blur: None,
                instances: None,
                morph: None,
                volume: None,
            },
        ));
        let frames: Vec<GpuMeshFrame> = meshes
            .finish(
                &camera,
                |_| camera,
                ndc_to_canvas,
                line_width,
                &Illumination::default(),
            )
            .into_iter()
            .map(|(_, frame)| frame)
            .collect();
        let mut scene = vello::Scene::new();
        scene.append(&frames[0].scene(), Some(placement));
        let mut three_d = GpuThreeD::default();
        three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames);
        let target = gpu.target(64, 64);
        gpu.render(&scene, &target);
        gpu.read(&target)
    };
    let glowing = draw(red_glow(), None);
    assert_eq!(pixel(&glowing, 32, 32), [255, 255, 255, 255]);
    let near = pixel(&glowing, 50, 32);
    assert!(near[0] > 60 && near[3] > 60 && near[1] < 8, "{near:?}");
    let shadowed = draw(
        None,
        Some(DropShadow {
            offset: gaanim_core::glam::DVec2::new(0.5, -0.5),
            blur_radius: 0.05,
            color: peniko::Color::new([0.0, 0.0, 0.0, 1.0]),
        }),
    );
    assert_eq!(pixel(&shadowed, 32, 32), [255, 255, 255, 255]);
    let cast = pixel(&shadowed, 52, 52);
    assert!(cast[3] > 230 && cast[0] < 8, "{cast:?}");
    assert_eq!(pixel(&shadowed, 12, 12)[3], 0);
}

/// The glow of a mesh hidden behind an opaque one does not show.
#[test]
fn a_hidden_mesh_does_not_glow() {
    let behind =
        DMat4::from_translation(DVec3::new(0.0, 0.0, -1.0)) * DMat4::from_scale(DVec3::splat(0.5));
    let front = DMat4::from_scale(DVec3::splat(1.5));
    let Some((pixels, _)) = draw_glowing(&[
        (quad(peniko::Color::WHITE), behind, red_glow()),
        (quad(peniko::Color::new([0.0, 0.0, 1.0, 1.0])), front, None),
    ]) else {
        return;
    };
    // Around where the small quad would glow, the front quad, and
    // outside it nothing.
    assert_eq!(pixel(&pixels, 44, 32), [0, 0, 255, 255]);
    assert_eq!(pixel(&pixels, 58, 32), [0, 0, 0, 0]);
}

/// A drawable in front of a glowing one covers its glow, as a drawable
/// drawn over a 2D glow does, and one behind it receives the glow.
#[test]
fn what_lies_in_front_of_a_glow_covers_it() {
    let glowing = DMat4::from_scale(DVec3::splat(0.5));
    // Blue covers the right half of the frame in front of the glowing
    // quad, and green its left half behind it.
    let side = |x: f64, z: f64| {
        DMat4::from_translation(DVec3::new(x, 0.0, z))
            * DMat4::from_scale(DVec3::new(1.0, 2.0, 1.0))
    };
    let Some((pixels, _)) = draw_glowing(&[
        (quad(peniko::Color::WHITE), glowing, red_glow()),
        (
            quad(peniko::Color::new([0.0, 0.0, 1.0, 1.0])),
            side(1.05, 1.0),
            None,
        ),
        (
            quad(peniko::Color::new([0.0, 1.0, 0.0, 1.0])),
            side(-1.05, -1.0),
            None,
        ),
    ]) else {
        return;
    };
    // The glowing quad spans pixels 24 to 40; blue starts at 33.
    assert_eq!(pixel(&pixels, 36, 42), [0, 0, 255, 255]);
    let behind = pixel(&pixels, 22, 32);
    assert!(behind[0] > 40 && behind[1] < 250, "{behind:?}");
}

#[test]
fn scenes_without_models_never_build_their_pipelines() {
    let Some((pixels, built)) = draw_meshes(&[(quad(peniko::Color::WHITE), None)]) else {
        return;
    };
    assert_eq!(pixel(&pixels, 32, 32), [255, 255, 255, 255]);
    assert!(!built);
}

#[test]
fn a_model_draws_its_texture_through_its_uvs() {
    let texture = two_texels([255, 0, 0, 255], [0, 0, 255, 255]);
    let mesh = model_quad(Some(texture), AlphaMode::Opaque, false);
    let Some((pixels, built)) = draw_meshes(&[(mesh, None)]) else {
        return;
    };
    assert!(built);
    // The quad spans pixels 16 to 48: its left half shows the left texel.
    assert_eq!(pixel(&pixels, 20, 32), [255, 0, 0, 255]);
    assert_eq!(pixel(&pixels, 44, 32), [0, 0, 255, 255]);
    assert_eq!(pixel(&pixels, 4, 4)[3], 0);
}

#[test]
fn a_cut_out_model_hides_its_transparent_texels_and_an_opaque_one_ignores_them() {
    let texture = two_texels([255, 0, 0, 255], [0, 255, 0, 40]);
    let masked = model_quad(Some(texture.clone()), AlphaMode::Mask(0.5), false);
    let Some((pixels, _)) = draw_meshes(&[(masked, None)]) else {
        return;
    };
    assert_eq!(pixel(&pixels, 20, 32), [255, 0, 0, 255]);
    assert_eq!(pixel(&pixels, 44, 32)[3], 0);
    let opaque = model_quad(Some(texture), AlphaMode::Opaque, false);
    let (pixels, _) = draw_meshes(&[(opaque, None)]).unwrap();
    assert_eq!(pixel(&pixels, 44, 32), [0, 255, 0, 255]);
}

#[test]
fn a_blended_model_draws_over_what_lies_behind_it() {
    let texture = two_texels([255, 255, 255, 128], [255, 255, 255, 128]);
    let mut front = model_quad(Some(texture), AlphaMode::Blend, false);
    for vertex in &mut front.vertices {
        vertex[2] = 1.0;
    }
    let back = quad(peniko::Color::from_rgb8(0, 0, 255));
    let Some((pixels, _)) = draw_meshes(&[(back, None), (front, None)]) else {
        return;
    };
    let [r, _, b, a] = pixel(&pixels, 32, 32);
    assert_eq!(a, 255);
    assert!(r > 100 && r < 200 && b == 255, "{r} {b}");
}

#[test]
fn a_skinned_model_draws_where_its_joints_take_it() {
    let mesh = model_quad(None, AlphaMode::Opaque, true);
    // Joint 0 one unit to the right: the quad spans pixels 32 to 64.
    let moved = DMat4::from_translation(DVec3::X).as_mat4().to_cols_array();
    let Some((pixels, _)) = draw_meshes(&[(mesh, Some(Arc::from(vec![moved])))]) else {
        return;
    };
    assert_eq!(pixel(&pixels, 20, 32)[3], 0);
    assert_eq!(pixel(&pixels, 56, 32), [255, 255, 255, 255]);
}

#[test]
fn a_morphed_model_draws_its_targets_by_their_weights() {
    // One target moves every corner a unit right: 16 pixels.
    let mut mesh = model_quad(None, AlphaMode::Opaque, false);
    let count = mesh.vertices.len();
    Arc::make_mut(mesh.surface.as_mut().unwrap()).morph_targets = vec![gaanim_scene::MorphTarget {
        positions: vec![[1.0, 0.0, 0.0]; count],
        normals: None,
    }];
    let weighed = |weight: f32| draw_morphed(&[(mesh.clone(), None, Some(Arc::from([weight])))]);
    // At weight 0 the quad spans pixels 16 to 48, at 1 from 32 to 64,
    // and at 0.5 from 24 to 56.
    let Some((rest, built)) = weighed(0.0) else {
        return;
    };
    assert!(built);
    assert_eq!(pixel(&rest, 20, 32)[3], 255);
    assert_eq!(pixel(&rest, 52, 32)[3], 0);
    let (full, _) = weighed(1.0).unwrap();
    assert_eq!(pixel(&full, 28, 32)[3], 0);
    assert_eq!(pixel(&full, 60, 32)[3], 255);
    let (half, _) = weighed(0.5).unwrap();
    for (x, alpha) in [(22, 0), (26, 255), (54, 255), (58, 0)] {
        assert_eq!(pixel(&half, x, 32)[3], alpha, "pixel {x}");
    }
}

#[test]
fn a_normal_map_tilts_the_shading_of_a_flat_face() {
    // A lit card facing the camera, its one texel tilting every normal
    // toward the key light, up and to the left, or away from it.
    let card = |texel: [u8; 4]| {
        let mut mesh = model_quad(None, AlphaMode::Opaque, false);
        mesh.material = Some(Material3D::default());
        let count = mesh.vertices.len();
        Arc::make_mut(mesh.surface.as_mut().unwrap()).normal_map = Some(gaanim_scene::NormalMap {
            texture: MeshTexture {
                image: ImageData {
                    data: Blob::from(texel.to_vec()),
                    format: ImageFormat::Rgba8,
                    alpha_type: ImageAlphaType::Alpha,
                    width: 1,
                    height: 1,
                },
                wrap: [TextureWrap::Repeat; 2],
                nearest: false,
            },
            scale: 1.0,
            tangents: vec![[1.0, 0.0, 0.0, 1.0]; count],
        });
        mesh
    };
    let shade = |texel| {
        draw_meshes(&[(card(texel), None)]).map(|(pixels, _)| {
            let [r, g, b, _] = pixel(&pixels, 32, 32);
            u32::from(r) + u32::from(g) + u32::from(b)
        })
    };
    let Some(flat) = shade([128, 128, 255, 255]) else {
        return;
    };
    let toward = shade([38, 128, 218, 255]).unwrap();
    let away = shade([218, 128, 218, 255]).unwrap();
    // The white card saturates toward the light, so it brightens less.
    assert!(
        toward > flat + 10 && flat > away + 40,
        "{toward} {flat} {away}"
    );
    // A flat texel shades as the card without a map.
    let mut plain = model_quad(None, AlphaMode::Opaque, false);
    plain.material = Some(Material3D::default());
    let (pixels, _) = draw_meshes(&[(plain, None)]).unwrap();
    let [r, g, b, _] = pixel(&pixels, 32, 32);
    let plain = u32::from(r) + u32::from(g) + u32::from(b);
    assert!(plain.abs_diff(flat) <= 3, "{plain} {flat}");
}

#[test]
fn a_skinned_lit_model_casts_its_posed_shadow() {
    // A lit floor and, a quarter unit above it, a lit card half a unit
    // wide on a joint. The key light casts its shadow 0.19 units right
    // and 0.38 down: the shadow follows where the joint takes the card.
    let mut floor = quad(peniko::Color::WHITE);
    floor.material = Some(Material3D::default());
    let mut card = model_quad(None, AlphaMode::Opaque, true);
    card.material = Some(Material3D::default());
    for vertex in &mut card.vertices {
        vertex[0] *= 0.25;
        vertex[1] *= 0.25;
        vertex[2] = 0.25;
    }
    let still = DMat4::IDENTITY.as_mat4().to_cols_array();
    let moved = DMat4::from_translation(DVec3::new(0.5, 0.0, 0.0))
        .as_mat4()
        .to_cols_array();
    let Some((rest, _)) = draw_meshes(&[
        (floor.clone(), None),
        (card.clone(), Some(Arc::from(vec![still]))),
    ]) else {
        return;
    };
    let (posed, _) = draw_meshes(&[(floor, None), (card, Some(Arc::from(vec![moved])))]).unwrap();
    // Pixel (33, 40) lies in the shadow of the card at rest, and in the
    // light once the joint moves it away.
    let shade = |pixels: &[u8]| pixel(pixels, 33, 40)[0];
    assert!(
        shade(&rest) + 40 < shade(&posed),
        "{} {}",
        shade(&rest),
        shade(&posed)
    );
    // Where the moved card lies, it is drawn.
    assert_eq!(pixel(&posed, 40, 32)[3], 255);
}

#[test]
fn a_filled_outline_draws_over_meshes_behind_it_and_under_those_in_front() {
    let Some(mut gpu) = crate::background::test_gpu::test_gpu() else {
        return;
    };
    let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
    camera.position = DVec3::new(0.0, 0.0, 10.0);
    let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
    let placement = camera.to_vello_transform();
    let opacity = GlobalOpacity(1.0);
    // A red square 2 units on a side at z = 0, 16 pixels per unit, its
    // top edge through the middle of pixel row 15.
    let fill = PlaneFillData {
        glyphs: vec![gaanim_scene::PlaneGlyph {
            outline: Arc::new(kurbo::Shape::to_path(
                &kurbo::Rect::new(-1.0, -1.0, 1.0, 1.03125),
                0.0,
            )),
            placement: kurbo::Affine::IDENTITY,
            color: peniko::Color::from_rgb8(255, 0, 0),
            gradient: None,
            clip: None,
            image: None,
        }],
    };
    let at_origin = GlobalSpatialTransform::default();
    // Blue cards over x from 0 to 1.5 in front, and from -1.5 to 0
    // behind.
    let blue = quad(peniko::Color::from_rgb8(0, 0, 255));
    let front = placed(0.75, DVec3::new(0.75, 0.0, 1.0));
    let back = placed(0.75, DVec3::new(-0.75, 0.0, -1.0));
    let mut sources = MeshSources::default();
    let mut meshes = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
    let card = |transform| Content {
        transform,
        opacity: &opacity,
        mesh: Some(&blue),
        lines: None,
        material: None,
        line_width: None,
        line_range: None,
        fill: None,
        joints: None,
        glow: None,
        shadow: None,
        blur: None,
        instances: None,
        morph: None,
        volume: None,
    };
    meshes.push(0, [(2, 1), (2, 1)], [None, None], &card(&front));
    meshes.push(0, [(3, 1), (3, 1)], [None, None], &card(&back));
    assert!(meshes.push_fill(
        0,
        (1, 1),
        None,
        &Content {
            transform: &at_origin,
            opacity: &opacity,
            mesh: None,
            lines: None,
            material: None,
            line_width: None,
            line_range: None,
            fill: Some(&fill),
            joints: None,
            glow: None,
            shadow: None,
            blur: None,
            instances: None,
            morph: None,
            volume: None,
        },
    ));
    let frames: Vec<GpuMeshFrame> = meshes
        .finish(
            &camera,
            |_| camera,
            ndc_to_canvas,
            line_width,
            &Illumination::default(),
        )
        .into_iter()
        .map(|(_, frame)| frame)
        .collect();
    // The outlines draw after the opaque meshes.
    assert!(frames[0].draws[2].source.fill.is_some());
    let mut scene = vello::Scene::new();
    scene.append(&frames[0].scene(), Some(placement));
    let mut three_d = GpuThreeD::default();
    three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames);
    let target = gpu.target(64, 64);
    gpu.render(&scene, &target);
    let pixels = gpu.read(&target);
    let pixel = |x: usize, y: usize| &pixels[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
    // Over the card behind it, the square; beside it, that card; in
    // front of it, the other card; beyond all, nothing.
    assert_eq!(pixel(24, 32), [255, 0, 0, 255]);
    assert_eq!(pixel(11, 32), [0, 0, 255, 255]);
    assert_eq!(pixel(40, 32), [0, 0, 255, 255]);
    assert_eq!(pixel(32, 8)[3], 0);
    // Its top edge: half covered, over nothing.
    let edge = pixel(32 - 4, 15);
    assert!((100..=156).contains(&edge[3]), "{edge:?}");
    assert_eq!(pixel(32 - 4, 17), [255, 0, 0, 255]);
}

#[test]
fn strokes_aligned_inside_and_outside_cover_their_ring() {
    let Some(mut gpu) = crate::background::test_gpu::test_gpu() else {
        return;
    };
    let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
    camera.position = DVec3::new(0.0, 0.0, 10.0);
    let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
    let placement = camera.to_vello_transform();
    let opacity = GlobalOpacity(1.0);
    let at_origin = GlobalSpatialTransform::default();
    // A circle of radius 1.2, stroked a quarter of a unit wide: 4 pixels.
    let circle = kurbo::Shape::to_path(&kurbo::Circle::new((0.1, -0.05), 1.2), 1e-3);
    let pen = kurbo::Stroke::new(0.25);
    let at_corner = |path: kurbo::BezPath| {
        let corner = kurbo::Shape::bounding_box(&path).origin().to_vec2();
        (Arc::new(kurbo::Affine::translate(-corner) * path), corner)
    };
    for align in [
        crate::effects::StrokeAlign::Inside,
        crate::effects::StrokeAlign::Outside,
    ] {
        let doubled = crate::pipeline::clipped_pen(&pen);
        let (outline, corner) = at_corner(kurbo::stroke(
            circle.iter(),
            &doubled,
            &kurbo::StrokeOpts::default(),
            1e-3,
        ));
        let (contour, contour_corner) = at_corner(circle.clone());
        let fill = PlaneFillData {
            glyphs: vec![gaanim_scene::PlaneGlyph {
                outline,
                placement: kurbo::Affine::translate(corner),
                color: peniko::Color::from_rgb8(255, 0, 0),
                gradient: None,
                clip: Some(gaanim_scene::PlaneClip {
                    outline: contour,
                    offset: corner - contour_corner,
                    outside: align == crate::effects::StrokeAlign::Outside,
                }),
                image: None,
            }],
        };
        let mut sources = MeshSources::default();
        let mut meshes = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
        assert!(meshes.push_fill(
            0,
            (1, 1),
            None,
            &Content {
                transform: &at_origin,
                opacity: &opacity,
                mesh: None,
                lines: None,
                material: None,
                line_width: None,
                line_range: None,
                fill: Some(&fill),
                joints: None,
                glow: None,
                shadow: None,
                blur: None,
                instances: None,
                morph: None,
                volume: None,
            },
        ));
        let frames: Vec<GpuMeshFrame> = meshes
            .finish(
                &camera,
                |_| camera,
                ndc_to_canvas,
                line_width,
                &Illumination::default(),
            )
            .into_iter()
            .map(|(_, frame)| frame)
            .collect();
        let mut scene = vello::Scene::new();
        scene.append(&frames[0].scene(), Some(placement));
        let mut three_d = GpuThreeD::default();
        three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames);
        let target = gpu.target(64, 64);
        gpu.render(&scene, &target);
        let drawn = gpu.read(&target);
        // Alpha against the exact area of the ring the stroke keeps,
        // within the circle or beyond it, by supersampling. (Vello's clip
        // layers soften these edges more; the area is the reference.)
        let (inner, outer) = match align {
            crate::effects::StrokeAlign::Inside => (0.95, 1.2),
            _ => (1.2, 1.45),
        };
        let exact = |x: usize, y: usize| {
            let mut inside = 0;
            for i in 0..8 {
                for j in 0..8 {
                    let px = -2.0 + (x as f64 + (f64::from(i) + 0.5) / 8.0) / 16.0;
                    let py = 2.0 - (y as f64 + (f64::from(j) + 0.5) / 8.0) / 16.0;
                    let r = (px - 0.1).hypot(py + 0.05);
                    inside += usize::from((inner..=outer).contains(&r));
                }
            }
            inside as f64 / 64.0 * 255.0
        };
        let (mut total, mut worst, mut count) = (0.0, 0.0_f64, 0);
        for y in 0..64 {
            for x in 0..64 {
                let truth = exact(x, y);
                let alpha = f64::from(drawn[(y * 64 + x) * 4 + 3]);
                if truth > 0.0 || alpha > 0.0 {
                    total += (alpha - truth).abs();
                    worst = worst.max((alpha - truth).abs());
                    count += 1;
                }
            }
        }
        let mean = total / f64::from(count);
        assert!(
            mean <= 5.0 && worst <= 40.0,
            "{align:?}: off by {mean:.1} on average, {worst:.0} at most"
        );
    }
}

#[test]
fn gradient_fills_match_their_2d_drawing() {
    let Some(mut gpu) = crate::background::test_gpu::test_gpu() else {
        return;
    };
    let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
    camera.position = DVec3::new(0.0, 0.0, 10.0);
    let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
    let placement = camera.to_vello_transform();
    let opacity = GlobalOpacity(1.0);
    let at_origin = GlobalSpatialTransform::default();
    // A square 3 units on a side, 16 pixels per unit: pixels 8 to 56.
    let square = kurbo::Shape::to_path(&kurbo::Rect::new(-1.5, -1.5, 1.5, 1.5), 0.0);
    let red = peniko::Color::from_rgb8(255, 0, 0);
    let blue = peniko::Color::from_rgb8(0, 0, 255);
    let white = peniko::Color::from_rgb8(255, 255, 255);
    let gradients = [
        peniko::Gradient::new_linear((-1.5, -1.0), (1.5, 1.0)).with_stops([red, blue]),
        peniko::Gradient::new_radial((0.2, 0.3), 1.2)
            .with_extend(peniko::Extend::Reflect)
            .with_stops([white, red, blue]),
        peniko::Gradient::new_sweep((0.0, 0.0), 0.5, 5.0).with_stops([red, white, blue]),
    ];
    for gradient in gradients {
        // The square laid at its corner, as in-world outlines are, with
        // its gradient mapped back to the square's units.
        let corner = kurbo::Vec2::new(-1.5, -1.5);
        let fill = PlaneFillData {
            glyphs: vec![gaanim_scene::PlaneGlyph {
                outline: Arc::new(kurbo::Affine::translate(-corner) * square.clone()),
                placement: kurbo::Affine::translate(corner),
                color: red,
                gradient: Some((Arc::new(gradient.clone()), kurbo::Affine::translate(corner))),
                clip: None,
                image: None,
            }],
        };
        let mut sources = MeshSources::default();
        let mut meshes = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
        assert!(meshes.push_fill(
            0,
            (1, 1),
            None,
            &Content {
                transform: &at_origin,
                opacity: &opacity,
                mesh: None,
                lines: None,
                material: None,
                line_width: None,
                line_range: None,
                fill: Some(&fill),
                joints: None,
                glow: None,
                shadow: None,
                blur: None,
                instances: None,
                morph: None,
                volume: None,
            },
        ));
        let frames: Vec<GpuMeshFrame> = meshes
            .finish(
                &camera,
                |_| camera,
                ndc_to_canvas,
                line_width,
                &Illumination::default(),
            )
            .into_iter()
            .map(|(_, frame)| frame)
            .collect();
        let mut scene = vello::Scene::new();
        scene.append(&frames[0].scene(), Some(placement));
        let mut three_d = GpuThreeD::default();
        three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames);
        let target = gpu.target(64, 64);
        gpu.render(&scene, &target);
        let drawn = gpu.read(&target);
        // Vello samples a gradient at a pixel's corner, the 3D pass at its
        // middle: half a pixel (a 32nd of a unit) up and left, the 2D
        // gradient is sampled where the 3D one is.
        let mut flat = vello::Scene::new();
        flat.fill(
            peniko::Fill::NonZero,
            placement,
            &peniko::Brush::Gradient(gradient.clone()),
            Some(kurbo::Affine::translate((-1.0 / 32.0, 1.0 / 32.0))),
            &square,
        );
        let target = gpu.target(64, 64);
        gpu.render(&flat, &target);
        let expected = gpu.read(&target);
        // Inside the square, clear of its edges.
        for y in 12..52 {
            for x in 12..52 {
                let at = (y * 64 + x) * 4;
                let worst = (0..4)
                    .map(|channel| drawn[at + channel].abs_diff(expected[at + channel]))
                    .max()
                    .unwrap();
                assert!(
                    worst <= 3,
                    "{:?} at ({x}, {y}): {:?} against {:?}",
                    gradient.kind,
                    &drawn[at..at + 4],
                    &expected[at..at + 4],
                );
            }
        }
    }
}

/// An outline filled with an image draws its texels where Vello draws
/// the image, the right way up, and nothing where the image is
/// transparent.
#[test]
fn image_fills_match_their_2d_drawing() {
    let Some(mut gpu) = crate::background::test_gpu::test_gpu() else {
        return;
    };
    let mut camera = Camera::ortho_2d_frame(4.0, 4.0, 64, 64);
    camera.position = DVec3::new(0.0, 0.0, 10.0);
    let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
    let placement = camera.to_vello_transform();
    let opacity = GlobalOpacity(1.0);
    let at_origin = GlobalSpatialTransform::default();
    // A square 3 units on a side, 16 pixels per unit: pixels 8 to 56,
    // filled with an image of 8 x 8 texels, 6 pixels each, its last
    // column transparent.
    let square = kurbo::Shape::to_path(&kurbo::Rect::new(-1.5, -1.5, 1.5, 1.5), 0.0);
    let texels: Vec<u8> = (0..64_u32)
        .flat_map(|at| {
            let (column, row) = (at % 8, at / 8);
            let alpha = if column == 7 { 0 } else { 255 };
            [(column * 32) as u8, (row * 32) as u8, 128, alpha]
        })
        .collect();
    let mut brush = peniko::ImageBrush::new(ImageData {
        data: Blob::from(texels),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width: 8,
        height: 8,
    });
    brush.sampler.quality = peniko::ImageQuality::Low;
    let to_local = kurbo::Affine::translate((-1.5, -1.5)) * kurbo::Affine::scale(3.0 / 8.0);
    let corner = kurbo::Vec2::new(-1.5, -1.5);
    let fill = PlaneFillData {
        glyphs: vec![gaanim_scene::PlaneGlyph {
            outline: Arc::new(kurbo::Affine::translate(-corner) * square.clone()),
            placement: kurbo::Affine::translate(corner),
            color: peniko::Color::WHITE,
            gradient: None,
            clip: None,
            image: Some(gaanim_scene::PlaneImage {
                brush: brush.clone(),
                to_pixels: to_local.inverse() * kurbo::Affine::translate(corner),
            }),
        }],
    };
    let mut sources = MeshSources::default();
    let mut meshes = GpuMeshes::new(&mut sources, (64, 64), placement, 0).unwrap();
    assert!(meshes.push_fill(
        0,
        (1, 1),
        None,
        &Content {
            transform: &at_origin,
            opacity: &opacity,
            mesh: None,
            lines: None,
            material: None,
            line_width: None,
            line_range: None,
            fill: Some(&fill),
            joints: None,
            glow: None,
            shadow: None,
            blur: None,
            instances: None,
            morph: None,
            volume: None,
        },
    ));
    let frames: Vec<GpuMeshFrame> = meshes
        .finish(
            &camera,
            |_| camera,
            ndc_to_canvas,
            line_width,
            &Illumination::default(),
        )
        .into_iter()
        .map(|(_, frame)| frame)
        .collect();
    let mut scene = vello::Scene::new();
    scene.append(&frames[0].scene(), Some(placement));
    let mut three_d = GpuThreeD::default();
    three_d.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &frames);
    let target = gpu.target(64, 64);
    gpu.render(&scene, &target);
    let drawn = gpu.read(&target);
    let mut flat = vello::Scene::new();
    flat.draw_image(&brush, placement * to_local);
    let target = gpu.target(64, 64);
    gpu.render(&flat, &target);
    let expected = gpu.read(&target);
    // The middle of each texel.
    for row in 0..8 {
        for column in 0..8 {
            let (x, y) = (8 + column * 6 + 3, 8 + row * 6 + 3);
            let at = (y * 64 + x) * 4;
            let worst = (0..4)
                .map(|channel| drawn[at + channel].abs_diff(expected[at + channel]))
                .max()
                .unwrap();
            assert!(
                worst <= 3,
                "texel ({column}, {row}): {:?} against {:?}",
                &drawn[at..at + 4],
                &expected[at..at + 4],
            );
        }
    }
}

fn wait(device: &wgpu::Device) {
    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });
}

/// Milliseconds a frame of `run` takes on the wall clock: the median of
/// batches of frames queued at once.
fn per_frame(device: &wgpu::Device, mut run: impl FnMut()) -> f64 {
    const BATCH: u32 = 40;
    run();
    wait(device);
    let mut batches: Vec<f64> = (0..7)
        .map(|_| {
            let start = std::time::Instant::now();
            for _ in 0..BATCH {
                run();
            }
            wait(device);
            start.elapsed().as_secs_f64() * 1000.0 / f64::from(BATCH)
        })
        .collect();
    batches.sort_by(f64::total_cmp);
    batches[batches.len() / 2]
}

/// The median milliseconds the GPU spends in `frame`'s color passes,
/// timed by `three_d.timing`.
fn gpu_ms(
    three_d: &mut GpuThreeD,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    frame: &GpuMeshFrame,
    view: (&wgpu::Texture, &wgpu::TextureView),
) -> f64 {
    const ROUNDS: u64 = 100;
    let buffer = |size, usage| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size,
            usage,
            mapped_at_creation: false,
        })
    };
    let resolved = buffer(
        16,
        wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
    );
    let read = buffer(
        16 * ROUNDS,
        wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
    );
    three_d.draw(device, queue, frame, view);
    for round in 0..ROUNDS {
        three_d.draw(device, queue, frame, view);
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.resolve_query_set(three_d.timing.as_ref().unwrap(), 0..2, &resolved, 0);
        encoder.copy_buffer_to_buffer(&resolved, 0, &read, round * 16, 16);
        queue.submit(Some(encoder.finish()));
    }
    read.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    wait(device);
    let period = f64::from(queue.get_timestamp_period());
    let mut times: Vec<f64> = read
        .slice(..)
        .get_mapped_range()
        .chunks_exact(16)
        .map(|pair| {
            let begin = u64::from_le_bytes(pair[..8].try_into().unwrap());
            let end = u64::from_le_bytes(pair[8..].try_into().unwrap());
            end.saturating_sub(begin) as f64 * period / 1e6
        })
        .collect();
    times.sort_by(f64::total_cmp);
    times[times.len() / 2]
}

/// Times 1,000 glyphs of New Computer Modern (the default text face, of
/// cubic outlines) filled from their curves by the 3D pass, against
/// Vello filling the same outlines, at three sizes on a 1920 x 1080
/// image, and prints what they upload. Run with `just test-package
/// gaanim_renderer --lib -- --ignored --nocapture bench_filled`.
#[test]
#[ignore]
fn bench_filled_outlines_of_a_thousand_glyphs() {
    struct Outline(kurbo::BezPath);
    impl ttf_parser::OutlineBuilder for Outline {
        fn move_to(&mut self, x: f32, y: f32) {
            self.0.move_to((f64::from(x), f64::from(y)));
        }
        fn line_to(&mut self, x: f32, y: f32) {
            self.0.line_to((f64::from(x), f64::from(y)));
        }
        fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
            self.0
                .quad_to((f64::from(x1), f64::from(y1)), (f64::from(x), f64::from(y)));
        }
        fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
            self.0.curve_to(
                (f64::from(x1), f64::from(y1)),
                (f64::from(x2), f64::from(y2)),
                (f64::from(x), f64::from(y)),
            );
        }
        fn close(&mut self) {
            self.0.close_path();
        }
    }
    let font = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/assets/fonts/NewCMMath-Regular.otf"
    ))
    .expect("the docs' math font");
    let face = ttf_parser::Face::parse(&font, 0).unwrap();
    let units = f64::from(face.units_per_em());
    let mut outlines: HashMap<char, Option<(Arc<kurbo::BezPath>, f64)>> = HashMap::new();
    let mut glyph = |c: char| {
        outlines
            .entry(c)
            .or_insert_with(|| {
                let id = face.glyph_index(c)?;
                let advance = f64::from(face.glyph_hor_advance(id)?);
                let mut outline = Outline(kurbo::BezPath::new());
                face.outline_glyph(id, &mut outline);
                Some((Arc::new(outline.0), advance))
            })
            .clone()
    };
    let text = "Sphinx of black quartz, judge my vow. The five boxing wizards jump quickly! ";
    // `count` glyphs in rows across the frame, `em` pixels tall.
    let mut layout = |em: f64, count: usize| {
        let scale = em / units;
        let (mut x, mut y) = (-940.0, 540.0 - em * 1.2);
        let mut glyphs = Vec::new();
        for c in text.chars().cycle() {
            if glyphs.len() == count {
                break;
            }
            let Some((outline, advance)) = glyph(c) else {
                continue;
            };
            if x + advance * scale > 940.0 {
                (x, y) = (-940.0, y - em * 1.3);
            }
            if !outline.elements().is_empty() {
                glyphs.push(gaanim_scene::PlaneGlyph {
                    outline,
                    placement: kurbo::Affine::translate((x, y)) * kurbo::Affine::scale(scale),
                    color: peniko::Color::from_rgb8(20, 20, 30),
                    gradient: None,
                    clip: None,
                    image: None,
                });
            }
            x += advance * scale;
        }
        PlaneFillData { glyphs }
    };

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all().with_env(),
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapters =
        pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all().with_env()));
    for adapter in adapters {
        let info = adapter.get_info();
        if info.device_type == wgpu::DeviceType::Cpu {
            continue;
        }
        println!("adapter: {} ({:?})", info.name, info.backend);
        let timestamps = wgpu::Features::TIMESTAMP_QUERY;
        if !adapter.features().contains(timestamps) {
            println!("  no timestamp queries");
            continue;
        }
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_features: timestamps,
            ..Default::default()
        }))
        .unwrap();
        let mut renderer = vello::Renderer::new(
            &device,
            vello::RendererOptions {
                use_cpu: false,
                antialiasing_support: vello::AaSupport::area_only(),
                num_init_threads: None,
                pipeline_cache: None,
            },
        )
        .unwrap();
        let (width, height) = (1920, 1080);
        let mut camera = Camera::ortho_2d_frame(1920.0, 1080.0, width, height);
        camera.position = DVec3::new(0.0, 0.0, 10.0);
        let (ndc_to_canvas, line_width) = canvas_projection(&camera).unwrap();
        let placement = camera.to_vello_transform();
        let opacity = GlobalOpacity(1.0);
        let at_origin = GlobalSpatialTransform::default();
        let vello_target = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let vello_view = vello_target.create_view(&Default::default());
        let vello_ms = |scene: &vello::Scene, renderer: &mut vello::Renderer| {
            per_frame(&device, || {
                renderer
                    .render_to_texture(
                        &device,
                        &queue,
                        scene,
                        &vello_view,
                        &vello::RenderParams {
                            base_color: peniko::Color::TRANSPARENT,
                            width,
                            height,
                            antialiasing_method: vello::AaConfig::Area,
                        },
                    )
                    .unwrap();
            })
        };
        // What a frame costs with nothing to draw but a tiny card.
        let card = quad(peniko::Color::WHITE);
        let speck = placed(0.5, DVec3::ZERO);
        let frame_of = |content: &Content<'_>, fill: bool| {
            let mut sources = MeshSources::default();
            let mut meshes = GpuMeshes::new(&mut sources, (width, height), placement, 0).unwrap();
            if fill {
                meshes.push_fill(0, (1, 1), None, content);
            } else {
                meshes.push(0, [(1, 1), (1, 1)], [None, None], content);
            }
            meshes
                .finish(
                    &camera,
                    |_| camera,
                    ndc_to_canvas,
                    line_width,
                    &Illumination::default(),
                )
                .remove(0)
                .1
        };
        let empty = frame_of(
            &Content {
                transform: &speck,
                opacity: &opacity,
                mesh: Some(&card),
                lines: None,
                material: None,
                line_width: None,
                line_range: None,
                fill: None,
                joints: None,
                glow: None,
                shadow: None,
                blur: None,
                instances: None,
                morph: None,
                volume: None,
            },
            false,
        );
        let mut three_d = GpuThreeD {
            timing: Some(device.create_query_set(&wgpu::QuerySetDescriptor {
                label: None,
                ty: wgpu::QueryType::Timestamp,
                count: 2,
            })),
            ..Default::default()
        };
        let target = Target::new(&device, &empty.image);
        let view = (&target.resolved, &target.resolved_view);
        let base_gpu = gpu_ms(&mut three_d, &device, &queue, &empty, view);
        let base_3d = per_frame(&device, || three_d.draw(&device, &queue, &empty, view));
        let base_vello = vello_ms(&vello::Scene::new(), &mut renderer);
        println!(
            "empty frame: 3D pass {base_gpu:.3} ms on the GPU, {base_3d:.3} ms wall; Vello {base_vello:.3} ms wall"
        );
        println!(
            "em px | glyphs | distinct | build ms | curves B | bands B | instances B | 3D GPU ms | 3D wall ms | Vello wall ms"
        );
        for (em, count) in [(12.0, 1000), (24.0, 1000), (36.0, 1000), (12.0, 10_000)] {
            let fill = layout(em, count);
            let start = std::time::Instant::now();
            let built = crate::plane_fill::build(&fill).unwrap();
            let build_ms = start.elapsed().as_secs_f64() * 1000.0;
            let distinct: HashSet<*const kurbo::BezPath> = fill
                .glyphs
                .iter()
                .map(|g| Arc::as_ptr(&g.outline))
                .collect();
            let frame = frame_of(
                &Content {
                    transform: &at_origin,
                    opacity: &opacity,
                    mesh: None,
                    lines: None,
                    material: None,
                    line_width: None,
                    line_range: None,
                    fill: Some(&fill),
                    joints: None,
                    glow: None,
                    shadow: None,
                    blur: None,
                    instances: None,
                    morph: None,
                    volume: None,
                },
                true,
            );
            let gpu = gpu_ms(&mut three_d, &device, &queue, &frame, view) - base_gpu;
            let wall = per_frame(&device, || three_d.draw(&device, &queue, &frame, view)) - base_3d;
            let mut scene = vello::Scene::new();
            for glyph in &fill.glyphs {
                scene.fill(
                    peniko::Fill::NonZero,
                    placement * glyph.placement,
                    glyph.color,
                    None,
                    &*glyph.outline,
                );
            }
            let vello = vello_ms(&scene, &mut renderer) - base_vello;
            println!(
                "{em} | {} | {} | {build_ms:.2} | {} | {} | {} | {gpu:.3} | {wall:.3} | {vello:.3}",
                built.count,
                distinct.len(),
                built.curves.len(),
                built.bands.len(),
                built.instances.len(),
            );
        }
    }
}
