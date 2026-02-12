use tower_lsp::lsp_types::*;

/// Provide code actions (quick fixes) for diagnostics in the given range.
pub fn provide_code_actions(uri: &Url, source: &str, range: &Range) -> Option<CodeActionResponse> {
    // Parse and lint
    let tree = crate::core::parser::parse(source).ok()?;
    let config = std::env::current_dir()
        .ok()
        .and_then(|cwd| crate::core::config::Config::load(&cwd).ok())
        .unwrap_or_default();
    let rules = crate::lint::rules::all_rules(&config.lint.disabled_rules, &config.lint.rules);

    let mut all_diags = Vec::new();
    for rule in &rules {
        all_diags.extend(rule.check(&tree, source, &config.lint));
    }

    let mut actions = Vec::new();

    for diag in &all_diags {
        // Only provide actions for diagnostics in the requested range
        let diag_line = diag.line as u32;
        if diag_line < range.start.line || diag_line > range.end.line {
            continue;
        }

        // Only fixable diagnostics produce code actions
        let Some(fix) = &diag.fix else {
            continue;
        };

        // Convert byte offsets to LSP positions
        let start_pos = byte_offset_to_position(source, fix.byte_start);
        let end_pos = byte_offset_to_position(source, fix.byte_end);

        let edit = TextEdit {
            range: Range::new(start_pos, end_pos),
            new_text: fix.replacement.clone(),
        };

        let title = format!("Fix: {} ({})", diag.message, diag.rule);

        let workspace_edit = WorkspaceEdit {
            changes: Some([(uri.clone(), vec![edit])].into_iter().collect()),
            ..Default::default()
        };

        // Create the LSP diagnostic to associate with
        let lsp_diag = Diagnostic {
            range: Range::new(
                Position::new(diag.line as u32, diag.column as u32),
                Position::new(
                    diag.line as u32,
                    diag.end_column.unwrap_or(diag.column + 1) as u32,
                ),
            ),
            severity: Some(match diag.severity {
                crate::lint::rules::Severity::Info => DiagnosticSeverity::INFORMATION,
                crate::lint::rules::Severity::Warning => DiagnosticSeverity::WARNING,
                crate::lint::rules::Severity::Error => DiagnosticSeverity::ERROR,
            }),
            code: Some(NumberOrString::String(diag.rule.to_string())),
            source: Some("gd".to_string()),
            message: diag.message.clone(),
            ..Default::default()
        };

        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
            title,
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![lsp_diag]),
            edit: Some(workspace_edit),
            is_preferred: Some(true),
            ..Default::default()
        }));
    }

    if actions.is_empty() {
        None
    } else {
        Some(actions)
    }
}

/// Convert a byte offset in source to an LSP Position (line, character).
fn byte_offset_to_position(source: &str, byte_offset: usize) -> Position {
    let mut line = 0u32;
    let mut col = 0u32;

    for (i, ch) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }

    Position::new(line, col)
}
