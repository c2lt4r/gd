use miette::{Result, miette};
use std::path::Path;
use tree_sitter::{Language, Parser, Tree};

/// Get the tree-sitter Godot resource language (.tscn/.tres).
pub fn resource_language() -> Language {
    tree_sitter_godot_resource::LANGUAGE.into()
}

/// Create a new parser configured for Godot resource files.
pub fn new_resource_parser() -> Result<Parser> {
    let mut parser = Parser::new();
    parser
        .set_language(&resource_language())
        .map_err(|e| miette!("Failed to set Godot resource language: {e}"))?;
    Ok(parser)
}

/// Parse Godot resource source code into a tree-sitter Tree.
///
/// Normalizes `&"…"` StringName literals to plain `"…"` strings before parsing,
/// since the tree-sitter-godot-resource grammar doesn't support the `&` sigil.
/// Godot treats both forms identically at runtime.
pub fn parse_resource(source: &str) -> Result<Tree> {
    let normalized = normalize_string_names(source);
    let mut parser = new_resource_parser()?;
    parser
        .parse(normalized.as_ref(), None)
        .ok_or_else(|| miette!("Failed to parse Godot resource source"))
}

/// Parse a Godot resource file (.tscn/.tres) from disk.
///
/// Returns the **normalized** source (with `&"` → `"`) and the parse tree,
/// so line/column positions remain consistent between the two.
pub fn parse_resource_file(path: &Path) -> Result<(String, Tree)> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| miette!("Failed to read {}: {e}", path.display()))?;
    let source = normalize_string_names(&raw).into_owned();
    let tree = {
        let mut parser = new_resource_parser()?;
        parser
            .parse(&source, None)
            .ok_or_else(|| miette!("Failed to parse Godot resource source"))?
    };
    Ok((source, tree))
}

/// Normalize `&"…"` StringName literals to `"…"` and return an owned copy.
///
/// Use this when you need to extract text from tree-sitter nodes using the
/// source bytes — the tree is parsed from the normalized text, so offsets
/// only match if you also extract from the normalized text.
pub fn normalize_for_extraction(source: &str) -> String {
    normalize_string_names(source).into_owned()
}

/// Replace `&"…"` StringName literals with plain `"…"` strings.
///
/// The `&` sigil is Godot's StringName marker. The tree-sitter grammar for
/// resource files doesn't recognize it, but the semantic difference is
/// irrelevant for validation — Godot coerces String ↔ StringName at load time.
fn normalize_string_names(source: &str) -> std::borrow::Cow<'_, str> {
    if source.contains("&\"") {
        std::borrow::Cow::Owned(source.replace("&\"", "\""))
    } else {
        std::borrow::Cow::Borrowed(source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_stringname_sigil() {
        let input = r#"name = &"default""#;
        let result = normalize_string_names(input);
        assert_eq!(result.as_ref(), r#"name = "default""#);
    }

    #[test]
    fn normalize_no_op_without_sigil() {
        let input = r#"name = "default""#;
        let result = normalize_string_names(input);
        assert!(matches!(result, std::borrow::Cow::Borrowed(_)));
        assert_eq!(result.as_ref(), input);
    }

    #[test]
    fn parse_resource_with_stringname_literals() {
        let source = r#"[gd_resource type="SpriteFrames" format=3]

[resource]
animations = [{"frames": [], "loop": true, "name": &"default", "speed": 10.0}]
"#;
        let tree = parse_resource(source).unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn parse_resource_multiple_stringnames() {
        let source = r#"[gd_resource type="SpriteFrames" format=3]

[resource]
animations = [{"name": &"idle", "speed": 5.0}, {"name": &"run", "speed": 10.0}]
"#;
        let tree = parse_resource(source).unwrap();
        assert!(!tree.root_node().has_error());
    }
}
