use std::cmp::Ordering;

use miette::{Result, miette};

use super::half_edge::HalfEdgeMesh;

/// A spatial filter that tests positions against a threshold on one axis.
pub struct SpatialFilter {
    pub axis: usize,   // 0=x, 1=y, 2=z
    pub op: Ordering,  // Greater or Less
    pub value: f64,
}

impl SpatialFilter {
    fn matches(&self, v: f64) -> bool {
        match self.op {
            Ordering::Greater => v > self.value,
            Ordering::Less => v < self.value,
            Ordering::Equal => (v - self.value).abs() < 1e-9,
        }
    }
}

/// Parse a spatial expression like "y>0.12", "z<-0.5", "x>=0".
pub fn parse_where(expr: &str) -> Result<SpatialFilter> {
    let expr = expr.trim();

    // Find the operator position (after the axis letter)
    let axis_char = expr
        .chars()
        .next()
        .ok_or_else(|| miette!("Empty --where expression"))?;

    let axis = match axis_char {
        'x' | 'X' => 0,
        'y' | 'Y' => 1,
        'z' | 'Z' => 2,
        _ => return Err(miette!("--where must start with x, y, or z (got '{axis_char}')")),
    };

    let rest = &expr[1..];

    let (op, val_str) = if let Some(s) = rest.strip_prefix(">=") {
        // >= maps to > with a small epsilon shift
        (Ordering::Greater, s)
    } else if let Some(s) = rest.strip_prefix("<=") {
        (Ordering::Less, s)
    } else if let Some(s) = rest.strip_prefix('>') {
        (Ordering::Greater, s)
    } else if let Some(s) = rest.strip_prefix('<') {
        (Ordering::Less, s)
    } else if let Some(s) = rest.strip_prefix('=') {
        (Ordering::Equal, s)
    } else {
        return Err(miette!(
            "Invalid operator in --where '{expr}'. Use >, <, >=, <=, or ="
        ));
    };

    let value: f64 = val_str
        .trim()
        .parse()
        .map_err(|_| miette!("Invalid number in --where '{expr}'"))?;

    Ok(SpatialFilter { axis, op, value })
}

/// Test whether a face's centroid passes the spatial filter.
pub fn face_matches(mesh: &HalfEdgeMesh, face_idx: usize, filter: &SpatialFilter) -> bool {
    let verts = mesh.face_vertices(face_idx);
    if verts.is_empty() {
        return false;
    }
    let n = verts.len() as f64;
    let sum: f64 = verts
        .iter()
        .map(|&vi| mesh.vertices[vi].position[filter.axis])
        .sum();
    filter.matches(sum / n)
}

/// Test whether a half-edge's midpoint passes the spatial filter.
pub fn edge_matches(mesh: &HalfEdgeMesh, he_idx: usize, filter: &SpatialFilter) -> bool {
    let he = &mesh.half_edges[he_idx];
    let v_to = mesh.vertices[he.vertex].position;
    let v_from = mesh.vertices[mesh.half_edges[he.prev].vertex].position;
    let mid = (v_to[filter.axis] + v_from[filter.axis]) * 0.5;
    filter.matches(mid)
}
