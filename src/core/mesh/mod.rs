pub mod array;
pub mod bevel;
pub mod boolean;
pub mod extrude;
pub mod extrude_face;
pub mod half_edge;
pub mod inset;
pub mod loft;
pub mod loop_cut;
pub mod merge;
pub mod mirror;
pub mod normals;
pub mod profile;
pub mod revolve;
pub mod solidify;
pub mod spatial;
pub mod spatial_filter;
pub mod subdivide;
pub mod taper;
pub mod topology;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;

use indexmap::IndexMap;
use miette::{Result, miette};
use serde::{Deserialize, Serialize};

use half_edge::HalfEdgeMesh;

/// Serialization version for forward compatibility.
const STATE_VERSION: u32 = 1;

/// Which plane a profile was drawn on.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PlaneKind {
    Front, // XY plane (extrude along Z)
    Side,  // ZY plane (extrude along X)
    Top,   // XZ plane (extrude along Y)
}

impl PlaneKind {
    /// The axis index for extrusion direction: X=0, Y=1, Z=2.
    #[allow(dead_code)]
    pub fn extrude_axis(self) -> usize {
        match self {
            Self::Front => 2, // Z
            Self::Side => 0,  // X
            Self::Top => 1,   // Y
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Front => "front",
            Self::Side => "side",
            Self::Top => "top",
        }
    }
}

/// A simple 3D transform (position + euler rotation in degrees + scale).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transform3D {
    pub position: [f64; 3],
    pub rotation: [f64; 3],
    pub scale: [f64; 3],
}

impl Default for Transform3D {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            rotation: [0.0; 3],
            scale: [1.0; 3],
        }
    }
}

impl Transform3D {
    /// Apply this transform to a point: scale → rotate (YXZ euler) → translate.
    pub fn apply_point(&self, p: [f64; 3]) -> [f64; 3] {
        // Scale
        let s = [
            p[0] * self.scale[0],
            p[1] * self.scale[1],
            p[2] * self.scale[2],
        ];
        // Rotate (Godot YXZ euler order)
        let r = self.rotation_matrix();
        let rotated = mat3_mul(r, s);
        // Translate
        [
            rotated[0] + self.position[0],
            rotated[1] + self.position[1],
            rotated[2] + self.position[2],
        ]
    }

    /// Apply inverse transform: un-translate → un-rotate → un-scale.
    pub fn inverse_apply_point(&self, p: [f64; 3]) -> [f64; 3] {
        // Un-translate
        let t = [
            p[0] - self.position[0],
            p[1] - self.position[1],
            p[2] - self.position[2],
        ];
        // Un-rotate (transpose of rotation matrix)
        let r = self.rotation_matrix();
        let rt = mat3_transpose(r);
        let unrotated = mat3_mul(rt, t);
        // Un-scale
        let sx = if self.scale[0].abs() > 1e-12 {
            1.0 / self.scale[0]
        } else {
            0.0
        };
        let sy = if self.scale[1].abs() > 1e-12 {
            1.0 / self.scale[1]
        } else {
            0.0
        };
        let sz = if self.scale[2].abs() > 1e-12 {
            1.0 / self.scale[2]
        } else {
            0.0
        };
        [unrotated[0] * sx, unrotated[1] * sy, unrotated[2] * sz]
    }

    /// Returns true if this transform is identity (no position, rotation, or non-unit scale).
    pub fn is_identity(&self) -> bool {
        let eps = 1e-12;
        self.position[0].abs() < eps
            && self.position[1].abs() < eps
            && self.position[2].abs() < eps
            && self.rotation[0].abs() < eps
            && self.rotation[1].abs() < eps
            && self.rotation[2].abs() < eps
            && (self.scale[0] - 1.0).abs() < eps
            && (self.scale[1] - 1.0).abs() < eps
            && (self.scale[2] - 1.0).abs() < eps
    }

    /// Build rotation matrix from euler angles in degrees (Godot YXZ order).
    fn rotation_matrix(&self) -> [[f64; 3]; 3] {
        let rx = self.rotation[0].to_radians();
        let ry = self.rotation[1].to_radians();
        let rz = self.rotation[2].to_radians();

        let (sx, cx) = (rx.sin(), rx.cos());
        let (sy, cy) = (ry.sin(), ry.cos());
        let (sz, cz) = (rz.sin(), rz.cos());

        // R = Ry * Rx * Rz (Godot's YXZ convention)
        [
            [cy * cz + sy * sx * sz, -cy * sz + sy * sx * cz, sy * cx],
            [cx * sz, cx * cz, -sx],
            [-sy * cz + cy * sx * sz, sy * sz + cy * sx * cz, cy * cx],
        ]
    }
}

fn mat3_mul(m: [[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

fn mat3_transpose(m: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    [
        [m[0][0], m[1][0], m[2][0]],
        [m[0][1], m[1][1], m[2][1]],
        [m[0][2], m[1][2], m[2][2]],
    ]
}

/// Shading mode for a mesh part.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub enum ShadingMode {
    /// Flat: each face gets its own normal (faceted look).
    #[default]
    Flat,
    /// Smooth: shared vertex normals averaged across adjacent faces.
    Smooth,
    /// Auto smooth: smooth below angle threshold (degrees), sharp above.
    AutoSmooth(f64),
}

/// A single mesh part in the session.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeshPart {
    pub mesh: HalfEdgeMesh,
    pub transform: Transform3D,
    pub color: Option<[f32; 3]>,
    pub profile_points: Option<Vec<[f64; 2]>>,
    pub profile_plane: Option<PlaneKind>,
    #[serde(default)]
    pub shading: ShadingMode,
    /// Hole contours for multi-contour profiles.
    #[serde(default)]
    pub profile_holes: Option<Vec<Vec<[f64; 2]>>>,
    /// Material preset name (e.g. "metal", "glass") — restored on every push.
    #[serde(default)]
    pub material_preset: Option<String>,
}

impl MeshPart {
    pub fn new() -> Self {
        Self {
            mesh: HalfEdgeMesh::default(),
            transform: Transform3D::default(),
            color: None,
            profile_points: None,
            profile_plane: None,
            shading: ShadingMode::default(),
            profile_holes: None,
            material_preset: None,
        }
    }
}

/// Persistent state for a mesh editing session.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeshState {
    version: u32,
    pub parts: IndexMap<String, MeshPart>,
    pub active: String,
    pub checkpoints: HashMap<String, IndexMap<String, MeshPart>>,
    /// Group snapshots keyed by checkpoint label (parallel to `checkpoints`).
    #[serde(default)]
    pub group_checkpoints: HashMap<String, HashMap<String, Vec<String>>>,
    /// Named groups of part names for batch operations.
    #[serde(default)]
    pub groups: HashMap<String, Vec<String>>,
}

impl MeshState {
    /// Create a new session with one empty part.
    pub fn new(name: &str) -> Self {
        let mut parts = IndexMap::new();
        parts.insert(name.to_string(), MeshPart::new());
        Self {
            version: STATE_VERSION,
            parts,
            active: name.to_string(),
            checkpoints: HashMap::new(),
            group_checkpoints: HashMap::new(),
            groups: HashMap::new(),
        }
    }

    /// Load state from `.gd-mesh/state.bin` in the project root.
    pub fn load(project_root: &Path) -> Result<Self> {
        let path = project_root.join(".gd-mesh").join("state.bin");
        if !path.exists() {
            return Err(miette!(
                "No mesh session found. Run 'gd mesh create' first."
            ));
        }
        let data = std::fs::read(&path).map_err(|e| miette!("Failed to read mesh state: {e}"))?;
        let state: Self = bincode::deserialize(&data)
            .map_err(|e| miette!("Failed to deserialize mesh state: {e}"))?;
        Ok(state)
    }

    /// Save state to `.gd-mesh/state.bin`.
    pub fn save(&self, project_root: &Path) -> Result<()> {
        let dir = project_root.join(".gd-mesh");
        std::fs::create_dir_all(&dir)
            .map_err(|e| miette!("Failed to create .gd-mesh directory: {e}"))?;
        let path = dir.join("state.bin");
        let data =
            bincode::serialize(self).map_err(|e| miette!("Failed to serialize mesh state: {e}"))?;
        std::fs::write(&path, data).map_err(|e| miette!("Failed to write mesh state: {e}"))?;
        Ok(())
    }

    /// Get a reference to the active part.
    pub fn active_part(&self) -> Result<&MeshPart> {
        self.parts
            .get(&self.active)
            .ok_or_else(|| miette!("Active part '{}' not found", self.active))
    }

    /// Get a mutable reference to the active part.
    pub fn active_part_mut(&mut self) -> Result<&mut MeshPart> {
        let name = self.active.clone();
        self.parts
            .get_mut(&name)
            .ok_or_else(|| miette!("Active part '{name}' not found"))
    }

    /// Get a mutable reference to a named part, or the active part if None.
    pub fn resolve_part_mut(&mut self, name: Option<&str>) -> Result<&mut MeshPart> {
        let key = name.unwrap_or(&self.active).to_string();
        self.parts
            .get_mut(&key)
            .ok_or_else(|| miette!("Part '{key}' not found"))
    }

    /// Get a reference to a named part, or the active part if None.
    pub fn resolve_part(&self, name: Option<&str>) -> Result<&MeshPart> {
        let key = name.unwrap_or(&self.active);
        self.parts
            .get(key)
            .ok_or_else(|| miette!("Part '{key}' not found"))
    }

    /// Generate GDScript to push a part's mesh to Godot as an `ArrayMesh`.
    ///
    /// The script finds the `MeshInstance3D` node under `_GdMeshHelper` and
    /// replaces its mesh with the computed indexed geometry.
    pub fn generate_push_script(&self, part_name: &str) -> Result<String> {
        let part = self
            .parts
            .get(part_name)
            .ok_or_else(|| miette!("Part '{part_name}' not found"))?;

        let (positions, normals, indices) = part.mesh.to_arrays_shaded(part.shading);

        let mut script = String::with_capacity(positions.len() * 20);
        script.push_str("extends Node\n\nfunc run():\n");
        script.push_str("\tvar root = get_tree().get_root()\n");
        script.push_str("\tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n");
        script.push_str("\tif not helper:\n");
        script.push_str("\t\treturn \"ERROR: No mesh session. Run 'gd mesh create' first.\"\n");

        // Find the mesh instance
        let _ = writeln!(
            script,
            "\tvar mesh_inst = helper.get_node_or_null(\"{part_name}\")"
        );
        script.push_str("\tif not mesh_inst:\n");
        let _ = writeln!(
            script,
            "\t\treturn \"ERROR: Part '{part_name}' not found in scene.\""
        );

        // Build arrays and assign mesh
        write_array_mesh(&mut script, &positions, &normals, &indices);

        // Restore material (full preset PBR properties if set, else just color)
        write_material_restore(&mut script, part);

        // Store profile metadata
        write_profile_metadata(&mut script, part);

        // Auto-focus: reframe all cameras to fit visible parts
        write_auto_focus(&mut script);

        // Return JSON result
        let vc = part.mesh.vertex_count();
        let fc = part.mesh.face_count();
        let (aabb_min, aabb_max) = part.mesh.aabb();
        let _ = writeln!(
            script,
            "\treturn JSON.stringify({{\"name\": \"{part_name}\", \"vertex_count\": {vc}, \"face_count\": {fc}, \
             \"aabb_min\": [{}, {}, {}], \"aabb_max\": [{}, {}, {}]}})",
            fmt_f64(aabb_min[0]),
            fmt_f64(aabb_min[1]),
            fmt_f64(aabb_min[2]),
            fmt_f64(aabb_max[0]),
            fmt_f64(aabb_max[1]),
            fmt_f64(aabb_max[2]),
        );

        Ok(script)
    }
}

/// Write the ArrayMesh construction portion of the push script.
fn write_array_mesh(script: &mut String, positions: &[f64], normals: &[f64], indices: &[u32]) {
    script.push_str("\tvar mesh = ArrayMesh.new()\n");
    script.push_str("\tvar arrays = []\n");
    script.push_str("\tarrays.resize(Mesh.ARRAY_MAX)\n");

    script.push_str("\tvar verts = PackedVector3Array()\n");
    for chunk in positions.chunks(3) {
        let _ = writeln!(
            script,
            "\tverts.append(Vector3({}, {}, {}))",
            fmt_f64(chunk[0]),
            fmt_f64(chunk[1]),
            fmt_f64(chunk[2])
        );
    }
    script.push_str("\tarrays[Mesh.ARRAY_VERTEX] = verts\n");

    script.push_str("\tvar norms = PackedVector3Array()\n");
    for chunk in normals.chunks(3) {
        let _ = writeln!(
            script,
            "\tnorms.append(Vector3({}, {}, {}))",
            fmt_f64(chunk[0]),
            fmt_f64(chunk[1]),
            fmt_f64(chunk[2])
        );
    }
    script.push_str("\tarrays[Mesh.ARRAY_NORMAL] = norms\n");

    script.push_str("\tvar idx = PackedInt32Array([\n\t\t");
    for (i, index) in indices.iter().enumerate() {
        if i > 0 {
            script.push_str(", ");
        }
        if i > 0 && i.is_multiple_of(12) {
            script.push_str("\n\t\t");
        }
        script.push_str(&index.to_string());
    }
    script.push_str("\n\t])\n");
    script.push_str("\tarrays[Mesh.ARRAY_INDEX] = idx\n");
    script.push_str("\tmesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)\n");
    script.push_str("\tmesh_inst.mesh = mesh\n");
}

/// Write material restoration to push script.
///
/// If the part has a stored material preset, restores the full PBR properties
/// (metallic, roughness, specular, etc.) so mesh edits don't wipe preset data.
/// Falls back to albedo-only restore from Godot meta if no preset is stored.
fn write_material_restore(script: &mut String, part: &MeshPart) {
    if let Some(ref preset) = part.material_preset {
        script.push_str("\tvar _mat = StandardMaterial3D.new()\n");
        // Restore color from Rust state or Godot meta fallback
        if let Some([r, g, b]) = part.color {
            let _ = writeln!(script, "\t_mat.albedo_color = Color({r}, {g}, {b})");
        } else {
            script.push_str("\tif mesh_inst.has_meta(\"part_color\"):\n");
            script.push_str("\t\t_mat.albedo_color = mesh_inst.get_meta(\"part_color\")\n");
        }
        // Set PBR properties based on preset
        let pbr = match preset.as_str() {
            "glass" => {
                "\t_mat.metallic = 0.0\n\t_mat.roughness = 0.05\n\t_mat.specular = 0.5\n\t_mat.transparency = 1\n\t_mat.albedo_color.a = 0.3\n\t_mat.refraction_enabled = true\n\t_mat.refraction_scale = 0.02\n"
            }
            "metal" => "\t_mat.metallic = 0.9\n\t_mat.roughness = 0.3\n\t_mat.specular = 0.8\n",
            "chrome" => "\t_mat.metallic = 1.0\n\t_mat.roughness = 0.05\n\t_mat.specular = 1.0\n",
            "rubber" => "\t_mat.metallic = 0.0\n\t_mat.roughness = 0.95\n\t_mat.specular = 0.1\n",
            "paint" => "\t_mat.metallic = 0.1\n\t_mat.roughness = 0.4\n\t_mat.specular = 0.5\n",
            "wood" => "\t_mat.metallic = 0.0\n\t_mat.roughness = 0.7\n\t_mat.specular = 0.2\n",
            "matte" => "\t_mat.metallic = 0.0\n\t_mat.roughness = 1.0\n\t_mat.specular = 0.0\n",
            _ => "\t_mat.metallic = 0.0\n\t_mat.roughness = 0.4\n\t_mat.specular = 0.5\n", // plastic
        };
        script.push_str(pbr);
        script.push_str("\tmesh_inst.material_override = _mat\n");
        script.push_str("\tmesh_inst.set_meta(\"part_color\", _mat.albedo_color)\n");
    } else {
        // No preset — restore just color from meta
        script.push_str("\tif mesh_inst.has_meta(\"part_color\"):\n");
        script.push_str("\t\tvar _mat = StandardMaterial3D.new()\n");
        script.push_str("\t\t_mat.albedo_color = mesh_inst.get_meta(\"part_color\")\n");
        script.push_str("\t\tmesh_inst.material_override = _mat\n");
    }
}

/// Auto-focus all cameras on the combined AABB of visible parts.
///
/// Runs after every push so the user always sees the object in the viewport.
fn write_auto_focus(script: &mut String) {
    script.push_str("\tvar _combined = AABB()\n");
    script.push_str("\tvar _first = true\n");
    script.push_str("\tfor _ch in helper.get_children():\n");
    script.push_str("\t\tif _ch is MeshInstance3D and not _ch.name.begins_with(\"_\") and _ch.visible and _ch.mesh and _ch.mesh.get_surface_count() > 0:\n");
    script.push_str("\t\t\tvar _ab = _ch.transform * _ch.mesh.get_aabb()\n");
    script.push_str("\t\t\tif _first:\n");
    script.push_str("\t\t\t\t_combined = _ab\n");
    script.push_str("\t\t\t\t_first = false\n");
    script.push_str("\t\t\telse:\n");
    script.push_str("\t\t\t\t_combined = _combined.merge(_ab)\n");
    script.push_str("\tif not _first:\n");
    script.push_str("\t\tvar _center = _combined.get_center()\n");
    script.push_str("\t\tvar _dims = _combined.size\n");
    script.push_str("\t\tvar _sz = max(max(_dims.x, _dims.y), _dims.z) * 1.5\n");
    script.push_str("\t\tif _sz < 0.5: _sz = 0.5\n");
    script.push_str("\t\tvar _rig = helper.get_node_or_null(\"_CameraRig\")\n");
    script.push_str("\t\tif _rig:\n");
    script.push_str("\t\t\t_rig.position = _center\n");
    script.push_str("\t\t\tfor _cam in _rig.get_children():\n");
    script.push_str("\t\t\t\tif _cam is Camera3D:\n");
    script.push_str("\t\t\t\t\tif _cam.projection == Camera3D.PROJECTION_ORTHOGONAL:\n");
    script.push_str("\t\t\t\t\t\t_cam.size = _sz\n");
    script.push_str("\t\t\t\t\telse:\n");
    script.push_str("\t\t\t\t\t\tvar _half_fov = deg_to_rad(_cam.fov * 0.5)\n");
    script.push_str("\t\t\t\t\t\tvar _dist = (_sz * 0.5) / tan(_half_fov)\n");
    script.push_str("\t\t\t\t\t\tif _dist < 1.0: _dist = 1.0\n");
    script.push_str("\t\t\t\t\t\t_cam.position = _cam.position.normalized() * _dist\n");
    script.push_str("\t\t\t\t\tif _cam.name == \"Top\" or _cam.name == \"Bottom\":\n");
    script.push_str("\t\t\t\t\t\t_cam.look_at(_center, Vector3.FORWARD)\n");
    script.push_str("\t\t\t\t\telse:\n");
    script.push_str("\t\t\t\t\t\t_cam.look_at(_center)\n");
}

/// Write profile metadata to push script.
fn write_profile_metadata(script: &mut String, part: &MeshPart) {
    if let Some(ref points) = part.profile_points {
        let pts_str: Vec<String> = points
            .iter()
            .map(|p| format!("Vector2({}, {})", fmt_f64(p[0]), fmt_f64(p[1])))
            .collect();
        let _ = writeln!(
            script,
            "\tmesh_inst.set_meta(\"profile_points\", [{}])",
            pts_str.join(", ")
        );
    }
    if let Some(plane) = part.profile_plane {
        let _ = writeln!(
            script,
            "\tmesh_inst.set_meta(\"profile_plane\", \"{}\")",
            plane.as_str()
        );
    }
}

/// Format f64 for GDScript: at most 6 decimal places, trim trailing zeros.
fn fmt_f64(v: f64) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0');
    let s = s.trim_end_matches('.');
    s.to_string()
}
