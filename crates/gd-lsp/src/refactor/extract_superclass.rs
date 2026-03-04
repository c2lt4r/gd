use std::fmt::Write as _;
use std::path::Path;

use miette::Result;
use serde::Serialize;

use gd_core::gd_ast::{self, GdExtends};

use super::{declaration_full_range, find_declaration_by_name, normalize_blank_lines};

#[derive(Serialize, Debug)]
pub struct ExtractSuperclassOutput {
    pub extracted: Vec<ExtractedMember>,
    pub from: String,
    pub to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class_name: Option<String>,
    pub old_extends: Option<String>,
    pub new_extends: String,
    pub applied: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Serialize, Debug)]
pub struct ExtractedMember {
    pub name: String,
    pub kind: String,
}

/// Extract specified members from a class into a new superclass file.
///
/// The new file inherits the original `extends` (if any), and the original file
/// is updated to extend the new superclass instead.
#[allow(clippy::too_many_lines)]
pub fn extract_superclass(
    file: &Path,
    names: &[String],
    to_file: &Path,
    class_name: Option<&str>,
    dry_run: bool,
    project_root: &Path,
) -> Result<ExtractSuperclassOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = gd_core::parser::parse(&source)?;
    let gd_file = gd_ast::convert(&tree, &source);

    let from_relative = gd_core::fs::relative_slash(file, project_root);
    let to_relative = gd_core::fs::relative_slash(to_file, project_root);

    // Resolve the original extends and class_name
    let orig_extends = match gd_file.extends {
        Some(GdExtends::Class(name)) => Some(name.to_string()),
        Some(GdExtends::Path(path)) => Some(format!("\"{path}\"")),
        None => None,
    };
    let orig_extends_range = gd_file.extends_node.map(|n| {
        let start = n.start_byte();
        let mut end = n.end_byte();
        // Include trailing newline
        if end < source.len() && source.as_bytes()[end] == b'\n' {
            end += 1;
        }
        (start, end)
    });

    // Find all declarations to extract
    let mut extractions: Vec<(String, String, usize, usize, String)> = Vec::new();
    let mut not_found = Vec::new();

    for name in names {
        let Some(decl_node) = find_declaration_by_name(&gd_file, name) else {
            not_found.push(name.clone());
            continue;
        };
        let kind = gd_file
            .find_decl_by_name(name)
            .map_or_else(|| "class_name".to_string(), |d| d.kind_str().to_string());
        let (start, end) = declaration_full_range(decl_node, &source);
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

    if to_file.exists() {
        return Err(miette::miette!("target file already exists: {to_relative}"));
    }

    // Check for dependency warnings
    let mut warnings = Vec::new();
    let extracted_names: Vec<&str> = extractions
        .iter()
        .map(|(n, _, _, _, _)| n.as_str())
        .collect();

    // Scan extracted symbol bodies for references to non-extracted symbols
    let all_decl_names: Vec<String> = gd_file
        .declarations
        .iter()
        .filter(|d| d.is_declaration())
        .map(|d| d.name().to_string())
        .filter(|n| !n.is_empty())
        .collect();
    for (name, _, start, end, _) in &extractions {
        let body = &source[*start..*end];
        for decl_name in &all_decl_names {
            if !extracted_names.contains(&decl_name.as_str())
                && decl_name != "class_name"
                && body.contains(decl_name.as_str())
                && is_identifier_reference(body, decl_name)
            {
                warnings.push(format!(
                    "'{name}' (moving) references '{decl_name}' (staying in subclass)"
                ));
            }
        }
    }

    // Check for non-extracted symbols referencing extracted ones
    for decl in &gd_file.declarations {
        if !decl.is_declaration() {
            continue;
        }
        let decl_name = decl.name();
        if decl_name.is_empty() || extracted_names.contains(&decl_name) {
            continue;
        }
        let (s, e) = declaration_full_range(decl.node(), &source);
        let body = &source[s..e];
        for extracted_name in &extracted_names {
            if body.contains(extracted_name) && is_identifier_reference(body, extracted_name) {
                warnings.push(format!(
                    "'{decl_name}' (staying) references '{extracted_name}' (moving to superclass) — will inherit"
                ));
            }
        }
    }

    warnings.sort();
    warnings.dedup();

    // Determine extends for the new superclass
    let superclass_extends = orig_extends.clone();

    // Determine what the original file should extend
    let new_extends_value = if let Some(cn) = class_name {
        cn.to_string()
    } else {
        format!("\"res://{to_relative}\"")
    };

    let extracted_output: Vec<ExtractedMember> = extractions
        .iter()
        .map(|(n, k, _, _, _)| ExtractedMember {
            name: n.clone(),
            kind: k.clone(),
        })
        .collect();

    if !dry_run {
        let mut tx = super::transaction::RefactorTransaction::new();

        // Build superclass file content
        let mut superclass_content = String::new();

        // Add extends to superclass (inherits from original's extends)
        if let Some(ref ext) = superclass_extends {
            let _ = writeln!(superclass_content, "extends {ext}");
        }

        // Add class_name if specified
        if let Some(cn) = class_name {
            let _ = writeln!(superclass_content, "class_name {cn}");
        }

        // Add blank line separator before members
        if !superclass_content.is_empty() {
            superclass_content.push('\n');
        }

        // Add extracted symbols
        let mut first = true;
        for (_, kind, _, _, text) in &extractions {
            if !first {
                let spacing = insertion_spacing(kind, &superclass_content);
                superclass_content.push_str(&spacing);
            }
            superclass_content.push_str(text);
            first = false;
        }

        // Ensure trailing newline
        if !superclass_content.ends_with('\n') {
            superclass_content.push('\n');
        }

        // Create parent directories if needed
        if let Some(parent) = to_file.parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent)
                .map_err(|e| miette::miette!("cannot create directory: {e}"))?;
        }

        super::validate_no_new_errors("", &superclass_content)?;
        tx.write_file(to_file, &superclass_content)?;

        // Modify source file: remove extracted symbols and update extends
        let mut sorted = extractions.clone();
        sorted.sort_by(|a, b| b.2.cmp(&a.2)); // sort by start_byte descending

        let mut new_source = source.clone();

        // Remove extracted symbols (bottom to top to preserve indices)
        for (_, _, start, end, _) in &sorted {
            new_source.replace_range(*start..*end, "");
        }

        // Update or add extends statement
        if let Some((ext_start, ext_end)) = orig_extends_range {
            // Recalculate offset after removals
            let removed_before: usize = sorted
                .iter()
                .filter(|(_, _, s, _, _)| *s < ext_start)
                .map(|(_, _, s, e, _)| e - s)
                .sum();
            let adj_start = ext_start - removed_before;
            let adj_end = ext_end - removed_before;
            new_source.replace_range(
                adj_start..adj_end,
                &format!("extends {new_extends_value}\n"),
            );
        } else {
            // No extends statement — insert at the top
            new_source.insert_str(0, &format!("extends {new_extends_value}\n\n"));
        }

        normalize_blank_lines(&mut new_source);
        super::validate_no_new_errors(&source, &new_source)?;
        tx.write_file(file, &new_source)?;

        let snapshots = tx.into_snapshots();
        let stack = super::undo::UndoStack::open(project_root);
        let _ = stack.record(
            "extract-superclass",
            &format!("extract {} to {}", names.join(", "), to_relative),
            &snapshots,
            project_root,
        );
    }

    Ok(ExtractSuperclassOutput {
        extracted: extracted_output,
        from: from_relative,
        to: to_relative,
        class_name: class_name.map(String::from),
        old_extends: orig_extends,
        new_extends: new_extends_value,
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

/// Quick check if `name` appears as an identifier reference (not part of another word).
fn is_identifier_reference(text: &str, name: &str) -> bool {
    let name_bytes = name.as_bytes();
    let text_bytes = text.as_bytes();
    let mut pos = 0;
    while pos + name_bytes.len() <= text_bytes.len() {
        if let Some(idx) = text[pos..].find(name) {
            let abs = pos + idx;
            let before_ok = abs == 0 || !is_ident_char(text_bytes[abs - 1]);
            let after_ok = abs + name_bytes.len() >= text_bytes.len()
                || !is_ident_char(text_bytes[abs + name_bytes.len()]);
            if before_ok && after_ok {
                return true;
            }
            pos = abs + 1;
        } else {
            break;
        }
    }
    false
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
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
            let path = temp.path().join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create parent dirs");
            }
            fs::write(&path, content).expect("write file");
        }
        temp
    }

    #[test]
    fn basic_extract_superclass() {
        let temp = setup_project(&[(
            "player.gd",
            "extends Node2D\n\nvar health = 100\n\n\nfunc take_damage(amount):\n\tvar new_hp = health - amount\n\thealth = new_hp\n\n\nfunc move_player():\n\tpass\n",
        )]);
        let names = vec!["health".to_string(), "take_damage".to_string()];
        let result = extract_superclass(
            &temp.path().join("player.gd"),
            &names,
            &temp.path().join("base_entity.gd"),
            Some("BaseEntity"),
            false,
            temp.path(),
        )
        .unwrap();

        assert!(result.applied);
        assert_eq!(result.extracted.len(), 2);
        assert_eq!(result.old_extends, Some("Node2D".to_string()));
        assert_eq!(result.new_extends, "BaseEntity");

        // Check superclass file
        let superclass = fs::read_to_string(temp.path().join("base_entity.gd")).unwrap();
        assert!(
            superclass.contains("extends Node2D"),
            "superclass should extend Node2D"
        );
        assert!(
            superclass.contains("class_name BaseEntity"),
            "should have class_name"
        );
        assert!(superclass.contains("var health"), "health should be moved");
        assert!(
            superclass.contains("func take_damage"),
            "take_damage should be moved"
        );

        // Check source file
        let source = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            source.contains("extends BaseEntity"),
            "source should extend BaseEntity"
        );
        assert!(!source.contains("var health"), "health should be removed");
        assert!(
            !source.contains("func take_damage"),
            "take_damage should be removed"
        );
        assert!(
            source.contains("func move_player"),
            "move_player should remain"
        );
    }

    #[test]
    fn extract_superclass_no_class_name() {
        let temp = setup_project(&[(
            "player.gd",
            "extends Node2D\n\nvar speed = 10\n\n\nfunc run():\n\tpass\n",
        )]);
        let names = vec!["speed".to_string()];
        let result = extract_superclass(
            &temp.path().join("player.gd"),
            &names,
            &temp.path().join("base.gd"),
            None,
            false,
            temp.path(),
        )
        .unwrap();

        assert!(result.applied);
        assert!(result.class_name.is_none());
        assert_eq!(result.new_extends, "\"res://base.gd\"");

        let source = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(source.contains("extends \"res://base.gd\""));

        let superclass = fs::read_to_string(temp.path().join("base.gd")).unwrap();
        assert!(superclass.contains("extends Node2D"));
        assert!(!superclass.contains("class_name"));
    }

    #[test]
    fn extract_superclass_no_original_extends() {
        let temp = setup_project(&[("player.gd", "var health = 100\n\n\nfunc die():\n\tpass\n")]);
        let names = vec!["health".to_string()];
        let result = extract_superclass(
            &temp.path().join("player.gd"),
            &names,
            &temp.path().join("base.gd"),
            Some("Base"),
            false,
            temp.path(),
        )
        .unwrap();

        assert!(result.applied);
        assert!(result.old_extends.is_none());

        let source = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(source.contains("extends Base"));

        let superclass = fs::read_to_string(temp.path().join("base.gd")).unwrap();
        assert!(
            !superclass.contains("extends"),
            "no extends when original had none"
        );
        assert!(superclass.contains("class_name Base"));
    }

    #[test]
    fn extract_superclass_dry_run() {
        let temp = setup_project(&[("player.gd", "extends Node2D\n\nvar speed = 10\n")]);
        let names = vec!["speed".to_string()];
        let result = extract_superclass(
            &temp.path().join("player.gd"),
            &names,
            &temp.path().join("base.gd"),
            Some("Base"),
            true,
            temp.path(),
        )
        .unwrap();

        assert!(!result.applied);
        assert!(
            !temp.path().join("base.gd").exists(),
            "dry run should not create file"
        );
        let source = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            source.contains("extends Node2D"),
            "dry run should not modify source"
        );
    }

    #[test]
    fn extract_superclass_symbol_not_found() {
        let temp = setup_project(&[("player.gd", "extends Node2D\n\nvar speed = 10\n")]);
        let names = vec!["nonexistent".to_string()];
        let result = extract_superclass(
            &temp.path().join("player.gd"),
            &names,
            &temp.path().join("base.gd"),
            None,
            false,
            temp.path(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn extract_superclass_target_exists() {
        let temp = setup_project(&[
            ("player.gd", "extends Node2D\n\nvar speed = 10\n"),
            ("base.gd", "var existing = 0\n"),
        ]);
        let names = vec!["speed".to_string()];
        let result = extract_superclass(
            &temp.path().join("player.gd"),
            &names,
            &temp.path().join("base.gd"),
            None,
            false,
            temp.path(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn extract_superclass_preserves_doc_comments() {
        let temp = setup_project(&[(
            "player.gd",
            "extends Node2D\n\n## The health value\nvar health = 100\n\n\nfunc keep():\n\tpass\n",
        )]);
        let names = vec!["health".to_string()];
        let result = extract_superclass(
            &temp.path().join("player.gd"),
            &names,
            &temp.path().join("base.gd"),
            Some("Base"),
            false,
            temp.path(),
        )
        .unwrap();

        assert!(result.applied);
        let superclass = fs::read_to_string(temp.path().join("base.gd")).unwrap();
        assert!(
            superclass.contains("## The health value"),
            "doc comment should be preserved"
        );
        assert!(superclass.contains("var health"));
    }

    #[test]
    fn extract_superclass_signals_and_functions() {
        let temp = setup_project(&[(
            "player.gd",
            "extends CharacterBody2D\n\nsignal died\nvar health = 100\n\n\nfunc take_damage(n):\n\thealth -= n\n\tif health <= 0:\n\t\tdied.emit()\n\n\nfunc input():\n\tpass\n",
        )]);
        let names = vec![
            "died".to_string(),
            "health".to_string(),
            "take_damage".to_string(),
        ];
        let result = extract_superclass(
            &temp.path().join("player.gd"),
            &names,
            &temp.path().join("base_entity.gd"),
            Some("BaseEntity"),
            false,
            temp.path(),
        )
        .unwrap();

        assert!(result.applied);
        assert_eq!(result.extracted.len(), 3);

        let superclass = fs::read_to_string(temp.path().join("base_entity.gd")).unwrap();
        assert!(superclass.contains("extends CharacterBody2D"));
        assert!(superclass.contains("class_name BaseEntity"));
        assert!(superclass.contains("signal died"));
        assert!(superclass.contains("var health"));
        assert!(superclass.contains("func take_damage"));

        let source = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(source.contains("extends BaseEntity"));
        assert!(source.contains("func input"));
        assert!(!source.contains("signal died"));
    }

    #[test]
    fn extract_superclass_with_subdirectory() {
        let temp = setup_project(&[(
            "entities/player.gd",
            "extends Node2D\n\nvar health = 100\n\n\nfunc keep():\n\tpass\n",
        )]);
        let names = vec!["health".to_string()];
        let result = extract_superclass(
            &temp.path().join("entities/player.gd"),
            &names,
            &temp.path().join("entities/base_entity.gd"),
            Some("BaseEntity"),
            false,
            temp.path(),
        )
        .unwrap();

        assert!(result.applied);
        assert!(temp.path().join("entities/base_entity.gd").exists());
    }

    #[test]
    fn is_identifier_reference_basic() {
        assert!(is_identifier_reference("health -= 1", "health"));
        assert!(is_identifier_reference("return health", "health"));
        assert!(!is_identifier_reference("var my_health = 0", "health"));
        assert!(!is_identifier_reference("healthbar.show()", "health"));
    }
}
