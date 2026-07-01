use crate::effects::{ClipMask, DropShadow, GaussianBlur, Glow};
use bevy::prelude::*;
use gaanim_animation::FillDrawProgress;
use gaanim_core::ObjectId;
use gaanim_math::GlobalSpatialTransform;
use gaanim_scene::{
    FillBrush, GlobalOpacity, MobjectId, Path2D, RenderLayer, RenderOrder, StrokeBrush, Visible,
    WorldBounds, PathSource,
};
use std::collections::HashMap;
use std::sync::Arc;

// Explicit imports from bevy_vello instead of glob for clarity
use bevy_vello::integrations::scene::VelloScene2d;

/// Marker component identifying the single global Vello compositing entity.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct MainVelloScene;

/// Retained GPU cache of precompiled local Vello scenes for each Mobject.
///
/// Under the "Fragment Retain" system, local shapes, fills, and strokes are compiled into
/// standalone scene fragments and cached. They are only invalidated and rebuilt when
/// the Mobject's visual components (geometry or brushes) change.
#[derive(Resource, Default)]
pub struct GaanimRenderCache {
    pub fragment_cache: HashMap<ObjectId, Arc<vello::Scene>>,
}

pub struct ExtractedElement {
    transform: kurbo::Affine,
    opacity: f32,
    render_order: RenderOrder,
    scene: Arc<vello::Scene>,
    clip_mask: Option<ClipMask>,
}

fn stroke_clip_path<'a>(
    visible_path: &'a kurbo::BezPath,
    source_path: Option<&'a kurbo::BezPath>,
) -> Option<&'a kurbo::BezPath> {
    let clip_path = source_path.unwrap_or(visible_path);
    clip_path
        .elements()
        .contains(&kurbo::PathEl::ClosePath)
        .then_some(clip_path)
}

/// System: Synchronizes the `gaanim_math::Camera` resource to the active Bevy `Camera2d`.
///
/// This ensures that zoom, pan, and rotation configured on the gaanim camera are reflected
/// in the actual rendered output, since `bevy_vello` relies on the Bevy camera for projection.
pub fn sync_gaanim_camera_to_bevy_system(
    gaanim_camera: Option<Res<gaanim_math::Camera>>,
    mut bevy_cameras: Query<(&mut Transform, &mut Projection), With<Camera2d>>,
) {
    let Some(cam) = gaanim_camera else { return };
    for (mut transform, mut projection) in &mut bevy_cameras {
        // Position — incorporate viewport_offset_y so the scene shifts up
        // when the timeline panel is visible (offset is in screen pixels).
        let effective_zoom = match cam.projection {
            gaanim_math::Projection::Orthographic { zoom } => zoom * cam.viewport_scale,
            _ => 1.0,
        };
        let offset_world_y = if effective_zoom > 0.0 {
            cam.viewport_offset_y / effective_zoom
        } else {
            0.0
        };
        transform.translation.x = cam.position.x as f32;
        transform.translation.y = (cam.position.y - offset_world_y) as f32;
        // Rotation (2D Z-axis only) — compute directly from quaternion
        // to avoid the three-trig overhead of full Euler decomposition.
        let z_angle = 2.0 * f64::atan2(cam.rotation.z, cam.rotation.w);
        transform.rotation = Quat::from_rotation_z(-z_angle as f32);

        // Projection: only Orthographic is supported for 2D Vello
        if let gaanim_math::Projection::Orthographic { zoom } = cam.projection {
            // Apply viewport_scale so the scene maintains its aspect ratio
            // when the window dimensions differ from the scene's native resolution.
            let effective = zoom * cam.viewport_scale;
            let scale = if effective > 0.0 {
                1.0 / effective as f32
            } else {
                1.0
            };
            if let Projection::Orthographic(ortho) = projection.as_mut() {
                ortho.scale = scale;
            }
        }
    }
}

/// System: Cleans up stale fragment cache entries when Mobject entities are destroyed.
///
/// Run this system before `gaanim_render_system` so the cache stays consistent.
pub fn gaanim_render_cache_sweep_system(
    mut cache: ResMut<GaanimRenderCache>,
    mut removed: RemovedComponents<MobjectId>,
    query_mobj_ids: Query<&MobjectId>,
) {
    // Only sweep when entities with MobjectId were actually removed.
    // On static frames this is a no-op (zero cost).
    if removed.read().next().is_none() {
        return;
    }
    let active: std::collections::HashSet<ObjectId> = query_mobj_ids.iter().map(|m| m.0).collect();
    cache.fragment_cache.retain(|id, _| active.contains(id));
}

/// Standalone function: extracts all visible Vello2D mobjects from a Bevy World
/// and compiles them into a single composited `vello::Scene`.
///
/// This bypasses Bevy's render graph and does NOT require `VelloScene2d`, `Commands`,
/// or any window/render plugins. It rebuilds all fragments every call (no caching),
/// making it suitable for headless export where every frame is different.
///
/// # Coordinate System
/// The returned `vello::Scene` is in **gaanim world space** (Y-up, origin at
/// centre).  When rendering directly with `vello::Renderer::render_to_texture`,
/// apply a camera transform that maps world coordinates to the output viewport.
/// The export path in `gaanim_export` already handles this correctly.
pub fn compile_scene_from_world(
    world: &mut World,
    camera: Option<&gaanim_math::Camera>,
) -> vello::Scene {
    let mut extracted = Vec::new();
    let mut culled_entities = std::collections::HashSet::new();

    let cam_bounds = camera.and_then(|cam| {
        if let gaanim_math::Projection::Orthographic { zoom } = cam.projection {
            let effective = zoom * cam.viewport_scale;
            let hw = (cam.viewport_width as f64) / (2.0 * effective);
            let hh = (cam.viewport_height as f64) / (2.0 * effective);
            let margin = 100.0 / effective.max(0.1);
            Some(gaanim_math::Bounds3D::new_2d(
                cam.position.x - hw - margin,
                cam.position.y - hh - margin,
                cam.position.x + hw + margin,
                cam.position.y + hh + margin,
            ))
        } else {
            None
        }
    });

    let mut query_mobjects = world.query_filtered::<(
        Entity,
        &MobjectId,
        &GlobalSpatialTransform,
        &GlobalOpacity,
        &RenderOrder,
        &RenderLayer,
        Option<&Path2D>,
        Option<&PathSource>,
        Option<&FillBrush>,
        Option<&StrokeBrush>,
        Option<&gaanim_scene::ObjectTag>,
    ), With<Visible>>();

    let mut query_effects = world.query::<(
        Option<&DropShadow>,
        Option<&FillDrawProgress>,
        Option<&ClipMask>,
        Option<&WorldBounds>,
        Option<&gaanim_scene::GroupMarker>,
    )>();

    let mut child_query = world.query::<&ChildOf>();

    for (
        entity,
        _mobj_id,
        transform,
        global_opacity,
        render_order,
        render_layer,
        path_opt,
        path_source_opt,
        fill_opt,
        stroke_opt,
        _tag,
    ) in query_mobjects.iter(world)
    {
        if *render_layer != RenderLayer::Vello2D {
            continue;
        }

        let Ok((shadow_opt, fill_progress_opt, clip_opt, world_bounds_opt, is_group_opt)) =
            query_effects.get(world, entity)
        else {
            continue;
        };

        if is_group_opt.is_some() {
            continue;
        }

        if let Some(ref bounds) = cam_bounds {
            let mut is_ancestor_culled = false;
            let mut current = entity;
            while let Ok(child_of) = child_query.get(world, current) {
                let parent = child_of.parent();
                if culled_entities.contains(&parent) {
                    is_ancestor_culled = true;
                    break;
                }
                current = parent;
            }

            if is_ancestor_culled {
                culled_entities.insert(entity);
                continue;
            }

            if let Some(w_bounds) = world_bounds_opt
                && !bounds.intersects(&w_bounds.0)
            {
                culled_entities.insert(entity);
                continue;
            }
        }

        let fill_alpha = fill_progress_opt
            .map(|f| f.0.clamp(0.0, 1.0))
            .unwrap_or(1.0);

        let mut scene = vello::Scene::new();
        let empty_bez = kurbo::BezPath::new();
        let elem_path = path_opt.map(|p| p.0.as_ref()).unwrap_or(&empty_bez);
        let source_path = path_source_opt.map(|p| p.0.as_ref());
        let elem_fill = fill_opt.and_then(|f| f.0.as_ref());
        let elem_stroke = stroke_opt.and_then(|s| s.brush.as_ref());
        let elem_stroke_style = stroke_opt.map(|s| &s.style);

        if let Some(shadow) = shadow_opt {
            let shadow_transform = kurbo::Affine::translate((shadow.offset.x, shadow.offset.y));
            let shadow_brush = peniko::Brush::Solid(shadow.color);
            scene.fill(
                peniko::Fill::NonZero,
                shadow_transform,
                &shadow_brush,
                None,
                elem_path,
            );
        }

        if fill_alpha < 1.0 {
            if let Some(fill_brush) = elem_fill {
                let modulated = modulate_brush_alpha(fill_brush, fill_alpha);
                if let Some(ref brush) = modulated {
                    scene.fill(
                        peniko::Fill::NonZero,
                        kurbo::Affine::IDENTITY,
                        brush,
                        None,
                        elem_path,
                    );
                }
            }
        } else if let Some(fill_brush) = elem_fill {
            scene.fill(
                peniko::Fill::NonZero,
                kurbo::Affine::IDENTITY,
                fill_brush,
                None,
                elem_path,
            );
        }

        if let Some(stroke_brush) = elem_stroke
            && let Some(style) = elem_stroke_style
        {
            if let Some(clip_path) = stroke_clip_path(elem_path, source_path) {
                scene.push_layer(
                    peniko::Fill::NonZero,
                    peniko::BlendMode::default(),
                    1.0,
                    kurbo::Affine::IDENTITY,
                    clip_path,
                );
                scene.stroke(
                    style,
                    kurbo::Affine::IDENTITY,
                    stroke_brush,
                    None,
                    elem_path,
                );
                scene.pop_layer();
            } else {
                scene.stroke(
                    style,
                    kurbo::Affine::IDENTITY,
                    stroke_brush,
                    None,
                    elem_path,
                );
            }
        }

        extracted.push(ExtractedElement {
            transform: transform.affine_2d,
            opacity: global_opacity.0,
            render_order: *render_order,
            scene: Arc::new(scene),
            clip_mask: clip_opt.cloned(),
        });
    }

    extracted.sort_by(
        |a, b| match a.render_order.z_index.cmp(&b.render_order.z_index) {
            std::cmp::Ordering::Equal => a
                .render_order
                .creation_order
                .cmp(&b.render_order.creation_order),
            other => other,
        },
    );

    let mut main_scene = vello::Scene::new();

    for elem in extracted {
        let mut layers_to_pop = 0;

        if let Some(clip) = &elem.clip_mask {
            main_scene.push_layer(
                clip.rule,
                peniko::BlendMode::default(),
                1.0,
                elem.transform,
                &clip.path,
            );
            layers_to_pop += 1;
        }

        if elem.opacity < 1.0 {
            main_scene.push_layer(
                peniko::Fill::NonZero,
                peniko::BlendMode::default(),
                elem.opacity,
                kurbo::Affine::IDENTITY,
                &kurbo::Rect::new(-1e9, -1e9, 1e9, 1e9),
            );
            layers_to_pop += 1;
        }

        main_scene.append(&elem.scene, Some(elem.transform));

        for _ in 0..layers_to_pop {
            main_scene.pop_layer();
        }
    }

    main_scene
}

/// System: Extracts, composites, and renders all visible gaanim 2D Mobjects.
///
/// 1. Queries all active Mobjects marked for Vello2D rendering that are visible.
/// 2. Performs change detection to invalidate changed local vector fragments in the retained cache.
/// 3. Compiles/caches local geometry (including drop shadows, fills, and strokes).
/// 4. Sorts Mobjects deterministically based on Z-index + creation order to prevent visual jitter.
/// 5. Composites all fragments into a single main scene using global transforms and opacities.
/// 6. Assigns/updates the single global `VelloScene2d` entity marked with `MainVelloScene`.
///
/// # Coordinate System
/// gaanim uses a Y-up coordinate system (positive Y = up on screen).
/// Scene elements remain entirely in gaanim's Y-up world space. The single
/// global `MainVelloScene` entity has a negative Y scale that converts the
/// completed scene to Vello's Y-down pixel space without changing the meaning
/// of `.at(x, y)` or requiring per-object coordinate workarounds.
pub fn gaanim_render_system(
    mut commands: Commands,
    mut cache: ResMut<GaanimRenderCache>,
    gaanim_camera: Option<Res<gaanim_math::Camera>>,
    child_query: Query<&ChildOf>,
    query_mobjects: Query<
        (
            Entity,
            Ref<MobjectId>,
            Ref<GlobalSpatialTransform>,
            Ref<GlobalOpacity>,
            Ref<RenderOrder>,
            Ref<RenderLayer>,
            Option<Ref<Path2D>>,
            Option<Ref<PathSource>>,
            Option<Ref<FillBrush>>,
            Option<Ref<StrokeBrush>>,
            Option<&gaanim_scene::ObjectTag>,
        ),
        With<Visible>,
    >,
    query_effects: Query<(
        Option<Ref<DropShadow>>,
        Option<Ref<Glow>>,
        Option<Ref<GaussianBlur>>,
        Option<Ref<ClipMask>>,
        Option<Ref<FillDrawProgress>>,
        Option<&WorldBounds>,
        Option<&gaanim_scene::GroupMarker>,
    )>,
    mut query_vello_scene: Query<
        (Entity, &mut VelloScene2d, &mut Transform),
        With<MainVelloScene>,
    >,
    mut local_extracted: Local<Vec<ExtractedElement>>,
    mut local_culled: Local<std::collections::HashSet<Entity>>,
) {
    local_extracted.clear();
    let mut scene_aabb_min = Vec3::splat(f32::INFINITY);
    let mut scene_aabb_max = Vec3::splat(f32::NEG_INFINITY);

    local_culled.clear();

    // 1. Calculate orthographic camera bounds for culling
    let cam_bounds = gaanim_camera.as_ref().and_then(|cam| {
        if let gaanim_math::Projection::Orthographic { zoom } = cam.projection {
            let effective = zoom * cam.viewport_scale;
            let hw = (cam.viewport_width as f64) / (2.0 * effective);
            let hh = (cam.viewport_height as f64) / (2.0 * effective);
            // Generous margin to avoid popping at boundaries
            let margin = 100.0 / effective.max(0.1);
            Some(gaanim_math::Bounds3D::new_2d(
                cam.position.x - hw - margin,
                cam.position.y - hh - margin,
                cam.position.x + hw + margin,
                cam.position.y + hh + margin,
            ))
        } else {
            None
        }
    });

    for (
        entity,
        mobj_id,
        transform,
        global_opacity,
        render_order,
        render_layer,
        path_ref,
        path_source_ref,
        fill_ref,
        stroke_ref,
        _tag,
    ) in &query_mobjects
    {
        // Look up effects, bounds and group marker components on-demand to keep Query tuple size small
        let (
            shadow_ref,
            glow_ref,
            blur_ref,
            clip_ref,
            fill_progress_ref,
            world_bounds_opt,
            is_group_opt,
        ) = query_effects
            .get(entity)
            .unwrap_or((None, None, None, None, None, None, None));

        // 2. Perform camera frustum culling and hierarchical culling propagation
        if let Some(bounds) = cam_bounds {
            // Check if any ancestor of this entity is already culled
            let mut is_ancestor_culled = false;
            let mut current = entity;
            while let Ok(child_of) = child_query.get(current) {
                let parent = child_of.parent();
                if local_culled.contains(&parent) {
                    is_ancestor_culled = true;
                    break;
                }
                current = parent;
            }

            if is_ancestor_culled {
                local_culled.insert(entity);
                continue;
            }

            // Check if this entity's own bounds are out of camera bounds
            if let Some(w_bounds) = world_bounds_opt
                && !bounds.intersects(&w_bounds.0)
            {
                local_culled.insert(entity);
                continue;
            }
        }

        // Only process visible Vello2D elements
        if *render_layer != RenderLayer::Vello2D {
            continue;
        }

        // Groups do not draw visual geometry directly, they only act as spatial nodes
        if is_group_opt.is_some() {
            continue;
        }

        // Fragment Invalidation Check: Only visual components trigger rebuild.
        let path_changed = path_ref.as_ref().is_some_and(|r| r.is_changed());
        let changed = path_changed
            || path_source_ref.as_ref().is_some_and(|r| r.is_changed())
            || fill_ref.as_ref().is_some_and(|r| r.is_changed())
            || stroke_ref.as_ref().is_some_and(|r| r.is_changed())
            || shadow_ref.as_ref().is_some_and(|r| r.is_changed())
            || glow_ref.as_ref().is_some_and(|r| r.is_changed())
            || blur_ref.as_ref().is_some_and(|r| r.is_changed())
            || clip_ref.as_ref().is_some_and(|r| r.is_changed())
            || fill_progress_ref.as_ref().is_some_and(|r| r.is_changed());

        if changed {
            cache.fragment_cache.remove(&mobj_id.0);
        }

        // Read the current fill progress. If the component is absent
        // (the common case for non-Writing entities) the fill is
        // rendered at full opacity, preserving legacy behavior.
        let fill_alpha = fill_progress_ref
            .as_ref()
            .map(|f| f.0.clamp(0.0, 1.0))
            .unwrap_or(1.0);

        // Fragment Retain: Retrieve the cached scene or compile a new local vector fragment.
        // Geometry data is borrowed from the Ref-components instead of cloned,
        // avoiding redundant allocations when only some visual components change.
        let empty_bez = kurbo::BezPath::new();
        let fragment = cache.fragment_cache.entry(mobj_id.0).or_insert_with(|| {
            let mut scene = vello::Scene::new();

            let elem_path = path_ref
                .as_ref()
                .map(|p| p.0.as_ref())
                .unwrap_or(&empty_bez);
            let source_path = path_source_ref.as_ref().map(|p| p.0.as_ref());
            let elem_fill = fill_ref.as_ref().and_then(|f| f.0.as_ref());
            let elem_stroke = stroke_ref.as_ref().and_then(|s| s.brush.as_ref());
            let elem_stroke_style = stroke_ref.as_ref().map(|s| &s.style);
            let elem_shadow = shadow_ref.as_deref();

            // 1. Draw Drop Shadow (rendered under the geometry with custom translation offset)
            if let Some(shadow) = elem_shadow {
                let shadow_transform = kurbo::Affine::translate((shadow.offset.x, shadow.offset.y));
                let shadow_brush = peniko::Brush::Solid(shadow.color);
                scene.fill(
                    peniko::Fill::NonZero,
                    shadow_transform,
                    &shadow_brush,
                    None,
                    elem_path,
                );
            }

            // 2. Draw Fill
            //
            // When the entity is mid-Write, the cached fill_alpha is < 1.0
            // and we modulate the brush's color alpha accordingly. This
            // only works cleanly for `Brush::Solid`; for gradient/image
            // brushes we still draw at full alpha (a small visual quirk
            // during the cross-fade, but the user-visible effect is
            // preserved: outline first, then fill).
            if fill_alpha < 1.0 {
                if let Some(fill_brush) = elem_fill {
                    let modulated = modulate_brush_alpha(fill_brush, fill_alpha);
                    if let Some(ref brush) = modulated {
                        scene.fill(
                            peniko::Fill::NonZero,
                            kurbo::Affine::IDENTITY,
                            brush,
                            None,
                            elem_path,
                        );
                    }
                }
            } else if let Some(fill_brush) = elem_fill {
                scene.fill(
                    peniko::Fill::NonZero,
                    kurbo::Affine::IDENTITY,
                    fill_brush,
                    None,
                    elem_path,
                );
            }

            // 3. Draw Stroke. We clip the stroke to the path region
            // so the outline is rendered as an "inner border" rather
            // than straddling the contour — this matches Manim's
            // `stroke_behind_fill=False` default and prevents the
            // glyphs from looking artificially thick.
            //
            // The clip only makes sense for **closed contours**
            // (text glyphs, filled shapes). Open paths like the
            // Typst `frac` horizontal rule are 1D curves with zero
            // fillable area; pushing a layer with `Fill::NonZero`
            // over them clips the stroke to nothing and the line
            // becomes invisible. So we only push the layer when the
            // path actually contains a `ClosePath` element.
            if let Some(stroke_brush) = elem_stroke
                && let Some(style) = elem_stroke_style
            {
                if let Some(clip_path) = stroke_clip_path(elem_path, source_path) {
                    scene.push_layer(
                        peniko::Fill::NonZero,
                        peniko::BlendMode::default(),
                        1.0,
                        kurbo::Affine::IDENTITY,
                        clip_path,
                    );
                    scene.stroke(
                        style,
                        kurbo::Affine::IDENTITY,
                        stroke_brush,
                        None,
                        elem_path,
                    );
                    scene.pop_layer();
                } else {
                    scene.stroke(
                        style,
                        kurbo::Affine::IDENTITY,
                        stroke_brush,
                        None,
                        elem_path,
                    );
                }
            }

            // TODO: GaussianBlur and Glow effects are not yet implemented
            // in Vello's native pipeline. They are read above to invalidate cache,
            // but do not affect the scene until a post-processing pass is wired.
            let _ = (glow_ref, blur_ref);

            Arc::new(scene)
        });

        local_extracted.push(ExtractedElement {
            transform: transform.affine_2d,
            opacity: global_opacity.0,
            render_order: *render_order,
            scene: Arc::clone(fragment),
            clip_mask: clip_ref.as_ref().map(|c| (*c).clone()),
        });

        // Accumulate AABB from WorldBounds if available, otherwise approximate from transform
        if let Some(bounds) = world_bounds_opt {
            let min = Vec3::new(bounds.0.min.x as f32, bounds.0.min.y as f32, 0.0);
            let max = Vec3::new(bounds.0.max.x as f32, bounds.0.max.y as f32, 0.0);
            scene_aabb_min = scene_aabb_min.min(min);
            scene_aabb_max = scene_aabb_max.max(max);
        } else {
            // Fallback: use the translation component of the affine 2D transform as a point
            // estimate with a generous margin. The 4x4 mat4 field is only available under the
            // `dim3` feature; for the 2D-only path we extract tx/ty from the affine coefficients.
            let coeffs = transform.affine_2d.as_coeffs();
            let center = Vec3::new(coeffs[4] as f32, coeffs[5] as f32, 0.0);
            let margin = Vec3::new(500.0, 500.0, 1.0);
            scene_aabb_min = scene_aabb_min.min(center - margin);
            scene_aabb_max = scene_aabb_max.max(center + margin);
        }
    }

    // Sort elements deterministically by RenderOrder to ensure correct layering
    local_extracted.sort_by(
        |a, b| match a.render_order.z_index.cmp(&b.render_order.z_index) {
            std::cmp::Ordering::Equal => a
                .render_order
                .creation_order
                .cmp(&b.render_order.creation_order),
            other => other,
        },
    );

    // Assemble the global composited Scene in Bevy world coordinates
    let mut main_scene = vello::Scene::new();

    for elem in local_extracted.drain(..) {
        let mut layers_to_pop = 0;

        if let Some(clip) = &elem.clip_mask {
            main_scene.push_layer(
                clip.rule,
                peniko::BlendMode::default(),
                1.0,
                elem.transform,
                &clip.path,
            );
            layers_to_pop += 1;
        }

        if elem.opacity < 1.0 {
            // Apply opacity layer with composite transform
            main_scene.push_layer(
                peniko::Fill::NonZero,
                peniko::BlendMode::default(),
                elem.opacity,
                kurbo::Affine::IDENTITY,
                &kurbo::Rect::new(-1e9, -1e9, 1e9, 1e9),
            );
            layers_to_pop += 1;
        }

        main_scene.append(&elem.scene, Some(elem.transform));

        for _ in 0..layers_to_pop {
            main_scene.pop_layer();
        }
    }

    // Compute a sensible AABB for the VelloScene2d entity.
    // If the scene is empty, use a default viewport-sized bounds to avoid zero-size culling.
    let (aabb_min, aabb_max) = if scene_aabb_min.x.is_finite() {
        (scene_aabb_min, scene_aabb_max)
    } else {
        let default = Vec3::new(640.0, 360.0, 0.0);
        (default * -1.0, default)
    };

    // Update the single global VelloScene2d or spawn it on demand
    let mut scene_entity_found = false;
    for (entity, mut scene, mut scene_transform) in &mut query_vello_scene {
        scene.reset();
        scene.append(&main_scene, None);
        scene_transform.scale = Vec3::new(1.0, -1.0, 1.0);
        scene_entity_found = true;

        // Update AABB to match current scene bounds
        commands
            .entity(entity)
            .insert(bevy::camera::primitives::Aabb::from_min_max(
                aabb_min, aabb_max,
            ));
    }

    if !scene_entity_found {
        commands.spawn((
            MainVelloScene,
            VelloScene2d::from(main_scene),
            bevy::camera::primitives::Aabb::from_min_max(aabb_min, aabb_max),
            Transform::from_scale(Vec3::new(1.0, -1.0, 1.0)),
            GlobalTransform::default(),
            Visibility::default(),
            InheritedVisibility::default(),
            ViewVisibility::default(),
        ));
    }
}

/// Returns a new `Brush` whose color alpha has been multiplied by `alpha`.
///
/// Used by the Write animation's cross-fade phase to fade the fill in
/// from `FillDrawProgress = 0.0` (invisible) to `1.0` (fully visible)
/// without changing the underlying brush. Delegates to `peniko::Brush`'s
/// built-in `multiply_alpha`, which handles `Solid`, `Gradient`, and
/// `Image` brush variants uniformly (with overflow saturation).
fn modulate_brush_alpha(brush: &peniko::Brush, alpha: f32) -> Option<peniko::Brush> {
    if alpha >= 1.0 {
        return None; // caller can use the original brush unmodified
    }
    let alpha = alpha.max(0.0);
    Some(brush.clone().multiply_alpha(alpha))
}
