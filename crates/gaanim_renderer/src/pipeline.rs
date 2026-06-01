use crate::effects::{ClipMask, DropShadow, GaussianBlur, Glow};
use bevy::prelude::*;
use gaanim_animation::FillDrawProgress;
use gaanim_core::ObjectId;
use gaanim_math::GlobalSpatialTransform;
use gaanim_scene::{
    FillBrush, GlobalOpacity, MobjectId, Path2D, RenderLayer, RenderOrder, StrokeBrush, Visible,
    WorldBounds,
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

struct ExtractedElement {
    transform: kurbo::Affine,
    opacity: f32,
    render_order: RenderOrder,
    scene: Arc<vello::Scene>,
    clip_mask: Option<ClipMask>,
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
        // Position
        transform.translation.x = cam.position.x as f32;
        transform.translation.y = cam.position.y as f32;
        // Rotation (2D Z-axis only) — compute directly from quaternion
        // to avoid the three-trig overhead of full Euler decomposition.
        let z_angle = 2.0 * f64::atan2(cam.rotation.z, cam.rotation.w);
        transform.rotation = Quat::from_rotation_z(-z_angle as f32);

        // Projection: only Orthographic is supported for 2D Vello
        if let gaanim_math::Projection::Orthographic { zoom } = cam.projection {
            let scale = 1.0 / zoom as f32;
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
    query_mobj_ids: Query<&MobjectId>,
) {
    let active: std::collections::HashSet<ObjectId> = query_mobj_ids.iter().map(|m| m.0).collect();
    cache.fragment_cache.retain(|id, _| active.contains(id));
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
/// Coordinates inside the `vello::Scene` are in **Bevy world space** (Y-up, origin at the center
/// of the viewport when using `Camera2d`). `bevy_vello` handles the conversion to Vello's native
/// Y-down pixel space automatically during its render pass.
pub fn gaanim_render_system(
    mut commands: Commands,
    mut cache: ResMut<GaanimRenderCache>,
    query_mobjects: Query<
        (
            Ref<MobjectId>,
            Ref<GlobalSpatialTransform>,
            Ref<GlobalOpacity>,
            Ref<RenderOrder>,
            Ref<RenderLayer>,
            Option<Ref<Path2D>>,
            Option<Ref<FillBrush>>,
            Option<Ref<StrokeBrush>>,
            Option<Ref<DropShadow>>,
            Option<Ref<Glow>>,
            Option<Ref<GaussianBlur>>,
            Option<Ref<ClipMask>>,
            Option<Ref<FillDrawProgress>>,
            Option<&WorldBounds>,
        ),
        With<Visible>,
    >,
    mut query_vello_scene: Query<(Entity, &mut VelloScene2d), With<MainVelloScene>>,
) {
    let mut extracted = Vec::new();
    let mut scene_aabb_min = Vec3::splat(f32::INFINITY);
    let mut scene_aabb_max = Vec3::splat(f32::NEG_INFINITY);

    for (
        mobj_id,
        transform,
        global_opacity,
        render_order,
        render_layer,
        path_ref,
        fill_ref,
        stroke_ref,
        shadow_ref,
        glow_ref,
        blur_ref,
        clip_ref,
        fill_progress_ref,
        world_bounds_opt,
    ) in &query_mobjects
    {
        // Only process visible Vello2D elements
        if *render_layer != RenderLayer::Vello2D {
            continue;
        }

        // Fragment Invalidation Check: Only visual components trigger rebuild.
        let path_changed = path_ref.as_ref().is_some_and(|r| r.is_changed());
        let changed = path_changed
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
        // Clones of geometry data only happen inside the closure when a rebuild is necessary.
        let fragment = cache.fragment_cache.entry(mobj_id.0).or_insert_with(|| {
            let mut scene = vello::Scene::new();

            let elem_path = path_ref.as_ref().map(|p| p.0.clone()).unwrap_or_default();
            let elem_fill = fill_ref.as_ref().and_then(|f| f.0.clone());
            let elem_stroke = stroke_ref.as_ref().and_then(|s| s.brush.clone());
            let elem_stroke_style = stroke_ref.as_ref().map(|s| s.style.clone());
            let elem_shadow = shadow_ref.as_ref().map(|s| (*s).clone());

            // 1. Draw Drop Shadow (rendered under the geometry with custom translation offset)
            if let Some(ref shadow) = elem_shadow {
                let shadow_transform = kurbo::Affine::translate((shadow.offset.x, shadow.offset.y));
                let shadow_brush = peniko::Brush::Solid(shadow.color);
                scene.fill(
                    peniko::Fill::NonZero,
                    shadow_transform,
                    &shadow_brush,
                    None,
                    &elem_path,
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
                if let Some(ref fill_brush) = elem_fill {
                    let modulated = modulate_brush_alpha(fill_brush, fill_alpha);
                    if let Some(brush) = modulated {
                        scene.fill(
                            peniko::Fill::NonZero,
                            kurbo::Affine::IDENTITY,
                            &brush,
                            None,
                            &elem_path,
                        );
                    }
                }
            } else if let Some(ref fill_brush) = elem_fill {
                scene.fill(
                    peniko::Fill::NonZero,
                    kurbo::Affine::IDENTITY,
                    fill_brush,
                    None,
                    &elem_path,
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
            if let Some(ref stroke_brush) = elem_stroke
                && let Some(ref style) = elem_stroke_style
            {
                let has_closed_contour = elem_path
                    .elements()
                    .iter()
                    .any(|&el| el == kurbo::PathEl::ClosePath);
                if has_closed_contour {
                    scene.push_layer(
                        peniko::Fill::NonZero,
                        peniko::BlendMode::default(),
                        1.0,
                        kurbo::Affine::IDENTITY,
                        &elem_path,
                    );
                    scene.stroke(
                        style,
                        kurbo::Affine::IDENTITY,
                        stroke_brush,
                        None,
                        &elem_path,
                    );
                    scene.pop_layer();
                } else {
                    scene.stroke(
                        style,
                        kurbo::Affine::IDENTITY,
                        stroke_brush,
                        None,
                        &elem_path,
                    );
                }
            }

            // TODO: GaussianBlur and Glow effects are not yet implemented
            // in Vello's native pipeline. They are read above to invalidate cache,
            // but do not affect the scene until a post-processing pass is wired.
            let _ = (glow_ref, blur_ref);

            Arc::new(scene)
        });

        extracted.push(ExtractedElement {
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
            // Fallback: use the translation component of the affine transform as a point estimate
            // with a generous margin. This avoids the ±1e6 hardcoded hack while still preventing
            // frustum culling for typical scene sizes.
            let (_, _, translation) = transform.mat4.to_scale_rotation_translation();
            let center = Vec3::new(translation.x as f32, translation.y as f32, 0.0);
            let margin = Vec3::new(500.0, 500.0, 1.0);
            scene_aabb_min = scene_aabb_min.min(center - margin);
            scene_aabb_max = scene_aabb_max.max(center + margin);
        }
    }

    // Sort elements deterministically by RenderOrder to ensure correct layering
    extracted.sort_by(
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

        main_scene.append(&*elem.scene, Some(elem.transform));

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
    for (entity, mut scene) in &mut query_vello_scene {
        scene.reset();
        scene.append(&main_scene, None);
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
            Transform::default(),
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
