use miette::Result;
use owo_colors::OwoColorize;

use crate::core::mesh::MeshState;

use super::{InfoArgs, OutputFormat, project_root};
use crate::cprintln;

pub fn cmd_info(args: &InfoArgs) -> Result<()> {
    let root = project_root()?;
    let state = MeshState::load(&root)?;

    if args.all {
        print_info_all(&state, args);
    } else {
        print_info_single(&state, args)?;
    }
    Ok(())
}

fn print_info_single(state: &MeshState, args: &InfoArgs) -> Result<()> {
    let part = state.active_part()?;
    let name = &state.active;
    let vc = part.mesh.vertex_count();
    let fc = part.mesh.face_count();
    let (aabb_min, aabb_max) = part.mesh.aabb();
    let plane = part.profile_plane.map_or("none", |p| p.as_str());
    let pts = part.profile_points.as_ref().map_or(0, Vec::len);

    let result = serde_json::json!({
        "name": name,
        "vertex_count": vc,
        "face_count": fc,
        "profile_plane": plane,
        "profile_point_count": pts,
        "aabb_position": aabb_min,
        "aabb_end": aabb_max,
    });

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            cprintln!(
                "Mesh: {} (vertices: {vc}, faces: {fc})",
                name.green().bold()
            );
            cprintln!(
                "AABB: ({:.2}, {:.2}, {:.2}) -> ({:.2}, {:.2}, {:.2})",
                aabb_min[0],
                aabb_min[1],
                aabb_min[2],
                aabb_max[0],
                aabb_max[1],
                aabb_max[2],
            );
            if pts > 0 {
                cprintln!("Profile: {plane} ({pts} points)");
            }
        }
    }
    Ok(())
}

fn print_info_all(state: &MeshState, args: &InfoArgs) {
    let mut total_vc = 0;
    let mut total_fc = 0;
    let mut parts_json = Vec::new();

    for (name, part) in &state.parts {
        let vc = part.mesh.vertex_count();
        let fc = part.mesh.face_count();
        total_vc += vc;
        total_fc += fc;
        let (amin, amax) = part.mesh.aabb();
        parts_json.push(serde_json::json!({
            "name": name,
            "vertex_count": vc,
            "face_count": fc,
            "position": part.transform.position,
            "rotation": part.transform.rotation,
            "scale": part.transform.scale,
            "aabb_min": amin,
            "aabb_max": amax,
        }));
    }

    let result = serde_json::json!({
        "active": state.active,
        "part_count": state.parts.len(),
        "total_vertex_count": total_vc,
        "total_face_count": total_fc,
        "parts": parts_json,
    });

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            let count = state.parts.len();
            cprintln!(
                "{count} parts, {total_vc} vertices, {total_fc} faces (active: {})",
                state.active.cyan()
            );
            for p in &parts_json {
                let name = p["name"].as_str().unwrap_or("?");
                let vc = p["vertex_count"].as_u64().unwrap_or(0);
                let fc = p["face_count"].as_u64().unwrap_or(0);
                let marker = if name == state.active { " *" } else { "" };
                cprintln!("  {}{marker}: {vc} vertices, {fc} faces", name.green());
                let fmt_vec = |arr: &serde_json::Value| -> String {
                    if let Some(a) = arr.as_array() {
                        format!(
                            "{:.2}, {:.2}, {:.2}",
                            a[0].as_f64().unwrap_or(0.0),
                            a[1].as_f64().unwrap_or(0.0),
                            a[2].as_f64().unwrap_or(0.0),
                        )
                    } else {
                        "?".to_string()
                    }
                };
                cprintln!(
                    "    pos({})  rot({})  scale({})",
                    fmt_vec(&p["position"]),
                    fmt_vec(&p["rotation"]),
                    fmt_vec(&p["scale"]),
                );
                cprintln!(
                    "    aabb({}) -> ({})",
                    fmt_vec(&p["aabb_min"]),
                    fmt_vec(&p["aabb_max"]),
                );
            }
        }
    }
}
