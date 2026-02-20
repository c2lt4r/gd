use std::path::Path;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use super::{OutputFormat, Plane, ReferenceArgs};
use crate::cprintln;

pub fn cmd_reference(args: &ReferenceArgs) -> Result<()> {
    let path = Path::new(&args.path);
    if !path.is_file() {
        return Err(miette!("Reference image not found: {}", args.path));
    }

    let metadata =
        std::fs::metadata(path).map_err(|e| miette!("Failed to read file metadata: {e}"))?;
    let size_bytes = metadata.len();

    let view_str = args.view.as_ref().map(Plane::as_str);

    match args.format {
        OutputFormat::Json => {
            let output = serde_json::json!({
                "path": args.path,
                "size_bytes": size_bytes,
                "view": view_str,
            });
            cprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        OutputFormat::Text => {
            let size_kb = size_bytes / 1024;
            let view_info = view_str.map_or(String::new(), |v| format!(" (view: {v})"));
            cprintln!("Reference: {} ({size_kb} KB){view_info}", args.path.green());
        }
    }
    Ok(())
}
