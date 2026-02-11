use clap::{Args, Subcommand};
use miette::{IntoDiagnostic, Result, miette};
use owo_colors::OwoColorize;
use std::fs;

use crate::core::project::GodotProject;

#[derive(Args)]
pub struct CiArgs {
    #[command(subcommand)]
    pub command: CiCommand,
}

#[derive(Subcommand)]
pub enum CiCommand {
    /// Generate GitHub Actions workflow
    Github(CiPlatformArgs),
    /// Generate GitLab CI configuration
    Gitlab(CiPlatformArgs),
}

#[derive(Args)]
pub struct CiPlatformArgs {
    /// Include export/build stage
    #[arg(long)]
    pub export: bool,
    /// Godot version to use (auto-detected from installed Godot)
    #[arg(long)]
    pub godot_version: Option<String>,
    /// Overwrite existing CI configuration
    #[arg(long)]
    pub force: bool,
}

pub fn exec(args: CiArgs) -> Result<()> {
    let project = GodotProject::discover(&std::env::current_dir().into_diagnostic()?)?;

    match args.command {
        CiCommand::Github(platform_args) => generate_github(&project, &platform_args),
        CiCommand::Gitlab(platform_args) => generate_gitlab(&project, &platform_args),
    }
}

fn resolve_godot_version(args: &CiPlatformArgs) -> String {
    args.godot_version
        .clone()
        .unwrap_or_else(crate::core::project::detect_godot_version)
}

fn generate_github(project: &GodotProject, args: &CiPlatformArgs) -> Result<()> {
    let godot_version = resolve_godot_version(args);
    let workflows_dir = project.root.join(".github/workflows");
    let ci_file = workflows_dir.join("ci.yml");

    // Check if file already exists
    if ci_file.exists() && !args.force {
        return Err(miette!(
            "GitHub Actions workflow already exists at .github/workflows/ci.yml\n\
             Use --force to overwrite"
        ));
    }

    // Create .github/workflows directory if it doesn't exist
    fs::create_dir_all(&workflows_dir)
        .map_err(|e| miette!("Failed to create .github/workflows directory: {e}"))?;

    // Generate the workflow content
    let export_job = if args.export {
        format!(
            r#"
  export:
    runs-on: ubuntu-latest
    needs: lint-and-format
    container:
      image: barichello/godot-ci:{version}
    steps:
      - uses: actions/checkout@v4

      - name: Setup export templates
        run: |
          mkdir -p ~/.local/share/godot/export_templates/{version}.stable
          mv /root/.local/share/godot/export_templates/{version}.stable/* ~/.local/share/godot/export_templates/{version}.stable/ || true

      - name: Export project
        run: |
          mkdir -p build
          godot --headless --export-release "Linux" build/game.x86_64
"#,
            version = godot_version
        )
    } else {
        String::new()
    };

    let content = format!(
        r#"name: GDScript CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  lint-and-format:
    runs-on: ubuntu-latest
    container:
      image: barichello/godot-ci:{version}
    steps:
      - uses: actions/checkout@v4

      - name: Install gd toolchain
        run: |
          curl -L https://github.com/c2lt4r/gd/releases/latest/download/gd-linux-x86_64 -o /usr/local/bin/gd
          chmod +x /usr/local/bin/gd

      - name: Check formatting
        run: gd fmt --check

      - name: Lint
        run: gd lint
{export_job}"#,
        version = godot_version,
        export_job = export_job
    );

    // Write the file
    fs::write(&ci_file, content)
        .map_err(|e| miette!("Failed to write GitHub Actions workflow: {e}"))?;

    println!("{}", "✓ GitHub Actions workflow created!".green().bold());
    println!("  {}", ci_file.display().dimmed());
    println!();
    println!("Next steps:");
    println!("  1. Review and customize .github/workflows/ci.yml");
    println!("  2. Update the download URL for the gd toolchain");
    println!("  3. Commit and push to enable CI");

    Ok(())
}

fn generate_gitlab(project: &GodotProject, args: &CiPlatformArgs) -> Result<()> {
    let godot_version = resolve_godot_version(args);
    let ci_file = project.root.join(".gitlab-ci.yml");

    // Check if file already exists
    if ci_file.exists() && !args.force {
        return Err(miette!(
            "GitLab CI configuration already exists at .gitlab-ci.yml\n\
             Use --force to overwrite"
        ));
    }

    // Generate the CI content
    let export_stage = if args.export { "  - export" } else { "" };

    let export_job = if args.export {
        r#"

export:
  stage: export
  script:
    - mkdir -p build
    - godot --headless --export-release "Linux" build/game.x86_64
  artifacts:
    paths:
      - build/
"#
        .to_string()
    } else {
        String::new()
    };

    let content = format!(
        r#"image: barichello/godot-ci:{version}

stages:
  - lint{export_stage}

lint:
  stage: lint
  script:
    - curl -L https://github.com/c2lt4r/gd/releases/latest/download/gd-linux-x86_64 -o /usr/local/bin/gd && chmod +x /usr/local/bin/gd
    - gd fmt --check
    - gd lint
{export_job}"#,
        version = godot_version,
        export_stage = export_stage,
        export_job = export_job
    );

    // Write the file
    fs::write(&ci_file, content)
        .map_err(|e| miette!("Failed to write GitLab CI configuration: {e}"))?;

    println!("{}", "✓ GitLab CI configuration created!".green().bold());
    println!("  {}", ci_file.display().dimmed());
    println!();
    println!("Next steps:");
    println!("  1. Review and customize .gitlab-ci.yml");
    println!("  2. Update the download URL for the gd toolchain");
    println!("  3. Commit and push to enable CI");

    Ok(())
}
