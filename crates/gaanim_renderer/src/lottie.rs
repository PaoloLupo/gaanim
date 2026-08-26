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
    warnings: Vec<String>,
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
        let warnings = compatibility_warnings(&mut json);
        let composition =
            velato::Composition::from_json(json).map_err(|error| LottieError::Parse {
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

        let asset = Arc::new(Self {
            path: cache_key.clone(),
            composition,
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
        let rendered = self.renderer.render_to_vello_scene(
            &self.playback.asset.composition,
            frame,
            transform,
            1.0,
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

fn compatibility_warnings(json: &mut Value) -> Vec<String> {
    let mut warnings = BTreeMap::<&'static str, usize>::new();
    scan_value(json, &mut warnings);
    sanitize_layers(json.get_mut("layers"), &mut warnings);
    if let Some(assets) = json.get_mut("assets").and_then(Value::as_array_mut) {
        for asset in assets {
            sanitize_layers(asset.get_mut("layers"), &mut warnings);
        }
    }
    warnings
        .into_iter()
        .map(|(code, count)| format!("{code}: {count} unsupported occurrence(s) may be omitted"))
        .collect()
}

fn warn(warnings: &mut BTreeMap<&'static str, usize>, code: &'static str) {
    *warnings.entry(code).or_default() += 1;
}

fn scan_value(value: &Value, warnings: &mut BTreeMap<&'static str, usize>) {
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
            if matches!(object.get("ty").and_then(Value::as_str), Some("gf" | "gs")) {
                warn(warnings, "gradient_color_stops");
            }
            if matches!(object.get("ty").and_then(Value::as_str), Some("st" | "gs"))
                && object
                    .get("d")
                    .and_then(Value::as_array)
                    .is_some_and(|v| !v.is_empty())
            {
                warn(warnings, "stroke_dash");
            }
            if object.get("s").and_then(Value::as_bool) == Some(true)
                && (object.contains_key("x") || object.contains_key("y"))
            {
                warn(warnings, "split_position");
            }
            for child in object.values() {
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

fn sanitize_layers(layers: Option<&mut Value>, warnings: &mut BTreeMap<&'static str, usize>) {
    let Some(layers) = layers.and_then(Value::as_array_mut) else {
        return;
    };
    layers.retain_mut(|layer| match layer.get("ty").and_then(Value::as_u64) {
        Some(0 | 1 | 3 | 4) => {
            if layer.get("ty").and_then(Value::as_u64) == Some(4) {
                sanitize_shapes(layer.get_mut("shapes"), warnings);
            }
            true
        }
        Some(2) => {
            warn(warnings, "image_layer");
            true
        }
        Some(5) => {
            warn(warnings, "text_layer");
            false
        }
        _ => {
            warn(warnings, "unknown_layer");
            false
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
        }
        true
    });
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
