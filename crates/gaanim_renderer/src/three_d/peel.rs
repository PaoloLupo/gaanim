//! Order-independent transparency (see [`gaanim_scene::Transparency3D`]):
//! the translucent surfaces of a frame peeled from the nearest, one layer per
//! pass, as many layers deep as the scene asks for. Each peel keeps, at each
//! sample, the nearest translucent surface in front of the opaque depth and
//! behind the layer peeled before; its color then draws under the layers
//! peeled before it, and the layers together over the opaque image. Built
//! the first time a frame peels, so other frames cost nothing more.

use vello::wgpu;

use super::glow::{mesh_attributes, model_attributes};
use super::{
    COLOR_FORMAT, DEPTH_FORMAT, INSTANCE_BYTES, MESH_VERTEX_BYTES, MODEL_SHADER,
    MODEL_VERTEX_BYTES, ModelPipelines, Pipelines, SAMPLES, SEGMENT_BYTES, SHADER,
};

/// What a translucent draw does in a peel.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Peel {
    /// Its nearest surface behind the last layer, into this layer's depth.
    Depth = 0,
    /// That surface's color, under the layers peeled before.
    Under = 1,
}

pub(super) struct PeelPipelines {
    module: wgpu::ShaderModule,
    /// Binds the opaque depth and the last layer's.
    layout: wgpu::BindGroupLayout,
    /// Meshes by `[culled][peel]`.
    meshes: [[wgpu::RenderPipeline; 2]; 2],
    /// Lines by peel.
    lines: [wgpu::RenderPipeline; 2],
    /// Imported models by `[culled][peel]`, built the first time one peels.
    models: Option<[[wgpu::RenderPipeline; 2]; 2]>,
    /// Lays the peeled layers over the opaque image.
    composite: wgpu::RenderPipeline,
    composite_layout: wgpu::BindGroupLayout,
}

/// Blends a peeled layer under the layers before it: what they leave
/// uncovered shows it.
const UNDER: wgpu::BlendState = {
    let under = wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::OneMinusDstAlpha,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    };
    wgpu::BlendState {
        color: under,
        alpha: under,
    }
};

impl PeelPipelines {
    pub(super) fn new(device: &wgpu::Device, base: &Pipelines) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gaanim-3d-peel"),
            source: wgpu::ShaderSource::Wgsl(format!("{SHADER}{MODEL_SHADER}{PEEL_SHADER}").into()),
        });
        let depth = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Depth,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: true,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gaanim-3d-peel"),
            entries: &[depth(0), depth(4)],
        });
        let peeling = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gaanim-3d-peel"),
            bind_group_layouts: &[Some(&base.layout), Some(&layout)],
            immediate_size: 0,
        });
        let shading = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gaanim-3d-peel-under"),
            bind_group_layouts: &[Some(&base.layout), Some(&base.shadow_layout)],
            immediate_size: 0,
        });
        let (mesh, instance) = mesh_attributes();
        let mesh_buffers = [
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
        let layouts = [&peeling, &shading];
        let meshes = [false, true].map(|culled| {
            [Peel::Depth, Peel::Under].map(|peel| {
                pipeline(
                    device,
                    &module,
                    layouts[peel as usize],
                    ("vertex", ["fragment_peel", "fragment"][peel as usize]),
                    &mesh_buffers,
                    culled,
                    peel,
                )
            })
        });
        let lines = [Peel::Depth, Peel::Under].map(|peel| {
            pipeline(
                device,
                &module,
                layouts[peel as usize],
                (
                    "vertex_line",
                    ["fragment_line_peel", "fragment_line"][peel as usize],
                ),
                &segment_buffers,
                false,
                peel,
            )
        });

        let composite_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gaanim-3d-peel-composite"),
            source: wgpu::ShaderSource::Wgsl(COMPOSITE_SHADER.into()),
        });
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gaanim-3d-peel-composite"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: true,
                },
                count: None,
            }],
        });
        let compositing = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gaanim-3d-peel-composite"),
            bind_group_layouts: &[Some(&composite_layout)],
            immediate_size: 0,
        });
        let composite = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gaanim-3d-peel-composite"),
            layout: Some(&compositing),
            vertex: wgpu::VertexState {
                module: &composite_module,
                entry_point: Some("vertex_peeled"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: SAMPLES,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &composite_module,
                entry_point: Some("fragment_peeled"),
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
        Self {
            module,
            layout,
            meshes,
            lines,
            models: None,
            composite,
            composite_layout,
        }
    }

    /// Build the pipelines of imported models, the first time one peels.
    pub(super) fn add_models(
        &mut self,
        device: &wgpu::Device,
        base: &Pipelines,
        models: &ModelPipelines,
    ) {
        if self.models.is_some() {
            return;
        }
        let layout = |label, group: &wgpu::BindGroupLayout| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[
                    Some(&base.layout),
                    Some(group),
                    Some(&models.texture_layout),
                    Some(&models.joints_layout),
                ],
                immediate_size: 0,
            })
        };
        let layouts = [
            layout("gaanim-3d-peel-model", &self.layout),
            layout("gaanim-3d-peel-model-under", &base.shadow_layout),
        ];
        let (mesh, instance) = mesh_attributes();
        let model = model_attributes();
        let buffers = [
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
            wgpu::VertexBufferLayout {
                array_stride: MODEL_VERTEX_BYTES as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &model,
            },
        ];
        self.models = Some([false, true].map(|culled| {
            [Peel::Depth, Peel::Under].map(|peel| {
                pipeline(
                    device,
                    &self.module,
                    &layouts[peel as usize],
                    (
                        "vertex_model",
                        ["fragment_model_peel", "fragment_model"][peel as usize],
                    ),
                    &buffers,
                    culled,
                    peel,
                )
            })
        }));
    }

    pub(super) fn mesh(&self, culled: bool, peel: Peel) -> &wgpu::RenderPipeline {
        &self.meshes[usize::from(culled)][peel as usize]
    }

    pub(super) fn lines(&self, peel: Peel) -> &wgpu::RenderPipeline {
        &self.lines[peel as usize]
    }

    pub(super) fn model(&self, culled: bool, peel: Peel) -> Option<&wgpu::RenderPipeline> {
        self.models
            .as_ref()
            .map(|models| &models[usize::from(culled)][peel as usize])
    }
}

/// A peel's pipeline: into the layer's depth, the nearest surface the
/// fragment shader keeps; or that surface's color, under what is peeled.
fn pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    (vertex, fragment): (&str, &str),
    buffers: &[wgpu::VertexBufferLayout<'_>],
    culled: bool,
    peel: Peel,
) -> wgpu::RenderPipeline {
    let under = [Some(wgpu::ColorTargetState {
        format: COLOR_FORMAT,
        blend: Some(UNDER),
        write_mask: wgpu::ColorWrites::ALL,
    })];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("gaanim-3d-peel"),
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
            depth_write_enabled: Some(peel == Peel::Depth),
            // Reversed depth: near is 1, far 0.
            depth_compare: Some(match peel {
                Peel::Depth => wgpu::CompareFunction::GreaterEqual,
                Peel::Under => wgpu::CompareFunction::Equal,
            }),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: SAMPLES,
            ..Default::default()
        },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fragment),
            compilation_options: Default::default(),
            targets: match peel {
                Peel::Depth => &[],
                Peel::Under => &under,
            },
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// What peels draw in besides the frame's scratch targets: the depth of
/// every other layer (the others lie in the scratch layer's), and the
/// peeled layers' color.
pub(super) struct PeelTargets {
    back: wgpu::TextureView,
    accumulated: wgpu::TextureView,
    /// Bind the opaque depth and the last layer's: the back depth's, for a
    /// layer peeled into the scratch layer, and the scratch layer's for one
    /// peeled into the back depth.
    groups: [wgpu::BindGroup; 2],
    composite_group: wgpu::BindGroup,
}

impl PeelTargets {
    pub(super) fn new(
        device: &wgpu::Device,
        pipelines: &PeelPipelines,
        size: (u32, u32),
        opaque: &wgpu::TextureView,
        layer: &wgpu::TextureView,
    ) -> Self {
        let texture = |label, format| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: size.0,
                        height: size.1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: SAMPLES,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let back = texture("gaanim-3d-peel-depth", DEPTH_FORMAT);
        let accumulated = texture("gaanim-3d-peeled", COLOR_FORMAT);
        let group = |last: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("gaanim-3d-peel"),
                layout: &pipelines.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(opaque),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(last),
                    },
                ],
            })
        };
        let groups = [group(&back), group(layer)];
        let composite_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gaanim-3d-peel-composite"),
            layout: &pipelines.composite_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&accumulated),
            }],
        });
        Self {
            back,
            accumulated,
            groups,
            composite_group,
        }
    }
}

/// Peel `layers` layers of a frame's translucent draws and lay them over its
/// color, `scratch` (opaque already, with its depth `opaque_depth`),
/// resolving into `resolve` when that is the frame's last pass.
/// `draw(pass, peel, group)` draws the translucent draws with the peel's
/// pipelines, binding `group` (or the shadow maps, for their color) to
/// group 1.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_peeled(
    encoder: &mut wgpu::CommandEncoder,
    pipelines: &PeelPipelines,
    targets: &PeelTargets,
    layers: u8,
    scratch: &wgpu::TextureView,
    layer: &wgpu::TextureView,
    resolve: Option<&wgpu::TextureView>,
    timestamp_writes: Option<wgpu::RenderPassTimestampWrites<'_>>,
    draw: &dyn Fn(&mut wgpu::RenderPass<'_>, Peel, Option<&wgpu::BindGroup>),
) {
    let depth_pass = |encoder: &mut wgpu::CommandEncoder,
                      view: &wgpu::TextureView,
                      load: wgpu::LoadOp<f32>|
     -> wgpu::RenderPass<'static> {
        encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gaanim-3d-peel"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view,
                    depth_ops: Some(wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            })
            .forget_lifetime()
    };
    // Before the first layer, the last one lies nearest of all: the first
    // keeps the nearest surface.
    drop(depth_pass(encoder, &targets.back, wgpu::LoadOp::Clear(1.0)));
    let depths = [layer, &targets.back];
    for index in 0..usize::from(layers) {
        let depth = depths[index % 2];
        {
            let mut pass = depth_pass(encoder, depth, wgpu::LoadOp::Clear(0.0));
            draw(&mut pass, Peel::Depth, Some(&targets.groups[index % 2]));
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("gaanim-3d-peel-under"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &targets.accumulated,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: if index == 0 {
                        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                    } else {
                        wgpu::LoadOp::Load
                    },
                    store: wgpu::StoreOp::Store,
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
        draw(&mut pass, Peel::Under, None);
    }
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("gaanim-3d-peel-composite"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: scratch,
            depth_slice: None,
            resolve_target: resolve,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: if resolve.is_some() {
                    wgpu::StoreOp::Discard
                } else {
                    wgpu::StoreOp::Store
                },
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(&pipelines.composite);
    pass.set_bind_group(0, &targets.composite_group, &[]);
    pass.draw(0..3, 0..1);
}

/// The peels' fragments, after [`SHADER`] and [`MODEL_SHADER`]: a fragment
/// a layer keeps lies in front of the opaque depth and behind the layer
/// before.
const PEEL_SHADER: &str = r#"
// The depth of the layer peeled before.
@group(1) @binding(4) var peeled_depth: texture_depth_multisampled_2d;

fn peelable(position: vec4<f32>, sample: u32) -> bool {
    let last = textureLoad(peeled_depth, vec2<i32>(position.xy), i32(sample));
    return !hidden(position, sample) && position.z < last;
}

@fragment
fn fragment_peel(in: VertexOut, @builtin(sample_index) sample: u32) {
    if !peelable(in.clip, sample) {
        discard;
    }
}

struct LineCoverage {
    @builtin(sample_mask) mask: u32,
}

@fragment
fn fragment_line_peel(in: LineOut, @builtin(sample_index) sample: u32) -> LineCoverage {
    if !peelable(in.clip, sample) {
        discard;
    }
    var out: LineCoverage;
    out.mask = line_fragment(in).mask;
    return out;
}

@fragment
fn fragment_model_peel(in: ModelOut, @builtin(sample_index) sample: u32) {
    let alpha = model_color(in).a;
    if !peelable(in.clip, sample) || alpha < model_cut() {
        discard;
    }
}
"#;

/// Lays the peeled layers, premultiplied, over the opaque image, sample by
/// sample.
const COMPOSITE_SHADER: &str = r#"
@group(0) @binding(0) var peeled: texture_multisampled_2d<f32>;

@vertex
fn vertex_peeled(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fragment_peeled(
    @builtin(position) at: vec4<f32>,
    @builtin(sample_index) sample: u32,
) -> @location(0) vec4<f32> {
    return textureLoad(peeled, vec2<i32>(at.xy), i32(sample));
}
"#;
