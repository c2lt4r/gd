//! Curated semantic constraints for lint rules.
//!
//! These encode domain knowledge about Godot APIs that ClassDB doesn't express.
//! The methods/properties exist in ClassDB, but the knowledge that they have
//! special usage requirements (need scene tree, physics-only, dangerous in
//! callbacks) is not in the class database.
//!
//! Generated data (area signals, print functions, virtual methods, keywords)
//! lives in `builtin_generated.rs` and should be accessed directly from there.

/// Methods that require the node to be inside the scene tree to work correctly.
/// Used by `look-at-before-tree` lint to detect calls in `_init`/`_enter_tree`.
pub const TREE_DEPENDENT_METHODS: &[&str] = &[
    "find_child",
    "get_children",
    "get_global_position",
    "get_global_transform",
    "get_node",
    "get_node_or_null",
    "get_parent",
    "get_tree",
    "get_viewport",
    "global_rotate",
    "global_translate",
    "look_at",
    "look_at_from_position",
    "to_global",
    "to_local",
];

/// Properties that are only valid after the node enters the scene tree.
/// Used by `look-at-before-tree` lint to detect reads in `_init`/`_enter_tree`.
pub const TREE_DEPENDENT_PROPERTIES: &[&str] = &[
    "global_basis",
    "global_position",
    "global_rotation",
    "global_rotation_degrees",
    "global_transform",
];

/// Methods that should only be called in `_physics_process`, not `_process`.
/// Used by `physics-in-process` lint.
pub const PHYSICS_METHODS: &[&str] = &[
    "apply_central_force",
    "apply_central_impulse",
    "apply_force",
    "apply_impulse",
    "apply_torque",
    "apply_torque_impulse",
    "move_and_collide",
    "move_and_slide",
    "set_velocity",
];

/// Properties that must not be directly assigned inside area monitoring callbacks.
/// Used by `monitoring-in-signal` lint.
pub const AREA_DANGEROUS_PROPERTIES: &[&str] = &["monitoring", "monitorable"];

// ── Lookup helpers ──────────────────────────────────────────────────

pub fn is_tree_dependent_method(method: &str) -> bool {
    TREE_DEPENDENT_METHODS.contains(&method)
}

pub fn is_tree_dependent_property(property: &str) -> bool {
    TREE_DEPENDENT_PROPERTIES.contains(&property)
}

pub fn is_physics_method(method: &str) -> bool {
    PHYSICS_METHODS.contains(&method)
}

pub fn is_area_dangerous_property(property: &str) -> bool {
    AREA_DANGEROUS_PROPERTIES.contains(&property)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_dependent_methods() {
        assert!(is_tree_dependent_method("look_at"));
        assert!(is_tree_dependent_method("to_global"));
        assert!(is_tree_dependent_method("get_parent"));
        assert!(!is_tree_dependent_method("add_child"));
    }

    #[test]
    fn tree_dependent_properties() {
        assert!(is_tree_dependent_property("global_position"));
        assert!(is_tree_dependent_property("global_transform"));
        assert!(!is_tree_dependent_property("position"));
    }

    #[test]
    fn physics_methods() {
        assert!(is_physics_method("move_and_slide"));
        assert!(is_physics_method("apply_force"));
        assert!(!is_physics_method("add_child"));
    }
}
