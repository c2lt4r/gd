use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{CheckArgs, OutputFormat, run_eval};
use crate::cprintln;

pub fn cmd_check(args: &CheckArgs) -> Result<()> {
    let script = gdscript::generate_check(args.margin, args.max_overlap);
    let result = run_eval(&script)?;
    let parsed: serde_json::Value = serde_json::from_str(&result)
        .map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let ok = parsed["ok"].as_bool().unwrap_or(false);
            let total = parsed["total_parts"].as_u64().unwrap_or(0);
            let floating = parsed["floating"].as_array();
            let clipping = parsed["clipping"].as_array();
            if ok {
                cprintln!(
                    "{} All {total} parts are connected, no clipping (margin={:.1}, max-overlap={:.1}%)",
                    "OK".green().bold(),
                    args.margin,
                    args.max_overlap
                );
            } else {
                if let Some(names) = floating
                    && !names.is_empty()
                {
                    cprintln!(
                        "{} {}/{total} floating parts detected (margin={:.1}):",
                        "WARN".yellow().bold(),
                        names.len(),
                        args.margin
                    );
                    for n in names {
                        if let Some(name) = n.as_str() {
                            cprintln!("  - {}", name.red());
                        }
                    }
                }
                if let Some(pairs) = clipping
                    && !pairs.is_empty()
                {
                    cprintln!(
                        "{} {} clipping pair(s) detected (max-overlap={:.1}%):",
                        "WARN".yellow().bold(),
                        pairs.len(),
                        args.max_overlap
                    );
                    for pair in pairs {
                        let a = pair["part_a"].as_str().unwrap_or("?");
                        let b = pair["part_b"].as_str().unwrap_or("?");
                        let pct = pair["overlap_percent"].as_f64().unwrap_or(0.0);
                        cprintln!(
                            "  - {} {} {} ({:.1}% overlap)",
                            a.red(),
                            "\u{2194}".dimmed(),
                            b.red(),
                            pct
                        );
                    }
                }
                let embedded = parsed["embedded"].as_array();
                if let Some(pairs) = embedded
                    && !pairs.is_empty()
                {
                    cprintln!(
                        "{} {} embedded pair(s) (>50% overlap — likely z-fighting):",
                        "ERROR".red().bold(),
                        pairs.len(),
                    );
                    for pair in pairs {
                        let a = pair["part_a"].as_str().unwrap_or("?");
                        let b = pair["part_b"].as_str().unwrap_or("?");
                        let pct = pair["overlap_percent"].as_f64().unwrap_or(0.0);
                        cprintln!(
                            "  - {} {} {} ({:.1}% overlap)",
                            a.red(),
                            "\u{2194}".dimmed(),
                            b.red(),
                            pct
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
