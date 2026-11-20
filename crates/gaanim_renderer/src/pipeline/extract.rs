//! The extraction both renderers share: the queries that turn the world's
//! drawables into [`ExtractedElement`]s and the loop that builds them. The
//! interactive preview runs it from [`super::gaanim_render_system`] and the
//! headless path (exports, bundles, captures) from [`super::extract_world`],
//! each with its own fragment cache behind [`FragmentSource`], so both draw a
//! drawable from the same values.

use super::*;
use gaanim_scene::prelude::{QueryItem, SystemParam};

/// What both extractions read of a drawable. What a fragment is built from
/// is read with its change tick, which the preview trusts to skip comparing
/// recipes of drawables that did not change (see [`FragmentRequest::changed`]).
pub(super) type MobjectData = (
    Entity,
    &'static MobjectId,
    &'static GlobalSpatialTransform,
    &'static GlobalOpacity,
    &'static RenderOrder,
    &'static RenderLayer,
    Option<Ref<'static, Path2D>>,
    Option<Ref<'static, PathSource>>,
    Option<Ref<'static, FillBrush>>,
    Option<Ref<'static, StrokeBrush>>,
    Option<Ref<'static, RasterImage>>,
    Option<Ref<'static, LottiePlayer>>,
);

/// A drawable's effects, bounds and group marker, read apart to keep
/// [`MobjectData`] small.
pub(super) type EffectsData = (
    Option<Ref<'static, DropShadow>>,
    Option<Ref<'static, Glow>>,
    Option<Ref<'static, GaussianBlur>>,
    Option<&'static ClipMask>,
    Option<Ref<'static, FillDrawProgress>>,
    Option<&'static WorldBounds>,
    Option<&'static gaanim_scene::GroupMarker>,
    Option<Ref<'static, WriteTipGlow>>,
    Option<Ref<'static, StrokeAlign>>,
    Option<Ref<'static, StrokeProfile>>,
    Has<StrokeScalesWithObject>,
    Option<Ref<'static, ChalkBrush>>,
);

pub(super) type MobjectItem<'w, 's> = QueryItem<'w, 's, MobjectData>;
pub(super) type EffectsItem<'w, 's> = QueryItem<'w, 's, EffectsData>;

/// Every query the extraction reads. Read-only, so the headless path runs it
/// through a [`SystemState`] on a shared `&World`.
#[derive(SystemParam)]
pub struct ElementQueries<'w, 's> {
    pub(super) mobjects:
        Query<'w, 's, MobjectData, (With<Visible>, Without<gaanim_scene::DrawnIn3D>)>,
    pub(super) effects: Query<'w, 's, EffectsData>,
    pub(super) parents: Query<'w, 's, &'static ChildOf>,
    pub(super) order: Query<
        'w,
        's,
        (
            &'static RenderOrder,
            Has<gaanim_scene::SegmentContent>,
            Option<&'static gaanim_scene::ZLayer>,
        ),
    >,
    pub(super) blends: Query<'w, 's, &'static ElementBlend>,
    pub(super) opacities: Query<'w, 's, &'static GlobalOpacity>,
    pub(super) shader_effects: Query<
        'w,
        's,
        (
            &'static crate::object_effects::ShaderEffect,
            Has<crate::object_effects::DepthOfFieldBlur>,
        ),
    >,
    pub(super) mattes: Query<'w, 's, (Entity, &'static crate::object_effects::Matte)>,
    pub(super) matte_sources: Query<'w, 's, (), With<crate::object_effects::MatteSource>>,
    pub(super) glasses: Query<'w, 's, (Entity, &'static crate::object_effects::Glass)>,
    pub(super) huds: Query<'w, 's, (), With<gaanim_scene::HudOverlay>>,
    pub(super) view_layers: Query<'w, 's, &'static ViewLayer>,
    pub(super) post_exempt: Query<'w, 's, (), With<PostProcessExempt>>,
    pub(super) motion_blur_exempt: Query<'w, 's, (), With<MotionBlurExempt>>,
    pub(super) echoes: Query<'w, 's, &'static gaanim_animation::EchoGhost>,
    pub(super) tips: TipQuery<'w, 's>,
    pub(super) backdrops: Query<'w, 's, &'static gaanim_scene::LayoutBackdrop>,
    pub(super) camera_screens: CameraScreenQuery<'w, 's>,
    pub(super) camera_sources: CameraSourceQuery<'w, 's>,
    pub(super) snapshots: Query<
        'w,
        's,
        (
            Entity,
            &'static CameraSnapshot,
            &'static GlobalSpatialTransform,
            &'static Path2D,
        ),
        With<Visible>,
    >,
    pub(super) float_signals: Query<'w, 's, &'static gaanim_animation::FloatSignal>,
    pub(super) view_cameras: Query<'w, 's, &'static crate::effects::ViewCamera>,
}

impl ElementQueries<'_, '_> {
    fn parent(&self, entity: Entity) -> Option<Entity> {
        self.parents.get(entity).ok().map(ChildOf::parent)
    }

    /// The camera views shown at `time`, with what `frozen` snapshots show.
    pub(super) fn camera_views(
        &self,
        time: f64,
        frozen: Option<&FrozenViews>,
    ) -> (
        HashMap<Entity, Arc<ExtractedCameraView>>,
        Vec<Arc<FrozenView>>,
    ) {
        let mut views = resolve_camera_views(
            self.camera_screens
                .iter()
                .filter(|(_, view, ..)| view.shown_at(time)),
            |entity| {
                let (transform, local, source, bounds, group) =
                    self.camera_sources.get(entity).ok()?;
                camera_source_frame(transform, local, source, bounds, group.is_some())
            },
            |zoom| {
                zoom.source
                    .evaluate(time, |logical| {
                        let (_, entity) = zoom.parameters.iter().find(|(id, _)| *id == logical)?;
                        self.float_signals
                            .get(*entity)
                            .ok()
                            .map(|signal| signal.value)
                    })
                    .ok()
            },
            |entity| self.view_cameras.get(entity).ok().map(|camera| camera.0),
        );
        let shown = frozen.map_or_else(Vec::new, |frozen| {
            snapshot_views(self.snapshots.iter(), frozen, &mut views)
        });
        (views, shown)
    }

    /// Whether any drawable belongs to a segment.
    pub(super) fn segmented(&self) -> bool {
        self.order.iter().any(|(_, member, _)| member)
    }
}

/// Motion blur sub-frames draw [`MotionBlurExempt`] drawables as the frame's
/// own time showed them.
pub(super) enum Pins<'a> {
    /// No motion blur: every drawable is extracted.
    Off,
    /// The frame's own time: exempt elements are also copied here.
    Record(&'a mut Vec<ExtractedElement>),
    /// A sub-frame: exempt drawables are left out, for the recorded copies.
    Replay,
}

/// What one frame's extraction depends on beyond the queries.
pub(super) struct ElementFrame<'a> {
    pub camera_views: &'a HashMap<Entity, Arc<ExtractedCameraView>>,
    /// World region outside which drawables, and what they contain, are
    /// left out, unless a camera view sees them.
    pub cull: Option<gaanim_math::Bounds3D>,
    /// The scene transition in progress, if it splits the frame.
    pub transition: Option<&'a gaanim_scene::SceneTransitionFrame>,
    /// Region of an opacity layer whose extent is unknown.
    pub opacity_fallback: kurbo::Rect,
    /// Antialiasing margin of opacity layers, in world units.
    pub antialias: f64,
    /// Fragments are drawn for a renderer that runs object effects.
    pub gpu: bool,
    pub pins: Pins<'a>,
}

/// What a drawable's fragment is built from.
pub(super) struct FragmentRequest<'a> {
    pub id: ObjectId,
    /// Without the drop shadow when the drawable's group draws it.
    pub parts: FragmentParts<'a>,
    /// The fragment without its shadow is needed too (see
    /// [`ExtractedElement::effect_shadow`]).
    pub effect_casts_shadow: bool,
    pub lottie: Option<&'a LottiePlayer>,
    pub gpu: bool,
    /// One of the components the fragment is built from changed since the
    /// extracting system last ran. Meaningful only to a system that runs
    /// once per frame, as the preview's does.
    pub changed: bool,
}

/// A built fragment and what goes with it.
pub(super) struct ElementFragment {
    pub scene: Arc<vello::Scene>,
    pub soft: Arc<[crate::object_effects::EffectLayer]>,
    /// A camera view screen's stroke, drawn above its view.
    pub overlay: Option<Arc<vello::Scene>>,
    pub effect_shadow: Option<Arc<crate::fragment::EffectShadow>>,
    /// What the fragment was built from, when the frame is captured.
    pub recipe: Option<Arc<FragmentRecipe>>,
}

/// Where an extraction gets its fragments: each renderer keeps its own cache.
pub(super) trait FragmentSource {
    /// The fragment of a drawable that is drawn this frame.
    fn fragment(&mut self, request: FragmentRequest<'_>) -> ElementFragment;
}

/// Extract every drawable `queries` shows into `out`, unsorted.
pub(super) fn extract_elements(
    queries: &ElementQueries,
    frame: &mut ElementFrame,
    source: &mut impl FragmentSource,
    out: &mut Vec<ExtractedElement>,
) {
    let segmented = queries.segmented();
    let mut culled = std::collections::HashSet::new();
    let replaying = matches!(frame.pins, Pins::Replay);
    let pinning = !matches!(frame.pins, Pins::Off);
    for mobject in &queries.mobjects {
        let Ok(effects) = queries.effects.get(mobject.0) else {
            continue;
        };
        let entity = mobject.0;
        let camera_view = frame.camera_views.get(&entity);
        let stroke_view = stroke_view(&mobject, &effects);

        let (
            _,
            mobj_id,
            transform,
            global_opacity,
            render_order,
            render_layer,
            path,
            path_source,
            fill,
            stroke,
            raster,
            lottie,
        ) = &mobject;
        let (
            shadow,
            glow,
            blur,
            clip,
            fill_progress,
            world_bounds,
            group,
            tip_glow,
            stroke_align,
            stroke_profile,
            _,
            chalk,
        ) = &effects;
        if **render_layer != RenderLayer::Vello2D
            || opacity_is_empty(global_opacity)
            // Groups do not draw geometry; they only act as spatial nodes.
            || group.is_some()
        {
            continue;
        }
        let pen = stroke
            .as_deref()
            .filter(|stroke| stroke.brush.is_some())
            .map(|stroke| {
                aligned_pen(
                    &stroke.style,
                    stroke_align.as_deref().copied().unwrap_or_default(),
                )
            });
        if let Some(bounds) = &frame.cull {
            let mut current = entity;
            let mut ancestor_culled = false;
            while let Some(parent) = queries.parent(current) {
                if culled.contains(&parent) {
                    ancestor_culled = true;
                    break;
                }
                current = parent;
            }
            // What a drawable draws beyond its bounds (a glow, a shadow) may
            // reach into the view from outside it.
            let outside = |world: &WorldBounds| {
                if bounds.intersects(&world.0) {
                    return false;
                }
                let reach = fragment_reach(
                    transform.affine_2d,
                    pen.as_deref(),
                    stroke_view,
                    shadow.as_deref(),
                    glow.as_deref(),
                    blur.as_deref(),
                );
                let reach = gaanim_core::glam::DVec3::new(reach, reach, 0.0);
                let drawn = gaanim_math::Bounds3D::new(world.0.min - reach, world.0.max + reach);
                !bounds.intersects(&drawn) && !seen_by_camera_views(frame.camera_views, &drawn)
            };
            if ancestor_culled || world_bounds.is_some_and(outside) {
                culled.insert(entity);
                continue;
            }
        }

        let mut opacity_group = entity;
        while let Some(parent) = queries.parent(opacity_group) {
            opacity_group = parent;
        }
        // A HUD stays sharp inside a layer the depth of field blurs.
        let mut under_hud = false;
        let effect_root = outermost_effect(entity, |node| {
            under_hud |= queries.huds.contains(node);
            (
                queries
                    .shader_effects
                    .get(node)
                    .is_ok_and(|(_, depth_blur)| !(depth_blur && under_hud)),
                queries.parent(node),
            )
        });
        let matte_root = outermost_effect(entity, |node| {
            (queries.mattes.contains(node), queries.parent(node))
        });
        let matte_of = outermost_effect(entity, |node| {
            (queries.matte_sources.contains(node), queries.parent(node))
        });
        let glass_root = outermost_effect(entity, |node| {
            (queries.glasses.contains(node), queries.parent(node))
        });
        let group_opacity = queries
            .opacities
            .get(opacity_group)
            .map_or(1.0, |opacity| opacity.0);
        let blend = queries
            .blends
            .get(entity)
            .ok()
            .map(|blend| blend.0)
            // An explicit normal blend paints plainly, without its own layer.
            .filter(|blend| *blend != peniko::BlendMode::default());

        let mut parts = FragmentParts {
            path: path.as_deref(),
            source: path_source.as_deref(),
            fill: fill.as_deref(),
            stroke: stroke.as_deref(),
            raster: raster.as_deref(),
            lottie: lottie.is_some(),
            shadow: shadow.as_deref(),
            glow: glow.as_deref(),
            blur: blur.as_deref(),
            fill_progress: fill_progress.as_deref(),
            tip_glow: tip_glow.as_deref(),
            stroke_align: stroke_align.as_deref(),
            stroke_profile: stroke_profile.as_deref(),
            stroke_view,
            screen: camera_view.is_some(),
            chalk: chalk.as_deref(),
        };
        let group_shadow = crate::fragment::group_shadow(
            &parts,
            opacity_group != entity && clip.is_none() && blend.is_none() && camera_view.is_none(),
        )
        .map(Arc::new);
        if group_shadow.is_some() {
            parts.shadow = None;
        }
        // Inside a shader effect, a drawable's own shadow may be cast from
        // what the effect draws instead (see `divert_effects`), so the
        // fragment without it is built too.
        let effect_casts_shadow = frame.gpu && effect_root.is_some() && parts.shadow.is_some();
        let changed = [
            path.as_ref().map(Ref::is_changed),
            path_source.as_ref().map(Ref::is_changed),
            fill.as_ref().map(Ref::is_changed),
            stroke.as_ref().map(Ref::is_changed),
            raster.as_ref().map(Ref::is_changed),
            lottie.as_ref().map(Ref::is_changed),
            shadow.as_ref().map(Ref::is_changed),
            glow.as_ref().map(Ref::is_changed),
            blur.as_ref().map(Ref::is_changed),
            fill_progress.as_ref().map(Ref::is_changed),
            tip_glow.as_ref().map(Ref::is_changed),
            stroke_align.as_ref().map(Ref::is_changed),
            stroke_profile.as_ref().map(Ref::is_changed),
            chalk.as_ref().map(Ref::is_changed),
        ]
        .contains(&Some(true));
        let fragment = source.fragment(FragmentRequest {
            id: mobj_id.0,
            parts,
            changed,
            effect_casts_shadow,
            lottie: lottie.as_deref(),
            gpu: frame.gpu,
        });
        // A sub-frame draws the copy recorded at the frame's own time; the
        // fragment is still asked for, so the cache keeps it.
        let exempt = pinning && queries.motion_blur_exempt.contains(entity);
        if exempt && replaying {
            continue;
        }

        // The visible outline, empty while a reveal shows nothing of it.
        let visible_path = path
            .as_ref()
            .filter(|_| !path_reveal_is_empty(tip_glow.as_deref()))
            .map(|path| path.0.as_ref());
        let extent = || {
            visible_path.and_then(|path| {
                fragment_extent(
                    path,
                    transform.affine_2d,
                    pen.as_deref(),
                    stroke_view,
                    shadow.as_deref(),
                    glow.as_deref(),
                    blur.as_deref(),
                )
            })
        };
        // Only translucent or blended elements open a layer; a Lottie draws
        // geometry that `Path2D` does not describe.
        let opacity_extent = (!(global_opacity.0 >= 1.0 && blend.is_none() || lottie.is_some()))
            .then(|| opacity_layer_extent(extent(), *world_bounds, frame.opacity_fallback));
        let opacity_bounds = opacity_extent.map_or(frame.opacity_fallback, |(rect, reach)| {
            pad_opacity_layer(rect, reach, frame.antialias, frame.opacity_fallback)
        });
        let effect_extent = effect_root.and_then(|_| extent()).map(reached_rect);
        let lottie_scene = fragment
            .recipe
            .as_ref()
            .and(lottie.as_ref())
            .map(|lottie| Arc::clone(lottie.scene()));
        let element = ExtractedElement {
            entity,
            outline: glass_root
                .and(path.as_ref())
                .map(|path| Arc::clone(&path.0)),
            recipe: fragment.recipe,
            lottie: lottie_scene,
            transform: transform.affine_2d,
            opacity: global_opacity.0,
            opacity_bounds,
            // A capture keeps the extent, so a replay at another resolution
            // pads the layer for its own pixels; an unbounded reach keeps the
            // fallback.
            opacity_extent: opacity_extent.filter(|(_, reach)| reach.is_finite()),
            opacity_group,
            group_opacity,
            group_shadow,
            effect_shadow: fragment.effect_shadow,
            effect_root,
            effect_extent,
            matte_root,
            matte_of,
            glass_root,
            render_order: stacked_render_order(
                **render_order,
                entity,
                |e| queries.parent(e),
                |e| {
                    queries
                        .order
                        .get(e)
                        .map_or(0, |(order, _, _)| order.z_index)
                },
            ),
            z_layer: inherited_z_layer(
                entity,
                |e| queries.parent(e),
                |e| {
                    queries
                        .order
                        .get(e)
                        .ok()
                        .and_then(|(_, _, layer)| layer.map(|layer| layer.0))
                },
            ),
            scene: fragment.scene,
            soft: fragment.soft,
            clip_mask: clip.cloned(),
            blend,
            transition_side: frame
                .transition
                .map_or_else(Default::default, |transition| {
                    transition.side_of(entity, |e| queries.parent(e))
                }),
            persistent: segmented
                && outside_segments(
                    entity,
                    |e| queries.parent(e),
                    |e| queries.order.get(e).is_ok_and(|(_, member, _)| member),
                ),
            post_exempt: queries.post_exempt.contains(entity),
            lineage: if frame.camera_views.is_empty() {
                Vec::new()
            } else {
                element_lineage(entity, |e| queries.parent(e))
            },
            view_bounds: world_bounds.map(|bounds| bounds_rect(&bounds.0)),
            in_views: !queries.huds.contains(entity),
            layer: queries
                .view_layers
                .get(entity)
                .ok()
                .map(|layer| Arc::clone(&layer.0)),
            screen: camera_view.map(|view| ExtractedScreen {
                view: Arc::clone(view),
                overlay: fragment.overlay,
            }),
            echo_rank: queries.echoes.get(entity).map_or(0, |echo| echo.rank),
            tip: queries.tips.contains(entity),
            backdrop: queries
                .backdrops
                .get(entity)
                .ok()
                .map(|backdrop| backdrop.depth),
        };
        if exempt && let Pins::Record(recorded) = &mut frame.pins {
            recorded.push(element.clone());
        }
        out.push(element);
    }
}

/// The pen correction of a drawable whose stroke keeps its scene-unit width.
fn stroke_view(
    mobject: &MobjectItem<'_, '_>,
    effects: &EffectsItem<'_, '_>,
) -> Option<kurbo::Affine> {
    let (_, _, transform, _, _, _, _, _, _, stroke, ..) = mobject;
    let (_, glow, .., scales, chalk) = effects;
    let strokes =
        stroke.is_some_and(|stroke| stroke.brush.is_some()) || glow.is_some() || chalk.is_some();
    (!scales && strokes)
        .then(|| scene_unit_stroke_transform(transform.affine_2d))
        .flatten()
}
