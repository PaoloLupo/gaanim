use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use bevy::prelude::*;
use gaanim_objects::prelude::ImageView;
use serde_json::Value;

static LOTTIE_CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<LottieAsset>>>> = OnceLock::new();
static WARNED_ASSETS: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();

#[derive(Debug, thiserror::Error)]
pub enum LottieError {
    #[error("could not read Lottie JSON '{path}': {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not parse Lottie JSON '{path}': {message}")]
    Parse { path: PathBuf, message: String },
    #[error("could not decode Lottie image asset '{asset_path}' referenced by '{path}': {source}")]
    ImageAsset {
        path: PathBuf,
        asset_path: PathBuf,
        #[source]
        source: Box<image::ImageError>,
    },
    #[error("Lottie image layer references missing asset '{asset_id}' in '{path}'")]
    MissingImageAsset { path: PathBuf, asset_id: String },
    #[error("Lottie JSON '{path}' has invalid dimensions, frame rate, or frame range")]
    InvalidMetadata { path: PathBuf },
    #[error("Lottie offset must be finite, non-negative, and before the end of the animation")]
    InvalidOffset,
    #[error("Lottie duration must be finite, positive, and contained in the source animation")]
    InvalidDuration,
    #[error("Lottie speed must be finite and positive")]
    InvalidSpeed,
}

/// One parsed Lottie composition shared by every drawable using the same file.
#[derive(Debug)]
pub struct LottieAsset {
    path: PathBuf,
    composition: velato::Composition,
    image_layers: Vec<LottieImageLayer>,
    solid_layers: Vec<LottieSolidLayer>,
    warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct LottieImageAssetSpec {
    directory: Option<String>,
    file_name: String,
    width: Option<f64>,
    height: Option<f64>,
    embedded: bool,
}

#[derive(Debug, Clone)]
struct LottieImageLayerSpec {
    precomposition: Option<String>,
    layer_index: usize,
    asset_id: String,
}

#[derive(Debug, Clone)]
struct LottieImageLayer {
    precomposition: Option<String>,
    layer_index: usize,
    image: vello::peniko::ImageBrush,
    width: f64,
    height: f64,
}

#[derive(Debug, Clone)]
struct LottieSolidLayer {
    precomposition: Option<String>,
    layer_index: usize,
    width: f64,
    height: f64,
    color: vello::peniko::Color,
}

impl LottieAsset {
    pub fn load(path: impl AsRef<Path>) -> Result<Arc<Self>, LottieError> {
        let requested = path.as_ref();
        let cache_key = requested
            .canonicalize()
            .unwrap_or_else(|_| requested.to_path_buf());
        let cache = LOTTIE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        if let Some(asset) = cache
            .lock()
            .expect("Lottie cache poisoned")
            .get(&cache_key)
            .cloned()
        {
            return Ok(asset);
        }

        let bytes = std::fs::read(&cache_key).map_err(|source| LottieError::Read {
            path: cache_key.clone(),
            source,
        })?;
        let mut json: Value =
            serde_json::from_slice(&bytes).map_err(|error| LottieError::Parse {
                path: cache_key.clone(),
                message: error.to_string(),
            })?;
        let image_assets = image_asset_specs(&json);
        let mut warnings = compatibility_warnings(&mut json);
        let image_layer_specs = image_layer_specs(&json);
        let solid_layers = solid_layer_specs(&json, &mut warnings);
        let composition = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            velato::Composition::from_json(json)
        }))
        .map_err(|panic| LottieError::Parse {
            path: cache_key.clone(),
            message: format!(
                "Velato could not safely convert this composition: {}",
                panic_message(panic)
            ),
        })?
        .map_err(|error| LottieError::Parse {
            path: cache_key.clone(),
            message: error.to_string(),
        })?;
        if composition.width == 0
            || composition.height == 0
            || !composition.frame_rate.is_finite()
            || composition.frame_rate <= 0.0
            || !composition.frames.start.is_finite()
            || !composition.frames.end.is_finite()
            || composition.frames.end <= composition.frames.start
        {
            return Err(LottieError::InvalidMetadata { path: cache_key });
        }
        let image_layers =
            load_image_layers(&cache_key, &image_assets, &image_layer_specs, &mut warnings)?;

        let asset = Arc::new(Self {
            path: cache_key.clone(),
            composition,
            image_layers,
            solid_layers,
            warnings,
        });
        cache
            .lock()
            .expect("Lottie cache poisoned")
            .insert(cache_key.clone(), asset.clone());

        if !asset.warnings.is_empty()
            && WARNED_ASSETS
                .get_or_init(|| Mutex::new(HashSet::new()))
                .lock()
                .expect("Lottie warning cache poisoned")
                .insert(cache_key)
        {
            for warning in &asset.warnings {
                eprintln!(
                    "[gaanim] Lottie warning for '{}': {warning}",
                    asset.path.display()
                );
            }
        }
        Ok(asset)
    }

    pub fn width(&self) -> usize {
        self.composition.width
    }

    pub fn height(&self) -> usize {
        self.composition.height
    }

    pub fn frame_rate(&self) -> f64 {
        self.composition.frame_rate
    }

    pub fn duration(&self) -> f64 {
        (self.composition.frames.end - self.composition.frames.start) / self.composition.frame_rate
    }

    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }
}

/// Timeline mapping and destination geometry for one Lottie drawable.
#[derive(Debug, Clone)]
pub struct LottiePlayback {
    pub asset: Arc<LottieAsset>,
    pub view: ImageView,
    pub scene_start: f64,
    pub source_offset: f64,
    pub source_duration: f64,
    pub looping: bool,
    pub speed: f64,
    pub active: bool,
}

impl LottiePlayback {
    pub fn new(
        asset: Arc<LottieAsset>,
        view: ImageView,
        source_offset: f64,
        duration: Option<f64>,
        looping: bool,
        speed: f64,
    ) -> Result<Self, LottieError> {
        let total = asset.duration();
        if !source_offset.is_finite() || source_offset < 0.0 || source_offset >= total {
            return Err(LottieError::InvalidOffset);
        }
        let source_duration = duration.unwrap_or(total - source_offset);
        if !source_duration.is_finite()
            || source_duration <= 0.0
            || source_offset + source_duration > total + 1e-9
        {
            return Err(LottieError::InvalidDuration);
        }
        if !speed.is_finite() || speed <= 0.0 {
            return Err(LottieError::InvalidSpeed);
        }
        Ok(Self {
            asset,
            view,
            scene_start: 0.0,
            source_offset,
            source_duration,
            looping,
            speed,
            active: false,
        })
    }

    pub fn source_frame(&self, scene_time: f64) -> f64 {
        let elapsed = if self.active {
            ((scene_time - self.scene_start).max(0.0) * self.speed).max(0.0)
        } else {
            0.0
        };
        let local = if self.looping {
            elapsed.rem_euclid(self.source_duration)
        } else {
            elapsed.min(self.source_duration)
        };
        let start = self.asset.composition.frames.start
            + self.source_offset * self.asset.composition.frame_rate;
        let end = start + self.source_duration * self.asset.composition.frame_rate;
        (start + local * self.asset.composition.frame_rate).min(end - 1e-7)
    }
}

/// Runtime Velato renderer and the last sampled Vello scene.
#[derive(Component)]
pub struct LottiePlayer {
    playback: LottiePlayback,
    renderer: velato::Renderer,
    scene: Arc<vello::Scene>,
    sampled_frame: f64,
}

#[derive(Clone, Copy)]
struct LottieLayerContext<'a> {
    precomposition: Option<&'a str>,
    layers: &'a [velato::model::Layer],
    frame: f64,
    global_transform: vello::kurbo::Affine,
    alpha: f64,
    next_layer_index: Option<usize>,
}

struct LottieRenderSink<'a> {
    scene: &'a mut vello::Scene,
    composition: &'a velato::Composition,
    image_layers: &'a [LottieImageLayer],
    solid_layers: &'a [LottieSolidLayer],
    frame: f64,
    root_transform: vello::kurbo::Affine,
    root_alpha: f64,
    layer_groups: Vec<Option<LottieLayerContext<'a>>>,
}

impl<'a> LottieRenderSink<'a> {
    fn current_context(&self) -> Option<LottieLayerContext<'a>> {
        if let Some(context) = self.layer_groups.last() {
            *context
        } else {
            Some(LottieLayerContext {
                precomposition: None,
                layers: &self.composition.layers,
                frame: self.frame,
                global_transform: self.root_transform,
                alpha: self.root_alpha,
                next_layer_index: None,
            })
        }
    }

    fn draw_image_layer(
        &mut self,
        context: LottieLayerContext<'a>,
        layer_index: usize,
        transform: vello::kurbo::Affine,
        alpha: f64,
    ) {
        let Some(image_layer) = self
            .image_layers
            .iter()
            .find(|image| {
                image.layer_index == layer_index
                    && image.precomposition.as_deref() == context.precomposition
            })
            .cloned()
        else {
            return;
        };
        let bounds = vello::kurbo::Rect::new(0.0, 0.0, image_layer.width, image_layer.height);
        let Some(mask_count) =
            self.push_content_layer(context, layer_index, bounds, transform, alpha)
        else {
            return;
        };

        let pixel_scale = vello::kurbo::Affine::scale_non_uniform(
            image_layer.width / image_layer.image.image.width as f64,
            image_layer.height / image_layer.image.image.height as f64,
        );
        self.scene
            .draw_image(&image_layer.image, transform * pixel_scale);
        self.pop_content_layer(mask_count);
    }

    fn draw_solid_layer(
        &mut self,
        context: LottieLayerContext<'a>,
        layer_index: usize,
        transform: vello::kurbo::Affine,
        alpha: f64,
    ) {
        let Some(solid) = self
            .solid_layers
            .iter()
            .find(|solid| {
                solid.layer_index == layer_index
                    && solid.precomposition.as_deref() == context.precomposition
            })
            .cloned()
        else {
            return;
        };
        let bounds = vello::kurbo::Rect::new(0.0, 0.0, solid.width, solid.height);
        let Some(mask_count) =
            self.push_content_layer(context, layer_index, bounds, transform, alpha)
        else {
            return;
        };
        self.scene.fill(
            vello::peniko::Fill::NonZero,
            transform,
            solid.color,
            None,
            &bounds,
        );
        self.pop_content_layer(mask_count);
    }

    fn push_content_layer(
        &mut self,
        context: LottieLayerContext<'a>,
        layer_index: usize,
        bounds: vello::kurbo::Rect,
        transform: vello::kurbo::Affine,
        alpha: f64,
    ) -> Option<usize> {
        let layer = context.layers.get(layer_index)?;
        if !layer.frames.contains(&context.frame) {
            return None;
        }

        let alpha = alpha.clamp(0.0, 1.0) as f32;
        if alpha <= 0.0 {
            return None;
        }
        let blend = layer
            .blend_mode
            .unwrap_or_else(|| vello::peniko::Mix::Normal.into());
        self.scene.push_layer(
            vello::peniko::Fill::NonZero,
            blend,
            alpha,
            transform,
            &bounds,
        );

        let mut mask_elements = Vec::new();
        for mask in &layer.masks {
            mask.geometry.evaluate(context.frame, &mut mask_elements);
            self.scene.push_clip_layer(
                vello::peniko::Fill::NonZero,
                transform,
                &mask_elements.as_slice(),
            );
            mask_elements.clear();
        }
        Some(layer.masks.len())
    }

    fn pop_content_layer(&mut self, mask_count: usize) {
        for _ in 0..mask_count {
            self.scene.pop_layer();
        }
        self.scene.pop_layer();
    }

    fn layer_transform(
        context: LottieLayerContext<'a>,
        layer_index: usize,
    ) -> Option<vello::kurbo::Affine> {
        let layer = context.layers.get(layer_index)?;
        let mut transform = layer.transform.evaluate(context.frame).into_owned();
        let mut parent_index = layer.parent;
        let mut count = 0_usize;
        while let Some(index) = parent_index {
            if count >= context.layers.len() {
                break;
            }
            let Some(parent) = context.layers.get(index) else {
                break;
            };
            parent_index = parent.parent;
            transform = parent.transform.evaluate(context.frame).into_owned() * transform;
            count += 1;
        }
        Some(context.global_transform * transform)
    }

    fn callback_layer_index(
        context: LottieLayerContext<'a>,
        callback_name: &str,
        callback_index: usize,
    ) -> Option<usize> {
        if let Some(end) = context.next_layer_index {
            // The pinned Velato revision reports index 0 for nested layers.
            // Follow its reverse traversal so duplicate layer names remain
            // distinguishable inside a precomposition instance.
            return context.layers[..end]
                .iter()
                .rposition(|layer| layer.name == callback_name);
        }
        if context
            .layers
            .get(callback_index)
            .is_some_and(|layer| layer.name == callback_name)
        {
            return Some(callback_index);
        }
        context
            .layers
            .iter()
            .rposition(|layer| layer.name == callback_name)
    }
}

impl velato::RenderSink for LottieRenderSink<'_> {
    fn push_layer(
        &mut self,
        blend: impl Into<vello::peniko::BlendMode>,
        alpha: f32,
        transform: vello::kurbo::Affine,
        shape: &impl vello::kurbo::Shape,
    ) {
        self.scene
            .push_layer(vello::peniko::Fill::NonZero, blend, alpha, transform, shape);
    }

    fn push_clip_layer(
        &mut self,
        transform: vello::kurbo::Affine,
        shape: &impl vello::kurbo::Shape,
    ) {
        self.scene
            .push_clip_layer(vello::peniko::Fill::NonZero, transform, shape);
    }

    fn pop_layer(&mut self) {
        self.scene.pop_layer();
    }

    fn draw(
        &mut self,
        stroke: Option<&velato::model::fixed::Stroke>,
        transform: vello::kurbo::Affine,
        brush: &velato::model::fixed::Brush,
        shape: &impl vello::kurbo::Shape,
    ) {
        if let Some(stroke) = stroke {
            self.scene.stroke(stroke, transform, brush, None, shape);
        } else {
            self.scene
                .fill(vello::peniko::Fill::NonZero, transform, brush, None, shape);
        }
    }

    fn begin_layer_group(&mut self, name: &str, index: usize) {
        let Some(context) = self.current_context() else {
            self.layer_groups.push(None);
            return;
        };
        let Some(index) = Self::callback_layer_index(context, name, index) else {
            self.layer_groups.push(None);
            return;
        };
        if let Some(Some(context)) = self.layer_groups.last_mut()
            && context.next_layer_index.is_some()
        {
            context.next_layer_index = Some(index);
        }
        let Some(layer) = context.layers.get(index) else {
            self.layer_groups.push(None);
            return;
        };
        let Some(transform) = Self::layer_transform(context, index) else {
            self.layer_groups.push(None);
            return;
        };
        let alpha = context.alpha * layer.opacity.evaluate(context.frame) / 100.0;
        self.draw_image_layer(context, index, transform, alpha);
        self.draw_solid_layer(context, index, transform, alpha);

        let child_context = if let velato::model::Content::Instance { name, .. } = &layer.content {
            self.composition
                .assets
                .get(name)
                .map(|layers| LottieLayerContext {
                    precomposition: Some(name),
                    layers,
                    frame: (context.frame - layer.start_frame) / layer.stretch,
                    global_transform: transform,
                    alpha,
                    next_layer_index: Some(layers.len()),
                })
        } else {
            None
        };
        self.layer_groups.push(child_context);
    }

    fn end_layer_group(&mut self) {
        self.layer_groups.pop();
    }
}

impl LottiePlayer {
    pub fn new(playback: LottiePlayback) -> Self {
        let sampled_frame = playback.source_frame(0.0);
        let mut player = Self {
            playback,
            renderer: velato::Renderer::new(),
            scene: Arc::new(vello::Scene::new()),
            sampled_frame,
        };
        player.render(sampled_frame);
        player
    }

    pub fn scene(&self) -> &Arc<vello::Scene> {
        &self.scene
    }

    fn render(&mut self, frame: f64) {
        let view = self.playback.view;
        let w2 = view.display_width * 0.5;
        let h2 = view.display_height * 0.5;
        let transform = vello::kurbo::Affine::new([
            view.scale_x,
            0.0,
            0.0,
            -view.scale_y,
            -view.source_x * view.scale_x - w2,
            view.source_y * view.scale_y + h2,
        ]);
        let mut rendered = vello::Scene::new();
        let mut sink = LottieRenderSink {
            scene: &mut rendered,
            composition: &self.playback.asset.composition,
            image_layers: &self.playback.asset.image_layers,
            solid_layers: &self.playback.asset.solid_layers,
            frame,
            root_transform: transform,
            root_alpha: 1.0,
            layer_groups: Vec::new(),
        };
        self.renderer.append(
            &self.playback.asset.composition,
            frame,
            transform,
            1.0,
            &mut sink,
        );
        let mut scene = vello::Scene::new();
        let clip = vello::kurbo::Rect::new(-w2, -h2, w2, h2);
        scene.push_clip_layer(
            vello::peniko::Fill::NonZero,
            vello::kurbo::Affine::IDENTITY,
            &clip,
        );
        scene.append(&rendered, None);
        scene.pop_layer();
        self.scene = Arc::new(scene);
        self.sampled_frame = frame;
    }
}

pub fn sample_lottie_system(
    playback_state: Option<Res<gaanim_animation::PlaybackState>>,
    mut players: Query<&mut LottiePlayer>,
) {
    let scene_time = playback_state
        .as_ref()
        .map_or(0.0, |state| state.current_time);
    for mut player in &mut players {
        let frame = player.playback.source_frame(scene_time);
        if frame.to_bits() != player.sampled_frame.to_bits() {
            player.render(frame);
        }
    }
}

pub fn clear_lottie_cache() {
    if let Some(cache) = LOTTIE_CACHE.get() {
        cache.lock().expect("Lottie cache poisoned").clear();
    }
    if let Some(warned) = WARNED_ASSETS.get() {
        warned
            .lock()
            .expect("Lottie warning cache poisoned")
            .clear();
    }
}

fn image_asset_specs(json: &Value) -> HashMap<String, LottieImageAssetSpec> {
    json.get("assets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|asset| asset.get("layers").is_none())
        .filter_map(|asset| {
            let id = asset.get("id")?.as_str()?.to_owned();
            let file_name = asset.get("p")?.as_str()?.to_owned();
            let embedded = asset
                .get("e")
                .is_some_and(|value| value.as_bool() == Some(true) || value.as_u64() == Some(1))
                || file_name.starts_with("data:");
            Some((
                id,
                LottieImageAssetSpec {
                    directory: asset.get("u").and_then(Value::as_str).map(str::to_owned),
                    file_name,
                    width: asset.get("w").and_then(Value::as_f64),
                    height: asset.get("h").and_then(Value::as_f64),
                    embedded,
                },
            ))
        })
        .collect()
}

fn image_layer_specs(json: &Value) -> Vec<LottieImageLayerSpec> {
    let mut specs = Vec::new();
    for_each_layer_set(json, |precomposition, layers| {
        specs.extend(
            layers
                .iter()
                .enumerate()
                .filter(|(_, layer)| {
                    layer.get("ty").and_then(Value::as_u64) == Some(2)
                        && layer.get("hd").and_then(Value::as_bool) != Some(true)
                })
                .filter_map(|(layer_index, layer)| {
                    Some(LottieImageLayerSpec {
                        precomposition: precomposition.map(str::to_owned),
                        layer_index,
                        asset_id: layer.get("refId")?.as_str()?.to_owned(),
                    })
                }),
        );
    });
    specs
}

fn solid_layer_specs(json: &Value, warnings: &mut Vec<String>) -> Vec<LottieSolidLayer> {
    let mut invalid_count = 0_usize;
    let mut solids = Vec::new();
    for_each_layer_set(json, |precomposition, layers| {
        solids.extend(
            layers
                .iter()
                .enumerate()
                .filter(|(_, layer)| {
                    layer.get("ty").and_then(Value::as_u64) == Some(1)
                        && layer.get("hd").and_then(Value::as_bool) != Some(true)
                })
                .filter_map(|(layer_index, layer)| {
                    let Some(width) = layer.get("sw").and_then(Value::as_f64) else {
                        invalid_count += 1;
                        return None;
                    };
                    let Some(height) = layer.get("sh").and_then(Value::as_f64) else {
                        invalid_count += 1;
                        return None;
                    };
                    let color = layer
                        .get("sc")
                        .and_then(Value::as_str)
                        .and_then(parse_solid_color);
                    if !width.is_finite() || width <= 0.0 || !height.is_finite() || height <= 0.0 {
                        invalid_count += 1;
                        return None;
                    }
                    let Some(color) = color else {
                        invalid_count += 1;
                        return None;
                    };
                    Some(LottieSolidLayer {
                        precomposition: precomposition.map(str::to_owned),
                        layer_index,
                        width,
                        height,
                        color,
                    })
                }),
        );
    });
    if invalid_count > 0 {
        warnings.push(format!(
            "invalid_solid_layer: {invalid_count} unsupported occurrence(s) may be omitted"
        ));
    }
    solids
}

fn for_each_layer_set(json: &Value, mut visit: impl FnMut(Option<&str>, &[Value])) {
    if let Some(layers) = json.get("layers").and_then(Value::as_array) {
        visit(None, layers);
    }
    let Some(assets) = json.get("assets").and_then(Value::as_array) else {
        return;
    };
    for asset in assets {
        let Some(precomposition) = asset.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(layers) = asset.get("layers").and_then(Value::as_array) else {
            continue;
        };
        visit(Some(precomposition), layers);
    }
}

fn parse_solid_color(value: &str) -> Option<vello::peniko::Color> {
    let hex = value.strip_prefix('#').unwrap_or(value);
    if hex.len() != 6 {
        return None;
    }
    let rgb = u32::from_str_radix(hex, 16).ok()?;
    Some(vello::peniko::Color::from_rgb8(
        (rgb >> 16) as u8,
        (rgb >> 8) as u8,
        rgb as u8,
    ))
}

fn load_image_layers(
    lottie_path: &Path,
    assets: &HashMap<String, LottieImageAssetSpec>,
    layers: &[LottieImageLayerSpec],
    warnings: &mut Vec<String>,
) -> Result<Vec<LottieImageLayer>, LottieError> {
    let mut image_layers = Vec::with_capacity(layers.len());
    let mut embedded_count = 0_usize;
    for layer in layers {
        let Some(asset) = assets.get(&layer.asset_id) else {
            return Err(LottieError::MissingImageAsset {
                path: lottie_path.to_path_buf(),
                asset_id: layer.asset_id.clone(),
            });
        };
        if asset.embedded {
            embedded_count += 1;
            continue;
        }

        let mut asset_path = lottie_path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf();
        if let Some(directory) = &asset.directory {
            asset_path.push(directory);
        }
        asset_path.push(&asset.file_name);
        let decoded = image::open(&asset_path).map_err(|source| LottieError::ImageAsset {
            path: lottie_path.to_path_buf(),
            asset_path: asset_path.clone(),
            source: Box::new(source),
        })?;
        let rgba = decoded.to_rgba8();
        let (pixel_width, pixel_height) = rgba.dimensions();
        let width = asset
            .width
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(pixel_width as f64);
        let height = asset
            .height
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(pixel_height as f64);
        let image = vello::peniko::ImageData {
            data: vello::peniko::Blob::from(rgba.into_raw()),
            format: vello::peniko::ImageFormat::Rgba8,
            alpha_type: vello::peniko::ImageAlphaType::Alpha,
            width: pixel_width,
            height: pixel_height,
        };
        image_layers.push(LottieImageLayer {
            precomposition: layer.precomposition.clone(),
            layer_index: layer.layer_index,
            image: vello::peniko::ImageBrush::new(image),
            width,
            height,
        });
    }
    if embedded_count > 0 {
        warnings.push(format!(
            "embedded_image_asset: {embedded_count} unsupported occurrence(s) may be omitted"
        ));
    }
    Ok(image_layers)
}

fn compatibility_warnings(json: &mut Value) -> Vec<String> {
    let mut warnings = BTreeMap::<&'static str, usize>::new();
    scan_value(json, &mut warnings);
    sanitize_layers(json.get_mut("layers"), &mut warnings);
    sanitize_assets(json.get_mut("assets"), &mut warnings);
    warnings
        .into_iter()
        .map(|(code, count)| format!("{code}: {count} unsupported occurrence(s) may be omitted"))
        .collect()
}

fn warn(warnings: &mut BTreeMap<&'static str, usize>, code: &'static str) {
    *warnings.entry(code).or_default() += 1;
}

fn scan_value(value: &mut Value, warnings: &mut BTreeMap<&'static str, usize>) {
    match value {
        Value::Object(object) => {
            if object.contains_key("tm") {
                warn(warnings, "time_remap");
            }
            if object
                .get("ef")
                .and_then(Value::as_array)
                .is_some_and(|v| !v.is_empty())
            {
                warn(warnings, "effects");
            }
            if object.contains_key("ti") || object.contains_key("to") {
                warn(warnings, "spatial_easing");
            }
            if matches!(object.get("ty").and_then(Value::as_str), Some("st" | "gs"))
                && object
                    .get("d")
                    .and_then(Value::as_array)
                    .is_some_and(|v| !v.is_empty())
            {
                warn(warnings, "stroke_dash");
            }
            if matches!(object.get("bm").and_then(Value::as_u64), Some(16 | 17)) {
                object.insert("bm".to_owned(), Value::from(0));
                warn(warnings, "blend_mode");
            }
            for child in object.values_mut() {
                scan_value(child, warnings);
            }
        }
        Value::Array(values) => {
            for child in values {
                scan_value(child, warnings);
            }
        }
        _ => {}
    }
}

fn sanitize_assets(assets: Option<&mut Value>, warnings: &mut BTreeMap<&'static str, usize>) {
    let Some(assets) = assets.and_then(Value::as_array_mut) else {
        return;
    };
    assets.retain_mut(|asset| {
        if asset.get("layers").and_then(Value::as_array).is_some() {
            sanitize_layers(asset.get_mut("layers"), warnings);
            true
        } else {
            // The pinned Velato converter panics on image assets. Image layers
            // are composed by Gaanim's RenderSink, so omit them only from the
            // JSON passed into Velato.
            false
        }
    });
}

fn sanitize_layers(layers: Option<&mut Value>, warnings: &mut BTreeMap<&'static str, usize>) {
    let Some(layers) = layers.and_then(Value::as_array_mut) else {
        return;
    };
    layers.retain_mut(|layer| {
        sanitize_transform(layer.get_mut("ks"), false, warnings);
        match layer.get("ty").and_then(Value::as_u64) {
            Some(0 | 1 | 3 | 4) => {
                if layer.get("ty").and_then(Value::as_u64) == Some(4) {
                    sanitize_shapes(layer.get_mut("shapes"), warnings);
                }
                true
            }
            Some(2) => true,
            Some(5) => {
                warn(warnings, "text_layer");
                false
            }
            _ => {
                warn(warnings, "unknown_layer");
                false
            }
        }
    });
}

fn sanitize_shapes(shapes: Option<&mut Value>, warnings: &mut BTreeMap<&'static str, usize>) {
    const SUPPORTED: &[&str] = &[
        "gr", "rc", "el", "tr", "st", "pb", "mm", "rp", "op", "fl", "tm", "sh", "gf", "gs", "tw",
        "rd", "sr",
    ];
    let Some(shapes) = shapes.and_then(Value::as_array_mut) else {
        return;
    };
    shapes.retain_mut(|shape| {
        let Some(kind) = shape.get("ty").and_then(Value::as_str) else {
            warn(warnings, "unknown_shape");
            return false;
        };
        if !SUPPORTED.contains(&kind) {
            warn(warnings, "unknown_shape");
            return false;
        }
        if kind == "gr" {
            sanitize_shapes(shape.get_mut("it"), warnings);
        } else if kind == "tr" {
            sanitize_transform(Some(shape), true, warnings);
        } else if matches!(kind, "gf" | "gs") {
            sanitize_gradient_stops(shape, warnings);
        }
        true
    });
}

fn sanitize_gradient_stops(shape: &mut Value, warnings: &mut BTreeMap<&'static str, usize>) {
    let Some(gradient) = shape.get_mut("g").and_then(Value::as_object_mut) else {
        warn(warnings, "gradient_color_stops");
        return;
    };
    let Some(color_count) = gradient
        .get("p")
        .and_then(Value::as_u64)
        .and_then(|count| usize::try_from(count).ok())
        .filter(|count| *count > 0)
    else {
        warn(warnings, "gradient_color_stops");
        return;
    };
    let Some(mut property) = gradient.get("k").cloned() else {
        warn(warnings, "gradient_color_stops");
        return;
    };

    let normalized_count = match normalize_gradient_property(&mut property, color_count) {
        Ok(count) => count,
        Err(()) => {
            warn(warnings, "gradient_color_stops");
            return;
        }
    };
    if let Some(normalized_count) = normalized_count {
        gradient.insert("p".to_owned(), Value::from(normalized_count));
        gradient.insert("k".to_owned(), property);
    }
}

fn normalize_gradient_property(
    property: &mut Value,
    color_count: usize,
) -> Result<Option<usize>, ()> {
    let values = property
        .get_mut("k")
        .and_then(Value::as_array_mut)
        .ok_or(())?;
    if values.iter().all(Value::is_number) {
        return normalize_gradient_values(values, color_count);
    }

    let mut normalized_count = None;
    for keyframe in values {
        let values = keyframe
            .get_mut("s")
            .and_then(Value::as_array_mut)
            .ok_or(())?;
        match normalize_gradient_values(values, color_count)? {
            Some(count) => {
                if normalized_count.is_some_and(|current| current != count) {
                    return Err(());
                }
                normalized_count = Some(count);
            }
            None if normalized_count.is_some() => return Err(()),
            None => {}
        }
    }
    Ok(normalized_count)
}

fn normalize_gradient_values(
    values: &mut Vec<Value>,
    color_count: usize,
) -> Result<Option<usize>, ()> {
    let raw = values
        .iter()
        .map(Value::as_f64)
        .collect::<Option<Vec<_>>>()
        .ok_or(())?;
    if raw.iter().any(|value| !value.is_finite()) || raw.len() < color_count.saturating_mul(4) {
        return Err(());
    }
    let (colors, alpha) = raw.split_at(color_count * 4);
    if alpha.is_empty() {
        return Ok(None);
    }
    if alpha.len() % 2 != 0 {
        return Err(());
    }

    let mut colors = colors
        .as_chunks::<4>()
        .0
        .iter()
        .map(|stop| (stop[0], [stop[1], stop[2], stop[3]]))
        .collect::<Vec<_>>();
    let mut alpha = alpha
        .as_chunks::<2>()
        .0
        .iter()
        .map(|stop| (stop[0], stop[1]))
        .collect::<Vec<_>>();
    colors.sort_by(|a, b| a.0.total_cmp(&b.0));
    alpha.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut offsets = colors
        .iter()
        .map(|(offset, _)| *offset)
        .chain(alpha.iter().map(|(offset, _)| *offset))
        .collect::<Vec<_>>();
    offsets.sort_by(f64::total_cmp);

    let mut normalized = Vec::with_capacity(offsets.len() * 6);
    for offset in &offsets {
        let color = interpolate_gradient_color(&colors, *offset).ok_or(())?;
        normalized.extend([*offset, color[0], color[1], color[2]]);
    }
    for offset in &offsets {
        let alpha = interpolate_gradient_scalar(&alpha, *offset).ok_or(())?;
        normalized.extend([*offset, alpha]);
    }
    *values = normalized.into_iter().map(Value::from).collect();
    Ok(Some(offsets.len()))
}

fn interpolate_gradient_color(stops: &[(f64, [f64; 3])], offset: f64) -> Option<[f64; 3]> {
    let (first_offset, first) = stops.first()?;
    if offset <= *first_offset {
        return Some(*first);
    }
    for pair in stops.windows(2) {
        let (start_offset, start) = pair[0];
        let (end_offset, end) = pair[1];
        if offset <= end_offset {
            let t = gradient_lerp_factor(start_offset, end_offset, offset);
            return Some([
                start[0] + (end[0] - start[0]) * t,
                start[1] + (end[1] - start[1]) * t,
                start[2] + (end[2] - start[2]) * t,
            ]);
        }
    }
    stops.last().map(|(_, color)| *color)
}

fn interpolate_gradient_scalar(stops: &[(f64, f64)], offset: f64) -> Option<f64> {
    let (first_offset, first) = *stops.first()?;
    if offset <= first_offset {
        return Some(first);
    }
    for pair in stops.windows(2) {
        let (start_offset, start) = pair[0];
        let (end_offset, end) = pair[1];
        if offset <= end_offset {
            let t = gradient_lerp_factor(start_offset, end_offset, offset);
            return Some(start + (end - start) * t);
        }
    }
    stops.last().map(|(_, value)| *value)
}

fn gradient_lerp_factor(start: f64, end: f64, value: f64) -> f64 {
    if start == end {
        0.0
    } else {
        ((value - start) / (end - start)).clamp(0.0, 1.0)
    }
}

fn sanitize_transform(
    transform: Option<&mut Value>,
    shape_transform: bool,
    warnings: &mut BTreeMap<&'static str, usize>,
) {
    let Some(transform) = transform.and_then(Value::as_object_mut) else {
        return;
    };

    let rotation = transform
        .get("r")
        .and_then(Value::as_object)
        .filter(|rotation| rotation.contains_key("z"))
        .and_then(|rotation| rotation.get("z"))
        .cloned();
    if let Some(rotation) = rotation {
        transform.insert("r".to_owned(), rotation);
        warn(warnings, "split_rotation");
    } else if !transform.contains_key("r") {
        if let Some(rotation) = transform.get("rz").cloned() {
            transform.insert("r".to_owned(), rotation);
            if transform.contains_key("rx")
                || transform.contains_key("ry")
                || transform.contains_key("or")
            {
                warn(warnings, "split_rotation");
            }
        } else {
            // Layer transforms default to zero rotation, but Velato's pinned
            // converter currently assumes that the property is always present.
            transform.insert("r".to_owned(), serde_json::json!({"a": 0, "k": 0.0}));
        }
    }

    if !shape_transform {
        return;
    }
    let Some(position) = transform.get_mut("p") else {
        return;
    };
    let Some(split) = position.as_object() else {
        return;
    };
    if split.get("s").and_then(Value::as_bool) != Some(true) {
        return;
    }

    let x = split.get("x").and_then(initial_scalar).unwrap_or(0.0);
    let y = split.get("y").and_then(initial_scalar).unwrap_or(0.0);
    let animated =
        split.get("x").is_some_and(animated_scalar) || split.get("y").is_some_and(animated_scalar);
    *position = serde_json::json!({"a": 0, "k": [x, y]});
    if animated {
        // Shape-group split positions are not yet animated by Velato. Preserve
        // their initial placement and report the approximation instead of panicking.
        warn(warnings, "animated_shape_split_position");
    }
}

fn animated_scalar(value: &Value) -> bool {
    value.get("k").and_then(Value::as_array).is_some()
}

fn initial_scalar(value: &Value) -> Option<f64> {
    let keyframes = value.get("k")?;
    if let Some(value) = keyframes.as_f64() {
        return Some(value);
    }
    let first = keyframes.as_array()?.first()?;
    if let Some(value) = first.as_f64() {
        return Some(value);
    }
    let start = first.get("s")?;
    start
        .as_f64()
        .or_else(|| start.as_array()?.first()?.as_f64())
}

fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = panic.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown importer panic".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset() -> Arc<LottieAsset> {
        Arc::new(LottieAsset {
            path: PathBuf::from("test.json"),
            composition: velato::Composition {
                frames: 10.0..70.0,
                frame_rate: 30.0,
                width: 100,
                height: 50,
                ..Default::default()
            },
            image_layers: Vec::new(),
            solid_layers: Vec::new(),
            warnings: Vec::new(),
        })
    }

    #[test]
    fn playback_maps_scene_time_to_source_frames() {
        let view = ImageView {
            source_x: 0.0,
            source_y: 0.0,
            source_width: 100.0,
            source_height: 50.0,
            display_width: 100.0,
            display_height: 50.0,
            scale_x: 1.0,
            scale_y: 1.0,
            quality: Default::default(),
        };
        let mut playback =
            LottiePlayback::new(asset(), view, 0.5, Some(1.0), false, 2.0).expect("valid playback");
        playback.active = true;
        playback.scene_start = 3.0;
        assert_eq!(playback.source_frame(3.0), 25.0);
        assert_eq!(playback.source_frame(3.25), 40.0);
        assert!(playback.source_frame(4.0) < 55.0);
    }

    #[test]
    fn unsupported_shapes_are_removed_with_a_warning() {
        let mut json = serde_json::json!({
            "layers": [{"ty": 4, "shapes": [{"ty": "el"}, {"ty": "zz"}]}]
        });
        let warnings = compatibility_warnings(&mut json);
        assert!(
            warnings
                .iter()
                .any(|warning| warning.starts_with("unknown_shape:"))
        );
        assert_eq!(json["layers"][0]["shapes"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn gradient_alpha_stops_are_preserved_for_static_and_animated_gradients() {
        let static_values = serde_json::json!([
            0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.5, 0.0, 1.0, 1.0
        ]);
        let animated_values =
            serde_json::json!([0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0.25, 1.0, 0.75, 0.0]);
        let mut json = serde_json::json!({
            "v": "5.7.4",
            "fr": 30.0,
            "ip": 0.0,
            "op": 30.0,
            "w": 100,
            "h": 100,
            "layers": [{
                "ddd": 0,
                "ty": 4,
                "ind": 1,
                "st": 0.0,
                "ip": 0.0,
                "op": 30.0,
                "ks": {
                    "a": {"a": 0, "k": [0.0, 0.0]},
                    "p": {"a": 0, "k": [0.0, 0.0]},
                    "s": {"a": 0, "k": [100.0, 100.0]},
                    "r": {"a": 0, "k": 0.0},
                    "o": {"a": 0, "k": 100.0}
                },
                "shapes": [
                    {
                        "ty": "rc",
                        "p": {"a": 0, "k": [50.0, 50.0]},
                        "s": {"a": 0, "k": [100.0, 100.0]},
                        "r": {"a": 0, "k": 0.0}
                    },
                    {
                        "ty": "gf",
                        "g": {"p": 2, "k": {"a": 0, "k": static_values}},
                        "s": {"a": 0, "k": [0.0, 50.0]},
                        "e": {"a": 0, "k": [100.0, 50.0]},
                        "t": 1,
                        "o": {"a": 0, "k": 100.0},
                        "r": 1
                    },
                    {
                        "ty": "gf",
                        "g": {"p": 2, "k": {"a": 1, "k": [
                            {"t": 0.0, "s": animated_values.clone()},
                            {"t": 30.0, "s": animated_values}
                        ]}},
                        "s": {"a": 0, "k": [0.0, 50.0]},
                        "e": {"a": 0, "k": [100.0, 50.0]},
                        "t": 1,
                        "o": {"a": 0, "k": 100.0},
                        "r": 1
                    }
                ]
            }]
        });

        let warnings = compatibility_warnings(&mut json);
        let static_gradient = &json["layers"][0]["shapes"][1]["g"];
        let animated_gradient = &json["layers"][0]["shapes"][2]["g"];
        let static_values = static_gradient["k"]["k"].as_array().unwrap();

        assert_eq!(static_gradient["p"], 5);
        assert_eq!(static_values.len(), 30);
        assert_eq!(static_values[8], 0.5);
        assert_eq!(static_values[9], 0.5);
        assert_eq!(static_values[10], 0.0);
        assert_eq!(static_values[11], 0.5);
        assert_eq!(static_values[25], 0.0);
        assert_eq!(animated_gradient["p"], 4);
        assert_eq!(
            animated_gradient["k"]["k"][0]["s"]
                .as_array()
                .unwrap()
                .len(),
            24
        );
        assert!(
            warnings
                .iter()
                .all(|warning| !warning.starts_with("gradient_color_stops:"))
        );
        assert!(velato::Composition::from_json(json).is_ok());
    }

    #[test]
    fn layer_split_position_is_supported_without_a_warning() {
        let mut json = serde_json::json!({
            "layers": [{
                "ty": 3,
                "ks": {
                    "p": {
                        "s": true,
                        "x": {"a": 0, "k": 10.0},
                        "y": {"a": 0, "k": 20.0}
                    },
                    "r": {"a": 0, "k": 0.0}
                }
            }]
        });

        let warnings = compatibility_warnings(&mut json);

        assert!(
            warnings
                .iter()
                .all(|warning| !warning.starts_with("split_position:"))
        );
    }

    #[test]
    fn known_velato_import_panics_are_sanitized() {
        let mut json = serde_json::json!({
            "v": "5.7.4",
            "fr": 30.0,
            "ip": 0.0,
            "op": 30.0,
            "w": 100,
            "h": 100,
            "assets": [{
                "id": "image_0",
                "w": 10,
                "h": 10,
                "u": "images/",
                "p": "dot.png",
                "e": 0
            }],
            "layers": [{
                "ddd": 0,
                "ty": 4,
                "ind": 1,
                "st": 0.0,
                "ip": 0.0,
                "op": 30.0,
                "bm": 16,
                "ks": {
                    "a": {"a": 0, "k": [0.0, 0.0]},
                    "p": {
                        "s": true,
                        "x": {"a": 0, "k": 50.0},
                        "y": {"a": 0, "k": 50.0}
                    },
                    "s": {"a": 0, "k": [100.0, 100.0]},
                    "o": {"a": 0, "k": 100.0}
                },
                "shapes": [{
                    "ty": "gr",
                    "it": [
                        {
                            "ty": "el",
                            "p": {"a": 0, "k": [0.0, 0.0]},
                            "s": {"a": 0, "k": [20.0, 20.0]}
                        },
                        {
                            "ty": "fl",
                            "c": {"a": 0, "k": [1.0, 0.0, 0.0, 1.0]},
                            "o": {"a": 0, "k": 100.0}
                        },
                        {
                            "ty": "tr",
                            "p": {
                                "s": true,
                                "x": {"a": 0, "k": 10.0},
                                "y": {"a": 0, "k": 20.0}
                            },
                            "a": {"a": 0, "k": [0.0, 0.0]},
                            "s": {"a": 0, "k": [100.0, 100.0]},
                            "r": {
                                "x": {"a": 0, "k": 0.0},
                                "y": {"a": 0, "k": 0.0},
                                "z": {"a": 0, "k": 15.0},
                                "or": {"a": 0, "k": [0.0, 0.0, 0.0]}
                            },
                            "o": {"a": 0, "k": 100.0}
                        }
                    ]
                }]
            }]
        });

        let warnings = compatibility_warnings(&mut json);
        assert_eq!(json["assets"].as_array().unwrap().len(), 0);
        let result = std::panic::catch_unwind(|| velato::Composition::from_json(json));

        assert!(result.is_ok(), "sanitized Lottie conversion must not panic");
        assert!(result.unwrap().is_ok(), "sanitized Lottie must parse");
        assert!(
            warnings
                .iter()
                .any(|warning| warning.starts_with("blend_mode:"))
        );
        assert!(
            warnings
                .iter()
                .any(|warning| warning.starts_with("split_rotation:"))
        );
    }

    #[test]
    fn external_image_layer_is_decoded_and_added_to_the_vello_scene() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "gaanim-lottie-image-{}-{unique}",
            std::process::id()
        ));
        let image_dir = root.join("images");
        std::fs::create_dir_all(&image_dir).unwrap();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]))
            .save(image_dir.join("dot.png"))
            .unwrap();
        let json_path = root.join("image.json");
        std::fs::write(
            &json_path,
            serde_json::to_vec(&serde_json::json!({
                "v": "5.7.4",
                "fr": 30.0,
                "ip": 0.0,
                "op": 30.0,
                "w": 100,
                "h": 100,
                "assets": [{
                    "id": "image_0",
                    "w": 20,
                    "h": 10,
                    "u": "images/",
                    "p": "dot.png",
                    "e": 0
                }],
                "layers": [{
                    "ddd": 0,
                    "ty": 2,
                    "ind": 1,
                    "st": 0.0,
                    "ip": 0.0,
                    "op": 30.0,
                    "refId": "image_0",
                    "ks": {
                        "a": {"a": 0, "k": [0.0, 0.0]},
                        "p": {"a": 0, "k": [10.0, 20.0]},
                        "s": {"a": 0, "k": [100.0, 100.0]},
                        "r": {"a": 0, "k": 0.0},
                        "o": {"a": 0, "k": 100.0}
                    }
                }]
            }))
            .unwrap(),
        )
        .unwrap();

        let asset = LottieAsset::load(&json_path).unwrap();
        assert_eq!(asset.image_layers.len(), 1);
        assert!(
            asset
                .warnings()
                .iter()
                .all(|warning| !warning.starts_with("image_layer:"))
        );
        let view = ImageView {
            source_x: 0.0,
            source_y: 0.0,
            source_width: 100.0,
            source_height: 100.0,
            display_width: 100.0,
            display_height: 100.0,
            scale_x: 1.0,
            scale_y: 1.0,
            quality: Default::default(),
        };
        let playback = LottiePlayback::new(asset, view, 0.0, None, false, 1.0).unwrap();
        let player = LottiePlayer::new(playback);
        assert_eq!(player.scene().encoding().resources.patches.len(), 1);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn image_and_solid_layers_inside_a_precomposition_are_rendered() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "gaanim-lottie-precomp-{}-{unique}",
            std::process::id()
        ));
        let image_dir = root.join("images");
        std::fs::create_dir_all(&image_dir).unwrap();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]))
            .save(image_dir.join("dot.png"))
            .unwrap();
        let json_path = root.join("precomp.json");
        let transform = serde_json::json!({
            "a": {"a": 0, "k": [0.0, 0.0]},
            "p": {"a": 0, "k": [10.0, 20.0]},
            "s": {"a": 0, "k": [100.0, 100.0]},
            "r": {"a": 0, "k": 0.0},
            "o": {"a": 0, "k": 100.0}
        });
        std::fs::write(
            &json_path,
            serde_json::to_vec(&serde_json::json!({
                "v": "5.7.4",
                "fr": 30.0,
                "ip": 0.0,
                "op": 30.0,
                "w": 100,
                "h": 100,
                "assets": [
                    {
                        "id": "image_0",
                        "w": 20,
                        "h": 10,
                        "u": "images/",
                        "p": "dot.png",
                        "e": 0
                    },
                    {
                        "id": "precomp_0",
                        "layers": [
                            {
                                "ddd": 0,
                                "ty": 1,
                                "ind": 1,
                                "nm": "Nested Layer",
                                "st": 0.0,
                                "ip": 0.0,
                                "op": 30.0,
                                "sw": 30.0,
                                "sh": 15.0,
                                "sc": "#336699",
                                "ks": transform.clone()
                            },
                            {
                                "ddd": 0,
                                "ty": 2,
                                "ind": 2,
                                "nm": "Nested Layer",
                                "st": 0.0,
                                "ip": 0.0,
                                "op": 30.0,
                                "refId": "image_0",
                                "ks": transform.clone()
                            }
                        ]
                    }
                ],
                "layers": [{
                    "ddd": 0,
                    "ty": 0,
                    "ind": 1,
                    "nm": "Container",
                    "st": 0.0,
                    "ip": 0.0,
                    "op": 30.0,
                    "sr": 1.0,
                    "w": 100.0,
                    "h": 100.0,
                    "refId": "precomp_0",
                    "ks": transform
                }]
            }))
            .unwrap(),
        )
        .unwrap();

        let asset = LottieAsset::load(&json_path).unwrap();
        assert_eq!(
            asset.image_layers[0].precomposition.as_deref(),
            Some("precomp_0")
        );
        assert_eq!(
            asset.solid_layers[0].precomposition.as_deref(),
            Some("precomp_0")
        );
        assert!(
            asset
                .warnings()
                .iter()
                .all(|warning| !warning.starts_with("nested_image_layer:"))
        );
        let view = ImageView {
            source_x: 0.0,
            source_y: 0.0,
            source_width: 100.0,
            source_height: 100.0,
            display_width: 100.0,
            display_height: 100.0,
            scale_x: 1.0,
            scale_y: 1.0,
            quality: Default::default(),
        };
        let without_custom_content = Arc::new(LottieAsset {
            path: json_path.clone(),
            composition: asset.composition.clone(),
            image_layers: Vec::new(),
            solid_layers: Vec::new(),
            warnings: Vec::new(),
        });
        let without_custom_content = LottiePlayer::new(
            LottiePlayback::new(without_custom_content, view, 0.0, None, false, 1.0).unwrap(),
        );
        let player =
            LottiePlayer::new(LottiePlayback::new(asset, view, 0.0, None, false, 1.0).unwrap());

        assert_eq!(player.scene().encoding().resources.patches.len(), 1);
        assert!(
            player.scene().encoding().draw_tags.len()
                > without_custom_content.scene().encoding().draw_tags.len()
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn solid_layer_is_added_to_the_vello_scene() {
        let mut json = serde_json::json!({
            "v": "5.7.4",
            "fr": 30.0,
            "ip": 0.0,
            "op": 30.0,
            "w": 100,
            "h": 100,
            "layers": [{
                "ddd": 0,
                "ty": 1,
                "ind": 1,
                "st": 0.0,
                "ip": 0.0,
                "op": 30.0,
                "sw": 20.0,
                "sh": 10.0,
                "sc": "#336699",
                "ks": {
                    "a": {"a": 0, "k": [0.0, 0.0]},
                    "p": {"a": 0, "k": [10.0, 20.0]},
                    "s": {"a": 0, "k": [100.0, 100.0]},
                    "r": {"a": 0, "k": 0.0},
                    "o": {"a": 0, "k": 50.0}
                }
            }]
        });

        let mut warnings = compatibility_warnings(&mut json);
        let solid_layers = solid_layer_specs(&json, &mut warnings);
        let composition = velato::Composition::from_json(json).unwrap();
        assert_eq!(solid_layers.len(), 1);
        assert!(warnings.is_empty());

        let view = ImageView {
            source_x: 0.0,
            source_y: 0.0,
            source_width: 100.0,
            source_height: 100.0,
            display_width: 100.0,
            display_height: 100.0,
            scale_x: 1.0,
            scale_y: 1.0,
            quality: Default::default(),
        };
        let without_solid = Arc::new(LottieAsset {
            path: PathBuf::from("solid.json"),
            composition: composition.clone(),
            image_layers: Vec::new(),
            solid_layers: Vec::new(),
            warnings: Vec::new(),
        });
        let without_solid = LottiePlayer::new(
            LottiePlayback::new(without_solid, view, 0.0, None, false, 1.0).unwrap(),
        );
        let with_solid = Arc::new(LottieAsset {
            path: PathBuf::from("solid.json"),
            composition,
            image_layers: Vec::new(),
            solid_layers,
            warnings: Vec::new(),
        });
        let with_solid = LottiePlayer::new(
            LottiePlayback::new(with_solid, view, 0.0, None, false, 1.0).unwrap(),
        );

        assert!(
            with_solid.scene().encoding().draw_tags.len()
                > without_solid.scene().encoding().draw_tags.len()
        );
    }

    #[test]
    fn derived_geometry_plugin_samples_lottie_for_headless_seek() {
        let view = ImageView {
            source_x: 0.0,
            source_y: 0.0,
            source_width: 100.0,
            source_height: 50.0,
            display_width: 100.0,
            display_height: 50.0,
            scale_x: 1.0,
            scale_y: 1.0,
            quality: Default::default(),
        };
        let mut playback =
            LottiePlayback::new(asset(), view, 0.0, None, false, 1.0).expect("valid playback");
        playback.active = true;
        let player = LottiePlayer::new(playback);
        let initial_frame = player.sampled_frame;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(gaanim_animation::PlaybackState::default())
            .add_plugins(crate::GaanimDerivedGeometryPlugin);
        let entity = app.world_mut().spawn(player).id();
        app.world_mut()
            .resource_mut::<gaanim_animation::PlaybackState>()
            .current_time = 1.0;
        app.update();

        let sampled_frame = app
            .world()
            .get::<LottiePlayer>(entity)
            .expect("Lottie player should remain spawned")
            .sampled_frame;
        assert_ne!(sampled_frame.to_bits(), initial_frame.to_bits());
    }
}
