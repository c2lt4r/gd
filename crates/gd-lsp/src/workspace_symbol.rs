#![allow(deprecated)] // SymbolInformation::deprecated field is deprecated in LSP spec but required by tower-lsp

use std::path::Path;

use tower_lsp::lsp_types::{Location, Position, Range, SymbolInformation, SymbolKind, Url};

use super::workspace::WorkspaceIndex;

/// Build a single `SymbolInformation` entry.
fn make_symbol(
    name: String,
    kind: SymbolKind,
    line: u32,
    uri: &Url,
    container_name: Option<&str>,
) -> SymbolInformation {
    #[allow(deprecated)]
    SymbolInformation {
        name,
        kind,
        tags: None,
        deprecated: None,
        location: Location {
            uri: uri.clone(),
            range: Range::new(Position::new(line, 0), Position::new(line, 0)),
        },
        container_name: container_name.map(String::from),
    }
}

/// Collect all symbols from a single file's symbol table into `results`.
fn collect_file_symbols(
    path: &Path,
    workspace: &WorkspaceIndex,
    query_lower: &str,
    results: &mut Vec<SymbolInformation>,
) {
    let Some(table_ref) = workspace.get_symbols(path) else {
        return;
    };
    let table = &*table_ref;

    let Ok(uri) = Url::from_file_path(path) else {
        return;
    };

    let container_name = path.file_name().and_then(|n| n.to_str()).map(String::from);

    let matches = |name: &str| -> bool {
        query_lower.is_empty() || name.to_lowercase().contains(query_lower)
    };

    if let Some(ref cn) = table.class_name
        && matches(cn)
    {
        results.push(make_symbol(
            cn.clone(),
            SymbolKind::CLASS,
            0,
            &uri,
            container_name.as_deref(),
        ));
    }

    for func in &table.functions {
        if matches(&func.name) {
            results.push(make_symbol(
                func.name.clone(),
                SymbolKind::FUNCTION,
                func.line as u32,
                &uri,
                container_name.as_deref(),
            ));
        }
    }

    for var in &table.variables {
        if matches(&var.name) {
            let kind = if var.is_constant {
                SymbolKind::CONSTANT
            } else {
                SymbolKind::VARIABLE
            };
            results.push(make_symbol(
                var.name.clone(),
                kind,
                var.line as u32,
                &uri,
                container_name.as_deref(),
            ));
        }
    }

    for signal in &table.signals {
        if matches(&signal.name) {
            results.push(make_symbol(
                signal.name.clone(),
                SymbolKind::EVENT,
                signal.line as u32,
                &uri,
                container_name.as_deref(),
            ));
        }
    }

    for enum_decl in &table.enums {
        if matches(&enum_decl.name) {
            results.push(make_symbol(
                enum_decl.name.clone(),
                SymbolKind::ENUM,
                enum_decl.line as u32,
                &uri,
                container_name.as_deref(),
            ));
        }
    }
}

/// Return workspace symbols matching `query` (case-insensitive substring).
///
/// An empty query returns all symbols — VS Code sends this for the initial load.
pub fn workspace_symbols(query: &str, workspace: &WorkspaceIndex) -> Vec<SymbolInformation> {
    let query_lower = query.to_lowercase();
    let mut results = Vec::new();

    for (path, _content) in workspace.all_files() {
        collect_file_symbols(&path, workspace, &query_lower, &mut results);
    }

    results
}

#[cfg(test)]
mod tests {
    #[test]
    fn empty_query_placeholder() {
        // Can't easily test with full WorkspaceIndex without filesystem
        // Just verify the function signature compiles
    }
}
