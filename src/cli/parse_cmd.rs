use std::env;

use clap::Args;
use miette::Result;
use owo_colors::OwoColorize;
use serde::Serialize;

use gd_core::fs::collect_gdscript_files;
use gd_core::{ceprintln, cprintln, parser};

use super::check_cmd;

#[derive(Args)]
pub struct ParseArgs {
    /// Files or directories to parse (defaults to current directory)
    pub paths: Vec<String>,
    /// Output format (text or json)
    #[arg(long, default_value = "text")]
    pub format: String,
}

#[derive(Serialize)]
struct ParseOutput {
    files_parsed: u32,
    files_with_errors: u32,
    errors: Vec<check_cmd::ParseError>,
    ok: bool,
}

pub fn exec(args: &ParseArgs) -> Result<()> {
    let cwd = env::current_dir().unwrap_or_default();

    let roots: Vec<std::path::PathBuf> = if args.paths.is_empty() {
        vec![cwd.clone()]
    } else {
        args.paths.iter().map(std::path::PathBuf::from).collect()
    };

    let json_mode = args.format == "json";
    let mut error_count = 0u32;
    let mut checked = 0u32;
    let mut parse_errors = Vec::new();

    for root in &roots {
        let files = collect_gdscript_files(root)?;
        for file in &files {
            checked += 1;
            match parser::parse_file(file) {
                Ok((source, tree)) => {
                    let root_node = tree.root_node();
                    if root_node.has_error() {
                        error_count += 1;
                        if json_mode {
                            check_cmd::collect_errors(
                                &mut root_node.walk(),
                                file,
                                &cwd,
                                &mut parse_errors,
                            );
                        } else {
                            check_cmd::report_errors(&mut root_node.walk(), &source, file);
                        }
                    }
                }
                Err(e) => {
                    error_count += 1;
                    if json_mode {
                        let rel = gd_core::fs::relative_slash(file, &cwd);
                        parse_errors.push(check_cmd::ParseError {
                            file: rel,
                            line: 0,
                            column: 0,
                            message: format!("{e}"),
                        });
                    } else {
                        ceprintln!("{e}");
                    }
                }
            }
        }
    }

    if json_mode {
        let output = ParseOutput {
            files_parsed: checked,
            files_with_errors: error_count,
            errors: parse_errors,
            ok: error_count == 0,
        };
        let json = serde_json::to_string_pretty(&output).map_err(|e| miette::miette!("{e}"))?;
        cprintln!("{json}");
        if !output.ok {
            std::process::exit(1);
        }
        return Ok(());
    }

    if error_count > 0 {
        ceprintln!("\n{checked} files parsed, {error_count} with errors");
        std::process::exit(1);
    }

    cprintln!("{} {} files parsed", "✓".green(), checked);
    Ok(())
}
