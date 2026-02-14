use clap::Args;
use miette::{Result, miette};
use owo_colors::OwoColorize;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use tree_sitter::Node;

#[derive(Args)]
pub struct StatsArgs {
    /// Paths to analyze (defaults to current directory)
    pub paths: Vec<String>,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: String,
    /// Show per-directory breakdown
    #[arg(long)]
    pub by_dir: bool,
    /// Show top N complexity hotspots (longest functions)
    #[arg(long)]
    pub top: Option<usize>,
    /// Compare stats against another git branch
    #[arg(long)]
    pub diff: Option<String>,
}

#[derive(Debug, Default, Serialize)]
struct ProjectStats {
    files: usize,
    lines_total: usize,
    lines_code: usize,
    lines_blank: usize,
    lines_comment: usize,
    classes: usize,
    functions: usize,
    signals: usize,
    avg_function_length: f64,
    longest_function: Option<FunctionInfo>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    hotspots: Vec<FunctionInfo>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    directories: Vec<DirStats>,
}

#[derive(Debug, Clone, Serialize)]
struct FunctionInfo {
    name: String,
    lines: usize,
    file: String,
}

#[derive(Debug, Clone, Serialize)]
struct DirStats {
    directory: String,
    files: usize,
    lines_code: usize,
    functions: usize,
}

#[derive(Debug, Serialize)]
struct StatsDiff {
    current: ProjectStats,
    other: ProjectStats,
    other_branch: String,
    delta: StatsDelta,
}

#[derive(Debug, Serialize)]
struct StatsDelta {
    files: i64,
    lines_code: i64,
    functions: i64,
    classes: i64,
    signals: i64,
}

#[derive(Debug, Default)]
struct FileStats {
    path: PathBuf,
    lines_total: usize,
    lines_code: usize,
    lines_blank: usize,
    lines_comment: usize,
    classes: usize,
    functions: usize,
    signals: usize,
    function_lengths: Vec<usize>,
    all_functions: Vec<FunctionInfo>,
    longest_function: Option<FunctionInfo>,
}

pub fn exec(args: &StatsArgs) -> Result<()> {
    let root = if args.paths.is_empty() {
        std::env::current_dir().map_err(|e| miette!("Failed to get current directory: {e}"))?
    } else {
        PathBuf::from(&args.paths[0])
    };

    // --diff mode: compare current vs another branch
    if let Some(ref branch) = args.diff {
        let current = collect_current_stats(&root, args)?;
        let other = collect_branch_stats(&root, branch)?;

        let delta = StatsDelta {
            files: current.files as i64 - other.files as i64,
            lines_code: current.lines_code as i64 - other.lines_code as i64,
            functions: current.functions as i64 - other.functions as i64,
            classes: current.classes as i64 - other.classes as i64,
            signals: current.signals as i64 - other.signals as i64,
        };

        let diff = StatsDiff {
            current,
            other,
            other_branch: branch.clone(),
            delta,
        };

        match args.format.as_str() {
            "json" => {
                let json = serde_json::to_string_pretty(&diff)
                    .map_err(|e| miette!("Failed to serialize: {e}"))?;
                println!("{json}");
            }
            "human" => output_human_diff(&diff),
            _ => return Err(miette!("Invalid format: {}", args.format)),
        }
        return Ok(());
    }

    let stats = collect_current_stats(&root, args)?;

    // Output results
    match args.format.as_str() {
        "json" => output_json(&stats)?,
        "human" => output_human(&stats, args.by_dir, args.top),
        _ => return Err(miette!("Invalid format: {}", args.format)),
    }

    Ok(())
}

fn collect_current_stats(root: &Path, args: &StatsArgs) -> Result<ProjectStats> {
    let files = crate::core::fs::collect_gdscript_files(root)?;

    if files.is_empty() {
        return Err(miette!("No .gd files found in {}", root.display()));
    }

    // Process files in parallel
    let file_stats: Vec<FileStats> = files
        .par_iter()
        .filter_map(|path| analyze_file(path, root).ok())
        .collect();

    // Aggregate statistics
    let mut stats = ProjectStats {
        files: file_stats.len(),
        ..Default::default()
    };

    let mut all_function_lengths = Vec::new();
    let mut all_functions = Vec::new();
    let mut dir_map: HashMap<String, (usize, usize, usize)> = HashMap::new();

    for fs in &file_stats {
        stats.lines_total += fs.lines_total;
        stats.lines_code += fs.lines_code;
        stats.lines_blank += fs.lines_blank;
        stats.lines_comment += fs.lines_comment;
        stats.classes += fs.classes;
        stats.functions += fs.functions;
        stats.signals += fs.signals;
        all_function_lengths.extend(&fs.function_lengths);
        all_functions.extend(fs.all_functions.clone());

        if let Some(ref func) = fs.longest_function {
            if let Some(ref current_longest) = stats.longest_function {
                if func.lines > current_longest.lines {
                    stats.longest_function = Some(func.clone());
                }
            } else {
                stats.longest_function = Some(func.clone());
            }
        }

        // Aggregate per-directory stats
        if args.by_dir {
            let dir = fs
                .path
                .parent()
                .and_then(|p| p.strip_prefix(root).ok())
                .map_or_else(
                    || ".".to_string(),
                    |p| {
                        use path_slash::PathExt;
                        let s = p.to_slash_lossy().to_string();
                        if s.is_empty() { ".".to_string() } else { s }
                    },
                );
            let entry = dir_map.entry(dir).or_insert((0, 0, 0));
            entry.0 += 1;
            entry.1 += fs.lines_code;
            entry.2 += fs.functions;
        }
    }

    // Calculate average function length
    if !all_function_lengths.is_empty() {
        let sum: usize = all_function_lengths.iter().sum();
        stats.avg_function_length = sum as f64 / all_function_lengths.len() as f64;
    }

    // Build hotspots (top N longest functions)
    if let Some(n) = args.top {
        all_functions.sort_by(|a, b| b.lines.cmp(&a.lines));
        stats.hotspots = all_functions.into_iter().take(n).collect();
    }

    // Build directory breakdown
    if args.by_dir {
        let mut dirs: Vec<DirStats> = dir_map
            .into_iter()
            .map(|(dir, (files, lines_code, functions))| DirStats {
                directory: dir,
                files,
                lines_code,
                functions,
            })
            .collect();
        dirs.sort_by(|a, b| b.lines_code.cmp(&a.lines_code));
        stats.directories = dirs;
    }

    Ok(stats)
}

fn analyze_file(path: &Path, root: &Path) -> Result<FileStats> {
    let (source, tree) = crate::core::parser::parse_file(path)?;
    let root_node = tree.root_node();

    let rel_path = crate::core::fs::relative_slash(path, root);

    let mut stats = FileStats {
        path: path.to_path_buf(),
        ..Default::default()
    };

    // Count lines
    let lines: Vec<&str> = source.lines().collect();
    stats.lines_total = lines.len();

    for line in &lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            stats.lines_blank += 1;
        } else if trimmed.starts_with('#') {
            stats.lines_comment += 1;
        } else {
            stats.lines_code += 1;
        }
    }

    // Walk AST to count nodes
    walk_node(root_node, &source, &rel_path, &mut stats);

    Ok(stats)
}

fn analyze_source(source: &str, rel_path: &str) -> FileStats {
    let mut stats = FileStats {
        path: PathBuf::from(rel_path),
        ..Default::default()
    };

    // Count lines
    let lines: Vec<&str> = source.lines().collect();
    stats.lines_total = lines.len();

    for line in &lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            stats.lines_blank += 1;
        } else if trimmed.starts_with('#') {
            stats.lines_comment += 1;
        } else {
            stats.lines_code += 1;
        }
    }

    // Walk AST to count nodes
    if let Ok(tree) = crate::core::parser::parse(source) {
        walk_node(tree.root_node(), source, rel_path, &mut stats);
    }

    stats
}

fn collect_branch_stats(root: &Path, branch: &str) -> Result<ProjectStats> {
    // List .gd files in the branch
    let output = Command::new("git")
        .args(["ls-tree", "-r", "--name-only", branch])
        .current_dir(root)
        .output()
        .map_err(|e| miette!("Failed to run git: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(miette!(
            "git ls-tree failed for '{}': {}",
            branch,
            stderr.trim()
        ));
    }

    let file_list = String::from_utf8_lossy(&output.stdout);
    let gd_files: Vec<&str> = file_list
        .lines()
        .filter(|f| {
            Path::new(f)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("gd"))
                && !f.starts_with('.')
        })
        .collect();

    if gd_files.is_empty() {
        return Ok(ProjectStats::default());
    }

    // Get content for each file and analyze
    let file_stats: Vec<FileStats> = gd_files
        .par_iter()
        .filter_map(|rel_path| {
            let git_path = format!("{branch}:{rel_path}");
            let output = Command::new("git")
                .args(["show", &git_path])
                .current_dir(root)
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }
            let content = String::from_utf8(output.stdout).ok()?;
            Some(analyze_source(&content, rel_path))
        })
        .collect();

    // Aggregate
    let mut stats = ProjectStats {
        files: file_stats.len(),
        ..Default::default()
    };

    let mut all_function_lengths = Vec::new();

    for fs in &file_stats {
        stats.lines_total += fs.lines_total;
        stats.lines_code += fs.lines_code;
        stats.lines_blank += fs.lines_blank;
        stats.lines_comment += fs.lines_comment;
        stats.classes += fs.classes;
        stats.functions += fs.functions;
        stats.signals += fs.signals;
        all_function_lengths.extend(&fs.function_lengths);

        if let Some(ref func) = fs.longest_function {
            if let Some(ref current_longest) = stats.longest_function {
                if func.lines > current_longest.lines {
                    stats.longest_function = Some(func.clone());
                }
            } else {
                stats.longest_function = Some(func.clone());
            }
        }
    }

    if !all_function_lengths.is_empty() {
        let sum: usize = all_function_lengths.iter().sum();
        stats.avg_function_length = sum as f64 / all_function_lengths.len() as f64;
    }

    Ok(stats)
}

fn walk_node(node: Node, source: &str, rel_path: &str, stats: &mut FileStats) {
    match node.kind() {
        "class_name_statement" => {
            stats.classes += 1;
        }
        "function_definition" => {
            stats.functions += 1;

            // Calculate function length
            let start_row = node.start_position().row;
            let end_row = node.end_position().row;
            let length = end_row - start_row + 1;
            stats.function_lengths.push(length);

            // Extract function name
            if let Some(name_node) = node.child_by_field_name("name") {
                let func_name = name_node.utf8_text(source.as_bytes()).unwrap_or("unknown");
                let func_info = FunctionInfo {
                    name: func_name.to_string(),
                    lines: length,
                    file: rel_path.to_string(),
                };

                stats.all_functions.push(func_info.clone());

                if let Some(ref longest) = stats.longest_function {
                    if length > longest.lines {
                        stats.longest_function = Some(func_info);
                    }
                } else {
                    stats.longest_function = Some(func_info);
                }
            }
        }
        "signal_statement" => {
            stats.signals += 1;
        }
        _ => {}
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_node(child, source, rel_path, stats);
    }
}

fn output_human(stats: &ProjectStats, show_dirs: bool, top_n: Option<usize>) {
    println!("{}", "Project Statistics".bright_cyan().bold());
    println!("{}", "──────────────────────────────".cyan());
    println!(
        "  Files:          {}",
        stats.files.to_string().bright_white()
    );
    println!(
        "  Lines (total):  {}",
        format_number(stats.lines_total).bright_white()
    );
    println!(
        "  Lines (code):   {}",
        format_number(stats.lines_code).bright_white()
    );
    println!(
        "  Lines (blank):  {}",
        format_number(stats.lines_blank).bright_white()
    );
    println!(
        "  Lines (comment): {}",
        format_number(stats.lines_comment).bright_white()
    );
    println!("{}", "──────────────────────────────".cyan());
    println!(
        "  Classes:        {}",
        stats.classes.to_string().bright_white()
    );
    println!(
        "  Functions:      {}",
        stats.functions.to_string().bright_white()
    );
    println!(
        "  Signals:        {}",
        stats.signals.to_string().bright_white()
    );
    println!("{}", "──────────────────────────────".cyan());
    println!(
        "  Avg function length: {} lines",
        format!("{:.1}", stats.avg_function_length).bright_white()
    );

    if let Some(ref longest) = stats.longest_function {
        println!(
            "  Longest function:    {} ({} lines)",
            longest.name.bright_yellow(),
            longest.lines.to_string().bright_white()
        );
        println!("                       in {}", longest.file.bright_blue());
    }

    if let Some(n) = top_n
        && !stats.hotspots.is_empty()
    {
        println!();
        println!(
            "{}",
            format!("Top {n} Longest Functions").bright_cyan().bold()
        );
        println!("{}", "──────────────────────────────".cyan());
        for (i, func) in stats.hotspots.iter().enumerate() {
            println!(
                "  {}. {} ({} lines) in {}",
                i + 1,
                func.name.bright_yellow(),
                func.lines.to_string().bright_white(),
                func.file.bright_blue()
            );
        }
    }

    if show_dirs && !stats.directories.is_empty() {
        println!();
        println!("{}", "Per-Directory Breakdown".bright_cyan().bold());
        println!("{}", "──────────────────────────────".cyan());
        for dir in &stats.directories {
            println!(
                "  {}: {} file{}, {} LOC, {} fn{}",
                dir.directory.bright_blue(),
                dir.files.to_string().bright_white(),
                if dir.files == 1 { "" } else { "s" },
                format_number(dir.lines_code).bright_white(),
                dir.functions.to_string().bright_white(),
                if dir.functions == 1 { "" } else { "s" },
            );
        }
    }
}

fn format_delta(val: i64) -> String {
    match val.cmp(&0) {
        std::cmp::Ordering::Greater => format!("+{val}"),
        std::cmp::Ordering::Less => format!("{val}"),
        std::cmp::Ordering::Equal => "0".to_string(),
    }
}

fn output_human_diff(diff: &StatsDiff) {
    println!(
        "{} vs {}",
        "Stats Diff".bright_cyan().bold(),
        diff.other_branch.bright_yellow()
    );
    println!("{}", "──────────────────────────────────────".cyan());
    println!(
        "  {:20} {:>10} {:>10} {:>10}",
        "",
        "Current".bright_white().bold(),
        diff.other_branch.bright_white().bold(),
        "Delta".bright_white().bold()
    );
    println!("{}", "──────────────────────────────────────".cyan());

    let rows: Vec<(&str, usize, usize, i64)> = vec![
        (
            "Files",
            diff.current.files,
            diff.other.files,
            diff.delta.files,
        ),
        (
            "Lines (code)",
            diff.current.lines_code,
            diff.other.lines_code,
            diff.delta.lines_code,
        ),
        (
            "Functions",
            diff.current.functions,
            diff.other.functions,
            diff.delta.functions,
        ),
        (
            "Classes",
            diff.current.classes,
            diff.other.classes,
            diff.delta.classes,
        ),
        (
            "Signals",
            diff.current.signals,
            diff.other.signals,
            diff.delta.signals,
        ),
    ];

    for (label, current, other, delta) in rows {
        let delta_str = format_delta(delta);
        let delta_colored = match delta.cmp(&0) {
            std::cmp::Ordering::Greater => delta_str.green().to_string(),
            std::cmp::Ordering::Less => delta_str.red().to_string(),
            std::cmp::Ordering::Equal => delta_str.dimmed().to_string(),
        };
        println!(
            "  {:20} {:>10} {:>10} {:>10}",
            label,
            format_number(current).bright_white(),
            format_number(other).bright_white(),
            delta_colored,
        );
    }
    println!("{}", "──────────────────────────────────────".cyan());
}

fn output_json(stats: &ProjectStats) -> Result<()> {
    let json = serde_json::to_string_pretty(stats)
        .map_err(|e| miette!("Failed to serialize stats to JSON: {e}"))?;
    println!("{json}");
    Ok(())
}

fn format_number(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (count, c) in s.chars().rev().enumerate() {
        if count > 0 && count % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }

    result.chars().rev().collect()
}
