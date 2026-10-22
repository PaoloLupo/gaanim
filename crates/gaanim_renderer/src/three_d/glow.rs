//! The glow, drop shadow and blur of 3D drawables, drawn by the 3D pass into
//! its own image: their visible silhouettes and depth, against the frame's
//! opaque depth, blurred as a 2D glow or shadow is blurred and laid over the
//! image around them (a shadow moved by its offset), under them and under
//! what lies in front of them, as a 2D glow or shadow lies under the
//! drawables drawn after it. A blurred drawable is drawn apart, after the
//! others, and its color blurred and laid over them where nothing nearer
//! hides it. Built the first time a frame draws one, so other frames cost
//! nothing more.

use std::num::NonZeroU64;

use vello::wgpu;

use super::{
    COLOR_FORMAT, DEPTH_FORMAT, DRAW_STRIDE, GpuDraw, INSTANCE_BYTES, MESH_VERTEX_BYTES,
    MODEL_SHADER, MODEL_VERTEX_BYTES, ModelPipelines, Pipelines, SAMPLES, SEGMENT_BYTES, SHADER,
};
use crate::effects::{DropShadow, Glow};
use crate::soft_effects::{GLOW_SIGMAS, SHADOW_GAIN, glow_depth};

/// The silhouettes: their coverage and depth, and the blurred ones, whose
/// faint tails 8 bits would band.
const MASK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const BLUR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Bytes of a pass's uniforms (`GlowPass` in [`PASS_SHADER`]), at
/// `DRAW_STRIDE` from one to the next.
const PASS_BYTES: usize = 48;

/// The glow or shadow of a draw: its blur in pixels, the gain of its alpha,
/// its sRGB color and its offset in pixels (none for a glow), as bits,
/// which the draws of one pass share.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct GlowDraw {
    sigma: u32,
    gain: u32,
    color: [u32; 4],
    offset: [u32; 2],
}

impl GlowDraw {
    /// `glow` drawn at `pixels_per_unit`, as a 2D glow: blurred by a
    /// quarter of its radius, and as opaque on its edge as a 2D glow is on
    /// its outline. `None` for a glow that shows nothing.
    pub(super) fn new(glow: &Glow, pixels_per_unit: f64) -> Option<Self> {
        let sigma = glow.radius * pixels_per_unit / GLOW_SIGMAS;
        let shows =
            glow.intensity.is_finite() && glow.intensity > 0.0 && glow.color.components[3] > 0.0;
        (sigma.is_finite() && sigma > 0.0 && shows).then(|| Self {
            sigma: (sigma as f32).to_bits(),
            // A blurred silhouette covers half of its edge, where the
            // outline a 2D glow blurs covers all of it.
            gain: ((glow_depth(glow.intensity) / 0.5) as f32).to_bits(),
            color: glow.color.components.map(f32::to_bits),
            offset: [0.0_f32.to_bits(); 2],
        })
    }

    /// `shadow` drawn at `pixels_per_unit`, as a 2D drop shadow: the
    /// silhouette moved by its offset (y up in the scene, down in the
    /// image), blurred by its radius and as opaque inside as a 2D shadow.
    /// `None` for a shadow that shows nothing.
    pub(super) fn shadow(shadow: &DropShadow, pixels_per_unit: f64) -> Option<Self> {
        // A sharp shadow is blurred by a fraction of a pixel.
        let sigma = (shadow.blur_radius.max(0.0) * pixels_per_unit).max(1e-3);
        let offset = [shadow.offset.x, -shadow.offset.y].map(|value| value * pixels_per_unit);
        let shows = shadow.color.components[3] > 0.0;
        (sigma.is_finite() && offset.iter().all(|value| value.is_finite()) && shows).then(|| Self {
            sigma: (sigma as f32).to_bits(),
            gain: (SHADOW_GAIN as f32).to_bits(),
            color: shadow.color.components.map(f32::to_bits),
            offset: offset.map(|value| (value as f32).to_bits()),
        })
    }
}

/// What draws the silhouettes, blurs them and lays them over the image.
pub(super) struct GlowPipelines {
    module: wgpu::ShaderModule,
    /// Meshes' silhouettes by `[culled]`, and lines'.
    meshes: [wgpu::RenderPipeline; 2],
    lines: wgpu::RenderPipeline,
    /// Imported models' by `[culled]`, built the first time one glows.
    models: Option<[wgpu::RenderPipeline; 2]>,
    /// Filled outlines', built the first time they glow or cast a shadow.
    fills: Option<wgpu::RenderPipeline>,
    blur: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    /// Lays a blurred drawable's color over the image.
    blur_composite: wgpu::RenderPipeline,
    blur_layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::BindGroupLayout,
}

/// The vertex buffers of meshes, as [`Pipelines`] binds them: positions,
/// normals and colors, then each draw's instance.
pub(super) fn mesh_attributes() -> ([wgpu::VertexAttribute; 3], [wgpu::VertexAttribute; 8]) {
    let mesh = [
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x3,
            offset: 0,
            shader_location: 0,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x3,
            offset: 12,
            shader_location: 1,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 24,
            shader_location: 2,
        },
    ];
    let instance = std::array::from_fn(|index| wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: index as u64 * 16,
        shader_location: 3 + index as u32,
    });
    (mesh, instance)
}

impl GlowPipelines {
    pub(super) fn new(device: &wgpu::Device, base: &Pipelines) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gaanim-3d-glow"),
            source: wgpu::ShaderSource::Wgsl(
                format!("{SHADER}{MODEL_SHADER}{SILHOUETTE_SHADER}").into(),
            ),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gaanim-3d-glow"),
            bind_group_layouts: &[Some(&base.layout), Some(&base.shadow_layout)],
            immediate_size: 0,
        });
        let (mesh_attributes, instance_attributes) = mesh_attributes();
        let mesh_buffers = [
            wgpu::VertexBufferLayout {
                array_stride: MESH_VERTEX_BYTES as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &mesh_attributes,
            },
            wgpu::VertexBufferLayout {
                array_stride: INSTANCE_BYTES as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &instance_attributes,
            },
        ];
        let segment_attributes: [wgpu::VertexAttribute; 4] =
            std::array::from_fn(|index| wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: index as u64 * 16,
                shader_location: index as u32,
            });
        let segment_buffers = [wgpu::VertexBufferLayout {
            array_stride: SEGMENT_BYTES as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &segment_attributes,
        }];
        let meshes = [false, true].map(|culled| {
            silhouette(
                device,
                &module,
                &layout,
                ("vertex", "fragment_glow"),
                &mesh_buffers,
                culled,
            )
        });
        let lines = silhouette(
            device,
            &module,
            &layout,
            ("vertex_line", "fragment_line_glow"),
            &segment_buffers,
            false,
        );

        let pass_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gaanim-3d-glow-pass"),
            source: wgpu::ShaderSource::Wgsl(PASS_SHADER.into()),
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
            binding: 1,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: NonZeroU64::new(PASS_BYTES as u64),
            },
            count: None,
        };
        let blur_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gaanim-3d-glow-blur"),
            entries: &[texture(0), uniforms],
        });
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gaanim-3d-glow-composite"),
            entries: &[
                texture(0),
                uniforms,
                texture(2),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: true,
                    },
                    count: None,
                },
            ],
        });
        let pass = |label, group: &wgpu::BindGroupLayout, fragment, format, blend| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[Some(group)],
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &pass_module,
                    entry_point: Some("vertex_glow_pass"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &pass_module,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let blur = pass(
            "gaanim-3d-glow-blur",
            &blur_layout,
            "fragment_glow_blur",
            BLUR_FORMAT,
            None,
        );
        let composite = pass(
            "gaanim-3d-glow-composite",
            &composite_layout,
            "fragment_glow_composite",
            COLOR_FORMAT,
            Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
        );
        let blur_composite = pass(
            "gaanim-3d-blur-composite",
            &composite_layout,
            "fragment_blur_composite",
            COLOR_FORMAT,
            Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
        );
        Self {
            module,
            meshes,
            lines,
            models: None,
            fills: None,
            blur,
            composite,
            blur_composite,
            blur_layout,
            composite_layout,
        }
    }

    /// Build the pipelines of models' silhouettes, the first time a model
    /// glows.
    pub(super) fn add_models(
        &mut self,
        device: &wgpu::Device,
        base: &Pipelines,
        models: &ModelPipelines,
    ) {
        if self.models.is_some() {
            return;
        }
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gaanim-3d-glow-model"),
            bind_group_layouts: &[
                Some(&base.layout),
                Some(&base.shadow_layout),
                Some(&models.texture_layout),
                Some(&models.joints_layout),
            ],
            immediate_size: 0,
        });
        let (mesh_attributes, instance_attributes) = mesh_attributes();
        let model_attributes = model_attributes();
        let buffers = [
            wgpu::VertexBufferLayout {
                array_stride: MESH_VERTEX_BYTES as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &mesh_attributes,
            },
            wgpu::VertexBufferLayout {
                array_stride: INSTANCE_BYTES as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &instance_attributes,
            },
            wgpu::VertexBufferLayout {
                array_stride: MODEL_VERTEX_BYTES as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &model_attributes,
            },
        ];
        self.models = Some([false, true].map(|culled| {
            silhouette(
                device,
                &self.module,
                &layout,
                ("vertex_model", "fragment_model_glow"),
                &buffers,
                culled,
            )
        }));
    }
}

/// An imported model's vertex buffer, as [`ModelPipelines`] binds it: UVs,
/// joints and weights.
pub(super) fn model_attributes() -> [wgpu::VertexAttribute; 3] {
    [
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 0,
            shader_location: 12,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint16x4,
            offset: 8,
            shader_location: 13,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 16,
            shader_location: 14,
        },
    ]
}

/// A pipeline drawing silhouettes into the multisampled mask where nothing
/// opaque hides them, nudged toward the viewer so that a drawable's own
/// surface, in the depth already, does not hide it.
fn silhouette(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    (vertex, fragment): (&str, &str),
    buffers: &[wgpu::VertexBufferLayout<'_>],
    culled: bool,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("gaanim-3d-glow-silhouette"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some(vertex),
            compilation_options: Default::default(),
            buffers,
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: culled.then_some(wgpu::Face::Back),
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            // Reversed depth: near is 1, and the bias moves nearer.
            depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
            stencil: Default::default(),
            bias: wgpu::DepthBiasState {
                constant: 2,
                slope_scale: 1.0,
                clamp: 0.0,
            },
        }),
        multisample: wgpu::MultisampleState {
            count: SAMPLES,
            ..Default::default()
        },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fragment),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: MASK_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// The textures the glow of an image draws in, as large as the image.
pub(super) struct GlowTargets {
    size: (u32, u32),
    /// The silhouettes, multisampled, and resolved.
    samples: wgpu::TextureView,
    mask: wgpu::TextureView,
    /// Blurred across, then down.
    across: wgpu::TextureView,
    blurred: wgpu::TextureView,
    across_group: Option<wgpu::BindGroup>,
    down_group: Option<wgpu::BindGroup>,
    /// The color of blurred drawables and its blurs, made the first time a
    /// frame blurs one.
    color: Option<BlurColor>,
    /// The bytes `uniforms` holds.
    capacity: usize,
    uniforms: Option<wgpu::Buffer>,
}

/// What a blurred drawable's color is drawn in, multisampled and resolved,
/// and blurred across, then down, with the groups that read each.
struct BlurColor {
    samples: wgpu::TextureView,
    resolved: wgpu::TextureView,
    across: wgpu::TextureView,
    blurred: wgpu::TextureView,
    groups: Option<[wgpu::BindGroup; 2]>,
}

impl GlowTargets {
    pub(super) fn new(device: &wgpu::Device, size: (u32, u32)) -> Self {
        let texture = |label, format, samples, usage| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: size.0,
                        height: size.1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let read = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        Self {
            size,
            samples: texture(
                "gaanim-3d-glow-samples",
                MASK_FORMAT,
                SAMPLES,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            ),
            mask: texture("gaanim-3d-glow-mask", MASK_FORMAT, 1, read),
            across: texture("gaanim-3d-glow-across", BLUR_FORMAT, 1, read),
            blurred: texture("gaanim-3d-glow-blurred", BLUR_FORMAT, 1, read),
            across_group: None,
            down_group: None,
            color: None,
            capacity: 0,
            uniforms: None,
        }
    }

    /// Make the textures of blurred drawables' color, once.
    fn color(&mut self, device: &wgpu::Device) -> &mut BlurColor {
        let size = self.size;
        self.color.get_or_insert_with(|| {
            let texture = |label, format, samples, usage| {
                device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some(label),
                        size: wgpu::Extent3d {
                            width: size.0,
                            height: size.1,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: samples,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage,
                        view_formats: &[],
                    })
                    .create_view(&Default::default())
            };
            let read =
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
            BlurColor {
                samples: texture(
                    "gaanim-3d-blur-samples",
                    COLOR_FORMAT,
                    SAMPLES,
                    wgpu::TextureUsages::RENDER_ATTACHMENT,
                ),
                resolved: texture("gaanim-3d-blur-color", COLOR_FORMAT, 1, read),
                across: texture("gaanim-3d-blur-across", BLUR_FORMAT, 1, read),
                blurred: texture("gaanim-3d-blur-blurred", BLUR_FORMAT, 1, read),
                groups: None,
            }
        })
    }

    pub(super) fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Hold `bytes` of pass uniforms, binding them anew when they outgrow
    /// the buffer.
    fn reserve(&mut self, device: &wgpu::Device, glow: &GlowPipelines, bytes: &[u8]) {
        if self.capacity < bytes.len() || self.uniforms.is_none() {
            let capacity = bytes.len().next_power_of_two().max(DRAW_STRIDE);
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gaanim-3d-glow-passes"),
                size: capacity as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let group = |label, source: &wgpu::TextureView| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(label),
                    layout: &glow.blur_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(source),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: pass_uniforms(&buffer),
                        },
                    ],
                })
            };
            self.across_group = Some(group("gaanim-3d-glow-across", &self.mask));
            self.down_group = Some(group("gaanim-3d-glow-down", &self.across));
            if let Some(color) = self.color.as_mut() {
                color.groups = None;
            }
            self.uniforms = Some(buffer);
            self.capacity = capacity;
        }
        if let Some(color) = self.color.as_mut()
            && color.groups.is_none()
        {
            let buffer = self.uniforms.as_ref().expect("reserved");
            let group = |label, source: &wgpu::TextureView| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(label),
                    layout: &glow.blur_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(source),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: pass_uniforms(buffer),
                        },
                    ],
                })
            };
            color.groups = Some([
                group("gaanim-3d-blur-across", &color.resolved),
                group("gaanim-3d-blur-down", &color.across),
            ]);
        }
    }

    /// What the composite of blurred drawables reads: their blurred color,
    /// the uniforms, their blurred silhouettes and the frame's depth.
    fn blur_composite_group(
        &self,
        device: &wgpu::Device,
        glow: &GlowPipelines,
        depth: &wgpu::TextureView,
    ) -> Option<wgpu::BindGroup> {
        let color = self.color.as_ref()?;
        Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gaanim-3d-blur-composite"),
            layout: &glow.composite_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color.blurred),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: pass_uniforms(self.uniforms.as_ref().expect("reserved")),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.blurred),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
            ],
        }))
    }

    /// What the composite reads: the blurred silhouettes, the uniforms, the
    /// sharp silhouettes and the frame's opaque `depth`, which the 3D pass
    /// may have drawn into a new texture since the last frame.
    fn composite_group(
        &self,
        device: &wgpu::Device,
        glow: &GlowPipelines,
        depth: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gaanim-3d-glow-composite"),
            layout: &glow.composite_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.blurred),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: pass_uniforms(self.uniforms.as_ref().expect("reserved")),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.mask),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
            ],
        })
    }
}

fn pass_uniforms(buffer: &wgpu::Buffer) -> wgpu::BindingResource<'_> {
    wgpu::BindingResource::Buffer(wgpu::BufferBinding {
        buffer,
        offset: 0,
        size: NonZeroU64::new(PASS_BYTES as u64),
    })
}

/// How a glow pass draws a draw's silhouette: binding its geometry and
/// issuing its draw call, given the pass, the draw and its index.
pub(super) type Silhouette<'a> =
    dyn Fn(&mut wgpu::RenderPass<'_>, &GlowPipelines, &GpuDraw, usize) + 'a;

/// How a blur pass draws the color of the draws alike `first..end`, as a
/// translucent layer is drawn: its nearest surface in front of the opaque
/// depth, into a depth of its own, then that surface's color.
pub(super) type BlurredColor<'a> = dyn Fn(&mut wgpu::RenderPass<'_>, usize, usize) + 'a;

/// What [`draw_soft`] draws with and over.
pub(super) struct SoftTargets<'a> {
    /// The frame's opaque depth, multisampled, and a depth for blurred
    /// drawables' layers, as large.
    pub depth: &'a wgpu::TextureView,
    pub layer: &'a wgpu::TextureView,
    /// The image drawn so far.
    pub target: &'a wgpu::TextureView,
}

/// Draw the blurred drawables of `draws`, then their shadows, then their
/// glows, over `targets.target`. For each blur they share, the color of the
/// draws with it, blurred and laid over the image where nothing nearer
/// hides it; for each shadow or glow, the silhouettes of the draws with it,
/// blurred and laid around them. `alike` tells which draws draw as one.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_soft(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    glow: &GlowPipelines,
    targets: &mut GlowTargets,
    draws: &[GpuDraw],
    alike: &dyn Fn(usize, usize) -> bool,
    soft_targets: SoftTargets<'_>,
    silhouette: &Silhouette<'_>,
    blurred_color: &BlurredColor<'_>,
) {
    let mut blurs: Vec<u32> = Vec::new();
    for draw in draws {
        if let Some(sigma) = draw.blur
            && !blurs.contains(&sigma)
        {
            blurs.push(sigma);
        }
    }
    let mut passes: Vec<GlowDraw> = Vec::new();
    for soft in [|draw: &GpuDraw| draw.shadow, |draw: &GpuDraw| draw.glow] {
        for draw in draws {
            if let Some(each) = soft(draw)
                && !passes.contains(&each)
            {
                passes.push(each);
            }
        }
    }
    if blurs.is_empty() && passes.is_empty() {
        return;
    }
    // Two entries a blur, then two a shadow or glow, `DRAW_STRIDE` apart:
    // across, then down, which the composite reads too.
    let mut bytes = Vec::with_capacity((blurs.len() + passes.len()) * 2 * DRAW_STRIDE);
    let mut push = |sigma: u32, gain: u32, color: [u32; 4], offset: [u32; 2]| {
        for across in [1.0_f32, 0.0] {
            let start = bytes.len();
            let values = [f32::from_bits(sigma), across, f32::from_bits(gain), 0.0];
            let offset = [
                f32::from_bits(offset[0]),
                f32::from_bits(offset[1]),
                0.0,
                0.0,
            ];
            for value in values
                .into_iter()
                .chain(color.map(f32::from_bits))
                .chain(offset)
            {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            bytes.resize(start + DRAW_STRIDE, 0);
        }
    };
    for sigma in &blurs {
        push(*sigma, 0, [0; 4], [0; 2]);
    }
    for each in &passes {
        push(each.sigma, each.gain, each.color, each.offset);
    }
    if !blurs.is_empty() {
        targets.color(device);
    }
    targets.reserve(device, glow, &bytes);
    queue.write_buffer(targets.uniforms.as_ref().expect("reserved"), 0, &bytes);
    let SoftTargets {
        depth,
        layer,
        target,
    } = soft_targets;
    let composite_group = targets.composite_group(device, glow, depth);
    let blur_composite_group = targets.blur_composite_group(device, glow, depth);
    let fullscreen = |encoder: &mut wgpu::CommandEncoder,
                      label,
                      view: &wgpu::TextureView,
                      load,
                      pipeline: &wgpu::RenderPipeline,
                      group: &wgpu::BindGroup,
                      offset: usize| {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[offset as u32]);
        pass.draw(0..3, 0..1);
    };
    let clear = wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT);
    // The silhouettes of the draws `shows` picks, into the mask.
    let silhouettes = |encoder: &mut wgpu::CommandEncoder, shows: &dyn Fn(&GpuDraw) -> bool| {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("gaanim-3d-glow-silhouettes"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &targets.samples,
                depth_slice: None,
                resolve_target: Some(&targets.mask),
                ops: wgpu::Operations {
                    load: clear,
                    store: wgpu::StoreOp::Discard,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        for (at, draw) in draws.iter().enumerate() {
            if shows(draw) {
                silhouette(&mut pass, glow, draw, at);
            }
        }
    };
    let mask_groups = (
        targets.across_group.as_ref().expect("reserved"),
        targets.down_group.as_ref().expect("reserved"),
    );
    for (index, sigma) in blurs.iter().enumerate() {
        let across = index * 2 * DRAW_STRIDE;
        let color = targets.color.as_ref().expect("made for blurs");
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gaanim-3d-blur-color"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color.samples,
                    depth_slice: None,
                    resolve_target: Some(&color.resolved),
                    ops: wgpu::Operations {
                        load: clear,
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: layer,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let mut at = 0;
            while at < draws.len() {
                if draws[at].blur != Some(*sigma) {
                    at += 1;
                    continue;
                }
                let first = at;
                at += 1;
                while at < draws.len() && alike(first, at) {
                    at += 1;
                }
                blurred_color(&mut pass, first, at);
            }
        }
        silhouettes(encoder, &|draw: &GpuDraw| draw.blur == Some(*sigma));
        let groups = color.groups.as_ref().expect("reserved");
        fullscreen(
            encoder,
            "gaanim-3d-glow-across",
            &targets.across,
            clear,
            &glow.blur,
            mask_groups.0,
            across,
        );
        fullscreen(
            encoder,
            "gaanim-3d-glow-down",
            &targets.blurred,
            clear,
            &glow.blur,
            mask_groups.1,
            across + DRAW_STRIDE,
        );
        fullscreen(
            encoder,
            "gaanim-3d-blur-across",
            &color.across,
            clear,
            &glow.blur,
            &groups[0],
            across,
        );
        fullscreen(
            encoder,
            "gaanim-3d-blur-down",
            &color.blurred,
            clear,
            &glow.blur,
            &groups[1],
            across + DRAW_STRIDE,
        );
        fullscreen(
            encoder,
            "gaanim-3d-blur-composite",
            target,
            wgpu::LoadOp::Load,
            &glow.blur_composite,
            blur_composite_group.as_ref().expect("made for blurs"),
            across + DRAW_STRIDE,
        );
    }
    for (index, each) in passes.iter().enumerate() {
        silhouettes(encoder, &|draw: &GpuDraw| {
            draw.glow == Some(*each) || draw.shadow == Some(*each)
        });
        let across = (blurs.len() + index) * 2 * DRAW_STRIDE;
        fullscreen(
            encoder,
            "gaanim-3d-glow-across",
            &targets.across,
            clear,
            &glow.blur,
            mask_groups.0,
            across,
        );
        fullscreen(
            encoder,
            "gaanim-3d-glow-down",
            &targets.blurred,
            clear,
            &glow.blur,
            mask_groups.1,
            across + DRAW_STRIDE,
        );
        fullscreen(
            encoder,
            "gaanim-3d-glow-composite",
            target,
            wgpu::LoadOp::Load,
            &glow.composite,
            &composite_group,
            across + DRAW_STRIDE,
        );
    }
}

impl GlowPipelines {
    pub(super) fn mesh(&self, culled: bool) -> &wgpu::RenderPipeline {
        &self.meshes[usize::from(culled)]
    }

    pub(super) fn lines(&self) -> &wgpu::RenderPipeline {
        &self.lines
    }

    pub(super) fn model(&self, culled: bool) -> Option<&wgpu::RenderPipeline> {
        self.models
            .as_ref()
            .map(|models| &models[usize::from(culled)])
    }

    pub(super) fn fill(&self) -> Option<&wgpu::RenderPipeline> {
        self.fills.as_ref()
    }

    /// Build the silhouettes of filled outlines, once.
    pub(super) fn add_fills(&mut self, device: &wgpu::Device, base: &Pipelines) {
        if self.fills.is_some() {
            return;
        }
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gaanim-3d-glow-fill"),
            source: wgpu::ShaderSource::Wgsl(
                format!(
                    "{SHADER}{}{FILL_SILHOUETTE_SHADER}",
                    crate::plane_fill::SHADER
                )
                .into(),
            ),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gaanim-3d-glow-fill"),
            bind_group_layouts: &[
                Some(&base.layout),
                Some(&base.shadow_layout),
                Some(&base.fill_layout),
            ],
            immediate_size: 0,
        });
        let attributes = super::fill_attributes();
        let buffers = [wgpu::VertexBufferLayout {
            array_stride: crate::plane_fill::GLYPH_BYTES as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &attributes,
        }];
        self.fills = Some(silhouette(
            device,
            &module,
            &layout,
            ("vertex_fill", "fragment_fill_glow"),
            &buffers,
            false,
        ));
    }
}

/// The silhouettes of filled outlines, after [`SHADER`] and the outlines'
/// own shader: as strong as an outline covers the pixel and is opaque, its
/// drawable's opacity included, with its depth weighted by that.
const FILL_SILHOUETTE_SHADER: &str = r#"
@fragment
fn fragment_fill_glow(in: FillOut) -> @location(0) vec4<f32> {
    let dx = dpdx(in.outline);
    let dy = dpdy(in.outline);
    let alpha = fill_paint(in.glyph.w, in.outline, dx, dy, in.color).a;
    let pixels = fill_pixels(in.outline);
    let coverage = fill_clipped(in.outline, in.band, in.glyph, in.clip_offset, in.clip_band, in.clip_glyph, pixels);
    let strength = coverage * alpha * draw.emissive.w;
    if strength <= 0.0 {
        discard;
    }
    return vec4<f32>(strength, in.clip.z * strength, 0.0, strength);
}
"#;

/// The silhouettes' fragments, after [`SHADER`] and [`MODEL_SHADER`]: as
/// strong as the draw is opaque, with its depth weighted by that, and the
/// strength again for the composite to leave the drawable itself alone.
const SILHOUETTE_SHADER: &str = r#"
fn glow_silhouette(clip: vec4<f32>) -> vec4<f32> {
    let strength = draw.emissive.w;
    return vec4<f32>(strength, clip.z * strength, 0.0, strength);
}

@fragment
fn fragment_glow(in: VertexOut) -> @location(0) vec4<f32> {
    return glow_silhouette(in.clip);
}

@fragment
fn fragment_line_glow(in: LineOut) -> LineFragment {
    var out = line_fragment(in);
    out.color = glow_silhouette(in.clip);
    return out;
}

@fragment
fn fragment_model_glow(in: ModelOut) -> @location(0) vec4<f32> {
    if model_color(in).a < model_cut() {
        discard;
    }
    return glow_silhouette(in.clip);
}
"#;

/// Blurs the silhouettes along one axis, as the 2D glow does, then lays the
/// glow's or shadow's color over the image, a shadow moved by its offset,
/// with an alpha that saturates as `1 - exp(-gain * coverage)`, except over
/// the drawables themselves and over what lies nearer than them, where it
/// hides.
const PASS_SHADER: &str = r#"
struct GlowPass {
    // The blur's sigma in pixels, 1 to blur across and 0 down, and the
    // gain of the alpha.
    sigma: f32,
    across: f32,
    gain: f32,
    unused: f32,
    // Its sRGB color.
    color: vec4<f32>,
    // A shadow's offset in pixels, y down.
    offset: vec4<f32>,
}

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var<uniform> glow_pass: GlowPass;
@group(0) @binding(2) var mask: texture_2d<f32>;
@group(0) @binding(3) var depth: texture_depth_multisampled_2d;

@vertex
fn vertex_glow_pass(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fragment_glow_blur(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(source));
    let p = vec2<i32>(at.xy);
    let sigma = max(glow_pass.sigma, 0.001);
    let radius = ceil(3.0 * sigma);
    let step = max(1.0, radius / 32.0);
    var sum = vec4<f32>(0.0);
    var total = 0.0;
    for (var i = -32; i <= 32; i++) {
        let x = f32(i) * step;
        if abs(x) > radius {
            continue;
        }
        let weight = exp(-0.5 * x * x / (sigma * sigma));
        let offset = i32(round(x));
        let q = p + select(vec2<i32>(0, offset), vec2<i32>(offset, 0), glow_pass.across > 0.5);
        // Beyond the image lies nothing.
        if all(q >= vec2<i32>(0)) && all(q < size) {
            sum += textureLoad(source, q, 0) * weight;
        }
        total += weight;
    }
    return sum / total;
}

// Lays a blurred drawable's color (premultiplied) over the image, except
// where the samples of what lies nearer than its blurred silhouette hide it.
@fragment
fn fragment_blur_composite(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(at.xy);
    let color = textureLoad(source, p, 0);
    if color.a <= 1e-6 {
        return vec4<f32>(0.0);
    }
    let halo = textureLoad(mask, p, 0);
    if halo.r <= 1e-6 {
        return color;
    }
    let near = halo.g / halo.r;
    var shown = 0.0;
    for (var sample = 0; sample < 4; sample++) {
        shown += select(0.25, 0.0, textureLoad(depth, p, sample) > near + 1e-5);
    }
    return color * shown;
}

@fragment
fn fragment_glow_composite(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(at.xy);
    // A shadow reads its silhouette where its offset brings it from.
    let shifted = p - vec2<i32>(round(glow_pass.offset.xy));
    let size = vec2<i32>(textureDimensions(source));
    if any(shifted < vec2<i32>(0)) || any(shifted >= size) {
        return vec4<f32>(0.0);
    }
    let halo = textureLoad(source, shifted, 0);
    if halo.r <= 1e-6 {
        return vec4<f32>(0.0);
    }
    // The depth of the glowing surfaces near the pixel: the samples of what
    // lies nearer (reversed depth, greater) hide the glow, as drawables in
    // front of a 2D glow cover it.
    let near = halo.g / halo.r;
    var shown = 0.0;
    for (var sample = 0; sample < 4; sample++) {
        shown += select(0.25, 0.0, textureLoad(depth, p, sample) > near + 1e-5);
    }
    let own = textureLoad(mask, p, 0).a;
    let alpha = clamp(1.0 - exp(-glow_pass.gain * halo.r), 0.0, 1.0)
        * glow_pass.color.a * (1.0 - own) * shown;
    return vec4<f32>(glow_pass.color.rgb * alpha, alpha);
}
"#;
