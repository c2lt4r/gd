use std::io::{BufRead, BufReader, Seek, SeekFrom};

use clap::Args;
use miette::{Result, miette};
use owo_colors::OwoColorize;

#[derive(Args)]
pub struct LogArgs {
    /// Show only the last N lines
    #[arg(short, long)]
    pub tail: Option<usize>,
    /// Follow the log in real-time (like tail -f)
    #[arg(short, long)]
    pub follow: bool,
    /// Clear the log file
    #[arg(long)]
    pub clear: bool,
}

pub fn exec(args: &LogArgs) -> Result<()> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let root = crate::core::config::find_project_root(&cwd)
        .ok_or_else(|| miette!("Not in a Godot project"))?;
    let log_path = crate::build::log_file_path(&root);

    if args.clear {
        if log_path.exists() {
            std::fs::write(&log_path, "")
                .map_err(|e| miette!("Failed to clear log file: {e}"))?;
            println!("{} Log cleared", "✓".green());
        } else {
            println!("No log file to clear.");
        }
        return Ok(());
    }

    if !log_path.exists() || std::fs::metadata(&log_path).is_ok_and(|m| m.len() == 0) {
        println!("No game log found. Run {} first.", "`gd run`".bold());
        return Ok(());
    }

    if args.follow {
        return follow_log(&log_path);
    }

    let content = std::fs::read_to_string(&log_path)
        .map_err(|e| miette!("Failed to read log file: {e}"))?;

    let lines: Vec<&str> = content.lines().collect();
    let start = if let Some(n) = args.tail {
        lines.len().saturating_sub(n)
    } else {
        0
    };

    for (i, line) in lines[start..].iter().enumerate() {
        let line_num = start + i + 1;
        println!("{line_num:>6}\t{line}");
    }

    Ok(())
}

fn follow_log(log_path: &std::path::Path) -> Result<()> {
    let file = std::fs::File::open(log_path)
        .map_err(|e| miette!("Failed to open log file: {e}"))?;
    let mut reader = BufReader::new(file);

    // Start at the end of the file
    reader
        .seek(SeekFrom::End(0))
        .map_err(|e| miette!("Failed to seek log file: {e}"))?;

    println!(
        "{} Following {} (Ctrl+C to stop)",
        "▶".green(),
        log_path.display()
    );

    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => {
                // No new data — wait and retry
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            Ok(_) => {
                // Trim the trailing newline for clean output
                print!("{line}");
            }
            Err(_) => break,
        }
    }

    Ok(())
}
