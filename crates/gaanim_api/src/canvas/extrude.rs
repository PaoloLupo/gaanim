//! `scene.geometry.extrude` (CA-06): a 3D mesh extruded from the outline of
//! a 2D drawable, built when the scene compiles, once the source's geometry
//! (shaped text, imported SVG) is known.

use std::sync::Arc;

use gaanim_core::ObjectId;
use gaanim_scene::{Material3D, TriangleMeshData};

use super::{DrawableHandle, SceneModel, SpawnKind};

/// How a `Primitive3D` is extruded from another drawable.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtrusionSpec {
    pub source: ObjectId,
    pub depth: f64,
    pub bevel: f64,
    pub tolerance: f64,
    /// Without one, the mesh takes the source's fill color.
    pub material: Option<Material3D>,
}

impl SceneModel {
    /// Extrude the area `source` fills into a closed 3D mesh `depth` deep,
    /// centered on the source's place in the z = 0 plane, with a 45° `bevel`
    /// on both caps. Curves flatten to `tolerance`. The source stays a
    /// drawable of its own; unless `keep_source`, it is hidden from here on.
    pub fn extrude(
        &mut self,
        source: &DrawableHandle,
        depth: f64,
        bevel: f64,
        tolerance: f64,
        material: Option<Material3D>,
        keep_source: bool,
    ) -> Result<DrawableHandle, String> {
        if !Arc::ptr_eq(&source.state, &self.state) {
            return Err("the drawable to extrude must belong to this scene".to_string());
        }
        if !depth.is_finite() || depth <= 0.0 {
            return Err(format!("depth must be finite and positive, got {depth}"));
        }
        if !bevel.is_finite() || bevel < 0.0 || bevel >= depth / 2.0 {
            return Err(format!(
                "bevel must be finite, non-negative and less than half the depth, got {bevel}"
            ));
        }
        if !tolerance.is_finite() || tolerance <= 0.0 {
            return Err(format!(
                "tolerance must be finite and positive, got {tolerance}"
            ));
        }
        if matches!(
            source.spec.lock().expect("object spec poisoned").kind,
            SpawnKind::Primitive3D(_)
                | SpawnKind::SurfaceMesh { .. }
                | SpawnKind::Polyline3D { .. }
                | SpawnKind::LineSegments3D { .. }
                | SpawnKind::Image { .. }
                | SpawnKind::Video { .. }
                | SpawnKind::Lottie { .. }
        ) {
            return Err("only 2D vector drawables can be extruded".to_string());
        }
        // A solid fill known now names the material, so animations start
        // from it; otherwise the compiled leaves' fill does.
        let material = material.or_else(|| {
            match source
                .spec
                .lock()
                .expect("object spec poisoned")
                .fill
                .as_ref()?
            {
                gaanim_core::peniko::Brush::Solid(color) => Some(Material3D::matte(*color)),
                _ => None,
            }
        });
        let mesh = self.spawn(SpawnKind::Primitive3D(TriangleMeshData {
            vertices: Vec::new(),
            indices: Vec::new(),
            normals: None,
            uvs: None,
            color: None,
            colors: None,
            material: Some(material.unwrap_or_default()),
        }));
        mesh.spec.lock().expect("object spec poisoned").extrusion = Some(ExtrusionSpec {
            source: source.id,
            depth,
            bevel,
            tolerance,
            material,
        });
        if !keep_source {
            source.clone().opacity(0.0);
        }
        Ok(mesh)
    }
}
