use miette::{Result, miette};
use std::path::Path;
use tree_sitter::{Language, Parser, Tree};

/// Get the tree-sitter GDScript language.
pub fn gdscript_language() -> Language {
    tree_sitter_gdscript::LANGUAGE.into()
}

/// Create a new parser configured for GDScript.
pub fn new_parser() -> Result<Parser> {
    let mut parser = Parser::new();
    parser
        .set_language(&gdscript_language())
        .map_err(|e| miette!("Failed to set GDScript language: {e}"))?;
    Ok(parser)
}

/// Parse GDScript source code into a tree-sitter Tree.
pub fn parse(source: &str) -> Result<Tree> {
    let mut parser = new_parser()?;
    parser
        .parse(source, None)
        .ok_or_else(|| miette!("Failed to parse GDScript source"))
}

/// Parse a GDScript file from disk.
pub fn parse_file(path: &Path) -> Result<(String, Tree)> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| miette!("Failed to read {}: {e}", path.display()))?;
    let tree = parse(&source)?;
    Ok((source, tree))
}
