use std::time::Duration;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use super::gdscript;
use super::{OutputFormat, ViewArgs, ViewName, run_eval};

/// Take a single orthographic screenshot for a given view.
fn capture_view(
    view_name: &str,
    output_dir: Option<&str>,
    grid: bool,
    camera_half_size: f64,
) -> Result<serde_json::Value> {
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
    let width = screenshot["width"].as_u64().unwrap_or(0);
    let height = screenshot["height"].as_u64().unwrap_or(0);

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

    let bounds = serde_json::json!({
        "x_min": -camera_half_size, "x_max": camera_half_size,
        "y_min": -camera_half_size, "y_max": camera_half_size,
    });

    Ok(serde_json::json!({
        "view": view_name.to_lowercase(),
        "path": final_path,
        "width": width,
        "height": height,
        "bounds": bounds,
    }))
}

pub fn cmd_view(args: &ViewArgs) -> Result<()> {
    // Auto-fit cameras to the combined AABB of all visible parts
    let autofit_script = gdscript::generate_autofit_cameras(args.zoom);
    let autofit_result = run_eval(&autofit_script)?;
    let autofit: serde_json::Value = serde_json::from_str(&autofit_result)
        .map_err(|e| miette!("Failed to parse autofit result: {e}"))?;
    let camera_half_size = autofit["camera_size"].as_f64().unwrap_or(10.0) / 2.0;

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

    let mut screenshots = Vec::new();
    for view in &views {
        let info = capture_view(view, args.output.as_deref(), args.grid, camera_half_size)?;
        screenshots.push(info);
    }

    // Restore original camera
    let restore_script = gdscript::generate_restore_camera();
    let _ = run_eval(&restore_script);

    match args.format {
        OutputFormat::Json => {
            let output = serde_json::json!({
                "screenshots": screenshots,
                "camera_size": autofit["camera_size"],
                "center": autofit["center"],
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        OutputFormat::Text => {
            for s in &screenshots {
                let view = s["view"].as_str().unwrap_or("?");
                let path = s["path"].as_str().unwrap_or("?");
                let w = s["width"].as_u64().unwrap_or(0);
                let h = s["height"].as_u64().unwrap_or(0);
                println!(
                    "{} {view}: {w}x{h} -> {}",
                    "Screenshot".green(),
                    path.cyan()
                );
            }
        }
    }
    Ok(())
}
