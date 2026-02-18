use std::path::Path;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use super::gdscript;
use super::{BatchArgs, OutputFormat, run_eval};

pub fn cmd_batch(args: &BatchArgs) -> Result<()> {
    let path = Path::new(&args.file);
    if !path.exists() {
        return Err(miette!("Batch file not found: {}", args.file));
    }
    let content =
        std::fs::read_to_string(path).map_err(|e| miette!("Failed to read batch file: {e}"))?;
    let commands: Vec<serde_json::Value> = serde_json::from_str(&content)
        .map_err(|e| miette!("Failed to parse batch JSON: {e}"))?;

    let mut results = Vec::new();
    for (i, cmd) in commands.iter().enumerate() {
        let cmd_type = cmd["command"]
            .as_str()
            .ok_or_else(|| miette!("Command {i}: missing 'command' field"))?;
        let result = execute_batch_command(cmd_type, cmd, i)?;
        results.push(result);
    }

    match args.format {
        OutputFormat::Json => {
            let output = serde_json::json!({
                "commands_run": results.len(),
                "results": results,
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        OutputFormat::Text => {
            println!(
                "Batch complete: {} commands executed",
                results.len().to_string().green()
            );
            for (i, r) in results.iter().enumerate() {
                let cmd = r["command"].as_str().unwrap_or("?");
                let ok = r["ok"].as_bool().unwrap_or(false);
                let status = if ok {
                    "ok".green().to_string()
                } else {
                    "FAILED".red().to_string()
                };
                println!("  {}: {} — {status}", (i + 1).to_string().dimmed(), cmd);
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn execute_batch_command(
    cmd_type: &str,
    cmd: &serde_json::Value,
    index: usize,
) -> Result<serde_json::Value> {
    let script = match cmd_type {
        "profile" => {
            let plane = cmd["plane"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: profile needs 'plane'"))?;
            let points = cmd["points"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: profile needs 'points'"))?;
            let pts = super::parse_points(points)?;
            gdscript::generate_profile(&pts, plane)
        }
        "extrude" => {
            let depth = cmd["depth"]
                .as_f64()
                .ok_or_else(|| miette!("Command {index}: extrude needs 'depth'"))?;
            let segments = cmd["segments"].as_u64().unwrap_or(1);
            #[allow(clippy::cast_possible_truncation)]
            gdscript::generate_extrude(depth, segments as u32)
        }
        "taper" => {
            let axis = cmd["axis"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: taper needs 'axis'"))?;
            let start = cmd["start"].as_f64().unwrap_or(1.0);
            let end = cmd["end"]
                .as_f64()
                .ok_or_else(|| miette!("Command {index}: taper needs 'end'"))?;
            let midpoint = cmd["midpoint"].as_f64();
            let range = match (cmd["from"].as_f64(), cmd["to"].as_f64()) {
                (Some(f), Some(t)) => Some((f, t)),
                _ => None,
            };
            gdscript::generate_taper(None, axis, start, end, midpoint, range)
        }
        "bevel" => {
            let radius = cmd["radius"]
                .as_f64()
                .ok_or_else(|| miette!("Command {index}: bevel needs 'radius'"))?;
            let segments = cmd["segments"].as_u64().unwrap_or(2);
            let edges = cmd["edges"].as_str().unwrap_or("all");
            #[allow(clippy::cast_possible_truncation)]
            gdscript::generate_bevel(radius, segments as u32, edges)
        }
        "subdivide" => {
            let iterations = cmd["iterations"].as_u64().unwrap_or(1);
            let part = cmd["part"].as_str();
            #[allow(clippy::cast_possible_truncation)]
            gdscript::generate_subdivide(part, iterations as u32)
        }
        "material" => {
            let color = cmd["color"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: material needs 'color'"))?;
            let part = cmd["part"].as_str();
            gdscript::generate_material(part, color)
        }
        "fix-normals" => gdscript::generate_fix_normals(cmd["part"].as_str()),
        "flip-normals" => {
            gdscript::generate_flip_normals(cmd["part"].as_str(), cmd["caps"].as_str())
        }
        "translate" => {
            let to = cmd["to"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: translate needs 'to'"))?;
            let (x, y, z) = super::parse_3d(to)?;
            let relative = cmd["relative"].as_bool().unwrap_or(false);
            gdscript::generate_translate(cmd["part"].as_str(), x, y, z, relative)
        }
        "rotate" => {
            let degrees = cmd["degrees"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: rotate needs 'degrees'"))?;
            let (rx, ry, rz) = super::parse_3d(degrees)?;
            gdscript::generate_rotate(cmd["part"].as_str(), rx, ry, rz)
        }
        "scale" => {
            let factor = cmd["factor"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: scale needs 'factor'"))?;
            let (sx, sy, sz) = super::parse_scale(factor)?;
            gdscript::generate_scale(cmd["part"].as_str(), sx, sy, sz, false)
        }
        "loop-cut" => {
            let axis = cmd["axis"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: loop-cut needs 'axis'"))?;
            let at = cmd["at"]
                .as_f64()
                .ok_or_else(|| miette!("Command {index}: loop-cut needs 'at'"))?;
            gdscript::generate_loop_cut(cmd["part"].as_str(), axis, at)
        }
        "checkpoint" => gdscript::generate_checkpoint(cmd["name"].as_str()),
        "restore" => gdscript::generate_restore(cmd["name"].as_str()),
        other => return Err(miette!("Command {index}: unknown batch command '{other}'")),
    };

    match run_eval(&script) {
        Ok(result) => {
            let parsed: serde_json::Value = serde_json::from_str(&result).unwrap_or_else(|_| {
                serde_json::json!({ "raw": result })
            });
            Ok(serde_json::json!({
                "command": cmd_type,
                "ok": true,
                "result": parsed,
            }))
        }
        Err(e) => Ok(serde_json::json!({
            "command": cmd_type,
            "ok": false,
            "error": e.to_string(),
        })),
    }
}
