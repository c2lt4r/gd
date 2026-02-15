use clap::Args;
use miette::{Result, miette};
use notify::RecursiveMode;
use notify_debouncer_mini::{DebouncedEventKind, new_debouncer};
use owo_colors::OwoColorize;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

#[derive(Args)]
pub struct WatchArgs {
    /// Paths to watch (defaults to current directory)
    pub paths: Vec<String>,
    /// Auto-format changed files
    #[arg(long)]
    pub fmt: bool,
    /// Run godot --check after changes
    #[arg(long)]
    pub check: bool,
    /// Disable lint (enabled by default)
    #[arg(long)]
    pub no_lint: bool,
}

pub fn exec(args: &WatchArgs) -> Result<()> {
    let cwd =
        std::env::current_dir().map_err(|e| miette!("Failed to get current directory: {e}"))?;

    // Determine watch paths
    let watch_paths: Vec<PathBuf> = if args.paths.is_empty() {
        vec![cwd]
    } else {
        args.paths.iter().map(PathBuf::from).collect()
    };

    // Validate paths
    for path in &watch_paths {
        if !path.exists() {
            return Err(miette!("Path does not exist: {}", path.display()));
        }
    }

    println!(
        "{} {}",
        "watch:".cyan().bold(),
        "Watching for .gd file changes... (press Ctrl+C to stop)".dimmed()
    );
    for path in &watch_paths {
        println!("  {} {}", "→".cyan(), path.display().dimmed());
    }
    println!();

    // Set up debounced file watcher
    let (tx, rx) = mpsc::channel();
    let mut debouncer = new_debouncer(Duration::from_millis(300), tx)
        .map_err(|e| miette!("Failed to create file watcher: {e}"))?;

    // Add watch paths
    for path in &watch_paths {
        debouncer
            .watcher()
            .watch(path, RecursiveMode::Recursive)
            .map_err(|e| miette!("Failed to watch {}: {e}", path.display()))?;
    }

    // Event loop
    loop {
        match rx.recv() {
            Ok(Ok(events)) => {
                // Filter for .gd file changes
                let gd_files: Vec<PathBuf> = events
                    .iter()
                    .filter(|e| matches!(e.kind, DebouncedEventKind::Any))
                    .filter_map(|e| {
                        let path = &e.path;
                        if path.extension().is_some_and(|ext| ext == "gd") && path.exists() {
                            Some(path.clone())
                        } else {
                            None
                        }
                    })
                    .collect();

                if gd_files.is_empty() {
                    continue;
                }

                // Print separator
                println!("{}", "────".dimmed());

                // Show which files changed
                for file in &gd_files {
                    println!("{} {}", "changed:".yellow(), file.display());
                }
                println!();

                // Run formatter if requested
                if args.fmt {
                    let paths: Vec<String> =
                        gd_files.iter().map(|p| p.display().to_string()).collect();
                    match crate::fmt::run_fmt(&paths, false, false) {
                        Ok(()) => {}
                        Err(e) => {
                            eprintln!("{} {e}", "fmt error:".red().bold());
                        }
                    }
                    println!();
                }

                // Run linter unless disabled
                if !args.no_lint {
                    let paths: Vec<String> =
                        gd_files.iter().map(|p| p.display().to_string()).collect();
                    let lint_opts = crate::lint::LintOptions::default();
                    match crate::lint::run_lint(&paths, &lint_opts) {
                        Ok(()) => {}
                        Err(e) => {
                            eprintln!("{} {e}", "lint error:".red().bold());
                        }
                    }
                    println!();
                }

                // Run check if requested
                if args.check {
                    match crate::cli::check_cmd::exec(&crate::cli::check_cmd::CheckArgs {
                        paths: vec![],
                        format: "text".to_string(),
                    }) {
                        Ok(()) => {}
                        Err(e) => {
                            eprintln!("{} {e}", "check error:".red().bold());
                        }
                    }
                    println!();
                }
            }
            Ok(Err(error)) => {
                eprintln!("{} {error}", "watch error:".red().bold());
            }
            Err(_) => {
                // Channel closed, exit
                break;
            }
        }
    }

    Ok(())
}
