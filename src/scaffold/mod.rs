pub mod templates;

use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

use miette::{Context, IntoDiagnostic, bail};
use owo_colors::OwoColorize;

use templates::{
    GD_TOML_TEMPLATE, GITIGNORE_TEMPLATE, project_godot_content, scene_content, script_content,
    template_for,
};

/// Create a new Godot project with the given name and template.
pub fn create_project(name: &str, template: &str) -> miette::Result<()> {
    let tpl = template_for(template).ok_or_else(|| {
        miette::miette!(
            "Unknown template '{}'. Valid templates: default, 2d, 3d",
            template
        )
    })?;

    let godot_version = crate::core::project::detect_godot_version();

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

/// Create a project from a GitHub template repository.
///
/// Accepts `owner/repo` or `owner/repo@ref` format. Downloads the repo as a zip,
/// finds `project.godot` to determine the Godot project root, and extracts from there.
pub fn create_from_github(name: &str, from: &str) -> miette::Result<()> {
    // Parse owner/repo[@ref], stripping any GitHub URL prefix
    let from = from
        .trim_start_matches("https://github.com/")
        .trim_start_matches("http://github.com/")
        .trim_end_matches('/')
        .trim_end_matches(".git");

    let (repo_path, git_ref) = if let Some((path, r)) = from.split_once('@') {
        (path, Some(r.to_string()))
    } else {
        (from, None)
    };

    let parts: Vec<&str> = repo_path.split('/').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        bail!(
            "Expected 'owner/repo' format, got '{}'\n  Example: gd new my-game --from nezvers/Godot-GameTemplate",
            from
        );
    }
    let owner = parts[0];
    let repo = parts[1];

    let project_dir = Path::new(name);
    if project_dir.exists() {
        bail!("Directory '{}' already exists", name);
    }

    // Resolve the git ref (branch/tag) to download
    let git_ref = match git_ref {
        Some(r) => r,
        None => resolve_default_branch(owner, repo)?,
    };

    println!(
        "  Downloading {}/{} ({})...",
        owner.cyan(),
        repo.cyan(),
        git_ref.yellow()
    );

    // Download zip archive from GitHub
    let url = format!(
        "https://github.com/{}/{}/archive/{}.zip",
        owner, repo, git_ref
    );
    let response = ureq::get(&url)
        .call()
        .map_err(|e| miette::miette!("Failed to download template: {e}"))?;

    let zip_data = response
        .into_body()
        .read_to_vec()
        .map_err(|e| miette::miette!("Failed to read download: {e}"))?;

    let cursor = io::Cursor::new(zip_data);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| miette::miette!("Failed to open archive: {e}"))?;

    // Scan for project.godot to find the Godot project root within the archive.
    // GitHub zips always have a top-level prefix dir (e.g., "Repo-main/").
    let project_root_prefix = find_project_root_in_archive(&mut archive);

    fs::create_dir_all(project_dir)
        .into_diagnostic()
        .wrap_err_with(|| format!("Failed to create directory '{}'", name))?;

    // Extract files
    let mut file_count = 0usize;
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| miette::miette!("Failed to read zip entry: {e}"))?;

        let file_path = file.name().to_string();

        // Strip the prefix to get the relative path within the project
        let relative = match file_path.strip_prefix(&project_root_prefix) {
            Some(r) => r,
            None => continue,
        };

        if relative.is_empty() {
            continue;
        }

        // Skip .git directory from the template
        if relative.starts_with(".git/") || relative == ".git" {
            continue;
        }

        let dest_path = project_dir.join(relative);

        if file.is_dir() {
            fs::create_dir_all(&dest_path)
                .map_err(|e| miette::miette!("Failed to create directory: {e}"))?;
        } else {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| miette::miette!("Failed to create parent directory: {e}"))?;
            }
            let mut outfile = fs::File::create(&dest_path)
                .map_err(|e| miette::miette!("Failed to create file: {e}"))?;
            io::copy(&mut file, &mut outfile)
                .map_err(|e| miette::miette!("Failed to extract file: {e}"))?;
            file_count += 1;
        }
    }

    if file_count == 0 {
        // Clean up empty directory on failure
        let _ = fs::remove_dir_all(project_dir);
        bail!("No files extracted from the template archive");
    }

    // Initialize git repo (fresh — template's .git was skipped)
    let git_ok = Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(project_dir)
        .status()
        .into_diagnostic()
        .wrap_err("Failed to run git init")?
        .success();

    // Print summary
    println!(
        "\n  {} Created project {} from {}/{}\n",
        "✓".green().bold(),
        name.cyan().bold(),
        owner,
        repo,
    );
    println!("    {} {} files extracted", "+".green(), file_count);
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

/// Scan a zip archive for `project.godot` and return the prefix path to use as the
/// extraction root. If `project.godot` is at `Repo-main/game/project.godot`, returns
/// `"Repo-main/game/"`. If at `Repo-main/project.godot`, returns `"Repo-main/"`.
/// Falls back to the GitHub top-level directory prefix if no project.godot found.
fn find_project_root_in_archive(archive: &mut zip::ZipArchive<io::Cursor<Vec<u8>>>) -> String {
    let mut shallowest: Option<String> = None;

    for i in 0..archive.len() {
        let Ok(file) = archive.by_index_raw(i) else {
            continue;
        };
        let path = file.name();

        // Look for entries ending with "project.godot"
        if path.ends_with("project.godot") {
            let prefix = path.trim_end_matches("project.godot");
            // Pick the shallowest (shortest prefix) match
            if shallowest.as_ref().is_none_or(|s| prefix.len() < s.len()) {
                shallowest = Some(prefix.to_string());
            }
        }
    }

    if let Some(prefix) = shallowest {
        return prefix;
    }

    // Fallback: use the first directory entry (GitHub's top-level prefix)
    for i in 0..archive.len() {
        let Ok(file) = archive.by_index_raw(i) else {
            continue;
        };
        let path = file.name();
        if path.ends_with('/') && path.matches('/').count() == 1 {
            return path.to_string();
        }
    }

    String::new()
}

/// Query the GitHub API to find a repository's default branch.
/// Falls back to "main" if the API call fails (e.g., rate-limited).
fn resolve_default_branch(owner: &str, repo: &str) -> miette::Result<String> {
    let url = format!("https://api.github.com/repos/{}/{}", owner, repo);
    match ureq::get(&url).header("User-Agent", "gd-cli").call() {
        Ok(mut resp) => {
            let info: serde_json::Value = resp
                .body_mut()
                .read_json()
                .map_err(|e| miette::miette!("Failed to parse GitHub API response: {e}"))?;
            if let Some(branch) = info.get("default_branch").and_then(|v| v.as_str()) {
                Ok(branch.to_string())
            } else {
                Ok("main".to_string())
            }
        }
        Err(_) => Ok("main".to_string()),
    }
}
