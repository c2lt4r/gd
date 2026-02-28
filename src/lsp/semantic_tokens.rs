use tower_lsp::lsp_types::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens,
    SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions, SemanticTokensResult,
    WorkDoneProgressOptions,
};

use super::workspace::WorkspaceIndex;

pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::NAMESPACE,   // 0 - autoloads
    SemanticTokenType::TYPE,        // 1 - type annotations
    SemanticTokenType::CLASS,       // 2 - class_name references, engine classes
    SemanticTokenType::ENUM,        // 3 - enum names
    SemanticTokenType::ENUM_MEMBER, // 4 - enum member references
    SemanticTokenType::FUNCTION,    // 5 - function declarations
    SemanticTokenType::METHOD,      // 6 - method calls
    SemanticTokenType::PROPERTY,    // 7 - property access
    SemanticTokenType::VARIABLE,    // 8 - variables
    SemanticTokenType::PARAMETER,   // 9 - function parameters
];

pub const TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION,     // 0
    SemanticTokenModifier::DEFINITION,      // 1
    SemanticTokenModifier::READONLY,        // 2
    SemanticTokenModifier::STATIC,          // 3
    SemanticTokenModifier::DEFAULT_LIBRARY, // 4
];

const MOD_DECLARATION: u32 = 1 << 0;
const MOD_READONLY: u32 = 1 << 2;
const MOD_DEFAULT_LIBRARY: u32 = 1 << 4;

const TYPE_NAMESPACE: u32 = 0;
const TYPE_TYPE: u32 = 1;
const TYPE_CLASS: u32 = 2;
const TYPE_ENUM: u32 = 3;
const TYPE_FUNCTION: u32 = 5;
const TYPE_METHOD: u32 = 6;
const TYPE_PROPERTY: u32 = 7;
const TYPE_VARIABLE: u32 = 8;
const TYPE_PARAMETER: u32 = 9;

/// A classified token before delta encoding.
struct RawToken {
    line: u32,
    col: u32,
    length: u32,
    token_type: u32,
    modifiers: u32,
}

/// Return the semantic token legend for capability registration.
pub fn legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: TOKEN_TYPES.to_vec(),
        token_modifiers: TOKEN_MODIFIERS.to_vec(),
    }
}

/// Return semantic token options for the server capabilities.
pub fn options() -> SemanticTokensOptions {
    SemanticTokensOptions {
        full: Some(SemanticTokensFullOptions::Bool(true)),
        legend: legend(),
        range: None,
        work_done_progress_options: WorkDoneProgressOptions::default(),
    }
}

/// Provide semantic tokens for a GDScript source file.
pub fn provide_semantic_tokens(
    source: &str,
    workspace: Option<&WorkspaceIndex>,
) -> Option<SemanticTokensResult> {
    let tree = crate::core::parser::parse(source).ok()?;
    let file = crate::core::gd_ast::convert(&tree, source);
    let bytes = source.as_bytes();

    let mut raw_tokens = Vec::new();
    walk_node(
        tree.root_node(),
        bytes,
        &file,
        workspace,
        &mut raw_tokens,
    );

    // Sort by line, then column for delta encoding.
    raw_tokens.sort_by(|a, b| a.line.cmp(&b.line).then(a.col.cmp(&b.col)));

    // Delta-encode.
    let mut tokens = Vec::with_capacity(raw_tokens.len());
    let mut prev_line = 0u32;
    let mut prev_col = 0u32;
    for raw in &raw_tokens {
        let delta_line = raw.line - prev_line;
        let delta_start = if delta_line == 0 {
            raw.col - prev_col
        } else {
            raw.col
        };
        tokens.push(SemanticToken {
            delta_line,
            delta_start,
            length: raw.length,
            token_type: raw.token_type,
            token_modifiers_bitset: raw.modifiers,
        });
        prev_line = raw.line;
        prev_col = raw.col;
    }

    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data: tokens,
    }))
}

/// Recursively walk the AST and classify identifier nodes.
fn walk_node(
    node: tree_sitter::Node,
    source: &[u8],
    file: &crate::core::gd_ast::GdFile,
    workspace: Option<&WorkspaceIndex>,
    tokens: &mut Vec<RawToken>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if (child.kind() == "identifier" || child.kind() == "name")
            && let Some(tok) = classify_identifier(child, source, file, workspace)
        {
            tokens.push(tok);
        }
        walk_node(child, source, file, workspace, tokens);
    }
}

/// Try to classify a single identifier/name node into a semantic token.
fn classify_identifier(
    node: tree_sitter::Node,
    source: &[u8],
    file: &crate::core::gd_ast::GdFile,
    workspace: Option<&WorkspaceIndex>,
) -> Option<RawToken> {
    let parent = node.parent()?;
    let text = node.utf8_text(source).ok()?;
    if text.is_empty() {
        return None;
    }

    let start = node.start_position();
    let length = u32::try_from(node.end_byte() - node.start_byte()).unwrap_or(0);
    let line = start.row as u32;
    let col = start.column as u32;
    let make = |token_type, modifiers| RawToken {
        line,
        col,
        length,
        token_type,
        modifiers,
    };

    // Check structural context (parent node determines role).
    if let Some((tt, m)) = classify_by_parent(&parent, &node) {
        return Some(make(tt, m));
    }

    // Check by name (workspace symbols, engine classes, enum members).
    classify_by_name(text, file, workspace).map(|(tt, m)| make(tt, m))
}

/// Classify an identifier based on its parent node context (declarations, calls, types).
fn classify_by_parent(parent: &tree_sitter::Node, node: &tree_sitter::Node) -> Option<(u32, u32)> {
    let pk = parent.kind();
    let is_name = || {
        parent
            .child_by_field_name("name")
            .is_some_and(|n| n.id() == node.id())
    };

    match pk {
        // Declaration names
        "function_definition" | "constructor_definition" if is_name() => {
            Some((TYPE_FUNCTION, MOD_DECLARATION))
        }
        "class_name_statement" if is_name() => Some((TYPE_CLASS, MOD_DECLARATION)),
        "enum_definition" if is_name() => Some((TYPE_ENUM, MOD_DECLARATION)),
        "variable_statement" if is_name() => Some((TYPE_VARIABLE, MOD_DECLARATION)),
        "const_statement" if is_name() => Some((TYPE_VARIABLE, MOD_DECLARATION | MOD_READONLY)),
        "signal_statement" if is_name() => Some((TYPE_PROPERTY, MOD_DECLARATION)),

        // Parameter declarations
        "typed_parameter" if parent.child(0).is_some_and(|c| c.id() == node.id()) => {
            Some((TYPE_PARAMETER, MOD_DECLARATION))
        }
        "parameters" if node.kind() == "identifier" => Some((TYPE_PARAMETER, MOD_DECLARATION)),

        // Function/method calls
        "call" if parent.named_child(0).is_some_and(|c| c.id() == node.id()) => {
            Some((TYPE_FUNCTION, 0))
        }
        "attribute_call" if parent.named_child(0).is_some_and(|c| c.id() == node.id()) => {
            Some((TYPE_METHOD, 0))
        }

        // Return type annotations
        "return_type" => Some((TYPE_TYPE, 0)),

        // Extends statement
        "extends_statement" => Some((TYPE_CLASS, 0)),

        // Type annotations in typed_parameter (non-first-child identifier is the type)
        "typed_parameter"
            if parent.child(0).is_none_or(|c| c.id() != node.id())
                && node.kind() == "identifier" =>
        {
            Some((TYPE_TYPE, 0))
        }

        // Type field within a declaration
        "type" => classify_type_parent(parent),
        "variable_statement" | "const_statement" => {
            if parent
                .child_by_field_name("type")
                .is_some_and(|t| t.id() == node.id())
            {
                Some((TYPE_TYPE, 0))
            } else {
                None
            }
        }

        _ => None,
    }
}

/// Check if a `type` parent node is inside a declaration.
fn classify_type_parent(type_node: &tree_sitter::Node) -> Option<(u32, u32)> {
    let gp = type_node.parent()?;
    let gpk = gp.kind();
    if gpk == "variable_statement" || gpk == "const_statement" || gpk == "signal_statement" {
        Some((TYPE_TYPE, 0))
    } else {
        None
    }
}

/// Classify an identifier by its text — workspace symbols, engine classes, enum members.
fn classify_by_name(
    text: &str,
    file: &crate::core::gd_ast::GdFile,
    workspace: Option<&WorkspaceIndex>,
) -> Option<(u32, u32)> {
    // Workspace autoload references
    if let Some(ws) = workspace {
        if ws.lookup_autoload(text).is_some() {
            return Some((TYPE_NAMESPACE, MOD_DEFAULT_LIBRARY));
        }
        if ws.lookup_class_name(text).is_some() {
            return Some((TYPE_CLASS, 0));
        }
    }

    // Engine class references from ClassDB
    if crate::class_db::class_exists(text) && text.chars().next().is_some_and(char::is_uppercase) {
        return Some((TYPE_CLASS, MOD_DEFAULT_LIBRARY));
    }

    // Enum member references
    for e in file.enums() {
        if e.members.iter().any(|m| m.name == text) {
            return Some((TYPE_PROPERTY, MOD_READONLY));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn function_declaration_highlighted() {
        let source = "func greet():\n\tpass\n";
        let result = provide_semantic_tokens(source, None);
        assert!(result.is_some());
        if let Some(SemanticTokensResult::Tokens(tokens)) = result {
            assert!(!tokens.data.is_empty());
        }
    }

    #[test]
    fn class_name_highlighted() {
        let source = "class_name Player\n";
        let result = provide_semantic_tokens(source, None);
        assert!(result.is_some());
    }

    #[test]
    fn empty_source() {
        let result = provide_semantic_tokens("", None);
        assert!(result.is_some());
    }

    #[test]
    fn legend_has_correct_counts() {
        let l = legend();
        assert_eq!(l.token_types.len(), TOKEN_TYPES.len());
        assert_eq!(l.token_modifiers.len(), TOKEN_MODIFIERS.len());
    }

    #[test]
    fn options_has_full_support() {
        let opts = options();
        assert!(opts.full.is_some());
        assert!(opts.range.is_none());
    }

    #[test]
    fn variable_declaration_highlighted() {
        let source = "var health: int = 100\n";
        let result = provide_semantic_tokens(source, None);
        assert!(result.is_some());
        if let Some(SemanticTokensResult::Tokens(tokens)) = result {
            assert!(!tokens.data.is_empty());
        }
    }

    #[test]
    fn const_declaration_has_readonly() {
        let source = "const MAX := 10\n";
        let result = provide_semantic_tokens(source, None);
        assert!(result.is_some());
        if let Some(SemanticTokensResult::Tokens(tokens)) = result {
            let max_token = tokens.data.iter().find(|t| {
                t.token_type == TYPE_VARIABLE && (t.token_modifiers_bitset & MOD_READONLY) != 0
            });
            assert!(max_token.is_some(), "const should have READONLY modifier");
        }
    }

    #[test]
    fn signal_declaration_highlighted() {
        let source = "signal health_changed\n";
        let result = provide_semantic_tokens(source, None);
        assert!(result.is_some());
        if let Some(SemanticTokensResult::Tokens(tokens)) = result {
            let signal_tok = tokens.data.iter().find(|t| {
                t.token_type == TYPE_PROPERTY && (t.token_modifiers_bitset & MOD_DECLARATION) != 0
            });
            assert!(
                signal_tok.is_some(),
                "signal name should be PROPERTY+DECLARATION"
            );
        }
    }

    #[test]
    fn engine_class_in_type_annotation() {
        let source = "var node: Node2D\n";
        let result = provide_semantic_tokens(source, None);
        assert!(result.is_some());
        if let Some(SemanticTokensResult::Tokens(tokens)) = result {
            let type_tok = tokens.data.iter().find(|t| t.token_type == TYPE_TYPE);
            assert!(
                type_tok.is_some(),
                "engine class in type annotation should be TYPE"
            );
        }
    }

    #[test]
    fn extends_highlighted_as_class() {
        let source = "extends Node2D\n";
        let result = provide_semantic_tokens(source, None);
        assert!(result.is_some());
        if let Some(SemanticTokensResult::Tokens(tokens)) = result {
            let class_tok = tokens.data.iter().find(|t| t.token_type == TYPE_CLASS);
            assert!(class_tok.is_some(), "extends target should be CLASS");
        }
    }

    #[test]
    fn delta_encoding_is_correct() {
        let source = "func a():\n\tpass\nfunc b():\n\tpass\n";
        let result = provide_semantic_tokens(source, None);
        if let Some(SemanticTokensResult::Tokens(tokens)) = result {
            assert!(tokens.data.len() >= 2, "should have at least 2 tokens");
            let second = &tokens.data[1];
            assert!(
                second.delta_line > 0,
                "second function should be on a later line"
            );
        }
    }

    #[test]
    fn method_call_highlighted() {
        let source = "func _ready():\n\tget_node(\".\").queue_free()\n";
        let result = provide_semantic_tokens(source, None);
        assert!(result.is_some());
        if let Some(SemanticTokensResult::Tokens(tokens)) = result {
            let method_tok = tokens.data.iter().find(|t| t.token_type == TYPE_METHOD);
            assert!(method_tok.is_some(), "method call should be METHOD");
        }
    }
}
