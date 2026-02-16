mod create;
mod get_property;
mod info;
mod remove_property;
mod remove_script;
mod set_property;
mod set_script;

#[cfg(test)]
mod tests;

use clap::{Args, Subcommand};
use miette::{Result, miette};

use crate::core::scene;

#[derive(Args)]
pub struct ResourceArgs {
    #[command(subcommand)]
    pub command: ResourceCommand,
}

#[derive(Subcommand)]
pub enum ResourceCommand {
    /// Create a new .tres resource file
    Create(CreateArgs),
    /// Set or update a property in the [resource] section
    SetProperty(SetPropertyArgs),
    /// Print a property value to stdout
    GetProperty(GetPropertyArgs),
    /// Remove a property from the [resource] section
    RemoveProperty(RemovePropertyArgs),
    /// Attach or change the script on a resource
    SetScript(SetScriptArgs),
    /// Remove the script from a resource
    RemoveScript(RemoveScriptArgs),
    /// Print resource structure as JSON
    Info(InfoArgs),
}

#[derive(Args)]
pub struct CreateArgs {
    /// Path for the new .tres file
    pub path: String,
    /// Resource type (e.g. Resource, Theme, SpriteFrames)
    #[arg(long = "type")]
    pub resource_type: String,
    /// Path to a .gd script to attach
    #[arg(long)]
    pub script: Option<String>,
    /// Preview changes without writing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct SetPropertyArgs {
    /// Path to the .tres file
    pub file: String,
    /// Property key
    #[arg(long)]
    pub key: String,
    /// Property value (Godot resource format)
    #[arg(long)]
    pub value: String,
    /// Preview changes without writing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct GetPropertyArgs {
    /// Path to the .tres file
    pub file: String,
    /// Property key
    #[arg(long)]
    pub key: String,
}

#[derive(Args)]
pub struct RemovePropertyArgs {
    /// Path to the .tres file
    pub file: String,
    /// Property key
    #[arg(long)]
    pub key: String,
    /// Preview changes without writing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct SetScriptArgs {
    /// Path to the .tres file
    pub file: String,
    /// Path to the .gd script file
    pub script: String,
    /// Preview changes without writing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct RemoveScriptArgs {
    /// Path to the .tres file
    pub file: String,
    /// Preview changes without writing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct InfoArgs {
    /// Path to the .tres file
    pub file: String,
    /// Output format: json or human (default: human)
    #[arg(long)]
    pub format: Option<String>,
}

pub fn exec(args: &ResourceArgs) -> Result<()> {
    match &args.command {
        ResourceCommand::Create(a) => create::exec_create(a),
        ResourceCommand::SetProperty(a) => set_property::exec_set_property(a),
        ResourceCommand::GetProperty(a) => get_property::exec_get_property(a),
        ResourceCommand::RemoveProperty(a) => remove_property::exec_remove_property(a),
        ResourceCommand::SetScript(a) => set_script::exec_set_script(a),
        ResourceCommand::RemoveScript(a) => remove_script::exec_remove_script(a),
        ResourceCommand::Info(a) => info::exec_info(a),
    }
}

/// Read a .tres file and parse it, returning the raw text and parsed data.
pub(crate) fn read_and_parse_resource(
    path: &std::path::Path,
) -> Result<(String, scene::ResourceData)> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| miette!("Failed to read {}: {e}", path.display()))?;
    let data = scene::parse_tres(&source)?;
    Ok((source, data))
}

/// Write content to a file, or print to stdout if dry_run is true.
pub(crate) fn write_or_dry_run(path: &std::path::Path, content: &str, dry_run: bool) -> Result<()> {
    if dry_run {
        print!("{content}");
        return Ok(());
    }
    std::fs::write(path, content).map_err(|e| miette!("Failed to write {}: {e}", path.display()))
}
