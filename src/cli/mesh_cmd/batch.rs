use std::path::Path;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use crate::core::mesh::{MeshState, PlaneKind, ShadingMode, normals};

use crate::cprintln;

use super::gdscript;
use super::{BatchArgs, OutputFormat, project_root, run_eval};

pub fn cmd_batch(args: &BatchArgs) -> Result<()> {
    let path = Path::new(&args.file);
    if !path.exists() {
        return Err(miette!("Batch file not found: {}", args.file));
    }
    let content =
        std::fs::read_to_string(path).map_err(|e| miette!("Failed to read batch file: {e}"))?;
    let commands: Vec<serde_json::Value> =
        serde_json::from_str(&content).map_err(|e| miette!("Failed to parse batch JSON: {e}"))?;

    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let mut results = Vec::new();
    for (i, cmd) in commands.iter().enumerate() {
        let cmd_type = cmd["command"]
            .as_str()
            .ok_or_else(|| miette!("Command {i}: missing 'command' field"))?;
        let result = execute_batch_command(cmd_type, cmd, i, &mut state, &root)?;
        results.push(result);
    }

    match args.format {
        OutputFormat::Json => {
            let output = serde_json::json!({
                "commands_run": results.len(),
                "results": results,
            });
            cprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        OutputFormat::Text => {
            cprintln!(
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
                cprintln!("  {}: {} — {status}", (i + 1).to_string().dimmed(), cmd);
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
    state: &mut MeshState,
    root: &Path,
) -> Result<serde_json::Value> {
    match cmd_type {
        // ── Rust geometry operations ─────────────────────────────────
        "profile" => {
            let plane_str = cmd["plane"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: profile needs 'plane'"))?;
            let plane = parse_plane(plane_str, index)?;

            let points_2d = if let Some(shape_str) = cmd["shape"].as_str() {
                batch_shape_points(shape_str, cmd, index)?
            } else {
                let points_str = cmd["points"]
                    .as_str()
                    .ok_or_else(|| miette!("Command {index}: profile needs 'points' or 'shape'"))?;
                let pts = super::parse_points(points_str)?;
                pts.iter().map(|&(x, y)| [x, y]).collect()
            };

            let part = state.active_part_mut()?;
            part.profile_points = Some(points_2d.clone());
            part.profile_plane = Some(plane);
            if let Some(mesh) = crate::core::mesh::profile::triangulate_profile(&points_2d, plane) {
                part.mesh = mesh;
            }

            save_push_active(state, root)?;
            Ok(ok_result(
                "profile",
                &serde_json::json!({
                    "plane": plane_str,
                    "point_count": points_2d.len(),
                }),
            ))
        }
        "extrude" => {
            let depth = cmd["depth"]
                .as_f64()
                .ok_or_else(|| miette!("Command {index}: extrude needs 'depth'"))?;
            #[allow(clippy::cast_possible_truncation)]
            let segments = cmd["segments"].as_u64().unwrap_or(1) as u32;

            let (profile, plane) = {
                let part = state.active_part()?;
                let p = part
                    .profile_points
                    .as_ref()
                    .ok_or_else(|| miette!("Command {index}: no profile set"))?
                    .clone();
                let k = part
                    .profile_plane
                    .ok_or_else(|| miette!("Command {index}: no profile plane"))?;
                (p, k)
            };

            let inset = match cmd["cap_inset"].as_f64() {
                Some(v) => v,
                None if profile.len() >= 8 => 0.15,
                None => 0.0,
            };
            let mesh = crate::core::mesh::extrude::extrude_with_inset(
                &profile, plane, depth, segments, inset,
            )
            .ok_or_else(|| miette!("Command {index}: extrude failed"))?;
            let vc = mesh.vertex_count();
            let fc = mesh.face_count();
            state.active_part_mut()?.mesh = mesh;

            save_push_active(state, root)?;
            Ok(ok_result(
                "extrude",
                &serde_json::json!({
                    "vertex_count": vc,
                    "face_count": fc,
                }),
            ))
        }
        "taper" => {
            let axis_str = cmd["axis"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: taper needs 'axis'"))?;
            let axis_idx = parse_axis(axis_str, index)?;
            let start = cmd["from_scale"]
                .as_f64()
                .or_else(|| cmd["start"].as_f64())
                .unwrap_or(1.0);
            let end = cmd["to_scale"]
                .as_f64()
                .or_else(|| cmd["end"].as_f64())
                .ok_or_else(|| miette!("Command {index}: taper needs 'to_scale'"))?;
            let midpoint = cmd["midpoint"].as_f64();
            let range = match (cmd["from"].as_f64(), cmd["to"].as_f64()) {
                (Some(f), Some(t)) => Some((f, t)),
                _ => None,
            };

            let part = state.active_part_mut()?;
            let count = crate::core::mesh::taper::taper(
                &mut part.mesh,
                axis_idx,
                start,
                end,
                midpoint,
                range,
            );

            save_push_active(state, root)?;
            Ok(ok_result(
                "taper",
                &serde_json::json!({
                    "axis": axis_str,
                    "vertices_modified": count,
                }),
            ))
        }
        "bevel" => {
            let radius = cmd["radius"]
                .as_f64()
                .ok_or_else(|| miette!("Command {index}: bevel needs 'radius'"))?;
            #[allow(clippy::cast_possible_truncation)]
            let segments = cmd["segments"].as_u64().unwrap_or(2) as u32;
            let edges = cmd["edges"].as_str().unwrap_or("all");
            let profile = cmd["profile"].as_f64().unwrap_or(0.5);

            let where_expr = cmd["where"].as_str();
            let spatial = where_expr
                .map(crate::core::mesh::spatial_filter::parse_where)
                .transpose()?;

            let part = state.active_part_mut()?;
            let beveled = crate::core::mesh::bevel::bevel_with_profile(
                &part.mesh,
                radius,
                segments,
                edges,
                profile,
                spatial.as_ref(),
            );
            let vc = beveled.vertex_count();
            let fc = beveled.face_count();
            part.mesh = beveled;

            save_push_active(state, root)?;
            Ok(ok_result(
                "bevel",
                &serde_json::json!({
                    "vertex_count": vc,
                    "face_count": fc,
                }),
            ))
        }
        "subdivide" => {
            #[allow(clippy::cast_possible_truncation)]
            let iterations = cmd["iterations"].as_u64().unwrap_or(1) as u32;
            let part_name = cmd["part"]
                .as_str()
                .map_or_else(|| state.active.clone(), String::from);

            let part = state.resolve_part_mut(cmd["part"].as_str())?;
            let result_mesh = crate::core::mesh::subdivide::subdivide(&part.mesh, iterations);
            let vc = result_mesh.vertex_count();
            let fc = result_mesh.face_count();
            part.mesh = result_mesh;

            state.save(root)?;
            let push = state.generate_push_script(&part_name)?;
            let _ = run_eval(&push)?;

            Ok(ok_result(
                "subdivide",
                &serde_json::json!({
                    "vertex_count": vc,
                    "face_count": fc,
                }),
            ))
        }
        "loop-cut" => {
            let axis_str = cmd["axis"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: loop-cut needs 'axis'"))?;
            let axis_idx = parse_axis(axis_str, index)?;
            let at = cmd["at"]
                .as_f64()
                .ok_or_else(|| miette!("Command {index}: loop-cut needs 'at'"))?;
            let part_name = cmd["part"]
                .as_str()
                .map_or_else(|| state.active.clone(), String::from);

            let part = state.resolve_part_mut(cmd["part"].as_str())?;
            let (result_mesh, splits) =
                crate::core::mesh::loop_cut::loop_cut(&part.mesh, axis_idx, at);
            let vc = result_mesh.vertex_count();
            part.mesh = result_mesh;

            state.save(root)?;
            let push = state.generate_push_script(&part_name)?;
            let _ = run_eval(&push)?;

            Ok(ok_result(
                "loop-cut",
                &serde_json::json!({
                    "axis": axis_str,
                    "at": at,
                    "triangles_split": splits,
                    "vertex_count": vc,
                }),
            ))
        }
        "fix-normals" => {
            let part_name = cmd["part"]
                .as_str()
                .map_or_else(|| state.active.clone(), String::from);
            let part = state.resolve_part_mut(cmd["part"].as_str())?;
            let total = part.mesh.face_count();
            let flipped = normals::fix_winding(&mut part.mesh);

            state.save(root)?;
            let push = state.generate_push_script(&part_name)?;
            let _ = run_eval(&push)?;

            Ok(ok_result(
                "fix-normals",
                &serde_json::json!({
                    "faces_flipped": flipped,
                    "total_faces": total,
                }),
            ))
        }
        "subtract" | "boolean" | "union" | "intersect" => {
            let tool = cmd["tool"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: boolean needs 'tool'"))?;
            let offset = if let Some(offset_str) = cmd["offset"].as_str() {
                let (x, y, z) = super::parse_3d(offset_str)?;
                [x, y, z]
            } else {
                [0.0; 3]
            };
            let mode_str = cmd["mode"].as_str().unwrap_or(match cmd_type {
                "union" => "union",
                "intersect" => "intersect",
                _ => "subtract",
            });
            let mode = match mode_str {
                "union" => crate::core::mesh::boolean::BooleanMode::Union,
                "intersect" => crate::core::mesh::boolean::BooleanMode::Intersect,
                _ => crate::core::mesh::boolean::BooleanMode::Subtract,
            };

            #[allow(clippy::cast_possible_truncation)]
            let count = cmd["count"].as_u64().unwrap_or(1).max(1) as u32;
            let spacing = if let Some(s) = cmd["spacing"].as_str() {
                let (x, y, z) = super::parse_3d(s)?;
                [x, y, z]
            } else {
                offset
            };

            let tool_mesh = state.resolve_part(Some(tool))?.mesh.clone();
            let target_mesh = state.active_part()?.mesh.clone();
            let mut current = target_mesh;
            for k in 0..count {
                let iter_offset = [
                    offset[0] + spacing[0] * k as f64,
                    offset[1] + spacing[1] * k as f64,
                    offset[2] + spacing[2] * k as f64,
                ];
                current = crate::core::mesh::boolean::boolean_op(
                    &current,
                    &tool_mesh,
                    iter_offset,
                    mode,
                );
            }
            let vc = current.vertex_count();
            let fc = current.face_count();
            state.active_part_mut()?.mesh = current;

            save_push_active(state, root)?;
            Ok(ok_result(
                cmd_type,
                &serde_json::json!({
                    "mode": mode_str,
                    "tool": tool,
                    "vertex_count": vc,
                    "face_count": fc,
                }),
            ))
        }
        "extrude-face" => {
            let depth = cmd["depth"]
                .as_f64()
                .ok_or_else(|| miette!("Command {index}: extrude-face needs 'depth'"))?;
            let where_str = cmd["where"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: extrude-face needs 'where'"))?;
            let sf = crate::core::mesh::spatial_filter::parse_where(where_str)?;
            let part = state.active_part_mut()?;
            let selected: Vec<usize> = (0..part.mesh.faces.len())
                .filter(|&fi| {
                    crate::core::mesh::spatial_filter::face_matches(&part.mesh, fi, &sf)
                })
                .collect();
            let result =
                crate::core::mesh::extrude_face::extrude_faces(&part.mesh, depth, &selected);
            let vc = result.vertex_count();
            let fc = result.face_count();
            part.mesh = result;

            save_push_active(state, root)?;
            Ok(ok_result(
                "extrude-face",
                &serde_json::json!({
                    "depth": depth,
                    "faces_selected": selected.len(),
                    "vertex_count": vc,
                    "face_count": fc,
                }),
            ))
        }
        "inset" => {
            let factor = cmd["factor"].as_f64().unwrap_or(0.2);
            let where_expr = cmd["where"].as_str();
            let spatial = where_expr
                .map(crate::core::mesh::spatial_filter::parse_where)
                .transpose()?;
            let part = state.active_part_mut()?;
            let result = if let Some(ref sf) = spatial {
                let selected: Vec<usize> = (0..part.mesh.faces.len())
                    .filter(|&fi| {
                        crate::core::mesh::spatial_filter::face_matches(&part.mesh, fi, sf)
                    })
                    .collect();
                crate::core::mesh::inset::inset_selected(&part.mesh, factor, Some(&selected))
            } else {
                crate::core::mesh::inset::inset(&part.mesh, factor)
            };
            let vc = result.vertex_count();
            let fc = result.face_count();
            part.mesh = result;

            save_push_active(state, root)?;
            Ok(ok_result(
                "inset",
                &serde_json::json!({
                    "factor": factor,
                    "vertex_count": vc,
                    "face_count": fc,
                }),
            ))
        }
        "solidify" => {
            let thickness = cmd["thickness"]
                .as_f64()
                .ok_or_else(|| miette!("Command {index}: solidify needs 'thickness'"))?;
            let part = state.active_part_mut()?;
            let result = crate::core::mesh::solidify::solidify(&part.mesh, thickness);
            let vc = result.vertex_count();
            let fc = result.face_count();
            part.mesh = result;

            save_push_active(state, root)?;
            Ok(ok_result(
                "solidify",
                &serde_json::json!({
                    "thickness": thickness,
                    "vertex_count": vc,
                    "face_count": fc,
                }),
            ))
        }
        "merge-verts" => {
            let distance = cmd["distance"].as_f64().unwrap_or(0.001);
            let part = state.active_part_mut()?;
            let (result, merged) =
                crate::core::mesh::merge::merge_by_distance(&part.mesh, distance);
            let vc = result.vertex_count();
            let fc = result.face_count();
            part.mesh = result;

            save_push_active(state, root)?;
            Ok(ok_result(
                "merge-verts",
                &serde_json::json!({
                    "distance": distance,
                    "merged": merged,
                    "vertex_count": vc,
                    "face_count": fc,
                }),
            ))
        }
        "array" => {
            #[allow(clippy::cast_possible_truncation)]
            let count = cmd["count"]
                .as_u64()
                .ok_or_else(|| miette!("Command {index}: array needs 'count'"))?
                as usize;
            let offset_str = cmd["offset"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: array needs 'offset'"))?;
            let (x, y, z) = super::parse_3d(offset_str)?;
            let part = state.active_part_mut()?;
            let result = crate::core::mesh::array::array(&part.mesh, count, [x, y, z]);
            let vc = result.vertex_count();
            let fc = result.face_count();
            part.mesh = result;

            save_push_active(state, root)?;
            Ok(ok_result(
                "array",
                &serde_json::json!({
                    "count": count,
                    "offset": [x, y, z],
                    "vertex_count": vc,
                    "face_count": fc,
                }),
            ))
        }
        "flip-normals" => {
            let part_name = cmd["part"]
                .as_str()
                .map_or_else(|| state.active.clone(), String::from);
            let part = state.resolve_part_mut(cmd["part"].as_str())?;
            let fc = part.mesh.face_count();

            let flipped = if let Some(caps_axis) = cmd["caps"].as_str() {
                let axis_idx = parse_axis(caps_axis, index)?;
                normals::flip_caps(&mut part.mesh, axis_idx)
            } else {
                normals::flip_all(&mut part.mesh);
                fc
            };

            state.save(root)?;
            let push = state.generate_push_script(&part_name)?;
            let _ = run_eval(&push)?;

            Ok(ok_result(
                "flip-normals",
                &serde_json::json!({
                    "flipped_faces": flipped,
                    "face_count": fc,
                }),
            ))
        }
        "shade-smooth" | "shade-flat" | "auto-smooth" => {
            let mode = match cmd_type {
                "shade-smooth" => ShadingMode::Smooth,
                "shade-flat" => ShadingMode::Flat,
                _ => ShadingMode::AutoSmooth(cmd["angle"].as_f64().unwrap_or(30.0)),
            };
            let part_name = cmd["part"].as_str().unwrap_or(&state.active).to_string();
            let p = state
                .parts
                .get_mut(&part_name)
                .ok_or_else(|| miette!("Command {index}: part '{part_name}' not found"))?;
            p.shading = mode;
            state.save(root)?;

            let push = state.generate_push_script(&part_name)?;
            let _ = run_eval(&push)?;

            Ok(ok_result(
                cmd_type,
                &serde_json::json!({
                    "part": part_name,
                }),
            ))
        }
        "checkpoint" => {
            let label = cmd["name"].as_str().unwrap_or("default").to_string();
            let parts_saved = state.parts.len();
            state.checkpoints.insert(label.clone(), state.parts.clone());
            state.save(root)?;

            Ok(ok_result(
                "checkpoint",
                &serde_json::json!({
                    "parts_saved": parts_saved,
                    "name": label,
                }),
            ))
        }
        "restore" => {
            let label = cmd["name"].as_str().unwrap_or("default").to_string();
            let saved = state
                .checkpoints
                .get(&label)
                .ok_or_else(|| miette!("Command {index}: checkpoint '{label}' not found"))?
                .clone();
            let parts_restored = saved.len();
            state.parts = saved;

            if !state.parts.contains_key(&state.active)
                && let Some(first) = state.parts.keys().next()
            {
                state.active = first.clone();
            }

            state.save(root)?;
            let names: Vec<String> = state.parts.keys().cloned().collect();
            for name in &names {
                let push = state.generate_push_script(name)?;
                let _ = run_eval(&push)?;
            }

            Ok(ok_result(
                "restore",
                &serde_json::json!({
                    "parts_restored": parts_restored,
                    "name": label,
                }),
            ))
        }

        // ── GDScript eval operations (non-geometry) ──────────────────
        "material" => {
            let color = cmd["color"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: material needs 'color'"))?;
            let part = cmd["part"].as_str();
            Ok(eval_and_wrap(
                "material",
                &gdscript::generate_material(part, color),
            ))
        }
        "translate" => {
            let to = cmd["to"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: translate needs 'to'"))?;
            let (x, y, z) = super::parse_3d(to)?;
            let relative = cmd["relative"].as_bool().unwrap_or(false);
            Ok(eval_and_wrap(
                "translate",
                &gdscript::generate_translate(cmd["part"].as_str(), x, y, z, relative),
            ))
        }
        "rotate" => {
            let degrees = cmd["degrees"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: rotate needs 'degrees'"))?;
            let (rx, ry, rz) = super::parse_3d(degrees)?;
            Ok(eval_and_wrap(
                "rotate",
                &gdscript::generate_rotate(cmd["part"].as_str(), rx, ry, rz),
            ))
        }
        "scale" => {
            let factor = cmd["factor"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: scale needs 'factor'"))?;
            let (sx, sy, sz) = super::parse_scale(factor)?;
            Ok(eval_and_wrap(
                "scale",
                &gdscript::generate_scale(cmd["part"].as_str(), sx, sy, sz, false),
            ))
        }

        other => Err(miette!("Command {index}: unknown batch command '{other}'")),
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

fn parse_plane(s: &str, index: usize) -> Result<PlaneKind> {
    match s {
        "front" => Ok(PlaneKind::Front),
        "side" => Ok(PlaneKind::Side),
        "top" => Ok(PlaneKind::Top),
        _ => Err(miette!("Command {index}: invalid plane '{s}'")),
    }
}

/// Generate 2D shape points from batch JSON `{"shape":"circle","radius":0.5,"segments":16}`.
fn batch_shape_points(shape: &str, cmd: &serde_json::Value, index: usize) -> Result<Vec<[f64; 2]>> {
    use std::f64::consts::TAU;

    match shape {
        "circle" => {
            let radius = cmd["radius"]
                .as_f64()
                .ok_or_else(|| miette!("Command {index}: circle needs 'radius'"))?;
            #[allow(clippy::cast_possible_truncation)]
            let segments = cmd["segments"].as_u64().unwrap_or(16) as u32;
            let segments = segments.max(3);
            Ok((0..segments)
                .map(|i| {
                    let angle = TAU * f64::from(i) / f64::from(segments);
                    [radius * angle.cos(), radius * angle.sin()]
                })
                .collect())
        }
        "ellipse" => {
            let rx = cmd["radius_x"]
                .as_f64()
                .or_else(|| cmd["radius"].as_f64())
                .ok_or_else(|| miette!("Command {index}: ellipse needs 'radius_x' or 'radius'"))?;
            let ry = cmd["radius_y"]
                .as_f64()
                .or_else(|| cmd["radius"].as_f64())
                .ok_or_else(|| miette!("Command {index}: ellipse needs 'radius_y' or 'radius'"))?;
            #[allow(clippy::cast_possible_truncation)]
            let segments = cmd["segments"].as_u64().unwrap_or(16) as u32;
            let segments = segments.max(3);
            Ok((0..segments)
                .map(|i| {
                    let angle = TAU * f64::from(i) / f64::from(segments);
                    [rx * angle.cos(), ry * angle.sin()]
                })
                .collect())
        }
        "rectangle" => {
            let width = cmd["width"]
                .as_f64()
                .ok_or_else(|| miette!("Command {index}: rectangle needs 'width'"))?;
            let height = cmd["height"]
                .as_f64()
                .ok_or_else(|| miette!("Command {index}: rectangle needs 'height'"))?;
            let hw = width / 2.0;
            let hh = height / 2.0;
            Ok(vec![[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]])
        }
        _ => Err(miette!(
            "Command {index}: unknown shape '{shape}' (circle, ellipse, rectangle)"
        )),
    }
}

fn parse_axis(s: &str, index: usize) -> Result<usize> {
    match s {
        "x" => Ok(0),
        "y" => Ok(1),
        "z" => Ok(2),
        _ => Err(miette!("Command {index}: invalid axis '{s}'")),
    }
}

/// Save state and push the active part to Godot.
fn save_push_active(state: &mut MeshState, root: &Path) -> Result<()> {
    let active = state.active.clone();
    state.save(root)?;
    let push = state.generate_push_script(&active)?;
    let _ = run_eval(&push)?;
    Ok(())
}

fn ok_result(cmd: &str, data: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "command": cmd,
        "ok": true,
        "result": data,
    })
}

fn eval_and_wrap(cmd_type: &str, script: &str) -> serde_json::Value {
    match run_eval(script) {
        Ok(result) => {
            let parsed: serde_json::Value = serde_json::from_str(&result)
                .unwrap_or_else(|_| serde_json::json!({ "raw": result }));
            serde_json::json!({
                "command": cmd_type,
                "ok": true,
                "result": parsed,
            })
        }
        Err(e) => serde_json::json!({
            "command": cmd_type,
            "ok": false,
            "error": e.to_string(),
        }),
    }
}
