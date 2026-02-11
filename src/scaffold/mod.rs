pub mod templates;

use std::fs;
use std::path::Path;
use std::process::Command;

use miette::{Context, IntoDiagnostic, bail};
use owo_colors::OwoColorize;

use templates::{
    GD_TOML_TEMPLATE, GITIGNORE_TEMPLATE, project_godot_content, scene_content, script_content,
    template_for,
};

/// Default Godot version used when Godot binary is not found.
const DEFAULT_GODOT_VERSION: &str = "4.6";

/// Detect the installed Godot version by running `godot --version`.
fn detect_godot_version() -> String {
    // Try GODOT_PATH env first, then search PATH
    let binary = std::env::var("GODOT_PATH")
        .ok()
        .unwrap_or_else(|| "godot".to_string());

    Command::new(&binary)
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| {
            let version_str = String::from_utf8_lossy(&out.stdout);
            // Output format: "4.6.stable.official.89cea1439"
            let parts: Vec<&str> = version_str.trim().splitn(3, '.').collect();
            if parts.len() >= 2 {
                Some(format!("{}.{}", parts[0], parts[1]))
            } else {
                None
            }
        })
        .unwrap_or_else(|| DEFAULT_GODOT_VERSION.to_string())
}

/// Create a new Godot project with the given name and template.
pub fn create_project(name: &str, template: &str) -> miette::Result<()> {
    let tpl = template_for(template).ok_or_else(|| {
        miette::miette!(
            "Unknown template '{}'. Valid templates: default, 2d, 3d",
            template
        )
    })?;

    let godot_version = detect_godot_version();

    let project_dir = Path::new(name);
    if project_dir.exists() {
        bail!("Directory '{}' already exists", name);
    }

    fs::create_dir_all(project_dir)
        .into_diagnostic()
        .wrap_err_with(|| format!("Failed to create directory '{}'", name))?;

    let files: &[(&str, String)] = &[
        (
            "project.godot",
            project_godot_content(name, tpl.renderer, tpl.renderer_feature, &godot_version),
        ),
        ("main.tscn", scene_content(tpl.node_type)),
        ("main.gd", script_content(tpl.node_type)),
        (".gitignore", GITIGNORE_TEMPLATE.to_owned()),
        ("gd.toml", GD_TOML_TEMPLATE.to_owned()),
    ];

    for (filename, content) in files {
        let path = project_dir.join(filename);
        fs::write(&path, content)
            .into_diagnostic()
            .wrap_err_with(|| format!("Failed to write {}", filename))?;
    }

    // Initialize git repo
    let git_ok = Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(project_dir)
        .status()
        .into_diagnostic()
        .wrap_err("Failed to run git init")?
        .success();

    // Print summary
    println!(
        "\n  {} Created project {} (template: {})\n",
        "✓".green().bold(),
        name.cyan().bold(),
        template.yellow(),
    );

    for (filename, _) in files {
        println!("    {} {}", "+".green(), filename);
    }

    if git_ok {
        println!("    {} Initialized git repository", "+".green());
    } else {
        println!(
            "    {} git init failed (git may not be installed)",
            "!".yellow()
        );
    }

    println!("\n  Run {} to get started.\n", format!("cd {name}").cyan());

    Ok(())
}
