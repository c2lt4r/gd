use std::time::Duration;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use super::gdscript;
use super::{DescribeArgs, OutputFormat, run_eval};
use crate::cprintln;

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
            cprintln!();
            for (view, path) in &captures {
                cprintln!("{} {view}: {}", "Screenshot".green(), path.cyan());
            }
        }
    }
    Ok(())
}
