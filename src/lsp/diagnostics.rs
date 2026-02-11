use tower_lsp::lsp_types::*;

/// Lint source code and return LSP diagnostics.
pub fn lint_source(source: &str, uri: &Url) -> Vec<Diagnostic> {
    // Parse with tree-sitter
    let tree = match crate::core::parser::parse(source) {
        Ok(tree) => tree,
        Err(_) => return vec![],
    };

    // Load config, searching upward from the file's directory
    let config = uri
        .to_file_path()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .and_then(|dir| crate::core::config::Config::load(&dir).ok())
        .unwrap_or_default();

    // Check if this file matches ignore patterns
    if let Ok(file_path) = uri.to_file_path() {
        let base = uri
            .to_file_path()
            .ok()
            .and_then(|p| crate::core::config::find_project_root(&p))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        if crate::lint::matches_ignore_pattern(&file_path, &base, &config.lint.ignore_patterns) {
            return vec![];
        }
    }

    // Run all lint rules
    let rules = crate::lint::rules::all_rules(&config.lint.disabled_rules, &config.lint.rules);
    let mut diags = Vec::new();
    for rule in &rules {
        diags.extend(rule.check(&tree, source, &config.lint));
    }

    // Parse suppressions and filter
    let suppressions = parse_suppressions(source);

    // Convert to LSP diagnostics
    diags
        .into_iter()
        .filter(|d| !is_suppressed(d, &suppressions))
        .map(|d| {
            let start = Position::new(d.line as u32, d.column as u32);
            let end = Position::new(d.line as u32, d.end_column.unwrap_or(d.column + 1) as u32);
            Diagnostic {
                range: Range::new(start, end),
                severity: Some(match d.severity {
                    crate::lint::rules::Severity::Warning => DiagnosticSeverity::WARNING,
                    crate::lint::rules::Severity::Error => DiagnosticSeverity::ERROR,
                }),
                code: Some(NumberOrString::String(d.rule.to_string())),
                source: Some("gd".to_string()),
                message: d.message,
                ..Default::default()
            }
        })
        .collect()
}

/// Minimal inline suppression parsing (replicates lint/mod.rs logic).
fn parse_suppressions(
    source: &str,
) -> std::collections::HashMap<usize, Option<std::collections::HashSet<String>>> {
    let mut suppressions = std::collections::HashMap::new();
    for (line_idx, line) in source.lines().enumerate() {
        if let Some(pos) = line.find("# gd:ignore") {
            let rest = &line[pos + "# gd:ignore".len()..];
            if let Some(rule_rest) = rest.strip_prefix("-next-line") {
                let rules = parse_rule_list(rule_rest);
                suppressions.insert(line_idx + 1, rules);
            } else {
                let rules = parse_rule_list(rest);
                suppressions.insert(line_idx, rules);
            }
        }
    }
    suppressions
}

fn parse_rule_list(text: &str) -> Option<std::collections::HashSet<String>> {
    let text = text.trim();
    if text.starts_with('[') {
        if let Some(end) = text.find(']') {
            let rules: std::collections::HashSet<String> = text[1..end]
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if rules.is_empty() { None } else { Some(rules) }
        } else {
            None
        }
    } else {
        None
    }
}

fn is_suppressed(
    diag: &crate::lint::rules::LintDiagnostic,
    suppressions: &std::collections::HashMap<usize, Option<std::collections::HashSet<String>>>,
) -> bool {
    if let Some(rules) = suppressions.get(&diag.line) {
        match rules {
            None => true,
            Some(rule_set) => rule_set.contains(diag.rule),
        }
    } else {
        false
    }
}
