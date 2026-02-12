use clap::Args;
use miette::Result;
use std::env;

use crate::core::{config::Config, config::find_project_root, fs::collect_gdscript_files, parser};
use crate::lint::matches_ignore_pattern;

#[derive(Args)]
pub struct CheckArgs {
    /// Files or directories to check (defaults to current directory)
    pub paths: Vec<String>,
}

pub fn exec(args: CheckArgs) -> Result<()> {
    let cwd = env::current_dir().unwrap_or_default();
    let config = Config::load(&cwd)?;
    let ignore_base = find_project_root(&cwd).unwrap_or_else(|| cwd.clone());

    let roots = if args.paths.is_empty() {
        vec![cwd]
    } else {
        args.paths.iter().map(std::path::PathBuf::from).collect()
    };

    let mut errors = 0u32;
    let mut checked = 0u32;

    for root in &roots {
        let files = collect_gdscript_files(root)?;
        for file in &files {
            if matches_ignore_pattern(file, &ignore_base, &config.lint.ignore_patterns) {
                continue;
            }
            checked += 1;
            match parser::parse_file(file) {
                Ok((source, tree)) => {
                    let root_node = tree.root_node();
                    if root_node.has_error() {
                        errors += 1;
                        // Walk to find ERROR nodes
                        let mut cursor = root_node.walk();
                        report_errors(&mut cursor, &source, file);
                    }
                }
                Err(e) => {
                    errors += 1;
                    eprintln!("{e}");
                }
            }
        }
    }

    if errors > 0 {
        eprintln!("\n{checked} files checked, {errors} with parse errors");
        std::process::exit(1);
    }

    use owo_colors::OwoColorize;
    println!("{} {} files checked", "✓".green(), checked);
    Ok(())
}

fn report_errors(cursor: &mut tree_sitter::TreeCursor, source: &str, file: &std::path::Path) {
    use owo_colors::OwoColorize;
    loop {
        let node = cursor.node();
        if node.is_error() || node.is_missing() {
            let start = node.start_position();
            let line = source.lines().nth(start.row).unwrap_or("");
            eprintln!(
                "{}:{}:{} {} parse error",
                file.display(),
                start.row + 1,
                start.column + 1,
                "error:".red().bold(),
            );
            eprintln!("  {line}");
        }
        if cursor.goto_first_child() {
            report_errors(cursor, source, file);
            cursor.goto_parent();
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}
