//! Synchronous glTF metadata for the deferred public API.
//!
//! Bevy's native loader owns rendering. This module validates the source and
//! extracts stable scene, node, bounds, and animation metadata before startup.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use gaanim_core::glam::{DMat4, DQuat, DVec3};
use gaanim_math::{Bounds3D, SpatialTransform};

type GltfCache = HashMap<(PathBuf, GltfSceneSelector), (std::time::SystemTime, GltfDocument)>;
static GLTF_CACHE: OnceLock<Mutex<GltfCache>> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum GltfSceneSelector {
    #[default]
    Default,
    Index(usize),
    Name(String),
}

#[derive(Debug, Clone)]
pub struct GltfNodeMetadata {
    pub index: usize,
    pub name: String,
    pub path: String,
    pub parent: Option<usize>,
    pub transform: SpatialTransform,
    pub bounds: Bounds3D,
    pub has_geometry: bool,
}

#[derive(Debug, Clone)]
pub struct GltfAnimationMetadata {
    pub index: usize,
    pub name: String,
    pub duration: f64,
}

#[derive(Debug, Clone)]
pub struct GltfDocument {
    pub path: PathBuf,
    pub scene_index: usize,
    pub scene_name: Option<String>,
    pub nodes: Vec<GltfNodeMetadata>,
    pub animations: Vec<GltfAnimationMetadata>,
    pub bounds: Bounds3D,
}

#[derive(Debug, thiserror::Error)]
pub enum GltfLoadError {
    #[error("glTF asset '{path}' does not exist")]
    Missing { path: PathBuf },
    #[error("unsupported 3D asset extension for '{path}'; expected .gltf or .glb")]
    UnsupportedExtension { path: PathBuf },
    #[error("could not import glTF asset '{path}': {source}")]
    Import {
        path: PathBuf,
        #[source]
        source: gltf::Error,
    },
    #[error("glTF asset '{path}' has no scenes")]
    NoScenes { path: PathBuf },
    #[error("glTF scene index {index} does not exist in '{path}'")]
    UnknownSceneIndex { path: PathBuf, index: usize },
    #[error("glTF scene '{name}' does not exist in '{path}'; available scenes: {available}")]
    UnknownSceneName {
        path: PathBuf,
        name: String,
        available: String,
    },
}

impl GltfDocument {
    pub fn load(
        path: impl AsRef<Path>,
        selector: &GltfSceneSelector,
    ) -> Result<Self, GltfLoadError> {
        let path = path.as_ref().to_path_buf();
        if !path.is_file() {
            return Err(GltfLoadError::Missing { path });
        }
        let supported = path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "gltf" | "glb"));
        if !supported {
            return Err(GltfLoadError::UnsupportedExtension { path });
        }

        let path = path
            .canonicalize()
            .map_err(|_| GltfLoadError::Missing { path })?;
        let modified = std::fs::metadata(&path)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let cache = GLTF_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        if let Some((cached_modified, document)) = cache
            .lock()
            .expect("glTF metadata cache poisoned")
            .get(&(path.clone(), selector.clone()))
            && *cached_modified == modified
        {
            return Ok(document.clone());
        }

        let (document, buffers, _images) =
            gltf::import(&path).map_err(|source| GltfLoadError::Import {
                path: path.clone(),
                source,
            })?;
        let scenes = document.scenes().collect::<Vec<_>>();
        if scenes.is_empty() {
            return Err(GltfLoadError::NoScenes { path });
        }
        let scene_index = match selector {
            GltfSceneSelector::Default => document
                .default_scene()
                .map(|scene| scene.index())
                .unwrap_or(0),
            GltfSceneSelector::Index(index) if *index < scenes.len() => *index,
            GltfSceneSelector::Index(index) => {
                return Err(GltfLoadError::UnknownSceneIndex {
                    path,
                    index: *index,
                });
            }
            GltfSceneSelector::Name(name) => scenes
                .iter()
                .find(|scene| scene.name() == Some(name.as_str()))
                .map(|scene| scene.index())
                .ok_or_else(|| GltfLoadError::UnknownSceneName {
                    path: path.clone(),
                    name: name.clone(),
                    available: scenes
                        .iter()
                        .map(|scene| scene.name().unwrap_or("<unnamed>"))
                        .collect::<Vec<_>>()
                        .join(", "),
                })?,
        };
        let scene = scenes
            .into_iter()
            .find(|scene| scene.index() == scene_index)
            .expect("selected glTF scene index must exist");

        let mut nodes = Vec::new();
        for node in scene.nodes() {
            collect_node(node, None, "", &mut nodes);
        }
        let mut path_counts = HashMap::<String, usize>::new();
        for node in &nodes {
            *path_counts.entry(node.path.clone()).or_default() += 1;
        }
        for node in &mut nodes {
            if path_counts.get(&node.path).copied().unwrap_or(0) > 1 {
                node.path = format!("{}#{}", node.path, node.index);
            }
        }

        for index in (0..nodes.len()).rev() {
            let node_index = nodes[index].index;
            let mut bounds = nodes[index].bounds;
            let mut has_bounds = nodes[index].has_geometry;
            let children = nodes
                .iter()
                .filter(|node| node.parent == Some(node_index))
                .filter(|node| node.has_geometry)
                .map(|node| transform_bounds(node.bounds, node.transform.to_mat4()))
                .collect::<Vec<_>>();
            for child in children {
                bounds = if has_bounds {
                    union(bounds, child)
                } else {
                    child
                };
                has_bounds = true;
            }
            nodes[index].bounds = bounds;
            nodes[index].has_geometry = has_bounds;
        }

        let mut bounds = Bounds3D::default();
        let mut has_bounds = false;
        for node in nodes
            .iter()
            .filter(|node| node.parent.is_none() && node.has_geometry)
        {
            let current = transform_bounds(node.bounds, node.transform.to_mat4());
            bounds = if has_bounds {
                union(bounds, current)
            } else {
                current
            };
            has_bounds = true;
        }

        let animations = document
            .animations()
            .map(|animation| {
                let mut duration = 0.0_f64;
                for channel in animation.channels() {
                    let reader = channel.reader(|buffer| Some(&buffers[buffer.index()]));
                    if let Some(inputs) = reader.read_inputs() {
                        for time in inputs {
                            duration = duration.max(f64::from(time));
                        }
                    }
                }
                GltfAnimationMetadata {
                    index: animation.index(),
                    name: animation
                        .name()
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("Animation{}", animation.index())),
                    duration,
                }
            })
            .collect();

        let result = Self {
            path,
            scene_index,
            scene_name: scene.name().map(str::to_owned),
            nodes,
            animations,
            bounds: if has_bounds {
                bounds
            } else {
                Bounds3D::default()
            },
        };
        cache.lock().expect("glTF metadata cache poisoned").insert(
            (result.path.clone(), selector.clone()),
            (modified, result.clone()),
        );
        Ok(result)
    }
}

/// Clear process-local glTF metadata so hot reload observes changed files.
pub fn clear_gltf_cache() {
    if let Some(cache) = GLTF_CACHE.get() {
        cache.lock().expect("glTF metadata cache poisoned").clear();
    }
}

fn collect_node(
    node: gltf::Node<'_>,
    parent: Option<usize>,
    parent_path: &str,
    output: &mut Vec<GltfNodeMetadata>,
) {
    let index = node.index();
    let name = node
        .name()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("GltfNode{index}"));
    let path = if parent_path.is_empty() {
        name.clone()
    } else {
        format!("{parent_path}/{name}")
    };
    let (translation, rotation, scale) = node.transform().decomposed();
    let transform = SpatialTransform {
        translation: DVec3::new(
            translation[0].into(),
            translation[1].into(),
            translation[2].into(),
        ),
        rotation: DQuat::from_xyzw(
            rotation[0].into(),
            rotation[1].into(),
            rotation[2].into(),
            rotation[3].into(),
        ),
        scale: DVec3::new(scale[0].into(), scale[1].into(), scale[2].into()),
        anchor: DVec3::ZERO,
    };
    let mut bounds = Bounds3D::default();
    let mut has_bounds = false;
    if let Some(mesh) = node.mesh() {
        for primitive in mesh.primitives() {
            let aabb = primitive.bounding_box();
            let current = Bounds3D::new(
                DVec3::new(aabb.min[0].into(), aabb.min[1].into(), aabb.min[2].into()),
                DVec3::new(aabb.max[0].into(), aabb.max[1].into(), aabb.max[2].into()),
            );
            bounds = if has_bounds {
                union(bounds, current)
            } else {
                current
            };
            has_bounds = true;
        }
    }
    output.push(GltfNodeMetadata {
        index,
        name,
        path: path.clone(),
        parent,
        transform,
        bounds: if has_bounds {
            bounds
        } else {
            Bounds3D::default()
        },
        has_geometry: has_bounds,
    });
    for child in node.children() {
        collect_node(child, Some(index), &path, output);
    }
}

fn union(a: Bounds3D, b: Bounds3D) -> Bounds3D {
    Bounds3D::new(a.min.min(b.min), a.max.max(b.max))
}

fn transform_bounds(bounds: Bounds3D, transform: DMat4) -> Bounds3D {
    let mut min = DVec3::splat(f64::INFINITY);
    let mut max = DVec3::splat(f64::NEG_INFINITY);
    for x in [bounds.min.x, bounds.max.x] {
        for y in [bounds.min.y, bounds.max.y] {
            for z in [bounds.min.z, bounds.max.z] {
                let point = transform.transform_point3(DVec3::new(x, y, z));
                min = min.min(point);
                max = max.max(point);
            }
        }
    }
    if min.x.is_finite() {
        Bounds3D::new(min, max)
    } else {
        Bounds3D::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "gaanim-gltf-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    const SCENES_JSON: &str = r#"{
        "asset":{"version":"2.0"},
        "scene":0,
        "scenes":[
            {"name":"Main","nodes":[0,1]},
            {"name":"Alt","nodes":[2]}
        ],
        "nodes":[
            {"name":"Robot","children":[3]},
            {"name":"Robot","children":[4]},
            {"name":"Solo"},
            {"name":"Arm"},
            {"name":"Arm"}
        ]
    }"#;

    #[test]
    fn rejects_non_gltf_extensions_before_import() {
        let path = std::env::temp_dir().join("gaanim-gltf-test.obj");
        std::fs::write(&path, b"not a model").unwrap();
        let result = GltfDocument::load(&path, &GltfSceneSelector::Default);
        std::fs::remove_file(path).ok();
        assert!(matches!(
            result,
            Err(GltfLoadError::UnsupportedExtension { .. })
        ));
    }

    #[test]
    fn selects_scenes_and_suffixes_duplicate_paths_stably() {
        let dir = fixture_dir("scenes");
        let path = dir.join("model.gltf");
        std::fs::write(&path, SCENES_JSON).unwrap();

        let main = GltfDocument::load(&path, &GltfSceneSelector::Default).unwrap();
        assert_eq!(main.scene_name.as_deref(), Some("Main"));
        assert!(main.nodes.iter().any(|node| node.path == "Robot/Arm#3"));
        assert!(main.nodes.iter().any(|node| node.path == "Robot/Arm#4"));

        let alt = GltfDocument::load(&path, &GltfSceneSelector::Name("Alt".into())).unwrap();
        assert_eq!(alt.nodes.len(), 1);
        assert_eq!(alt.nodes[0].path, "Solo");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn loads_glb_container_metadata() {
        let dir = fixture_dir("glb");
        let path = dir.join("model.glb");
        let mut json = SCENES_JSON.as_bytes().to_vec();
        while !json.len().is_multiple_of(4) {
            json.push(b' ');
        }
        let total_length = 12 + 8 + json.len();
        let mut glb = Vec::with_capacity(total_length);
        glb.extend_from_slice(&0x4654_6C67_u32.to_le_bytes());
        glb.extend_from_slice(&2_u32.to_le_bytes());
        glb.extend_from_slice(&(total_length as u32).to_le_bytes());
        glb.extend_from_slice(&(json.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x4E4F_534A_u32.to_le_bytes());
        glb.extend_from_slice(&json);
        std::fs::write(&path, glb).unwrap();

        let document = GltfDocument::load(&path, &GltfSceneSelector::Index(1)).unwrap();
        assert_eq!(document.scene_name.as_deref(), Some("Alt"));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn reports_missing_external_resources() {
        let dir = fixture_dir("external");
        let path = dir.join("model.gltf");
        std::fs::write(
            &path,
            r#"{"asset":{"version":"2.0"},"buffers":[{"uri":"missing.bin","byteLength":4}],"scenes":[{"nodes":[]}],"nodes":[]}"#,
        )
        .unwrap();
        assert!(matches!(
            GltfDocument::load(&path, &GltfSceneSelector::Default),
            Err(GltfLoadError::Import { .. })
        ));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn acceptance_fixture_exposes_nodes_and_actions() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/assets/gltf_animation_fixture.gltf");
        let document =
            GltfDocument::load(path, &GltfSceneSelector::Name("Presentation".to_owned())).unwrap();
        assert_eq!(
            document
                .animations
                .iter()
                .map(|animation| animation.name.as_str())
                .collect::<Vec<_>>(),
            ["Walk", "Wave"]
        );
        assert!(
            document
                .animations
                .iter()
                .all(|animation| animation.duration == 1.0)
        );
        assert!(
            document
                .nodes
                .iter()
                .any(|node| node.path == "Robot/Rig/Arm")
        );
    }
}
