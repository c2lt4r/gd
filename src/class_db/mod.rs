//! Static Godot 4.x class database for validation and suggestions.

#[allow(dead_code)]
mod generated;

/// Check if a class exists in the Godot class hierarchy.
pub fn class_exists(name: &str) -> bool {
    generated::CLASSES
        .binary_search_by_key(&name, |c| c.name)
        .is_ok()
}

/// Get the parent class of a given class.
#[allow(dead_code)]
pub fn parent_class(name: &str) -> Option<&'static str> {
    generated::CLASSES
        .binary_search_by_key(&name, |c| c.name)
        .ok()
        .map(|i| generated::CLASSES[i].parent)
        .filter(|p| !p.is_empty())
}

/// Check if `child` inherits from `ancestor` (direct or transitive).
#[allow(dead_code)]
pub fn inherits(child: &str, ancestor: &str) -> bool {
    let mut current = child;
    while let Some(parent) = parent_class(current) {
        if parent == ancestor {
            return true;
        }
        current = parent;
    }
    false
}

/// Check if a constant or enum member exists on a class (including inherited).
pub fn constant_exists(class: &str, name: &str) -> bool {
    let mut current = class;
    loop {
        let key = format!("{current}.{name}");
        if generated::CONSTANTS
            .binary_search_by_key(&key.as_str(), |&(k, _)| k)
            .is_ok()
            || generated::ENUM_MEMBERS
                .binary_search_by_key(&key.as_str(), |&(k, _)| k)
                .is_ok()
        {
            return true;
        }
        match parent_class(current) {
            Some(parent) => current = parent,
            None => return false,
        }
    }
}

/// Check if an enum member exists on a class (including inherited).
pub fn enum_member_exists(class: &str, name: &str) -> bool {
    let mut current = class;
    loop {
        let key = format!("{current}.{name}");
        if generated::ENUM_MEMBERS
            .binary_search_by_key(&key.as_str(), |&(k, _)| k)
            .is_ok()
        {
            return true;
        }
        match parent_class(current) {
            Some(parent) => current = parent,
            None => return false,
        }
    }
}

/// Check if a name is an enum type on a class (e.g. `Viewport.MSAA` is the enum type
/// for `MSAA_DISABLED`, `MSAA_2X`, etc.). Walks the inheritance chain.
pub fn enum_type_exists(class: &str, name: &str) -> bool {
    let mut current = class;
    loop {
        let prefix = format!("{current}.");
        for &(key, enum_type) in generated::ENUM_MEMBERS {
            if key.starts_with(&*prefix) && enum_type == name {
                return true;
            }
        }
        match parent_class(current) {
            Some(parent) => current = parent,
            None => return false,
        }
    }
}

/// Check if a method exists on a class (including inherited methods).
#[allow(dead_code)]
pub fn method_exists(class: &str, method: &str) -> bool {
    let key = format!("{class}.{method}");
    if generated::METHODS
        .binary_search_by_key(&key.as_str(), |&(k, _)| k)
        .is_ok()
    {
        return true;
    }
    // Check parent classes
    if let Some(parent) = parent_class(class) {
        return method_exists(parent, method);
    }
    false
}

/// Curated list of methods that require the node to be in the scene tree.
pub fn is_tree_dependent_method(method: &str) -> bool {
    matches!(
        method,
        "look_at"
            | "look_at_from_position"
            | "to_global"
            | "to_local"
            | "get_global_position"
            | "get_global_transform"
            | "global_translate"
            | "global_rotate"
            | "get_parent"
            | "get_tree"
            | "get_node"
            | "get_node_or_null"
            | "find_child"
            | "get_children"
            | "get_viewport"
    )
}

/// Suggest similar constants for a typo using Levenshtein distance (walks inheritance).
pub fn suggest_constant(class: &str, typo: &str, max_distance: usize) -> Vec<&'static str> {
    let mut suggestions: Vec<(&str, usize)> = Vec::new();
    let mut current = class;

    loop {
        let prefix = format!("{current}.");

        for &(key, _) in generated::CONSTANTS {
            if let Some(name) = key.strip_prefix(&prefix) {
                let dist = strsim::levenshtein(typo, name);
                if dist <= max_distance {
                    suggestions.push((name, dist));
                }
            }
        }

        for &(key, _) in generated::ENUM_MEMBERS {
            if let Some(name) = key.strip_prefix(&prefix) {
                let dist = strsim::levenshtein(typo, name);
                if dist <= max_distance {
                    suggestions.push((name, dist));
                }
            }
        }

        match parent_class(current) {
            Some(parent) => current = parent,
            None => break,
        }
    }

    suggestions.sort_by_key(|&(_, d)| d);
    suggestions.dedup_by_key(|&mut (n, _)| n);
    suggestions.into_iter().map(|(n, _)| n).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_class_exists() {
        assert!(class_exists("Node"));
        assert!(class_exists("Node2D"));
        assert!(class_exists("Node3D"));
        assert!(class_exists("Environment"));
        assert!(class_exists("CharacterBody2D"));
        assert!(!class_exists("NonExistentClass"));
    }

    #[test]
    fn test_parent_class() {
        assert_eq!(parent_class("Node2D"), Some("CanvasItem"));
        assert_eq!(parent_class("Node3D"), Some("Node"));
        assert_eq!(parent_class("Object"), None);
    }

    #[test]
    fn test_inherits() {
        assert!(inherits("Node2D", "Node"));
        assert!(inherits("Node2D", "Object"));
        assert!(inherits("CharacterBody2D", "Node"));
        assert!(!inherits("Node", "Node2D"));
    }

    #[test]
    fn test_constant_exists() {
        assert!(constant_exists("Environment", "TONE_MAPPER_LINEAR"));
        assert!(!constant_exists("Environment", "TONE_MAP_NONEXISTENT"));
    }

    #[test]
    fn test_enum_member_exists() {
        assert!(enum_member_exists("Environment", "TONE_MAPPER_LINEAR"));
    }

    #[test]
    fn test_method_exists() {
        assert!(method_exists("Node", "add_child"));
        assert!(method_exists("Node2D", "add_child")); // inherited
        assert!(!method_exists("Node", "nonexistent_method"));
    }

    #[test]
    fn test_tree_dependent_methods() {
        assert!(is_tree_dependent_method("look_at"));
        assert!(is_tree_dependent_method("to_global"));
        assert!(is_tree_dependent_method("get_parent"));
        assert!(!is_tree_dependent_method("add_child"));
    }

    #[test]
    fn test_suggest_constant() {
        let suggestions = suggest_constant("Environment", "TONE_MAPR_LINEAR", 3);
        assert!(!suggestions.is_empty());
        assert!(suggestions.contains(&"TONE_MAPPER_LINEAR"));
    }

    #[test]
    fn test_reported_missing_constants() {
        // These were previously flagged as unknown
        assert!(constant_exists("Mesh", "PRIMITIVE_TRIANGLES"));
        assert!(constant_exists("BaseMaterial3D", "SHADING_MODE_UNSHADED"));
        assert!(constant_exists("BoxContainer", "ALIGNMENT_CENTER"));
        assert!(constant_exists("SubViewport", "UPDATE_ALWAYS"));
    }

    #[test]
    fn test_enum_type_exists() {
        assert!(enum_type_exists("Viewport", "MSAA"));
        assert!(enum_type_exists("Viewport", "Scaling3DMode"));
        assert!(!enum_type_exists("Viewport", "NONEXISTENT_ENUM"));
    }

    #[test]
    fn test_inherited_constants() {
        // SubViewport inherits from Viewport — constants on parent should resolve
        assert!(class_exists("SubViewport"));
        assert!(class_exists("Viewport"));
        // Viewport constants should be accessible via SubViewport
        assert!(constant_exists("Viewport", "MSAA_4X"));
        assert!(constant_exists("SubViewport", "MSAA_4X")); // inherited
    }
}
