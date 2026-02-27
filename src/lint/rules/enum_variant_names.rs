use tree_sitter::Node;
use crate::core::gd_ast::GdFile;

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct EnumVariantNames;

impl LintRule for EnumVariantNames {
    fn name(&self) -> &'static str {
        "enum-variant-names"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Style
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_node(file.node, source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "enum_definition" {
        check_enum(node, source, diags);
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_node(cursor.node(), source, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn check_enum(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let enum_name = node
        .child_by_field_name("name")
        .map(|n| source[n.byte_range()].to_string());

    let variant_names = collect_variant_names(node, source);
    if variant_names.len() < 2 {
        return;
    }

    // Check common prefix (by _-separated segments)
    if let Some(prefix) = common_underscore_prefix(&variant_names)
        && prefix_relates_to_enum(&prefix, enum_name.as_deref())
    {
        diags.push(LintDiagnostic {
            rule: "enum-variant-names",
            message: format!(
                "all variants of `{}` have prefix `{prefix}_`; consider removing it",
                enum_name.as_deref().unwrap_or("<anonymous>")
            ),
            severity: Severity::Warning,
            line: node.start_position().row,
            column: node.start_position().column,
            end_column: None,
            fix: None,
            context_lines: None,
        });
        return; // Don't double-report prefix and suffix
    }

    // Check common suffix
    if let Some(suffix) = common_underscore_suffix(&variant_names)
        && suffix_relates_to_enum(&suffix, enum_name.as_deref())
    {
        diags.push(LintDiagnostic {
            rule: "enum-variant-names",
            message: format!(
                "all variants of `{}` have suffix `_{suffix}`; consider removing it",
                enum_name.as_deref().unwrap_or("<anonymous>")
            ),
            severity: Severity::Warning,
            line: node.start_position().row,
            column: node.start_position().column,
            end_column: None,
            fix: None,
            context_lines: None,
        });
    }
}

fn collect_variant_names(enum_node: Node, source: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut cursor = enum_node.walk();
    if !cursor.goto_first_child() {
        return names;
    }
    loop {
        let child = cursor.node();
        if child.kind() == "enumerator_list" {
            let mut list_cursor = child.walk();
            if list_cursor.goto_first_child() {
                loop {
                    let item = list_cursor.node();
                    if item.kind() == "enumerator"
                        && let Some(name_node) = item.child(0)
                    {
                        names.push(source[name_node.byte_range()].to_string());
                    }
                    if !list_cursor.goto_next_sibling() {
                        break;
                    }
                }
            }
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    names
}

/// Find the longest common prefix of underscore-separated segments.
/// Returns the prefix segments joined with `_` (e.g. "COLOR" for COLOR_RED, COLOR_GREEN).
fn common_underscore_prefix(names: &[String]) -> Option<String> {
    let split: Vec<Vec<&str>> = names.iter().map(|n| n.split('_').collect()).collect();
    let min_segments = split.iter().map(Vec::len).min().unwrap_or(0);
    if min_segments < 2 {
        // Need at least 2 segments per name for a prefix to be meaningful
        // (prefix + remainder)
        return None;
    }

    let mut common_count = 0;
    for i in 0..min_segments - 1 {
        let first = split[0][i];
        if split.iter().all(|s| s[i] == first) {
            common_count = i + 1;
        } else {
            break;
        }
    }

    if common_count == 0 {
        return None;
    }

    Some(split[0][..common_count].join("_"))
}

/// Find the longest common suffix of underscore-separated segments.
fn common_underscore_suffix(names: &[String]) -> Option<String> {
    let split: Vec<Vec<&str>> = names.iter().map(|n| n.split('_').collect()).collect();
    let min_segments = split.iter().map(Vec::len).min().unwrap_or(0);
    if min_segments < 2 {
        return None;
    }

    let mut common_count = 0;
    for i in 1..min_segments {
        let first = split[0][split[0].len() - i];
        if split.iter().all(|s| s[s.len() - i] == first) {
            common_count = i;
        } else {
            break;
        }
    }

    if common_count == 0 {
        return None;
    }

    let seg = &split[0];
    Some(seg[seg.len() - common_count..].join("_"))
}

/// Check if a prefix relates to the enum name.
/// Convert both to a comparable form (uppercase, no underscores) and check containment.
fn prefix_relates_to_enum(prefix: &str, enum_name: Option<&str>) -> bool {
    let Some(name) = enum_name else {
        // Anonymous enum — still warn if 2+ segment prefix
        return prefix.contains('_') || prefix.len() >= 2;
    };
    let norm_prefix = prefix.replace('_', "").to_uppercase();
    let norm_name = to_upper_no_sep(name);
    // Prefix matches or contains the enum name, or enum name contains the prefix
    norm_prefix == norm_name || norm_prefix.contains(&norm_name) || norm_name.contains(&norm_prefix)
}

/// Check if a suffix relates to the enum name.
fn suffix_relates_to_enum(suffix: &str, enum_name: Option<&str>) -> bool {
    let Some(name) = enum_name else {
        return suffix.contains('_') || suffix.len() >= 2;
    };
    let norm_suffix = suffix.replace('_', "").to_uppercase();
    let norm_name = to_upper_no_sep(name);
    norm_suffix == norm_name || norm_suffix.contains(&norm_name) || norm_name.contains(&norm_suffix)
}

/// Convert PascalCase or snake_case to uppercase without separators.
/// "ItemType" -> "ITEMTYPE", "ITEM_TYPE" -> "ITEMTYPE"
fn to_upper_no_sep(name: &str) -> String {
    name.replace('_', "").to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;
    use crate::core::gd_ast;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        EnumVariantNames.check(&file, source, &config)
    }

    #[test]
    fn warns_on_common_prefix_matching_name() {
        let source = "enum Color { COLOR_RED, COLOR_GREEN, COLOR_BLUE }\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("prefix"));
        assert!(diags[0].message.contains("COLOR_"));
    }

    #[test]
    fn warns_on_common_suffix_matching_name() {
        let source = "enum ItemType { SWORD_TYPE, SHIELD_TYPE, POTION_TYPE }\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("suffix"));
        assert!(diags[0].message.contains("_TYPE"));
    }

    #[test]
    fn no_warning_without_common_affix() {
        let source = "enum State { IDLE, RUNNING, JUMPING }\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_clean_enum() {
        let source = "enum Color { RED, GREEN, BLUE }\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_single_variant() {
        let source = "enum Singleton { COLOR_ONLY }\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn prefix_partial_match() {
        // "ITEM" prefix, enum name "Item" -> should match
        let source = "enum Item { ITEM_SWORD, ITEM_SHIELD }\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("prefix"));
    }

    #[test]
    fn no_warning_unrelated_prefix() {
        // Common prefix "BIG" doesn't relate to enum name "Size"
        let source = "enum Size { BIG_SMALL, BIG_LARGE }\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn opt_in_rule() {
        assert!(!EnumVariantNames.default_enabled());
    }

    #[test]
    fn suffix_with_pascal_case_name() {
        // "State" enum, variants end with "_STATE"
        let source = "enum State { IDLE_STATE, RUNNING_STATE, JUMPING_STATE }\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("suffix"));
    }

    #[test]
    fn handles_enum_with_values() {
        let source = "enum Color { COLOR_RED = 0, COLOR_GREEN = 1, COLOR_BLUE = 2 }\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }
}
