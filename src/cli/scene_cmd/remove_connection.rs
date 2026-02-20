use std::path::PathBuf;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use super::{RemoveConnectionArgs, clean_double_blanks, read_and_parse_scene, write_or_dry_run};
use crate::cprintln;

pub(crate) fn exec_remove_connection(args: &RemoveConnectionArgs) -> Result<()> {
    let path = PathBuf::from(&args.scene);
    if !path.exists() {
        return Err(miette!("Scene file not found: {}", args.scene));
    }

    let (source, _data) = read_and_parse_scene(&path)?;
    let result =
        remove_matching_connection(&source, &args.signal, &args.from, &args.to, &args.method)?;

    write_or_dry_run(&path, &result, args.dry_run)?;

    if !args.dry_run {
        cprintln!(
            "{} Removed connection {}.{} → {}.{} from {}",
            "✓".green(),
            args.from,
            args.signal,
            args.to,
            args.method.bold(),
            args.scene,
        );
    }

    Ok(())
}

/// Remove a matching connection line from the scene source.
pub(crate) fn remove_matching_connection(
    source: &str,
    signal: &str,
    from: &str,
    to: &str,
    method: &str,
) -> Result<String> {
    let target =
        format!("[connection signal=\"{signal}\" from=\"{from}\" to=\"{to}\" method=\"{method}\"]");

    let lines: Vec<&str> = source.lines().collect();
    let mut found = false;
    let mut result: Vec<String> = Vec::with_capacity(lines.len());

    for line in &lines {
        if line.trim() == target {
            found = true;
            continue;
        }
        result.push((*line).to_string());
    }

    if !found {
        return Err(miette!(
            "Connection not found: {}.{} → {}.{}",
            from,
            signal,
            to,
            method
        ));
    }

    let mut output = result.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    Ok(clean_double_blanks(&output))
}
