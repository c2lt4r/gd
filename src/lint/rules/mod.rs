//! Lint rules module - each rule analyzes the tree-sitter AST.

pub mod assert_always_false;
pub mod assert_always_true;
pub mod await_in_ready;
pub mod breakpoint_statement;
pub mod comparison_with_boolean;
pub mod comparison_with_itself;
pub mod duplicate_function;
pub mod duplicate_key;
pub mod duplicate_signal;
pub mod duplicated_load;
pub mod empty_function;
pub mod float_comparison;
pub mod integer_division;
pub mod long_function;
pub mod magic_number;
pub mod missing_return;
pub mod missing_type_hint;
pub mod monitoring_in_signal;
pub mod naming_convention;
pub mod node_ready_order;
pub mod null_after_await;
pub mod preload_type_hint;
pub mod private_method_access;
pub mod return_type_mismatch;
pub mod self_assignment;
pub mod shadowed_variable;
pub mod signal_name_convention;
pub mod standalone_expression;
pub mod standalone_ternary;
pub mod static_type_inference;
pub mod unnecessary_pass;
pub mod unreachable_code;
pub mod unsafe_void_return;
pub mod untyped_array;
pub mod untyped_array_literal;
pub mod unused_preload;
pub mod unused_signal;
pub mod unused_variable;
pub mod use_before_assign;
pub mod variant_inference;

pub mod callable_null_check;
pub mod class_definitions_order;

pub mod cyclomatic_complexity;
pub mod deeply_nested_code;
pub mod duplicate_delegate;
pub mod enum_naming;
pub mod enum_variable_without_default;
pub mod enum_without_class_name;
pub mod get_node_default_without_onready;
pub mod get_node_in_process;
pub mod god_object;
pub mod incompatible_ternary;
pub mod look_at_before_tree;
pub mod loop_variable_name;
pub mod max_file_lines;
pub mod max_line_length;
pub mod max_public_methods;
pub mod narrowing_conversion;
pub mod native_method_override;
pub mod onready_with_export;
pub mod parameter_naming;
pub mod parameter_shadows_field;
pub mod physics_in_process;
pub mod print_statement;
pub mod redundant_else;
pub mod redundant_static_unload;
pub mod return_value_discarded;
pub mod signal_not_connected;
pub mod todo_comment;
pub mod too_many_parameters;
pub mod unused_parameter;
pub mod unused_private_class_variable;

pub mod missing_tool;
pub mod shadowed_variable_base_class;
pub mod static_called_on_instance;

use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use tree_sitter::Tree;

use crate::core::config::{LintConfig, RuleConfig};
use crate::core::symbol_table::SymbolTable;
use crate::core::workspace_index::ProjectIndex;

/// Clippy-style category for organizing lint rules.
/// Each rule belongs to exactly one category. Categories can be bulk-enabled
/// or disabled via `[lint]` in gd.toml.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LintCategory {
    /// Definite bugs
    Correctness,
    /// Likely bugs, may be intentional
    Suspicious,
    /// Naming and code style
    Style,
    /// Code size and complexity metrics
    Complexity,
    /// Godot runtime performance
    Performance,
    /// Godot engine best practices
    Godot,
    /// Type system strictness
    TypeSafety,
    /// Unused code and debug artifacts
    Maintenance,
}

impl fmt::Display for LintCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LintCategory::Correctness => write!(f, "correctness"),
            LintCategory::Suspicious => write!(f, "suspicious"),
            LintCategory::Style => write!(f, "style"),
            LintCategory::Complexity => write!(f, "complexity"),
            LintCategory::Performance => write!(f, "performance"),
            LintCategory::Godot => write!(f, "godot"),
            LintCategory::TypeSafety => write!(f, "type_safety"),
            LintCategory::Maintenance => write!(f, "maintenance"),
        }
    }
}

impl FromStr for LintCategory {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "correctness" => Ok(LintCategory::Correctness),
            "suspicious" => Ok(LintCategory::Suspicious),
            "style" => Ok(LintCategory::Style),
            "complexity" => Ok(LintCategory::Complexity),
            "performance" => Ok(LintCategory::Performance),
            "godot" => Ok(LintCategory::Godot),
            "type_safety" => Ok(LintCategory::TypeSafety),
            "maintenance" => Ok(LintCategory::Maintenance),
            _ => Err(format!(
                "invalid category '{s}': expected correctness, suspicious, style, complexity, performance, godot, type_safety, or maintenance"
            )),
        }
    }
}

/// Severity of a lint diagnostic.
/// Ordered: Info < Warning < Error (used for `--severity` filtering).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl FromStr for Severity {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "info" => Ok(Severity::Info),
            "warning" => Ok(Severity::Warning),
            "error" => Ok(Severity::Error),
            _ => Err(format!(
                "invalid severity '{s}': expected info, warning, or error"
            )),
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Info => write!(f, "info"),
            Severity::Warning => write!(f, "warning"),
            Severity::Error => write!(f, "error"),
        }
    }
}

/// A single lint diagnostic.
#[derive(Debug, serde::Serialize)]
pub struct LintDiagnostic {
    pub rule: &'static str,
    pub message: String,
    pub severity: Severity,
    pub line: usize,
    pub column: usize,
    /// End column of the span (exclusive). Used for underline display.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_column: Option<usize>,
    /// If this diagnostic is auto-fixable, this holds the replacement.
    #[serde(skip)]
    pub fix: Option<Fix>,
    /// Surrounding source lines (populated by `--context N`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_lines: Option<Vec<String>>,
}

/// An auto-fix: replace a byte range with new text.
#[derive(Debug, Clone)]
pub struct Fix {
    pub byte_start: usize,
    pub byte_end: usize,
    pub replacement: String,
}

/// Trait implemented by each lint rule.
pub trait LintRule: Send + Sync {
    /// Unique rule identifier (e.g. "naming-convention").
    fn name(&self) -> &'static str;

    /// Category this rule belongs to (e.g. correctness, style, performance).
    fn category(&self) -> LintCategory;

    /// Whether this rule is enabled by default. Opt-in rules return false
    /// and must be explicitly enabled via `[lint.rules.<name>]` in gd.toml.
    fn default_enabled(&self) -> bool {
        true
    }

    /// Run the rule against a parsed file and return diagnostics.
    fn check(&self, tree: &Tree, source: &str, config: &LintConfig) -> Vec<LintDiagnostic>;

    /// Run the rule with access to the per-file symbol table.
    /// Default delegates to `check()`, ignoring the symbol table.
    /// Override this in rules that need declaration-level type information.
    fn check_with_symbols(
        &self,
        tree: &Tree,
        source: &str,
        config: &LintConfig,
        _symbols: &SymbolTable,
    ) -> Vec<LintDiagnostic> {
        self.check(tree, source, config)
    }

    /// Run the rule with access to the project-wide symbol index.
    /// Default delegates to `check_with_symbols()`.
    /// Override this in rules that need cross-file resolution (Layer 3).
    fn check_with_project(
        &self,
        tree: &Tree,
        source: &str,
        config: &LintConfig,
        symbols: &SymbolTable,
        _project: &ProjectIndex,
    ) -> Vec<LintDiagnostic> {
        self.check_with_symbols(tree, source, config, symbols)
    }
}

/// Return all active rules based on config.
///
/// Resolution order (highest wins):
/// 1. `disabled_rules` list → always disables (backward compat)
/// 2. Per-rule `severity = "off"` → always disables
/// 3. Per-rule config (severity != "off") → enables with that severity
/// 4. Category setting → enables/disables + sets severity for all rules in category
/// 5. Rule's built-in default (`default_enabled` + default severity)
#[allow(clippy::too_many_lines)]
pub fn all_rules(
    disabled: &[String],
    rules_config: &HashMap<String, RuleConfig>,
    lint_config: &LintConfig,
) -> Vec<Box<dyn LintRule>> {
    let all: Vec<Box<dyn LintRule>> = vec![
        Box::new(naming_convention::NamingConvention),
        Box::new(unused_variable::UnusedVariable),
        Box::new(missing_type_hint::MissingTypeHint),
        Box::new(empty_function::EmptyFunction),
        Box::new(long_function::LongFunction),
        Box::new(duplicate_signal::DuplicateSignal),
        Box::new(self_assignment::SelfAssignment),
        Box::new(unreachable_code::UnreachableCode),
        Box::new(shadowed_variable::ShadowedVariable),
        Box::new(comparison_with_boolean::ComparisonWithBoolean),
        Box::new(unnecessary_pass::UnnecessaryPass),
        Box::new(preload_type_hint::PreloadTypeHint),
        Box::new(integer_division::IntegerDivision),
        Box::new(signal_name_convention::SignalNameConvention),
        Box::new(magic_number::MagicNumber),
        Box::new(float_comparison::FloatComparison),
        Box::new(return_type_mismatch::ReturnTypeMismatch),
        Box::new(private_method_access::PrivateMethodAccess),
        Box::new(untyped_array::UntypedArray),
        Box::new(duplicate_function::DuplicateFunction),
        Box::new(unused_signal::UnusedSignal),
        Box::new(duplicate_key::DuplicateKey),
        Box::new(await_in_ready::AwaitInReady),
        Box::new(missing_return::MissingReturn),
        Box::new(unused_preload::UnusedPreload),
        Box::new(static_type_inference::StaticTypeInference),
        Box::new(node_ready_order::NodeReadyOrder),
        Box::new(enum_naming::EnumNaming),
        Box::new(parameter_naming::ParameterNaming),
        Box::new(too_many_parameters::TooManyParameters),
        Box::new(cyclomatic_complexity::CyclomaticComplexity),
        Box::new(deeply_nested_code::DeeplyNestedCode),
        Box::new(get_node_in_process::GetNodeInProcess),
        Box::new(physics_in_process::PhysicsInProcess),
        Box::new(redundant_else::RedundantElse),
        Box::new(unused_parameter::UnusedParameter),
        Box::new(duplicated_load::DuplicatedLoad),
        Box::new(standalone_expression::StandaloneExpression),
        Box::new(comparison_with_itself::ComparisonWithItself),
        Box::new(class_definitions_order::ClassDefinitionsOrder),
        Box::new(loop_variable_name::LoopVariableName),
        Box::new(max_file_lines::MaxFileLines),
        Box::new(max_line_length::MaxLineLength),
        Box::new(max_public_methods::MaxPublicMethods),
        Box::new(print_statement::PrintStatement),
        Box::new(todo_comment::TodoComment),
        Box::new(parameter_shadows_field::ParameterShadowsField),
        Box::new(god_object::GodObject),
        Box::new(duplicate_delegate::DuplicateDelegate),
        Box::new(signal_not_connected::SignalNotConnected),
        Box::new(callable_null_check::CallableNullCheck),
        Box::new(breakpoint_statement::BreakpointStatement),
        Box::new(variant_inference::VariantInference),
        Box::new(untyped_array_literal::UntypedArrayLiteral),
        Box::new(null_after_await::NullAfterAwait),
        Box::new(look_at_before_tree::LookAtBeforeTree),
        Box::new(monitoring_in_signal::MonitoringInSignal),
        Box::new(use_before_assign::UseBeforeAssign),
        Box::new(enum_without_class_name::EnumWithoutClassName),
        Box::new(onready_with_export::OnreadyWithExport),
        Box::new(enum_variable_without_default::EnumVariableWithoutDefault),
        Box::new(redundant_static_unload::RedundantStaticUnload),
        Box::new(get_node_default_without_onready::GetNodeDefaultWithoutOnready),
        Box::new(unused_private_class_variable::UnusedPrivateClassVariable),
        Box::new(native_method_override::NativeMethodOverride),
        Box::new(narrowing_conversion::NarrowingConversion),
        Box::new(unsafe_void_return::UnsafeVoidReturn),
        Box::new(return_value_discarded::ReturnValueDiscarded),
        Box::new(incompatible_ternary::IncompatibleTernary),
        Box::new(standalone_ternary::StandaloneTernary),
        Box::new(assert_always_true::AssertAlwaysTrue),
        Box::new(assert_always_false::AssertAlwaysFalse),
        Box::new(shadowed_variable_base_class::ShadowedVariableBaseClass),
        Box::new(static_called_on_instance::StaticCalledOnInstance),
        Box::new(missing_tool::MissingTool),
    ];
    all.into_iter()
        .filter(|r| {
            let name = r.name();

            // 1. disabled_rules always wins
            if disabled.iter().any(|d| d == name) {
                return false;
            }

            // 2. Per-rule severity = "off" always disables
            if rules_config
                .get(name)
                .is_some_and(|c| c.severity.as_deref() == Some("off"))
            {
                return false;
            }

            // 3. Per-rule config with severity != "off" → enables
            if rules_config.get(name).is_some_and(|c| c.severity.is_some()) {
                return true;
            }

            // 4. Category setting
            if let Some(cat_level) = lint_config.category_level(r.category()) {
                return cat_level != "off";
            }

            // 5. Rule's built-in default
            if r.default_enabled() {
                true
            } else {
                // Opt-in: only include if explicitly configured (no severity set but has config entry)
                rules_config.get(name).is_some()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_rules_config() -> HashMap<String, RuleConfig> {
        HashMap::new()
    }

    fn default_lint_config() -> LintConfig {
        LintConfig::default()
    }

    #[test]
    fn all_rules_returns_all_default_enabled_with_no_config() {
        let rules = all_rules(&[], &empty_rules_config(), &default_lint_config());
        // Default-enabled rules should be present, opt-in ones should not
        assert!(rules.iter().any(|r| r.name() == "duplicate-signal"));
        assert!(!rules.iter().any(|r| r.name() == "variant-inference"));
    }

    #[test]
    fn category_enables_opt_in_rules() {
        // type_safety = "warning" should enable opt-in rules like variant-inference
        let mut config = default_lint_config();
        config.type_safety = Some("warning".to_string());
        let rules = all_rules(&[], &empty_rules_config(), &config);
        assert!(rules.iter().any(|r| r.name() == "variant-inference"));
        assert!(rules.iter().any(|r| r.name() == "static-type-inference"));
    }

    #[test]
    fn category_disables_default_enabled_rules() {
        // correctness = "off" should disable rules like duplicate-signal
        let mut config = default_lint_config();
        config.correctness = Some("off".to_string());
        let rules = all_rules(&[], &empty_rules_config(), &config);
        assert!(!rules.iter().any(|r| r.name() == "duplicate-signal"));
        assert!(!rules.iter().any(|r| r.name() == "duplicate-function"));
    }

    #[test]
    fn per_rule_overrides_category() {
        // style = "off" disables all style rules, but per-rule severity re-enables one
        let mut config = default_lint_config();
        config.style = Some("off".to_string());
        let mut rules_config = HashMap::new();
        rules_config.insert(
            "naming-convention".to_string(),
            RuleConfig {
                severity: Some("error".to_string()),
                ..RuleConfig::default()
            },
        );
        let rules = all_rules(&[], &rules_config, &config);
        assert!(rules.iter().any(|r| r.name() == "naming-convention"));
        assert!(!rules.iter().any(|r| r.name() == "shadowed-variable"));
    }

    #[test]
    fn disabled_rules_overrides_category() {
        // type_safety = "warning" enables variant-inference, but disabled_rules overrides
        let mut config = default_lint_config();
        config.type_safety = Some("warning".to_string());
        let disabled = vec!["variant-inference".to_string()];
        let rules = all_rules(&disabled, &empty_rules_config(), &config);
        assert!(!rules.iter().any(|r| r.name() == "variant-inference"));
        // Other type_safety rules still enabled
        assert!(rules.iter().any(|r| r.name() == "missing-type-hint"));
    }

    #[test]
    fn every_rule_has_a_category() {
        let rules = all_rules(&[], &empty_rules_config(), &default_lint_config());
        for rule in &rules {
            // Just verify it doesn't panic
            let _ = rule.category();
        }
    }

    #[test]
    fn category_display_roundtrip() {
        for cat in [
            LintCategory::Correctness,
            LintCategory::Suspicious,
            LintCategory::Style,
            LintCategory::Complexity,
            LintCategory::Performance,
            LintCategory::Godot,
            LintCategory::TypeSafety,
            LintCategory::Maintenance,
        ] {
            let s = cat.to_string();
            let parsed: LintCategory = s.parse().unwrap();
            assert_eq!(cat, parsed);
        }
    }
}
