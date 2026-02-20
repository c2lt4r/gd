use std::f64::consts::TAU;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use crate::core::mesh::{MeshState, PlaneKind};

use super::gdscript;
use super::{OutputFormat, ProfileArgs, ProfileShape, parse_points, project_root, run_eval};
use crate::cprintln;

pub fn cmd_profile(args: &ProfileArgs) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let resolved = resolve_profile(&state, args)?;

    // Parse hole contours
    let holes = parse_holes(&args.hole)?;

    // Apply to active part
    let part = state.active_part_mut()?;
    part.profile_points = Some(resolved.points.clone());
    part.profile_plane = Some(resolved.plane);
    part.profile_holes = if holes.is_empty() {
        None
    } else {
        Some(holes.clone())
    };

    if let Some(mesh) =
        crate::core::mesh::profile::triangulate_profile(&resolved.points, resolved.plane)
    {
        part.mesh = mesh;
    }

    state.save(&root)?;

    // Push to Godot
    let push = state.generate_push_script(&state.active.clone())?;
    let _ = run_eval(&push)?;

    // Store metadata on the Godot node
    let parsed_tuples: Vec<(f64, f64)> = resolved.points.iter().map(|p| (p[0], p[1])).collect();
    let meta_script = gdscript::generate_profile(&parsed_tuples, resolved.plane.as_str());
    let _ = run_eval(&meta_script);

    // Output
    print_result(&resolved, holes.len(), &args.format);
    Ok(())
}

struct ResolvedProfile {
    points: Vec<[f64; 2]>,
    plane: PlaneKind,
    label: String,
}

fn resolve_profile(state: &MeshState, args: &ProfileArgs) -> Result<ResolvedProfile> {
    if let Some(ref src_part) = args.copy_profile_from {
        let src = state.resolve_part(Some(src_part))?.clone();
        let points = src
            .profile_points
            .ok_or_else(|| miette!("Part '{src_part}' has no profile to copy"))?;
        let plane = src
            .profile_plane
            .ok_or_else(|| miette!("Part '{src_part}' has no profile plane"))?;
        Ok(ResolvedProfile {
            label: format!("copied from {src_part}"),
            points,
            plane,
        })
    } else if let Some(ref shape) = args.shape {
        let plane = args
            .plane
            .as_ref()
            .ok_or_else(|| miette!("--plane is required"))?;
        let (points, label) = generate_shape(shape, args)?;
        Ok(ResolvedProfile {
            points,
            plane: plane.to_plane_kind(),
            label,
        })
    } else {
        let points_str = args
            .points
            .as_deref()
            .ok_or_else(|| miette!("--points, --shape, or --copy-profile-from is required"))?;
        let plane = args
            .plane
            .as_ref()
            .ok_or_else(|| miette!("--plane is required"))?;
        let parsed = parse_points(points_str)?;
        let points: Vec<[f64; 2]> = parsed.iter().map(|&(x, y)| [x, y]).collect();
        Ok(ResolvedProfile {
            label: format!("{} points", points.len()),
            points,
            plane: plane.to_plane_kind(),
        })
    }
}

/// Parse `--hole` flag values into hole contours.
fn parse_holes(hole_args: &[String]) -> Result<Vec<Vec<[f64; 2]>>> {
    let mut holes = Vec::new();
    for h in hole_args {
        let parsed = parse_points(h)?;
        let points: Vec<[f64; 2]> = parsed.iter().map(|&(x, y)| [x, y]).collect();
        holes.push(points);
    }
    Ok(holes)
}

fn print_result(resolved: &ResolvedProfile, hole_count: usize, format: &OutputFormat) {
    let mut result = serde_json::json!({
        "plane": resolved.plane.as_str(),
        "point_count": resolved.points.len(),
        "label": resolved.label,
    });
    if hole_count > 0 {
        result["hole_count"] = serde_json::json!(hole_count);
    }

    match format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            if hole_count > 0 {
                cprintln!(
                    "Profile set: {} ({} points, {} holes) on {} plane",
                    resolved.label.cyan(),
                    resolved.points.len().to_string().green().bold(),
                    hole_count.to_string().cyan(),
                    resolved.plane.as_str().cyan()
                );
            } else {
                cprintln!(
                    "Profile set: {} ({} points) on {} plane",
                    resolved.label.cyan(),
                    resolved.points.len().to_string().green().bold(),
                    resolved.plane.as_str().cyan()
                );
            }
        }
    }
}

/// Generate 2D profile points from a built-in shape.
/// Returns `(points, label)`.
fn generate_shape(shape: &ProfileShape, args: &ProfileArgs) -> Result<(Vec<[f64; 2]>, String)> {
    match shape {
        ProfileShape::Circle => {
            let radius = args
                .radius
                .ok_or_else(|| miette!("--radius is required for circle shape"))?;
            if radius <= 0.0 {
                return Err(miette!("--radius must be positive"));
            }
            let segments = args.segments.max(3);
            let points = generate_circle(radius, segments);
            Ok((points, format!("circle r={radius} n={segments}")))
        }
        ProfileShape::Ellipse => {
            let rx = args
                .radius_x
                .or(args.radius)
                .ok_or_else(|| miette!("--radius-x (or --radius) is required for ellipse"))?;
            let ry = args
                .radius_y
                .or(args.radius)
                .ok_or_else(|| miette!("--radius-y (or --radius) is required for ellipse"))?;
            if rx <= 0.0 || ry <= 0.0 {
                return Err(miette!("radii must be positive"));
            }
            let segments = args.segments.max(3);
            let points = generate_ellipse(rx, ry, segments);
            Ok((points, format!("ellipse rx={rx} ry={ry} n={segments}")))
        }
        ProfileShape::Rectangle => {
            let width = args
                .width
                .ok_or_else(|| miette!("--width is required for rectangle shape"))?;
            let height = args
                .height
                .ok_or_else(|| miette!("--height is required for rectangle shape"))?;
            if width <= 0.0 || height <= 0.0 {
                return Err(miette!("--width and --height must be positive"));
            }
            let points = generate_rectangle(width, height);
            Ok((points, format!("rectangle {width}x{height}")))
        }
    }
}

/// Generate CCW circle points centered at origin.
fn generate_circle(radius: f64, segments: u32) -> Vec<[f64; 2]> {
    (0..segments)
        .map(|i| {
            let angle = TAU * f64::from(i) / f64::from(segments);
            [radius * angle.cos(), radius * angle.sin()]
        })
        .collect()
}

/// Generate CCW ellipse points centered at origin.
fn generate_ellipse(rx: f64, ry: f64, segments: u32) -> Vec<[f64; 2]> {
    (0..segments)
        .map(|i| {
            let angle = TAU * f64::from(i) / f64::from(segments);
            [rx * angle.cos(), ry * angle.sin()]
        })
        .collect()
}

/// Generate CCW rectangle points centered at origin.
fn generate_rectangle(width: f64, height: f64) -> Vec<[f64; 2]> {
    let hw = width / 2.0;
    let hh = height / 2.0;
    vec![[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]]
}
