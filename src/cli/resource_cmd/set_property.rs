use std::path::PathBuf;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use super::{SetPropertyArgs, read_and_parse_resource, write_or_dry_run};
use crate::cprintln;

pub(crate) fn exec_set_property(args: &SetPropertyArgs) -> Result<()> {
    let path = PathBuf::from(&args.file);
    if !path.exists() {
        return Err(miette!("File not found: {}", args.file));
    }

    let (source, _data) = read_and_parse_resource(&path)?;
    let result = apply_set_property(&source, &args.key, &args.value)?;

    write_or_dry_run(&path, &result, args.dry_run)?;

    if !args.dry_run {
        cprintln!(
            "{} Set {} = {} in {}",
            "✓".green(),
            args.key.bold(),
            args.value,
            args.file,
        );
    }

    Ok(())
}

pub(crate) fn apply_set_property(source: &str, key: &str, value: &str) -> Result<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut result: Vec<String> = Vec::with_capacity(lines.len() + 1);

    let prop_prefix = format!("{key} = ");
    let new_line = format!("{key} = {value}");

    let mut in_resource = false;
    let mut replaced = false;
    let mut resource_found = false;
    let mut insert_after = None;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with("[resource") {
            in_resource = true;
            resource_found = true;
            insert_after = Some(i);
            result.push((*line).to_string());
            continue;
        }

        if in_resource && trimmed.starts_with('[') {
            in_resource = false;
        }

        if in_resource && trimmed.starts_with(&prop_prefix) {
            result.push(new_line.clone());
            replaced = true;
            continue;
        }

        if in_resource && !trimmed.is_empty() {
            insert_after = Some(result.len());
        }

        result.push((*line).to_string());
    }

    if !resource_found {
        return Err(miette!("No [resource] section found in file"));
    }

    if !replaced {
        // Insert the new property after the [resource] header (or last property)
        if let Some(idx) = insert_after {
            result.insert(idx + 1, new_line);
        }
    }

    let mut output = result.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    Ok(output)
}
