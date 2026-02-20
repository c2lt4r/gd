use crate::cprintln;
use clap::Args;
use miette::{Result, miette};
use owo_colors::OwoColorize;
use std::env;
use std::path::Path;

#[derive(Args)]
pub struct CleanArgs {
    /// Only remove build/ directory, keep .godot/ cache
    #[arg(long)]
    pub keep_cache: bool,
}

pub fn exec(args: &CleanArgs) -> Result<()> {
    let cwd = env::current_dir().unwrap_or_default();
    let project = crate::core::project::GodotProject::discover(&cwd)?;

    let dirs_to_clean: Vec<&str> = if args.keep_cache {
        vec!["build"]
    } else {
        vec![".godot", "build"]
    };

    let mut removed = false;
    for dir_name in dirs_to_clean {
        let dir = project.root.join(dir_name);
        if dir.exists() {
            remove_dir(&dir)?;
            cprintln!("{} Removed {}", "✓".green(), dir_name);
            removed = true;
        }
    }

    if removed {
        cprintln!("{} Clean complete", "✓".green());
    } else {
        cprintln!("{} Nothing to clean", "✓".green());
    }
    Ok(())
}

fn remove_dir(path: &Path) -> Result<()> {
    std::fs::remove_dir_all(path).map_err(|e| miette!("Failed to remove {}: {e}", path.display()))
}
