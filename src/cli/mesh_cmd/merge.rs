use miette::Result;
use owo_colors::OwoColorize;

use crate::core::mesh::MeshState;

use super::{MergeArgs, OutputFormat, project_root, run_eval};
use crate::cprintln;

pub fn cmd_merge(args: &MergeArgs) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    if args.all {
        let names: Vec<String> = state.parts.keys().cloned().collect();
        let mut total_merged = 0usize;
        let mut results = Vec::new();

        for name in &names {
            let part = state.parts.get_mut(name).unwrap();
            let (result, merged) =
                crate::core::mesh::merge::merge_by_distance(&part.mesh, args.distance);
            let vc = result.vertex_count();
            let fc = result.face_count();
            part.mesh = result;
            total_merged += merged;
            results.push(serde_json::json!({
                "part": name,
                "merged": merged,
                "vertex_count": vc,
                "face_count": fc,
            }));
        }

        state.save(&root)?;

        for name in &names {
            let push = state.generate_push_script(name)?;
            let _ = run_eval(&push);
        }

        let result = serde_json::json!({
            "distance": args.distance,
            "total_merged": total_merged,
            "parts": results,
        });

        match args.format {
            OutputFormat::Json => {
                cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
            }
            OutputFormat::Text => {
                cprintln!(
                    "Merged {} vertices across {} parts (distance {:.6})",
                    total_merged.to_string().green().bold(),
                    names.len().to_string().cyan(),
                    args.distance,
                );
            }
        }
    } else {
        let part = state.active_part_mut()?;
        let (result, merged) =
            crate::core::mesh::merge::merge_by_distance(&part.mesh, args.distance);
        let vc = result.vertex_count();
        let fc = result.face_count();
        part.mesh = result;

        state.save(&root)?;

        let active = state.active.clone();
        let push = state.generate_push_script(&active)?;
        let _ = run_eval(&push)?;

        let result = serde_json::json!({
            "distance": args.distance,
            "merged": merged,
            "vertex_count": vc,
            "face_count": fc,
        });

        match args.format {
            OutputFormat::Json => {
                cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
            }
            OutputFormat::Text => {
                let d = args.distance;
                cprintln!(
                    "Merged {} vertices (distance {d:.6}), {} vertices remaining",
                    merged.to_string().green().bold(),
                    vc.to_string().cyan(),
                );
            }
        }
    }
    Ok(())
}
