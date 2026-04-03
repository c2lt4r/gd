//! SSR replacement engine — source-level splice with capture substitution.
//!
//! Given a replacement template and captures from the matcher, produces
//! rewritten source code via direct string splicing.  This is the same
//! strategy used by the existing refactoring commands (`rename`,
//! `change-signature`).
//!
//! ## Why source-level splice
//!
//! - Captures store original source text → formatting and comments are
//!   preserved exactly.
//! - The matcher already records byte ranges for every match and capture.
//! - No printer round-trip edge cases.

use std::collections::HashMap;

use super::captures::{Capture, MatchResult};
use super::pattern::{SsrTemplate, is_ident_continue, is_ident_start};

// ═══════════════════════════════════════════════════════════════════════
//  Public API
// ═══════════════════════════════════════════════════════════════════════

/// Render a replacement string by substituting placeholders in the
/// template with captured source text.
#[allow(clippy::implicit_hasher)]
pub fn render_replacement(template: &SsrTemplate, captures: &HashMap<String, Capture>) -> String {
    substitute_placeholders(&template.source, captures)
}

/// Apply all replacements to a source file, producing the new source.
///
/// Overlapping matches are deduplicated (outermost wins).  Splices are
/// applied in reverse byte order so earlier replacements don't shift
/// later offsets.
pub fn apply_replacements(source: &str, matches: &[MatchResult], template: &SsrTemplate) -> String {
    let deduped = deduplicate_overlapping(matches);

    let mut result = source.to_string();
    // Apply in reverse byte order.
    for m in deduped.iter().rev() {
        let raw = render_replacement(template, &m.captures);
        let replacement = reindent(&raw, source, m.matched_range.start);
        result.replace_range(m.matched_range.clone(), &replacement);
    }

    result
}

// ═══════════════════════════════════════════════════════════════════════
//  Placeholder substitution
// ═══════════════════════════════════════════════════════════════════════

/// Walk the template source string, replacing `$name` / `$$name`
/// placeholders with captured source text.
fn substitute_placeholders(template_source: &str, captures: &HashMap<String, Capture>) -> String {
    let mut result = String::with_capacity(template_source.len());
    let bytes = template_source.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] != b'$' {
            result.push(bytes[i] as char);
            i += 1;
            continue;
        }

        // Check for variadic '$$'.
        let variadic = i + 1 < len && bytes[i + 1] == b'$';
        i += if variadic { 2 } else { 1 };

        // Read the identifier name.
        let name_start = i;
        if i >= len || !is_ident_start(bytes[i]) {
            // Not a valid placeholder — emit '$' literally.
            if variadic {
                result.push('$');
            }
            result.push('$');
            continue;
        }
        while i < len && is_ident_continue(bytes[i]) {
            i += 1;
        }
        let name = &template_source[name_start..i];

        // Strip optional `:Type` constraint suffix.
        if !variadic && i < len && bytes[i] == b':' {
            let colon_pos = i;
            i += 1;
            if i < len && is_ident_start(bytes[i]) {
                while i < len && is_ident_continue(bytes[i]) {
                    i += 1;
                }
            } else {
                i = colon_pos; // not a constraint, rewind
            }
        }

        // Substitute the placeholder with captured text.
        match captures.get(name) {
            Some(Capture::Expr(captured)) => {
                result.push_str(&captured.source_text);
            }
            Some(Capture::ArgList(args)) => {
                for (j, arg) in args.iter().enumerate() {
                    if j > 0 {
                        result.push_str(", ");
                    }
                    result.push_str(&arg.source_text);
                }
            }
            None => {
                // Unbound — emit as-is (should have been caught in validation).
                if variadic {
                    result.push('$');
                }
                result.push('$');
                result.push_str(name);
            }
        }
    }

    result
}

// ═══════════════════════════════════════════════════════════════════════
//  Overlap deduplication
// ═══════════════════════════════════════════════════════════════════════

/// Remove overlapping matches, keeping the outermost (longest) one.
///
/// Sorts by start offset ascending, then by range length descending.
/// Skips any match whose range is fully contained within an already
/// accepted match.
fn deduplicate_overlapping(matches: &[MatchResult]) -> Vec<&MatchResult> {
    let mut sorted: Vec<&MatchResult> = matches.iter().collect();
    sorted.sort_by(|a, b| {
        a.matched_range
            .start
            .cmp(&b.matched_range.start)
            .then_with(|| {
                let a_len = a.matched_range.end - a.matched_range.start;
                let b_len = b.matched_range.end - b.matched_range.start;
                b_len.cmp(&a_len) // longer (outer) first
            })
    });

    let mut accepted: Vec<&MatchResult> = Vec::new();
    for m in &sorted {
        let dominated = accepted.iter().any(|prev| {
            prev.matched_range.start <= m.matched_range.start
                && m.matched_range.end <= prev.matched_range.end
        });
        if !dominated {
            accepted.push(m);
        }
    }

    accepted
}

// ═══════════════════════════════════════════════════════════════════════
//  Indentation
// ═══════════════════════════════════════════════════════════════════════

/// Adjust indentation of a multi-line replacement to match the
/// indentation at the match site.
///
/// For single-line replacements this is a no-op.  For multi-line
/// replacements, each line after the first gets the match site's
/// indentation prepended.
fn reindent(replacement: &str, source: &str, match_start: usize) -> String {
    if !replacement.contains('\n') {
        return replacement.to_string();
    }

    // Find indentation at the match site: scan backwards from
    // match_start to the previous newline (or start of file).
    let line_start = source[..match_start].rfind('\n').map_or(0, |pos| pos + 1);
    let indent: &str = &source[line_start..match_start]
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect::<String>();

    let mut result = String::with_capacity(replacement.len() + indent.len() * 4);
    for (idx, line) in replacement.lines().enumerate() {
        if idx > 0 {
            result.push('\n');
            result.push_str(indent);
        }
        result.push_str(line);
    }
    // Preserve trailing newline if present.
    if replacement.ends_with('\n') {
        result.push('\n');
    }

    result
}
