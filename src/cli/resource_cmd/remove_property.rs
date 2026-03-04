use std::path::PathBuf;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use super::{RemovePropertyArgs, read_and_parse_resource, write_or_dry_run};
use gd_core::{ceprintln, cprintln};

pub(crate) fn exec_remove_property(args: &RemovePropertyArgs) -> Result<()> {
    let path = PathBuf::from(&args.file);
    if !path.exists() {
        return Err(miette!("File not found: {}", args.file));
    }

    if args.key == "script" {
        ceprintln!(
            "{} To remove a script, use `gd resource remove-script` instead",
            "warning:".yellow().bold(),
        );
    }

    let (source, _data) = read_and_parse_resource(&path)?;
    let result = apply_remove_property(&source, &args.key)?;

    write_or_dry_run(&path, &result, args.dry_run)?;

    if !args.dry_run {
        cprintln!(
            "{} Removed property '{}' from {}",
            "✓".green(),
            args.key.bold(),
            args.file,
        );
    }

    Ok(())
}

pub(crate) fn apply_remove_property(source: &str, key: &str) -> Result<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut result: Vec<String> = Vec::with_capacity(lines.len());

    let prop_prefix = format!("{key} = ");
    let mut in_resource = false;
    let mut removed = false;

    for line in &lines {
        let trimmed = line.trim();

        if trimmed.starts_with("[resource") {
            in_resource = true;
        } else if in_resource && trimmed.starts_with('[') {
            in_resource = false;
        }

        if in_resource && trimmed.starts_with(&prop_prefix) {
            removed = true;
            continue;
        }

        result.push((*line).to_string());
    }

    if !removed {
        return Err(miette!(
            "Property '{}' not found in [resource] section",
            key
        ));
    }

    let mut output = result.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    Ok(output)
}
