use std::env;
use std::path::PathBuf;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use gd_core::config::find_project_root;
use gd_core::scene;

use super::{SetScriptArgs, read_and_parse_resource, write_or_dry_run};
use gd_core::cprintln;

pub(crate) fn exec_set_script(args: &SetScriptArgs) -> Result<()> {
    let path = PathBuf::from(&args.file);
    if !path.exists() {
        return Err(miette!("File not found: {}", args.file));
    }

    let cwd = env::current_dir().unwrap_or_default();
    let project_root = find_project_root(&cwd)
        .ok_or_else(|| miette!("No project.godot found — run from a Godot project directory"))?;

    let res_path = resolve_script_path(&args.script, &cwd, &project_root)?;

    let (source, data) = read_and_parse_resource(&path)?;
    let result = apply_set_script(&source, &data, &res_path)?;

    write_or_dry_run(&path, &result, args.dry_run)?;

    if !args.dry_run {
        cprintln!(
            "{} Set script to {} in {}",
            "✓".green(),
            res_path.bold(),
            args.file,
        );
    }

    Ok(())
}

pub(crate) fn apply_set_script(
    source: &str,
    data: &scene::ResourceData,
    res_path: &str,
) -> Result<String> {
    // Check if there's already a script property
    let existing_script = data
        .properties
        .iter()
        .find(|(k, _)| k == "script")
        .map(|(_, v)| v.clone());

    if let Some(ref script_value) = existing_script {
        // Already has a script — replace the ext_resource path
        if let Some(ext_id) = scene_helpers::extract_ext_resource_id(script_value) {
            return Ok(replace_script_ext_resource(source, ext_id, res_path));
        }
        return Err(miette!(
            "Cannot parse existing script reference: {script_value}"
        ));
    }

    // No script yet — add ext_resource + script property
    Ok(add_new_script(source, data, res_path))
}

/// Replace the path of an existing script ext_resource.
fn replace_script_ext_resource(source: &str, ext_id: &str, new_path: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut result: Vec<String> = Vec::with_capacity(lines.len());
    let ext_pattern = format!("id=\"{ext_id}\"");

    for line in &lines {
        let trimmed = line.trim();
        if trimmed.starts_with("[ext_resource") && trimmed.contains(&ext_pattern) {
            // Replace the path in this ext_resource
            let new_line = replace_ext_resource_path(trimmed, new_path);
            result.push(new_line);
            continue;
        }
        result.push((*line).to_string());
    }

    let mut output = result.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

/// Replace the path="..." in an ext_resource line.
fn replace_ext_resource_path(line: &str, new_path: &str) -> String {
    if let Some(start) = line.find("path=\"") {
        let after = &line[start + "path=\"".len()..];
        if let Some(end) = after.find('"') {
            return format!("{}path=\"{new_path}\"{}", &line[..start], &after[end + 1..]);
        }
    }
    line.to_string()
}

/// Add a new ext_resource for the script and a script property in [resource].
fn add_new_script(source: &str, data: &scene::ResourceData, res_path: &str) -> String {
    let next_id = scene_helpers::next_ext_resource_id(&data.ext_resources);
    let ext_line = format!("[ext_resource type=\"Script\" path=\"{res_path}\" id=\"{next_id}\"]");
    let script_prop = format!("script = ExtResource(\"{next_id}\")");

    let lines: Vec<&str> = source.lines().collect();
    let mut result: Vec<String> = Vec::with_capacity(lines.len() + 4);

    let mut ext_inserted = false;
    let mut script_inserted = false;

    for line in &lines {
        let trimmed = line.trim();

        // Insert ext_resource before the first non-ext section
        if !ext_inserted && scene_helpers::is_non_ext_section(trimmed) {
            result.push(ext_line.clone());
            result.push(String::new());
            ext_inserted = true;
        }

        // Update load_steps in the gd_resource header
        if trimmed.starts_with("[gd_resource") {
            if trimmed.contains("load_steps=") {
                result.push(scene_helpers::increment_load_steps(trimmed));
            } else {
                // No load_steps yet — insert it before format=
                result.push(insert_load_steps(trimmed, 2));
            }
            continue;
        }

        result.push((*line).to_string());

        // After the [resource] header, insert the script property
        if !script_inserted && trimmed.starts_with("[resource") {
            result.push(script_prop.clone());
            script_inserted = true;
        }
    }

    // If we never found a non-ext section (resource has only header)
    if !ext_inserted {
        result.push(String::new());
        result.push(ext_line);
    }

    let mut output = result.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

/// Insert `load_steps=N` before `format=` in a gd_resource header.
fn insert_load_steps(line: &str, steps: u32) -> String {
    if let Some(pos) = line.find("format=") {
        format!("{}load_steps={steps} {}", &line[..pos], &line[pos..])
    } else {
        // No format= either — just append before the closing ]
        if let Some(pos) = line.rfind(']') {
            format!("{} load_steps={steps}]", &line[..pos])
        } else {
            line.to_string()
        }
    }
}

/// Resolve a script path to a `res://` path.
fn resolve_script_path(
    script: &str,
    cwd: &std::path::Path,
    project_root: &std::path::Path,
) -> Result<String> {
    if script.starts_with("res://") {
        return Ok(script.to_string());
    }

    let script_path = PathBuf::from(script);
    let abs_script = if script_path.is_absolute() {
        script_path
    } else {
        let from_cwd = cwd.join(&script_path);
        if from_cwd.exists() {
            from_cwd
        } else {
            let from_root = project_root.join(&script_path);
            if from_root.exists() {
                from_root
            } else {
                return Err(miette!("Script file not found: {script}"));
            }
        }
    };

    let rel = abs_script
        .strip_prefix(project_root)
        .map_err(|_| miette!("Script is not inside the project root"))?;

    Ok(format!(
        "res://{}",
        path_slash::PathBufExt::to_slash_lossy(&rel.to_path_buf())
    ))
}

/// Module-internal access to scene_cmd helpers.
mod scene_helpers {
    pub(super) use crate::cli::scene_cmd::{
        extract_ext_resource_id, increment_load_steps, is_non_ext_section, next_ext_resource_id,
    };
}
