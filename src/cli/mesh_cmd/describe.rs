use std::time::Duration;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use super::gdscript;
use super::{DescribeArgs, OutputFormat, project_root, run_eval};
use crate::core::mesh::MeshState;
use crate::core::mesh::spatial;
use crate::cprintln;

#[expect(clippy::too_many_lines)]
pub fn cmd_describe(args: &DescribeArgs) -> Result<()> {
    // 1. Focus all parts so composite view works
    let focus_script = gdscript::generate_focus_all();
    run_eval(&focus_script)?;

    // 2. Collect part inventory via info --all
    let info_script = gdscript::generate_info_all();
    let info_result = run_eval(&info_script)?;
    let info: serde_json::Value = serde_json::from_str(&info_result)
        .map_err(|e| miette!("Failed to parse info result: {e}"))?;

    // 3. Auto-fit cameras and capture screenshots
    let autofit_script = gdscript::generate_autofit_cameras(args.zoom);
    let autofit_result = run_eval(&autofit_script)?;
    let autofit: serde_json::Value = serde_json::from_str(&autofit_result)
        .map_err(|e| miette!("Failed to parse autofit result: {e}"))?;

    let views = args.view.camera_names();

    let mut captures = Vec::new();
    for view_name in &views {
        let switch_script = gdscript::generate_switch_camera(view_name);
        run_eval(&switch_script)?;
        std::thread::sleep(Duration::from_millis(150));

        let capture_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(1);
        let capture_script =
            gdscript::generate_capture_screenshot(&view_name.to_lowercase(), capture_id);
        let result = run_eval(&capture_script)?;
        let screenshot: serde_json::Value = serde_json::from_str(&result)
            .map_err(|e| miette!("Failed to parse screenshot result: {e}"))?;

        let path = screenshot["path"]
            .as_str()
            .ok_or_else(|| miette!("No screenshot path in result"))?;

        let final_path = if let Some(ref dir) = args.output {
            std::fs::create_dir_all(dir)
                .map_err(|e| miette!("Failed to create output directory: {e}"))?;
            let dest = std::path::Path::new(dir).join(format!("{}.png", view_name.to_lowercase()));
            std::fs::copy(path, &dest).map_err(|e| miette!("Failed to copy screenshot: {e}"))?;
            let _ = std::fs::remove_file(path);
            dest.to_string_lossy().to_string()
        } else {
            path.to_string()
        };

        captures.push((view_name.to_lowercase(), final_path));
    }

    // Restore original camera
    let restore_script = gdscript::generate_restore_camera();
    let _ = run_eval(&restore_script);

    // 4. Load Rust-side mesh state for spatial analysis and health metrics
    let mesh_state = project_root().and_then(|root| MeshState::load(&root)).ok();

    match args.format {
        OutputFormat::Json => {
            let views_map: serde_json::Map<String, serde_json::Value> = captures
                .iter()
                .map(|(v, p)| (v.clone(), serde_json::Value::String(p.clone())))
                .collect();

            let mut output = info.clone();
            output["views"] = serde_json::Value::Object(views_map);
            output["camera_size"] = autofit["camera_size"].clone();
            output["center"] = autofit["center"].clone();

            if let Some(ref state) = mesh_state {
                // Add per-part health metrics
                if let Some(parts_arr) = output["parts"].as_array_mut() {
                    for part_json in parts_arr {
                        if let Some(name) = part_json["name"].as_str()
                            && let Some(part) = state.parts.get(name)
                        {
                            let nm = spatial::count_non_manifold_edges(&part.mesh);
                            let wt = spatial::is_watertight(&part.mesh);
                            part_json["non_manifold_edges"] = serde_json::json!(nm);
                            part_json["watertight"] = serde_json::json!(wt);
                        }
                    }
                }

                // Add relationships
                let relationships = spatial::relationship_report(state);
                if !relationships.is_empty() {
                    output["relationships"] = serde_json::json!(relationships);
                }
            }

            cprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        OutputFormat::Text => {
            // Part summary
            let active = info["active"].as_str().unwrap_or("?");
            let count = info["part_count"].as_u64().unwrap_or(0);
            let total_vc = info["total_vertex_count"].as_u64().unwrap_or(0);
            let total_fc = info["total_face_count"].as_u64().unwrap_or(0);
            cprintln!(
                "{count} parts, {total_vc} vertices, {total_fc} faces (active: {})",
                active.cyan()
            );
            if let Some(parts) = info["parts"].as_array() {
                for p in parts {
                    let name = p["name"].as_str().unwrap_or("?");
                    let vc = p["vertex_count"].as_u64().unwrap_or(0);
                    let marker = if name == active { " *" } else { "" };
                    cprintln!("  {}{marker}: {vc} vertices", name.green());
                }
            }

            // Print spatial issues if available
            if let Some(ref state) = mesh_state {
                let issues = spatial::check_part_relationships(state);
                if !issues.is_empty() {
                    cprintln!();
                    cprintln!("{}", "Spatial issues:".red().bold());
                    for issue in &issues {
                        cprintln!("  {} {}", "!".red(), issue.detail);
                    }
                }
            }

            cprintln!();
            for (view, path) in &captures {
                cprintln!("{} {view}: {}", "Screenshot".green(), path.cyan());
            }
        }
    }
    Ok(())
}
