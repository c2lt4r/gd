use miette::Result;
use owo_colors::OwoColorize;

use gd_mesh::MeshState;
use gd_mesh::boolean::{self, BooleanMode};
use gd_mesh::half_edge::HalfEdgeMesh;

use super::{BooleanArgs, BooleanOp, OutputFormat, inject_stats, parse_3d, project_root, run_eval};
use gd_core::cprintln;

/// Apply a `Transform3D` to all vertices in a mesh (scale → rotate → translate).
fn transform_mesh(mesh: &HalfEdgeMesh, t: &gd_mesh::Transform3D) -> HalfEdgeMesh {
    if t.is_identity() {
        return mesh.clone();
    }
    let mut result = mesh.clone();
    for v in &mut result.vertices {
        v.position = t.apply_point(v.position);
    }
    result
}

/// Apply inverse transform to all vertices (un-translate → un-rotate → un-scale).
fn inverse_transform_mesh(mesh: &HalfEdgeMesh, t: &gd_mesh::Transform3D) -> HalfEdgeMesh {
    if t.is_identity() {
        return mesh.clone();
    }
    let mut result = mesh.clone();
    for v in &mut result.vertices {
        v.position = t.inverse_apply_point(v.position);
    }
    result
}

pub fn cmd_boolean(args: &BooleanArgs) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let active_name = state.active.clone();
    let tool_name = &args.tool;

    let explicit_offset = if let Some(ref offset_str) = args.offset {
        let (x, y, z) = parse_3d(offset_str)?;
        [x, y, z]
    } else {
        [0.0; 3]
    };

    let spacing = if let Some(ref spacing_str) = args.spacing {
        let (x, y, z) = parse_3d(spacing_str)?;
        [x, y, z]
    } else {
        explicit_offset
    };

    let count = args.count.unwrap_or(1).max(1);

    let mode = match args.mode {
        BooleanOp::Subtract => BooleanMode::Subtract,
        BooleanOp::Union => BooleanMode::Union,
        BooleanOp::Intersect => BooleanMode::Intersect,
    };

    let tool_part = state.resolve_part(Some(tool_name))?;
    let tool_transform = tool_part.transform.clone();
    let tool_mesh = tool_part.mesh.clone();

    let target_part = state.active_part()?;
    let target_transform = target_part.transform.clone();
    let target_mesh = target_part.mesh.clone();

    // Transform both meshes to world space so the boolean sees correct geometry
    let mut current = transform_mesh(&target_mesh, &target_transform);
    let tool_world = transform_mesh(&tool_mesh, &tool_transform);

    for k in 0..count {
        let iter_offset = [
            explicit_offset[0] + spacing[0] * k as f64,
            explicit_offset[1] + spacing[1] * k as f64,
            explicit_offset[2] + spacing[2] * k as f64,
        ];
        current = boolean::boolean_op(&current, &tool_world, iter_offset, mode);
    }

    // Transform result back to target's local coordinate space
    let result_local = inverse_transform_mesh(&current, &target_transform);

    let vc = result_local.vertex_count();
    let fc = result_local.face_count();

    if fc == 0 {
        eprintln!(
            "{}: boolean produced empty mesh — tool may not overlap target in world space",
            "warning".yellow().bold(),
        );
    }

    state.active_part_mut()?.mesh = result_local;
    state.save(&root)?;

    // Push to Godot
    let push = state.generate_push_script(&active_name)?;
    let _ = run_eval(&push)?;

    let mode_str = match args.mode {
        BooleanOp::Subtract => "subtract",
        BooleanOp::Union => "union",
        BooleanOp::Intersect => "intersect",
    };

    let mut result = serde_json::json!({
        "mode": mode_str,
        "name": active_name,
        "tool": tool_name,
        "count": count,
        "face_count": fc,
        "vertex_count": vc,
    });
    inject_stats(&mut result, &state);

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            cprintln!(
                "Boolean {}: {} with {} (x{count}), {fc} faces, {vc} vertices",
                mode_str.cyan(),
                active_name.green().bold(),
                tool_name.cyan(),
            );
        }
    }
    Ok(())
}
