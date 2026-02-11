use clap::{Args, Subcommand};
use miette::{Result, miette};
use owo_colors::OwoColorize;
use std::fs;
use std::io;
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
    /// Install an addon from a git URL or Asset Library
    Install(InstallArgs),
    /// Remove an installed addon
    Remove(RemoveArgs),
    /// Search the Godot Asset Library
    Search(SearchArgs),
}

#[derive(Args)]
pub struct InstallArgs {
    /// Git URL, Asset Library ID (numeric), or addon name to install
    pub source: String,
    /// Custom name for the addon directory
    #[arg(long)]
    pub name: Option<String>,
}

#[derive(Args)]
pub struct RemoveArgs {
    /// Name of the addon to remove
    pub name: String,
}

#[derive(Args)]
pub struct SearchArgs {
    /// Search query
    pub query: String,
}

pub fn exec(args: AddonsArgs) -> Result<()> {
    match args.command {
        AddonsCommand::List => list_addons(),
        AddonsCommand::Install(install_args) => install_addon(install_args),
        AddonsCommand::Remove(remove_args) => remove_addon(remove_args),
        AddonsCommand::Search(search_args) => search_addons(search_args),
    }
}

fn list_addons() -> Result<()> {
    let cwd =
        std::env::current_dir().map_err(|e| miette!("Failed to get current directory: {e}"))?;
    let project = crate::core::project::GodotProject::discover(&cwd)?;
    let addons_dir = project.root.join("addons");

    if !addons_dir.exists() {
        println!("No addons directory found.");
        return Ok(());
    }

    let entries =
        fs::read_dir(&addons_dir).map_err(|e| miette!("Failed to read addons directory: {e}"))?;

    let mut addons = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|e| miette!("Failed to read directory entry: {e}"))?;
        let path = entry.path();

        if path.is_dir() {
            let plugin_cfg = path.join("plugin.cfg");
            if plugin_cfg.exists()
                && let Ok(info) = parse_plugin_cfg(&plugin_cfg)
            {
                addons.push((entry.file_name().to_string_lossy().to_string(), info));
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
    let cwd =
        std::env::current_dir().map_err(|e| miette!("Failed to get current directory: {e}"))?;
    let project = crate::core::project::GodotProject::discover(&cwd)?;
    let addons_dir = project.root.join("addons");

    // Create addons directory if it doesn't exist
    if !addons_dir.exists() {
        fs::create_dir(&addons_dir)
            .map_err(|e| miette!("Failed to create addons directory: {e}"))?;
    }

    // Determine if source is an Asset Library ID, git URL, or name
    if args.source.chars().all(|c| c.is_ascii_digit()) {
        // Numeric ID - install from Asset Library
        install_from_asset_library(&args.source, args.name.as_deref(), &addons_dir)
    } else if args.source.starts_with("http") || args.source.starts_with("git") {
        // Git URL - use existing git clone logic
        install_from_git(&args.source, args.name.as_deref(), &addons_dir)
    } else {
        // Name - search Asset Library for exact match
        install_from_asset_library_by_name(&args.source, args.name.as_deref(), &addons_dir)
    }
}

fn install_from_git(
    url: &str,
    custom_name: Option<&str>,
    addons_dir: &std::path::Path,
) -> Result<()> {
    // Determine addon name
    let addon_name = if let Some(name) = custom_name {
        name.to_string()
    } else {
        // Extract from URL (last path segment without .git)
        let url_path = url.trim_end_matches('/');
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

    println!("Installing {} from {}...", addon_name.green().bold(), url);

    // Run git clone
    let status = ProcessCommand::new("git")
        .arg("clone")
        .arg(url)
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

fn install_from_asset_library(
    asset_id: &str,
    custom_name: Option<&str>,
    addons_dir: &std::path::Path,
) -> Result<()> {
    println!(
        "Fetching asset {} from Godot Asset Library...",
        asset_id.cyan()
    );

    // Get asset details
    let url = format!(
        "https://godotengine.org/asset-library/api/asset/{}",
        asset_id
    );
    let mut response = ureq::get(&url)
        .call()
        .map_err(|e| miette!("Failed to fetch asset details: {e}"))?;
    let asset: AssetInfo = response
        .body_mut()
        .read_json()
        .map_err(|e| miette!("Failed to parse asset details: {e}"))?;

    if asset.download_url.is_empty() {
        return Err(miette!("Asset has no download URL"));
    }

    let addon_name = custom_name.unwrap_or(&asset.title).to_string();
    let addon_path = addons_dir.join(&addon_name);

    if addon_path.exists() {
        return Err(miette!("Addon '{}' already exists", addon_name));
    }

    println!(
        "Downloading {} by {}...",
        asset.title.green().bold(),
        asset.author
    );

    // Download the zip file
    let zip_response = ureq::get(&asset.download_url)
        .call()
        .map_err(|e| miette!("Failed to download asset: {e}"))?;

    let zip_data = zip_response
        .into_body()
        .read_to_vec()
        .map_err(|e| miette!("Failed to read download: {e}"))?;

    // Extract the zip
    let cursor = io::Cursor::new(zip_data);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| miette!("Failed to open zip archive: {e}"))?;

    // Find the addons directory in the archive and extract it
    let mut found_addon = false;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| miette!("Failed to read zip entry: {e}"))?;

        let file_path = file.name();

        // Skip if not in addons directory
        if !file_path.starts_with("addons/") {
            continue;
        }

        found_addon = true;

        // Remove "addons/" prefix and prepend our addon name
        let relative_path = file_path.strip_prefix("addons/").unwrap();

        // Skip the top-level addons directory itself
        if relative_path.is_empty() {
            continue;
        }

        let dest_path = addon_path.join(relative_path);

        if file.is_dir() {
            fs::create_dir_all(&dest_path)
                .map_err(|e| miette!("Failed to create directory: {e}"))?;
        } else {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| miette!("Failed to create parent directory: {e}"))?;
            }

            let mut outfile =
                fs::File::create(&dest_path).map_err(|e| miette!("Failed to create file: {e}"))?;

            io::copy(&mut file, &mut outfile)
                .map_err(|e| miette!("Failed to extract file: {e}"))?;
        }
    }

    if !found_addon {
        return Err(miette!(
            "No addons directory found in the downloaded archive"
        ));
    }

    println!("\n{}", "Successfully installed!".green().bold());
    println!("  Name: {}", asset.title);
    println!("  Author: {}", asset.author);
    if !asset.godot_version.is_empty() {
        println!("  Godot version: {}", asset.godot_version.cyan());
    }
    if !asset.description.is_empty() {
        println!("  Description: {}", asset.description.dimmed());
    }

    Ok(())
}

fn install_from_asset_library_by_name(
    name: &str,
    custom_name: Option<&str>,
    addons_dir: &std::path::Path,
) -> Result<()> {
    println!("Searching Asset Library for '{}'...", name);

    // Search for the asset
    let search_url = format!(
        "https://godotengine.org/asset-library/api/asset?filter={}&max_results=10",
        urlencoding::encode(name)
    );

    let search_response: AssetSearchResponse = ureq::get(&search_url)
        .call()
        .map_err(|e| miette!("Failed to search Asset Library: {e}"))?
        .body_mut()
        .read_json()
        .map_err(|e| miette!("Failed to parse search results: {e}"))?;

    // Find exact match (case-insensitive)
    let asset = search_response.result.iter()
        .find(|a| a.title.eq_ignore_ascii_case(name))
        .ok_or_else(|| miette!("No exact match found for '{}'. Try 'gd addons search {}' to see available options.", name, name))?;

    // Install using the asset ID
    install_from_asset_library(&asset.asset_id, custom_name, addons_dir)
}

fn remove_addon(args: RemoveArgs) -> Result<()> {
    let cwd =
        std::env::current_dir().map_err(|e| miette!("Failed to get current directory: {e}"))?;
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
    let content =
        fs::read_to_string(path).map_err(|e| miette!("Failed to read plugin.cfg: {e}"))?;

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

fn search_addons(args: SearchArgs) -> Result<()> {
    println!("Searching Asset Library for '{}'...\n", args.query);

    let search_url = format!(
        "https://godotengine.org/asset-library/api/asset?filter={}&max_results=10",
        urlencoding::encode(&args.query)
    );

    let search_response: AssetSearchResponse = ureq::get(&search_url)
        .call()
        .map_err(|e| miette!("Failed to search Asset Library: {e}"))?
        .body_mut()
        .read_json()
        .map_err(|e| miette!("Failed to parse search results: {e}"))?;

    if search_response.result.is_empty() {
        println!("No results found for '{}'", args.query);
        return Ok(());
    }

    println!("Found {} result(s):\n", search_response.result.len());

    for asset in &search_response.result {
        println!("  {} {}", "ID:".dimmed(), asset.asset_id.cyan());
        println!("  {} {}", "Title:".dimmed(), asset.title.green().bold());
        println!("  {} {}", "Author:".dimmed(), asset.author);
        println!("  {} {}", "Category:".dimmed(), asset.category);
        if !asset.cost.is_empty() {
            println!("  {} {}", "License:".dimmed(), asset.cost);
        }
        if !asset.godot_version.is_empty() {
            println!("  {} {}", "Godot version:".dimmed(), asset.godot_version);
        }
        if !asset.description.is_empty() {
            let desc = if asset.description.len() > 100 {
                format!("{}...", &asset.description[..97])
            } else {
                asset.description.clone()
            };
            println!("  {} {}", "Description:".dimmed(), desc);
        }
        println!();
    }

    println!(
        "To install, run: {}",
        "gd addons install <ID>".to_string().cyan()
    );

    Ok(())
}

// Asset Library API response structs
#[derive(serde::Deserialize)]
struct AssetSearchResponse {
    result: Vec<AssetInfo>,
}

#[derive(serde::Deserialize)]
struct AssetInfo {
    asset_id: String,
    title: String,
    author: String,
    category: String,
    cost: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    godot_version: String,
    #[serde(default)]
    download_url: String,
}
