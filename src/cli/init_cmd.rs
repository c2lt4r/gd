use clap::Args;
use miette::{miette, Result};
use owo_colors::OwoColorize;
use std::env;
use std::fs;

use crate::core::project::GodotProject;
use crate::scaffold::templates::GD_TOML_TEMPLATE;

#[derive(Args)]
pub struct InitArgs {
    /// Overwrite existing gd.toml
    #[arg(long)]
    pub force: bool,
}

pub fn exec(args: InitArgs) -> Result<()> {
    let cwd = env::current_dir().map_err(|e| miette!("Failed to get current directory: {e}"))?;

    let project = GodotProject::discover(&cwd)?;
    let config_path = project.root.join("gd.toml");

    if config_path.exists() && !args.force {
        return Err(miette!(
            "gd.toml already exists at {}\nUse {} to overwrite",
            config_path.display(),
            "--force".bold()
        ));
    }

    fs::write(&config_path, GD_TOML_TEMPLATE)
        .map_err(|e| miette!("Failed to write gd.toml: {e}"))?;

    let name = project.name()?;

    println!(
        "{} gd toolchain in project {}",
        "Initialized".green().bold(),
        name.bold()
    );
    println!("  Project root: {}", project.root.display());
    println!("  Config: {}", config_path.display());

    Ok(())
}
