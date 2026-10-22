//! Volumes (see [`gaanim_scene::VolumeData`]): a scalar field drawn as a
//! cloud that glows and absorbs light. The 3D pass draws the far faces of
//! the field's box, and each pixel marches the ray through the box from the
//! eye in, up to what is opaque, accumulating the color and opacity each
//! value gives, then lays what it gathered over the image. Volumes draw
//! after the opaque content and before the translucent content. Built the
//! first time a frame draws one, so other frames cost nothing more.

use vello::wgpu;

use super::glow::mesh_attributes;
use super::{COLOR_FORMAT, INSTANCE_BYTES, MESH_VERTEX_BYTES, Pipelines, SAMPLES, SHADER};

pub(super) struct VolumePipelines {
    /// The field's values, the color and opacity of each, and their
    /// sampler.
    pub(super) layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
}

impl VolumePipelines {
    pub(super) fn new(device: &wgpu::Device, base: &Pipelines) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gaanim-3d-volume"),
            source: wgpu::ShaderSource::Wgsl(format!("{SHADER}{VOLUME_SHADER}").into()),
        });
        let texture = |binding, dimension| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: dimension,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gaanim-3d-volume"),
            entries: &[
                texture(0, wgpu::TextureViewDimension::D3),
                texture(1, wgpu::TextureViewDimension::D2),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gaanim-3d-volume"),
            bind_group_layouts: &[Some(&base.layout), Some(&base.depth_layout), Some(&layout)],
            immediate_size: 0,
        });
        let (mesh, instance) = mesh_attributes();
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gaanim-3d-volume"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vertex_volume"),
                compilation_options: Default::default(),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: MESH_VERTEX_BYTES as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &mesh,
                    },
                    wgpu::VertexBufferLayout {
                        array_stride: INSTANCE_BYTES as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &instance,
                    },
                ],
            },
            // The box's far faces: each pixel it covers once, also with the
            // eye inside it.
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Front),
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: SAMPLES,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fragment_volume"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("gaanim-3d-volume"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            layout,
            pipeline,
            sampler,
        }
    }

    pub(super) fn pipeline(&self) -> &wgpu::RenderPipeline {
        &self.pipeline
    }

    /// Upload `volume`'s values and colors; `None` when its values do not
    /// fill its grid.
    pub(super) fn upload(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        volume: &gaanim_scene::VolumeData,
    ) -> Option<wgpu::BindGroup> {
        let texels = volume.texels()?;
        let [width, height, depth] = volume.size;
        let values = upload(
            device,
            queue,
            "gaanim-3d-volume-values",
            (wgpu::TextureFormat::R8Unorm, wgpu::TextureDimension::D3),
            [width, height, depth],
            texels,
        );
        let transfer = volume.transfer.data.data();
        if transfer.len() < 256 * 4 {
            return None;
        }
        let transfer = upload(
            device,
            queue,
            "gaanim-3d-volume-transfer",
            (wgpu::TextureFormat::Rgba8Unorm, wgpu::TextureDimension::D2),
            [256, 1, 1],
            &transfer[..256 * 4],
        );
        Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gaanim-3d-volume"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&values),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&transfer),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        }))
    }
}

/// A texture of `size` texels in `format`, holding `bytes`.
fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    (format, dimension): (wgpu::TextureFormat, wgpu::TextureDimension),
    [width, height, depth]: [u32; 3],
    bytes: &[u8],
) -> wgpu::TextureView {
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: depth,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let texel = format.block_copy_size(None).unwrap_or(1);
    queue.write_texture(
        texture.as_image_copy(),
        bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * texel),
            rows_per_image: Some(height),
        },
        size,
    );
    texture.create_view(&Default::default())
}

/// The march through a volume, after [`SHADER`]. A volume's draw uniforms
/// hold its box, from `line.xyz` to `outline.xyz`, the absorption of a
/// value of full opacity per scene unit in `line.w`, and the steps of its
/// march in `outline.w`.
const VOLUME_SHADER: &str = r#"
@group(2) @binding(0) var volume_values: texture_3d<f32>;
@group(2) @binding(1) var volume_transfer: texture_2d<f32>;
@group(2) @binding(2) var volume_sampler: sampler;

struct VolumeOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
}

@vertex
fn vertex_volume(in: VertexIn) -> VolumeOut {
    let model = mat4x4<f32>(in.model_x, in.model_y, in.model_z, in.model_w);
    let world = (model * vec4<f32>(in.position, 1.0)).xyz;
    var out: VolumeOut;
    out.clip = image_clip(world);
    out.world = world;
    return out;
}

fn to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        return value / 12.92;
    }
    return pow((value + 0.055) / 1.055, 2.4);
}

// A point's depth in the image, reversed as the depth buffer holds it.
fn depth_of(clip: vec4<f32>) -> f32 {
    return clip.z / clip.w;
}

@fragment
fn fragment_volume(in: VolumeOut) -> @location(0) vec4<f32> {
    // The model's inverse: its normal matrix is the inverse's transpose.
    let to_local = transpose(mat3x3<f32>(draw.normal[0].xyz, draw.normal[1].xyz, draw.normal[2].xyz));
    let offset = draw.model[3].xyz;
    let low = draw.line.xyz;
    let high = draw.outline.xyz;
    // The ray through the pixel, in the world: from the eye, or along the
    // view from far behind the box.
    var direction = frame.forward.xyz;
    var origin = in.world - direction * 1.0e4;
    if frame.eye.w > 0.5 {
        origin = frame.eye.xyz;
        direction = normalize_or_zero(in.world - origin);
    }
    // In the box's coordinates, where a step of the ray still moves a
    // scene unit of it along the world.
    let start = to_local * (origin - offset);
    let along = to_local * direction;
    let inverse = 1.0 / select(along, vec3<f32>(1.0e-12), abs(along) < vec3<f32>(1.0e-12));
    let a = (low - start) * inverse;
    let b = (high - start) * inverse;
    let near = max(max(max(min(a.x, b.x), min(a.y, b.y)), min(a.z, b.z)), 0.0);
    let far = min(min(max(a.x, b.x), max(a.y, b.y)), max(a.z, b.z));
    if far <= near {
        discard;
    }
    // The ray stops at what is opaque.
    let opaque = textureLoad(opaque_depth, vec2<i32>(in.clip.xy), 0);
    let clip_near = image_clip(origin + direction * near);
    let clip_far = image_clip(origin + direction * far);
    let steps = max(draw.outline.w, 1.0);
    let span = (far - near) / steps;
    let density = draw.line.w;
    let grid = vec3<f32>(textureDimensions(volume_values));
    // Each pixel starts its march a different fraction of a step in, so that
    // steps do not band (interleaved gradient noise).
    let jitter = fract(52.9829189 * fract(dot(in.clip.xy, vec2<f32>(0.06711056, 0.00583715))));
    var color = vec3<f32>(0.0);
    var alpha = 0.0;
    for (var i = 0u; i < u32(steps); i = i + 1u) {
        let s = (f32(i) + jitter) / steps;
        if depth_of(mix(clip_near, clip_far, s)) < opaque {
            break;
        }
        let local = start + along * (near + s * (far - near));
        // The grid's points lie at the middle of the texels.
        let uvw = ((local - low) / (high - low) * (grid - 1.0) + 0.5) / grid;
        let value = textureSampleLevel(volume_values, volume_sampler, uvw, 0.0).r;
        let texel = textureSampleLevel(volume_transfer, volume_sampler, vec2<f32>((value * 255.0 + 0.5) / 256.0, 0.5), 0.0);
        let taken = 1.0 - exp(-texel.a * density * span);
        let linear = vec3<f32>(to_linear(texel.r), to_linear(texel.g), to_linear(texel.b));
        color += (1.0 - alpha) * taken * linear;
        alpha += (1.0 - alpha) * taken;
        if alpha > 0.995 {
            break;
        }
    }
    if alpha < 1.0 / 512.0 {
        discard;
    }
    return output(vec4<f32>(color / alpha, alpha));
}
"#;
