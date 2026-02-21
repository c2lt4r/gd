use super::PlaneKind;
use super::extrude::extrude_with_inset;
use super::half_edge::HalfEdgeMesh;

/// Rust-native unit cube: 8 vertices at ±0.5, 6 quad faces (CCW winding).
pub fn cube() -> HalfEdgeMesh {
    #[rustfmt::skip]
    let positions: &[[f64; 3]] = &[
        [-0.5, -0.5, -0.5], // 0: left-bottom-back
        [ 0.5, -0.5, -0.5], // 1: right-bottom-back
        [ 0.5,  0.5, -0.5], // 2: right-top-back
        [-0.5,  0.5, -0.5], // 3: left-top-back
        [-0.5, -0.5,  0.5], // 4: left-bottom-front
        [ 0.5, -0.5,  0.5], // 5: right-bottom-front
        [ 0.5,  0.5,  0.5], // 6: right-top-front
        [-0.5,  0.5,  0.5], // 7: left-top-front
    ];

    // CCW winding when viewed from outside each face
    let faces: &[&[usize]] = &[
        &[4, 5, 6, 7], // front  (+Z)
        &[1, 0, 3, 2], // back   (-Z)
        &[3, 7, 6, 2], // top    (+Y)
        &[0, 1, 5, 4], // bottom (-Y)
        &[0, 4, 7, 3], // left   (-X)
        &[5, 1, 2, 6], // right  (+X)
    ];

    HalfEdgeMesh::from_polygons(positions, faces)
}

/// Rust-native UV sphere: `segments` longitude slices, `rings` latitude bands.
///
/// Radius 0.5, centered at origin (matching Godot's default `SphereMesh`).
/// Top/bottom caps are triangle fans, middle bands are quads.
pub fn sphere(segments: u32, rings: u32) -> HalfEdgeMesh {
    let segs = segments.max(3) as usize;
    let rings = rings.max(2) as usize;
    let radius = 0.5;

    // (rings - 1) latitude rings of vertices + 2 poles
    let n_verts = segs * (rings - 1) + 2;
    let mut positions = Vec::with_capacity(n_verts);

    // Top pole (index 0)
    positions.push([0.0, radius, 0.0]);

    // Latitude rings from top to bottom (ring 1..rings-1)
    for r in 1..rings {
        let phi = std::f64::consts::PI * r as f64 / rings as f64;
        let y = radius * phi.cos();
        let ring_r = radius * phi.sin();
        for s in 0..segs {
            let theta = std::f64::consts::TAU * s as f64 / segs as f64;
            positions.push([ring_r * theta.sin(), y, ring_r * theta.cos()]);
        }
    }

    // Bottom pole (last index)
    positions.push([0.0, -radius, 0.0]);
    let bottom = positions.len() - 1;

    let mut faces: Vec<Vec<usize>> = Vec::new();

    // Top cap: triangle fan from pole to first ring (CCW from outside = looking down)
    for s in 0..segs {
        let next = (s + 1) % segs;
        faces.push(vec![0, 1 + s, 1 + next]);
    }

    // Middle bands: quads (CCW from outside)
    for r in 0..rings - 2 {
        let row_start = 1 + r * segs;
        let next_row = 1 + (r + 1) * segs;
        for s in 0..segs {
            let next = (s + 1) % segs;
            faces.push(vec![
                row_start + s,
                next_row + s,
                next_row + next,
                row_start + next,
            ]);
        }
    }

    // Bottom cap: triangle fan from last ring to pole (CCW from outside = looking up)
    let last_row = 1 + (rings - 2) * segs;
    for s in 0..segs {
        let next = (s + 1) % segs;
        faces.push(vec![last_row + next, last_row + s, bottom]);
    }

    let face_slices: Vec<&[usize]> = faces.iter().map(Vec::as_slice).collect();
    HalfEdgeMesh::from_polygons(&positions, &face_slices)
}

/// Rust-native cylinder: circle profile extruded along Y with grid-fill caps.
///
/// Radius 0.5, height 1.0, centered at origin (matching Godot's default
/// `CylinderMesh`). Uses the existing extrude pipeline for correct quad
/// topology and multi-ring caps.
pub fn cylinder(segments: u32) -> HalfEdgeMesh {
    let segs = segments.max(3);
    let radius = 0.5;

    // Generate circle profile on the Top (XZ) plane — extrudes along Y
    let points: Vec<[f64; 2]> = (0..segs)
        .map(|i| {
            let angle = std::f64::consts::TAU * f64::from(i) / f64::from(segs);
            [radius * angle.cos(), radius * angle.sin()]
        })
        .collect();

    // Extrude 1.0 unit along Y with auto-inset caps (0.15 for smooth cap topology)
    extrude_with_inset(&points, PlaneKind::Top, 1.0, 1, 0.15)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_geometry() {
        let mesh = cube();
        assert_eq!(mesh.vertex_count(), 8);
        assert_eq!(mesh.face_count(), 6);
        // All faces should be quads
        for f in 0..6 {
            assert_eq!(mesh.face_vertices(f).len(), 4, "face {f} should be a quad");
        }
        // Watertight
        assert!(mesh.boundary_edges().is_empty(), "cube should be watertight");
    }

    #[test]
    fn cube_normals_outward() {
        let mesh = cube();
        let center = mesh_center(&mesh);
        for f in 0..mesh.face_count() {
            let verts = mesh.face_vertices(f);
            let normal = face_normal(&mesh, f);
            let fc = face_centroid(&mesh, &verts);
            let outward = [fc[0] - center[0], fc[1] - center[1], fc[2] - center[2]];
            let dot = normal[0] * outward[0] + normal[1] * outward[1] + normal[2] * outward[2];
            assert!(dot > 0.0, "face {f} normal should point outward (dot={dot})");
        }
    }

    #[test]
    fn cube_dimensions() {
        let mesh = cube();
        let (mn, mx) = mesh.aabb();
        for ax in 0..3 {
            assert!((mn[ax] - -0.5).abs() < 1e-10, "min[{ax}] should be -0.5");
            assert!((mx[ax] - 0.5).abs() < 1e-10, "max[{ax}] should be 0.5");
        }
    }

    #[test]
    fn sphere_geometry() {
        let mesh = sphere(16, 8);
        assert!(mesh.face_count() > 0);
        assert!(mesh.vertex_count() > 0);
        // Watertight
        assert!(
            mesh.boundary_edges().is_empty(),
            "sphere should be watertight"
        );
    }

    #[test]
    fn sphere_normals_outward() {
        let mesh = sphere(16, 8);
        let center = mesh_center(&mesh);
        for f in 0..mesh.face_count() {
            let verts = mesh.face_vertices(f);
            let normal = face_normal(&mesh, f);
            let fc = face_centroid(&mesh, &verts);
            let outward = [fc[0] - center[0], fc[1] - center[1], fc[2] - center[2]];
            let dot = normal[0] * outward[0] + normal[1] * outward[1] + normal[2] * outward[2];
            assert!(
                dot > -1e-6,
                "sphere face {f} normal should point outward (dot={dot})"
            );
        }
    }

    #[test]
    fn sphere_dimensions() {
        let mesh = sphere(32, 16);
        let (mn, mx) = mesh.aabb();
        for ax in 0..3 {
            assert!(mn[ax] >= -0.5 - 1e-10, "min[{ax}] should be >= -0.5");
            assert!(mx[ax] <= 0.5 + 1e-10, "max[{ax}] should be <= 0.5");
        }
    }

    #[test]
    fn cylinder_geometry() {
        let mesh = cylinder(32);
        assert!(mesh.face_count() > 0);
        assert!(mesh.vertex_count() > 0);
        // Watertight
        assert!(
            mesh.boundary_edges().is_empty(),
            "cylinder should be watertight"
        );
    }

    #[test]
    fn cylinder_normals_outward() {
        let mesh = cylinder(16);
        let center = mesh_center(&mesh);
        for f in 0..mesh.face_count() {
            let verts = mesh.face_vertices(f);
            let normal = face_normal(&mesh, f);
            let fc = face_centroid(&mesh, &verts);
            let outward = [fc[0] - center[0], fc[1] - center[1], fc[2] - center[2]];
            let dot = normal[0] * outward[0] + normal[1] * outward[1] + normal[2] * outward[2];
            assert!(
                dot > -1e-6,
                "cylinder face {f} normal should point outward (dot={dot})"
            );
        }
    }

    #[test]
    fn cylinder_dimensions() {
        let mesh = cylinder(32);
        let (mn, mx) = mesh.aabb();
        // Height along Y: -0.5 to 0.5
        assert!((mn[1] - -0.5).abs() < 1e-10, "min Y should be -0.5");
        assert!((mx[1] - 0.5).abs() < 1e-10, "max Y should be 0.5");
        // Radius 0.5 on XZ
        assert!(mn[0] >= -0.5 - 1e-10, "min X should be >= -0.5");
        assert!(mx[0] <= 0.5 + 1e-10, "max X should be <= 0.5");
    }

    // ── Test helpers ────────────────────────────────────────────────

    fn mesh_center(mesh: &HalfEdgeMesh) -> [f64; 3] {
        let n = mesh.vertex_count() as f64;
        let mut c = [0.0; 3];
        for v in &mesh.vertices {
            c[0] += v.position[0];
            c[1] += v.position[1];
            c[2] += v.position[2];
        }
        [c[0] / n, c[1] / n, c[2] / n]
    }

    fn face_centroid(mesh: &HalfEdgeMesh, verts: &[usize]) -> [f64; 3] {
        let n = verts.len() as f64;
        let mut c = [0.0; 3];
        for &vi in verts {
            let p = mesh.vertices[vi].position;
            c[0] += p[0];
            c[1] += p[1];
            c[2] += p[2];
        }
        [c[0] / n, c[1] / n, c[2] / n]
    }

    fn face_normal(mesh: &HalfEdgeMesh, f: usize) -> [f64; 3] {
        let verts = mesh.face_vertices(f);
        if verts.len() < 3 {
            return [0.0, 1.0, 0.0];
        }
        let p0 = mesh.vertices[verts[0]].position;
        let p1 = mesh.vertices[verts[1]].position;
        let p2 = mesh.vertices[verts[2]].position;
        let u = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let v = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if len < 1e-15 {
            [0.0, 1.0, 0.0]
        } else {
            [n[0] / len, n[1] / len, n[2] / len]
        }
    }
}
