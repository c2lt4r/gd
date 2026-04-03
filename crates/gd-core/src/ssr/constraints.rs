//! Type-aware constraint checking for SSR placeholders.
//!
//! After the structural matcher (Phase 2) produces candidates, this
//! module filters them by checking type constraints (`$x:Node`,
//! `$x:{has_method("foo")}`, `$x:Variant`) against the inferred types
//! of captured expressions.

use std::collections::HashMap;

use crate::gd_ast::GdFile;
use crate::type_inference::{self, InferredType};
use crate::workspace_index::ProjectIndex;

use super::captures::Capture;
use super::pattern::{PlaceholderInfo, StructuralPredicate, TypeConstraint};

// ═══════════════════════════════════════════════════════════════════════
//  Public API
// ═══════════════════════════════════════════════════════════════════════

/// Check whether all type-constrained placeholders in `captures`
/// satisfy their constraints.
///
/// Returns `true` if every constrained placeholder's captured
/// expression has a type that matches its constraint.  Unconstrained
/// placeholders are always satisfied.
///
/// **Conservative**: if a captured expression's type cannot be inferred
/// (returns `None` or `Variant` for a non-Variant constraint), the
/// constraint is NOT satisfied.
#[allow(clippy::implicit_hasher)]
pub fn satisfies_constraints(
    captures: &HashMap<String, Capture>,
    placeholders: &HashMap<String, PlaceholderInfo>,
    file: &GdFile<'_>,
    source: &str,
    project: Option<&ProjectIndex>,
) -> bool {
    for (name, capture) in captures {
        let Some(ph) = placeholders.get(name) else {
            continue;
        };
        let Some(constraint) = &ph.constraint else {
            continue;
        };

        let Capture::Expr(captured) = capture else {
            // Variadic captures (ArgList) don't support type constraints.
            continue;
        };

        // Find the tree-sitter node for this capture by byte range.
        let Some(ts_node) = file
            .node
            .descendant_for_byte_range(captured.byte_range.start, captured.byte_range.end)
        else {
            return false;
        };

        // Infer the type of the captured expression.
        let inferred = if let Some(proj) = project {
            type_inference::infer_expression_type_with_project(&ts_node, source, file, proj)
        } else {
            type_inference::infer_expression_type(&ts_node, source, file)
        };

        let Some(inferred) = inferred else {
            return false; // unknown type → constraint not satisfied
        };

        if !check_constraint(&inferred, constraint, project) {
            return false;
        }
    }
    true
}

// ═══════════════════════════════════════════════════════════════════════
//  Constraint checking
// ═══════════════════════════════════════════════════════════════════════

/// Check a single constraint against an inferred type.
fn check_constraint(
    inferred: &InferredType,
    constraint: &TypeConstraint,
    project: Option<&ProjectIndex>,
) -> bool {
    match constraint {
        TypeConstraint::Nominal(expected) => check_nominal(inferred, expected),
        TypeConstraint::Structural(predicate) => check_structural(inferred, predicate, project),
        TypeConstraint::VariantOnly => matches!(inferred, InferredType::Variant),
    }
}

/// Nominal check: inferred type is `expected` or a subclass of it.
fn check_nominal(inferred: &InferredType, expected: &str) -> bool {
    match inferred {
        InferredType::Builtin(name) => {
            // Built-in types: exact match (int, float, String, etc.)
            // or ClassDB inheritance for engine types (Vector2, Node, etc.)
            *name == expected || gd_class_db::inherits(name, expected)
        }
        InferredType::Class(name) => name == expected || gd_class_db::inherits(name, expected),
        InferredType::Enum(name) => name == expected,
        InferredType::TypedArray(_) => expected == "Array",
        InferredType::Void | InferredType::Variant => false,
    }
}

/// Structural/duck-typing check: inferred type satisfies a predicate.
fn check_structural(
    inferred: &InferredType,
    predicate: &StructuralPredicate,
    project: Option<&ProjectIndex>,
) -> bool {
    let class_name = match inferred {
        InferredType::Builtin(name) => *name,
        InferredType::Class(name) => name.as_str(),
        _ => return false,
    };

    match predicate {
        StructuralPredicate::HasMethod(method) => {
            // Check ClassDB first, then user project.
            gd_class_db::method_exists(class_name, method)
                || project.is_some_and(|p| p.method_exists(class_name, method))
        }
        StructuralPredicate::HasProperty(prop) => {
            gd_class_db::property_exists(class_name, prop)
                || project.is_some_and(|p| p.variable_type(class_name, prop).is_some())
        }
        StructuralPredicate::HasSignal(signal) => {
            gd_class_db::signal_exists(class_name, signal)
                || project.is_some_and(|p| p.signal_exists(class_name, signal))
        }
        StructuralPredicate::Extends(ancestor) => {
            class_name == ancestor || gd_class_db::inherits(class_name, ancestor)
        }
    }
}
