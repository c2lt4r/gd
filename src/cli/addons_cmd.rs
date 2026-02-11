use clap::{Args, Subcommand};
use miette::{miette, Result};
use owo_colors::OwoColorize;
use std::fs;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

#[derive(Args)]
pub struct AddonsArgs {
    #[command(subcommand)]
    pub command: AddonsCommand,
}

#[derive(Subcommand)]
pub enum AddonsCommand {
    /// List installed addons
    List,
    /// Install an addon from a git URL
    Install(InstallArgs),
    /// Remove an installed addon
    Remove(RemoveArgs),
}

#[derive(Args)]
pub struct InstallArgs {
    /// Git URL to install from
    pub url: String,
    /// Custom name for the addon directory
    #[arg(long)]
    pub name: Option<String>,
}

#[derive(Args)]
pub struct RemoveArgs {
    /// Name of the addon to remove
    pub name: String,
}

pub fn exec(args: AddonsArgs) -> Result<()> {
    match args.command {
        AddonsCommand::List => list_addons(),
        AddonsCommand::Install(install_args) => install_addon(install_args),
        AddonsCommand::Remove(remove_args) => remove_addon(remove_args),
    }
}

fn list_addons() -> Result<()> {
    let cwd = std::env::current_dir()
        .map_err(|e| miette!("Failed to get current directory: {e}"))?;
    let project = crate::core::project::GodotProject::discover(&cwd)?;
    let addons_dir = project.root.join("addons");

    if !addons_dir.exists() {
        println!("No addons directory found.");
        return Ok(());
    }

    let entries = fs::read_dir(&addons_dir)
        .map_err(|e| miette!("Failed to read addons directory: {e}"))?;

    let mut addons = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|e| miette!("Failed to read directory entry: {e}"))?;
        let path = entry.path();

        if path.is_dir() {
            let plugin_cfg = path.join("plugin.cfg");
            if plugin_cfg.exists() {
                if let Ok(info) = parse_plugin_cfg(&plugin_cfg) {
                    addons.push((entry.file_name().to_string_lossy().to_string(), info));
                }
            }
        }
    }

    if addons.is_empty() {
        println!("No addons installed.");
        return Ok(());
    }

    println!("Installed addons ({}):\n", addons.len());

    for (dir_name, info) in addons {
        print!("  {}", dir_name.green().bold());
        if let Some(version) = &info.version {
            print!(" {}", version.cyan());
        }
        println!();

        if let Some(desc) = &info.description {
            println!("    {}", desc.dimmed());
        }
        if let Some(author) = &info.author {
            println!("    Author: {}", author.dimmed());
        }
        println!();
    }

    Ok(())
}

fn install_addon(args: InstallArgs) -> Result<()> {
    let cwd = std::env::current_dir()
        .map_err(|e| miette!("Failed to get current directory: {e}"))?;
    let project = crate::core::project::GodotProject::discover(&cwd)?;
    let addons_dir = project.root.join("addons");

    // Create addons directory if it doesn't exist
    if !addons_dir.exists() {
        fs::create_dir(&addons_dir)
            .map_err(|e| miette!("Failed to create addons directory: {e}"))?;
    }

    // Determine addon name
    let addon_name = if let Some(name) = args.name {
        name
    } else {
        // Extract from URL (last path segment without .git)
        let url_path = args.url.trim_end_matches('/');
        let last_segment = url_path
            .rsplit('/')
            .next()
            .ok_or_else(|| miette!("Invalid URL format"))?;
        last_segment.trim_end_matches(".git").to_string()
    };

    let addon_path = addons_dir.join(&addon_name);

    if addon_path.exists() {
        return Err(miette!("Addon '{}' already exists", addon_name));
    }

    println!("Installing {} from {}...", addon_name.green().bold(), args.url);

    // Run git clone
    let status = ProcessCommand::new("git")
        .arg("clone")
        .arg(&args.url)
        .arg(&addon_path)
        .status()
        .map_err(|e| miette!("Failed to run git clone: {e}"))?;

    if !status.success() {
        return Err(miette!("Git clone failed"));
    }

    // Check for plugin.cfg and display info
    let plugin_cfg = addon_path.join("plugin.cfg");
    if plugin_cfg.exists() {
        if let Ok(info) = parse_plugin_cfg(&plugin_cfg) {
            println!("\n{}", "Successfully installed!".green().bold());
            if let Some(name) = &info.name {
                println!("  Name: {}", name);
            }
            if let Some(version) = &info.version {
                println!("  Version: {}", version.cyan());
            }
            if let Some(author) = &info.author {
                println!("  Author: {}", author);
            }
            if let Some(desc) = &info.description {
                println!("  Description: {}", desc.dimmed());
            }
        }
    } else {
        println!("{}", "Installed successfully!".green().bold());
    }

    Ok(())
}

fn remove_addon(args: RemoveArgs) -> Result<()> {
    let cwd = std::env::current_dir()
        .map_err(|e| miette!("Failed to get current directory: {e}"))?;
    let project = crate::core::project::GodotProject::discover(&cwd)?;
    let addons_dir = project.root.join("addons");
    let addon_path = addons_dir.join(&args.name);

    if !addon_path.exists() {
        return Err(miette!("Addon '{}' not found", args.name));
    }

    if !addon_path.is_dir() {
        return Err(miette!("'{}' is not a directory", args.name));
    }

    // Safety check: verify it has plugin.cfg
    let plugin_cfg = addon_path.join("plugin.cfg");
    if !plugin_cfg.exists() {
        return Err(miette!(
            "Directory '{}' does not appear to be a valid addon (no plugin.cfg found)",
            args.name
        ));
    }

    // Remove the directory
    fs::remove_dir_all(&addon_path)
        .map_err(|e| miette!("Failed to remove addon directory: {e}"))?;

    println!("{} {}", "Removed addon".green(), args.name.bold());

    Ok(())
}

#[derive(Debug)]
struct PluginInfo {
    name: Option<String>,
    description: Option<String>,
    author: Option<String>,
    version: Option<String>,
}

fn parse_plugin_cfg(path: &PathBuf) -> Result<PluginInfo> {
    let content = fs::read_to_string(path)
        .map_err(|e| miette!("Failed to read plugin.cfg: {e}"))?;

    let mut info = PluginInfo {
        name: None,
        description: None,
        author: None,
        version: None,
    };

    for line in content.lines() {
        let line = line.trim();
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim().trim_matches('"');

            match key {
                "name" => info.name = Some(value.to_string()),
                "description" => info.description = Some(value.to_string()),
                "author" => info.author = Some(value.to_string()),
                "version" => info.version = Some(value.to_string()),
                _ => {}
            }
        }
    }

    Ok(info)
}
