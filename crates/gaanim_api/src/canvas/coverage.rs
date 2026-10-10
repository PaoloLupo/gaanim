//! Texts that an opaque shape hides completely where a presentation rests,
//! for `gaanim check`.

use std::collections::{HashMap, HashSet};

use bevy::prelude::{App, ChildOf, Children, Entity, World};
use gaanim_core::kurbo::{BezPath, Point, Rect, Shape};
use gaanim_core::peniko::Brush;
use gaanim_math::GlobalSpatialTransform;
use gaanim_scene::components::TextSpan;
use gaanim_scene::{
    CoversOnPurpose, FillBrush, GlobalOpacity, HudOverlay, LayoutBackdrop, LocalBounds, ObjectTag,
    Path2D, RasterImage, RenderLayer, RenderOrder, SegmentContent, Visible, ZLayer,
};
use gaanim_timeline::timeline::Timeline;

use super::SceneModel;

/// Below this opacity a glyph counts as hidden on purpose.
const SHOWN: f32 = 0.05;
/// Points of a text's box, per side, that must all lie in a shape's fill.
const SAMPLES: usize = 5;

/// Where a drawn element sits in the renderer's draw order; see
/// `ExtractedElement::draw_order` in `gaanim_renderer`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct DrawKey {
    layer: i32,
    z_index: i32,
    persistent: bool,
    creation: u64,
}

/// An instant where the presentation rests.
struct Rest {
    time: f64,
    place: String,
}

impl SceneModel {
    /// Texts that an opaque shape drawn above them hides completely at a
    /// stop or at the end of a segment, as warnings that name the text, the
    /// shape and where it is first hidden. A text faded out, or behind a
    /// translucent shape, an image or a shape that covers it on purpose, is
    /// not reported.
    pub fn covered_text_warnings(&self) -> Vec<String> {
        let rests = self.rests();
        if rests.is_empty() {
            return Vec::new();
        }
        let mut app = self.app_for_check();
        let mut covered: Vec<(Entity, Entity, String)> = Vec::new();
        let mut reported = HashSet::new();
        for rest in &rests {
            app.world_mut().resource_mut::<Timeline>().seek_request = Some(rest.time);
            // The seek and the systems that follow what it moved.
            app.update();
            for (text, shape) in covered_texts(app.world_mut()) {
                if reported.insert(text) {
                    covered.push((text, shape, rest.place.clone()));
                }
            }
        }
        let world = app.world();
        covered
            .into_iter()
            .map(|(text, shape, place)| {
                format!(
                    "text \"{}\" is hidden behind a {} {place}; draw it above with z_layer or z_index, or mark the {} with covers_on_purpose()",
                    text_content(world, text),
                    shape_name(world, shape),
                    cover_name(world, shape),
                )
            })
            .collect()
    }

    /// The stops and segment ends, where a presentation shows a still frame.
    fn rests(&self) -> Vec<Rest> {
        let manifest = self.segment_manifest();
        let named = manifest.segments.len() > 1;
        let mut rests = Vec::new();
        for segment in &manifest.segments {
            // The next segment starts at this one's end, so its last frame
            // is drawn just before.
            let last = (segment.end_time - 1e-4).max(segment.start_time);
            let within = if named {
                format!(" of segment `{}`", segment.name)
            } else {
                String::new()
            };
            for (index, stop) in segment.stops.iter().enumerate() {
                let label = stop
                    .name
                    .as_ref()
                    .map_or_else(|| format!("{}", index + 1), |name| format!("`{name}`"));
                rests.push(Rest {
                    time: stop.time.min(last),
                    place: format!("at stop {label}{within}"),
                });
            }
            rests.push(Rest {
                time: last,
                place: if named {
                    format!("at the end{within}")
                } else {
                    "at the end of the scene".to_string()
                },
            });
        }
        rests
    }

    /// The scene compiled into an app that runs the systems the preview
    /// runs between a seek and a frame, without rendering.
    fn app_for_check(&self) -> App {
        let saved = self
            .state
            .lock()
            .expect("canvas state poisoned")
            .layout_diagnostics
            .clone();
        let mut app = App::new();
        app.add_plugins(bevy::prelude::MinimalPlugins)
            .add_plugins(gaanim_scene::GaanimScenePlugin)
            .add_plugins(gaanim_animation::GaanimAnimationPlugin)
            .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
            .add_plugins(gaanim_text::GaanimTextPlugin);
        self.compile(app.world_mut());
        // Compiling records the layout diagnostics again.
        self.state
            .lock()
            .expect("canvas state poisoned")
            .layout_diagnostics = saved;
        app.finish();
        app.cleanup();
        app.update();
        app
    }
}

/// Texts, by root, that an opaque shape drawn above hides completely in the
/// world as it stands, with that shape.
fn covered_texts(world: &mut World) -> Vec<(Entity, Entity)> {
    let mut members = world.query::<&SegmentContent>();
    let mut orders = world.query::<(Entity, &RenderOrder)>();
    let mut glyphs = world.query::<(Entity, &ChildOf, &TextSpan, &LocalBounds)>();
    let mut fills = world.query::<(Entity, &Path2D, &FillBrush)>();
    let world: &World = world;

    let segmented = members.iter(world).next().is_some();
    let drawn: HashMap<Entity, (DrawKey, f32)> = orders
        .iter(world)
        .filter_map(|(entity, order)| Some((entity, drawn(world, entity, *order, segmented)?)))
        .collect();

    // The box each text's shown glyphs draw in, and the last of them.
    let mut texts: HashMap<Entity, (Rect, DrawKey)> = HashMap::new();
    for (glyph, parent, _, bounds) in glyphs.iter(world) {
        let Some((key, opacity)) = drawn.get(&glyph) else {
            continue;
        };
        let Some(transform) = world.get::<GlobalSpatialTransform>(glyph) else {
            continue;
        };
        let local = Rect::new(
            bounds.0.min.x,
            bounds.0.min.y,
            bounds.0.max.x,
            bounds.0.max.y,
        );
        let rect = transform.affine_2d.transform_rect_bbox(local);
        if *opacity < SHOWN || !(rect.is_finite() && rect.area() > 0.0) {
            continue;
        }
        texts
            .entry(parent.parent())
            .and_modify(|(bounds, last)| {
                *bounds = bounds.union(rect);
                *last = (*last).max(*key);
            })
            .or_insert((rect, *key));
    }
    if texts.is_empty() {
        return Vec::new();
    }

    let shapes: Vec<(Entity, DrawKey, Rect, BezPath)> = fills
        .iter(world)
        .filter(|(entity, path, fill)| {
            !path.0.elements().is_empty()
                && matches!(&fill.0, Some(Brush::Solid(color)) if color.components[3] >= 0.999)
                && drawn
                    .get(entity)
                    .is_some_and(|(_, opacity)| *opacity >= 0.999)
                && opaque_cover(world, *entity)
                && !covers_on_purpose(world, *entity)
        })
        .filter_map(|(entity, path, _)| {
            let transform = world.get::<GlobalSpatialTransform>(entity)?;
            let outline = transform.affine_2d * (*path.0).clone();
            let bounds = outline.bounding_box();
            (bounds.area() > 0.0).then(|| (entity, drawn[&entity].0, bounds, outline))
        })
        .collect();

    let mut covered: Vec<(Entity, Entity)> = texts
        .into_iter()
        .filter_map(|(text, (bounds, last))| {
            // A shape flush with the text's box still hides it.
            let inner = bounds.inset(-0.01 * bounds.width().min(bounds.height()));
            let (shape, ..) = shapes.iter().find(|(_, key, shape_bounds, outline)| {
                *key > last
                    && contains_rect(*shape_bounds, inner)
                    && samples(inner).all(|point| outline.contains(point))
            })?;
            Some((text, *shape))
        })
        .collect();
    covered.sort();
    covered
}

/// `entity`'s place in the draw order and the opacity it draws with, when
/// the renderer draws it now in the scene (not as a HUD overlay).
fn drawn(
    world: &World,
    entity: Entity,
    order: RenderOrder,
    segmented: bool,
) -> Option<(DrawKey, f32)> {
    let opacity = world.get::<GlobalOpacity>(entity)?.0;
    if world.get::<Visible>(entity).is_none()
        || opacity <= f32::EPSILON
        || world
            .get::<RenderLayer>(entity)
            .is_some_and(|layer| *layer != RenderLayer::Vello2D)
    {
        return None;
    }
    let mut key = DrawKey {
        layer: 0,
        z_index: order.z_index,
        persistent: segmented,
        creation: order.creation_order,
    };
    let mut layer = None;
    let mut current = Some(entity);
    while let Some(node) = current {
        if world.get::<HudOverlay>(node).is_some() {
            return None;
        }
        layer = layer.or(world.get::<ZLayer>(node).map(|layer| layer.0));
        key.persistent &= world.get::<SegmentContent>(node).is_none();
        current = world.get::<ChildOf>(node).map(ChildOf::parent);
        if let Some(parent) = current {
            let lift = world
                .get::<RenderOrder>(parent)
                .map_or(0, |order| order.z_index);
            key.z_index = key.z_index.saturating_add(lift);
        }
    }
    key.layer = layer.unwrap_or(0);
    Some((key, opacity))
}

/// The drawable to mark when `shape` covers a text on purpose: the
/// outermost group it belongs to, which is what a script names, or the shape.
fn cover_name(world: &World, shape: Entity) -> String {
    let mut top = shape;
    while let Some(parent) = world.get::<ChildOf>(top).map(ChildOf::parent) {
        if world.get::<RenderOrder>(parent).is_none() {
            break;
        }
        top = parent;
    }
    if top == shape {
        "shape".to_string()
    } else {
        let name = world
            .get::<ObjectTag>(top)
            .map(|tag| tag.0.as_str())
            .filter(|tag| !tag.is_empty())
            .unwrap_or("group");
        format!("{name} it belongs to")
    }
}

/// Whether `entity` or a group it belongs to hides texts on purpose.
fn covers_on_purpose(world: &World, entity: Entity) -> bool {
    let mut current = Some(entity);
    while let Some(node) = current {
        if world.get::<CoversOnPurpose>(node).is_some() {
            return true;
        }
        current = world.get::<ChildOf>(node).map(ChildOf::parent);
    }
    false
}

/// Whether `entity` draws its whole fill as is: not a glyph, an image, a
/// box background (drawn beneath its box's content), an echo copy, a camera
/// view or a fill being drawn, clipped or blended.
fn opaque_cover(world: &World, entity: Entity) -> bool {
    world.get::<TextSpan>(entity).is_none()
        && world
            .get::<RasterImage>(entity)
            .is_none_or(|raster| raster.image.is_none())
        && world
            .get::<gaanim_renderer::lottie::LottiePlayer>(entity)
            .is_none()
        && world.get::<LayoutBackdrop>(entity).is_none()
        && world.get::<gaanim_animation::EchoGhost>(entity).is_none()
        && world
            .get::<gaanim_renderer::effects::CameraView>(entity)
            .is_none()
        && world
            .get::<gaanim_renderer::effects::ClipMask>(entity)
            .is_none()
        && world
            .get::<gaanim_renderer::effects::ElementBlend>(entity)
            .is_none()
        && world
            .get::<gaanim_animation::FillDrawProgress>(entity)
            .is_none_or(|progress| progress.0 >= 0.999)
}

fn contains_rect(outer: Rect, inner: Rect) -> bool {
    outer.x0 <= inner.x0 && outer.y0 <= inner.y0 && inner.x1 <= outer.x1 && inner.y1 <= outer.y1
}

/// A grid of points over `rect`, corners included.
fn samples(rect: Rect) -> impl Iterator<Item = Point> {
    let at =
        |from: f64, to: f64, index: usize| from + (to - from) * index as f64 / (SAMPLES - 1) as f64;
    (0..SAMPLES).flat_map(move |i| {
        (0..SAMPLES).map(move |j| Point::new(at(rect.x0, rect.x1, i), at(rect.y0, rect.y1, j)))
    })
}

/// What a text says, shortened.
fn text_content(world: &World, text: Entity) -> String {
    let mut glyphs: Vec<TextSpan> = world
        .get::<Children>(text)
        .into_iter()
        .flat_map(|children| children.iter())
        .filter_map(|child| world.get::<TextSpan>(*child).copied())
        .collect();
    glyphs.sort_by_key(|glyph| glyph.char_index);
    // Spaces draw no glyph: a gap in the source between two glyphs stands
    // for one.
    let mut content = String::new();
    let mut end = None;
    for glyph in &glyphs {
        if end.is_some_and(|end| glyph.source_range.start > end) {
            content.push(' ');
        }
        content.push(glyph.character);
        end = Some(glyph.source_range.end);
    }
    let content = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if content.chars().count() > 40 {
        format!("{}…", content.chars().take(39).collect::<String>())
    } else {
        content
    }
}

/// The kind of shape `entity` draws, such as `Rectangle`.
fn shape_name(world: &World, entity: Entity) -> String {
    world
        .get::<ObjectTag>(entity)
        .map(|tag| tag.0.strip_prefix("SvgPath#").unwrap_or(&tag.0))
        .filter(|tag| !tag.is_empty())
        .unwrap_or("shape")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::super::SceneModel;
    use gaanim_core::peniko::Color;

    const RED: Color = Color::from_rgb8(200, 30, 30);

    /// Warnings for a text with `cover` declared after it.
    fn warnings(cover: impl FnOnce(&mut SceneModel)) -> Vec<String> {
        let mut canvas = SceneModel::new(640, 360);
        canvas.text("Pier 2");
        cover(&mut canvas);
        canvas.wait(1.0);
        canvas.covered_text_warnings()
    }

    #[test]
    fn an_opaque_shape_drawn_above_a_text_hides_it() {
        let hidden = warnings(|canvas| {
            canvas.rect(4.0, 2.0).fill(RED);
        });
        assert_eq!(
            hidden,
            [
                "text \"Pier 2\" is hidden behind a Rectangle at the end of the scene; draw it above with z_layer or z_index, or mark the shape with covers_on_purpose()"
            ]
        );
        for cover in [
            |canvas: &mut SceneModel| {
                canvas.rect(4.0, 2.0).fill(RED).opacity(0.5);
            },
            |canvas: &mut SceneModel| {
                canvas.rect(4.0, 2.0).fill(RED).z_index(-1);
            },
            |canvas: &mut SceneModel| {
                canvas.rect(0.2, 0.2).fill(RED);
            },
            |canvas: &mut SceneModel| {
                canvas.rect(4.0, 2.0).fill(RED).covers_on_purpose();
            },
        ] {
            assert!(warnings(cover).is_empty());
        }
    }

    #[test]
    fn a_layer_in_front_keeps_a_text_above_any_z_index() {
        let mut canvas = SceneModel::new(640, 360);
        canvas.z_layers(&["model", "overlay"], None).unwrap();
        canvas.text("Pier 2").z_layer("overlay").unwrap();
        canvas.rect(4.0, 2.0).fill(RED).z_index(400);
        canvas.wait(1.0);
        assert!(canvas.covered_text_warnings().is_empty());
    }

    #[test]
    fn a_group_that_covers_on_purpose_takes_its_members_with_it() {
        let dialog = |canvas: &mut SceneModel| {
            let panel = canvas.rect(4.0, 2.0).fill(RED);
            let title = canvas.text("Define");
            canvas.group(&[&panel, &title])
        };
        let hidden = warnings(|canvas| {
            dialog(canvas);
        });
        assert_eq!(hidden.len(), 1, "{hidden:?}");
        assert!(
            hidden[0].ends_with("or mark the group it belongs to with covers_on_purpose()"),
            "{hidden:?}"
        );
        let hidden = warnings(|canvas| {
            dialog(canvas).covers_on_purpose();
        });
        assert!(hidden.is_empty(), "{hidden:?}");
    }

    #[test]
    fn the_warning_says_where_the_text_is_first_hidden() {
        let mut canvas = SceneModel::new(640, 360);
        canvas.segment("muros", None).unwrap();
        canvas.text("Pier 2");
        canvas.wait(1.0);
        canvas.stop(Some("lupa".to_string())).unwrap();
        let wall = canvas.rect(4.0, 2.0).fill(RED).hidden().unwrap();
        canvas.play(vec![wall.animate().fade_in().duration(1.0)]);
        canvas.segment("vigas", None).unwrap();
        canvas.text("Viga");
        canvas.wait(1.0);
        let warnings = canvas.covered_text_warnings();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(
            warnings[0].contains("at the end of segment `muros`"),
            "{warnings:?}"
        );
    }
}
