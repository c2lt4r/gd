use miette::{Result, miette};
use owo_colors::OwoColorize;

use super::{InitArgs, OutputFormat, project_root};

/// Minimal 3D workspace scene with lighting and a neutral background.
const WORKSPACE_SCENE: &str = "\
[gd_scene load_steps=2 format=3]

[sub_resource type=\"Environment\" id=\"Environment_mesh\"]
background_mode = 1
background_color = Color(0.08, 0.12, 0.18, 1)
ambient_light_source = 1
ambient_light_color = Color(0.3, 0.3, 0.3, 1)
tonemap_mode = 3

[node name=\"MeshWorkspace\" type=\"Node3D\"]

[node name=\"DirectionalLight3D\" type=\"DirectionalLight3D\" parent=\".\"]
transform = Transform3D(1, 0, 0, 0, 0.707107, -0.707107, 0, 0.707107, 0.707107, 0, 0, 0)
shadow_enabled = true

[node name=\"WorldEnvironment\" type=\"WorldEnvironment\" parent=\".\"]
environment = SubResource(\"Environment_mesh\")
";

pub fn cmd_init(args: &InitArgs) -> Result<()> {
    let root = project_root()?;
    let scene_path = root.join(&args.scene);

    if scene_path.exists() && !args.force {
        return Err(miette!(
            "Scene already exists: {} (use --force to overwrite)",
            args.scene
        ));
    }

    std::fs::write(&scene_path, WORKSPACE_SCENE)
        .map_err(|e| miette!("Failed to write scene: {e}"))?;

    // Set as main scene in project.godot so `gd run` works without args
    let res_scene = format!("res://{}", args.scene);
    set_main_scene(&root, &res_scene)?;

    match args.format {
        OutputFormat::Json => {
            let output = serde_json::json!({
                "scene": args.scene,
                "main_scene": res_scene,
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        OutputFormat::Text => {
            println!("Created mesh workspace: {}", args.scene.green());
            println!("Main scene set to: {res_scene}");
            println!("Start with: {}", "gd run".cyan());
            println!();
            println!("Coordinate system:");
            println!("  X = right/left,  Y = up/down,  Z = toward/away from camera");
            println!("  Forward = -Z,  1 unit = 1 meter");
        }
    }
    Ok(())
}

/// Set `run/main_scene` in the `[application]` section of project.godot.
fn set_main_scene(root: &std::path::Path, res_path: &str) -> Result<()> {
    let godot_path = root.join("project.godot");
    let content = std::fs::read_to_string(&godot_path)
        .map_err(|e| miette!("Failed to read project.godot: {e}"))?;

    let main_scene_line = format!("run/main_scene=\"{res_path}\"");
    let mut lines: Vec<String> = content.lines().map(String::from).collect();
    let mut found_app = false;
    let mut replaced = false;

    for (i, line) in lines.iter_mut().enumerate() {
        if line.trim() == "[application]" {
            found_app = true;
            continue;
        }
        if found_app && line.starts_with("run/main_scene") {
            line.clone_from(&main_scene_line);
            replaced = true;
            break;
        }
        // Hit next section header — insert before it
        if found_app && line.starts_with('[') && !replaced {
            lines.insert(i, main_scene_line.clone());
            replaced = true;
            break;
        }
    }

    if !found_app {
        // No [application] section — append one
        lines.push(String::new());
        lines.push("[application]".to_string());
        lines.push(main_scene_line);
    } else if !replaced {
        // [application] was the last section with no next header
        lines.push(main_scene_line);
    }

    let mut output = lines.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    std::fs::write(&godot_path, output)
        .map_err(|e| miette!("Failed to write project.godot: {e}"))?;
    Ok(())
}
