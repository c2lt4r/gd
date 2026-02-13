//! Lint rules module - each rule analyzes the tree-sitter AST.

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
pub mod static_type_inference;
pub mod unnecessary_pass;
pub mod unreachable_code;
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
pub mod get_node_in_process;
pub mod god_object;
pub mod look_at_before_tree;
pub mod loop_variable_name;
pub mod max_file_lines;
pub mod max_line_length;
pub mod max_public_methods;
pub mod parameter_naming;
pub mod parameter_shadows_field;
pub mod physics_in_process;
pub mod print_statement;
pub mod redundant_else;
pub mod signal_not_connected;
pub mod todo_comment;
pub mod too_many_parameters;
pub mod unused_parameter;

use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use tree_sitter::Tree;

use crate::core::config::{LintConfig, RuleConfig};

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

    /// Whether this rule is enabled by default. Opt-in rules return false
    /// and must be explicitly enabled via `[lint.rules.<name>]` in gd.toml.
    fn default_enabled(&self) -> bool {
        true
    }

    /// Run the rule against a parsed file and return diagnostics.
    fn check(&self, tree: &Tree, source: &str, config: &LintConfig) -> Vec<LintDiagnostic>;
}

/// Return all active rules based on config.
/// - Default-enabled rules are included unless explicitly disabled.
/// - Opt-in rules (default_enabled=false) are included only when
///   configured in `[lint.rules.<name>]` with severity != "off".
pub fn all_rules(
    disabled: &[String],
    rules_config: &HashMap<String, RuleConfig>,
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
    ];
    all.into_iter()
        .filter(|r| {
            let name = r.name();
            if disabled.iter().any(|d| d == name) {
                return false;
            }
            // severity = "off" disables any rule (default-enabled or opt-in)
            if rules_config
                .get(name)
                .is_some_and(|c| c.severity.as_deref() == Some("off"))
            {
                return false;
            }
            if r.default_enabled() {
                true
            } else {
                // Opt-in: only include if explicitly configured
                rules_config.get(name).is_some()
            }
        })
        .collect()
}
