//! One 3D mesh becoming another: both sampled on a shared grid.
//!
//! Two meshes rarely share their vertices, so a morph samples each at the
//! same `COLUMNS + 1` by `ROWS + 1` grid of parameters and moves every grid
//! point from one mesh to the other. A mesh is sampled
//!
//! - by its texture coordinates, when they cover the unit square once, as a
//!   sphere's, a torus's, a plane's or a revolved profile's do;
//! - otherwise by rays from the center of its bounds, at the outermost
//!   point each ray meets (a cube, an extruded letter, a capped cylinder).
//!   A ray's longitude and polar angle are the sphere's texture
//!   coordinates, so a sphere and a cube meet point for point;
//! - and when most rays miss it, as an open sheet, by its points seen
//!   along its thinnest side.
//!
//! Each grid runs so that its quads face the way the mesh's normals do,
//! and the second one is then turned and shifted along the sides it wraps
//! around to the parameters that move its points least, as two outlines
//! pair in 2D: a sphere's poles open into a torus's hole.

use std::collections::VecDeque;
use std::f32::consts::{PI, TAU};
use std::sync::Arc;

use gaanim_core::glam::{Vec2, Vec3};
use gaanim_scene::{MeshBlend, TriangleMeshData};

/// Grid steps around (`u`, the longitude of a ray).
pub const COLUMNS: usize = 128;
/// Grid steps along (`v`, the polar angle of a ray from +y).
pub const ROWS: usize = 96;
const STRIDE: usize = COLUMNS + 1;
const POINTS: usize = STRIDE * (ROWS + 1);

/// Smallest share of rays that must meet a mesh for rays to sample it.
const RAY_HITS: f32 = 0.5;
/// Largest share of the grid texture coordinates may leave uncovered.
const UV_GAPS: f32 = 0.03;
/// Steps between the shifts, and between the points, a first search tries.
const COARSE: usize = 4;

fn index(i: usize, j: usize) -> usize {
    j * STRIDE + i
}

/// How a mesh was sampled; two meshes sampled by rays share their grid
/// already.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sampling {
    TextureCoordinates,
    Rays,
    Sheet,
}

/// A mesh sampled at every grid point.
#[derive(Debug, Clone)]
struct Grid {
    positions: Vec<Vec3>,
    normals: Vec<Vec3>,
    colors: Option<Vec<[f32; 4]>>,
    sampling: Sampling,
    /// Whether the first and last columns (rows) are the same points.
    wraps: (bool, bool),
}

/// Where a triangle of the mesh meets a grid point.
#[derive(Debug, Clone, Copy)]
struct Hit {
    triangle: usize,
    weights: Vec3,
    /// How far out the point lies: the ray's distance, or a sheet's height.
    depth: f32,
    /// For a point on a crease, the triangle and weights of the same point
    /// on the crease's other side, whose normal it shares.
    across: Option<(usize, Vec3)>,
}

/// Two meshes sampled on one grid, as two meshes of one layout whose
/// vertices pair: each frame of the morph blends them ([`MeshBlend`]).
#[derive(Debug, Clone)]
pub struct MeshMorph {
    from: Arc<TriangleMeshData>,
    to: Arc<TriangleMeshData>,
}

impl MeshMorph {
    /// The morph of `from` into `to`, whose base colors fill in the
    /// vertex colors of a mesh without them when the other has them.
    /// `None` when either has no triangle to sample.
    pub fn new(
        from: &TriangleMeshData,
        to: &TriangleMeshData,
        base_colors: ([f32; 4], [f32; 4]),
    ) -> Option<Self> {
        let mut a = sample(from)?;
        let mut b = sample(to)?;
        match (&a.colors, &b.colors) {
            (Some(_), None) => b.colors = Some(vec![base_colors.1; POINTS]),
            (None, Some(_)) => a.colors = Some(vec![base_colors.0; POINTS]),
            _ => {}
        }
        if a.sampling != Sampling::Rays || b.sampling != Sampling::Rays {
            let remap = align(&a, &b);
            a = remap.a.apply(&a);
            b = remap.b.apply(&b);
        }
        let indices = grid_indices([&a, &b]);
        let mesh = |grid: Grid, of: &TriangleMeshData| {
            Arc::new(TriangleMeshData {
                vertices: grid.positions.iter().map(|p| p.to_array()).collect(),
                indices: indices.clone(),
                normals: Some(grid.normals.iter().map(|n| n.to_array()).collect()),
                uvs: None,
                color: of.color,
                colors: grid.colors,
                material: of.material,
                surface: None,
                blend: None,
            })
        };
        Some(Self {
            from: mesh(a, from),
            to: mesh(b, to),
        })
    }

    /// The mesh `t` of the way from one mesh to the other; `t` may leave
    /// `[0, 1]`, as an overshooting easing does.
    pub fn at(&self, t: f32) -> TriangleMeshData {
        let blend = Arc::new(MeshBlend {
            from: Arc::clone(&self.from),
            to: Arc::clone(&self.to),
            t,
        });
        MeshBlend::mesh(blend).expect("the two grids share their layout")
    }
}

/// Two triangles per grid quad, `(i, j)`, `(i + 1, j)`, `(i + 1, j + 1)`
/// and `(i, j + 1)`, which face along `du × dv`. Each quad is split along
/// the diagonal whose triangles turn least from the normals the grids
/// sampled at their corners, so a crease that crosses quads aslant runs
/// along their diagonals instead of being cut across.
fn grid_indices(grids: [&Grid; 2]) -> Vec<u32> {
    let deviation = |corners: [usize; 3]| {
        grids
            .iter()
            .map(|grid| {
                let [a, b, c] = corners.map(|corner| grid.positions[corner]);
                let face = (b - a).cross(c - a).normalize_or_zero();
                corners
                    .iter()
                    .map(|&corner| 1.0 - face.dot(grid.normals[corner]))
                    .sum::<f32>()
            })
            .sum::<f32>()
    };
    let mut indices = Vec::with_capacity(COLUMNS * ROWS * 6);
    for j in 0..ROWS {
        for i in 0..COLUMNS {
            let a = index(i, j);
            let b = index(i + 1, j);
            let c = index(i + 1, j + 1);
            let d = index(i, j + 1);
            let along_ac = deviation([a, b, c]) + deviation([a, c, d]);
            let along_bd = deviation([a, b, d]) + deviation([b, c, d]);
            let quad = if along_bd < along_ac {
                [a, b, d, b, c, d]
            } else {
                [a, b, c, a, c, d]
            };
            indices.extend(quad.map(|corner| corner as u32));
        }
    }
    indices
}

fn sample(mesh: &TriangleMeshData) -> Option<Grid> {
    let positions: Vec<Vec3> = mesh.vertices.iter().map(|v| Vec3::from_array(*v)).collect();
    let triangles: Vec<[usize; 3]> = mesh
        .indices
        .as_chunks::<3>()
        .0
        .iter()
        .map(|t| t.map(|index| index as usize))
        .filter(|t| t.iter().all(|&index| index < positions.len()))
        .collect();
    if triangles.is_empty() {
        return None;
    }
    let normals = vertex_normals(mesh, &positions, &triangles);
    let colors = mesh
        .colors
        .as_deref()
        .filter(|colors| colors.len() == positions.len());
    let finish = |hits: Vec<Option<Hit>>, sampling| {
        let hits = fill_gaps(hits)?;
        Some(resolve(
            &hits,
            &triangles,
            Vertices {
                positions: &positions,
                normals: &normals,
                colors,
            },
            sampling,
        ))
    };

    if let Some(hits) = mesh
        .uvs
        .as_ref()
        .filter(|uvs| uvs.len() == positions.len())
        .and_then(|uvs| by_texture_coordinates(uvs, &triangles))
    {
        return finish(hits, Sampling::TextureCoordinates);
    }
    let (hits, met) = by_rays(&positions, &triangles);
    if met as f32 >= RAY_HITS * POINTS as f32 {
        return finish(hits, Sampling::Rays);
    }
    finish(by_sheet(&positions, &triangles), Sampling::Sheet)
}

/// The mesh's normals, or the area-weighted normals of its faces.
fn vertex_normals(
    mesh: &TriangleMeshData,
    positions: &[Vec3],
    triangles: &[[usize; 3]],
) -> Vec<Vec3> {
    if let Some(normals) = mesh
        .normals
        .as_ref()
        .filter(|normals| normals.len() == positions.len())
    {
        return normals
            .iter()
            .map(|n| Vec3::from_array(*n).normalize_or_zero())
            .collect();
    }
    let mut normals = vec![Vec3::ZERO; positions.len()];
    for &[a, b, c] in triangles {
        let face = (positions[b] - positions[a]).cross(positions[c] - positions[a]);
        for vertex in [a, b, c] {
            normals[vertex] += face;
        }
    }
    normals.into_iter().map(Vec3::normalize_or_zero).collect()
}

/// The grid point at `(i, j)`'s parameters: `(i / COLUMNS, j / ROWS)`.
fn parameters(i: usize, j: usize) -> Vec2 {
    Vec2::new(i as f32 / COLUMNS as f32, j as f32 / ROWS as f32)
}

/// Rasterizes the triangles at `coords`, one point per vertex in the unit
/// square, onto the grid. With `depths`, a grid point two triangles cover
/// keeps the higher one; without, the first.
fn rasterize(
    coords: &[Vec2],
    depths: Option<&[f32]>,
    triangles: &[[usize; 3]],
) -> Vec<Option<Hit>> {
    let mut hits = vec![None; POINTS];
    let scale = Vec2::new(COLUMNS as f32, ROWS as f32);
    for (triangle, &[a, b, c]) in triangles.iter().enumerate() {
        let (p0, p1, p2) = (coords[a], coords[b], coords[c]);
        let area = (p1 - p0).perp_dot(p2 - p0);
        if area.abs() < 1e-12 {
            continue;
        }
        let low = (p0.min(p1).min(p2) * scale - 1e-3).ceil().max(Vec2::ZERO);
        let high = (p0.max(p1).max(p2) * scale + 1e-3).floor().min(scale);
        if low.x > high.x || low.y > high.y {
            continue;
        }
        for j in low.y as usize..=high.y as usize {
            for i in low.x as usize..=high.x as usize {
                let q = parameters(i, j);
                let w1 = (q - p0).perp_dot(p2 - p0) / area;
                let w2 = (p1 - p0).perp_dot(q - p0) / area;
                let weights = Vec3::new(1.0 - w1 - w2, w1, w2);
                if weights.min_element() < -1e-4 {
                    continue;
                }
                let depth = depths.map_or(0.0, |depths| {
                    weights.dot(Vec3::new(depths[a], depths[b], depths[c]))
                });
                let slot = &mut hits[index(i, j)];
                if slot.is_none_or(|hit: Hit| depths.is_some() && depth > hit.depth) {
                    *slot = Some(Hit {
                        triangle,
                        weights,
                        depth,
                        across: None,
                    });
                }
            }
        }
    }
    hits
}

/// The grid sampled by texture coordinates, when they cover the unit
/// square once: their triangles add up to its area and leave little of
/// it out. A cube's six faces each cover it, and are sampled by rays.
fn by_texture_coordinates(uvs: &[[f32; 2]], triangles: &[[usize; 3]]) -> Option<Vec<Option<Hit>>> {
    let coords: Vec<Vec2> = uvs.iter().map(|uv| Vec2::from_array(*uv)).collect();
    let area: f32 = triangles
        .iter()
        .map(|&[a, b, c]| {
            (coords[b] - coords[a])
                .perp_dot(coords[c] - coords[a])
                .abs()
                * 0.5
        })
        .sum();
    if !(0.97..=1.03).contains(&area) {
        return None;
    }
    let hits = rasterize(&coords, None, triangles);
    let gaps = hits.iter().filter(|hit| hit.is_none()).count();
    (gaps as f32 <= UV_GAPS * POINTS as f32).then_some(hits)
}

/// The direction of grid point `(i, j)`'s ray: a sphere's normal at those
/// texture coordinates.
fn direction(i: usize, j: usize) -> Vec3 {
    let theta = TAU * i as f32 / COLUMNS as f32;
    let phi = PI * j as f32 / ROWS as f32;
    Vec3::new(phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin())
}

fn bounds(positions: &[Vec3]) -> (Vec3, Vec3) {
    positions.iter().fold(
        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(min, max), point| (min.min(*point), max.max(*point)),
    )
}

/// The grid sampled by rays from the center of the mesh's bounds, at the
/// outermost point each ray meets, and how many rays meet the mesh. Each
/// triangle is tested only by the rays near it: those within the cone
/// around its vertices' directions.
fn by_rays(positions: &[Vec3], triangles: &[[usize; 3]]) -> (Vec<Option<Hit>>, usize) {
    let (min, max) = bounds(positions);
    let center = (min + max) * 0.5;
    let size = (max - min).length().max(f32::MIN_POSITIVE);
    let directions: Vec<Vec3> = (0..=ROWS)
        .flat_map(|j| (0..=COLUMNS).map(move |i| direction(i, j)))
        .collect();
    let mut hits: Vec<Option<Hit>> = vec![None; POINTS];
    let mut test = |triangle: usize, point: usize, corners: &[Vec3; 3]| {
        if let Some((distance, weights)) = ray_triangle(directions[point], corners)
            && hits[point].is_none_or(|hit| distance > hit.depth)
        {
            hits[point] = Some(Hit {
                triangle,
                weights,
                depth: distance,
                across: None,
            });
        }
    };
    let row_step = PI / ROWS as f32;
    let column_step = TAU / COLUMNS as f32;
    for (triangle, &[a, b, c]) in triangles.iter().enumerate() {
        let corners = [positions[a], positions[b], positions[c]].map(|p| p - center);
        let rows = cone(&corners, size).map(|(axis, cos_radius)| {
            let radius = cos_radius.acos() + row_step;
            let polar = axis.y.clamp(-1.0, 1.0).acos();
            let first = ((polar - radius) / row_step).floor().max(0.0) as usize;
            let last = ((polar + radius) / row_step).ceil().min(ROWS as f32) as usize;
            // A cone around a pole spans every longitude.
            let span = if polar - radius <= 0.0 || polar + radius >= PI {
                None
            } else {
                let half = (radius.sin() / polar.sin()).min(1.0).asin() + column_step;
                (half < PI).then(|| (axis.z.atan2(axis.x), half))
            };
            (first, last, span)
        });
        match rows {
            Some((first, last, Some((longitude, half)))) => {
                let start = ((longitude - half) / column_step).floor() as isize;
                let end = ((longitude + half) / column_step).ceil() as isize;
                for j in first..=last {
                    for step in start..=end {
                        let i = step.rem_euclid(COLUMNS as isize) as usize;
                        test(triangle, index(i, j), &corners);
                        if i == 0 {
                            test(triangle, index(COLUMNS, j), &corners);
                        }
                    }
                }
            }
            Some((first, last, None)) => {
                for j in first..=last {
                    for i in 0..=COLUMNS {
                        test(triangle, index(i, j), &corners);
                    }
                }
            }
            None => {
                for point in 0..POINTS {
                    test(triangle, point, &corners);
                }
            }
        }
    }
    snap_to_creases(&mut hits, positions, triangles, center);
    let met = hits.iter().filter(|hit| hit.is_some()).count();
    (hits, met)
}

/// Two faces meet in a crease when their normals part by more than this
/// cosine's angle (35°), as a tube's corners do.
const CREASE_COSINE: f32 = 0.819;

/// The direction of the ray at grid parameters `(i, j)`, which may lie
/// between grid points.
fn direction_at(i: f32, j: f32) -> Vec3 {
    let theta = TAU * i / COLUMNS as f32;
    let phi = PI * j / ROWS as f32;
    Vec3::new(phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin())
}

/// Moves each grid point beside a crease onto it. Rays meet a cube's edge
/// between two grid points, so its quads would cut the edge in steps;
/// where two neighbors along a row or column meet faces that part in a
/// crease, the ray between them where the faces change is found by
/// bisection, and the nearer of the two points moves to it. A crease then
/// runs through grid points and stays straight.
fn snap_to_creases(
    hits: &mut [Option<Hit>],
    positions: &[Vec3],
    triangles: &[[usize; 3]],
    center: Vec3,
) {
    let face = |triangle: usize| {
        let [a, b, c] = triangles[triangle];
        (positions[b] - positions[a])
            .cross(positions[c] - positions[a])
            .normalize_or_zero()
    };
    let original = hits.to_vec();
    // The triangles met around grid point `(i, j)`, where the ray between
    // it and a neighbor meets the mesh.
    let around = |i: usize, j: usize, triangles: &mut Vec<usize>| {
        for nj in j.saturating_sub(1)..=(j + 1).min(ROWS) {
            for ni in i.saturating_sub(1)..=(i + 1).min(COLUMNS) {
                if let Some(hit) = original[index(ni, nj)]
                    && !triangles.contains(&hit.triangle)
                {
                    triangles.push(hit.triangle);
                }
            }
        }
    };
    let mut moved = vec![false; POINTS];
    let mut candidates = Vec::new();
    for (di, dj) in [(1, 0), (0, 1)] {
        for j in 0..=ROWS - dj {
            for i in 0..=COLUMNS - di {
                let (a, b) = (index(i, j), index(i + di, j + dj));
                let (Some(first), Some(second)) = (original[a], original[b]) else {
                    continue;
                };
                let side = face(first.triangle);
                if side.dot(face(second.triangle)) > CREASE_COSINE {
                    continue;
                }
                candidates.clear();
                around(i, j, &mut candidates);
                around(i + di, j + dj, &mut candidates);
                let cast = |s: f32| {
                    let direction =
                        direction_at(i as f32 + di as f32 * s, j as f32 + dj as f32 * s);
                    candidates
                        .iter()
                        .filter_map(|&triangle| {
                            let [p, q, r] = triangles[triangle];
                            let corners =
                                [positions[p], positions[q], positions[r]].map(|v| v - center);
                            ray_triangle(direction, &corners).map(|(depth, weights)| Hit {
                                triangle,
                                weights,
                                depth,
                                across: None,
                            })
                        })
                        .max_by(|x, y| x.depth.total_cmp(&y.depth))
                };
                let (mut low, mut high) = ((0.0, first), (1.0, second));
                for _ in 0..12 {
                    let middle = (low.0 + high.0) * 0.5;
                    let Some(hit) = cast(middle) else {
                        break;
                    };
                    if face(hit.triangle).dot(side) > CREASE_COSINE {
                        low = (middle, hit);
                    } else {
                        high = (middle, hit);
                    }
                }
                // The point lies on the crease, and shades halfway between
                // its two sides, so the crease shows as an even bevel.
                let (point, hit) = if low.0 + high.0 < 1.0 {
                    (
                        a,
                        Hit {
                            across: Some((high.1.triangle, high.1.weights)),
                            ..low.1
                        },
                    )
                } else {
                    (
                        b,
                        Hit {
                            across: Some((low.1.triangle, low.1.weights)),
                            ..high.1
                        },
                    )
                };
                if !moved[point] {
                    moved[point] = true;
                    hits[point] = Some(hit);
                }
            }
        }
    }
    // The last column is the first one again.
    for j in 0..=ROWS {
        hits[index(COLUMNS, j)] = hits[index(0, j)];
    }
}

/// The cone of directions from the center that holds a triangle: its
/// axis and the cosine of its half angle. `None` when the cone is wider
/// than a right angle, or a corner sits at the center, and every ray
/// tests it.
fn cone(corners: &[Vec3; 3], size: f32) -> Option<(Vec3, f32)> {
    if corners.iter().any(|corner| corner.length() < 1e-6 * size) {
        return None;
    }
    let directions = corners.map(Vec3::normalize);
    let axis = (directions[0] + directions[1] + directions[2]).try_normalize()?;
    let cos_radius = directions
        .iter()
        .map(|direction| axis.dot(*direction))
        .fold(1.0_f32, f32::min);
    (cos_radius > 0.2).then_some((axis, cos_radius))
}

/// Where the ray from the origin along `direction` meets the triangle
/// `corners`: its distance and the barycentric weights of the point.
fn ray_triangle(direction: Vec3, corners: &[Vec3; 3]) -> Option<(f32, Vec3)> {
    let [a, b, c] = *corners;
    let (e1, e2) = (b - a, c - a);
    let p = direction.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inverse = 1.0 / det;
    let origin = -a;
    let u = origin.dot(p) * inverse;
    let q = origin.cross(e1);
    let v = direction.dot(q) * inverse;
    const EDGE: f32 = 1e-4;
    if u < -EDGE || v < -EDGE || u + v > 1.0 + EDGE {
        return None;
    }
    let distance = e2.dot(q) * inverse;
    (distance > 0.0).then_some((distance, Vec3::new(1.0 - u - v, u, v)))
}

/// The grid sampled across a sheet's two widest sides, at its highest
/// point along the thinnest where it folds over itself.
fn by_sheet(positions: &[Vec3], triangles: &[[usize; 3]]) -> Vec<Option<Hit>> {
    let (min, max) = bounds(positions);
    let extent = max - min;
    let thinnest = (0..3)
        .min_by(|&a, &b| extent[a].total_cmp(&extent[b]))
        .unwrap_or(2);
    let (across, along) = ((thinnest + 1) % 3, (thinnest + 2) % 3);
    let span = |axis: usize| extent[axis].max(f32::MIN_POSITIVE);
    let coords: Vec<Vec2> = positions
        .iter()
        .map(|p| {
            Vec2::new(
                (p[across] - min[across]) / span(across),
                (p[along] - min[along]) / span(along),
            )
        })
        .collect();
    let depths: Vec<f32> = positions.iter().map(|p| p[thinnest]).collect();
    rasterize(&coords, Some(&depths), triangles)
}

/// Every grid point no triangle met takes the nearest one's sample, so a
/// point past a sheet's edge or a mesh's gap lies on that edge. `None`
/// when no point met the mesh.
fn fill_gaps(mut hits: Vec<Option<Hit>>) -> Option<Vec<Hit>> {
    let mut queue: VecDeque<usize> = (0..POINTS).filter(|&point| hits[point].is_some()).collect();
    if queue.is_empty() {
        return None;
    }
    while let Some(point) = queue.pop_front() {
        let (i, j) = (point % STRIDE, point / STRIDE);
        let neighbors = [
            (i > 0).then(|| index(i - 1, j)),
            (i < COLUMNS).then(|| index(i + 1, j)),
            (j > 0).then(|| index(i, j - 1)),
            (j < ROWS).then(|| index(i, j + 1)),
        ];
        for neighbor in neighbors.into_iter().flatten() {
            if hits[neighbor].is_none() {
                hits[neighbor] = hits[point];
                queue.push_back(neighbor);
            }
        }
    }
    hits.into_iter().collect()
}

/// What a mesh's vertices hold that its grid samples.
struct Vertices<'a> {
    positions: &'a [Vec3],
    normals: &'a [Vec3],
    colors: Option<&'a [[f32; 4]]>,
}

fn resolve(
    hits: &[Hit],
    triangles: &[[usize; 3]],
    Vertices {
        positions,
        normals,
        colors,
    }: Vertices<'_>,
    sampling: Sampling,
) -> Grid {
    let blend = |hit: &Hit, values: &dyn Fn(usize) -> Vec3| {
        let [a, b, c] = triangles[hit.triangle];
        values(a) * hit.weights.x + values(b) * hit.weights.y + values(c) * hit.weights.z
    };
    let positions: Vec<Vec3> = hits
        .iter()
        .map(|hit| blend(hit, &|v| positions[v]))
        .collect();
    let normals = hits
        .iter()
        .map(|hit| {
            let normal = blend(hit, &|v| normals[v]);
            let across = hit.across.map_or(Vec3::ZERO, |(triangle, weights)| {
                let across = Hit {
                    triangle,
                    weights,
                    ..*hit
                };
                blend(&across, &|v| normals[v])
            });
            (normal.normalize_or_zero() + across.normalize_or_zero()).normalize_or_zero()
        })
        .collect();
    let colors = colors.map(|colors| {
        hits.iter()
            .map(|hit| {
                let [a, b, c] = triangles[hit.triangle];
                std::array::from_fn(|k| {
                    colors[a][k] * hit.weights.x
                        + colors[b][k] * hit.weights.y
                        + colors[c][k] * hit.weights.z
                })
            })
            .collect()
    });
    let mut grid = Grid {
        wraps: wraps(&positions),
        positions,
        normals,
        colors,
        sampling,
    };
    orient(&mut grid);
    grid
}

/// Whether the grid's first and last columns, and its first and last rows,
/// are the same points.
fn wraps(positions: &[Vec3]) -> (bool, bool) {
    let (min, max) = bounds(positions);
    let tolerance = 1e-4 * (max - min).length().max(f32::MIN_POSITIVE);
    let columns = (0..=ROWS)
        .all(|j| positions[index(0, j)].distance(positions[index(COLUMNS, j)]) <= tolerance);
    let rows = (0..=COLUMNS)
        .all(|i| positions[index(i, 0)].distance(positions[index(i, ROWS)]) <= tolerance);
    (columns, rows)
}

/// Reverses the grid's columns if its quads face against the mesh's
/// normals, so every grid's triangles face out.
fn orient(grid: &mut Grid) {
    let mut facing = 0.0;
    for j in 0..ROWS {
        for i in 0..COLUMNS {
            let origin = grid.positions[index(i, j)];
            let across = grid.positions[index(i + 1, j)] - origin;
            let along = grid.positions[index(i, j + 1)] - origin;
            facing += across.cross(along).dot(grid.normals[index(i, j)]);
        }
    }
    if facing < 0.0 {
        *grid = Remap {
            mirror: true,
            ..Remap::default()
        }
        .apply(grid);
    }
}

/// A reparameterization of a grid: reversing its columns (`mirror`),
/// turning it half a turn (`flip`, which keeps the way it faces), and
/// shifting it along the sides it wraps around.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Remap {
    mirror: bool,
    flip: bool,
    shift: (usize, usize),
}

impl Remap {
    /// The point of the grid that point `(i, j)` reads.
    fn source(&self, i: usize, j: usize, wraps: (bool, bool)) -> usize {
        let (mut i, mut j) = (i, j);
        if self.mirror != self.flip {
            i = COLUMNS - i;
        }
        if self.flip {
            j = ROWS - j;
        }
        if wraps.0 {
            i = (i + self.shift.0) % COLUMNS;
        }
        if wraps.1 {
            j = (j + self.shift.1) % ROWS;
        }
        index(i, j)
    }

    fn apply(&self, grid: &Grid) -> Grid {
        let sources: Vec<usize> = (0..=ROWS)
            .flat_map(|j| (0..=COLUMNS).map(move |i| (i, j)))
            .map(|(i, j)| self.source(i, j, grid.wraps))
            .collect();
        Grid {
            positions: sources.iter().map(|&s| grid.positions[s]).collect(),
            normals: sources.iter().map(|&s| grid.normals[s]).collect(),
            colors: grid
                .colors
                .as_ref()
                .map(|colors| sources.iter().map(|&s| colors[s]).collect()),
            sampling: grid.sampling,
            wraps: grid.wraps,
        }
    }
}

/// Reparameterizations of the two grids that pair their points.
struct Alignment {
    a: Remap,
    b: Remap,
}

/// The reparameterization that moves the points of `a` least on their way
/// to those of `b`: `b` turned half a turn or not, and shifted along each
/// side either grid wraps around (the one that wraps; `b` if both do).
/// Shifts are first tried every [`COARSE`] steps on every [`COARSE`]th
/// point, then one by one around the best on every point.
fn align(a: &Grid, b: &Grid) -> Alignment {
    let shifted_u = if b.wraps.0 {
        Some(true)
    } else {
        a.wraps.0.then_some(false)
    };
    let shifted_v = if b.wraps.1 {
        Some(true)
    } else {
        a.wraps.1.then_some(false)
    };
    let alignment = |flip: bool, (du, dv): (usize, usize)| {
        let on = |shifted: Option<bool>, of_b: bool, by: usize| match shifted {
            Some(shifted) if shifted == of_b => by,
            _ => 0,
        };
        Alignment {
            a: Remap {
                mirror: false,
                flip: false,
                shift: (on(shifted_u, false, du), on(shifted_v, false, dv)),
            },
            b: Remap {
                mirror: false,
                flip,
                shift: (on(shifted_u, true, du), on(shifted_v, true, dv)),
            },
        }
    };
    let cost = |candidate: &Alignment, step: usize| {
        let mut total = 0.0_f64;
        for j in (0..=ROWS).step_by(step) {
            for i in (0..=COLUMNS).step_by(step) {
                let from = a.positions[candidate.a.source(i, j, a.wraps)];
                let to = b.positions[candidate.b.source(i, j, b.wraps)];
                total += f64::from(from.distance_squared(to));
            }
        }
        total
    };
    let shifts = |wraps: Option<bool>, steps: usize, stride: usize| -> Vec<usize> {
        match wraps {
            Some(_) => (0..steps).step_by(stride).collect(),
            None => vec![0],
        }
    };

    let mut best = (false, (0, 0), f64::INFINITY);
    for flip in [false, true] {
        for &du in &shifts(shifted_u, COLUMNS, COARSE) {
            for &dv in &shifts(shifted_v, ROWS, COARSE) {
                let total = cost(&alignment(flip, (du, dv)), COARSE);
                if total < best.2 {
                    best = (flip, (du, dv), total);
                }
            }
        }
    }
    let (flip, (du, dv), _) = best;
    let around = |wraps: Option<bool>, at: usize, steps: usize| -> Vec<usize> {
        match wraps {
            Some(_) => (0..=2 * COARSE)
                .map(|offset| (at + steps + offset - COARSE) % steps)
                .collect(),
            None => vec![0],
        }
    };
    let mut refined = (alignment(flip, (du, dv)), f64::INFINITY);
    for &du in &around(shifted_u, du, COLUMNS) {
        for &dv in &around(shifted_v, dv, ROWS) {
            let candidate = alignment(flip, (du, dv));
            let total = cost(&candidate, 1);
            if total < refined.1 {
                refined = (candidate, total);
            }
        }
    }
    refined.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives3d;
    use gaanim_scene::Material3D;

    const WHITE: [f32; 4] = [1.0; 4];

    fn volume(frame: &TriangleMeshData) -> f32 {
        frame
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|v| Vec3::from_array(frame.vertices[v as usize]));
                a.dot(b.cross(c))
            })
            .sum::<f32>()
            / 6.0
    }

    #[test]
    fn a_sphere_and_a_cube_meet_point_for_point_along_rays() {
        let sphere = primitives3d::sphere(1.0, 32, 16, Material3D::default()).unwrap();
        let cube = primitives3d::cube(2.0, Material3D::default()).unwrap();
        let morph = MeshMorph::new(&sphere, &cube, (WHITE, WHITE)).unwrap();
        assert_eq!(
            sample(&sphere).unwrap().sampling,
            Sampling::TextureCoordinates
        );
        assert_eq!(sample(&cube).unwrap().sampling, Sampling::Rays);
        let (start, end) = (morph.at(0.0), morph.at(1.0));
        // Each sphere point lies on the sphere and moves out along its own
        // direction onto the cube's surface.
        for (a, b) in start.vertices.iter().zip(&end.vertices) {
            let (a, b) = (Vec3::from_array(*a), Vec3::from_array(*b));
            assert!((a.length() - 1.0).abs() < 0.02, "{a}");
            assert!((b.abs().max_element() - 1.0).abs() < 1e-3, "{b}");
            assert!(a.normalize().dot(b.normalize()) > 0.999, "{a} {b}");
        }
        // The grid's triangles face out at both ends and halfway.
        let sphere_volume = 4.0 / 3.0 * PI;
        assert!((volume(&start) - sphere_volume).abs() < 0.05 * sphere_volume);
        assert!((volume(&end) - 8.0).abs() < 0.1);
        assert!(volume(&morph.at(0.5)) > sphere_volume);
        let points: Vec<Vec3> = end.vertices.iter().map(|v| Vec3::from_array(*v)).collect();
        let (min, max) = bounds(&points);
        assert!((min + 1.0).abs().max_element() < 1e-3);
        assert!((max - 1.0).abs().max_element() < 1e-3);
        // Each frame keeps the two grids it blends, for a recording.
        let blend = morph.at(0.25).blend.unwrap();
        assert_eq!((blend.t, blend.from.vertices.len()), (0.25, POINTS));
    }

    #[test]
    fn a_cube_sampled_by_rays_keeps_grid_points_along_its_edges() {
        let cube = primitives3d::cube(2.0, Material3D::default()).unwrap();
        let grid = sample(&cube).unwrap();
        assert_eq!(grid.sampling, Sampling::Rays);
        let on = |value: f32| (value.abs() - 1.0).abs() < 1e-3;
        // Every edge of the top face, which rows of rays cross aslant,
        // runs through grid points from end to end.
        for along_x in [true, false] {
            for sign in [-1.0, 1.0] {
                let mut spots: Vec<f32> = grid
                    .positions
                    .iter()
                    .filter(|p| {
                        let side = if along_x { p.z } else { p.x };
                        (p.y - 1.0).abs() < 1e-3 && on(side) && side * sign > 0.0
                    })
                    .map(|p| if along_x { p.x } else { p.z })
                    .collect();
                spots.sort_by(f32::total_cmp);
                let widest = spots.windows(2).map(|w| w[1] - w[0]).fold(0.0, f32::max);
                assert!(spots.len() > 8 && widest < 0.15, "{spots:?}");
            }
        }
    }

    #[test]
    fn a_cube_grid_cuts_into_the_cube_only_at_its_corners() {
        let cube = primitives3d::cube(2.0, Material3D::default()).unwrap();
        let grid = sample(&cube).unwrap();
        let indices = grid_indices([&grid, &grid]);
        // How far inside the cube each grid triangle's center lies.
        let depths: Vec<f32> = indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|v| grid.positions[v as usize]);
                1.0 - ((a + b + c) / 3.0).abs().max_element()
            })
            .collect();
        let cut = depths.iter().filter(|depth| **depth > 0.005).count();
        let deepest = depths.iter().copied().fold(0.0, f32::max);
        assert!(
            cut <= 8 && deepest < 0.01,
            "{cut} triangles, {deepest} deep"
        );
    }

    #[test]
    fn a_sphere_opens_its_poles_into_a_torus_hole() {
        let sphere = primitives3d::sphere(1.0, 32, 16, Material3D::default()).unwrap();
        let torus = primitives3d::torus(1.0, 0.4, 32, 16, Material3D::default()).unwrap();
        let morph = MeshMorph::new(&sphere, &torus, (WHITE, WHITE)).unwrap();
        assert_eq!(sample(&torus).unwrap().wraps, (true, true));
        let end = morph.at(1.0);
        let point = |i, j| Vec3::from_array(end.vertices[index(i, j)]);
        // The poles land on the inner circle, the equator on the outer one.
        let radius = |p: Vec3| Vec2::new(p.x, p.z).length();
        for i in [0, COLUMNS / 3, COLUMNS] {
            assert!((radius(point(i, 0)) - 0.6).abs() < 0.02);
            assert!((radius(point(i, ROWS)) - 0.6).abs() < 0.02);
            assert!((radius(point(i, ROWS / 2)) - 1.4).abs() < 0.02);
        }
        // North stays up: the north pole opens over the top of the tube.
        assert!(point(0, ROWS / 4).y > 0.3);
        assert!(volume(&end) > 0.0);
    }

    #[test]
    fn two_sheets_meet_across_their_widest_sides() {
        let mesh = |height: f32| {
            let n = 8;
            let mut vertices = Vec::new();
            for z in 0..=n {
                for x in 0..=n {
                    let (u, v) = (x as f32 / n as f32, z as f32 / n as f32);
                    vertices.push([u * 4.0 - 2.0, height * u * v, v * 2.0 - 1.0]);
                }
            }
            let mut indices = Vec::new();
            for z in 0..n {
                for x in 0..n {
                    let a = z * (n + 1) + x;
                    indices.extend_from_slice(&[a, a + n + 1, a + 1, a + 1, a + n + 1, a + n + 2]);
                }
            }
            TriangleMeshData {
                vertices,
                indices,
                normals: None,
                uvs: None,
                color: None,
                colors: None,
                material: None,
                surface: None,
                blend: None,
            }
        };
        let morph = MeshMorph::new(&mesh(0.5), &mesh(1.0), (WHITE, WHITE)).unwrap();
        assert_eq!(sample(&mesh(0.5)).unwrap().sampling, Sampling::Sheet);
        // Each point keeps its place across the sheet and only rises.
        let (start, end) = (morph.at(0.0), morph.at(1.0));
        for (a, b) in start.vertices.iter().zip(&end.vertices) {
            assert!((a[0] - b[0]).abs() < 1e-4 && (a[2] - b[2]).abs() < 1e-4);
            assert!(b[1] >= a[1] - 1e-4);
        }
    }

    #[test]
    fn a_mesh_with_vertex_colors_blends_into_one_with_its_base_color() {
        let mut colored = primitives3d::sphere(1.0, 32, 16, Material3D::default()).unwrap();
        colored.colors = Some(vec![[1.0, 0.0, 0.0, 1.0]; colored.vertices.len()]);
        let plain = primitives3d::sphere(2.0, 32, 16, Material3D::default()).unwrap();
        let blue = [0.0, 0.0, 1.0, 1.0];
        let morph = MeshMorph::new(&colored, &plain, (WHITE, blue)).unwrap();
        let halfway = morph.at(0.5);
        let colors = halfway.colors.unwrap();
        assert!(
            colors
                .iter()
                .all(|c| (c[0] - 0.5).abs() < 1e-4 && (c[2] - 0.5).abs() < 1e-4)
        );
        assert!(
            halfway
                .vertices
                .iter()
                .all(|v| (Vec3::from_array(*v).length() - 1.5).abs() < 0.03)
        );
    }

    #[test]
    fn a_mesh_without_triangles_has_no_morph() {
        let mut empty = primitives3d::cube(1.0, Material3D::default()).unwrap();
        empty.indices.clear();
        let cube = primitives3d::cube(1.0, Material3D::default()).unwrap();
        assert!(MeshMorph::new(&empty, &cube, (WHITE, WHITE)).is_none());
    }
}
