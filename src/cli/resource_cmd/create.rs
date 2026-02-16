use std::env;
use std::path::PathBuf;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use crate::core::config::find_project_root;

use super::{CreateArgs, write_or_dry_run};

pub(crate) fn exec_create(args: &CreateArgs) -> Result<()> {
    let path = PathBuf::from(&args.path);
    if path.exists() {
        return Err(miette!("File already exists: {}", args.path));
    }

    let content = if let Some(ref script) = args.script {
        let cwd = env::current_dir().unwrap_or_default();
        let project_root = find_project_root(&cwd);
        let res_path = resolve_script_to_res(script, &cwd, project_root.as_deref())?;
        generate_resource_with_script(&args.resource_type, &res_path)
    } else {
        generate_resource(&args.resource_type)
    };

    write_or_dry_run(&path, &content, args.dry_run)?;

    if !args.dry_run {
        println!(
            "{} Created {} (type: {})",
            "✓".green(),
            args.path.bold(),
            args.resource_type,
        );
    }

    Ok(())
}

pub(crate) fn generate_resource(resource_type: &str) -> String {
    format!("[gd_resource type=\"{resource_type}\" format=3]\n\n[resource]\n")
}

pub(crate) fn generate_resource_with_script(resource_type: &str, res_path: &str) -> String {
    format!(
        "[gd_resource type=\"{resource_type}\" load_steps=2 format=3]\n\n\
         [ext_resource type=\"Script\" path=\"{res_path}\" id=\"1\"]\n\n\
         [resource]\nscript = ExtResource(\"1\")\n"
    )
}

/// Resolve a script path to a `res://` path.
fn resolve_script_to_res(
    script: &str,
    cwd: &std::path::Path,
    project_root: Option<&std::path::Path>,
) -> Result<String> {
    // Already a res:// path
    if let Some(stripped) = script.strip_prefix("res://") {
        // Validate the file exists if we know the project root
        if let Some(root) = project_root {
            let full = root.join(stripped);
            if !full.exists() {
                return Err(miette!("Script file not found: {script}"));
            }
        }
        return Ok(script.to_string());
    }

    let script_path = PathBuf::from(script);
    let abs_script = if script_path.is_absolute() {
        script_path
    } else {
        // Try CWD first, then project root
        let from_cwd = cwd.join(&script_path);
        if from_cwd.exists() {
            from_cwd
        } else if let Some(root) = project_root {
            let from_root = root.join(&script_path);
            if from_root.exists() {
                from_root
            } else {
                return Err(miette!("Script file not found: {script}"));
            }
        } else {
            return Err(miette!("Script file not found: {script}"));
        }
    };

    let root = project_root
        .ok_or_else(|| miette!("No project.godot found — cannot resolve script to res://"))?;

    let rel = abs_script
        .strip_prefix(root)
        .map_err(|_| miette!("Script is not inside the project root"))?;

    Ok(format!(
        "res://{}",
        path_slash::PathBufExt::to_slash_lossy(&rel.to_path_buf())
    ))
}
