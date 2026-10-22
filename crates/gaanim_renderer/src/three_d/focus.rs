//! The depth of field of 3D content (see [`gaanim_scene::DepthOfField`]):
//! each pixel of a frame's image blurs by the circle of confusion of what
//! it shows, a Gaussian of `aperture * |1 - focus / depth|` scene units at
//! most `max_blur`, `depth` being its distance along the view in units of
//! the camera's distance to its target, as a parallax layer's is. The image
//! is blurred across and then down, each pixel spreading over its
//! neighbors by its own blur (gathered at each of them); what lies behind a
//! pixel spreads over it no more than the pixel itself blurs, so a sharp
//! edge in front of a blurred background keeps its edge. Built the first
//! time a frame is out of focus, so other frames cost nothing more.

use std::num::NonZeroU64;

use gaanim_core::glam::{DMat4, DVec3};
use gaanim_math::Camera;
use vello::wgpu;
use wgpu::util::DeviceExt;

use super::COLOR_FORMAT;

/// The image blurred across, whose faint tails 8 bits would band.
const ACROSS_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// The blur and depth of what colors each pixel blurred across, the depth
/// as precise as the depth buffer's.
const SPREAD_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg32Float;
/// Bytes of a pass's uniforms (`Focus` in [`FOCUS_SHADER`]).
const FOCUS_BYTES: usize = 48;
/// The most taps on each side of a pixel.
const MAX_TAPS: f64 = 64.0;

/// How a frame's image is out of focus, as bits: what turns its depths
/// into distances along the view, the lens, and the taps of its blur.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct FocusDraw {
    /// The third and fourth rows of the inverse projection, applied to a
    /// depth (z and 1): the view's z and w.
    inverse: [u32; 4],
    /// The focus, the aperture and the largest blur, in units of the
    /// camera's distance to its target and in scene units, and the image's
    /// pixels per scene unit.
    lens: [u32; 4],
    /// The camera's distance to its target along the view, the step
    /// between taps in pixels, and the taps on each side.
    taps: [u32; 3],
}

impl FocusDraw {
    /// The depth of field `depth_of_field` of an image of `camera` drawn
    /// `pixels_per_unit` pixels per scene unit, whose content lies between
    /// the distances `nearest` and `farthest` along the view; `None` when it
    /// blurs nothing, as when the camera is not away from its target.
    pub(super) fn new(
        depth_of_field: &gaanim_scene::DepthOfField,
        camera: &Camera,
        pixels_per_unit: f64,
        (nearest, farthest): (f64, f64),
    ) -> Option<Self> {
        let forward = camera.rotation * DVec3::NEG_Z;
        let plane = forward.dot(camera.target - camera.position);
        // The blur grows away from the focus on either side: the content's
        // largest lies at its nearest or its farthest point.
        let blur_at = |distance: f64| depth_of_field.blur_at((distance / plane).max(1e-9));
        let largest = blur_at(nearest).max(blur_at(farthest)) * pixels_per_unit;
        let shows = depth_of_field.aperture > 0.0
            && depth_of_field.focus > 0.0
            && plane.is_finite()
            && plane > 1e-9
            && largest.is_finite()
            && largest >= 0.1;
        if !shows {
            return None;
        }
        let inverse: DMat4 = camera.projection_matrix().inverse();
        if !inverse.is_finite() {
            return None;
        }
        // A Gaussian's reach: three of its sigmas.
        let reach = 3.0 * largest;
        let step = (reach / MAX_TAPS).max(1.0);
        let taps = (reach / step).ceil();
        let bits = |value: f64| (value as f32).to_bits();
        Some(Self {
            inverse: [
                bits(inverse.z_axis.z),
                bits(inverse.w_axis.z),
                bits(inverse.z_axis.w),
                bits(inverse.w_axis.w),
            ],
            lens: [
                bits(depth_of_field.focus),
                bits(depth_of_field.aperture),
                bits(depth_of_field.max_blur),
                bits(pixels_per_unit),
            ],
            taps: [bits(plane), bits(step), bits(taps)],
        })
    }

    /// The uniforms of its passes.
    fn uniforms(&self) -> Vec<u8> {
        let float = f32::from_bits;
        let [plane, step, taps] = self.taps;
        let mut bytes = Vec::with_capacity(FOCUS_BYTES);
        super::push_f32s(&mut bytes, &self.inverse.map(float));
        super::push_f32s(&mut bytes, &self.lens.map(float));
        super::push_f32s(&mut bytes, &[float(plane), float(step), float(taps), 0.0]);
        bytes
    }
}

pub(super) struct FocusPipelines {
    /// The image and its depth, into the image blurred across with the blur
    /// and depth of what colors each pixel of it.
    across_layout: wgpu::BindGroupLayout,
    across: wgpu::RenderPipeline,
    /// That image and what colors it, into the frame's image.
    down_layout: wgpu::BindGroupLayout,
    down: wgpu::RenderPipeline,
}

impl FocusPipelines {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gaanim-3d-focus"),
            source: wgpu::ShaderSource::Wgsl(FOCUS_SHADER.into()),
        });
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let uniforms = wgpu::BindGroupLayoutEntry {
            binding: 2,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(FOCUS_BYTES as u64),
            },
            count: None,
        };
        let across_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gaanim-3d-focus-across"),
            entries: &[
                texture(0),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: true,
                    },
                    count: None,
                },
                uniforms,
            ],
        });
        let down_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gaanim-3d-focus-down"),
            entries: &[texture(0), uniforms, texture(3)],
        });
        let pipeline = |layout: &wgpu::BindGroupLayout,
                        fragment,
                        targets: &[Option<wgpu::ColorTargetState>]| {
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("gaanim-3d-focus"),
                bind_group_layouts: &[Some(layout)],
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("gaanim-3d-focus"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vertex_focus"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets,
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let target = |format| {
            Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })
        };
        Self {
            across: pipeline(
                &across_layout,
                "fragment_across",
                &[target(ACROSS_FORMAT), target(SPREAD_FORMAT)],
            ),
            down: pipeline(&down_layout, "fragment_down", &[target(COLOR_FORMAT)]),
            across_layout,
            down_layout,
        }
    }
}

/// The textures a blur draws in, as large as the image: a copy of the
/// sharp image, the image blurred across, and the blur and depth of what
/// colors each pixel of it.
pub(super) struct FocusTargets {
    size: (u32, u32),
    sharp: wgpu::Texture,
    sharp_view: wgpu::TextureView,
    across: wgpu::TextureView,
    spread: wgpu::TextureView,
}

impl FocusTargets {
    pub(super) fn new(device: &wgpu::Device, size: (u32, u32)) -> Self {
        let texture = |label, format, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let drawn = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let sharp = texture(
            "gaanim-3d-focus-sharp",
            COLOR_FORMAT,
            wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let view = |texture: wgpu::Texture| texture.create_view(&Default::default());
        Self {
            size,
            sharp_view: sharp.create_view(&Default::default()),
            sharp,
            across: view(texture("gaanim-3d-focus-across", ACROSS_FORMAT, drawn)),
            spread: view(texture("gaanim-3d-focus-spread", SPREAD_FORMAT, drawn)),
        }
    }

    pub(super) fn size(&self) -> (u32, u32) {
        self.size
    }
}

/// Blur `image` (the frame's resolved image, `resolved` its texture) as
/// `focus` says, by the depth of what it shows, `depth` (the frame's
/// multisampled opaque depth).
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_focus(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    pipelines: &FocusPipelines,
    targets: &FocusTargets,
    focus: &FocusDraw,
    resolved: &wgpu::Texture,
    image: &wgpu::TextureView,
    depth: &wgpu::TextureView,
) {
    let (width, height) = targets.size;
    encoder.copy_texture_to_texture(
        resolved.as_image_copy(),
        targets.sharp.as_image_copy(),
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    let uniforms = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("gaanim-3d-focus"),
        contents: &focus.uniforms(),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let attachment = |view| {
        Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })
    };
    let passes = [
        (
            &pipelines.across,
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("gaanim-3d-focus-across"),
                layout: &pipelines.across_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&targets.sharp_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(depth),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: uniforms.as_entire_binding(),
                    },
                ],
            }),
            vec![attachment(&targets.across), attachment(&targets.spread)],
        ),
        (
            &pipelines.down,
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("gaanim-3d-focus-down"),
                layout: &pipelines.down_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&targets.across),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: uniforms.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&targets.spread),
                    },
                ],
            }),
            vec![attachment(image)],
        ),
    ];
    for (pipeline, group, attachments) in &passes {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("gaanim-3d-focus"),
            color_attachments: attachments,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.draw(0..3, 0..1);
    }
}

const FOCUS_SHADER: &str = r#"
struct Focus {
    // The view's z and w of a depth (z, 1): the third and fourth rows of the
    // inverse projection.
    inverse: vec4<f32>,
    // The focus, the aperture and the largest blur (scene units), and the
    // image's pixels per scene unit.
    lens: vec4<f32>,
    // The camera's distance to its target, the step between taps in pixels
    // and the taps on each side.
    taps: vec4<f32>,
}

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var depth: texture_depth_multisampled_2d;
@group(0) @binding(2) var<uniform> focus: Focus;
// Down: the blur and depth of what colors each pixel blurred across.
@group(0) @binding(3) var spread: texture_2d<f32>;

// The smallest blur, in pixels: a sharp pixel keeps its color.
const SHARP: f32 = 0.2;

@vertex
fn vertex_focus(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

// The blur of what a pixel shows, from its depth (reversed: near is 1, far
// 0, where nothing was drawn), in pixels.
fn blur_of(stored: f32) -> f32 {
    let z = 1.0 - stored;
    let view_w = focus.inverse.z * z + focus.inverse.w;
    let distance = -(focus.inverse.x * z + focus.inverse.y) / view_w;
    let depth = distance / focus.taps.x;
    if !(depth > 0.0) {
        return focus.lens.z * focus.lens.w;
    }
    let confusion = focus.lens.y * abs(1.0 - focus.lens.x / depth);
    return max(min(confusion, focus.lens.z) * focus.lens.w, SHARP);
}

// What a pixel `x` pixels away from a pixel at the depth `center`, which
// blurs by `center_blur`, receives of one at the depth `stored` that blurs
// by `blur`: what lies behind spreads over the pixel no more than it
// blurs, so a sharp edge in front keeps its edge.
fn spread_weight(x: f32, blur: f32, stored: f32, center: f32, center_blur: f32) -> vec2<f32> {
    var spread = blur;
    if stored < center {
        spread = min(blur, center_blur);
    }
    return vec2<f32>(exp(-0.5 * x * x / (spread * spread)) / spread, spread);
}

struct Across {
    @location(0) color: vec4<f32>,
    // The blur and depth of what colors the pixel, weighed by how much it
    // covers: blurring down spreads its color by the blur it came with.
    @location(1) spread: vec2<f32>,
}

@fragment
fn fragment_across(@builtin(position) at: vec4<f32>) -> Across {
    let size = vec2<i32>(textureDimensions(source));
    let p = vec2<i32>(at.xy);
    let center = textureLoad(depth, p, 0);
    let center_blur = blur_of(center);
    let taps = i32(focus.taps.z);
    var sum = vec4<f32>(0.0);
    var total = 0.0;
    var colored = vec3<f32>(0.0);
    for (var i = -taps; i <= taps; i++) {
        let offset = i32(round(f32(i) * focus.taps.y));
        let q = vec2<i32>(p.x + offset, p.y);
        if q.x < 0 || q.x >= size.x {
            continue;
        }
        let stored = textureLoad(depth, q, 0);
        let weighed = spread_weight(f32(offset), blur_of(stored), stored, center, center_blur);
        let color = textureLoad(source, q, 0);
        sum += color * weighed.x;
        total += weighed.x;
        let covered = weighed.x * color.a;
        colored += vec3<f32>(weighed.y, stored, 1.0) * covered;
    }
    var out: Across;
    out.color = sum / max(total, 1e-6);
    out.spread = vec2<f32>(center_blur, center);
    if colored.z > 1e-6 {
        out.spread = colored.xy / colored.z;
    }
    return out;
}

@fragment
fn fragment_down(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(source));
    let p = vec2<i32>(at.xy);
    let here = textureLoad(spread, p, 0).xy;
    let taps = i32(focus.taps.z);
    var sum = vec4<f32>(0.0);
    var total = 0.0;
    for (var i = -taps; i <= taps; i++) {
        let offset = i32(round(f32(i) * focus.taps.y));
        let q = vec2<i32>(p.x, p.y + offset);
        if q.y < 0 || q.y >= size.y {
            continue;
        }
        let there = textureLoad(spread, q, 0).xy;
        let weighed = spread_weight(f32(offset), there.x, there.y, here.y, here.x);
        sum += textureLoad(source, q, 0) * weighed.x;
        total += weighed.x;
    }
    return sum / max(total, 1e-6);
}
"#;
