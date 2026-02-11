use tower_lsp::lsp_types::*;

/// Format a GDScript document and return a full-document TextEdit.
pub fn format_document(source: &str, options: &FormattingOptions) -> Option<Vec<TextEdit>> {
    // Determine indentation from LSP options, allow project config to override
    let config = crate::core::config::Config::default();
    let use_tabs = if config.fmt.use_tabs {
        true
    } else {
        !options.insert_spaces
    };
    let indent_size = if config.fmt.indent_size > 0 {
        config.fmt.indent_size
    } else {
        options.tab_size as usize
    };

    // Parse the source
    let tree = crate::core::parser::parse(source).ok()?;
    let root = tree.root_node();

    // Format using our printer
    let mut printer = crate::fmt::printer::Printer::new(use_tabs, indent_size);
    printer.format(&root, source);
    let formatted = printer.finish();

    // If nothing changed, return empty edits
    if formatted == source {
        return Some(vec![]);
    }

    // Return a single edit that replaces the entire document
    let line_count = source.lines().count();
    let last_line = source.lines().last().unwrap_or("");

    Some(vec![TextEdit {
        range: Range::new(
            Position::new(0, 0),
            Position::new(line_count as u32, last_line.len() as u32),
        ),
        new_text: formatted,
    }])
}
