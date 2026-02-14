use std::path::Path;

use miette::Result;
use serde::Serialize;

use super::{
    declaration_full_range, declaration_kind_str, find_declaration_by_name, get_declaration_name,
    normalize_blank_lines,
};

#[derive(Serialize, Debug)]
pub struct ExtractClassOutput {
    pub extracted: Vec<ExtractedSymbol>,
    pub from: String,
    pub to: String,
    pub applied: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Serialize, Debug)]
pub struct ExtractedSymbol {
    pub name: String,
    pub kind: String,
}

/// Extract multiple symbols from a file into a new (or existing) file,
/// updating internal cross-references.
#[allow(clippy::too_many_lines)]
pub fn extract_class(
    file: &Path,
    names: &[String],
    to_file: &Path,
    dry_run: bool,
    project_root: &Path,
) -> Result<ExtractClassOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = crate::core::parser::parse(&source)?;
    let root = tree.root_node();

    let from_relative = crate::core::fs::relative_slash(file, project_root);
    let to_relative = crate::core::fs::relative_slash(to_file, project_root);

    // Find all declarations to extract: (name, kind, start_byte, end_byte, text)
    let mut extractions: Vec<(String, String, usize, usize, String)> = Vec::new();
    let mut not_found = Vec::new();

    for name in names {
        let Some(decl) = find_declaration_by_name(root, &source, name) else {
            not_found.push(name.clone());
            continue;
        };
        let kind = declaration_kind_str(decl.kind()).to_string();
        let (start, end) = declaration_full_range(decl, &source);
        let text = source[start..end].to_string();
        extractions.push((name.clone(), kind, start, end, text));
    }

    if !not_found.is_empty() {
        return Err(miette::miette!(
            "symbols not found: {}",
            not_found.join(", ")
        ));
    }

    if extractions.is_empty() {
        return Err(miette::miette!("no symbols specified"));
    }

    // Check target for name conflicts
    if to_file.exists() {
        let target_source = std::fs::read_to_string(to_file)
            .map_err(|e| miette::miette!("cannot read target file: {e}"))?;
        let target_tree = crate::core::parser::parse(&target_source)?;
        let target_root = target_tree.root_node();
        for (name, _, _, _, _) in &extractions {
            if find_declaration_by_name(target_root, &target_source, name).is_some() {
                return Err(miette::miette!(
                    "target already contains a declaration named '{name}'"
                ));
            }
        }
    }

    // Detect cross-references: symbols being extracted that reference
    // symbols NOT being extracted (and vice versa)
    let mut warnings = Vec::new();
    let extracted_names: Vec<&str> = extractions
        .iter()
        .map(|(n, _, _, _, _)| n.as_str())
        .collect();

    // Check for references from extracted symbols to non-extracted symbols
    let workspace = crate::lsp::workspace::WorkspaceIndex::new(project_root.to_path_buf());
    for (name, _, _, _, _) in &extractions {
        let refs =
            crate::lsp::references::find_references_by_name(name, &workspace, Some(file), None);
        let file_uri = tower_lsp::lsp_types::Url::from_file_path(file).ok();
        for loc in &refs {
            // Check if this reference is from a non-extracted symbol in the same file
            if let Some(ref uri) = file_uri
                && &loc.uri == uri
            {
                let ref_line = loc.range.start.line as usize;
                // Check if this reference is within a non-extracted declaration
                for decl_check in root.children(&mut root.walk()) {
                    if super::DECLARATION_KINDS.contains(&decl_check.kind())
                        && let Some(decl_name) = get_declaration_name(decl_check, &source)
                        && !extracted_names.contains(&decl_name.as_str())
                        && decl_check.start_position().row <= ref_line
                        && ref_line <= decl_check.end_position().row
                    {
                        warnings.push(format!(
                            "'{decl_name}' (staying) references '{name}' (moving)"
                        ));
                        break;
                    }
                }
            }
        }
    }

    // Check for cross-file references to extracted symbols
    for (name, _, _, _, _) in &extractions {
        let refs = crate::lsp::references::find_references_by_name(name, &workspace, None, None);
        let file_uri = tower_lsp::lsp_types::Url::from_file_path(file).ok();
        let cross_file = refs
            .iter()
            .filter(|loc| {
                if let Some(ref uri) = file_uri {
                    &loc.uri != uri
                } else {
                    true
                }
            })
            .count();
        if cross_file > 0 {
            warnings.push(format!(
                "'{name}' has {cross_file} cross-file reference(s) that may need updating"
            ));
        }
    }

    // Deduplicate warnings
    warnings.sort();
    warnings.dedup();

    let extracted_output: Vec<ExtractedSymbol> = extractions
        .iter()
        .map(|(n, k, _, _, _)| ExtractedSymbol {
            name: n.clone(),
            kind: k.clone(),
        })
        .collect();

    if !dry_run {
        // Build target content
        let mut target_content = if to_file.exists() {
            std::fs::read_to_string(to_file)
                .map_err(|e| miette::miette!("cannot read target file: {e}"))?
        } else {
            String::new()
        };

        // Add symbols in their original order
        for (_, kind, _, _, text) in &extractions {
            let spacing = insertion_spacing(kind, &target_content);
            target_content.push_str(&spacing);
            target_content.push_str(text);
        }

        // Create parent directories if needed
        if let Some(parent) = to_file.parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent)
                .map_err(|e| miette::miette!("cannot create directory: {e}"))?;
        }

        std::fs::write(to_file, &target_content)
            .map_err(|e| miette::miette!("cannot write target file: {e}"))?;

        // Remove extracted symbols from source (bottom to top)
        let mut sorted = extractions.clone();
        sorted.sort_by(|a, b| b.2.cmp(&a.2)); // sort by start_byte descending

        let mut new_source = source.clone();
        for (_, _, start, end, _) in &sorted {
            new_source.replace_range(*start..*end, "");
        }
        normalize_blank_lines(&mut new_source);
        std::fs::write(file, &new_source)
            .map_err(|e| miette::miette!("cannot write source file: {e}"))?;
    }

    Ok(ExtractClassOutput {
        extracted: extracted_output,
        from: from_relative,
        to: to_relative,
        applied: !dry_run,
        warnings,
    })
}

/// Determine blank-line spacing before inserting a declaration.
fn insertion_spacing(decl_kind: &str, target_source: &str) -> String {
    let trimmed = target_source.trim_end();
    if trimmed.is_empty() {
        return String::new();
    }

    let needs_extra = matches!(
        decl_kind,
        "function"
            | "class"
            | "function_definition"
            | "constructor_definition"
            | "class_definition"
    );

    let trailing_newlines = target_source.len() - trimmed.len();

    if needs_extra {
        if trailing_newlines >= 3 {
            String::new()
        } else {
            "\n".repeat(3 - trailing_newlines)
        }
    } else if trailing_newlines >= 2 {
        String::new()
    } else {
        "\n".repeat(2 - trailing_newlines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_project(files: &[(&str, &str)]) -> TempDir {
        let temp = tempfile::Builder::new()
            .prefix("gdtest")
            .tempdir()
            .expect("create temp dir");
        fs::write(
            temp.path().join("project.godot"),
            "[application]\nconfig/name=\"test\"\n",
        )
        .expect("write project.godot");
        for (name, content) in files {
            fs::write(temp.path().join(name), content).expect("write file");
        }
        temp
    }

    #[test]
    fn extract_to_new_file() {
        let temp = setup_project(&[(
            "player.gd",
            "var speed = 10\n\n\nfunc helper():\n\tpass\n\n\nfunc keep():\n\tpass\n",
        )]);
        let names = vec!["speed".to_string(), "helper".to_string()];
        let result = extract_class(
            &temp.path().join("player.gd"),
            &names,
            &temp.path().join("extracted.gd"),
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.extracted.len(), 2);

        let source = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(!source.contains("var speed"), "speed should be extracted");
        assert!(
            !source.contains("func helper"),
            "helper should be extracted"
        );
        assert!(source.contains("func keep"), "keep should stay");

        let target = fs::read_to_string(temp.path().join("extracted.gd")).unwrap();
        assert!(target.contains("var speed"), "speed should be in target");
        assert!(target.contains("func helper"), "helper should be in target");
    }

    #[test]
    fn extract_to_existing_file() {
        let temp = setup_project(&[
            (
                "player.gd",
                "var a = 1\nvar b = 2\n\n\nfunc keep():\n\tpass\n",
            ),
            ("target.gd", "var existing = 0\n"),
        ]);
        let names = vec!["a".to_string(), "b".to_string()];
        let result = extract_class(
            &temp.path().join("player.gd"),
            &names,
            &temp.path().join("target.gd"),
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);

        let target = fs::read_to_string(temp.path().join("target.gd")).unwrap();
        assert!(target.contains("var existing"));
        assert!(target.contains("var a"));
        assert!(target.contains("var b"));
    }

    #[test]
    fn extract_duplicate_error() {
        let temp = setup_project(&[
            ("player.gd", "func helper():\n\tpass\n"),
            ("target.gd", "func helper():\n\treturn 1\n"),
        ]);
        let names = vec!["helper".to_string()];
        let result = extract_class(
            &temp.path().join("player.gd"),
            &names,
            &temp.path().join("target.gd"),
            false,
            temp.path(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn extract_not_found_error() {
        let temp = setup_project(&[("player.gd", "var x = 1\n")]);
        let names = vec!["nonexistent".to_string()];
        let result = extract_class(
            &temp.path().join("player.gd"),
            &names,
            &temp.path().join("target.gd"),
            false,
            temp.path(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn extract_dry_run() {
        let temp = setup_project(&[("player.gd", "var speed = 10\n\n\nfunc keep():\n\tpass\n")]);
        let names = vec!["speed".to_string()];
        let result = extract_class(
            &temp.path().join("player.gd"),
            &names,
            &temp.path().join("target.gd"),
            true,
            temp.path(),
        )
        .unwrap();
        assert!(!result.applied);
        assert!(!temp.path().join("target.gd").exists());
        let source = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(source.contains("var speed"), "dry run should not modify");
    }

    #[test]
    fn extract_preserves_doc_comments() {
        let temp = setup_project(&[(
            "player.gd",
            "## The speed\nvar speed = 10\n\n\nfunc keep():\n\tpass\n",
        )]);
        let names = vec!["speed".to_string()];
        let result = extract_class(
            &temp.path().join("player.gd"),
            &names,
            &temp.path().join("target.gd"),
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let target = fs::read_to_string(temp.path().join("target.gd")).unwrap();
        assert!(
            target.contains("## The speed"),
            "doc comment should be preserved"
        );
        assert!(target.contains("var speed"));
    }
}
