//! Native 3D meshes rasterized on the GPU: a prototype behind
//! `GAANIM_3D=gpu` (issue #563).
//!
//! The CPU path ([`crate::three_d`]) projects and shades every triangle and
//! draws it as a Vello path with its own gradient, so its cost grows with the
//! triangles three times: projecting, encoding and rasterizing. Here a mesh
//! is expanded once per change ([`MeshSources`]), and each frame is a list of
//! draws with their uniforms ([`GpuMeshFrame`]) that [`GpuThreeD`] rasterizes
//! with a depth buffer and 4x MSAA. The frame's composition draws the result
//! as one image beneath the 2D content; the image's texture is handed to Vello
//! with [`vello::Renderer::override_image`], as shader backgrounds are.
//!
//! The shading is [`crate::three_d`]'s illustrative style, evaluated per
//! pixel in linear light, where the CPU path lit vertices and let Vello
//! interpolate their colors in sRGB; a flat face keeps one color. Faces and
//! lines meet in a depth buffer, and geometry crossing the near plane is
//! clipped rather than dropped. A drawable with an opacity draws as one
//! layer, as on the CPU path: a depth prepass keeps only its nearest
//! surface, which then blends once.
//!
//! Lines, translucent colors, camera views, transitions, view layers and
//! recordings still go through the CPU path.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::num::NonZeroU64;
use std::sync::{Arc, Mutex, OnceLock};

use gaanim_core::glam::{DMat4, DVec3};
use gaanim_core::{kurbo, peniko};
use gaanim_scene::{LineListData, Material3D, TriangleMeshData};
use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
use vello::wgpu;
use wgpu::util::DeviceExt;

use crate::three_d::{Content, Illumination};

/// Whether `GAANIM_3D=gpu` asks for the GPU path.
pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("GAANIM_3D").is_ok_and(|value| value.trim().eq_ignore_ascii_case("gpu"))
    })
}

/// Bytes of an expanded mesh vertex (position and flat-face flag, normal and
/// vertex-color flag, linear color, centroid of its face) and of a line
/// segment (both ends, the first with the vertex-color flag, and their
/// linear colors).
const VERTEX_BYTES: usize = 64;
/// Bytes of the frame uniforms (`Frame` in [`SHADER`]).
const FRAME_BYTES: usize = 400;
/// Bytes of one draw's uniforms (`Draw` in [`SHADER`]), and their stride in
/// the draw buffer, the minimum uniform offset alignment.
const DRAW_BYTES: usize = 176;
const DRAW_STRIDE: usize = 256;
/// Lights the shader reads; further lights are left out.
const MAX_LIGHTS: usize = 8;
const SAMPLES: u32 = 4;
const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// A mesh expanded to one vertex per triangle corner, or a line list to one
/// instance per segment, as the shader reads it. Built once per change of
/// the mesh or the lines.
pub struct MeshSource {
    vertices: Vec<u8>,
    /// Vertices of a mesh, segments of lines.
    count: u32,
    lines: bool,
    /// The mesh carries its own colors, and whether all of them are opaque.
    has_colors: bool,
    opaque_colors: bool,
    /// Bounding sphere in model space.
    center: DVec3,
    radius: f64,
}

impl MeshSource {
    /// `None` when `mesh` has no triangle.
    fn new(mesh: &TriangleMeshData) -> Option<Self> {
        let count = mesh.vertices.len();
        if count < 3 || mesh.indices.len() < 3 {
            return None;
        }
        let positions: Vec<DVec3> = mesh
            .vertices
            .iter()
            .map(|v| DVec3::new(v[0].into(), v[1].into(), v[2].into()))
            .collect();
        let colors = mesh.colors.as_ref().filter(|colors| colors.len() == count);
        let normals: Vec<DVec3> = match mesh.normals.as_ref().filter(|n| n.len() == count) {
            Some(normals) => normals
                .iter()
                .map(|n| DVec3::new(n[0].into(), n[1].into(), n[2].into()).normalize_or_zero())
                .collect(),
            None => {
                let mut normals = vec![DVec3::ZERO; count];
                for triangle in mesh.indices.as_chunks::<3>().0 {
                    let [a, b, c] = triangle.map(|index| index as usize);
                    if a >= count || b >= count || c >= count {
                        continue;
                    }
                    let face = (positions[b] - positions[a]).cross(positions[c] - positions[a]);
                    for index in [a, b, c] {
                        normals[index] += face;
                    }
                }
                normals.iter().map(|n| n.normalize_or_zero()).collect()
            }
        };
        let mut vertices = Vec::with_capacity(mesh.indices.len() * VERTEX_BYTES);
        let mut corners = 0u32;
        for triangle in mesh.indices.as_chunks::<3>().0 {
            let index = triangle.map(|index| index as usize);
            if index.iter().any(|&index| index >= count) {
                continue;
            }
            let [a, b, c] = index.map(|index| positions[index]);
            let face = (b - a).cross(c - a).normalize_or_zero();
            let centroid = (a + b + c) / 3.0;
            let [na, nb, nc] = index.map(|index| normals[index]);
            // As the CPU path decides it: a face whose vertex normals agree
            // shades as one color.
            let flat = na != DVec3::ZERO && na.abs_diff_eq(nb, 1e-9) && na.abs_diff_eq(nc, 1e-9);
            for corner in index {
                let normal = if normals[corner] == DVec3::ZERO {
                    face
                } else {
                    normals[corner]
                };
                let p = positions[corner];
                push_f32s(
                    &mut vertices,
                    &[
                        p.x as f32,
                        p.y as f32,
                        p.z as f32,
                        f32::from(u8::from(flat)),
                    ],
                );
                push_f32s(
                    &mut vertices,
                    &[
                        normal.x as f32,
                        normal.y as f32,
                        normal.z as f32,
                        f32::from(u8::from(colors.is_some())),
                    ],
                );
                push_f32s(
                    &mut vertices,
                    &colors.map_or([0.0; 4], |colors| colors[corner]),
                );
                push_f32s(
                    &mut vertices,
                    &[centroid.x as f32, centroid.y as f32, centroid.z as f32, 0.0],
                );
                corners += 1;
            }
        }
        if corners == 0 {
            return None;
        }
        let (center, radius) = bounding_sphere(&positions);
        Some(Self {
            vertices,
            count: corners,
            lines: false,
            has_colors: colors.is_some(),
            opaque_colors: colors.is_none_or(|colors| colors.iter().all(|c| c[3] >= 0.999)),
            center,
            radius,
        })
    }

    /// The segments of `lines`, paired as the CPU path pairs them; `None`
    /// when it has none.
    fn lines(lines: &LineListData) -> Option<Self> {
        let count = lines.points.len();
        let colors = lines.colors.as_ref().filter(|colors| colors.len() == count);
        let pairs: Vec<(usize, usize)> = if lines.strip {
            (1..count).map(|index| (index - 1, index)).collect()
        } else if let Some(indices) = &lines.indices {
            indices
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&[a, b]| (a as usize, b as usize))
                .filter(|&(a, b)| a < count && b < count)
                .collect()
        } else {
            (0..count / 2)
                .map(|pair| (pair * 2, pair * 2 + 1))
                .collect()
        };
        if pairs.is_empty() {
            return None;
        }
        let mut vertices = Vec::with_capacity(pairs.len() * VERTEX_BYTES);
        for (a, b) in &pairs {
            let [pa, pb] = [lines.points[*a], lines.points[*b]];
            push_f32s(
                &mut vertices,
                &[pa[0], pa[1], pa[2], f32::from(u8::from(colors.is_some()))],
            );
            push_f32s(&mut vertices, &[pb[0], pb[1], pb[2], 0.0]);
            push_f32s(&mut vertices, &colors.map_or([0.0; 4], |colors| colors[*a]));
            push_f32s(&mut vertices, &colors.map_or([0.0; 4], |colors| colors[*b]));
        }
        let positions: Vec<DVec3> = lines
            .points
            .iter()
            .map(|p| DVec3::new(p[0].into(), p[1].into(), p[2].into()))
            .collect();
        let (center, radius) = bounding_sphere(&positions);
        Some(Self {
            vertices,
            count: pairs.len() as u32,
            lines: true,
            has_colors: colors.is_some(),
            opaque_colors: colors.is_none_or(|colors| colors.iter().all(|c| c[3] >= 0.999)),
            center,
            radius,
        })
    }
}

/// Center and radius of a sphere around `points`.
fn bounding_sphere(points: &[DVec3]) -> (DVec3, f64) {
    let (low, high) = points.iter().fold(
        (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
        |(low, high), p| (low.min(*p), high.max(*p)),
    );
    let center = (low + high) * 0.5;
    let radius = points
        .iter()
        .map(|p| p.distance(center))
        .fold(0.0, f64::max);
    (center, radius)
}

fn push_f32s(bytes: &mut Vec<u8>, values: &[f32]) {
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
}

fn push_mat4(bytes: &mut Vec<u8>, matrix: DMat4) {
    push_f32s(bytes, &matrix.to_cols_array().map(|value| value as f32));
}

/// The expanded meshes and lines of the drawables, kept from frame to frame
/// while they do not change.
#[derive(bevy::prelude::Resource, Default)]
pub struct MeshSources {
    /// By drawable and whether it is its lines.
    sources: HashMap<(u64, bool), (u32, Option<Arc<MeshSource>>)>,
    used: HashSet<(u64, bool)>,
}

impl MeshSources {
    /// The expanded mesh or lines (`lines`) of drawable `key` at `revision`,
    /// expanded by `expand` when they changed.
    fn get(
        &mut self,
        key: u64,
        lines: bool,
        revision: u32,
        expand: impl FnOnce() -> Option<MeshSource>,
    ) -> Option<Arc<MeshSource>> {
        self.used.insert((key, lines));
        match self.sources.entry((key, lines)) {
            Entry::Occupied(mut entry) => {
                let (held, source) = entry.get_mut();
                if *held != revision {
                    *held = revision;
                    *source = expand().map(Arc::new);
                }
                source.clone()
            }
            Entry::Vacant(entry) => entry.insert((revision, expand().map(Arc::new))).1.clone(),
        }
    }

    /// Forget the meshes and lines of drawables this frame did not draw.
    fn end_frame(&mut self) {
        let used = std::mem::take(&mut self.used);
        self.sources.retain(|key, _| used.contains(key));
    }
}

/// The image the meshes of a frame draw into: one per size, so that frames
/// refresh one slot of Vello's atlas.
fn frame_image(width: u32, height: u32) -> ImageData {
    static IMAGES: Mutex<Vec<ImageData>> = Mutex::new(Vec::new());
    let mut images = IMAGES.lock().expect("3D image cache poisoned");
    if let Some(image) = images
        .iter()
        .find(|image| image.width == width && image.height == height)
    {
        return image.clone();
    }
    if images.len() >= 4 {
        images.remove(0);
    }
    // Vello copies the registered texture instead of reading these bytes; a
    // missing texture draws a transparent image.
    let image = ImageData {
        data: Blob::from(vec![0_u8; width as usize * height as usize * 4]),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::AlphaPremultiplied,
        width,
        height,
    };
    images.push(image.clone());
    image
}

/// One mesh or line list of a frame, as [`GpuThreeD`] draws it.
#[derive(Clone)]
struct GpuDraw {
    source: Arc<MeshSource>,
    /// Lit meshes hide their back faces.
    culled: bool,
    /// Drawn after the opaque meshes, as one layer of its opacity.
    translucent: bool,
}

/// The meshes a frame draws on the GPU, with what the shader reads.
#[derive(Clone)]
pub struct GpuMeshFrame {
    image: ImageData,
    /// Maps the image's pixels into the scene (the inverse of the placement
    /// the meshes were drawn for).
    image_to_scene: kurbo::Affine,
    uniforms: Arc<[u8]>,
    /// Opaque draws first, then translucent ones from back to front.
    draws: Arc<[GpuDraw]>,
    /// `DRAW_STRIDE` bytes per draw.
    draw_uniforms: Arc<[u8]>,
    digest: u64,
}

impl GpuMeshFrame {
    /// Draw the frame's image in `scene`, in scene units.
    pub(crate) fn draw_image(&self, scene: &mut vello::Scene) {
        scene.draw_image(&self.image, self.image_to_scene);
    }
}

/// A mesh taken from the CPU path, waiting for the frame to be built.
struct Pending {
    source: Arc<MeshSource>,
    model: DMat4,
    opacity: f32,
    base: [f32; 4],
    material: Option<Material3D>,
}

/// Collects the meshes of a frame that the GPU draws.
pub(crate) struct GpuMeshes<'a> {
    sources: &'a mut MeshSources,
    /// Pixels of the image, and the map from scene units to them.
    size: (u32, u32),
    placement: kurbo::Affine,
    pending: Vec<Pending>,
}

impl<'a> GpuMeshes<'a> {
    /// Meshes drawn into a `size` image placed by `placement`, which maps
    /// scene units to its pixels.
    pub(crate) fn new(
        sources: &'a mut MeshSources,
        size: (u32, u32),
        placement: kurbo::Affine,
    ) -> Option<Self> {
        (size.0 > 0 && size.1 > 0 && placement.determinant().abs() > 1e-12).then_some(Self {
            sources,
            size,
            placement,
            pending: Vec::new(),
        })
    }

    /// Take the mesh of drawable `key` (its mesh at `revision`) when the GPU
    /// draws it; `false` leaves it to the CPU path.
    pub(crate) fn push(&mut self, key: u64, revision: u32, content: &Content<'_>) -> bool {
        let Some(mesh) = content.mesh else {
            return false;
        };
        let opacity = content.opacity.0.clamp(0.0, 1.0);
        let base = content
            .material
            .map(|material| material.color)
            .or(mesh.color)
            .unwrap_or(peniko::Color::WHITE);
        let Some(source) = self
            .sources
            .get(key, false, revision, || MeshSource::new(mesh))
        else {
            return false;
        };
        // Translucent colors blend with the mesh's own faces, which the
        // depth buffer does not sort.
        let opaque = if source.has_colors {
            source.opaque_colors
        } else {
            base.components[3] >= 0.999
        };
        if !opaque {
            return false;
        }
        if opacity > f32::EPSILON {
            self.pending.push(Pending {
                source,
                model: content.transform.mat4,
                opacity,
                base: to_linear(base),
                material: content.material.copied(),
            });
        }
        true
    }

    /// Take the lines of drawable `key` (its lines at `revision`) when the
    /// GPU draws them; `false` leaves them to the CPU path.
    pub(crate) fn push_lines(&mut self, key: u64, revision: u32, content: &Content<'_>) -> bool {
        let Some(lines) = content.lines else {
            return false;
        };
        let Some(source) = self
            .sources
            .get(key, true, revision, || MeshSource::lines(lines))
        else {
            return false;
        };
        let opaque = if source.has_colors {
            source.opaque_colors
        } else {
            lines.color.components[3] >= 0.999
        };
        if !opaque {
            return false;
        }
        let opacity = content.opacity.0.clamp(0.0, 1.0);
        if opacity > f32::EPSILON {
            self.pending.push(Pending {
                source,
                model: content.transform.mat4,
                opacity,
                base: to_linear(lines.color),
                material: None,
            });
        }
        true
    }

    /// The frame of the meshes and lines taken, seen by `camera`, whose
    /// canvas projection maps NDC to scene units by `ndc_to_scene` and draws
    /// lines `line_width` scene units wide.
    pub(crate) fn finish(
        self,
        camera: &gaanim_math::Camera,
        ndc_to_scene: kurbo::Affine,
        line_width: f64,
        illumination: &Illumination,
    ) -> Option<GpuMeshFrame> {
        self.sources.end_frame();
        if self.pending.is_empty() {
            return None;
        }
        let (width, height) = self.size;
        let image = frame_image(width, height);
        let pixels_to_ndc = kurbo::Affine::new([
            2.0 / f64::from(width),
            0.0,
            0.0,
            -2.0 / f64::from(height),
            -1.0,
            1.0,
        ]);
        let [a, b, c, d, e, f] = (pixels_to_ndc * self.placement * ndc_to_scene).as_coeffs();
        let perspective = matches!(
            camera.projection,
            gaanim_math::Projection::Perspective { .. }
        );
        let forward = camera.rotation * DVec3::NEG_Z;
        let lighting = &illumination.lighting;
        let intensity = if lighting.enabled {
            lighting.intensity.max(0.0)
        } else {
            0.0
        };
        let lights = &illumination.lights[..illumination.lights.len().min(MAX_LIGHTS)];

        let mut uniforms = Vec::with_capacity(FRAME_BYTES);
        push_mat4(
            &mut uniforms,
            camera.projection_matrix() * camera.view_matrix(),
        );
        // With half the image's size in pixels, which lines are measured in.
        push_f32s(
            &mut uniforms,
            &[a as f32, c as f32, e as f32, width as f32 * 0.5],
        );
        push_f32s(
            &mut uniforms,
            &[b as f32, d as f32, f as f32, height as f32 * 0.5],
        );
        let eye = camera.position;
        push_f32s(
            &mut uniforms,
            &[
                eye.x as f32,
                eye.y as f32,
                eye.z as f32,
                f32::from(u8::from(perspective)),
            ],
        );
        push_f32s(
            &mut uniforms,
            &[
                forward.x as f32,
                forward.y as f32,
                forward.z as f32,
                intensity,
            ],
        );
        let line_px = line_width * self.placement.determinant().abs().sqrt();
        push_f32s(
            &mut uniforms,
            &[
                f32::from(u8::from(lights.is_empty())),
                lights.len() as f32,
                (line_px * 0.5) as f32,
                0.0,
            ],
        );
        for index in 0..MAX_LIGHTS {
            match lights.get(index) {
                Some(light) => {
                    let p = light.position;
                    push_f32s(
                        &mut uniforms,
                        &[
                            p.x as f32,
                            p.y as f32,
                            p.z as f32,
                            f32::from(u8::from(light.directional)),
                        ],
                    );
                    push_f32s(
                        &mut uniforms,
                        &[light.color[0], light.color[1], light.color[2], 0.0],
                    );
                }
                None => push_f32s(&mut uniforms, &[0.0; 8]),
            }
        }
        debug_assert_eq!(uniforms.len(), FRAME_BYTES);

        // Opaque meshes first, near ones first so that the depth test skips
        // what they hide; translucent ones after them, from back to front.
        let view = camera.view_matrix();
        let mut pending: Vec<(f64, Pending)> = self
            .pending
            .into_iter()
            .map(|pending| {
                let center = pending.model.transform_point3(pending.source.center);
                let scale = [
                    pending.model.x_axis.truncate().length(),
                    pending.model.y_axis.truncate().length(),
                    pending.model.z_axis.truncate().length(),
                ]
                .into_iter()
                .fold(0.0, f64::max);
                let depth = -view.transform_point3(center).z + pending.source.radius * scale;
                (depth, pending)
            })
            .collect();
        pending.sort_by(|(a_depth, a), (b_depth, b)| {
            let (a_translucent, b_translucent) = (a.opacity < 0.999, b.opacity < 0.999);
            a_translucent.cmp(&b_translucent).then_with(|| {
                if a_translucent {
                    b_depth.total_cmp(a_depth)
                } else {
                    a_depth.total_cmp(b_depth)
                }
            })
        });

        let mut draws = Vec::with_capacity(pending.len());
        let mut draw_uniforms = Vec::with_capacity(pending.len() * DRAW_STRIDE);
        for (_, pending) in pending {
            let start = draw_uniforms.len();
            push_mat4(&mut draw_uniforms, pending.model);
            push_mat4(&mut draw_uniforms, pending.model.inverse().transpose());
            push_f32s(&mut draw_uniforms, &pending.base);
            let (emissive, surface) = match pending.material {
                Some(material) => {
                    let glow = to_linear(material.emissive);
                    let strength = material.emissive_strength.max(0.0);
                    (
                        [glow[0] * strength, glow[1] * strength, glow[2] * strength],
                        [material.roughness, material.metallic, 1.0, 0.0],
                    )
                }
                None => ([0.0; 3], [0.0; 4]),
            };
            push_f32s(
                &mut draw_uniforms,
                &[emissive[0], emissive[1], emissive[2], pending.opacity],
            );
            push_f32s(&mut draw_uniforms, &surface);
            debug_assert_eq!(draw_uniforms.len() - start, DRAW_BYTES);
            draw_uniforms.resize(start + DRAW_STRIDE, 0);
            draws.push(GpuDraw {
                culled: pending.material.is_some() && !pending.source.lines,
                translucent: pending.opacity < 0.999,
                source: pending.source,
            });
        }

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        image.data.id().hash(&mut hasher);
        uniforms.hash(&mut hasher);
        draw_uniforms.hash(&mut hasher);
        for draw in &draws {
            (Arc::as_ptr(&draw.source) as usize).hash(&mut hasher);
            draw.culled.hash(&mut hasher);
            draw.source.lines.hash(&mut hasher);
            draw.translucent.hash(&mut hasher);
        }
        Some(GpuMeshFrame {
            image,
            image_to_scene: self.placement.inverse(),
            uniforms: uniforms.into(),
            draws: draws.into(),
            draw_uniforms: draw_uniforms.into(),
            digest: hasher.finish(),
        })
    }
}

fn to_linear(color: peniko::Color) -> [f32; 4] {
    let [r, g, b, a] = color.components;
    let linear = |value: f32| {
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    [linear(r), linear(g), linear(b), a]
}

/// Where the meshes are drawn: a multisampled color and depth target and
/// the texture they resolve into, which Vello draws as the frame's image.
struct Target {
    image: ImageData,
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    resolved: wgpu::Texture,
    resolved_view: wgpu::TextureView,
}

impl Target {
    fn new(device: &wgpu::Device, image: &ImageData) -> Self {
        let texture = |label, samples, format, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: image.width,
                    height: image.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let color = texture(
            "gaanim-3d-color",
            SAMPLES,
            COLOR_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let depth = texture(
            "gaanim-3d-depth",
            SAMPLES,
            DEPTH_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let resolved = texture(
            "gaanim-3d-image",
            1,
            COLOR_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        Self {
            image: image.clone(),
            color: color.create_view(&Default::default()),
            depth: depth.create_view(&Default::default()),
            resolved_view: resolved.create_view(&Default::default()),
            resolved,
        }
    }

    fn texture_copy(&self) -> wgpu::TexelCopyTextureInfoBase<wgpu::Texture> {
        wgpu::TexelCopyTextureInfoBase {
            texture: self.resolved.clone(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        }
    }
}

/// What a draw does in its pass.
#[derive(Clone, Copy)]
enum Pass {
    /// Opaque: writes color and depth.
    Opaque = 0,
    /// A translucent mesh's depth alone, so that only its nearest surface
    /// draws.
    Depth = 1,
    /// That surface, blended once at the mesh's opacity.
    Blend = 2,
}

struct Pipelines {
    layout: wgpu::BindGroupLayout,
    /// Meshes by `[culled][pass]`.
    pipelines: [[wgpu::RenderPipeline; 3]; 2],
    /// Lines by pass.
    lines: [wgpu::RenderPipeline; 3],
}

impl Pipelines {
    fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gaanim-3d-mesh"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let uniform = |binding, dynamic, size: usize| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: dynamic,
                min_binding_size: NonZeroU64::new(size as u64),
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gaanim-3d-mesh"),
            entries: &[uniform(0, false, FRAME_BYTES), uniform(1, true, DRAW_BYTES)],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gaanim-3d-mesh"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let attributes: [wgpu::VertexAttribute; 4] =
            std::array::from_fn(|index| wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: index as u64 * 16,
                shader_location: index as u32,
            });
        let pipeline = |lines: bool, culled: bool, pass: Pass| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("gaanim-3d-mesh"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some(if lines { "vertex_line" } else { "vertex" }),
                    compilation_options: Default::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: VERTEX_BYTES as u64,
                        step_mode: if lines {
                            wgpu::VertexStepMode::Instance
                        } else {
                            wgpu::VertexStepMode::Vertex
                        },
                        attributes: &attributes,
                    }],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: culled.then_some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(!matches!(pass, Pass::Blend)),
                    // Reversed depth: near is 1, far 0.
                    depth_compare: Some(match pass {
                        Pass::Opaque => wgpu::CompareFunction::Greater,
                        Pass::Depth => wgpu::CompareFunction::GreaterEqual,
                        Pass::Blend => wgpu::CompareFunction::Equal,
                    }),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: SAMPLES,
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(if lines { "fragment_line" } else { "fragment" }),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend: matches!(pass, Pass::Blend)
                            .then_some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: if matches!(pass, Pass::Depth) {
                            wgpu::ColorWrites::empty()
                        } else {
                            wgpu::ColorWrites::ALL
                        },
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let passes = |lines, culled| {
            [
                pipeline(lines, culled, Pass::Opaque),
                pipeline(lines, culled, Pass::Depth),
                pipeline(lines, culled, Pass::Blend),
            ]
        };
        Self {
            layout,
            pipelines: [passes(false, false), passes(false, true)],
            lines: passes(true, false),
        }
    }
}

/// Draws [`GpuMeshFrame`]s on the device of a Vello renderer.
#[derive(Default)]
pub struct GpuThreeD {
    device: Option<wgpu::Device>,
    pipelines: Option<Pipelines>,
    /// Vertex buffers by mesh; holding the mesh keeps its address unique.
    meshes: HashMap<usize, (Arc<MeshSource>, wgpu::Buffer)>,
    target: Option<Target>,
    frame_buffer: Option<wgpu::Buffer>,
    /// The draw uniforms, with room for that many bytes, and their bind group.
    draw_buffer: Option<(wgpu::Buffer, usize, wgpu::BindGroup)>,
    /// Digest of the frame the image holds.
    drawn: Option<u64>,
}

impl GpuThreeD {
    /// Forget what the previous renderer held; the next frame registers its
    /// image with the new one.
    pub fn renderer_replaced(&mut self) {
        self.target = None;
        self.drawn = None;
    }

    /// Draw `frame` into its image for the next render of `renderer`, unless
    /// the image already holds it; returns whether the image changed. Call
    /// before the frame is submitted to `queue`.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut vello::Renderer,
        frame: Option<&GpuMeshFrame>,
    ) -> bool {
        if self.device.as_ref() != Some(device) {
            if let Some(target) = self.target.take() {
                renderer.override_image(&target.image, None);
            }
            *self = Self {
                device: Some(device.clone()),
                ..Self::default()
            };
        }
        let Some(frame) = frame else {
            if let Some(target) = self.target.take() {
                renderer.override_image(&target.image, None);
            }
            self.meshes.clear();
            self.drawn = None;
            return false;
        };
        if self
            .target
            .as_ref()
            .is_none_or(|target| target.image.data.id() != frame.image.data.id())
        {
            if let Some(target) = self.target.take() {
                renderer.override_image(&target.image, None);
            }
            let target = Target::new(device, &frame.image);
            renderer.override_image(&target.image, Some(target.texture_copy()));
            self.target = Some(target);
            self.drawn = None;
        }
        if self.drawn == Some(frame.digest) {
            return false;
        }
        let pipelines = self.pipelines.get_or_insert_with(|| Pipelines::new(device));

        self.meshes.retain(|key, _| {
            frame
                .draws
                .iter()
                .any(|draw| Arc::as_ptr(&draw.source) as usize == *key)
        });
        for draw in frame.draws.iter() {
            self.meshes
                .entry(Arc::as_ptr(&draw.source) as usize)
                .or_insert_with(|| {
                    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("gaanim-3d-vertices"),
                        contents: &draw.source.vertices,
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    (Arc::clone(&draw.source), buffer)
                });
        }

        let frame_buffer = self.frame_buffer.get_or_insert_with(|| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gaanim-3d-frame"),
                size: FRAME_BYTES as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        queue.write_buffer(frame_buffer, 0, &frame.uniforms);
        let needed = frame.draw_uniforms.len().max(DRAW_STRIDE);
        if self
            .draw_buffer
            .as_ref()
            .is_none_or(|(_, capacity, _)| *capacity < needed)
        {
            let capacity = needed.next_power_of_two();
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gaanim-3d-draws"),
                size: capacity as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("gaanim-3d-mesh"),
                layout: &pipelines.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: frame_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &buffer,
                            offset: 0,
                            size: NonZeroU64::new(DRAW_BYTES as u64),
                        }),
                    },
                ],
            });
            self.draw_buffer = Some((buffer, capacity, bind_group));
        }
        let (draw_buffer, _, bind_group) = self.draw_buffer.as_ref().expect("draw buffer is set");
        queue.write_buffer(draw_buffer, 0, &frame.draw_uniforms);

        let target = self.target.as_ref().expect("target is set");
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("gaanim-3d-mesh"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gaanim-3d-mesh"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.color,
                    depth_slice: None,
                    resolve_target: Some(&target.resolved_view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth,
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
            for (index, draw) in frame.draws.iter().enumerate() {
                let Some((_, vertices)) = self.meshes.get(&(Arc::as_ptr(&draw.source) as usize))
                else {
                    continue;
                };
                let pipelines = if draw.source.lines {
                    &pipelines.lines
                } else {
                    &pipelines.pipelines[usize::from(draw.culled)]
                };
                pass.set_bind_group(0, bind_group, &[(index * DRAW_STRIDE) as u32]);
                pass.set_vertex_buffer(0, vertices.slice(..));
                let passes: &[Pass] = if draw.translucent {
                    &[Pass::Depth, Pass::Blend]
                } else {
                    &[Pass::Opaque]
                };
                for &each in passes {
                    pass.set_pipeline(&pipelines[each as usize]);
                    if draw.source.lines {
                        // A quad per segment.
                        pass.draw(0..6, 0..draw.source.count);
                    } else {
                        pass.draw(0..draw.source.count, 0..1);
                    }
                }
            }
        }
        queue.submit(Some(encoder.finish()));
        renderer.mark_override_image_dirty(&target.image);
        self.drawn = Some(frame.digest);
        true
    }
}

/// The mesh shader: [`crate::three_d`]'s projection and illustrative shading,
/// per vertex.
const SHADER: &str = r#"
struct Light {
    position: vec4<f32>,
    color: vec4<f32>,
}

struct Frame {
    view_proj: mat4x4<f32>,
    // NDC to the image's NDC, applied to clip x, y and w; and half the
    // image's width and height in pixels.
    ndc_x: vec4<f32>,
    ndc_y: vec4<f32>,
    // Eye, and 1 under perspective.
    eye: vec4<f32>,
    // View direction, and the scene's light intensity.
    forward: vec4<f32>,
    // 1 when the fixed key light shines, the count of lights, and half the
    // width of lines in pixels.
    params: vec4<f32>,
    lights: array<Light, 8>,
}

struct Draw {
    model: mat4x4<f32>,
    normal: mat4x4<f32>,
    // Linear base color.
    base: vec4<f32>,
    // Linear emission times its strength, and the drawable's opacity.
    emissive: vec4<f32>,
    // Roughness, metalness, and 1 when lit.
    material: vec4<f32>,
}

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<uniform> draw: Draw;

struct VertexIn {
    // Position, and 1 on a flat face.
    @location(0) position: vec4<f32>,
    // Normal, and 1 when the vertex has a color.
    @location(1) normal: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) centroid: vec4<f32>,
}

struct VertexOut {
    @invariant @builtin(position) clip: vec4<f32>,
    // Linear color: the surface's, or a flat face's shaded color.
    @location(0) color: vec4<f32>,
    // World position and normal of a smooth lit surface, shaded per pixel.
    @location(1) world: vec3<f32>,
    @location(2) normal: vec3<f32>,
    // 1 when the pixel is lit by `shade`; 0 when `color` is final.
    @location(3) @interpolate(flat) lit: u32,
}

const KEY_LIGHT = vec3<f32>(-3.0, 6.0, 4.0);
const KEY_WRAP: f32 = 0.35;
const KEY_STRENGTH: f32 = 0.62;
const SKY: f32 = 0.5;
const GROUND: f32 = 0.3;
const SKY_TINT = vec3<f32>(0.86, 0.92, 1.0);
const RIM: f32 = 0.22;

fn normalize_or_zero(v: vec3<f32>) -> vec3<f32> {
    let size = length(v);
    if size > 1e-20 {
        return v / size;
    }
    return vec3<f32>(0.0);
}

fn toward_viewer(point: vec3<f32>) -> vec3<f32> {
    if frame.eye.w > 0.5 {
        return normalize_or_zero(frame.eye.xyz - point);
    }
    return -frame.forward.xyz;
}

fn to_srgb(value: f32) -> f32 {
    let v = clamp(value, 0.0, 1.0);
    if v <= 0.0031308 {
        return v * 12.92;
    }
    return 1.055 * pow(v, 1.0 / 2.4) - 0.055;
}

fn shade(base: vec4<f32>, normal: vec3<f32>, viewer: vec3<f32>, point: vec3<f32>) -> vec4<f32> {
    let metallic = clamp(draw.material.y, 0.0, 1.0);
    let gloss = 1.0 - clamp(draw.material.x, 0.0, 1.0);
    let polish = gloss * gloss;
    let intensity = frame.forward.w;
    let facing_up = 0.5 + 0.5 * normal.y;
    let sky = GROUND + (SKY - GROUND) * facing_up;
    let shininess = 8.0 + 248.0 * polish;
    let fixed = frame.params.x > 0.5;
    let count = select(u32(frame.params.y), 1u, fixed);
    var keyed = vec3<f32>(0.0);
    var highlight = vec3<f32>(0.0);
    for (var i = 0u; i < count; i = i + 1u) {
        var light = Light(vec4<f32>(KEY_LIGHT, 1.0), vec4<f32>(1.0));
        if !fixed {
            light = frame.lights[i];
        }
        var toward_light = light.position.xyz;
        if light.position.w < 0.5 {
            toward_light = light.position.xyz - point;
        }
        toward_light = normalize_or_zero(toward_light);
        let key = max((dot(normal, toward_light) + KEY_WRAP) / (1.0 + KEY_WRAP), 0.0);
        let half = normalize_or_zero(toward_light + viewer);
        let shine = pow(max(dot(normal, half), 0.0), shininess) * polish * min(key, 1.0);
        keyed += KEY_STRENGTH * key * light.color.rgb;
        highlight += shine * light.color.rgb;
    }
    var diffuse: vec3<f32>;
    if fixed {
        diffuse = (vec3<f32>(sky) + keyed) * intensity;
        highlight = highlight * intensity;
    } else {
        diffuse = vec3<f32>(sky * intensity) + keyed;
    }
    let reflection = (0.25 + 0.95 * facing_up * facing_up) * intensity;
    let facing = clamp(dot(normal, viewer), 0.0, 1.0);
    let rim = RIM * pow(1.0 - facing, 3.0) * intensity;
    let dielectric = base.rgb * diffuse + highlight * 0.9;
    let metal = base.rgb * reflection * SKY_TINT + highlight * (0.35 + 0.65 * base.rgb);
    let rgb = dielectric + (metal - dielectric) * metallic
        + rim * SKY_TINT * (0.4 + 0.6 * base.rgb) + draw.emissive.rgb;
    return vec4<f32>(rgb, base.a);
}

// Clip coordinates of a world point in the image, with depth reversed (near
// is 1, far 0), which a float depth buffer holds far more precisely.
fn image_clip(world: vec3<f32>) -> vec4<f32> {
    let clip = frame.view_proj * vec4<f32>(world, 1.0);
    let xyw = vec3<f32>(clip.x, clip.y, clip.w);
    return vec4<f32>(
        dot(frame.ndc_x.xyz, xyw),
        dot(frame.ndc_y.xyz, xyw),
        clip.w - clip.z,
        clip.w,
    );
}

// How much farther lines lie than they are, relative to their depth: a line
// on a surface, as a floor grid under a plot, stays beneath it.
const LINE_DEPTH_BIAS: f32 = 1e-5;

fn srgb_color(color: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(
        to_srgb(color.r),
        to_srgb(color.g),
        to_srgb(color.b),
        clamp(color.a, 0.0, 1.0),
    );
}

@vertex
fn vertex(in: VertexIn) -> VertexOut {
    let world = (draw.model * vec4<f32>(in.position.xyz, 1.0)).xyz;
    var out: VertexOut;
    out.clip = image_clip(world);
    out.world = world;
    out.normal = (draw.normal * vec4<f32>(in.normal.xyz, 0.0)).xyz;
    out.lit = 0u;
    let colored = in.normal.w > 0.5;
    out.color = draw.base;
    if colored {
        out.color = in.color;
    }
    if draw.material.z > 0.5 {
        if in.position.w > 0.5 && !colored {
            // A flat face has one color, lit as seen along the view axis.
            let centroid = (draw.model * vec4<f32>(in.centroid.xyz, 1.0)).xyz;
            out.color = shade(draw.base, normalize_or_zero(out.normal), -frame.forward.xyz, centroid);
        } else {
            out.lit = 1u;
        }
    }
    return out;
}

@fragment
fn fragment(in: VertexOut) -> @location(0) vec4<f32> {
    var color = in.color;
    if in.lit == 1u {
        color = shade(color, normalize_or_zero(in.normal), toward_viewer(in.world), in.world);
    }
    let srgb = srgb_color(color);
    let alpha = srgb.a * draw.emissive.w;
    return vec4<f32>(srgb.rgb * alpha, alpha);
}

struct LineIn {
    // First end, and 1 when the ends have their own colors.
    @location(0) start: vec4<f32>,
    @location(1) end: vec4<f32>,
    @location(2) start_color: vec4<f32>,
    @location(3) end_color: vec4<f32>,
}

struct LineOut {
    @invariant @builtin(position) clip: vec4<f32>,
    // Linear color.
    @location(0) @interpolate(linear) color: vec4<f32>,
    // Where the fragment lies, and the segment's ends, in image pixels.
    @location(1) @interpolate(linear) pixel: vec2<f32>,
    @location(2) @interpolate(flat) ends: vec4<f32>,
}

// The corners of a segment's quad: which end, and which side.
const CORNERS = array<vec2<f32>, 6>(
    vec2<f32>(0.0, -1.0),
    vec2<f32>(1.0, -1.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, -1.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, 1.0),
);

@vertex
fn vertex_line(@builtin(vertex_index) index: u32, in: LineIn) -> LineOut {
    var out: LineOut;
    var start = image_clip((draw.model * vec4<f32>(in.start.xyz, 1.0)).xyz);
    var end = image_clip((draw.model * vec4<f32>(in.end.xyz, 1.0)).xyz);
    // The part in front of the near plane, where reversed depth reaches w.
    let start_out = start.z - start.w;
    let end_out = end.z - end.w;
    if start_out > 0.0 && end_out > 0.0 {
        out.clip = vec4<f32>(0.0, 0.0, -1.0, 1.0);
        return out;
    }
    if start_out > 0.0 {
        start = mix(start, end, start_out / (start_out - end_out));
    }
    if end_out > 0.0 {
        end = mix(end, start, end_out / (end_out - start_out));
    }
    let half_size = vec2<f32>(frame.ndc_x.w, frame.ndc_y.w);
    let s0 = start.xy / start.w * half_size;
    let s1 = end.xy / end.w * half_size;
    let span = s1 - s0;
    let span_length = length(span);
    var along = vec2<f32>(1.0, 0.0);
    if span_length > 1e-6 {
        along = span / span_length;
    }
    let across = vec2<f32>(-along.y, along.x);
    // Half the width, and a pixel of antialiasing around it.
    let reach = frame.params.z + 1.0;
    var corners = CORNERS;
    let corner = corners[index];
    var pixel = s0 - along * reach;
    var clip = start;
    var color = in.start_color;
    if corner.x > 0.5 {
        pixel = s1 + along * reach;
        clip = end;
        color = in.end_color;
    }
    if in.start.w < 0.5 {
        color = draw.base;
    }
    pixel += across * corner.y * reach;
    out.clip = vec4<f32>(pixel / half_size * clip.w, clip.z * (1.0 - LINE_DEPTH_BIAS), clip.w);
    out.pixel = pixel;
    out.ends = vec4<f32>(s0, s1);
    out.color = color;
    return out;
}

struct LineFragment {
    @location(0) color: vec4<f32>,
    // The samples a round-capped segment covers, as many as its coverage.
    @builtin(sample_mask) mask: u32,
}

@fragment
fn fragment_line(in: LineOut) -> LineFragment {
    let s0 = in.ends.xy;
    let span = in.ends.zw - s0;
    let t = clamp(dot(in.pixel - s0, span) / max(dot(span, span), 1e-12), 0.0, 1.0);
    let distance = length(in.pixel - (s0 + span * t));
    let coverage = clamp(frame.params.z + 0.5 - distance, 0.0, 1.0);
    let samples = u32(round(coverage * 4.0));
    var out: LineFragment;
    out.mask = (1u << samples) - 1u;
    let srgb = srgb_color(in.color);
    let alpha = srgb.a * draw.emissive.w;
    out.color = vec4<f32>(srgb.rgb * alpha, alpha);
    return out;
}
"#;

#[cfg(not(target_os = "emscripten"))]
pub use canvas::GpuThreeDFrame;
#[cfg(not(target_os = "emscripten"))]
pub(crate) use canvas::{RenderGpuThreeD, build};

/// The interactive canvas's side: the composed frame's meshes reach the
/// render world, which draws them before Vello renders the canvas.
#[cfg(not(target_os = "emscripten"))]
mod canvas {
    use std::sync::{Arc, Mutex, Weak};

    use bevy::prelude::*;
    use bevy::render::renderer::{RenderDevice, RenderQueue};
    use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};

    use super::{GpuMeshFrame, GpuThreeD};
    use crate::canvas::VelloRenderer;

    /// The meshes of the latest composed frame. Present only when the GPU
    /// path is on and a render world draws them.
    #[derive(Resource, Default)]
    pub struct GpuThreeDFrame(pub(crate) Option<GpuMeshFrame>);

    #[derive(Resource, Default)]
    struct ExtractedGpuThreeD(Option<GpuMeshFrame>);

    #[derive(Resource, Default)]
    pub(crate) struct RenderGpuThreeD {
        three_d: GpuThreeD,
        renderer: Weak<Mutex<vello::Renderer>>,
        /// The image changed this frame, so the canvas draws it again.
        pub(crate) changed: bool,
    }

    pub(crate) fn build(app: &mut App) {
        if !super::enabled() {
            return;
        }
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<ExtractedGpuThreeD>()
            .init_resource::<RenderGpuThreeD>()
            .add_systems(ExtractSchedule, extract_meshes)
            .add_systems(
                Render,
                prepare_meshes
                    .in_set(RenderSystems::PrepareResources)
                    .run_if(resource_exists::<RenderDevice>)
                    .run_if(resource_exists::<VelloRenderer>),
            );
        app.init_resource::<GpuThreeDFrame>();
    }

    fn extract_meshes(
        frame: Extract<Option<Res<GpuThreeDFrame>>>,
        mut extracted: ResMut<ExtractedGpuThreeD>,
    ) {
        extracted.0 = frame.as_ref().and_then(|frame| frame.0.clone());
    }

    fn prepare_meshes(
        extracted: Res<ExtractedGpuThreeD>,
        device: Res<RenderDevice>,
        queue: Res<RenderQueue>,
        vello_renderer: Res<VelloRenderer>,
        mut state: ResMut<RenderGpuThreeD>,
    ) {
        let shared: &Arc<Mutex<vello::Renderer>> = &vello_renderer;
        let Ok(mut renderer) = shared.lock() else {
            return;
        };
        let state = &mut *state;
        if !Weak::ptr_eq(&state.renderer, &Arc::downgrade(shared)) {
            state.three_d.renderer_replaced();
            state.renderer = Arc::downgrade(shared);
        }
        state.changed = state.three_d.prepare(
            device.wgpu_device(),
            &queue,
            &mut renderer,
            extracted.0.as_ref(),
        );
    }
}
