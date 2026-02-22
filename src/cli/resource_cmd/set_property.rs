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
    let mut consuming_multiline = false;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with("[resource") {
            in_resource = true;
            resource_found = true;
            insert_after = Some(i);
            consuming_multiline = false;
            result.push((*line).to_string());
            continue;
        }

        if in_resource && trimmed.starts_with('[') {
            in_resource = false;
            consuming_multiline = false;
        }

        // Skip continuation lines of a replaced multi-line value
        if consuming_multiline {
            if is_continuation_line(trimmed) {
                continue;
            }
            consuming_multiline = false;
        }

        if in_resource && trimmed.starts_with(&prop_prefix) {
            result.push(new_line.clone());
            replaced = true;
            // Check if the old value spans multiple lines (e.g. array/dict literal)
            if is_multiline_start(trimmed, &prop_prefix) {
                consuming_multiline = true;
            }
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

/// Check if a property line starts a multi-line value (unclosed bracket/brace).
fn is_multiline_start(line: &str, prop_prefix: &str) -> bool {
    let val = &line[prop_prefix.len()..];
    let open_brackets = val.chars().filter(|&c| c == '[').count();
    let close_brackets = val.chars().filter(|&c| c == ']').count();
    let open_braces = val.chars().filter(|&c| c == '{').count();
    let close_braces = val.chars().filter(|&c| c == '}').count();
    (open_brackets > close_brackets) || (open_braces > close_braces)
}

/// Check if a line is a continuation of a multi-line value (not a new property
/// or section header).
fn is_continuation_line(trimmed: &str) -> bool {
    // A new property: `identifier = value`
    // A section header: `[...]`
    // A blank line: end of property in Godot format
    if trimmed.is_empty() || trimmed.starts_with('[') {
        return false;
    }
    // A new property line has the pattern: identifier space = space value
    // Continuation lines are things like: `  "item",` or `]` or `}`
    !looks_like_property(trimmed)
}

/// Heuristic: does this line look like a Godot resource property (`key = value`)?
fn looks_like_property(trimmed: &str) -> bool {
    // Property keys are identifiers: start with letter/underscore, contain alphanum/_
    if let Some(eq_pos) = trimmed.find(" = ") {
        let key = &trimmed[..eq_pos];
        !key.is_empty()
            && key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '/')
    } else {
        false
    }
}
