//! RGBA to NV12 on the GPU for video exports.
//!
//! FFmpeg converted every RGBA frame to YUV 4:2:0 on the CPU, after the
//! render loop had read 4 bytes per pixel back from the GPU. A compute pass
//! now writes the frame as NV12 (a luma plane, then interleaved Cb and Cr
//! at half resolution), so the readback and the pipe carry 1.5 bytes per
//! pixel and FFmpeg only rearranges planes. It uses the conversion FFmpeg
//! applied by default to untagged RGB input: BT.601 coefficients, limited
//! range, on the sRGB-encoded values; chroma is the mean of each 2x2 block.

use vello::wgpu;

/// Converts the export target to NV12 into a buffer the readback copies.
pub(crate) struct Nv12Converter {
    pipeline: wgpu::ComputePipeline,
    bind_group: wgpu::BindGroup,
    buffer: wgpu::Buffer,
    width: u32,
    height: u32,
}

impl Nv12Converter {
    /// A converter for `texture` (`width` by `height`, RGBA8), or `None`
    /// when the size cannot be packed: each invocation writes whole 32-bit
    /// words of four pixels across two rows.
    pub(crate) fn new(
        device: &wgpu::Device,
        texture: &wgpu::Texture,
        width: u32,
        height: u32,
    ) -> Option<Self> {
        if width == 0 || height == 0 || !width.is_multiple_of(4) || !height.is_multiple_of(2) {
            return None;
        }
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gaanim-export-nv12"),
            source: wgpu::ShaderSource::Wgsl(shader_source(width, height).into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gaanim-export-nv12-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gaanim-export-nv12-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("gaanim-export-nv12-pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gaanim-export-nv12"),
            size: frame_bytes(width, height) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gaanim-export-nv12-bind-group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buffer.as_entire_binding(),
                },
            ],
        });
        Some(Self {
            pipeline,
            bind_group,
            buffer,
            width,
            height,
        })
    }

    /// Bytes of one converted frame.
    pub(crate) fn frame_bytes(&self) -> usize {
        frame_bytes(self.width, self.height)
    }

    /// Convert the target and copy the frame to the start of `staging`.
    pub(crate) fn encode(&self, encoder: &mut wgpu::CommandEncoder, staging: &wgpu::Buffer) {
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("gaanim-export-nv12-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(
                (self.width / 4).div_ceil(8),
                (self.height / 2).div_ceil(8),
                1,
            );
        }
        encoder.copy_buffer_to_buffer(&self.buffer, 0, staging, 0, self.frame_bytes() as u64);
    }
}

/// Bytes of an NV12 frame: the luma plane and the half-height chroma plane.
pub(crate) fn frame_bytes(width: u32, height: u32) -> usize {
    width as usize * height as usize * 3 / 2
}

/// One invocation converts four pixels across two rows: two luma words and
/// one word of two Cb, Cr pairs.
fn shader_source(width: u32, height: u32) -> String {
    format!(
        r#"
const WIDTH: u32 = {width}u;
const HEIGHT: u32 = {height}u;

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> frame: array<u32>;

fn luma(c: vec3<f32>) -> u32 {{
    return u32(clamp(round(16.0 + 65.481 * c.r + 128.553 * c.g + 24.966 * c.b), 0.0, 255.0));
}}

fn chroma(c: vec3<f32>) -> u32 {{
    let cb = clamp(round(128.0 - 37.797 * c.r - 74.203 * c.g + 112.0 * c.b), 0.0, 255.0);
    let cr = clamp(round(128.0 + 112.0 * c.r - 93.786 * c.g - 18.214 * c.b), 0.0, 255.0);
    return u32(cb) | (u32(cr) << 8u);
}}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {{
    let x = id.x * 4u;
    let y = id.y * 2u;
    if (x >= WIDTH || y >= HEIGHT) {{
        return;
    }}
    var pixels: array<vec3<f32>, 8>;
    for (var row = 0u; row < 2u; row++) {{
        var word = 0u;
        for (var column = 0u; column < 4u; column++) {{
            let c = textureLoad(source, vec2<u32>(x + column, y + row), 0).rgb;
            pixels[row * 4u + column] = c;
            word |= luma(c) << (8u * column);
        }}
        frame[((y + row) * WIDTH + x) / 4u] = word;
    }}
    let left = (pixels[0] + pixels[1] + pixels[4] + pixels[5]) * 0.25;
    let right = (pixels[2] + pixels[3] + pixels[6] + pixels[7]) * 0.25;
    frame[(WIDTH * HEIGHT + (y / 2u) * WIDTH + x) / 4u] = chroma(left) | (chroma(right) << 16u);
}}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_pack_whole_words_or_are_refused() {
        assert_eq!(frame_bytes(1920, 1080), 1920 * 1080 * 3 / 2);
        assert!(shader_source(8, 2).contains("const WIDTH: u32 = 8u;"));
    }
}
