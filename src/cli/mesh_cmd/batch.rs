use std::path::Path;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use crate::core::mesh::half_edge::HalfEdgeMesh;
use crate::core::mesh::{
    MeshPart, MeshState, PlaneKind, ShadingMode, Transform3D, normals, spatial,
};

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
        let result = execute_with_spatial_checks(cmd_type, cmd, i, &mut state, &root)?;
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

#[expect(clippy::too_many_lines)]
pub fn execute_batch_command(
    cmd_type: &str,
    cmd: &serde_json::Value,
    index: usize,
    state: &mut MeshState,
    root: &Path,
) -> Result<serde_json::Value> {
    match cmd_type {
        // ── Rust geometry operations ─────────────────────────────────
        "profile" => {
            // Handle copy_profile_from: reuse another part's profile
            if let Some(source) = cmd["copy_profile_from"].as_str() {
                let src = state
                    .parts
                    .get(source)
                    .ok_or_else(|| miette!("Command {index}: part '{source}' not found"))?;
                let points = src
                    .profile_points
                    .clone()
                    .ok_or_else(|| miette!("Command {index}: part '{source}' has no profile"))?;
                let plane = src.profile_plane.ok_or_else(|| {
                    miette!("Command {index}: part '{source}' has no profile plane")
                })?;
                let holes = src.profile_holes.clone();
                let pc = points.len();

                let part = state.active_part_mut()?;
                part.profile_points = Some(points.clone());
                part.profile_plane = Some(plane);
                part.profile_holes = holes;
                if let Some(mesh) = crate::core::mesh::profile::triangulate_profile(&points, plane)
                {
                    part.mesh = mesh;
                }

                save_push_active(state, root)?;
                return Ok(ok_result(
                    "profile",
                    &serde_json::json!({"copy_profile_from": source, "point_count": pc}),
                ));
            }

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

            // Parse hole contours
            let holes: Option<Vec<Vec<[f64; 2]>>> = cmd["hole"].as_array().map(|arr| {
                arr.iter()
                    .filter_map(|h| {
                        h.as_str().and_then(|s| {
                            super::parse_points(s)
                                .ok()
                                .map(|pts| pts.iter().map(|&(x, y)| [x, y]).collect())
                        })
                    })
                    .collect()
            });

            let part = state.active_part_mut()?;
            part.profile_points = Some(points_2d.clone());
            part.profile_plane = Some(plane);
            part.profile_holes = holes;
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

            // Transform both meshes to world space so the boolean sees correct geometry
            let tool_part = state.resolve_part(Some(tool))?;
            let tool_transform = tool_part.transform.clone();
            let tool_mesh = tool_part.mesh.clone();
            let tool_world = transform_mesh(&tool_mesh, &tool_transform);

            let target_part = state.active_part()?;
            let target_transform = target_part.transform.clone();
            let target_mesh = target_part.mesh.clone();
            let mut current = transform_mesh(&target_mesh, &target_transform);

            for k in 0..count {
                let iter_offset = [
                    offset[0] + spacing[0] * k as f64,
                    offset[1] + spacing[1] * k as f64,
                    offset[2] + spacing[2] * k as f64,
                ];
                current = crate::core::mesh::boolean::boolean_op(
                    &current,
                    &tool_world,
                    iter_offset,
                    mode,
                );
            }

            // Transform result back to target's local coordinate space
            let result_local = inverse_transform_mesh(&current, &target_transform);
            let vc = result_local.vertex_count();
            let fc = result_local.face_count();
            state.active_part_mut()?.mesh = result_local;

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
                .filter(|&fi| crate::core::mesh::spatial_filter::face_matches(&part.mesh, fi, &sf))
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
            let preset = cmd["preset"].as_str();
            let color = cmd["color"].as_str();
            let part = cmd["part"].as_str();

            // Handle --group → expand to parts pattern
            if let Some(group_name) = cmd["group"].as_str() {
                let members = state
                    .groups
                    .get(group_name)
                    .ok_or_else(|| miette!("Command {index}: group '{group_name}' not found"))?
                    .clone();
                let pattern = members.join(",");
                let script = if let Some(p) = preset {
                    gdscript::generate_material_preset_multi(&pattern, p, color)
                } else if let Some(c) = color {
                    gdscript::generate_material_multi(&pattern, c)
                } else {
                    return Err(miette!(
                        "Command {index}: material needs 'color' or 'preset'"
                    ));
                };
                return Ok(eval_and_wrap("material", &script));
            }

            // Handle --parts pattern
            if let Some(pattern) = cmd["parts"].as_str() {
                let script = if let Some(p) = preset {
                    gdscript::generate_material_preset_multi(pattern, p, color)
                } else if let Some(c) = color {
                    gdscript::generate_material_multi(pattern, c)
                } else {
                    return Err(miette!(
                        "Command {index}: material needs 'color' or 'preset'"
                    ));
                };
                return Ok(eval_and_wrap("material", &script));
            }

            // Single part
            let script = if let Some(p) = preset {
                gdscript::generate_material_preset(part, p, color)
            } else if let Some(c) = color {
                gdscript::generate_material(part, c)
            } else {
                return Err(miette!(
                    "Command {index}: material needs 'color' or 'preset'"
                ));
            };
            Ok(eval_and_wrap("material", &script))
        }
        "translate" => {
            let to = cmd["to"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: translate needs 'to'"))?;
            let (x, y, z) = super::parse_3d(to)?;
            let relative = cmd["relative"].as_bool().unwrap_or(false);

            if let Some(group_name) = cmd["group"].as_str() {
                let members = state
                    .groups
                    .get(group_name)
                    .ok_or_else(|| miette!("Command {index}: group '{group_name}' not found"))?
                    .clone();
                for name in &members {
                    let part = state
                        .parts
                        .get_mut(name)
                        .ok_or_else(|| miette!("Part '{name}' not found"))?;
                    bake_translate(part, x, y, z, relative);
                }
                state.save(root)?;
                for name in &members {
                    let push = state.generate_push_script(name)?;
                    let _ = run_eval(&push)?;
                }
                Ok(ok_result(
                    "translate",
                    &serde_json::json!({"group": group_name, "count": members.len()}),
                ))
            } else {
                let part_name = cmd["part"].as_str().unwrap_or(&state.active).to_string();
                let part = state.resolve_part_mut(Some(&part_name))?;
                bake_translate(part, x, y, z, relative);
                state.save(root)?;
                let push = state.generate_push_script(&part_name)?;
                let _ = run_eval(&push)?;
                Ok(ok_result(
                    "translate",
                    &serde_json::json!({"name": part_name, "position": [x, y, z]}),
                ))
            }
        }
        "rotate" => {
            let degrees = cmd["degrees"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: rotate needs 'degrees'"))?;
            let (rx, ry, rz) = super::parse_3d(degrees)?;
            let transform = Transform3D {
                rotation: [rx, ry, rz],
                ..Transform3D::default()
            };

            if let Some(group_name) = cmd["group"].as_str() {
                let members = state
                    .groups
                    .get(group_name)
                    .ok_or_else(|| miette!("Command {index}: group '{group_name}' not found"))?
                    .clone();
                for name in &members {
                    let part = state
                        .parts
                        .get_mut(name)
                        .ok_or_else(|| miette!("Part '{name}' not found"))?;
                    for v in &mut part.mesh.vertices {
                        v.position = transform.apply_point(v.position);
                    }
                    part.transform.rotation = [0.0; 3];
                }
                state.save(root)?;
                for name in &members {
                    let push = state.generate_push_script(name)?;
                    let _ = run_eval(&push)?;
                }
                Ok(ok_result(
                    "rotate",
                    &serde_json::json!({"group": group_name, "count": members.len()}),
                ))
            } else {
                let part_name = cmd["part"].as_str().unwrap_or(&state.active).to_string();
                let part = state.resolve_part_mut(Some(&part_name))?;
                for v in &mut part.mesh.vertices {
                    v.position = transform.apply_point(v.position);
                }
                part.transform.rotation = [0.0; 3];
                state.save(root)?;
                let push = state.generate_push_script(&part_name)?;
                let _ = run_eval(&push)?;
                Ok(ok_result(
                    "rotate",
                    &serde_json::json!({"name": part_name, "rotation": [rx, ry, rz]}),
                ))
            }
        }
        "scale" => {
            let factor = cmd["factor"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: scale needs 'factor'"))?;
            let (sx, sy, sz) = super::parse_scale(factor)?;
            let remap = cmd["remap"].as_bool().unwrap_or(false);
            let transform = Transform3D {
                scale: [sx, sy, sz],
                ..Transform3D::default()
            };

            if let Some(group_name) = cmd["group"].as_str() {
                let members = state
                    .groups
                    .get(group_name)
                    .ok_or_else(|| miette!("Command {index}: group '{group_name}' not found"))?
                    .clone();
                for name in &members {
                    let part = state
                        .parts
                        .get_mut(name)
                        .ok_or_else(|| miette!("Part '{name}' not found"))?;
                    for v in &mut part.mesh.vertices {
                        v.position = transform.apply_point(v.position);
                    }
                    if remap {
                        let (aabb_min, aabb_max) = part.mesh.aabb();
                        let center = [
                            (aabb_min[0] + aabb_max[0]) * 0.5,
                            (aabb_min[1] + aabb_max[1]) * 0.5,
                            (aabb_min[2] + aabb_max[2]) * 0.5,
                        ];
                        for v in &mut part.mesh.vertices {
                            v.position[0] -= center[0];
                            v.position[1] -= center[1];
                            v.position[2] -= center[2];
                        }
                    }
                    part.transform.scale = [1.0; 3];
                }
                state.save(root)?;
                for name in &members {
                    let push = state.generate_push_script(name)?;
                    let _ = run_eval(&push)?;
                }
                Ok(ok_result(
                    "scale",
                    &serde_json::json!({"group": group_name, "count": members.len()}),
                ))
            } else {
                let part_name = cmd["part"].as_str().unwrap_or(&state.active).to_string();
                let part = state.resolve_part_mut(Some(&part_name))?;
                for v in &mut part.mesh.vertices {
                    v.position = transform.apply_point(v.position);
                }
                if remap {
                    let (aabb_min, aabb_max) = part.mesh.aabb();
                    let center = [
                        (aabb_min[0] + aabb_max[0]) * 0.5,
                        (aabb_min[1] + aabb_max[1]) * 0.5,
                        (aabb_min[2] + aabb_max[2]) * 0.5,
                    ];
                    for v in &mut part.mesh.vertices {
                        v.position[0] -= center[0];
                        v.position[1] -= center[1];
                        v.position[2] -= center[2];
                    }
                }
                part.transform.scale = [1.0; 3];
                state.save(root)?;
                let push = state.generate_push_script(&part_name)?;
                let _ = run_eval(&push)?;
                Ok(ok_result(
                    "scale",
                    &serde_json::json!({"name": part_name, "scale": [sx, sy, sz]}),
                ))
            }
        }

        // ── Part lifecycle + session commands (for replay) ───────────
        "create" => {
            let name = cmd["name"].as_str().unwrap_or("body");
            let from = cmd["from"].as_str().unwrap_or("empty");

            *state = MeshState::new(name);
            state.save(root)?;

            let script = gdscript::generate_create(name, from);
            let _result = run_eval(&script)?;

            super::build_primitive_mesh(from, state);
            state.save(root)?;

            if state.active_part().is_ok_and(|p| p.mesh.face_count() > 0) {
                let push = state.generate_push_script(&state.active.clone())?;
                let _ = run_eval(&push)?;
            }

            Ok(ok_result(
                "create",
                &serde_json::json!({"name": name, "from": from}),
            ))
        }
        "add-part" => {
            let name = cmd["name"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: add-part needs 'name'"))?;
            let from = cmd["from"].as_str().unwrap_or("empty");

            state.parts.insert(name.to_string(), MeshPart::new());
            state.active = name.to_string();
            state.save(root)?;

            let script = gdscript::generate_add_part(name, from);
            let result = run_eval(&script)?;
            let _parsed: serde_json::Value =
                serde_json::from_str(&result).unwrap_or_else(|_| serde_json::json!({}));

            super::build_primitive_mesh(from, state);
            state.save(root)?;

            if state.active_part().is_ok_and(|p| p.mesh.face_count() > 0) {
                let push = state.generate_push_script(&state.active.clone())?;
                let _ = run_eval(&push)?;
            }

            Ok(ok_result(
                "add-part",
                &serde_json::json!({"name": name, "from": from}),
            ))
        }
        "remove-part" => {
            if let Some(group_name) = cmd["group"].as_str() {
                let members = state
                    .groups
                    .get(group_name)
                    .ok_or_else(|| miette!("Command {index}: group '{group_name}' not found"))?
                    .clone();
                for member in &members {
                    let script = gdscript::generate_remove_part(member);
                    let _ = run_eval(&script);
                    state.parts.shift_remove(member.as_str());
                }
                state.groups.remove(group_name);
                if members.contains(&state.active) {
                    state.active = state.parts.keys().next().cloned().unwrap_or_default();
                }
                state.save(root)?;
                Ok(ok_result(
                    "remove-part",
                    &serde_json::json!({"group": group_name, "count": members.len()}),
                ))
            } else {
                let name = cmd["name"].as_str().ok_or_else(|| {
                    miette!("Command {index}: remove-part needs 'name' or 'group'")
                })?;
                let script = gdscript::generate_remove_part(name);
                let _ = run_eval(&script);
                state.parts.shift_remove(name);
                if state.active == name {
                    state.active = state.parts.keys().next().cloned().unwrap_or_default();
                }
                state.save(root)?;
                Ok(ok_result("remove-part", &serde_json::json!({"name": name})))
            }
        }
        "duplicate-part" => {
            let src = cmd["name"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: duplicate-part needs 'name'"))?;
            let dst = cmd["as"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: duplicate-part needs 'as'"))?;

            let src_part = state
                .parts
                .get(src)
                .ok_or_else(|| miette!("Command {index}: part '{src}' not found"))?
                .clone();

            let mut new_part = src_part;
            let mirror_axis = cmd["mirror"].as_str().or_else(|| cmd["symmetric"].as_str());

            if let Some(axis_str) = mirror_axis {
                let axis_idx = parse_axis(axis_str, index)?;
                crate::core::mesh::mirror::mirror(&mut new_part.mesh, axis_idx);
                new_part.transform.position[axis_idx] = -new_part.transform.position[axis_idx];
            }

            state.parts.insert(dst.to_string(), new_part);
            state.active = dst.to_string();
            state.save(root)?;

            let symmetric = cmd["symmetric"].as_str().is_some();
            let script = if let Some(axis_str) = mirror_axis {
                gdscript::generate_mirror_part(src, dst, axis_str, symmetric)
            } else {
                gdscript::generate_duplicate_part(src, dst)
            };
            let _ = run_eval(&script);

            let push = state.generate_push_script(dst)?;
            let _ = run_eval(&push);

            Ok(ok_result(
                "duplicate-part",
                &serde_json::json!({"name": src, "as": dst}),
            ))
        }
        "revolve" => {
            let axis_str = cmd["axis"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: revolve needs 'axis'"))?;
            let axis_idx = parse_axis(axis_str, index)?;
            let degrees = cmd["degrees"].as_f64().unwrap_or(360.0);
            #[allow(clippy::cast_possible_truncation)]
            let segments = cmd["segments"].as_u64().unwrap_or(32) as u32;
            let cap = cmd["cap"].as_bool().unwrap_or(false);

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

            let mesh = crate::core::mesh::revolve::revolve(
                &profile, plane, axis_idx, degrees, segments, cap,
            )
            .ok_or_else(|| miette!("Command {index}: revolve failed"))?;
            let vc = mesh.vertex_count();
            let fc = mesh.face_count();
            state.active_part_mut()?.mesh = mesh;

            save_push_active(state, root)?;
            Ok(ok_result(
                "revolve",
                &serde_json::json!({
                    "axis": axis_str,
                    "degrees": degrees,
                    "vertex_count": vc,
                    "face_count": fc,
                }),
            ))
        }
        "move-vertex" => {
            #[allow(clippy::cast_possible_truncation)]
            let idx = cmd["index"]
                .as_u64()
                .ok_or_else(|| miette!("Command {index}: move-vertex needs 'index'"))?
                as usize;
            let delta_str = cmd["delta"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: move-vertex needs 'delta'"))?;
            let (dx, dy, dz) = super::parse_3d(delta_str)?;

            let part = state.active_part_mut()?;
            if idx >= part.mesh.vertices.len() {
                return Err(miette!(
                    "Command {index}: vertex index {idx} out of range (have {})",
                    part.mesh.vertices.len()
                ));
            }
            part.mesh.vertices[idx].position[0] += dx;
            part.mesh.vertices[idx].position[1] += dy;
            part.mesh.vertices[idx].position[2] += dz;

            save_push_active(state, root)?;
            Ok(ok_result(
                "move-vertex",
                &serde_json::json!({"index": idx, "delta": [dx, dy, dz]}),
            ))
        }
        "group" => {
            let name = cmd["name"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: group needs 'name'"))?;
            let parts_str = cmd["parts"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: group needs 'parts'"))?;

            let all_names: Vec<String> = state.parts.keys().cloned().collect();
            let matched = super::match_part_pattern(&all_names, parts_str);
            let members: Vec<String> = matched.iter().map(|s| (*s).to_string()).collect();

            state.groups.insert(name.to_string(), members.clone());
            state.save(root)?;

            Ok(ok_result(
                "group",
                &serde_json::json!({"name": name, "members": members}),
            ))
        }
        "ungroup" => {
            let name = cmd["name"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: ungroup needs 'name'"))?;

            state.groups.remove(name);
            state.save(root)?;

            Ok(ok_result("ungroup", &serde_json::json!({"name": name})))
        }

        "focus" => {
            let name = cmd["part"]
                .as_str()
                .ok_or_else(|| miette!("Command {index}: focus needs 'part'"))?;
            if !state.parts.contains_key(name) {
                return Err(miette!("Command {index}: part '{name}' not found"));
            }
            state.active = name.to_string();
            state.save(root)?;
            Ok(ok_result("focus", &serde_json::json!({"active": name})))
        }

        other => Err(miette!("Command {index}: unknown batch command '{other}'")),
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Apply a `Transform3D` to all vertices in a mesh (scale → rotate → translate).
fn transform_mesh(mesh: &HalfEdgeMesh, t: &Transform3D) -> HalfEdgeMesh {
    if t.is_identity() {
        return mesh.clone();
    }
    let mut result = mesh.clone();
    for v in &mut result.vertices {
        v.position = t.apply_point(v.position);
    }
    result
}

/// Apply inverse transform to all vertices (un-translate → un-rotate → un-scale).
fn inverse_transform_mesh(mesh: &HalfEdgeMesh, t: &Transform3D) -> HalfEdgeMesh {
    if t.is_identity() {
        return mesh.clone();
    }
    let mut result = mesh.clone();
    for v in &mut result.vertices {
        v.position = t.inverse_apply_point(v.position);
    }
    result
}

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

/// Execute a batch command and append spatial relationship errors to the result.
pub fn execute_with_spatial_checks(
    cmd_type: &str,
    cmd: &serde_json::Value,
    index: usize,
    state: &mut MeshState,
    root: &Path,
) -> Result<serde_json::Value> {
    let mut result = execute_batch_command(cmd_type, cmd, index, state, root)?;
    if state.parts.len() > 1 {
        let issues = spatial::check_part_relationships(state);
        if !issues.is_empty() {
            result["errors"] = serde_json::json!(
                issues
                    .iter()
                    .map(spatial::SpatialIssue::to_json)
                    .collect::<Vec<_>>()
            );
        }
    }
    Ok(result)
}

fn ok_result(cmd: &str, data: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "command": cmd,
        "ok": true,
        "result": data,
    })
}

/// Bake translation directly into mesh vertices.
fn bake_translate(part: &mut MeshPart, x: f64, y: f64, z: f64, relative: bool) {
    if relative {
        for v in &mut part.mesh.vertices {
            v.position[0] += x;
            v.position[1] += y;
            v.position[2] += z;
        }
    } else {
        let (amin, amax) = part.mesh.aabb();
        let delta = [
            x - (amin[0] + amax[0]) * 0.5,
            y - (amin[1] + amax[1]) * 0.5,
            z - (amin[2] + amax[2]) * 0.5,
        ];
        for v in &mut part.mesh.vertices {
            v.position[0] += delta[0];
            v.position[1] += delta[1];
            v.position[2] += delta[2];
        }
    }
    part.transform.position = [0.0; 3];
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
