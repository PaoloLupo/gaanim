//! Custom WGSL post-processing of everything Vello draws inside the camera frame.

use bevy::prelude::{Entity, Resource};
use gaanim_animation::ScalarSource;
use gaanim_core::{ObjectId, kurbo};
use naga::valid::{Capabilities, ValidationFlags, Validator};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use vello::wgpu;

use crate::gpu_scope::ScopeCheck;

use crate::post_bloom::{
    self, BLOOM_COMPOSITE_PREAMBLE, BloomChain, BloomDispatch, BloomPipelines,
};

const SHADER_PREAMBLE: &str = r#"
struct GaanimPostParams {
    // Camera frame origin (xy) and size (zw) in target pixels.
    frame: vec4<f32>,
    // Target size (xy) and timeline seconds (z).
    canvas: vec4<f32>,
    // Processed pixel region: origin (xy) and size (zw).
    region: vec4<u32>,
}

@group(0) @binding(0)
var gaanim_post_source: texture_2d<f32>;

@group(0) @binding(1)
var gaanim_post_sampler: sampler;

@group(0) @binding(2)
var gaanim_post_output: texture_storage_2d<rgba8unorm, write>;

@group(0) @binding(3)
var<uniform> gaanim_post_params: GaanimPostParams;

fn gaanim_scene(uv: vec2<f32>) -> vec4<f32> {
    let frame = gaanim_post_params.frame;
    let pixel = clamp(
        frame.xy + uv * frame.zw,
        frame.xy + vec2<f32>(0.5),
        frame.xy + frame.zw - vec2<f32>(0.5),
    );
    return textureSampleLevel(
        gaanim_post_source,
        gaanim_post_sampler,
        pixel / gaanim_post_params.canvas.xy,
        0.0,
    );
}
"#;

const SHADER_ENTRY_POINT: &str = r#"
@compute @workgroup_size(8, 8, 1)
fn gaanim_apply_post(@builtin(global_invocation_id) id: vec3<u32>) {
    let region = gaanim_post_params.region;
    if (id.x >= region.z || id.y >= region.w) {
        return;
    }
    let frame = gaanim_post_params.frame;
    let pixel = vec2<f32>(region.xy + id.xy) + vec2<f32>(0.5);
    let uv = (pixel - frame.xy) / frame.zw;
    let color = gaanim_post(uv, frame.zw, gaanim_post_params.canvas.z);
    textureStore(gaanim_post_output, id.xy, clamp(color, vec4<f32>(0.0), vec4<f32>(1.0)));
}
"#;

const TRANSITION_PREAMBLE: &str = r#"
@group(0) @binding(7)
var gaanim_post_incoming: texture_2d<f32>;

@group(0) @binding(8)
var gaanim_post_above: texture_2d<f32>;

/// Eased progress of the transition, from 0 (outgoing) to 1 (incoming).
var<private> progress: f32;

fn gaanim_frame_sample(source: texture_2d<f32>, uv: vec2<f32>) -> vec4<f32> {
    let frame = gaanim_post_params.frame;
    let pixel = clamp(
        frame.xy + uv * frame.zw,
        frame.xy + vec2<f32>(0.5),
        frame.xy + frame.zw - vec2<f32>(0.5),
    );
    return textureSampleLevel(source, gaanim_post_sampler, pixel / gaanim_post_params.canvas.xy, 0.0);
}

/// The outgoing segment alone at `uv`.
fn gaanim_from(uv: vec2<f32>) -> vec4<f32> {
    return gaanim_frame_sample(gaanim_post_source, uv);
}

/// The incoming segment alone at `uv`.
fn gaanim_to(uv: vec2<f32>) -> vec4<f32> {
    return gaanim_frame_sample(gaanim_post_incoming, uv);
}

/// Camera frame size in pixels.
fn gaanim_resolution() -> vec2<f32> {
    return gaanim_post_params.frame.zw;
}

/// Absolute timeline seconds.
fn gaanim_time() -> f32 {
    return gaanim_post_params.canvas.z;
}
"#;

const TRANSITION_ENTRY_POINT: &str = r#"
@compute @workgroup_size(8, 8, 1)
fn gaanim_apply_post(@builtin(global_invocation_id) id: vec3<u32>) {
    let region = gaanim_post_params.region;
    if (id.x >= region.z || id.y >= region.w) {
        return;
    }
    progress = gaanim_post_params.canvas.w;
    let frame = gaanim_post_params.frame;
    let pixel = vec2<f32>(region.xy + id.xy) + vec2<f32>(0.5);
    let uv = (pixel - frame.xy) / frame.zw;
    let blended = clamp(transition(uv), vec4<f32>(0.0), vec4<f32>(1.0));
    // Drawables of neither segment and the overlays stay sharp above the
    // blend: straight-alpha source-over.
    let above = textureLoad(gaanim_post_above, vec2<i32>(region.xy + id.xy), 0);
    let alpha = above.a + blended.a * (1.0 - above.a);
    var rgb = vec3<f32>(0.0);
    if (alpha > 0.0) {
        rgb = (above.rgb * above.a + blended.rgb * blended.a * (1.0 - above.a)) / alpha;
    }
    textureStore(gaanim_post_output, id.xy, vec4<f32>(rgb, alpha));
}
"#;

const PARAMS_SIZE: u64 = 48;

/// Most named uniforms one post-process pass may declare.
pub const MAX_POST_UNIFORMS: usize = 32;

/// A WGSL function applied to the rendered 2D scene inside the camera frame.
///
/// `source` must define
/// `fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32>`
/// and may call `gaanim_scene(uv)` to sample the rendered scene. `uv` is
/// normalized with (0, 0) at the top-left corner of the camera frame,
/// `resolution` is the frame size in pixels and `time` is absolute timeline
/// seconds. Colors are straight-alpha sRGB values as stored in the target.
///
/// Declared uniforms are `f32` fields of `gaanim_uniforms` (for example
/// `gaanim_uniforms.amount`), and a shader built with data reads it from the
/// storage array `gaanim_data: array<vec4<f32>>`.
#[derive(Clone)]
pub struct PostProcessShader {
    source: Arc<str>,
    uniforms: Arc<[Arc<str>]>,
    data: Option<Arc<[[f32; 4]]>>,
    /// Whether the pass first builds a bloom mip chain from its input and
    /// reads it with `gaanim_bloom(uv)`.
    bloom: bool,
    /// Whether this is a scene transition: `transition(uv)` blends the
    /// outgoing and incoming segments, see [`Self::transition`].
    transition: bool,
    /// The complete module: the preamble, `source` and the entry point.
    complete: Arc<str>,
    /// Whether the output depends on the timeline time.
    time: TimeUse,
}

/// How a pass's output depends on the timeline time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TimeUse {
    Never,
    Always,
    /// Unless the uniform at this index is 0, as a preset's `animated`.
    UnlessZero(usize),
}

impl fmt::Debug for PostProcessShader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_struct("PostProcessShader");
        debug.field("source_len", &self.source.len());
        debug.field("uniforms", &self.uniforms);
        debug.field("data_len", &self.data.as_ref().map(|data| data.len()));
        if gaanim_core::fingerprint::identity_debug() {
            debug.field("source", &self.source);
        }
        debug.finish_non_exhaustive()
    }
}

impl PartialEq for PostProcessShader {
    fn eq(&self, other: &Self) -> bool {
        self.complete == other.complete && self.data == other.data
    }
}

impl PostProcessShader {
    pub fn new(source: impl Into<Arc<str>>) -> Result<Self, PostProcessError> {
        Self::with_uniforms(source, std::iter::empty::<&str>())
    }

    /// A shader that reads `f32` uniforms named `uniforms` from
    /// `gaanim_uniforms`, in declaration order.
    pub fn with_uniforms<N: AsRef<str>>(
        source: impl Into<Arc<str>>,
        uniforms: impl IntoIterator<Item = N>,
    ) -> Result<Self, PostProcessError> {
        Self::build(source.into(), uniforms, None, false, false)
    }

    /// A scene transition. `source` defines
    /// `fn transition(uv: vec2<f32>) -> vec4<f32>`, which may read
    /// `gaanim_from(uv)` (the outgoing segment alone), `gaanim_to(uv)` (the
    /// incoming one), the eased `progress` from 0 to 1, `gaanim_resolution()`
    /// and `gaanim_time()`, plus `gaanim_uniforms` and `gaanim_data` as a
    /// post-process pass does. `uv` has (0, 0) at the top-left corner of the
    /// camera frame. Drawables of neither segment are composited above.
    pub fn transition<N: AsRef<str>>(
        source: impl Into<Arc<str>>,
        uniforms: impl IntoIterator<Item = N>,
        data: Option<Arc<[[f32; 4]]>>,
    ) -> Result<Self, PostProcessError> {
        if data.as_ref().is_some_and(|data| data.is_empty()) {
            return Err(PostProcessError::InvalidWgsl(
                "transition data must not be empty".to_string(),
            ));
        }
        Self::build(source.into(), uniforms, data, false, true)
    }

    /// A shader that composites a bloom of its input, read with
    /// `gaanim_bloom(uv)` in linear light. The chain reads the uniforms
    /// `threshold` (sRGB brightness where glow starts) and `radius` (0..1
    /// spread), which `uniforms` must declare.
    pub(crate) fn with_bloom<N: AsRef<str>>(
        source: impl Into<Arc<str>>,
        uniforms: impl IntoIterator<Item = N>,
    ) -> Result<Self, PostProcessError> {
        let shader = Self::build(source.into(), uniforms, None, true, false)?;
        for name in ["threshold", "radius"] {
            if !shader.uniforms.iter().any(|uniform| &**uniform == name) {
                return Err(PostProcessError::InvalidUniforms(format!(
                    "a bloom pass must declare the uniform {name:?}"
                )));
            }
        }
        Ok(shader)
    }

    /// A shader that also reads `data` from `gaanim_data`, such as a lookup
    /// table. `data` must not be empty.
    pub fn with_data<N: AsRef<str>>(
        source: impl Into<Arc<str>>,
        uniforms: impl IntoIterator<Item = N>,
        data: impl Into<Arc<[[f32; 4]]>>,
    ) -> Result<Self, PostProcessError> {
        let data = data.into();
        if data.is_empty() {
            return Err(PostProcessError::InvalidWgsl(
                "post-process data must not be empty".to_string(),
            ));
        }
        Self::build(source.into(), uniforms, Some(data), false, false)
    }

    fn build<N: AsRef<str>>(
        source: Arc<str>,
        uniforms: impl IntoIterator<Item = N>,
        data: Option<Arc<[[f32; 4]]>>,
        bloom: bool,
        transition: bool,
    ) -> Result<Self, PostProcessError> {
        let uniforms = uniforms
            .into_iter()
            .map(|name| Arc::<str>::from(name.as_ref()))
            .collect::<Arc<[_]>>();
        validate_uniform_names(&uniforms)?;
        let complete: Arc<str> =
            complete_shader(&source, &uniforms, data.is_some(), bloom, transition).into();
        validate_post_source(&source, &complete, transition)?;
        let time = if transition || source_reads_time(&source) {
            TimeUse::Always
        } else {
            TimeUse::Never
        };
        Ok(Self {
            source,
            uniforms,
            data,
            bloom,
            transition,
            complete,
            time,
        })
    }

    /// The same shader, whose output depends on the time only while the
    /// uniform `uniform` is not 0.
    pub(crate) fn time_gated_by(mut self, uniform: &str) -> Self {
        if let Some(index) = self.uniforms.iter().position(|name| &**name == uniform) {
            self.time = TimeUse::UnlessZero(index);
        }
        self
    }

    /// Whether the pass's output, with these uniform values, depends on the
    /// timeline time. Frames whose passes do not can be reused while nothing
    /// else changes.
    pub fn reads_time(&self, uniforms: &[f32]) -> bool {
        match self.time {
            TimeUse::Never => false,
            TimeUse::Always => true,
            TimeUse::UnlessZero(index) => uniforms.get(index).is_none_or(|value| *value != 0.0),
        }
    }

    /// Load WGSL source from an asset file. Relative paths are resolved by the caller.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, PostProcessError> {
        Self::from_file_with_uniforms(path, std::iter::empty::<&str>())
    }

    /// [`Self::from_file`] with named uniforms, as [`Self::with_uniforms`].
    pub fn from_file_with_uniforms<N: AsRef<str>>(
        path: impl AsRef<Path>,
        uniforms: impl IntoIterator<Item = N>,
    ) -> Result<Self, PostProcessError> {
        let path = path.as_ref().to_path_buf();
        let source =
            std::fs::read_to_string(&path).map_err(|error| PostProcessError::ReadSource {
                path,
                message: error.to_string(),
            })?;
        Self::with_uniforms(source, uniforms)
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    /// Names of the declared uniforms, in declaration order.
    pub fn uniforms(&self) -> &[Arc<str>] {
        &self.uniforms
    }

    /// Storage data the shader reads as `gaanim_data`, e.g. a LUT.
    pub fn data(&self) -> Option<&Arc<[[f32; 4]]>> {
        self.data.as_ref()
    }

    /// Whether the pass composites a bloom of its input.
    pub fn bloom(&self) -> bool {
        self.bloom
    }

    /// Whether the shader is a scene transition.
    pub fn is_transition(&self) -> bool {
        self.transition
    }

    /// Rebuild a shader from the parts [`Self::source`], [`Self::uniforms`],
    /// [`Self::data`], [`Self::bloom`] and [`Self::is_transition`] return.
    pub fn from_parts(
        source: impl Into<Arc<str>>,
        uniforms: &[Arc<str>],
        data: Option<Arc<[[f32; 4]]>>,
        bloom: bool,
        transition: bool,
    ) -> Result<Self, PostProcessError> {
        Self::build(source.into(), uniforms.iter(), data, bloom, transition)
    }

    /// The transition a [`gaanim_scene::TransitionShader`] describes, built
    /// once per description and reused.
    pub fn for_transition(
        description: &Arc<gaanim_scene::TransitionShader>,
    ) -> Result<Self, PostProcessError> {
        type Built = Vec<(
            std::sync::Weak<gaanim_scene::TransitionShader>,
            PostProcessShader,
        )>;
        static BUILT: std::sync::Mutex<Built> = std::sync::Mutex::new(Vec::new());
        let mut built = BUILT
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        built.retain(|(weak, _)| weak.strong_count() > 0);
        if let Some((_, shader)) = built
            .iter()
            .find(|(weak, _)| std::ptr::eq(weak.as_ptr(), Arc::as_ptr(description)))
        {
            return Ok(shader.clone());
        }
        let shader = Self::transition(
            description.source.as_str(),
            description.uniforms.iter(),
            description
                .data
                .as_ref()
                .map(|data| Arc::<[[f32; 4]]>::from(data.as_slice())),
        )?;
        built.push((Arc::downgrade(description), shader.clone()));
        Ok(shader)
    }

    /// Value of the uniform `name` among `values`, given in declaration order.
    fn uniform_value(&self, values: &[f32], name: &str) -> Option<f32> {
        let index = self
            .uniforms
            .iter()
            .position(|uniform| &**uniform == name)?;
        values.get(index).copied()
    }
}

#[derive(Clone, Debug, Error)]
pub enum PostProcessError {
    #[error("could not read post-process WGSL asset '{path}': {message}")]
    ReadSource { path: PathBuf, message: String },
    #[error("invalid post-process WGSL: {0}")]
    InvalidWgsl(String),
    #[error("invalid post-process uniforms: {0}")]
    InvalidUniforms(String),
    #[error("invalid .cube LUT: {0}")]
    InvalidLut(String),
}

/// One pass of a post-process chain: a shader and the value of each of its
/// uniforms, in the shader's declaration order.
#[derive(Clone, Debug)]
pub struct PostProcessPass {
    pub shader: PostProcessShader,
    pub values: Vec<ScalarSource>,
}

impl PostProcessPass {
    /// A pass with constant uniforms.
    pub fn constant(shader: PostProcessShader, values: &[f64]) -> Result<Self, PostProcessError> {
        Self::new(
            shader,
            values.iter().copied().map(ScalarSource::constant).collect(),
        )
    }

    pub fn new(
        shader: PostProcessShader,
        values: Vec<ScalarSource>,
    ) -> Result<Self, PostProcessError> {
        if values.len() != shader.uniforms().len() {
            return Err(PostProcessError::InvalidUniforms(format!(
                "{} values for {} declared uniforms",
                values.len(),
                shader.uniforms().len()
            )));
        }
        Ok(Self { shader, values })
    }
}

impl From<PostProcessShader> for PostProcessPass {
    /// A pass whose uniforms, if any, are all zero.
    fn from(shader: PostProcessShader) -> Self {
        let values = vec![ScalarSource::constant(0.0); shader.uniforms().len()];
        Self { shader, values }
    }
}

/// Post-processing selected by one authored segment.
#[derive(Clone, Debug, Default)]
pub enum PostProcessOverride {
    /// Use the scene post-process.
    #[default]
    Inherit,
    /// Draw this segment without post-processing.
    Disabled,
    /// Replace the scene post-process while this segment is active.
    Passes(Vec<PostProcessPass>),
}

/// Post-process override and time range of one authored segment.
#[derive(Clone, Debug)]
pub struct SegmentPostProcess {
    pub start_time: f64,
    pub end_time: f64,
    /// A terminal stop keeps the outgoing segment active at its shared boundary.
    pub hold_at_end: bool,
    pub post: PostProcessOverride,
}

/// Scene post-process chain and per-segment overrides, inserted by scene
/// compilation.
#[derive(Resource, Clone, Debug, Default)]
pub struct CanvasPostProcess {
    pub passes: Vec<PostProcessPass>,
    pub segments: Vec<SegmentPostProcess>,
    /// Entities holding the signals of the parameters that uniforms read.
    pub parameters: Vec<(ObjectId, Entity)>,
}

impl CanvasPostProcess {
    /// Post-process chain active at an exact timeline position; empty when
    /// nothing applies.
    pub fn passes_at(&self, time_seconds: f64) -> &[PostProcessPass] {
        let segment = crate::pipeline::active_segment(&self.segments, time_seconds, |segment| {
            (segment.start_time, segment.end_time, segment.hold_at_end)
        });
        match segment.map(|segment| &segment.post) {
            None | Some(PostProcessOverride::Inherit) => &self.passes,
            Some(PostProcessOverride::Disabled) => &[],
            Some(PostProcessOverride::Passes(passes)) => passes,
        }
    }

    /// Frame to post-process at `time_seconds`, with the camera frame in
    /// target pixels, reading parameter uniforms through `signal` (the value
    /// of the entity holding a parameter's signal). `None` when nothing
    /// applies.
    pub fn request_with(
        &self,
        time_seconds: f64,
        frame: kurbo::Rect,
        mut signal: impl FnMut(Entity) -> Option<f64>,
    ) -> Option<PostProcessRequest> {
        let time = time_seconds as f32;
        if !time.is_finite() || !(frame.width() > 0.0 && frame.height() > 0.0) {
            return None;
        }
        let passes = self.passes_at(time_seconds);
        if passes.is_empty() {
            return None;
        }
        let mut resolve = |logical: ObjectId| {
            let entity = self
                .parameters
                .iter()
                .find_map(|(id, entity)| (*id == logical).then_some(*entity))?;
            signal(entity)
        };
        let passes = passes
            .iter()
            .map(|pass| {
                let values = pass
                    .values
                    .iter()
                    .map(|source| {
                        let value =
                            source.evaluate(time_seconds, &mut resolve).unwrap_or(0.0) as f32;
                        if value.is_finite() { value } else { 0.0 }
                    })
                    .collect();
                (pass.shader.clone(), values)
            })
            .collect();
        Some(PostProcessRequest {
            passes,
            frame,
            time,
            transition: None,
        })
    }

    /// [`Self::request_with`] for chains whose uniforms read no parameter.
    pub fn request(&self, time_seconds: f64, frame: kurbo::Rect) -> Option<PostProcessRequest> {
        self.request_with(time_seconds, frame, |_| None)
    }
}

/// One frame of post-processing for [`GpuPostProcess`].
#[derive(Clone, Debug)]
pub struct PostProcessRequest {
    /// Passes in order, each with its uniform values.
    pub passes: Vec<(PostProcessShader, Vec<f32>)>,
    /// Camera frame in target pixels (top-left origin).
    pub frame: kurbo::Rect,
    /// Timeline seconds.
    pub time: f32,
    /// A scene transition blended before the passes; the target then holds
    /// the outgoing segment, see [`TransitionInputs`].
    pub transition: Option<TransitionPass>,
}

impl PostProcessRequest {
    /// Whether this request turns a frame into the same pixels as `other`:
    /// the same passes and values in the same frame, and the same time
    /// unless no pass reads it. A transition always differs.
    pub fn same_output(&self, other: &Self) -> bool {
        self.frame == other.frame
            && self.transition.is_none()
            && other.transition.is_none()
            && self.passes.len() == other.passes.len()
            && self.passes.iter().zip(&other.passes).all(
                |((shader, values), (other_shader, other_values))| {
                    shader == other_shader && values == other_values
                },
            )
            && (self.time == other.time
                || !self
                    .passes
                    .iter()
                    .any(|(shader, values)| shader.reads_time(values)))
    }
}

/// The shader transition of one frame.
#[derive(Clone, Debug)]
pub struct TransitionPass {
    pub shader: PostProcessShader,
    pub values: Vec<f32>,
    /// Eased progress from 0 to 1.
    pub progress: f32,
}

impl PostProcessRequest {
    /// Add the shader transition of `frame`, starting an empty chain when
    /// `request` is `None`. A shader that fails to build is logged and left
    /// out, so the frame shows the outgoing segment.
    pub fn with_transition(
        request: Option<Self>,
        transition: Option<&gaanim_scene::TransitionShaderFrame>,
        frame: kurbo::Rect,
        time: f32,
    ) -> Option<Self> {
        let Some(transition) = transition else {
            return request;
        };
        let shader = match PostProcessShader::for_transition(&transition.shader) {
            Ok(shader) => shader,
            Err(error) => {
                tracing::error!("transition shader failed; cutting instead: {error}");
                return request;
            }
        };
        let mut request = request.unwrap_or(Self {
            passes: Vec::new(),
            frame,
            time,
            transition: None,
        });
        request.transition = Some(TransitionPass {
            shader,
            values: transition.values.clone(),
            progress: transition.progress,
        });
        Some(request)
    }
}

/// The extra textures a shader transition reads: the incoming segment and
/// the layer composited above the blend. Both match the target's size.
#[derive(Clone, Copy)]
pub struct TransitionInputs<'a> {
    pub incoming: &'a wgpu::Texture,
    pub above: &'a wgpu::Texture,
}

struct PostPipeline {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
}

struct PreparedPass {
    pipeline: Arc<PostPipeline>,
    bind_group: wgpu::BindGroup,
    /// Bloom chain stages recorded before the pass, if it composites one.
    bloom: Vec<BloomDispatch>,
}

struct PreparedFrame {
    passes: Vec<PreparedPass>,
    target: wgpu::Texture,
    /// Origin and size of the processed region in target pixels.
    region: [u32; 4],
}

/// Buffers of one pass, reused across frames while their sizes hold.
#[derive(Default)]
struct PassBuffers {
    uniforms: Option<wgpu::Buffer>,
    data: Option<(Arc<[[f32; 4]]>, wgpu::Buffer)>,
    bloom: Option<BloomChain>,
    /// The pass's bind group, kept while it binds the same resources.
    bind_group: Option<(PassBindings, wgpu::BindGroup)>,
}

/// What a pass's bind group binds besides the sampler and the parameters,
/// which last as long as the device.
struct PassBindings {
    pipeline: Arc<PostPipeline>,
    source: wgpu::TextureView,
    output: wgpu::TextureView,
    uniforms: Option<wgpu::Buffer>,
    data: Option<wgpu::Buffer>,
    bloom: Option<wgpu::TextureView>,
    /// The transition's incoming segment and layer above.
    inputs: Option<[wgpu::TextureView; 2]>,
}

impl PassBindings {
    fn matches(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.pipeline, &other.pipeline)
            && self.source == other.source
            && self.output == other.output
            && self.uniforms == other.uniforms
            && self.data == other.data
            && self.bloom == other.bloom
            && self.inputs == other.inputs
    }

    fn bind_group(
        &self,
        device: &wgpu::Device,
        sampler: &wgpu::Sampler,
        params: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        let mut entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&self.source),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&self.output),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: params.as_entire_binding(),
            },
        ];
        if let Some(buffer) = &self.uniforms {
            entries.push(wgpu::BindGroupEntry {
                binding: 4,
                resource: buffer.as_entire_binding(),
            });
        }
        if let Some(buffer) = &self.data {
            entries.push(wgpu::BindGroupEntry {
                binding: 5,
                resource: buffer.as_entire_binding(),
            });
        }
        if let Some(view) = &self.bloom {
            entries.push(wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::TextureView(view),
            });
        }
        if let Some([incoming, above]) = &self.inputs {
            entries.push(wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::TextureView(incoming),
            });
            entries.push(wgpu::BindGroupEntry {
                binding: 8,
                resource: wgpu::BindingResource::TextureView(above),
            });
        }
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gaanim-post-process-bind-group"),
            layout: &self.pipeline.layout,
            entries: &entries,
        })
    }
}

/// The view of `texture`, kept in `cache` while the texture stays the same.
fn cached_view(
    cache: &mut Option<(wgpu::Texture, wgpu::TextureView)>,
    texture: &wgpu::Texture,
) -> wgpu::TextureView {
    if let Some((cached, view)) = cache.as_ref()
        && cached == texture
    {
        return view.clone();
    }
    let view = texture.create_view(&Default::default());
    *cache = Some((texture.clone(), view.clone()));
    view
}

/// Applies a [`PostProcessRequest`] to a render target on the GPU.
///
/// Each pass writes a scratch texture the size of the camera frame, which is
/// then copied back over the frame, so the next pass reads its result; pixels
/// outside the frame are untouched. Views and bind groups are kept while the
/// textures they bind stay the same. Used by both the interactive render
/// world and the direct export.
#[derive(Default)]
pub struct GpuPostProcess {
    device: Option<wgpu::Device>,
    /// Pipelines by complete shader.
    pipelines: HashMap<Arc<str>, CachedPipeline>,
    sampler: Option<wgpu::Sampler>,
    params: Option<wgpu::Buffer>,
    buffers: Vec<PassBuffers>,
    bloom: Option<BloomPipelines>,
    scratch: Option<(wgpu::Texture, wgpu::TextureView)>,
    /// The last target and its view.
    target_view: Option<(wgpu::Texture, wgpu::TextureView)>,
    /// The last transition inputs (incoming, above) and their views.
    input_views: Option<([wgpu::Texture; 2], [wgpu::TextureView; 2])>,
    frame: Option<PreparedFrame>,
}

impl GpuPostProcess {
    /// Prepare `request` for `target`, which must be an `Rgba8Unorm` texture
    /// with `TEXTURE_BINDING` and `COPY_DST` usage. Returns whether
    /// [`Self::encode`] will draw a pass. A pass whose shader fails to build
    /// is skipped. A transition in `request` runs first, reading `target` as
    /// the outgoing segment and `inputs`; without `inputs` it is skipped.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::Texture,
        request: Option<&PostProcessRequest>,
        inputs: Option<TransitionInputs<'_>>,
    ) -> bool {
        self.frame = None;
        if self.device.as_ref() != Some(device) {
            *self = Self {
                device: Some(device.clone()),
                ..Self::default()
            };
        }
        let Some(request) = request else {
            self.release_bindings();
            return false;
        };
        let Some(region) = frame_region(request.frame, target.width(), target.height()) else {
            return false;
        };
        // The transition runs first, then the chain; without its inputs it
        // cannot run.
        let jobs: Vec<(&PostProcessShader, &[f32])> = request
            .transition
            .as_ref()
            .filter(|_| inputs.is_some())
            .map(|transition| (&transition.shader, transition.values.as_slice()))
            .into_iter()
            .chain(
                request
                    .passes
                    .iter()
                    .map(|(shader, values)| (shader, values.as_slice())),
            )
            .collect();
        // Keep only the pipelines of the current chain; a hot reload replaces them.
        self.pipelines
            .retain(|complete, _| jobs.iter().any(|(shader, _)| shader.complete == *complete));
        let pipelines: Vec<_> = jobs
            .iter()
            .map(|(shader, _)| self.pipeline(device, shader))
            .collect();
        if pipelines.iter().all(Option::is_none) {
            return false;
        }

        let output_view = match &self.scratch {
            Some((scratch, view))
                if scratch.width() == region[2] && scratch.height() == region[3] =>
            {
                view.clone()
            }
            _ => {
                let scratch = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("gaanim-post-process-scratch"),
                    size: wgpu::Extent3d {
                        width: region[2],
                        height: region[3],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                let view = scratch.create_view(&Default::default());
                self.scratch = Some((scratch, view.clone()));
                view
            }
        };
        let params = self
            .params
            .get_or_insert_with(|| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("gaanim-post-process-params"),
                    size: PARAMS_SIZE,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })
            .clone();
        let sampler = self
            .sampler
            .get_or_insert_with(|| {
                device.create_sampler(&wgpu::SamplerDescriptor {
                    label: Some("gaanim-post-process-sampler"),
                    address_mode_u: wgpu::AddressMode::ClampToEdge,
                    address_mode_v: wgpu::AddressMode::ClampToEdge,
                    mag_filter: wgpu::FilterMode::Linear,
                    min_filter: wgpu::FilterMode::Linear,
                    ..Default::default()
                })
            })
            .clone();
        queue.write_buffer(
            &params,
            0,
            &params_bytes(request, target.width(), target.height(), region),
        );

        let source_view = cached_view(&mut self.target_view, target);
        let input_views = inputs.map(|inputs| match &self.input_views {
            Some(([incoming, above], views))
                if incoming == inputs.incoming && above == inputs.above =>
            {
                views.clone()
            }
            _ => {
                let textures = [inputs.incoming.clone(), inputs.above.clone()];
                let views = textures
                    .each_ref()
                    .map(|texture| texture.create_view(&Default::default()));
                self.input_views = Some((textures, views.clone()));
                views
            }
        });
        self.buffers.resize_with(jobs.len(), PassBuffers::default);
        let mut passes = Vec::with_capacity(jobs.len());
        for (((shader, values), pipeline), buffers) in jobs
            .iter()
            .copied()
            .zip(pipelines)
            .zip(self.buffers.iter_mut())
        {
            let Some(pipeline) = pipeline else {
                continue;
            };
            let uniform_buffer = (!values.is_empty()).then(|| {
                let bytes = uniform_bytes(values);
                let buffer = match &buffers.uniforms {
                    Some(buffer) if buffer.size() == bytes.len() as u64 => buffer.clone(),
                    _ => {
                        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                            label: Some("gaanim-post-process-uniforms"),
                            size: bytes.len() as u64,
                            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                            mapped_at_creation: false,
                        });
                        buffers.uniforms = Some(buffer.clone());
                        buffer
                    }
                };
                queue.write_buffer(&buffer, 0, &bytes);
                buffer
            });
            let data_buffer = shader.data.as_ref().map(|data| match &buffers.data {
                Some((uploaded, buffer)) if Arc::ptr_eq(uploaded, data) => buffer.clone(),
                _ => {
                    let bytes: Vec<u8> = data
                        .iter()
                        .flatten()
                        .flat_map(|value| value.to_ne_bytes())
                        .collect();
                    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("gaanim-post-process-data"),
                        size: bytes.len() as u64,
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    queue.write_buffer(&buffer, 0, &bytes);
                    buffers.data = Some((data.clone(), buffer.clone()));
                    buffer
                }
            });
            let mut bloom = Vec::new();
            let bloom_view = shader.bloom.then(|| {
                let pipelines = self
                    .bloom
                    .get_or_insert_with(|| BloomPipelines::new(device));
                let mut chain = match buffers.bloom.take() {
                    Some(chain) if chain.fits(region[2], region[3]) => chain,
                    _ => BloomChain::new(device, region[2], region[3]),
                };
                bloom = chain.prepare(
                    device,
                    queue,
                    pipelines,
                    &sampler,
                    target,
                    region,
                    shader.uniform_value(values, "threshold").unwrap_or(0.8),
                    shader.uniform_value(values, "radius").unwrap_or(0.5),
                );
                let view = chain.result_view().clone();
                buffers.bloom = Some(chain);
                view
            });
            let inputs = if shader.transition {
                let Some(views) = &input_views else {
                    continue;
                };
                Some(views.clone())
            } else {
                None
            };
            let bindings = PassBindings {
                pipeline: pipeline.clone(),
                source: source_view.clone(),
                output: output_view.clone(),
                uniforms: uniform_buffer,
                data: data_buffer,
                bloom: bloom_view,
                inputs,
            };
            let bind_group = match &buffers.bind_group {
                Some((bound, bind_group)) if bound.matches(&bindings) => bind_group.clone(),
                _ => {
                    let bind_group = bindings.bind_group(device, &sampler, &params);
                    buffers.bind_group = Some((bindings, bind_group.clone()));
                    bind_group
                }
            };
            passes.push(PreparedPass {
                pipeline,
                bind_group,
                bloom,
            });
        }
        self.frame = Some(PreparedFrame {
            passes,
            target: target.clone(),
            region,
        });
        true
    }

    /// Drop the pending frame, keeping device resources for later frames.
    pub fn clear(&mut self) {
        self.frame = None;
        self.release_bindings();
    }

    /// Forget the cached views and bind groups, which keep the textures
    /// they bind alive, such as a canvas texture replaced by a resize.
    fn release_bindings(&mut self) {
        self.target_view = None;
        self.input_views = None;
        for buffers in &mut self.buffers {
            buffers.bind_group = None;
            if let Some(chain) = &mut buffers.bloom {
                chain.unbind();
            }
        }
    }

    /// Record the passes prepared by [`Self::prepare`], if any.
    pub fn encode(&self, encoder: &mut wgpu::CommandEncoder) {
        let (Some(frame), Some((scratch, _))) = (&self.frame, &self.scratch) else {
            return;
        };
        let [x, y, width, height] = frame.region;
        for pass in &frame.passes {
            if let Some(pipelines) = &self.bloom {
                post_bloom::encode(encoder, pipelines, &pass.bloom);
            }
            {
                let mut compute = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("gaanim-post-process-pass"),
                    timestamp_writes: None,
                });
                compute.set_pipeline(&pass.pipeline.pipeline);
                compute.set_bind_group(0, &pass.bind_group, &[]);
                compute.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
            }
            // The next pass samples the target, so each result lands there.
            encoder.copy_texture_to_texture(
                scratch.as_image_copy(),
                wgpu::TexelCopyTextureInfo {
                    texture: &frame.target,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x, y, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    fn pipeline(
        &mut self,
        device: &wgpu::Device,
        shader: &PostProcessShader,
    ) -> Option<Arc<PostPipeline>> {
        if !self.pipelines.contains_key(&shader.complete) {
            let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let pipeline = Arc::new(PostPipeline::new(
                device,
                &shader.complete,
                !shader.uniforms.is_empty(),
                shader.data.is_some(),
                shader.bloom,
                shader.transition,
            ));
            let cached = match crate::gpu_scope::check(error_scope) {
                ScopeCheck::Pending(scope) => CachedPipeline::Checking(pipeline, scope),
                outcome => CachedPipeline::Ready(validated(pipeline, outcome)),
            };
            self.pipelines.insert(shader.complete.clone(), cached);
        }
        let cached = self.pipelines.get_mut(&shader.complete)?;
        if let CachedPipeline::Checking(pipeline, scope) = cached {
            let outcome = scope.poll()?;
            let pipeline = validated(pipeline.clone(), outcome);
            *cached = CachedPipeline::Ready(pipeline);
        }
        match cached {
            CachedPipeline::Ready(pipeline) => pipeline.clone(),
            CachedPipeline::Checking(..) => None,
        }
    }
}

/// A post-process pipeline as built.
enum CachedPipeline {
    /// `None` records a failed build.
    Ready(Option<Arc<PostPipeline>>),
    /// Built, and WebGPU has not validated it yet: the pass is skipped until then.
    Checking(Arc<PostPipeline>, crate::gpu_scope::PendingScope),
}

fn validated(pipeline: Arc<PostPipeline>, outcome: ScopeCheck) -> Option<Arc<PostPipeline>> {
    match outcome {
        ScopeCheck::Valid => Some(pipeline),
        ScopeCheck::Invalid(error) => {
            tracing::error!("post-process shader failed; drawing without it: {error}");
            None
        }
        ScopeCheck::Pending(_) => None,
    }
}

impl PostPipeline {
    fn new(
        device: &wgpu::Device,
        complete: &str,
        uniforms: bool,
        data: bool,
        bloom: bool,
        transition: bool,
    ) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gaanim-post-process-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(complete)),
        });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty,
            count: None,
        };
        let uniform = wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let mut entries = vec![
            entry(
                0,
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
            ),
            entry(
                1,
                wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            ),
            entry(
                2,
                wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
            ),
            entry(3, uniform),
        ];
        if uniforms {
            entries.push(entry(4, uniform));
        }
        if data {
            entries.push(entry(
                5,
                wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
            ));
        }
        let texture = wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        if bloom {
            entries.push(entry(6, texture));
        }
        if transition {
            entries.push(entry(7, texture));
            entries.push(entry(8, texture));
        }
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gaanim-post-process-layout"),
            entries: &entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gaanim-post-process-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("gaanim-post-process-pipeline"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("gaanim_apply_post"),
            compilation_options: Default::default(),
            cache: None,
        });
        Self { layout, pipeline }
    }
}

fn frame_region(frame: kurbo::Rect, width: u32, height: u32) -> Option<[u32; 4]> {
    let x0 = frame.x0.round().clamp(0.0, f64::from(width));
    let y0 = frame.y0.round().clamp(0.0, f64::from(height));
    let x1 = frame.x1.round().clamp(0.0, f64::from(width));
    let y1 = frame.y1.round().clamp(0.0, f64::from(height));
    (x1 > x0 && y1 > y0).then_some([x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32])
}

fn params_bytes(
    request: &PostProcessRequest,
    width: u32,
    height: u32,
    region: [u32; 4],
) -> [u8; PARAMS_SIZE as usize] {
    let frame = request.frame;
    let floats = [
        frame.x0 as f32,
        frame.y0 as f32,
        frame.width() as f32,
        frame.height() as f32,
        width as f32,
        height as f32,
        request.time,
        request
            .transition
            .as_ref()
            .map_or(0.0, |transition| transition.progress),
    ];
    let mut bytes = [0_u8; PARAMS_SIZE as usize];
    for (chunk, value) in bytes.as_chunks_mut::<4>().0.iter_mut().zip(floats) {
        chunk.copy_from_slice(&value.to_ne_bytes());
    }
    for (chunk, value) in bytes[32..].as_chunks_mut::<4>().0.iter_mut().zip(region) {
        chunk.copy_from_slice(&value.to_ne_bytes());
    }
    bytes
}

/// Uniform values as `f32` fields, padded to the 16-byte struct alignment.
fn uniform_bytes(values: &[f32]) -> Vec<u8> {
    let mut bytes: Vec<u8> = values
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect();
    bytes.resize(bytes.len().div_ceil(16) * 16, 0);
    bytes
}

fn complete_shader(
    source: &str,
    uniforms: &[Arc<str>],
    data: bool,
    bloom: bool,
    transition: bool,
) -> String {
    let mut declarations = String::new();
    if !uniforms.is_empty() {
        declarations.push_str("struct GaanimUniforms {\n");
        for name in uniforms {
            declarations.push_str(&format!("    {name}: f32,\n"));
        }
        declarations.push_str(
            "}\n\n@group(0) @binding(4)\nvar<uniform> gaanim_uniforms: GaanimUniforms;\n",
        );
    }
    if data {
        declarations.push_str(
            "\n@group(0) @binding(5)\nvar<storage, read> gaanim_data: array<vec4<f32>>;\n",
        );
    }
    if bloom {
        declarations.push_str(BLOOM_COMPOSITE_PREAMBLE);
    }
    if transition {
        declarations.push_str(TRANSITION_PREAMBLE);
        return format!("{SHADER_PREAMBLE}\n{declarations}\n{source}\n{TRANSITION_ENTRY_POINT}");
    }
    format!("{SHADER_PREAMBLE}\n{declarations}\n{source}\n{SHADER_ENTRY_POINT}")
}

fn validate_uniform_names(names: &[Arc<str>]) -> Result<(), PostProcessError> {
    if names.len() > MAX_POST_UNIFORMS {
        return Err(PostProcessError::InvalidUniforms(format!(
            "at most {MAX_POST_UNIFORMS} uniforms per pass, got {}",
            names.len()
        )));
    }
    for (index, name) in names.iter().enumerate() {
        let mut chars = name.chars();
        let valid = chars
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
            && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            && !name.starts_with("__")
            && name.as_ref() != "_";
        if !valid {
            return Err(PostProcessError::InvalidUniforms(format!(
                "{name:?} is not a WGSL identifier (letters, digits and '_', not starting with a digit)"
            )));
        }
        if names[..index].contains(name) {
            return Err(PostProcessError::InvalidUniforms(format!(
                "uniform {name:?} is declared twice"
            )));
        }
    }
    Ok(())
}

/// Whether `source` may read the timeline time: through `gaanim_time()`, or
/// through the `time` parameter of `gaanim_post` named anywhere but in its
/// declaration. Comments or another identifier of the same name count too,
/// which only costs a frame that could have been reused.
fn source_reads_time(source: &str) -> bool {
    let words = |name: &str| {
        let is_identifier = |c: char| c.is_ascii_alphanumeric() || c == '_';
        source
            .match_indices(name)
            .filter(|&(at, _)| {
                let before = source[..at].chars().next_back();
                let after = source[at + name.len()..].chars().next();
                !before.is_some_and(is_identifier) && !after.is_some_and(is_identifier)
            })
            .count()
    };
    if words("gaanim_time") > 0 {
        return true;
    }
    let parameter = source.find("fn gaanim_post").and_then(|start| {
        let signature = &source[start..];
        let open = signature.find('(')?;
        let close = open + signature[open..].find(')')?;
        let name = signature[open + 1..close]
            .split(',')
            .nth(2)?
            .split(':')
            .next()?;
        Some(name.trim())
    });
    match parameter {
        Some(name) if !name.is_empty() => words(name) > 1,
        _ => true,
    }
}

fn validate_post_source(
    source: &str,
    complete: &str,
    transition: bool,
) -> Result<(), PostProcessError> {
    if transition && !source.contains("fn transition") {
        return Err(PostProcessError::InvalidWgsl(
            "a transition must define fn transition(uv: vec2<f32>) -> vec4<f32>".to_string(),
        ));
    }
    if !transition && !source.contains("gaanim_post") {
        return Err(PostProcessError::InvalidWgsl(
            "source must define gaanim_post(uv, resolution, time)".to_string(),
        ));
    }
    let module = naga::front::wgsl::parse_str(complete)
        .map_err(|error| PostProcessError::InvalidWgsl(error.emit_to_string(complete)))?;
    Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .map_err(|error| PostProcessError::InvalidWgsl(error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::background::test_gpu::test_gpu;

    #[test]
    fn passes_know_whether_they_read_the_time() {
        let shader = |body: &str| {
            PostProcessShader::with_uniforms(
                format!("fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, {body}"),
                ["animated"],
            )
            .unwrap()
        };
        let still = shader("time: f32) -> vec4<f32> { return gaanim_scene(uv); }");
        assert!(!still.reads_time(&[1.0]));
        let moving = shader("time: f32) -> vec4<f32> { return gaanim_scene(uv) * sin(time); }");
        assert!(moving.reads_time(&[0.0]));
        let renamed = shader("t: f32) -> vec4<f32> { return gaanim_scene(uv + vec2<f32>(t)); }");
        assert!(renamed.reads_time(&[0.0]));
        assert!(source_reads_time(
            "fn gaanim_post(uv: vec2<f32>, r: vec2<f32>, _t: f32) -> vec4<f32> { return vec4<f32>(gaanim_time()); }"
        ));
        let gated = moving.clone().time_gated_by("animated");
        assert!(!gated.reads_time(&[0.0]) && gated.reads_time(&[1.0]));

        let frame = kurbo::Rect::new(0.0, 0.0, 16.0, 9.0);
        let request = |shader: &PostProcessShader, time: f32| PostProcessRequest {
            passes: vec![(shader.clone(), vec![0.0])],
            frame,
            time,
            transition: None,
        };
        assert!(request(&still, 1.0).same_output(&request(&still, 2.0)));
        assert!(!request(&moving, 1.0).same_output(&request(&moving, 2.0)));
        assert!(request(&gated, 1.0).same_output(&request(&gated, 2.0)));
        assert!(!request(&still, 1.0).same_output(&request(&moving, 1.0)));
    }

    const INVERT: &str = "fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {\n\
         let color = gaanim_scene(uv + vec2<f32>(1.0, 0.0) / resolution);\n\
         return vec4<f32>(vec3<f32>(1.0) - color.rgb, color.a) + time * 0.0;\n}";

    fn shader() -> PostProcessShader {
        PostProcessShader::new(INVERT).unwrap()
    }

    #[test]
    fn validation_accepts_the_documented_contract() {
        assert!(shader().source().contains("gaanim_scene"));
    }

    #[test]
    fn validation_rejects_missing_or_mistyped_entry_functions() {
        assert!(matches!(
            PostProcessShader::new("fn other() {}"),
            Err(PostProcessError::InvalidWgsl(_))
        ));
        let wrong = "fn gaanim_post(uv: vec2<f32>) -> vec4<f32> { return gaanim_scene(uv); }";
        assert!(matches!(
            PostProcessShader::new(wrong),
            Err(PostProcessError::InvalidWgsl(_))
        ));
    }

    #[test]
    fn source_can_be_loaded_from_an_asset_file() {
        let path =
            std::env::temp_dir().join(format!("gaanim_post_process_{}.wgsl", std::process::id()));
        std::fs::write(&path, INVERT).unwrap();
        assert_eq!(
            PostProcessShader::from_file(&path).unwrap().source(),
            INVERT
        );
        std::fs::remove_file(&path).unwrap();
        assert!(matches!(
            PostProcessShader::from_file(&path),
            Err(PostProcessError::ReadSource { .. })
        ));
    }

    #[test]
    fn segments_inherit_disable_or_replace_the_scene_post_process() {
        let scene = shader();
        let other = PostProcessShader::new(
            "fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {\n\
             return gaanim_scene(uv);\n}",
        )
        .unwrap();
        let segment = |start, end, post| SegmentPostProcess {
            start_time: start,
            end_time: end,
            hold_at_end: false,
            post,
        };
        let post = CanvasPostProcess {
            passes: vec![scene.clone().into()],
            segments: vec![
                segment(0.0, 1.0, PostProcessOverride::Inherit),
                segment(1.0, 2.0, PostProcessOverride::Disabled),
                segment(
                    2.0,
                    3.0,
                    PostProcessOverride::Passes(vec![other.clone().into()]),
                ),
            ],
            parameters: Vec::new(),
        };
        let source_at = |time| {
            post.passes_at(time)
                .first()
                .map(|pass| pass.shader.source())
        };
        assert_eq!(source_at(0.5), Some(scene.source()));
        assert!(post.passes_at(1.5).is_empty());
        assert_eq!(source_at(2.5), Some(other.source()));
        assert_eq!(source_at(9.0), Some(scene.source()));

        let frame = kurbo::Rect::new(0.0, 0.0, 16.0, 9.0);
        assert!(post.request(1.5, frame).is_none());
        assert!(post.request(0.5, kurbo::Rect::ZERO).is_none());
        assert!(post.request(f64::NAN, frame).is_none());
        assert_eq!(post.request(0.5, frame).unwrap().time, 0.5);
    }

    #[test]
    fn frame_regions_are_clipped_to_the_target() {
        assert_eq!(
            frame_region(kurbo::Rect::new(-4.0, 2.4, 70.0, 20.6), 64, 36),
            Some([0, 2, 64, 19])
        );
        assert_eq!(
            frame_region(kurbo::Rect::new(80.0, 0.0, 90.0, 10.0), 64, 36),
            None
        );
    }

    #[test]
    fn gpu_pass_processes_only_the_camera_frame() {
        let Some(gpu) = test_gpu() else {
            eprintln!("skipped: no GPU adapter");
            return;
        };
        let (width, height) = (64_u32, 36_u32);
        // Vertical stripes: the red channel encodes the column.
        let pixels: Vec<u8> = (0..height)
            .flat_map(|_| (0..width).flat_map(|x| [(x * 4) as u8, 64, 200, 255]))
            .collect();
        let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
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
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        gpu.queue.write_texture(
            target.as_image_copy(),
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: None,
            },
            target.size(),
        );

        let frame = kurbo::Rect::new(8.0, 4.0, 40.0, 28.0);
        let request = CanvasPostProcess {
            passes: vec![shader().into()],
            ..Default::default()
        }
        .request(1.0, frame)
        .unwrap();
        let mut post = GpuPostProcess::default();
        assert!(post.prepare(&gpu.device, &gpu.queue, &target, Some(&request), None));
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        post.encode(&mut encoder);
        gpu.queue.submit(Some(encoder.finish()));
        let out = gpu.read(&target);

        let at = |x: u32, y: u32| &out[((y * width + x) * 4) as usize..][..4];
        // Outside the frame nothing changes.
        assert_eq!(at(2, 2), &[8, 64, 200, 255]);
        assert_eq!(at(50, 30), &[200, 64, 200, 255]);
        // The top-left frame pixel is uv ~ (0, 0) and reads its right-hand
        // neighbour (x = 9) through gaanim_scene, inverted.
        assert_eq!(at(8, 4), &[255 - 36, 255 - 64, 255 - 200, 255]);
        // The last column clamps its sample to the frame edge (x = 39).
        assert_eq!(at(39, 27), &[255 - 156, 255 - 64, 255 - 200, 255]);

        assert!(!post.prepare(&gpu.device, &gpu.queue, &target, None, None));
    }

    const MIX: &str = "fn transition(uv: vec2<f32>) -> vec4<f32> {\n\
         return mix(gaanim_from(uv), gaanim_to(uv), progress * gaanim_uniforms.gain);\n}";

    #[test]
    fn transitions_require_their_function_and_validate() {
        let shader = PostProcessShader::transition(MIX, ["gain"], None).unwrap();
        assert!(shader.is_transition());
        assert!(matches!(
            PostProcessShader::transition(INVERT, std::iter::empty::<&str>(), None),
            Err(PostProcessError::InvalidWgsl(_))
        ));
        assert!(matches!(
            PostProcessShader::transition(
                "fn transition(uv: vec2<f32>) -> f32 { return 1.0; }",
                std::iter::empty::<&str>(),
                None
            ),
            Err(PostProcessError::InvalidWgsl(_))
        ));
        let rebuilt =
            PostProcessShader::from_parts(shader.source(), shader.uniforms(), None, false, true)
                .unwrap();
        assert_eq!(rebuilt, shader);
    }

    #[test]
    fn gpu_transition_blends_both_segments_under_the_layer_above() {
        let Some(gpu) = test_gpu() else {
            return;
        };
        let (width, height) = (16_u32, 8_u32);
        let texture = |rgba: [u8; 4], left_only: bool| {
            let pixels: Vec<u8> = (0..height)
                .flat_map(|_| {
                    (0..width).flat_map(move |x| {
                        if left_only && x >= 4 {
                            [0, 0, 0, 0]
                        } else {
                            rgba
                        }
                    })
                })
                .collect();
            let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
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
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            gpu.queue.write_texture(
                texture.as_image_copy(),
                &pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: None,
                },
                texture.size(),
            );
            texture
        };
        let target = texture([200, 0, 0, 255], false);
        let incoming = texture([0, 0, 200, 255], false);
        let above = texture([0, 255, 0, 255], true);
        let shader = Arc::new(gaanim_scene::TransitionShader {
            source: MIX.to_string(),
            uniforms: vec!["gain".to_string()],
            values: vec![1.0],
            data: None,
        });
        let frame = kurbo::Rect::new(0.0, 0.0, 12.0, f64::from(height));
        let request = PostProcessRequest::with_transition(
            None,
            Some(&gaanim_scene::TransitionShaderFrame {
                values: shader.values.clone(),
                shader,
                progress: 0.25,
            }),
            frame,
            0.0,
        )
        .unwrap();
        let mut post = GpuPostProcess::default();
        assert!(post.prepare(
            &gpu.device,
            &gpu.queue,
            &target,
            Some(&request),
            Some(TransitionInputs {
                incoming: &incoming,
                above: &above,
            }),
        ));
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        post.encode(&mut encoder);
        gpu.queue.submit(Some(encoder.finish()));
        let out = gpu.read(&target);
        let at = |x: u32, y: u32| &out[((y * width + x) * 4) as usize..][..4];
        // The layer above covers the left columns.
        assert_eq!(at(1, 3), &[0, 255, 0, 255]);
        // A quarter of the way from the outgoing red to the incoming blue.
        assert_eq!(at(8, 3), &[150, 0, 50, 255]);
        // Outside the camera frame the outgoing segment stays.
        assert_eq!(at(14, 3), &[200, 0, 0, 255]);

        // Without its inputs the transition does not run.
        assert!(!post.prepare(&gpu.device, &gpu.queue, &target, Some(&request), None));
    }

    const TINT: &str = "fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {\n\
         let color = gaanim_scene(uv);\n\
         return vec4<f32>(color.r + gaanim_uniforms.red, color.g * gaanim_uniforms.green, color.b, color.a);\n}";

    #[test]
    fn uniforms_are_declared_fields_with_valid_names() {
        let tint = PostProcessShader::with_uniforms(TINT, ["red", "green"]).unwrap();
        assert_eq!(tint.uniforms().len(), 2);
        // A field the shader reads must be declared.
        assert!(matches!(
            PostProcessShader::with_uniforms(TINT, ["red"]),
            Err(PostProcessError::InvalidWgsl(_))
        ));
        for bad in ["2x", "a-b", "", "_", "__x"] {
            assert!(
                matches!(
                    PostProcessShader::with_uniforms(TINT, [bad, "red", "green"]),
                    Err(PostProcessError::InvalidUniforms(_))
                ),
                "{bad:?}"
            );
        }
        assert!(matches!(
            PostProcessShader::with_uniforms(TINT, ["red", "green", "red"]),
            Err(PostProcessError::InvalidUniforms(_))
        ));
        let many: Vec<String> = (0..=MAX_POST_UNIFORMS).map(|i| format!("u{i}")).collect();
        assert!(matches!(
            PostProcessShader::with_uniforms(TINT, &many),
            Err(PostProcessError::InvalidUniforms(_))
        ));
        assert!(matches!(
            PostProcessPass::constant(tint.clone(), &[1.0]),
            Err(PostProcessError::InvalidUniforms(_))
        ));
        assert!(matches!(
            PostProcessShader::with_data(INVERT, std::iter::empty::<&str>(), Vec::new()),
            Err(PostProcessError::InvalidWgsl(_))
        ));
    }

    #[test]
    fn requests_evaluate_uniforms_from_parameter_signals() {
        let tint = PostProcessShader::with_uniforms(TINT, ["red", "green"]).unwrap();
        let parameter = ObjectId::from_raw(3);
        let entity = Entity::from_raw_u32(7).unwrap();
        let post = CanvasPostProcess {
            passes: vec![
                PostProcessPass::new(
                    tint,
                    vec![
                        ScalarSource::signal(parameter),
                        ScalarSource::constant(f64::NAN),
                    ],
                )
                .unwrap(),
                shader().into(),
            ],
            parameters: vec![(parameter, entity)],
            ..Default::default()
        };
        let frame = kurbo::Rect::new(0.0, 0.0, 16.0, 9.0);
        let request = post
            .request_with(0.5, frame, |held| (held == entity).then_some(0.25))
            .unwrap();
        assert_eq!(request.passes.len(), 2);
        // Non-finite values are sent as zero.
        assert_eq!(request.passes[0].1, [0.25, 0.0]);
        assert!(request.passes[1].1.is_empty());
        // Without the signal the uniform falls back to zero.
        assert_eq!(post.request(0.5, frame).unwrap().passes[0].1, [0.0, 0.0]);
    }

    #[test]
    fn gpu_chains_passes_and_reads_uniforms_and_data() {
        let Some(gpu) = test_gpu() else {
            eprintln!("skipped: no GPU adapter");
            return;
        };
        let (width, height) = (16_u32, 8_u32);
        let pixels: Vec<u8> = (0..width * height)
            .flat_map(|_| [40_u8, 200, 10, 255])
            .collect();
        let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
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
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        gpu.queue.write_texture(
            target.as_image_copy(),
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: None,
            },
            target.size(),
        );
        let tint = PostProcessShader::with_uniforms(TINT, ["red", "green"]).unwrap();
        let blue = PostProcessShader::with_data(
            "fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {\n\
             let color = gaanim_scene(uv);\n\
             return vec4<f32>(color.rg, gaanim_data[1].z, color.a);\n}",
            std::iter::empty::<&str>(),
            vec![[0.0; 4], [0.0, 0.0, 1.0, 0.0]],
        )
        .unwrap();
        let request = CanvasPostProcess {
            passes: vec![
                PostProcessPass::constant(tint, &[0.5, 0.5]).unwrap(),
                blue.into(),
                // The inversion reads what the tint and data passes wrote.
                shader().into(),
            ],
            ..Default::default()
        }
        .request(0.0, kurbo::Rect::new(0.0, 0.0, 16.0, 8.0))
        .unwrap();
        let mut post = GpuPostProcess::default();
        assert!(post.prepare(&gpu.device, &gpu.queue, &target, Some(&request), None));
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        post.encode(&mut encoder);
        gpu.queue.submit(Some(encoder.finish()));
        let out = gpu.read(&target);
        // red 40/255 + 0.5 = 168, green 200 * 0.5 = 100, blue from data = 255,
        // then inverted.
        let pixel = &out[..4];
        assert!((i32::from(pixel[0]) - (255 - 168)).abs() <= 1, "{pixel:?}");
        assert!((i32::from(pixel[1]) - (255 - 100)).abs() <= 1, "{pixel:?}");
        assert_eq!(pixel[2], 0);
        assert_eq!(pixel[3], 255);
    }

    #[test]
    fn gpu_bloom_spreads_light_around_bright_pixels_only() {
        let Some(gpu) = test_gpu() else {
            eprintln!("skipped: no GPU adapter");
            return;
        };
        let (width, height) = (256_u32, 64_u32);
        // Black with a white 8x8 square on the left and a dim gray one far to
        // the right, below the threshold.
        let pixels: Vec<u8> = (0..width * height)
            .flat_map(|index| {
                let (x, y) = (index % width, index / width);
                let inside = |x0: u32| (x0..x0 + 8).contains(&x) && (28..36).contains(&y);
                if inside(28) {
                    [255, 255, 255, 255]
                } else if inside(220) {
                    [90, 90, 90, 255]
                } else {
                    [0, 0, 0, 255]
                }
            })
            .collect();
        let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
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
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        gpu.queue.write_texture(
            target.as_image_copy(),
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: None,
            },
            target.size(),
        );
        let bloom = crate::post_presets::PostPreset::Bloom.shader();
        let request = CanvasPostProcess {
            passes: vec![PostProcessPass::constant(bloom, &[0.8, 1.0, 0.6]).unwrap()],
            ..Default::default()
        }
        .request(0.0, kurbo::Rect::new(0.0, 0.0, 256.0, 64.0))
        .unwrap();
        let mut post = GpuPostProcess::default();
        // Twice, to reuse the chain's textures on the second frame.
        for _ in 0..2 {
            assert!(post.prepare(&gpu.device, &gpu.queue, &target, Some(&request), None));
            let mut encoder = gpu.device.create_command_encoder(&Default::default());
            post.encode(&mut encoder);
            gpu.queue.submit(Some(encoder.finish()));
            gpu.queue.write_texture(
                target.as_image_copy(),
                &pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: None,
                },
                target.size(),
            );
        }
        assert!(post.prepare(&gpu.device, &gpu.queue, &target, Some(&request), None));
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        post.encode(&mut encoder);
        gpu.queue.submit(Some(encoder.finish()));
        let out = gpu.read(&target);
        let red = |x: u32, y: u32| out[((y * width + x) * 4) as usize];
        assert_eq!(red(31, 31), 255, "the bright square stays white");
        assert!(red(40, 32) > 20, "light spreads next to the bright square");
        assert!(red(40, 32) > red(50, 32), "and fades with distance");
        assert!(red(224, 32).abs_diff(90) <= 2, "dim pixels do not bloom");
        assert!(red(212, 32) <= 3, "nothing glows around dim pixels");
        assert!(red(252, 2) <= 3, "far corners stay dark");
    }
}
