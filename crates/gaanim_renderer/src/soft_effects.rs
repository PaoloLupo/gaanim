//! Drop shadows and glows blurred on the GPU.
//!
//! A soft effect is drawn in its fragment as one image, placed in the
//! fragment's local coordinates. Its [`EffectLayer`] holds the silhouette
//! the image is blurred from; [`crate::object_effects::ObjectEffects`]
//! renders it, blurs it with two separable Gaussian passes that also tint
//! it, and keeps the result while the fragment draws the same image: the
//! image belongs to one built fragment, so its pixels never change.
//!
//! The texture holds three texels per sigma, whatever the zoom: a Gaussian
//! that wide is smooth enough to be scaled up without showing its texels.
//! A renderer that does not run object effects draws the vector
//! approximations in [`crate::pipeline`] instead.

use std::sync::{Arc, OnceLock};

use gaanim_core::kurbo;
use gaanim_core::peniko::{self, Blob, ImageAlphaType, ImageData, ImageFormat};

use crate::effects::{DropShadow, Glow};
use crate::object_effects::EffectLayer;
use crate::post_process::{PostProcessRequest, PostProcessShader};

/// Texels per sigma of a soft effect's texture.
const TEXELS_PER_SIGMA: f64 = 3.0;
/// Largest side of a soft effect's texture; a larger effect gets fewer
/// texels per sigma.
const MAX_SOFT_TEXTURE: f64 = 2048.0;
/// The blur reaches this many sigmas from the silhouette.
const SOFT_REACH: f64 = 3.0;

/// Blurs the texture's alpha along one axis; the vertical pass then tints
/// it with an alpha that saturates as `1 - exp(-gain * coverage)`, like the
/// vector effects, which stack translucent copies instead of averaging them.
const SOFT_BLUR: &str = r#"
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let p = uv * resolution;
    let sigma = max(gaanim_uniforms.sigma, 0.001);
    let horizontal = gaanim_uniforms.horizontal > 0.5;
    let radius = ceil(3.0 * sigma);
    let step = max(1.0, radius / 32.0);
    var sum = 0.0;
    var total = 0.0;
    for (var i = -32; i <= 32; i++) {
        let x = f32(i) * step;
        if (abs(x) > radius) {
            continue;
        }
        let weight = exp(-0.5 * x * x / (sigma * sigma));
        let offset = select(vec2<f32>(0.0, x), vec2<f32>(x, 0.0), horizontal);
        sum += gaanim_scene((p + offset) / resolution).a * weight;
        total += weight;
    }
    let coverage = sum / total;
    if (horizontal) {
        return vec4<f32>(0.0, 0.0, 0.0, coverage);
    }
    let alpha = 1.0 - exp(-gaanim_uniforms.gain * coverage);
    return vec4<f32>(
        gaanim_uniforms.red,
        gaanim_uniforms.green,
        gaanim_uniforms.blue,
        clamp(alpha * gaanim_uniforms.alpha, 0.0, 1.0),
    );
}
"#;

fn soft_blur_shader() -> Option<&'static PostProcessShader> {
    static SHADER: OnceLock<Option<PostProcessShader>> = OnceLock::new();
    SHADER
        .get_or_init(|| {
            PostProcessShader::with_uniforms(
                SOFT_BLUR,
                [
                    "sigma",
                    "horizontal",
                    "red",
                    "green",
                    "blue",
                    "alpha",
                    "gain",
                ],
            )
            .ok()
        })
        .as_ref()
}

/// Draw into `scene` the image of `silhouette` (drawn in black within
/// `bounds`, local coordinates) blurred by `sigma` local units and tinted
/// `color` with an alpha of `1 - exp(-gain * coverage)`, and return the
/// layer that fills it. `None` when the blur is not finite and positive, or
/// the shader is unavailable.
fn draw_soft_image(
    scene: &mut vello::Scene,
    silhouette: vello::Scene,
    bounds: kurbo::Rect,
    sigma: f64,
    color: peniko::Color,
    gain: f64,
) -> Option<EffectLayer> {
    let shader = soft_blur_shader()?;
    if !(sigma.is_finite() && sigma > 0.0 && bounds.is_finite()) {
        return None;
    }
    let reach = SOFT_REACH * sigma;
    let area = bounds.inflate(reach, reach);
    if !(area.width() > 0.0 && area.height() > 0.0) {
        return None;
    }
    let mut density = TEXELS_PER_SIGMA / sigma;
    let largest = area.width().max(area.height()) * density;
    if largest > MAX_SOFT_TEXTURE {
        density *= MAX_SOFT_TEXTURE / largest;
    }
    let side = |length: f64| (length * density).ceil().clamp(1.0, MAX_SOFT_TEXTURE) as u32;
    let (width, height) = (side(area.width()), side(area.height()));
    // A new allocation per built fragment: its id names these pixels.
    let image = ImageData {
        data: Blob::from(vec![0_u8; width as usize * height as usize * 4]),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width,
        height,
    };
    // Image pixels run top to bottom over the y-up local rectangle.
    let image_to_local = kurbo::Affine::translate((area.x0, area.y1))
        * kurbo::Affine::scale_non_uniform(
            area.width() / f64::from(width),
            -area.height() / f64::from(height),
        );
    scene.fill(
        peniko::Fill::NonZero,
        kurbo::Affine::IDENTITY,
        &peniko::Brush::Image(peniko::ImageBrush::new(image.clone())),
        Some(image_to_local),
        &area,
    );
    let sigma_pixels = (sigma * f64::from(width) / area.width()) as f32;
    let [red, green, blue, alpha] = color.components;
    let gain = gain as f32;
    let pass = |horizontal: f32| {
        (
            shader.clone(),
            vec![sigma_pixels, horizontal, red, green, blue, alpha, gain],
        )
    };
    Some(EffectLayer {
        scene: Arc::new(silhouette),
        to_pixels: image_to_local.inverse(),
        image,
        request: PostProcessRequest {
            passes: vec![pass(1.0), pass(0.0)],
            frame: kurbo::Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
            time: 0.0,
            transition: None,
        },
        fixed: true,
    })
}

/// Opacity of a shadow where its silhouette is fully covered, relative to
/// its color's alpha, as the vector shadow's copies compose to.
const SHADOW_CORE: f64 = 0.97;

/// The blurred drop shadow of a fragment as an image: `silhouette` is what
/// the caster paints, already shifted by the shadow's offset, within
/// `bounds`. `None` for a sharp shadow, which stays a plain fill.
pub(crate) fn draw_shadow_image(
    scene: &mut vello::Scene,
    silhouette: vello::Scene,
    bounds: kurbo::Rect,
    shadow: &DropShadow,
) -> Option<EffectLayer> {
    draw_soft_image(
        scene,
        silhouette,
        bounds,
        shadow.blur_radius,
        shadow.color,
        -(1.0 - SHADOW_CORE).ln(),
    )
}

/// Compositions a group shadow may go undrawn before it is forgotten.
const GROUP_SHADOW_RETENTION: u32 = 16;

/// A group shadow's image as last drawn for one run of members.
struct CachedGroupShadow {
    outline: kurbo::BezPath,
    shadow: DropShadow,
    /// Draws the image in the run's local coordinates.
    scene: Arc<vello::Scene>,
    layer: EffectLayer,
    idle: u32,
}

#[derive(Default)]
struct GroupShadows {
    /// Layers of the group shadows drawn since [`collect_group_shadows`],
    /// for a composition whose renderer runs object effects.
    collected: Option<Vec<EffectLayer>>,
    /// By the run's first member, so a run that keeps its outline keeps its
    /// image.
    cache: std::collections::HashMap<u64, CachedGroupShadow>,
}

thread_local! {
    static GROUP_SHADOWS: std::cell::RefCell<GroupShadows> =
        std::cell::RefCell::new(GroupShadows::default());
}

/// Start a composition whose renderer runs object effects: group shadows
/// drawn until [`take_group_shadows`] are images, see
/// [`group_shadow_image`]. Compositions on one thread take turns, as with
/// [`crate::object_effects::effect_image`].
pub(crate) fn collect_group_shadows() {
    GROUP_SHADOWS.with_borrow_mut(|shadows| shadows.collected = Some(Vec::new()));
}

/// End the composition [`collect_group_shadows`] started: the layers of the
/// group shadows it drew, each image once. Forgets the shadows this thread
/// has not drawn for [`GROUP_SHADOW_RETENTION`] compositions.
pub(crate) fn take_group_shadows() -> Vec<EffectLayer> {
    GROUP_SHADOWS.with_borrow_mut(|shadows| {
        shadows.cache.retain(|_, cached| {
            cached.idle += 1;
            cached.idle <= GROUP_SHADOW_RETENTION
        });
        let mut seen = std::collections::HashSet::new();
        shadows
            .collected
            .take()
            .unwrap_or_default()
            .into_iter()
            .filter(|layer| seen.insert(layer.image.data.id()))
            .collect()
    })
}

/// The shadow `outline` (in the run's local coordinates) casts, drawn as an
/// image, for the run whose first member is `key`; `None` outside a
/// composition that collects group shadows, or for a sharp shadow, which
/// the caller draws as vectors.
pub(crate) fn group_shadow_image(
    key: u64,
    outline: &kurbo::BezPath,
    shadow: &DropShadow,
) -> Option<Arc<vello::Scene>> {
    use kurbo::Shape;
    GROUP_SHADOWS.with_borrow_mut(|shadows| {
        let collected = shadows.collected.as_mut()?;
        if let Some(cached) = shadows.cache.get_mut(&key)
            && cached.shadow == *shadow
            && cached.outline == *outline
        {
            cached.idle = 0;
            collected.push(cached.layer.clone());
            return Some(Arc::clone(&cached.scene));
        }
        let offset = kurbo::Affine::translate((shadow.offset.x, shadow.offset.y));
        let mut silhouette = vello::Scene::new();
        silhouette.fill(
            peniko::Fill::NonZero,
            offset,
            peniko::Color::BLACK,
            None,
            outline,
        );
        let bounds = (offset * outline).bounding_box();
        let mut scene = vello::Scene::new();
        let layer = draw_shadow_image(&mut scene, silhouette, bounds, shadow)?;
        let scene = Arc::new(scene);
        collected.push(layer.clone());
        shadows.cache.insert(
            key,
            CachedGroupShadow {
                outline: outline.clone(),
                shadow: shadow.clone(),
                scene: Arc::clone(&scene),
                layer,
                idle: 0,
            },
        );
        Some(scene)
    })
}

/// Width of the line a glow blurs, as a share of its radius.
const GLOW_LINE: f64 = 0.15;
/// A glow's radius in sigmas of its blur: the vector glow's rings fade as
/// the cube of the distance left to the radius, about this Gaussian.
const GLOW_SIGMAS: f64 = 4.0;

/// The glow around `path` as an image, or `None` for a glow that draws
/// nothing (or a zoomed coordinate view's pen, which keeps the vector
/// glow).
pub(crate) fn draw_glow_image(
    scene: &mut vello::Scene,
    path: &kurbo::BezPath,
    glow: &Glow,
    view: Option<kurbo::Affine>,
) -> Option<EffectLayer> {
    use kurbo::Shape;
    if view.is_some()
        || path.elements().is_empty()
        || !(glow.radius.is_finite() && glow.radius > 0.0)
        || !(glow.intensity.is_finite() && glow.intensity > 0.0)
    {
        return None;
    }
    let width = glow.radius * GLOW_LINE;
    let sigma = glow.radius / GLOW_SIGMAS;
    let mut silhouette = vello::Scene::new();
    silhouette.stroke(
        &kurbo::Stroke::new(width),
        kurbo::Affine::IDENTITY,
        peniko::Color::BLACK,
        None,
        path,
    );
    let bounds = path.bounding_box().inflate(width, width);
    let gain = glow_depth(glow.intensity) / line_coverage(width, sigma);
    draw_soft_image(scene, silhouette, bounds, sigma, glow.color, gain)
}

/// Optical depth of a glow on its outline: the vector glow stacks rings
/// whose alphas, from the outline out to the radius, compose to
/// `1 - exp(-depth)`.
fn glow_depth(intensity: f32) -> f64 {
    const SAMPLES: u32 = 256;
    let intensity = f64::from(intensity);
    (0..SAMPLES)
        .map(|index| {
            let distance = (f64::from(index) + 0.5) / f64::from(SAMPLES);
            let falloff = 1.0 - distance;
            let alpha = (intensity * 0.18 * falloff * falloff).clamp(0.0, 0.999);
            -(1.0 - alpha).ln()
        })
        .sum::<f64>()
        * 7.0
        / f64::from(SAMPLES)
}

/// Coverage left on a straight line `width` wide after a Gaussian blur of
/// `sigma`, at its center.
fn line_coverage(width: f64, sigma: f64) -> f64 {
    erf(width / (2.0 * std::f64::consts::SQRT_2 * sigma)).max(1.0e-6)
}

/// The error function (Abramowitz and Stegun 7.1.26, within 1.5e-7).
fn erf(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.327_591_1 * x.abs());
    let poly = t
        * (0.254_829_592
            + t * (-0.284_496_736
                + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    let value = 1.0 - poly * (-x * x).exp();
    value.copysign(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soft_images_hold_three_texels_per_sigma_within_their_reach() {
        let mut scene = vello::Scene::new();
        let shadow = DropShadow {
            blur_radius: 0.1,
            ..DropShadow::default()
        };
        let bounds = kurbo::Rect::new(0.0, 0.0, 1.0, 0.5);
        let layer = draw_shadow_image(&mut scene, vello::Scene::new(), bounds, &shadow).unwrap();
        // 1.6 by 1.1 units with the reach, at 30 texels per unit.
        assert_eq!((layer.image.width, layer.image.height), (48, 33));
        assert!(layer.fixed);
        assert_eq!(layer.request.passes.len(), 2);
        let sharp = DropShadow {
            blur_radius: 0.0,
            ..DropShadow::default()
        };
        assert!(draw_shadow_image(&mut scene, vello::Scene::new(), bounds, &sharp).is_none());
    }

    #[test]
    fn large_soft_images_are_capped() {
        let mut scene = vello::Scene::new();
        let shadow = DropShadow {
            blur_radius: 0.001,
            ..DropShadow::default()
        };
        let bounds = kurbo::Rect::new(0.0, 0.0, 20.0, 1.0);
        let layer = draw_shadow_image(&mut scene, vello::Scene::new(), bounds, &shadow).unwrap();
        assert_eq!(layer.image.width, 2048);
    }

    #[test]
    fn a_group_shadow_keeps_its_image_while_its_outline_holds() {
        use kurbo::Shape;
        let outline = kurbo::Rect::new(0.0, 0.0, 2.0, 1.0).to_path(0.1);
        let shadow = DropShadow::default();
        // Outside a collecting composition the caller draws vectors.
        assert!(group_shadow_image(7, &outline, &shadow).is_none());

        collect_group_shadows();
        let first = group_shadow_image(7, &outline, &shadow).unwrap();
        let again = group_shadow_image(7, &outline, &shadow).unwrap();
        assert!(Arc::ptr_eq(&first, &again));
        let layers = take_group_shadows();
        assert_eq!(layers.len(), 1, "one image, drawn twice");

        collect_group_shadows();
        let moved = kurbo::Rect::new(0.0, 0.0, 3.0, 1.0).to_path(0.1);
        let changed = group_shadow_image(7, &moved, &shadow).unwrap();
        assert!(!Arc::ptr_eq(&first, &changed));
        let layers = take_group_shadows();
        assert_ne!(layers[0].image.data.id(), first_image_id(&first));
    }

    fn first_image_id(scene: &vello::Scene) -> u64 {
        scene
            .encoding()
            .resources
            .patches
            .iter()
            .find_map(|patch| match patch {
                vello_encoding::Patch::Image { image, .. } => Some(image.data.id()),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn glow_depth_matches_the_ring_stack() {
        assert!((erf(0.5) - 0.520_499_877_8).abs() < 1.0e-6);
        assert!((erf(-1.0) + 0.842_700_792_9).abs() < 1.0e-6);
        // About a third opaque on the outline at intensity 1.
        let alpha = 1.0 - (-glow_depth(1.0)).exp();
        assert!((0.3..0.4).contains(&alpha), "{alpha}");
        assert!(glow_depth(2.0) > glow_depth(1.0));
    }
}
