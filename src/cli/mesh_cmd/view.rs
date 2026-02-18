use std::time::Duration;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use super::gdscript;
use super::{OutputFormat, ViewArgs, ViewName, run_eval};

/// Capture a single orthographic screenshot. Returns (view_name, file_path).
fn capture_view(
    view_name: &str,
    output_dir: Option<&str>,
    grid: bool,
    camera_half_size: f64,
) -> Result<(String, String)> {
    // Determine grid plane for this view
    let grid_plane = match view_name {
        "Front" | "Back" => Some("front"),
        "Side" | "Left" => Some("side"),
        "Top" | "Bottom" => Some("top"),
        _ => None, // Iso: no grid
    };

    // Add grid if requested (scaled to camera size)
    if grid
        && let Some(plane) = grid_plane
    {
        let grid_script = gdscript::generate_grid(plane, camera_half_size);
        let _ = run_eval(&grid_script);
    }

    // Switch camera
    let switch_script = gdscript::generate_switch_camera(view_name);
    run_eval(&switch_script)?;

    // Wait for Godot to render a frame with the new camera
    std::thread::sleep(Duration::from_millis(150));

    // Capture screenshot via eval
    let capture_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(1);
    let capture_script =
        gdscript::generate_capture_screenshot(&view_name.to_lowercase(), capture_id);
    let result = run_eval(&capture_script)?;
    let screenshot: serde_json::Value = serde_json::from_str(&result)
        .map_err(|e| miette!("Failed to parse screenshot result: {e}"))?;

    // Remove grid after capture
    if grid && grid_plane.is_some() {
        let remove_script = gdscript::generate_remove_grid();
        let _ = run_eval(&remove_script);
    }

    let path = screenshot["path"]
        .as_str()
        .ok_or_else(|| miette!("No screenshot path in result"))?;

    // Copy to output directory if specified
    let final_path = if let Some(dir) = output_dir {
        std::fs::create_dir_all(dir)
            .map_err(|e| miette!("Failed to create output directory: {e}"))?;
        let dest = std::path::Path::new(dir).join(format!("{}.png", view_name.to_lowercase()));
        std::fs::copy(path, &dest).map_err(|e| miette!("Failed to copy screenshot: {e}"))?;
        let _ = std::fs::remove_file(path);
        dest.to_string_lossy().to_string()
    } else {
        path.to_string()
    };

    Ok((view_name.to_lowercase(), final_path))
}

pub fn cmd_view(args: &ViewArgs) -> Result<()> {
    // If --focus is set, switch visibility before capturing
    if let Some(ref focus) = args.focus {
        if focus.eq_ignore_ascii_case("all") {
            let script = gdscript::generate_focus_all();
            run_eval(&script)?;
        } else {
            let script = gdscript::generate_focus(focus);
            run_eval(&script)?;
        }
    }

    // Always clear any stale normal-debug overlay from a previous call
    // (safe no-op if no ShaderMaterial is present)
    {
        let clear_script = gdscript::generate_normal_debug_clear();
        let _ = run_eval(&clear_script);
    }

    // Auto-fit cameras to the combined AABB of all visible parts
    let autofit_script = gdscript::generate_autofit_cameras(args.zoom);
    let autofit_result = run_eval(&autofit_script)?;
    let autofit: serde_json::Value = serde_json::from_str(&autofit_result)
        .map_err(|e| miette!("Failed to parse autofit result: {e}"))?;
    let camera_half_size = autofit["camera_size"].as_f64().unwrap_or(10.0) / 2.0;
    let visible_parts = autofit["visible_parts"].as_u64().unwrap_or(0);
    let total_parts = autofit["total_parts"].as_u64().unwrap_or(0);
    let has_hidden = total_parts > 0 && visible_parts < total_parts;

    // Apply face-orientation debug shader if --normals
    if args.normals {
        let debug_script = gdscript::generate_normal_debug();
        run_eval(&debug_script)?;
    }

    let views: Vec<&str> = match args.view {
        ViewName::Front => vec!["Front"],
        ViewName::Back => vec!["Back"],
        ViewName::Side => vec!["Side"],
        ViewName::Left => vec!["Left"],
        ViewName::Top => vec!["Top"],
        ViewName::Bottom => vec!["Bottom"],
        ViewName::Iso => vec!["Iso"],
        ViewName::All => vec!["Front", "Back", "Side", "Left", "Top", "Bottom", "Iso"],
    };

    let mut captures = Vec::new();
    for view in &views {
        let pair = capture_view(view, args.output.as_deref(), args.grid, camera_half_size)?;
        captures.push(pair);
    }

    // Remove face-orientation debug shader
    if args.normals {
        let clear_script = gdscript::generate_normal_debug_clear();
        let _ = run_eval(&clear_script);
    }

    // Restore original camera
    let restore_script = gdscript::generate_restore_camera();
    let _ = run_eval(&restore_script);

    match args.format {
        OutputFormat::Json => {
            // Flat view→path map instead of repeated per-screenshot objects
            let views_map: serde_json::Map<String, serde_json::Value> = captures
                .iter()
                .map(|(v, p)| (v.clone(), serde_json::Value::String(p.clone())))
                .collect();

            let mut output = serde_json::json!({
                "camera_size": autofit["camera_size"],
                "center": autofit["center"],
                "bounds": [-camera_half_size, camera_half_size],
                "views": views_map,
            });
            if args.normals {
                output["mode"] = serde_json::json!("normal_debug");
            }
            if has_hidden {
                output["warning"] = serde_json::json!(format!(
                    "Only {visible_parts}/{total_parts} parts visible. Use --focus all to show all parts."
                ));
            }
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        OutputFormat::Text => {
            if has_hidden {
                eprintln!(
                    "{}: only {visible_parts}/{total_parts} parts visible — use {} to show all",
                    "Warning".yellow(),
                    "--focus all".cyan()
                );
            }
            for (view, path) in &captures {
                println!(
                    "{} {view}: {}",
                    "Screenshot".green(),
                    path.cyan()
                );
            }
        }
    }
    Ok(())
}
