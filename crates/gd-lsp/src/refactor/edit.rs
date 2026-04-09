use std::path::Path;

use miette::Result;
use serde::Serialize;

use gd_core::gd_ast;

use super::line_starts;

// ── Output ──────────────────────────────────────────────────────────────────

#[derive(Serialize, Debug)]
pub struct EditOutput {
    pub file: String,
    pub operation: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    pub applied: bool,
    pub lines_changed: u32,
    /// Number of lint diagnostics on the file after the edit.
    /// 0 means clean (exit 0), >0 means applied with warnings (exit 2).
    pub diagnostics: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// How the user identifies the target AST node(s) for `replace`.
pub enum ReplaceTarget {
    /// `--name <sym>` (+ optional `--body`)
    Name { name: String, body_only: bool },
    /// `--line <N>` — single AST node at this line
    Line(usize),
    /// `--line <N>-<M>` — AST nodes in line range
    LineRange { start: usize, end: usize },
}

/// How the user identifies the anchor for `insert`.
pub enum InsertAnchor {
    /// `--name <sym>`
    Name(String),
    /// `--line <N>`
    Line(usize),
}

/// Where to insert relative to the anchor.
pub enum InsertPosition {
    Before,
    After,
    /// First child of anchor (function body or class body)
    Into,
    /// Last child of anchor (function body or class body)
    IntoEnd,
}

// ── Shared helpers ──────────────────────────────────────────────────────────

/// Validate that an edit didn't introduce new parse errors compared to the original.
fn validate_no_new_errors(original: &str, edited: &str) -> Result<()> {
    super::validate_no_new_errors(original, edited)
}

/// Validate that the mutation didn't introduce new lint diagnostics.
/// Returns `Err` with exit-code-2 semantics if new diagnostics were introduced.
fn validate_no_new_diagnostics(original: &str, mutated: &str, project_root: &Path) -> Result<u32> {
    let original_count = super::mutation::lint_diagnostic_count(original, project_root);
    let mutated_count = super::mutation::lint_diagnostic_count(mutated, project_root);
    if mutated_count > original_count {
        return Err(miette::miette!(
            "mutation introduced {} new lint diagnostic{} ({} → {})",
            mutated_count - original_count,
            if mutated_count - original_count == 1 {
                ""
            } else {
                "s"
            },
            original_count,
            mutated_count,
        ));
    }
    Ok(mutated_count)
}

/// Persist a single-file mutation through the [`MutationSet`] pipeline.
fn persist(file: &Path, content: &str, project_root: &Path) -> Result<u32> {
    let mut ms = super::mutation::MutationSet::new();
    ms.insert(file.to_path_buf(), content.to_string());
    let result = super::mutation::commit(&ms, project_root)?;
    Ok(result.diagnostics.get(file).copied().unwrap_or(0))
}

/// Format GDScript source using the project's formatter config.
fn format_source(source: &str, project_root: &Path) -> Result<String> {
    let config = gd_core::config::Config::load(project_root)?;
    let tree = gd_core::parser::parse(source)?;
    let mut printer = gd_fmt::printer::Printer::from_config(&config.fmt);
    printer.format(&tree.root_node(), source);
    let formatted = printer.finish();
    if let Some(err) = gd_fmt::verify_format(source, &formatted, &config.fmt) {
        return Err(miette::miette!("format safety check failed: {err}"));
    }
    Ok(formatted)
}

// ── replace-body ────────────────────────────────────────────────────────────

// ── insert ──────────────────────────────────────────────────────────────────

// ── insert-into (class body) ────────────────────────────────────────────────

// ── replace-symbol ──────────────────────────────────────────────────────────

// ── edit-range ──────────────────────────────────────────────────────────────

pub fn replace(
    file: &Path,
    target: &ReplaceTarget,
    class: Option<&str>,
    new_content: &str,
    no_format: bool,
    project_root: &Path,
) -> Result<EditOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let rel = gd_core::fs::relative_slash(file, project_root);
    let ctx = EditCtx {
        file,
        source: &source,
        rel,
        no_format,
        project_root,
    };

    match target {
        ReplaceTarget::Name { name, body_only } => {
            replace_by_name(&ctx, name, *body_only, class, new_content)
        }
        ReplaceTarget::Line(line) => replace_by_line_range(&ctx, *line, *line, new_content),
        ReplaceTarget::LineRange { start, end } => {
            replace_by_line_range(&ctx, *start, *end, new_content)
        }
    }
}

/// Common context threaded through replace/insert helpers.
struct EditCtx<'a> {
    file: &'a Path,
    source: &'a str,
    rel: String,
    no_format: bool,
    project_root: &'a Path,
}

impl EditCtx<'_> {
    fn finish(
        &self,
        owned: &gd_core::ast_owned::OwnedFile,
        operation: &'static str,
        symbol: Option<String>,
    ) -> Result<EditOutput> {
        let result = gd_core::printer::print_file(owned, self.source);
        validate_no_new_errors(self.source, &result)?;
        let final_source = if self.no_format {
            result
        } else {
            format_source(&result, self.project_root)?
        };
        let lines_changed = diff_line_count(self.source, &final_source);
        let diagnostics = persist(self.file, &final_source, self.project_root)?;
        Ok(EditOutput {
            file: self.rel.clone(),
            operation,
            symbol,
            applied: true,
            lines_changed,
            diagnostics,
            warnings: vec![],
        })
    }
}

fn replace_by_name(
    ctx: &EditCtx<'_>,
    name: &str,
    body_only: bool,
    class: Option<&str>,
    new_content: &str,
) -> Result<EditOutput> {
    let tree =
        gd_core::parser::parse(ctx.source).map_err(|e| miette::miette!("parse error: {e}"))?;
    let gd_file = gd_ast::convert(&tree, ctx.source);
    let mut owned = gd_core::ast_owned::OwnedFile::from_borrowed(&gd_file);

    // Special case: class_name replaces entire file
    if owned.class_name.as_deref() == Some(name) && class.is_none() && !body_only {
        let parse_input = new_content.to_string();
        if let Ok(new_tree) = gd_core::parser::parse(&parse_input) {
            let new_file = gd_ast::convert(&new_tree, &parse_input);
            let new_owned = gd_core::ast_owned::OwnedFile::from_borrowed(&new_file);
            owned.class_name = new_owned.class_name;
            owned.extends = new_owned.extends;
            owned.is_tool = new_owned.is_tool;
            owned.declarations = new_owned.declarations;
            for d in &mut owned.declarations {
                d.clear_spans();
            }
        }
        owned.span = None;
        return ctx.finish(&owned, "replace", Some(name.to_string()));
    }

    if body_only {
        replace_body_ast(&mut owned, ctx, name, class, new_content)
    } else {
        replace_decl_ast(&mut owned, ctx, name, class, new_content)
    }
}

fn replace_body_ast(
    owned: &mut gd_core::ast_owned::OwnedFile,
    ctx: &EditCtx<'_>,
    name: &str,
    class: Option<&str>,
    new_content: &str,
) -> Result<EditOutput> {
    let first_content_line = new_content
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    if first_content_line.starts_with("func ") || first_content_line.starts_with("static func ") {
        return Err(miette::miette!(
            "input appears to contain a function signature (`{}`); \
             replace --body expects only the body (indented statements), not the signature",
            first_content_line.chars().take(60).collect::<String>()
        ));
    }

    let decls = if let Some(cls) = class {
        let class_idx = gd_core::ast_owned::OwnedDecl::find_by_name(&owned.declarations, cls)
            .ok_or_else(|| miette::miette!("class '{cls}' not found in {}", ctx.rel))?;
        let gd_core::ast_owned::OwnedDecl::Class(ref mut c) = owned.declarations[class_idx] else {
            return Err(miette::miette!("'{cls}' is not a class"));
        };
        c.span = None;
        &mut c.declarations as &mut Vec<_>
    } else {
        &mut owned.declarations
    };

    let idx = gd_core::ast_owned::OwnedDecl::find_by_name(decls, name)
        .ok_or_else(|| miette::miette!("symbol '{name}' not found in {}", ctx.rel))?;

    let gd_core::ast_owned::OwnedDecl::Func(ref mut func) = decls[idx] else {
        return Err(miette::miette!(
            "'{name}' is not a function — --body only works on functions"
        ));
    };

    let new_stmts = parse_replacement_stmts(new_content)?;
    func.body = new_stmts;
    func.span = None;
    owned.span = None;

    ctx.finish(owned, "replace", Some(name.to_string()))
}

fn replace_decl_ast(
    owned: &mut gd_core::ast_owned::OwnedFile,
    ctx: &EditCtx<'_>,
    name: &str,
    class: Option<&str>,
    new_content: &str,
) -> Result<EditOutput> {
    let new_decls = parse_replacement_decls(new_content)?;

    if let Some(cls) = class {
        let class_idx = gd_core::ast_owned::OwnedDecl::find_by_name(&owned.declarations, cls)
            .ok_or_else(|| miette::miette!("class '{cls}' not found in {}", ctx.rel))?;
        let gd_core::ast_owned::OwnedDecl::Class(ref mut c) = owned.declarations[class_idx] else {
            return Err(miette::miette!("'{cls}' is not a class"));
        };
        let idx = gd_core::ast_owned::OwnedDecl::find_by_name(&c.declarations, name)
            .ok_or_else(|| miette::miette!("symbol '{name}' not found in class '{cls}'"))?;
        splice_vec(&mut c.declarations, idx, idx, new_decls);
        c.span = None;
    } else {
        let idx = gd_core::ast_owned::OwnedDecl::find_by_name(&owned.declarations, name)
            .ok_or_else(|| miette::miette!("symbol '{name}' not found in {}", ctx.rel))?;
        splice_vec(&mut owned.declarations, idx, idx, new_decls);
    }

    owned.span = None;
    ctx.finish(owned, "replace", Some(name.to_string()))
}

fn replace_by_line_range(
    ctx: &EditCtx<'_>,
    start_line: usize,
    end_line: usize,
    new_content: &str,
) -> Result<EditOutput> {
    if start_line == 0 || end_line == 0 {
        return Err(miette::miette!("line numbers are 1-based"));
    }
    if start_line > end_line {
        return Err(miette::miette!(
            "start-line ({start_line}) must be <= end-line ({end_line})"
        ));
    }

    let effectively_empty =
        ctx.source.is_empty() || ctx.source.chars().all(|c| c == '\n' || c == '\r');
    if effectively_empty && start_line == 1 && end_line == 1 {
        return edit_range_into_empty(
            ctx.source,
            new_content,
            ctx.no_format,
            ctx.project_root,
            ctx.file,
            &ctx.rel,
        );
    }

    let starts = line_starts(ctx.source);
    let total_lines = starts.len();

    if start_line > total_lines {
        return Err(miette::miette!(
            "start-line {start_line} exceeds file length ({total_lines} lines)"
        ));
    }

    let start_byte = starts[start_line - 1];
    let end_byte = if end_line >= total_lines {
        ctx.source.len()
    } else {
        starts[end_line]
    };

    let tree =
        gd_core::parser::parse(ctx.source).map_err(|e| miette::miette!("parse error: {e}"))?;
    let gd_file = gd_ast::convert(&tree, ctx.source);
    let mut owned = gd_core::ast_owned::OwnedFile::from_borrowed(&gd_file);

    let target = find_range_target(
        &owned.declarations,
        start_byte,
        end_byte,
        start_line,
        end_line,
        &starts,
    )?;
    apply_range_target(&mut owned, target, new_content)?;

    ctx.finish(&owned, "replace", None)
}

// ── Unified insert ──────────────────────────────────────────────────────────

pub fn insert_cmd(
    file: &Path,
    anchor: &InsertAnchor,
    position: &InsertPosition,
    class: Option<&str>,
    content: &str,
    no_format: bool,
    project_root: &Path,
) -> Result<EditOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let rel = gd_core::fs::relative_slash(file, project_root);
    let ctx = EditCtx {
        file,
        source: &source,
        rel,
        no_format,
        project_root,
    };

    let tree = gd_core::parser::parse(&source).map_err(|e| miette::miette!("parse error: {e}"))?;
    let gd_file = gd_ast::convert(&tree, &source);
    let mut owned = gd_core::ast_owned::OwnedFile::from_borrowed(&gd_file);

    let (decls, anchor_idx, anchor_name) =
        resolve_anchor(&mut owned, anchor, class, &ctx.rel, &source)?;

    match position {
        InsertPosition::Before => {
            let new_decls = parse_replacement_decls(content)?;
            insert_vec(decls, anchor_idx, new_decls);
        }
        InsertPosition::After => {
            let new_decls = parse_replacement_decls(content)?;
            insert_vec(decls, anchor_idx + 1, new_decls);
        }
        InsertPosition::Into | InsertPosition::IntoEnd => {
            let at_end = matches!(position, InsertPosition::IntoEnd);
            insert_into_container(&mut decls[anchor_idx], content, at_end, &anchor_name)?;
        }
    }

    owned.span = None;

    let op = match position {
        InsertPosition::Before => "insert-before",
        InsertPosition::After => "insert-after",
        InsertPosition::Into | InsertPosition::IntoEnd => "insert-into",
    };

    ctx.finish(&owned, op, Some(anchor_name))
}

/// Resolve an `InsertAnchor` to the mutable declaration list and index.
fn resolve_anchor<'a>(
    owned: &'a mut gd_core::ast_owned::OwnedFile,
    anchor: &InsertAnchor,
    class: Option<&str>,
    rel: &str,
    source: &str,
) -> Result<(&'a mut Vec<gd_core::ast_owned::OwnedDecl>, usize, String)> {
    if let Some(cls) = class {
        let class_idx = gd_core::ast_owned::OwnedDecl::find_by_name(&owned.declarations, cls)
            .ok_or_else(|| miette::miette!("class '{cls}' not found in {rel}"))?;
        let gd_core::ast_owned::OwnedDecl::Class(ref mut c) = owned.declarations[class_idx] else {
            return Err(miette::miette!("'{cls}' is not a class"));
        };
        c.span = None;
        let (idx, name) = match anchor {
            InsertAnchor::Name(n) => {
                let i = gd_core::ast_owned::OwnedDecl::find_by_name(&c.declarations, n)
                    .ok_or_else(|| miette::miette!("symbol '{n}' not found in class '{cls}'"))?;
                (i, n.clone())
            }
            InsertAnchor::Line(line) => {
                let starts = line_starts(source);
                if *line == 0 || *line > starts.len() {
                    return Err(miette::miette!("line {line} out of range"));
                }
                let byte = starts[*line - 1];
                let i = gd_core::ast_owned::OwnedDecl::find_at_byte(&c.declarations, byte)
                    .ok_or_else(|| miette::miette!("no declaration found at line {line}"))?;
                let n = c.declarations[i].name().to_string();
                (i, n)
            }
        };
        Ok((&mut c.declarations, idx, name))
    } else {
        let (idx, name) = match anchor {
            InsertAnchor::Name(n) => {
                let i = gd_core::ast_owned::OwnedDecl::find_by_name(&owned.declarations, n)
                    .ok_or_else(|| miette::miette!("symbol '{n}' not found in {rel}"))?;
                (i, n.clone())
            }
            InsertAnchor::Line(line) => {
                let starts = line_starts(source);
                if *line == 0 || *line > starts.len() {
                    return Err(miette::miette!("line {line} out of range"));
                }
                let byte = starts[*line - 1];
                let i = gd_core::ast_owned::OwnedDecl::find_at_byte(&owned.declarations, byte)
                    .ok_or_else(|| miette::miette!("no declaration found at line {line}"))?;
                let n = owned.declarations[i].name().to_string();
                (i, n)
            }
        };
        Ok((&mut owned.declarations, idx, name))
    }
}

/// Insert content into a container declaration (function body or class body).
fn insert_into_container(
    decl: &mut gd_core::ast_owned::OwnedDecl,
    content: &str,
    at_end: bool,
    anchor_name: &str,
) -> Result<()> {
    use gd_core::ast_owned::OwnedDecl;
    match decl {
        OwnedDecl::Func(f) => {
            let new_stmts = parse_replacement_stmts(content)?;
            let pos = if at_end { f.body.len() } else { 0 };
            for (i, stmt) in new_stmts.into_iter().enumerate() {
                f.body.insert(pos + i, stmt);
            }
            f.span = None;
            for s in &mut f.body {
                s.clear_spans();
            }
            Ok(())
        }
        OwnedDecl::Class(c) => {
            let new_decls = parse_replacement_decls(content)?;
            let pos = if at_end { c.declarations.len() } else { 0 };
            for (i, d) in new_decls.into_iter().enumerate() {
                c.declarations.insert(pos + i, d);
            }
            c.span = None;
            for d in &mut c.declarations {
                d.clear_spans();
            }
            Ok(())
        }
        _ => Err(miette::miette!(
            "'{anchor_name}' is not a function or class — --into requires a container"
        )),
    }
}

fn edit_range_into_empty(
    source: &str,
    new_content: &str,
    no_format: bool,
    project_root: &Path,
    file: &Path,
    rel: &str,
) -> Result<EditOutput> {
    let trimmed = new_content.trim_end();
    let mut result = String::from(trimmed);
    if !result.ends_with('\n') {
        result.push('\n');
    }
    validate_no_new_errors("", &result)?;
    let final_source = if no_format {
        result
    } else {
        format_source(&result, project_root)?
    };
    let lines_changed = diff_line_count(source, &final_source);
    validate_no_new_diagnostics(source, &final_source, project_root)?;
    let diagnostics = persist(file, &final_source, project_root)?;
    Ok(EditOutput {
        file: rel.to_string(),
        operation: "replace-range",
        symbol: None,
        applied: true,
        lines_changed,
        diagnostics,
        warnings: vec![],
    })
}

/// Result of locating which AST nodes a byte range covers.
#[derive(Clone, Copy)]
enum RangeTarget {
    /// Range covers complete top-level declarations `[first..=last]`.
    Decls { first: usize, last: usize },
    /// Range falls inside a single declaration at `decl_idx` and covers
    /// complete statements `[first..=last]` within that declaration's body.
    BodyStmts {
        decl_idx: usize,
        first: usize,
        last: usize,
    },
    /// Range covers only blank lines / comments — no AST nodes matched.
    Empty { insert_at: usize },
}
/// Recursively locate which AST nodes a byte range covers.
fn find_range_target(
    decls: &[gd_core::ast_owned::OwnedDecl],
    start_byte: usize,
    end_byte: usize,
    start_line: usize,
    end_line: usize,
    line_starts: &[usize],
) -> Result<RangeTarget> {
    let mut first_idx: Option<usize> = None;
    let mut last_idx: Option<usize> = None;
    let mut containing: Option<usize> = None;

    for (i, decl) in decls.iter().enumerate() {
        let Some(span) = decl.span() else { continue };
        let ds = span.start;
        let de = span.end;

        if de <= start_byte || ds >= end_byte {
            continue;
        }

        if ds >= start_byte && de <= end_byte {
            if first_idx.is_none() {
                first_idx = Some(i);
            }
            last_idx = Some(i);
            continue;
        }

        // Range is fully inside this declaration — try descending
        if ds <= start_byte && de >= end_byte {
            if let gd_core::ast_owned::OwnedDecl::Func(f) = &decls[i] {
                // Only descend if the range starts at or after the first
                // body statement's line (not at the function signature).
                let body_line = f
                    .body
                    .first()
                    .and_then(gd_core::ast_owned::OwnedStmt::span)
                    .map(|s| byte_to_line_1based(line_starts, s.start));
                if body_line.is_some_and(|bl| start_line >= bl) {
                    containing = Some(i);
                    continue;
                }
            }
            // Range overlaps the signature — treat as partial overlap
            let node_start = byte_to_line_1based(line_starts, ds);
            let node_end = byte_to_line_1based(line_starts, de.saturating_sub(1));
            return Err(miette::miette!(
                "range {start_line}-{end_line} splits a declaration \
                 (lines {node_start}-{node_end}); \
                 adjust to align with declaration boundaries"
            ));
        }

        let node_start = byte_to_line_1based(line_starts, ds);
        let node_end = byte_to_line_1based(line_starts, de.saturating_sub(1));
        return Err(miette::miette!(
            "range {start_line}-{end_line} splits a declaration \
             (lines {node_start}-{node_end}); \
             adjust to align with declaration boundaries"
        ));
    }

    if let (Some(first), Some(last)) = (first_idx, last_idx) {
        return Ok(RangeTarget::Decls { first, last });
    }

    if let Some(idx) = containing {
        let body = match &decls[idx] {
            gd_core::ast_owned::OwnedDecl::Func(f) => &f.body,
            _ => unreachable!("only Func sets containing"),
        };

        return find_stmt_range(
            body,
            start_byte,
            end_byte,
            start_line,
            end_line,
            line_starts,
        )
        .map(|target| match target {
            RangeTarget::Decls { first, last } => RangeTarget::BodyStmts {
                decl_idx: idx,
                first,
                last,
            },
            other => other,
        });
    }

    let insert_at = decls
        .iter()
        .position(|d| d.span().is_some_and(|s| s.start >= start_byte))
        .unwrap_or(decls.len());
    Ok(RangeTarget::Empty { insert_at })
}
/// Find which statements in a body a byte range covers.
fn find_stmt_range(
    stmts: &[gd_core::ast_owned::OwnedStmt],
    start_byte: usize,
    end_byte: usize,
    start_line: usize,
    end_line: usize,
    line_starts: &[usize],
) -> Result<RangeTarget> {
    let mut first_idx: Option<usize> = None;
    let mut last_idx: Option<usize> = None;

    for (i, stmt) in stmts.iter().enumerate() {
        let Some(span) = stmt.span() else { continue };
        let ss = span.start;
        let se = span.end;

        if se <= start_byte || ss >= end_byte {
            continue;
        }

        if ss >= start_byte && se <= end_byte {
            if first_idx.is_none() {
                first_idx = Some(i);
            }
            last_idx = Some(i);
            continue;
        }

        let node_start = byte_to_line_1based(line_starts, ss);
        let node_end = byte_to_line_1based(line_starts, se.saturating_sub(1));
        return Err(miette::miette!(
            "range {start_line}-{end_line} splits a statement \
             (lines {node_start}-{node_end}); \
             adjust to align with statement boundaries"
        ));
    }

    if let (Some(first), Some(last)) = (first_idx, last_idx) {
        Ok(RangeTarget::Decls { first, last })
    } else {
        let insert_at = stmts
            .iter()
            .position(|s| s.span().is_some_and(|sp| sp.start >= start_byte))
            .unwrap_or(stmts.len());
        Ok(RangeTarget::Empty { insert_at })
    }
}

/// Parse replacement content as top-level declarations.
fn parse_replacement_decls(content: &str) -> Result<Vec<gd_core::ast_owned::OwnedDecl>> {
    let needs_wrapper = !content.trim_start().starts_with("extends ")
        && !content.trim_start().starts_with("class_name ")
        && !content.trim_start().starts_with("@tool");
    let parse_input = if needs_wrapper {
        format!("extends Node\n{content}")
    } else {
        content.to_string()
    };
    let tree = gd_core::parser::parse(&parse_input)
        .map_err(|e| miette::miette!("replacement content has parse errors: {e}"))?;
    let gd_file = gd_ast::convert(&tree, &parse_input);
    let owned = gd_core::ast_owned::OwnedFile::from_borrowed(&gd_file);
    let mut decls = owned.declarations;
    for d in &mut decls {
        d.clear_spans();
    }
    Ok(decls)
}
/// Parse replacement content as function body statements.
/// If content has no indentation, auto-indents with one tab so it parses
/// correctly inside the wrapper function.
fn parse_replacement_stmts(content: &str) -> Result<Vec<gd_core::ast_owned::OwnedStmt>> {
    // Ensure content is indented so it parses as function body
    let indented = if content.lines().any(|l| !l.trim().is_empty())
        && content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .all(|l| !l.starts_with('\t') && !l.starts_with(' '))
    {
        // No indentation at all — add one tab to each non-empty line
        content
            .lines()
            .map(|l| {
                if l.trim().is_empty() {
                    String::new()
                } else {
                    format!("\t{l}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        content.to_string()
    };

    let parse_input = format!("extends Node\nfunc _wrapper():\n{indented}");
    let tree = gd_core::parser::parse(&parse_input)
        .map_err(|e| miette::miette!("replacement content has parse errors: {e}"))?;
    let gd_file = gd_ast::convert(&tree, &parse_input);
    let owned = gd_core::ast_owned::OwnedFile::from_borrowed(&gd_file);
    for decl in &owned.declarations {
        if let gd_core::ast_owned::OwnedDecl::Func(f) = decl
            && f.name == "_wrapper"
        {
            let mut stmts = f.body.clone();
            for s in &mut stmts {
                s.clear_spans();
            }
            return Ok(stmts);
        }
    }
    Err(miette::miette!(
        "failed to parse replacement as function body statements"
    ))
}
/// Splice items into a `Vec`, replacing indices `first..=last`.
fn splice_vec<T: Clone>(vec: &mut Vec<T>, first: usize, last: usize, new_items: Vec<T>) {
    let mut result = Vec::new();
    result.extend(vec[..first].iter().cloned());
    result.extend(new_items);
    result.extend(vec[last + 1..].iter().cloned());
    *vec = result;
}
/// Insert items into a `Vec` at position `at`.
fn insert_vec<T: Clone>(vec: &mut Vec<T>, at: usize, new_items: Vec<T>) {
    let mut result = Vec::new();
    result.extend(vec[..at].iter().cloned());
    result.extend(new_items);
    result.extend(vec[at..].iter().cloned());
    *vec = result;
}
/// Apply a `RangeTarget` by parsing replacement content and splicing into the AST.
fn apply_range_target(
    owned: &mut gd_core::ast_owned::OwnedFile,
    target: RangeTarget,
    new_content: &str,
) -> Result<()> {
    match target {
        RangeTarget::Decls { first, last } => {
            let new_decls = parse_replacement_decls(new_content)?;
            splice_vec(&mut owned.declarations, first, last, new_decls);
        }
        RangeTarget::BodyStmts {
            decl_idx,
            first,
            last,
        } => {
            let new_stmts = parse_replacement_stmts(new_content)?;
            let body = match &mut owned.declarations[decl_idx] {
                gd_core::ast_owned::OwnedDecl::Func(f) => {
                    f.span = None;
                    &mut f.body
                }
                _ => unreachable!("find_range_target only returns BodyStmts for Func"),
            };
            splice_vec(body, first, last, new_stmts);
            // Clear spans on all remaining body statements so the printer
            // regenerates them with correct indentation (the function's
            // span was cleared, so verbatim spans would read garbage).
            for s in body.iter_mut() {
                s.clear_spans();
            }
        }
        RangeTarget::Empty { insert_at } => {
            let new_decls = parse_replacement_decls(new_content)?;
            insert_vec(&mut owned.declarations, insert_at, new_decls);
        }
    }
    owned.span = None;
    Ok(())
}

/// Convert a byte offset to a 1-based line number given `line_starts`.
fn byte_to_line_1based(starts: &[usize], byte: usize) -> usize {
    match starts.binary_search(&byte) {
        Ok(i) => i + 1,
        Err(i) => i,
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Count the number of lines that differ between two strings.
fn diff_line_count(old: &str, new: &str) -> u32 {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let max = old_lines.len().max(new_lines.len());
    let mut changed = 0u32;
    for i in 0..max {
        let a = old_lines.get(i).copied().unwrap_or("");
        let b = new_lines.get(i).copied().unwrap_or("");
        if a != b {
            changed += 1;
        }
    }
    // Also count extra lines in longer file
    changed
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup(files: &[(&str, &str)]) -> TempDir {
        let temp = tempfile::Builder::new()
            .prefix("gdtest")
            .tempdir()
            .expect("create temp dir");
        fs::write(
            temp.path().join("project.godot"),
            "[application]\nconfig/name=\"test\"\n",
        )
        .expect("write project.godot");
        for (name, content) in files {
            fs::write(temp.path().join(name), content).expect("write file");
        }
        temp
    }

    // ── replace ─────────────────────────────────────────────────────────

    #[test]
    fn replace_name_whole_decl() {
        let temp = setup(&[(
            "player.gd",
            "extends Node\n\n\nvar speed = 10\n\n\nfunc _ready():\n\tpass\n",
        )]);
        let file = temp.path().join("player.gd");
        let result = replace(
            &file,
            &ReplaceTarget::Name {
                name: "speed".to_string(),
                body_only: false,
            },
            None,
            "var speed = 99\n",
            true,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.operation, "replace");
        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("var speed = 99"));
        assert!(!content.contains("var speed = 10"));
        assert!(content.contains("func _ready():"));
    }

    #[test]
    fn replace_name_function() {
        let temp = setup(&[("player.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);
        let file = temp.path().join("player.gd");
        let result = replace(
            &file,
            &ReplaceTarget::Name {
                name: "_ready".to_string(),
                body_only: false,
            },
            None,
            "func _ready():\n\tprint(\"hello\")\n",
            true,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("print(\"hello\")"));
        assert!(!content.contains("\tpass"));
    }

    #[test]
    fn replace_name_body_only() {
        let temp = setup(&[("player.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);
        let file = temp.path().join("player.gd");
        let result = replace(
            &file,
            &ReplaceTarget::Name {
                name: "_ready".to_string(),
                body_only: true,
            },
            None,
            "\tprint(\"hello\")\n",
            true,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("func _ready():"));
        assert!(content.contains("print(\"hello\")"));
        assert!(!content.contains("\tpass"));
    }

    #[test]
    fn replace_name_body_reindents_from_zero() {
        let temp = setup(&[("player.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);
        let file = temp.path().join("player.gd");
        let result = replace(
            &file,
            &ReplaceTarget::Name {
                name: "_ready".to_string(),
                body_only: true,
            },
            None,
            "print(\"a\")\nprint(\"b\")\n",
            true,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("\tprint(\"a\")"));
        assert!(content.contains("\tprint(\"b\")"));
    }

    #[test]
    fn replace_name_body_rejects_non_function() {
        let temp = setup(&[("player.gd", "extends Node\nvar speed = 10\n")]);
        let file = temp.path().join("player.gd");
        let result = replace(
            &file,
            &ReplaceTarget::Name {
                name: "speed".to_string(),
                body_only: true,
            },
            None,
            "\t42\n",
            true,
            temp.path(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn replace_name_body_rejects_signature_in_input() {
        let temp = setup(&[("player.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);
        let file = temp.path().join("player.gd");
        let result = replace(
            &file,
            &ReplaceTarget::Name {
                name: "_ready".to_string(),
                body_only: true,
            },
            None,
            "func _ready():\n\tprint(1)\n",
            true,
            temp.path(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn replace_name_in_class() {
        let temp = setup(&[(
            "player.gd",
            "extends Node\n\n\nclass Inner:\n\tfunc foo():\n\t\tpass\n",
        )]);
        let file = temp.path().join("player.gd");
        let result = replace(
            &file,
            &ReplaceTarget::Name {
                name: "foo".to_string(),
                body_only: true,
            },
            Some("Inner"),
            "\t\tprint(1)\n",
            true,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("print(1)"));
    }

    #[test]
    fn replace_line_single() {
        let temp = setup(&[(
            "player.gd",
            "extends Node\nvar a = 1\nvar b = 2\nvar c = 3\n",
        )]);
        let file = temp.path().join("player.gd");
        let result = replace(
            &file,
            &ReplaceTarget::Line(2),
            None,
            "var x = 10\n",
            true,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("var x = 10"));
        assert!(!content.contains("var a = 1"));
        assert!(content.contains("var b = 2"));
    }

    #[test]
    fn replace_line_range() {
        let temp = setup(&[(
            "player.gd",
            "extends Node\nvar a = 1\nvar b = 2\nvar c = 3\n",
        )]);
        let file = temp.path().join("player.gd");
        let result = replace(
            &file,
            &ReplaceTarget::LineRange { start: 2, end: 3 },
            None,
            "var x = 10\nvar y = 20\n",
            true,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("var x = 10"));
        assert!(content.contains("var y = 20"));
        assert!(!content.contains("var a = 1"));
        assert!(!content.contains("var b = 2"));
        assert!(content.contains("var c = 3"));
    }

    #[test]
    fn replace_line_range_body_stmts() {
        let temp = setup(&[(
            "player.gd",
            "extends Node\n\n\nfunc _ready():\n\tvar x = 1\n\tvar y = 2\n\tprint(x)\n",
        )]);
        let file = temp.path().join("player.gd");
        let result = replace(
            &file,
            &ReplaceTarget::LineRange { start: 5, end: 6 },
            None,
            "\tvar a = 10\n\tvar b = 20\n",
            true,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("var a = 10"));
        assert!(!content.contains("var x = 1"));
        assert!(content.contains("print(x)"));
    }

    #[test]
    fn replace_line_rejects_split() {
        let temp = setup(&[("player.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);
        let file = temp.path().join("player.gd");
        let result = replace(
            &file,
            &ReplaceTarget::Line(4),
            None,
            "var x = 1\n",
            true,
            temp.path(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn replace_empty_file() {
        let temp = setup(&[("empty.gd", "")]);
        let file = temp.path().join("empty.gd");
        let result = replace(
            &file,
            &ReplaceTarget::LineRange { start: 1, end: 1 },
            None,
            "extends Node\n",
            true,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("extends Node"));
    }

    // ── insert ──────────────────────────────────────────────────────────

    #[test]
    fn insert_name_after() {
        let temp = setup(&[("player.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);
        let file = temp.path().join("player.gd");
        let result = insert_cmd(
            &file,
            &InsertAnchor::Name("_ready".to_string()),
            &InsertPosition::After,
            None,
            "func _process(delta):\n\tpass\n",
            true,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("func _ready():"));
        assert!(content.contains("func _process(delta):"));
        let ready_pos = content.find("func _ready():").unwrap();
        let process_pos = content.find("func _process(delta):").unwrap();
        assert!(process_pos > ready_pos);
    }

    #[test]
    fn insert_name_before() {
        let temp = setup(&[("player.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);
        let file = temp.path().join("player.gd");
        let result = insert_cmd(
            &file,
            &InsertAnchor::Name("_ready".to_string()),
            &InsertPosition::Before,
            None,
            "var speed = 10\n",
            true,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("var speed = 10"));
        assert!(content.contains("func _ready():"));
        let var_pos = content.find("var speed").unwrap();
        let func_pos = content.find("func _ready():").unwrap();
        assert!(var_pos < func_pos);
    }

    #[test]
    fn insert_into_function_body() {
        let temp = setup(&[(
            "player.gd",
            "extends Node\n\n\nfunc _ready():\n\tprint(\"end\")\n",
        )]);
        let file = temp.path().join("player.gd");
        let result = insert_cmd(
            &file,
            &InsertAnchor::Name("_ready".to_string()),
            &InsertPosition::Into,
            None,
            "\tprint(\"start\")\n",
            true,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("print(\"start\")"));
        assert!(content.contains("print(\"end\")"));
        let start_pos = content.find("print(\"start\")").unwrap();
        let end_pos = content.find("print(\"end\")").unwrap();
        assert!(start_pos < end_pos, "start should come before end");
    }

    #[test]
    fn insert_into_end_function_body() {
        let temp = setup(&[(
            "player.gd",
            "extends Node\n\n\nfunc _ready():\n\tprint(\"start\")\n",
        )]);
        let file = temp.path().join("player.gd");
        let result = insert_cmd(
            &file,
            &InsertAnchor::Name("_ready".to_string()),
            &InsertPosition::IntoEnd,
            None,
            "\tprint(\"end\")\n",
            true,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(&file).unwrap();
        let start_pos = content.find("print(\"start\")").unwrap();
        let end_pos = content.find("print(\"end\")").unwrap();
        assert!(start_pos < end_pos, "end should come after start");
    }

    #[test]
    fn insert_into_class_body() {
        let temp = setup(&[("player.gd", "extends Node\n\n\nclass Inner:\n\tvar x = 1\n")]);
        let file = temp.path().join("player.gd");
        let result = insert_cmd(
            &file,
            &InsertAnchor::Name("Inner".to_string()),
            &InsertPosition::IntoEnd,
            None,
            "\tfunc foo():\n\t\tpass\n",
            true,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("var x = 1"));
        assert!(content.contains("func foo():"));
    }

    #[test]
    fn insert_into_non_container_rejected() {
        let temp = setup(&[("player.gd", "extends Node\nvar speed = 10\n")]);
        let file = temp.path().join("player.gd");
        let result = insert_cmd(
            &file,
            &InsertAnchor::Name("speed".to_string()),
            &InsertPosition::Into,
            None,
            "var x = 1\n",
            true,
            temp.path(),
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("container"),
            "expected container error, got: {msg}"
        );
    }

    #[test]
    fn insert_line_after() {
        let temp = setup(&[("player.gd", "extends Node\nvar a = 1\nvar b = 2\n")]);
        let file = temp.path().join("player.gd");
        let result = insert_cmd(
            &file,
            &InsertAnchor::Line(2),
            &InsertPosition::After,
            None,
            "var inserted = 99\n",
            true,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("var inserted = 99"));
        let a_pos = content.find("var a = 1").unwrap();
        let ins_pos = content.find("var inserted = 99").unwrap();
        let b_pos = content.find("var b = 2").unwrap();
        assert!(ins_pos > a_pos && ins_pos < b_pos);
    }

    // ── helpers ─────────────────────────────────────────────────────────

    #[test]
    fn diff_line_count_empty_to_content() {
        assert_eq!(diff_line_count("", "extends Node\nvar x = 1\n"), 2);
    }
}
