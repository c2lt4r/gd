//! `gd ssr` — Structured Search & Replace for GDScript.

use std::path::PathBuf;
use std::process;

use clap::Args;
use miette::{Result, miette};
use owo_colors::OwoColorize;
use serde_json::json;

use gd_core::config;
use gd_core::fs::collect_gdscript_files;
use gd_core::gd_ast;
use gd_core::parser;
use gd_core::ssr::{
    Capture, MatchResult, SsrPattern, SsrTemplate, apply_replacements,
    find_matches_constrained, parse_pattern, parse_template, render_replacement,
};
use gd_core::workspace_index::ProjectIndex;

// ═══════════════════════════════════════════════════════════════════════
//  Args
// ═══════════════════════════════════════════════════════════════════════

#[derive(Args)]
pub struct SsrArgs {
    /// SSR search pattern (GDScript with $placeholders)
    pub pattern: String,

    /// Replacement template (omit for search-only mode)
    #[arg(short, long)]
    pub replace: Option<String>,

    /// Preview changes without applying
    #[arg(short = 'n', long)]
    pub dry_run: bool,

    /// Restrict to specific file(s) (repeatable)
    #[arg(short, long)]
    pub file: Vec<PathBuf>,

    /// Output format: human (default) or json
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,

    /// Print match count only
    #[arg(short, long)]
    pub count: bool,
}

#[derive(Clone, Debug)]
pub enum OutputFormat {
    Human,
    Json,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "human" => Ok(Self::Human),
            "json" => Ok(Self::Json),
            other => Err(format!("unknown format: {other}")),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Execution
// ═══════════════════════════════════════════════════════════════════════

pub fn exec(args: &SsrArgs) -> Result<()> {
    // 1. Parse pattern.
    let pattern = parse_pattern(&args.pattern)?;

    // 2. Parse replacement template (if provided).
    let template = args
        .replace
        .as_ref()
        .map(|r| parse_template(r, &pattern))
        .transpose()?;

    // 3. Find project root.
    let cwd =
        std::env::current_dir().map_err(|e| miette!("failed to get working directory: {e}"))?;
    let project_root = config::find_project_root(&cwd)
        .ok_or_else(|| miette!("not in a Godot project (no project.godot found)"))?;

    // 4. Collect target files.
    let files = if args.file.is_empty() {
        collect_gdscript_files(&project_root)?
    } else {
        args.file.clone()
    };

    // 5. Build project index (for type constraints).
    let has_constraints = pattern
        .placeholders
        .values()
        .any(|p| p.constraint.is_some());
    let project = if has_constraints {
        Some(ProjectIndex::build(&project_root))
    } else {
        None
    };

    // 6. Find all matches across files.
    let all_matches = find_all(&pattern, &files, project.as_ref(), &project_root)?;

    if all_matches.is_empty() {
        if matches!(args.format, OutputFormat::Human) && !args.count {
            eprintln!("0 matches");
        }
        if matches!(args.format, OutputFormat::Json) {
            println!("{}", json!({ "matches": [], "total": 0, "files": 0 }));
        }
        process::exit(1);
    }

    // 7. Dispatch by mode.
    if let Some(tmpl) = &template {
        if args.dry_run || args.count {
            print_replace_preview(&all_matches, tmpl, &args.format, args.count, &project_root);
        } else {
            apply_all(&all_matches, tmpl, &files, &args.format, &project_root)?;
        }
    } else {
        print_search_results(&all_matches, &args.format, args.count, &project_root);
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
//  Find matches across files
// ═══════════════════════════════════════════════════════════════════════

/// Per-file match results.
struct FileMatches {
    path: PathBuf,
    source: String,
    matches: Vec<MatchResult>,
}

fn find_all(
    pattern: &SsrPattern,
    files: &[PathBuf],
    project: Option<&ProjectIndex>,
    project_root: &std::path::Path,
) -> Result<Vec<FileMatches>> {
    let mut all = Vec::new();

    for path in files {
        let source = std::fs::read_to_string(path)
            .map_err(|e| miette!("failed to read {}: {e}", path.display()))?;
        let tree = parser::parse(&source)?;
        let file = gd_ast::convert(&tree, &source);

        let rel = gd_core::fs::relative_slash(path, project_root);
        let matches =
            find_matches_constrained(pattern, &file, &source, PathBuf::from(&rel), project);

        if !matches.is_empty() {
            all.push(FileMatches {
                path: path.clone(),
                source,
                matches,
            });
        }
    }

    Ok(all)
}

// ═══════════════════════════════════════════════════════════════════════
//  Search output
// ═══════════════════════════════════════════════════════════════════════

fn print_search_results(
    all: &[FileMatches],
    format: &OutputFormat,
    count_only: bool,
    _project_root: &std::path::Path,
) {
    let total: usize = all.iter().map(|f| f.matches.len()).sum();
    let file_count = all.len();

    if count_only {
        match format {
            OutputFormat::Human => println!("{total} matches in {file_count} files"),
            OutputFormat::Json => println!("{}", json!({ "total": total, "files": file_count })),
        }
        return;
    }

    match format {
        OutputFormat::Human => {
            for fm in all {
                for m in &fm.matches {
                    let text = &fm.source[m.matched_range.clone()];
                    println!(
                        "{}  {}",
                        format!("{}:{}", fm.path.display(), m.line).green(),
                        text.trim()
                    );
                }
            }
            println!();
            println!("{total} matches in {file_count} files");
        }
        OutputFormat::Json => {
            let matches: Vec<_> = all
                .iter()
                .flat_map(|fm| {
                    fm.matches.iter().map(move |m| {
                        let text = &fm.source[m.matched_range.clone()];
                        json!({
                            "file": fm.path.display().to_string(),
                            "line": m.line,
                            "text": text.trim(),
                            "captures": captures_to_json(&m.captures),
                        })
                    })
                })
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "matches": matches,
                    "total": total,
                    "files": file_count,
                }))
                .unwrap()
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Replace preview / apply
// ═══════════════════════════════════════════════════════════════════════

fn print_replace_preview(
    all: &[FileMatches],
    template: &SsrTemplate,
    format: &OutputFormat,
    count_only: bool,
    _project_root: &std::path::Path,
) {
    let total: usize = all.iter().map(|f| f.matches.len()).sum();
    let file_count = all.len();

    if count_only {
        match format {
            OutputFormat::Human => println!("{total} replacements in {file_count} files"),
            OutputFormat::Json => println!("{}", json!({ "total": total, "files": file_count })),
        }
        return;
    }

    match format {
        OutputFormat::Human => {
            for fm in all {
                for m in &fm.matches {
                    let original = fm.source[m.matched_range.clone()].trim();
                    let replacement = render_replacement(template, &m.captures);
                    println!("{}:{}", fm.path.display().to_string().green(), m.line);
                    println!("  {} {}", "-".red(), original);
                    println!("  {} {}", "+".green(), replacement.trim());
                    println!();
                }
            }
            println!("{total} replacements in {file_count} files (dry run)");
        }
        OutputFormat::Json => {
            let replacements: Vec<_> = all
                .iter()
                .flat_map(|fm| {
                    fm.matches.iter().map(move |m| {
                        let original = fm.source[m.matched_range.clone()].trim();
                        let replacement = render_replacement(template, &m.captures);
                        json!({
                            "file": fm.path.display().to_string(),
                            "line": m.line,
                            "original": original,
                            "replacement": replacement.trim(),
                            "captures": captures_to_json(&m.captures),
                        })
                    })
                })
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "replacements": replacements,
                    "total": total,
                    "files": file_count,
                    "applied": false,
                }))
                .unwrap()
            );
        }
    }
}

fn apply_all(
    all: &[FileMatches],
    template: &SsrTemplate,
    _files: &[PathBuf],
    format: &OutputFormat,
    _project_root: &std::path::Path,
) -> Result<()> {
    let mut total = 0;
    let mut file_count = 0;

    for fm in all {
        let new_source = apply_replacements(&fm.source, &fm.matches, template);

        // Re-parse to validate no syntax errors introduced.
        let check = parser::parse(&new_source);
        if let Ok(tree) = &check
            && tree.root_node().has_error()
        {
            return Err(miette!(
                "replacement would introduce syntax errors in {}",
                fm.path.display()
            ));
        }

        std::fs::write(&fm.path, &new_source)
            .map_err(|e| miette!("failed to write {}: {e}", fm.path.display()))?;

        total += fm.matches.len();
        file_count += 1;
    }

    match format {
        OutputFormat::Human => {
            println!("{total} replacements applied in {file_count} files");
        }
        OutputFormat::Json => {
            println!(
                "{}",
                json!({ "total": total, "files": file_count, "applied": true })
            );
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
//  Helpers
// ═══════════════════════════════════════════════════════════════════════

fn captures_to_json(captures: &std::collections::HashMap<String, Capture>) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (name, cap) in captures {
        match cap {
            Capture::Expr(c) => {
                map.insert(name.clone(), json!(c.source_text));
            }
            Capture::ArgList(args) => {
                let texts: Vec<_> = args.iter().map(|c| json!(c.source_text)).collect();
                map.insert(name.clone(), json!(texts));
            }
        }
    }
    serde_json::Value::Object(map)
}
