use miette::Result;
use owo_colors::OwoColorize;
use serde::Serialize;

use gd_core::config::Config;
use gd_core::cprintln;

#[derive(clap::Args)]
pub struct EnvArgs {
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Serialize)]
struct EnvInfo {
    gd_version: String,
    godot_version: Option<String>,
    godot_path: Option<String>,
    os: String,
    arch: String,
    project_root: Option<String>,
    config_path: Option<String>,
    wsl: bool,
}

#[allow(clippy::unnecessary_wraps)]
pub fn exec(args: &EnvArgs) -> Result<()> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let config = Config::load(&cwd).unwrap_or_default();

    let project_root = gd_core::config::find_project_root(&cwd);
    let godot_path = crate::build::find_godot(&config).ok();
    let godot_version = godot_path.as_ref().and_then(|p| query_godot_version(p));
    let config_path = project_root
        .as_ref()
        .map(|r| r.join("gd.toml"))
        .filter(|p| p.exists());

    let info = EnvInfo {
        gd_version: env!("CARGO_PKG_VERSION").to_string(),
        godot_version,
        godot_path: godot_path.map(|p| p.to_string_lossy().to_string()),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        project_root: project_root.map(|p| p.to_string_lossy().to_string()),
        config_path: config_path.map(|p| p.to_string_lossy().to_string()),
        wsl: gd_core::fs::is_wsl(),
    };

    if args.json {
        let json = serde_json::to_string_pretty(&info).unwrap();
        cprintln!("{json}");
    } else {
        print_row("gd", &info.gd_version);
        print_row(
            "godot",
            info.godot_version.as_deref().unwrap_or("not found"),
        );
        print_row(
            "godot path",
            info.godot_path.as_deref().unwrap_or("not found"),
        );
        print_row("os", &format!("{} ({})", info.os, info.arch));
        if info.wsl {
            print_row("wsl", "yes");
        }
        print_row("project", info.project_root.as_deref().unwrap_or("none"));
        print_row("config", info.config_path.as_deref().unwrap_or("none"));
    }

    Ok(())
}

fn print_row(label: &str, value: &str) {
    cprintln!("{:>12} {}", label.bold(), value);
}

fn query_godot_version(godot: &std::path::Path) -> Option<String> {
    let output = std::process::Command::new(godot)
        .arg("--version")
        .output()
        .ok()?;
    let raw = String::from_utf8_lossy(&output.stdout);
    let version = raw.trim();
    if version.is_empty() {
        None
    } else {
        Some(version.to_string())
    }
}
