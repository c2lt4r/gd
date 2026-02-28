use std::fmt::Write as _;

use tower_lsp::lsp_types::{
    Documentation, MarkupContent, MarkupKind, ParameterInformation, ParameterLabel, Position,
    SignatureHelp, SignatureInformation,
};

use crate::core::gd_ast::{self, GdExtends};

use super::workspace::WorkspaceIndex;

/// Provide signature help for the function call enclosing the cursor.
pub fn provide_signature_help(
    source: &str,
    position: Position,
    workspace: Option<&WorkspaceIndex>,
) -> Option<SignatureHelp> {
    let (func_name, receiver, active_parameter) = find_call_context(source, position)?;

    // Parse the source to build a symbol table for signature resolution.
    let tree = crate::core::parser::parse(source).ok()?;
    let file = gd_ast::convert(&tree, source);

    let signature =
        resolve_signature(&func_name, receiver.as_deref(), source, &tree, &file, workspace)?;

    Some(SignatureHelp {
        signatures: vec![signature],
        active_signature: Some(0),
        active_parameter: Some(active_parameter),
    })
}

/// Find the function call context at the cursor using text scanning.
///
/// Walks backward from the cursor to find the nearest unmatched `(`, then
/// extracts the function name preceding it. Returns the function name,
/// optional receiver, and the active parameter index (comma count).
fn find_call_context(source: &str, position: Position) -> Option<(String, Option<String>, u32)> {
    // Convert position to a byte offset.
    let offset = position_to_offset(source, position)?;
    let before = &source[..offset];

    // Walk backward to find the unmatched open paren, tracking commas and parens.
    let mut depth: u32 = 0;
    let mut comma_count: u32 = 0;
    let mut paren_offset = None;

    for (i, ch) in before.char_indices().rev() {
        match ch {
            ')' | ']' => depth += 1,
            '(' | '[' => {
                if depth == 0 {
                    if ch == '(' {
                        paren_offset = Some(i);
                    }
                    break;
                }
                depth -= 1;
            }
            ',' if depth == 0 => comma_count += 1,
            _ => {}
        }
    }

    let paren_pos = paren_offset?;

    // Extract the identifier (and optional receiver) before the `(`.
    let before_paren = before[..paren_pos].trim_end();
    if before_paren.is_empty() {
        return None;
    }

    // Find where the identifier starts by walking backward through valid identifier chars.
    let ident_start = before_paren
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map_or(0, |i| i + 1);
    let func_name = &before_paren[ident_start..];
    if func_name.is_empty() || !func_name.starts_with(|c: char| c.is_alphabetic() || c == '_') {
        return None;
    }

    // Check for a receiver: look for `.` before the function name.
    let prefix = before_paren[..ident_start].trim_end();
    let receiver = if let Some(stripped) = prefix.strip_suffix('.') {
        let before_dot = stripped.trim_end();
        let recv_start = before_dot
            .rfind(|c: char| !c.is_alphanumeric() && c != '_')
            .map_or(0, |i| i + 1);
        let recv = &before_dot[recv_start..];
        if recv.is_empty() {
            None
        } else {
            Some(recv.to_string())
        }
    } else {
        None
    };

    Some((func_name.to_string(), receiver, comma_count))
}

/// Convert an LSP `Position` (line, character) to a byte offset in the source.
fn position_to_offset(source: &str, position: Position) -> Option<usize> {
    let mut offset = 0;
    for (i, line) in source.lines().enumerate() {
        if i == position.line as usize {
            let col = position.character as usize;
            // Clamp to line length to handle cursor at end of line
            let clamped_col = col.min(line.len());
            return Some(offset + clamped_col);
        }
        // +1 for the newline character
        offset += line.len() + 1;
    }
    // If the position is at the very end (past last line)
    if position.line as usize == source.lines().count() {
        Some(source.len())
    } else {
        None
    }
}

/// Resolve the signature for a function name, optionally with a receiver.
fn resolve_signature(
    func_name: &str,
    receiver: Option<&str>,
    source: &str,
    tree: &tree_sitter::Tree,
    file: &gd_ast::GdFile,
    workspace: Option<&WorkspaceIndex>,
) -> Option<SignatureInformation> {
    // 1. If there is a receiver, try to resolve the method on that type.
    if let Some(recv) = receiver {
        return resolve_method_signature(func_name, recv, source, tree, file, workspace);
    }

    // 2. Check same-file function declarations.
    if let Some(sig) = resolve_same_file_signature(func_name, source, tree) {
        return Some(sig);
    }

    // 3. Check builtin functions.
    if let Some(doc) = super::builtins::lookup_function(func_name) {
        return Some(SignatureInformation {
            label: doc.brief.to_string(),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: doc.description.to_string(),
            })),
            parameters: None,
            active_parameter: None,
        });
    }

    // 4. Check workspace files.
    if let Some(ws) = workspace
        && let Some(sig) = resolve_workspace_signature(func_name, ws)
    {
        return Some(sig);
    }

    None
}

/// Resolve a method signature on a receiver type.
fn resolve_method_signature(
    method: &str,
    receiver: &str,
    source: &str,
    tree: &tree_sitter::Tree,
    file: &gd_ast::GdFile,
    workspace: Option<&WorkspaceIndex>,
) -> Option<SignatureInformation> {
    // Resolve the receiver's class name.
    let class_name = if receiver == "self" || receiver == "super" {
        match file.extends {
            Some(GdExtends::Class(name)) if crate::class_db::class_exists(name) => {
                Some(name.to_string())
            }
            _ => None,
        }
    } else if crate::class_db::class_exists(receiver) {
        Some(receiver.to_string())
    } else {
        let position = Position::new(0, 0);
        super::completion::resolve_simple_receiver(receiver, source, position, workspace)
    };

    // For self/super, also check same-file declarations first
    if (receiver == "self" || receiver == "super")
        && let Some(sig) = resolve_same_file_signature(method, source, tree)
    {
        return Some(sig);
    }

    // Check workspace class files
    if let Some(ws) = workspace {
        let class = class_name.as_deref().unwrap_or(receiver);
        let content = ws
            .lookup_class_name(class)
            .and_then(|path| ws.get_content(&path))
            .or_else(|| ws.autoload_content(class));
        if let Some(content) = content
            && let Ok(file_tree) = crate::core::parser::parse(&content)
            && let Some(sig) = resolve_same_file_signature(method, &content, &file_tree)
        {
            return Some(sig);
        }
    }

    let class = class_name.as_deref().unwrap_or(receiver);

    // Check ClassDB methods
    if let Some(ret) = crate::class_db::method_return_type(class, method) {
        let label = format!("{method}() -> {ret}");
        return Some(SignatureInformation {
            label,
            documentation: None,
            parameters: None,
            active_parameter: None,
        });
    }

    // Check builtin members (walk inheritance chain)
    let mut cur = class;
    loop {
        if let Some(doc) = super::builtins::lookup_member_for(cur, method) {
            return Some(SignatureInformation {
                label: doc.brief.to_string(),
                documentation: Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: doc.description.to_string(),
                })),
                parameters: None,
                active_parameter: None,
            });
        }
        match crate::class_db::parent_class(cur) {
            Some(parent) => cur = parent,
            None => break,
        }
    }

    None
}

/// Resolve a function signature from the same file's symbol table.
fn resolve_same_file_signature(
    func_name: &str,
    source: &str,
    tree: &tree_sitter::Tree,
) -> Option<SignatureInformation> {
    let symbols = crate::core::symbol_table::build(tree, source);
    let func = symbols.functions.iter().find(|f| f.name == func_name)?;

    let params: Vec<ParameterInformation> = func
        .params
        .iter()
        .map(|p| {
            let label = format_param(p);
            ParameterInformation {
                label: ParameterLabel::Simple(label),
                documentation: None,
            }
        })
        .collect();

    let param_labels: Vec<String> = func.params.iter().map(format_param).collect();

    let mut label = format!("func {}({})", func.name, param_labels.join(", "));
    if let Some(ref ret) = func.return_type {
        let _ = write!(label, " -> {}", ret.name);
    }

    let documentation = func.doc.as_ref().map(|d| {
        Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: d.clone(),
        })
    });

    Some(SignatureInformation {
        label,
        documentation,
        parameters: if params.is_empty() {
            None
        } else {
            Some(params)
        },
        active_parameter: None,
    })
}

/// Format a parameter declaration as a label string.
fn format_param(p: &crate::core::symbol_table::ParamDecl) -> String {
    match &p.type_ann {
        Some(ann) => format!("{}: {}", p.name, ann.name),
        None => p.name.clone(),
    }
}

/// Resolve a function signature from workspace files.
fn resolve_workspace_signature(
    func_name: &str,
    ws: &WorkspaceIndex,
) -> Option<SignatureInformation> {
    for (_, content) in ws.all_files() {
        if let Ok(file_tree) = crate::core::parser::parse(&content)
            && let Some(sig) = resolve_same_file_signature(func_name, &content, &file_tree)
        {
            return Some(sig);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_help_same_file_function() {
        let source = "func greet(name: String, age: int):\n\tpass\n\nfunc main():\n\tgreet(\n";
        // Cursor inside greet( at line 4, col 7
        let result = provide_signature_help(source, Position::new(4, 7), None);
        assert!(result.is_some());
        let help = result.unwrap();
        assert_eq!(help.signatures.len(), 1);
        assert_eq!(help.active_parameter, Some(0));
        let sig = &help.signatures[0];
        assert!(
            sig.label.contains("greet"),
            "label should contain 'greet', got: {}",
            sig.label
        );
        assert!(
            sig.label.contains("name: String"),
            "label should contain 'name: String', got: {}",
            sig.label
        );
        assert!(sig.parameters.is_some());
        assert_eq!(sig.parameters.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn signature_help_second_param() {
        let source =
            "func greet(name: String, age: int):\n\tpass\n\nfunc main():\n\tgreet(\"hi\", \n";
        let result = provide_signature_help(source, Position::new(4, 13), None);
        assert!(result.is_some());
        let help = result.unwrap();
        assert_eq!(help.active_parameter, Some(1));
    }

    #[test]
    fn no_signature_outside_call() {
        let source = "func main():\n\tvar x = 1\n";
        let result = provide_signature_help(source, Position::new(1, 10), None);
        assert!(result.is_none());
    }

    #[test]
    fn signature_help_with_return_type() {
        let source = "func add(a: int, b: int) -> int:\n\treturn a + b\n\nfunc main():\n\tadd(\n";
        let result = provide_signature_help(source, Position::new(4, 5), None);
        assert!(result.is_some());
        let sig = &result.unwrap().signatures[0];
        assert!(
            sig.label.contains("-> int"),
            "should show return type, got: {}",
            sig.label
        );
    }

    #[test]
    fn signature_help_no_params() {
        let source = "func hello():\n\tpass\n\nfunc main():\n\thello(\n";
        let result = provide_signature_help(source, Position::new(4, 7), None);
        assert!(result.is_some());
        let sig = &result.unwrap().signatures[0];
        assert!(
            sig.label.contains("hello()"),
            "should show empty params, got: {}",
            sig.label
        );
        assert!(sig.parameters.is_none());
    }

    #[test]
    fn signature_help_builtin_function() {
        let source = "func main():\n\tclamp(\n";
        let result = provide_signature_help(source, Position::new(1, 7), None);
        assert!(result.is_some());
        let sig = &result.unwrap().signatures[0];
        assert!(
            sig.label.contains("clamp"),
            "should show clamp, got: {}",
            sig.label
        );
    }

    #[test]
    fn find_context_simple_call() {
        let (name, recv, commas) =
            find_call_context("func f():\n\tgreet(", Position::new(1, 7)).unwrap();
        assert_eq!(name, "greet");
        assert!(recv.is_none());
        assert_eq!(commas, 0);
    }

    #[test]
    fn find_context_with_commas() {
        let (_, _, commas) =
            find_call_context("func f():\n\tgreet(\"hi\", ", Position::new(1, 13)).unwrap();
        assert_eq!(commas, 1);
    }

    #[test]
    fn find_context_method_call() {
        let (name, recv, _) =
            find_call_context("func f():\n\tself.greet(", Position::new(1, 12)).unwrap();
        assert_eq!(name, "greet");
        assert_eq!(recv.as_deref(), Some("self"));
    }

    #[test]
    fn find_context_nested_parens() {
        // Inner call is fully closed, so we find the outer call.
        let (name, _, commas) =
            find_call_context("func f():\n\touter(inner(1), ", Position::new(1, 17)).unwrap();
        assert_eq!(name, "outer");
        assert_eq!(commas, 1);
    }

    #[test]
    fn no_context_outside_call() {
        let result = find_call_context("func main():\n\tvar x = 1", Position::new(1, 10));
        assert!(result.is_none());
    }
}
