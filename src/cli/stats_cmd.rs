use clap::Args;
use miette::{miette, Result};
use owo_colors::OwoColorize;
use rayon::prelude::*;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tree_sitter::Node;

#[derive(Args)]
pub struct StatsArgs {
    /// Paths to analyze (defaults to current directory)
    pub paths: Vec<String>,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: String,
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
}

#[derive(Debug, Clone, Serialize)]
struct FunctionInfo {
    name: String,
    lines: usize,
    file: String,
}

#[derive(Debug, Default)]
struct FileStats {
    lines_total: usize,
    lines_code: usize,
    lines_blank: usize,
    lines_comment: usize,
    classes: usize,
    functions: usize,
    signals: usize,
    function_lengths: Vec<usize>,
    longest_function: Option<FunctionInfo>,
}

pub fn exec(args: StatsArgs) -> Result<()> {
    let root = if args.paths.is_empty() {
        std::env::current_dir().map_err(|e| miette!("Failed to get current directory: {e}"))?
    } else {
        PathBuf::from(&args.paths[0])
    };

    let files = crate::core::fs::collect_gdscript_files(&root)?;

    if files.is_empty() {
        return Err(miette!("No .gd files found in {}", root.display()));
    }

    // Process files in parallel
    let file_stats: Vec<FileStats> = files
        .par_iter()
        .filter_map(|path| analyze_file(path).ok())
        .collect();

    // Aggregate statistics
    let mut stats = ProjectStats {
        files: file_stats.len(),
        ..Default::default()
    };

    let mut all_function_lengths = Vec::new();

    for fs in file_stats {
        stats.lines_total += fs.lines_total;
        stats.lines_code += fs.lines_code;
        stats.lines_blank += fs.lines_blank;
        stats.lines_comment += fs.lines_comment;
        stats.classes += fs.classes;
        stats.functions += fs.functions;
        stats.signals += fs.signals;
        all_function_lengths.extend(fs.function_lengths);

        if let Some(func) = fs.longest_function {
            if let Some(ref current_longest) = stats.longest_function {
                if func.lines > current_longest.lines {
                    stats.longest_function = Some(func);
                }
            } else {
                stats.longest_function = Some(func);
            }
        }
    }

    // Calculate average function length
    if !all_function_lengths.is_empty() {
        let sum: usize = all_function_lengths.iter().sum();
        stats.avg_function_length = sum as f64 / all_function_lengths.len() as f64;
    }

    // Output results
    match args.format.as_str() {
        "json" => output_json(&stats)?,
        "human" => output_human(&stats),
        _ => return Err(miette!("Invalid format: {}", args.format)),
    }

    Ok(())
}

fn analyze_file(path: &Path) -> Result<FileStats> {
    let (source, tree) = crate::core::parser::parse_file(path)?;
    let root_node = tree.root_node();

    let mut stats = FileStats::default();

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
    walk_node(root_node, &source, path, &mut stats);

    Ok(stats)
}

fn walk_node(node: Node, source: &str, path: &Path, stats: &mut FileStats) {
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
                    file: path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string(),
                };

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
        walk_node(child, source, path, stats);
    }
}

fn output_human(stats: &ProjectStats) {
    println!("{}", "Project Statistics".bright_cyan().bold());
    println!("{}", "──────────────────────────────".cyan());
    println!("  Files:          {}", stats.files.to_string().bright_white());
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
    println!("  Classes:        {}", stats.classes.to_string().bright_white());
    println!("  Functions:      {}", stats.functions.to_string().bright_white());
    println!("  Signals:        {}", stats.signals.to_string().bright_white());
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
        println!(
            "                       in {}",
            longest.file.bright_blue()
        );
    }
}

fn output_json(stats: &ProjectStats) -> Result<()> {
    let json = serde_json::to_string_pretty(stats)
        .map_err(|e| miette!("Failed to serialize stats to JSON: {e}"))?;
    println!("{}", json);
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
