use miette::{Result, miette};
use owo_colors::OwoColorize;
use serde::Serialize;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use tree_sitter::{Node, Tree};

use crate::core::fs::collect_gdscript_files;
use crate::core::parser::parse_file;

/// Documentation for a signal.
#[derive(Debug, Clone, Serialize)]
pub struct DocSignal {
    pub name: String,
    pub params: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// Documentation for a property.
#[derive(Debug, Clone, Serialize)]
pub struct DocProperty {
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub type_hint: String,
    pub is_exported: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// Documentation for a method.
#[derive(Debug, Clone, Serialize)]
pub struct DocMethod {
    pub name: String,
    pub params: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub return_type: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// Documentation for a class.
#[derive(Debug, Clone, Serialize)]
pub struct DocClass {
    pub name: String,
    pub file: PathBuf,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub extends: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub signals: Vec<DocSignal>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<DocProperty>,
    pub methods: Vec<DocMethod>,
}

/// Extract documentation from a `GDScript` file.
#[allow(clippy::too_many_lines)]
pub fn extract_docs(source: &str, tree: &Tree, file_path: &Path) -> DocClass {
    let root = tree.root_node();
    let mut class_name = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown")
        .to_string();
    let mut extends = String::new();
    let mut class_description = String::new();
    let mut signals = Vec::new();
    let mut properties = Vec::new();
    let mut methods = Vec::new();

    // Walk all children of the root to find declarations
    let mut cursor = root.walk();
    let children: Vec<Node> = root.children(&mut cursor).collect();

    let mut accumulated_doc = String::new();
    let mut i = 0;

    while i < children.len() {
        let node = children[i];
        let kind = node.kind();

        // Accumulate doc comments
        if kind == "comment" {
            let text = node.utf8_text(source.as_bytes()).unwrap_or("");
            if text.starts_with("##") {
                let doc_line = text.trim_start_matches("##").trim();
                if !accumulated_doc.is_empty() {
                    accumulated_doc.push('\n');
                }
                accumulated_doc.push_str(doc_line);
            }
            i += 1;
            continue;
        }

        // Process declaration nodes with accumulated docs
        match kind {
            "class_name_statement" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    class_name = name_node
                        .utf8_text(source.as_bytes())
                        .unwrap_or("")
                        .to_string();
                }
                if !accumulated_doc.is_empty() {
                    class_description.clone_from(&accumulated_doc);
                }
            }
            "extends_statement" => {
                // Get the type being extended
                for i in 0..node.named_child_count() {
                    if let Some(type_node) = node.named_child(i)
                        && (type_node.kind() == "type" || type_node.kind() == "identifier")
                    {
                        extends = type_node
                            .utf8_text(source.as_bytes())
                            .unwrap_or("")
                            .to_string();
                        break;
                    }
                }
            }
            "signal_statement" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = name_node
                        .utf8_text(source.as_bytes())
                        .unwrap_or("")
                        .to_string();
                    let params = if let Some(params_node) = node.child_by_field_name("parameters") {
                        params_node
                            .utf8_text(source.as_bytes())
                            .unwrap_or("()")
                            .to_string()
                    } else {
                        String::new()
                    };
                    signals.push(DocSignal {
                        name,
                        params,
                        description: accumulated_doc.clone(),
                    });
                }
            }
            "variable_statement" => {
                let is_exported = has_export_annotation(&node, source);
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = name_node
                        .utf8_text(source.as_bytes())
                        .unwrap_or("")
                        .to_string();
                    let type_hint = if let Some(type_node) = node.child_by_field_name("type") {
                        type_node
                            .utf8_text(source.as_bytes())
                            .unwrap_or("")
                            .to_string()
                    } else {
                        String::new()
                    };
                    properties.push(DocProperty {
                        name,
                        type_hint,
                        is_exported,
                        description: accumulated_doc.clone(),
                    });
                }
            }
            "function_definition" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = name_node
                        .utf8_text(source.as_bytes())
                        .unwrap_or("")
                        .to_string();
                    let params = if let Some(params_node) = node.child_by_field_name("parameters") {
                        params_node
                            .utf8_text(source.as_bytes())
                            .unwrap_or("()")
                            .to_string()
                    } else {
                        "()".to_string()
                    };
                    let return_type =
                        if let Some(ret_node) = node.child_by_field_name("return_type") {
                            ret_node
                                .utf8_text(source.as_bytes())
                                .unwrap_or("")
                                .to_string()
                        } else {
                            String::new()
                        };
                    methods.push(DocMethod {
                        name,
                        params,
                        return_type,
                        description: accumulated_doc.clone(),
                    });
                }
            }
            _ => {}
        }

        // Clear accumulated doc after processing declaration
        if kind != "comment" {
            accumulated_doc.clear();
        }

        i += 1;
    }

    DocClass {
        name: class_name,
        file: file_path.to_path_buf(),
        extends,
        description: class_description,
        signals,
        properties,
        methods,
    }
}

/// Check if a variable has an @export annotation.
fn has_export_annotation(node: &Node, source: &str) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "annotations" {
            let mut annot_cursor = child.walk();
            for annot in child.children(&mut annot_cursor) {
                if annot.kind() == "annotation" {
                    let mut ident_cursor = annot.walk();
                    for ident_child in annot.children(&mut ident_cursor) {
                        if ident_child.kind() == "identifier" {
                            let name = ident_child.utf8_text(source.as_bytes()).unwrap_or("");
                            if name == "export" {
                                return true;
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

/// Render documentation to markdown.
pub fn render_markdown(doc: &DocClass) -> String {
    let mut output = String::new();

    // Title
    let _ = writeln!(output, "# {}\n", doc.name);

    // Metadata
    if !doc.extends.is_empty() {
        let _ = writeln!(output, "**Extends:** {}", doc.extends);
    }
    let _ = writeln!(output, "**File:** `{}`\n", doc.file.display());

    // Description
    if !doc.description.is_empty() {
        let _ = writeln!(output, "{}\n", doc.description);
    }

    // Signals
    if !doc.signals.is_empty() {
        output.push_str("## Signals\n\n");
        for signal in &doc.signals {
            let _ = writeln!(output, "### {}{}\n", signal.name, signal.params);
            if !signal.description.is_empty() {
                let _ = writeln!(output, "{}\n", signal.description);
            }
        }
    }

    // Properties
    if !doc.properties.is_empty() {
        output.push_str("## Properties\n\n");
        output.push_str("| Name | Type | Export | Description |\n");
        output.push_str("|------|------|--------|-------------|\n");
        for prop in &doc.properties {
            let export = if prop.is_exported { "yes" } else { "no" };
            let _ = writeln!(
                output,
                "| {} | {} | {} | {} |",
                prop.name, prop.type_hint, export, prop.description
            );
        }
        output.push('\n');
    }

    // Methods
    if !doc.methods.is_empty() {
        output.push_str("## Methods\n\n");
        for method in &doc.methods {
            let signature = if method.return_type.is_empty() {
                format!("{}{}", method.name, method.params)
            } else {
                format!("{}{} -> {}", method.name, method.params, method.return_type)
            };
            let _ = writeln!(output, "### {signature}\n");
            if !method.description.is_empty() {
                let _ = writeln!(output, "{}\n", method.description);
            }
        }
    }

    output
}

/// Generate documentation for `GDScript` files.
pub fn run_doc(paths: &[String], output_dir: &str, stdout: bool) -> Result<()> {
    let paths_to_process = if paths.is_empty() {
        vec![".".to_string()]
    } else {
        paths.to_vec()
    };

    let mut all_files = Vec::new();
    for path_str in &paths_to_process {
        let path = Path::new(path_str);
        if path.is_dir() {
            all_files.extend(collect_gdscript_files(path)?);
        } else if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("gd") {
            all_files.push(path.to_path_buf());
        }
    }

    if all_files.is_empty() {
        println!("{}", "No GDScript files found".yellow());
        return Ok(());
    }

    if !stdout {
        fs::create_dir_all(output_dir)
            .map_err(|e| miette!("Failed to create output directory: {e}"))?;
    }

    let mut generated_count = 0;
    for file_path in all_files {
        let (source, tree) = parse_file(&file_path)?;
        let doc = extract_docs(&source, &tree, &file_path);

        let markdown = render_markdown(&doc);

        if stdout {
            println!("{markdown}");
        } else {
            let output_file_name = format!("{}.md", doc.name);
            let output_path = Path::new(output_dir).join(output_file_name);
            fs::write(&output_path, markdown)
                .map_err(|e| miette!("Failed to write {}: {e}", output_path.display()))?;
            println!("{} {}", "Generated".green(), output_path.display());
            generated_count += 1;
        }
    }

    if !stdout {
        println!(
            "\n{} {} documentation file{}",
            "Generated".green().bold(),
            generated_count,
            if generated_count == 1 { "" } else { "s" }
        );
    }

    Ok(())
}

/// Output documentation as JSON to stdout.
pub fn run_doc_json(paths: &[String]) -> Result<()> {
    let paths_to_process = if paths.is_empty() {
        vec![".".to_string()]
    } else {
        paths.to_vec()
    };

    let mut all_files = Vec::new();
    for path_str in &paths_to_process {
        let path = Path::new(path_str);
        if path.is_dir() {
            all_files.extend(collect_gdscript_files(path)?);
        } else if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("gd") {
            all_files.push(path.to_path_buf());
        }
    }

    if all_files.is_empty() {
        println!("[]");
        return Ok(());
    }

    let mut docs = Vec::new();
    for file_path in all_files {
        let (source, tree) = parse_file(&file_path)?;
        let doc = extract_docs(&source, &tree, &file_path);
        docs.push(doc);
    }

    let json =
        serde_json::to_string_pretty(&docs).map_err(|e| miette!("failed to serialize: {e}"))?;
    println!("{json}");
    Ok(())
}

/// Check that all public methods have doc comments.
/// Returns Ok if all documented, Err with details if any are missing.
pub fn run_doc_check(paths: &[String]) -> Result<()> {
    let paths_to_process = if paths.is_empty() {
        vec![".".to_string()]
    } else {
        paths.to_vec()
    };

    let mut all_files = Vec::new();
    for path_str in &paths_to_process {
        let path = Path::new(path_str);
        if path.is_dir() {
            all_files.extend(collect_gdscript_files(path)?);
        } else if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("gd") {
            all_files.push(path.to_path_buf());
        }
    }

    if all_files.is_empty() {
        println!("No GDScript files found");
        return Ok(());
    }

    let mut undocumented: Vec<(String, String)> = Vec::new();
    let mut total_methods = 0usize;
    let mut documented_methods = 0usize;

    for file_path in &all_files {
        let (source, tree) = parse_file(file_path)?;
        let doc = extract_docs(&source, &tree, file_path);

        let file_display = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        for method in &doc.methods {
            // Skip private methods (prefixed with _)
            if method.name.starts_with('_') {
                continue;
            }
            total_methods += 1;
            if method.description.is_empty() {
                undocumented.push((file_display.to_string(), method.name.clone()));
            } else {
                documented_methods += 1;
            }
        }
    }

    if undocumented.is_empty() {
        println!(
            "{} All {total_methods} public method{} documented",
            "✓".green().bold(),
            if total_methods == 1 { "" } else { "s" }
        );
        Ok(())
    } else {
        for (file, method) in &undocumented {
            println!("{} {file}: {method}() missing doc comment", "✗".red());
        }
        println!(
            "\n{documented_methods}/{total_methods} public methods documented"
        );
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser::parse;

    #[test]
    fn test_extract_basic_class_doc() {
        let source = r"
## A player character.
class_name Player
extends CharacterBody2D

## Health points.
@export var health: int = 100

## Take damage and return remaining health.
func take_damage(amount: int) -> int:
    health -= amount
    return health
";

        let tree = parse(source).unwrap();
        let path = Path::new("player.gd");
        let doc = extract_docs(source, &tree, path);

        assert_eq!(doc.name, "Player");
        assert_eq!(doc.extends, "CharacterBody2D");
        assert_eq!(doc.description, "A player character.");
        assert_eq!(doc.properties.len(), 1);
        assert_eq!(doc.properties[0].name, "health");
        assert_eq!(doc.properties[0].type_hint, "int");
        assert!(doc.properties[0].is_exported);
        assert_eq!(doc.properties[0].description, "Health points.");
        assert_eq!(doc.methods.len(), 1);
        assert_eq!(doc.methods[0].name, "take_damage");
        assert_eq!(doc.methods[0].return_type, "int");
        assert_eq!(
            doc.methods[0].description,
            "Take damage and return remaining health."
        );
    }

    #[test]
    fn test_extract_signal_doc() {
        let source = r"
## Emitted when health changes.
signal health_changed(new_health: int)
";

        let tree = parse(source).unwrap();
        let path = Path::new("test.gd");
        let doc = extract_docs(source, &tree, path);

        assert_eq!(doc.signals.len(), 1);
        assert_eq!(doc.signals[0].name, "health_changed");
        assert_eq!(doc.signals[0].description, "Emitted when health changes.");
    }

    #[test]
    fn test_multiline_doc_comment() {
        let source = r"
## This is a multi-line
## documentation comment
## for a function.
func test_func():
    pass
";

        let tree = parse(source).unwrap();
        let path = Path::new("test.gd");
        let doc = extract_docs(source, &tree, path);

        assert_eq!(doc.methods.len(), 1);
        assert_eq!(
            doc.methods[0].description,
            "This is a multi-line\ndocumentation comment\nfor a function."
        );
    }

    #[test]
    fn test_doc_json_serialization() {
        let source = r"
## A player.
class_name Player
extends Node2D

## Move the player.
func move(direction: Vector2) -> void:
    pass

func _ready():
    pass
";
        let tree = parse(source).unwrap();
        let path = Path::new("player.gd");
        let doc = extract_docs(source, &tree, path);

        let json = serde_json::to_string_pretty(&doc).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["name"], "Player");
        assert_eq!(parsed["extends"], "Node2D");
        assert_eq!(parsed["methods"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["methods"][0]["name"], "move");
        assert_eq!(parsed["methods"][0]["description"], "Move the player.");
        // _ready has no doc comment → description field should be absent (skip_serializing_if)
        assert!(parsed["methods"][1].get("description").is_none());
    }

    #[test]
    fn test_doc_json_skip_empty_fields() {
        let source = "func helper():\n\tpass\n";
        let tree = parse(source).unwrap();
        let path = Path::new("helper.gd");
        let doc = extract_docs(source, &tree, path);

        let json = serde_json::to_string(&doc).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // No extends, description, signals, or properties
        assert!(parsed.get("extends").is_none());
        assert!(parsed.get("description").is_none());
        assert!(parsed.get("signals").is_none());
        assert!(parsed.get("properties").is_none());
    }
}
