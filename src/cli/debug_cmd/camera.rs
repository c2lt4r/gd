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
        OutputFormat::Human => println!("{}", "2D camera transformed".green()),
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
        OutputFormat::Human => println!("{}", "3D camera transformed".green()),
    }
    Ok(())
}

// ── Screenshot (binary protocol) ────────────────────────────────────

/// Take a screenshot and return (width, height, base64_data).
/// Reused by `cmd_screenshot` and `--screenshot` flags on set-prop commands.
pub(crate) fn take_screenshot_b64() -> Result<(u64, u64, String)> {
    use base64::Engine;

    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(1);
    let result = daemon_cmd("debug_request_screenshot", serde_json::json!({"id": id}))
        .ok_or_else(|| miette!("Screenshot failed — is a game running?"))?;

    let width = result["width"].as_u64().unwrap_or(0);
    let height = result["height"].as_u64().unwrap_or(0);
    let png_b64 = result["data"]
        .as_str()
        .ok_or_else(|| miette!("No screenshot data in response"))?;

    // Convert PNG → JPEG to reduce base64 size (~3-5x smaller)
    let png_bytes = base64::engine::general_purpose::STANDARD
        .decode(png_b64)
        .map_err(|e| miette!("Failed to decode screenshot data: {e}"))?;
    let jpeg_b64 = png_to_jpeg_b64(&png_bytes)?;
    Ok((width, height, jpeg_b64))
}

/// Convert PNG bytes to JPEG, return as base64.
fn png_to_jpeg_b64(png_bytes: &[u8]) -> Result<String> {
    use base64::Engine;

    let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    let mut reader = decoder
        .read_info()
        .map_err(|e| miette!("Failed to decode PNG: {e}"))?;
    let mut buf = vec![0u8; reader.output_buffer_size().unwrap_or(0)];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| miette!("Failed to read PNG frame: {e}"))?;
    let pixels = &buf[..info.buffer_size()];
    let width = info.width as u16;
    let height = info.height as u16;

    // Convert to RGB if RGBA (strip alpha)
    let rgb_data = match info.color_type {
        png::ColorType::Rgba => {
            let mut rgb = Vec::with_capacity(pixels.len() / 4 * 3);
            for chunk in pixels.chunks_exact(4) {
                rgb.extend_from_slice(&chunk[..3]);
            }
            rgb
        }
        png::ColorType::Rgb => pixels.to_vec(),
        other => return Err(miette!("Unsupported PNG color type: {other:?}")),
    };

    let mut jpeg_buf = Vec::new();
    let encoder = jpeg_encoder::Encoder::new(&mut jpeg_buf, 80);
    encoder
        .encode(&rgb_data, width, height, jpeg_encoder::ColorType::Rgb)
        .map_err(|e| miette!("Failed to encode JPEG: {e}"))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&jpeg_buf))
}

/// Print screenshot as base64 JPEG (default) or write to file (PNG).
fn print_screenshot(
    b64_data: &str,
    width: u64,
    height: u64,
    output: Option<&str>,
    format: &OutputFormat,
) -> Result<()> {
    if let Some(output) = output {
        // --output writes JPEG to file (same as base64 output, just decoded to bytes)
        use base64::Engine;
        let img_bytes = base64::engine::general_purpose::STANDARD
            .decode(b64_data)
            .map_err(|e| miette!("Failed to decode screenshot data: {e}"))?;

        let fmt_label = "jpeg";
        let bytes_to_write = img_bytes;

        std::fs::write(output, &bytes_to_write)
            .map_err(|e| miette!("Failed to write screenshot to {output}: {e}"))?;

        match format {
            OutputFormat::Json => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "path": output,
                        "width": width,
                        "height": height,
                        "format": fmt_label,
                        "size": bytes_to_write.len(),
                    }))
                    .unwrap()
                );
            }
            OutputFormat::Human => {
                let size_kb = bytes_to_write.len() / 1024;
                println!(
                    "{} {}x{} ({size_kb} KB) → {}",
                    "Screenshot saved".green(),
                    width,
                    height,
                    output.cyan(),
                );
            }
        }
        return Ok(());
    }

    // Default: output JPEG base64 to stdout
    match format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "width": width,
                    "height": height,
                    "format": "jpeg",
                    "data": b64_data,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            print!("{b64_data}");
        }
    }
    Ok(())
}

pub(crate) fn cmd_screenshot(args: &ScreenshotArgs) -> Result<()> {
    ensure_binary_debug()?;
    let (width, height, b64_data) = take_screenshot_b64()?;
    print_screenshot(
        &b64_data,
        width,
        height,
        args.output.as_deref(),
        &args.format,
    )
}
