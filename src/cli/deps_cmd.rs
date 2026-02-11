use clap::Args;
use miette::{Result, miette};
use owo_colors::OwoColorize;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

#[derive(Args)]
pub struct DepsArgs {
    /// Paths to analyze (defaults to current directory)
    pub paths: Vec<String>,
    /// Output format: tree (default), dot, json
    #[arg(long, default_value = "tree")]
    pub format: String,
    /// Skip circular dependency detection
    #[arg(long)]
    pub no_cycle_check: bool,
}

#[derive(Debug, Serialize)]
struct DepsOutput {
    files: usize,
    dependencies: HashMap<String, Vec<String>>,
}

pub fn exec(args: DepsArgs) -> Result<()> {
    let root = if args.paths.is_empty() {
        std::env::current_dir().map_err(|e| miette!("Failed to get current directory: {e}"))?
    } else {
        PathBuf::from(&args.paths[0])
    };

    let files = crate::core::fs::collect_gdscript_files(&root)?;

    if files.is_empty() {
        println!("No GDScript files found in {}", root.display());
        return Ok(());
    }

    // Build dependency map
    let mut dep_map: HashMap<String, Vec<String>> = HashMap::new();
    for file_path in &files {
        let rel = file_path
            .strip_prefix(&root)
            .unwrap_or(file_path)
            .to_string_lossy()
            .to_string();
        let deps = extract_dependencies(file_path)?;
        dep_map.insert(rel, deps);
    }

    match args.format.as_str() {
        "tree" => output_tree(&dep_map),
        "dot" => output_dot(&dep_map),
        "json" => output_json(&dep_map, files.len())?,
        _ => {
            return Err(miette!(
                "Invalid format '{}'. Use: tree, dot, json",
                args.format
            ));
        }
    }

    if !args.no_cycle_check {
        let cycles = detect_cycles(&dep_map);
        if !cycles.is_empty() {
            eprintln!();
            for cycle in &cycles {
                eprintln!(
                    "{} circular dependency detected: {}",
                    "warning:".yellow().bold(),
                    cycle.join(" -> ")
                );
            }
            return Err(miette!(
                "found {} circular dependenc{}",
                cycles.len(),
                if cycles.len() == 1 { "y" } else { "ies" }
            ));
        }
    }

    Ok(())
}

fn extract_dependencies(path: &Path) -> Result<Vec<String>> {
    let (source, tree) = crate::core::parser::parse_file(path)?;
    let root = tree.root_node();
    let mut deps = Vec::new();

    collect_deps(root, source.as_bytes(), &mut deps);

    deps.sort();
    deps.dedup();
    Ok(deps)
}

fn collect_deps(node: Node, source: &[u8], deps: &mut Vec<String>) {
    match node.kind() {
        "extends_statement" => {
            // Look for string child (path-based extends like `extends "res://..."`)
            // or identifier/type child (class-based extends like `extends Node2D`)
            for i in 0..node.named_child_count() {
                if let Some(child) = node.named_child(i) {
                    match child.kind() {
                        "string" => {
                            // Use utf8_text on the string node directly, don't traverse children
                            if let Ok(text) = child.utf8_text(source) {
                                let trimmed = text.trim_matches('"').trim_matches('\'');
                                if !trimmed.is_empty() {
                                    deps.push(trimmed.to_string());
                                }
                            }
                        }
                        "identifier" | "type" => {
                            if let Ok(text) = child.utf8_text(source) {
                                deps.push(text.to_string());
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        "call" => {
            // Look for preload("...") and load("...") calls
            if let Some(func_node) = node.child_by_field_name("function")
                && let Ok(func_name) = func_node.utf8_text(source)
                && (func_name == "preload" || func_name == "load")
                && let Some(args_node) = node.child_by_field_name("arguments")
            {
                for i in 0..args_node.named_child_count() {
                    if let Some(arg) = args_node.named_child(i)
                        && arg.kind() == "string"
                        && let Ok(text) = arg.utf8_text(source)
                    {
                        let trimmed = text.trim_matches('"').trim_matches('\'');
                        if !trimmed.is_empty() {
                            deps.push(trimmed.to_string());
                        }
                    }
                }
            }
        }
        _ => {}
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_deps(child, source, deps);
    }
}

/// Detect cycles using DFS with white/gray/black coloring.
fn detect_cycles(dep_map: &HashMap<String, Vec<String>>) -> Vec<Vec<String>> {
    let mut cycles = Vec::new();
    let mut white: HashSet<&str> = dep_map.keys().map(|s| s.as_str()).collect();
    let mut gray: HashSet<&str> = HashSet::new();
    let mut black: HashSet<&str> = HashSet::new();
    let mut path: Vec<&str> = Vec::new();

    // Also include dependency targets as potential nodes
    for deps in dep_map.values() {
        for dep in deps {
            white.insert(dep.as_str());
        }
    }

    while let Some(&node) = white.iter().next() {
        dfs_cycle(
            node,
            dep_map,
            &mut white,
            &mut gray,
            &mut black,
            &mut path,
            &mut cycles,
        );
    }

    cycles
}

fn dfs_cycle<'a>(
    node: &'a str,
    dep_map: &'a HashMap<String, Vec<String>>,
    white: &mut HashSet<&'a str>,
    gray: &mut HashSet<&'a str>,
    black: &mut HashSet<&'a str>,
    path: &mut Vec<&'a str>,
    cycles: &mut Vec<Vec<String>>,
) {
    white.remove(node);
    gray.insert(node);
    path.push(node);

    if let Some(deps) = dep_map.get(node) {
        for dep in deps {
            let dep_str = dep.as_str();
            if gray.contains(dep_str) {
                // Found a cycle - extract it from path
                let cycle_start = path.iter().position(|&n| n == dep_str).unwrap();
                let mut cycle: Vec<String> =
                    path[cycle_start..].iter().map(|s| s.to_string()).collect();
                cycle.push(dep.clone());
                cycles.push(cycle);
            } else if white.contains(dep_str) {
                dfs_cycle(dep_str, dep_map, white, gray, black, path, cycles);
            }
        }
    }

    path.pop();
    gray.remove(node);
    black.insert(node);
}

fn output_tree(dep_map: &HashMap<String, Vec<String>>) {
    println!("{}", "Dependencies".bright_cyan().bold());
    println!("{}", "────────────────────────────".cyan());

    let mut files: Vec<_> = dep_map.keys().collect();
    files.sort();

    for (i, file) in files.iter().enumerate() {
        let deps = &dep_map[*file];
        let is_last = i == files.len() - 1;
        let prefix = if is_last { "└──" } else { "├──" };

        if deps.is_empty() {
            println!("  {} {} {}", prefix, file.cyan(), "(no deps)".dimmed());
        } else {
            println!("  {} {}", prefix, file.cyan());
            let continuation = if is_last { "    " } else { "│   " };
            for (j, dep) in deps.iter().enumerate() {
                let dep_prefix = if j == deps.len() - 1 {
                    "└──"
                } else {
                    "├──"
                };
                println!("  {}  {} {}", continuation, dep_prefix, dep.yellow());
            }
        }
    }
}

fn output_dot(dep_map: &HashMap<String, Vec<String>>) {
    println!("digraph dependencies {{");
    println!("  rankdir=LR;");
    println!("  node [shape=box];");

    let mut files: Vec<_> = dep_map.keys().collect();
    files.sort();

    for file in &files {
        let deps = &dep_map[*file];
        for dep in deps {
            println!("  \"{}\" -> \"{}\";", file, dep);
        }
    }

    println!("}}");
}

fn output_json(dep_map: &HashMap<String, Vec<String>>, file_count: usize) -> Result<()> {
    let output = DepsOutput {
        files: file_count,
        dependencies: dep_map.clone(),
    };
    let json = serde_json::to_string_pretty(&output)
        .map_err(|e| miette!("Failed to serialize JSON: {e}"))?;
    println!("{json}");
    Ok(())
}
