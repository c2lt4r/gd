use std::time::Duration;

use clap::Args;
use miette::{Result, miette};
use owo_colors::OwoColorize;

use crate::debug::godot_debug_server::LogEntry;

#[derive(Args)]
pub struct LogArgs {
    /// Show only the last N lines (default: 50)
    #[arg(short, long)]
    pub tail: Option<usize>,
    /// Follow the log in real-time (like tail -f)
    #[arg(short, long)]
    pub follow: bool,
    /// Show only errors and warnings
    #[arg(short, long)]
    pub errors: bool,
    /// Filter lines matching a pattern
    #[arg(short, long)]
    pub grep: Option<String>,
    /// Clear the log buffer
    #[arg(long)]
    pub clear: bool,
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

pub fn exec(args: &LogArgs) -> Result<()> {
    if args.clear {
        let resp = crate::lsp::daemon_client::query_daemon(
            "log_clear",
            serde_json::json!({}),
            Some(Duration::from_secs(2)),
        );
        if resp.is_some() {
            println!("{} Log cleared", "\u{2713}".green());
        } else {
            return Err(miette!(
                "No daemon running. Start a game with: gd run"
            ));
        }
        return Ok(());
    }

    if args.follow {
        return follow_log(args);
    }

    // One-shot query
    let count = args.tail.unwrap_or(50);
    let type_filter = if args.errors { Some("errors") } else { None };

    let entries = query_log(0, count, type_filter)?;

    if entries.is_empty() {
        if args.json {
            println!("[]");
        } else {
            println!("No log output. Run {} first.", "`gd run`".bold());
        }
        return Ok(());
    }

    if args.json {
        let filtered = filter_entries(&entries, args.grep.as_deref());
        println!(
            "{}",
            serde_json::to_string_pretty(&filtered).unwrap_or_default()
        );
    } else {
        print_entries(&entries, args.grep.as_deref());
    }

    Ok(())
}

fn follow_log(args: &LogArgs) -> Result<()> {
    let type_filter = if args.errors { Some("errors") } else { None };

    // First, show the last 20 lines to give context
    let initial = query_log(0, 20, type_filter)?;
    let mut cursor = 0u64;
    if !initial.is_empty() {
        print_entries(&initial, args.grep.as_deref());
        cursor = initial.last().map_or(0, |e| e.seq);
    }

    eprintln!(
        "{} Following game output (Ctrl+C to stop)",
        "\u{25b6}".green(),
    );

    loop {
        let entries = query_log_after(cursor, type_filter)?;
        if !entries.is_empty() {
            cursor = entries.last().map_or(cursor, |e| e.seq);
            print_entries(&entries, args.grep.as_deref());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn query_log(
    after_seq: u64,
    count: usize,
    type_filter: Option<&str>,
) -> Result<Vec<LogEntry>> {
    let mut params = serde_json::json!({"after_seq": after_seq, "count": count});
    if let Some(f) = type_filter {
        params["type_filter"] = serde_json::json!(f);
    }
    let resp = crate::lsp::daemon_client::query_daemon(
        "log_query",
        params,
        Some(Duration::from_secs(2)),
    )
    .ok_or_else(|| miette!("No daemon running. Start a game with: gd run"))?;

    let entries: Vec<LogEntry> = resp
        .get("entries")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    Ok(entries)
}

fn query_log_after(
    after_seq: u64,
    type_filter: Option<&str>,
) -> Result<Vec<LogEntry>> {
    query_log(after_seq, 0, type_filter)
}

fn filter_entries<'a>(entries: &'a [LogEntry], grep: Option<&str>) -> Vec<&'a LogEntry> {
    match grep {
        Some(pattern) => entries
            .iter()
            .filter(|e| e.message.contains(pattern))
            .collect(),
        None => entries.iter().collect(),
    }
}

fn print_entries(entries: &[LogEntry], grep: Option<&str>) {
    for entry in entries {
        if let Some(pattern) = grep
            && !entry.message.contains(pattern)
        {
            continue;
        }
        match entry.r#type.as_str() {
            "error" => eprintln!("{} {}", "ERROR".red().bold(), entry.message),
            "warning" => eprintln!("{} {}", " WARN".yellow().bold(), entry.message),
            _ => println!("{}", entry.message),
        }
    }
}
