use std::path::Path;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use super::args::{OutputFormat, ScreenshotArgs, TransformCamera2dArgs, TransformCamera3dArgs};
use super::{daemon_cmd, ensure_binary_debug};

// ── Camera transforms (binary protocol) ──────────────────────────────

pub(crate) fn cmd_transform_camera_2d(args: &TransformCamera2dArgs) -> Result<()> {
    ensure_binary_debug()?;
    let parsed: serde_json::Value = serde_json::from_str(&args.transform)
        .map_err(|e| miette!("Invalid transform JSON: {e}"))?;
    if let Some(arr) = parsed.as_array() {
        if arr.len() != 6 {
            return Err(miette!(
                "2D transform requires exactly 6 floats, got {}",
                arr.len()
            ));
        }
    } else {
        return Err(miette!("Transform must be a JSON array of 6 floats"));
    }
    daemon_cmd(
        "debug_transform_camera_2d",
        serde_json::json!({"transform": parsed}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true})).unwrap()
            );
        }
        OutputFormat::Text => println!("{}", "2D camera transformed".green()),
    }
    Ok(())
}

pub(crate) fn cmd_transform_camera_3d(args: &TransformCamera3dArgs) -> Result<()> {
    ensure_binary_debug()?;
    let parsed: serde_json::Value = serde_json::from_str(&args.transform)
        .map_err(|e| miette!("Invalid transform JSON: {e}"))?;
    if let Some(arr) = parsed.as_array() {
        if arr.len() != 12 {
            return Err(miette!(
                "3D transform requires exactly 12 floats, got {}",
                arr.len()
            ));
        }
    } else {
        return Err(miette!("Transform must be a JSON array of 12 floats"));
    }
    daemon_cmd(
        "debug_transform_camera_3d",
        serde_json::json!({
            "transform": parsed,
            "perspective": args.perspective,
            "fov": args.fov,
            "near": args.near,
            "far": args.far,
        }),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true})).unwrap()
            );
        }
        OutputFormat::Text => println!("{}", "3D camera transformed".green()),
    }
    Ok(())
}

// ── Screenshot (binary protocol) ────────────────────────────────────

/// Take a screenshot and return (width, height, png_path).
/// Reused by `cmd_screenshot` and `--screenshot` flags on set-prop commands.
pub(crate) fn take_screenshot(output: Option<&str>) -> Result<(u64, u64, String)> {
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(1);
    let result = daemon_cmd("debug_request_screenshot", serde_json::json!({"id": id}))
        .ok_or_else(|| miette!("Screenshot failed — is a game running?"))?;

    let width = result["width"].as_u64().unwrap_or(0);
    let height = result["height"].as_u64().unwrap_or(0);
    let png_path = result["path"]
        .as_str()
        .ok_or_else(|| miette!("No screenshot path in response"))?;

    // If --output was given, copy the PNG there; otherwise return temp path as-is
    let final_path = if let Some(dest) = output {
        std::fs::copy(png_path, dest)
            .map_err(|e| miette!("Failed to copy screenshot to {dest}: {e}"))?;
        let _ = std::fs::remove_file(png_path);
        dest.to_string()
    } else {
        png_path.to_string()
    };

    Ok((width, height, final_path))
}

pub(crate) fn cmd_screenshot(args: &ScreenshotArgs) -> Result<()> {
    ensure_binary_debug()?;
    let (width, height, path) = take_screenshot(args.output.as_deref())?;
    let size = Path::new(&path).metadata().map(|m| m.len()).unwrap_or(0);

    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "path": path,
                    "width": width,
                    "height": height,
                    "format": "png",
                    "size": size,
                }))
                .unwrap()
            );
        }
        OutputFormat::Text => {
            let size_kb = size / 1024;
            println!(
                "{} {}x{} ({size_kb} KB) → {}",
                "Screenshot saved".green(),
                width,
                height,
                path.cyan(),
            );
        }
    }
    Ok(())
}
