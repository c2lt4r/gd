use std::path::PathBuf;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use super::{CreateArgs, write_or_dry_run};
use gd_core::cprintln;

pub(crate) fn exec_create(args: &CreateArgs) -> Result<()> {
    let path = PathBuf::from(&args.path);

    if path.exists() {
        return Err(miette!("File already exists: {}", args.path));
    }

    let root_name = args.root_name.clone().unwrap_or_else(|| {
        let stem = path
            .file_stem()
            .map_or_else(|| "Root".to_string(), |s| s.to_string_lossy().to_string());
        to_pascal_case(&stem)
    });

    let content = generate_scene(&args.root_type, &root_name);

    write_or_dry_run(&path, &content, args.dry_run)?;

    if !args.dry_run {
        cprintln!(
            "{} Created {} (root: {} [{}])",
            "✓".green(),
            args.path.bold(),
            root_name.bold(),
            args.root_type,
        );
    }

    Ok(())
}

/// Generate a minimal Godot 4.x .tscn scene.
pub(crate) fn generate_scene(root_type: &str, root_name: &str) -> String {
    format!("[gd_scene format=3]\n\n[node name=\"{root_name}\" type=\"{root_type}\"]\n")
}

/// Convert a snake_case or lowercase string to PascalCase.
pub(crate) fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) => {
                    let mut word = c.to_uppercase().to_string();
                    word.extend(chars);
                    word
                }
                None => String::new(),
            }
        })
        .collect()
}
