use miette::Result;
use owo_colors::OwoColorize;

use crate::core::mesh::MeshState;
use crate::core::mesh::boolean::{self, BooleanMode};

use super::{BooleanArgs, BooleanOp, OutputFormat, parse_3d, project_root, run_eval};

pub fn cmd_boolean(args: &BooleanArgs) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let active_name = state.active.clone();
    let tool_name = &args.tool;

    let offset = if let Some(ref offset_str) = args.offset {
        let (x, y, z) = parse_3d(offset_str)?;
        [x, y, z]
    } else {
        [0.0; 3]
    };

    let mode = match args.mode {
        BooleanOp::Subtract => BooleanMode::Subtract,
        BooleanOp::Union => BooleanMode::Union,
        BooleanOp::Intersect => BooleanMode::Intersect,
    };

    let tool_mesh = state.resolve_part(Some(tool_name))?.mesh.clone();
    let target_mesh = state.active_part()?.mesh.clone();

    let result_mesh = boolean::boolean_op(&target_mesh, &tool_mesh, offset, mode);
    let vc = result_mesh.vertex_count();
    let fc = result_mesh.face_count();

    state.active_part_mut()?.mesh = result_mesh;
    state.save(&root)?;

    // Push to Godot
    let push = state.generate_push_script(&active_name)?;
    let _ = run_eval(&push)?;

    let mode_str = match args.mode {
        BooleanOp::Subtract => "subtract",
        BooleanOp::Union => "union",
        BooleanOp::Intersect => "intersect",
    };

    let result = serde_json::json!({
        "mode": mode_str,
        "name": active_name,
        "tool": tool_name,
        "face_count": fc,
        "vertex_count": vc,
    });

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            println!(
                "Boolean {}: {} with {}, {fc} faces, {vc} vertices",
                mode_str.cyan(),
                active_name.green().bold(),
                tool_name.cyan(),
            );
        }
    }
    Ok(())
}
