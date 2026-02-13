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
pub fn parse_resource(source: &str) -> Result<Tree> {
    let mut parser = new_resource_parser()?;
    parser
        .parse(source, None)
        .ok_or_else(|| miette!("Failed to parse Godot resource source"))
}

/// Parse a Godot resource file (.tscn/.tres) from disk.
pub fn parse_resource_file(path: &Path) -> Result<(String, Tree)> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| miette!("Failed to read {}: {e}", path.display()))?;
    let tree = parse_resource(&source)?;
    Ok((source, tree))
}
