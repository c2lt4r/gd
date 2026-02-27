use crate::core::gd_ast::{self, GdDecl, GdFile};

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

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_decls(file, &mut |decl| {
            if let GdDecl::Enum(e) = decl {
                let enum_name = if e.name.is_empty() { None } else { Some(e.name) };
                let variant_names: Vec<&str> = e.members.iter().map(|m| m.name).collect();
                if variant_names.len() < 2 {
                    return;
                }

                // Check common prefix (by _-separated segments)
                if let Some(prefix) = common_underscore_prefix(&variant_names)
                    && prefix_relates_to_enum(&prefix, enum_name)
                {
                    diags.push(LintDiagnostic {
                        rule: "enum-variant-names",
                        message: format!(
                            "all variants of `{}` have prefix `{prefix}_`; consider removing it",
                            enum_name.unwrap_or("<anonymous>"),
                        ),
                        severity: Severity::Warning,
                        line: e.node.start_position().row,
                        column: e.node.start_position().column,
                        end_column: None,
                        fix: None,
                        context_lines: None,
                    });
                    return; // Don't double-report prefix and suffix
                }

                // Check common suffix
                if let Some(suffix) = common_underscore_suffix(&variant_names)
                    && suffix_relates_to_enum(&suffix, enum_name)
                {
                    diags.push(LintDiagnostic {
                        rule: "enum-variant-names",
                        message: format!(
                            "all variants of `{}` have suffix `_{suffix}`; consider removing it",
                            enum_name.unwrap_or("<anonymous>"),
                        ),
                        severity: Severity::Warning,
                        line: e.node.start_position().row,
                        column: e.node.start_position().column,
                        end_column: None,
                        fix: None,
                        context_lines: None,
                    });
                }
            }
        });
        diags
    }
}

/// Find the longest common prefix of underscore-separated segments.
/// Returns the prefix segments joined with `_` (e.g. "COLOR" for COLOR_RED, COLOR_GREEN).
fn common_underscore_prefix(names: &[&str]) -> Option<String> {
    let split: Vec<Vec<&str>> = names.iter().map(|n| n.split('_').collect()).collect();
    let min_segments = split.iter().map(Vec::len).min().unwrap_or(0);
    if min_segments < 2 {
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
fn common_underscore_suffix(names: &[&str]) -> Option<String> {
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
fn prefix_relates_to_enum(prefix: &str, enum_name: Option<&str>) -> bool {
    let Some(name) = enum_name else {
        return prefix.contains('_') || prefix.len() >= 2;
    };
    let norm_prefix = prefix.replace('_', "").to_uppercase();
    let norm_name = to_upper_no_sep(name);
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
