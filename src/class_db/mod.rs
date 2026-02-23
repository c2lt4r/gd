//! Static Godot 4.x class database for validation and suggestions.

#[allow(dead_code)]
pub(crate) mod generated;

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

/// Check if a signal exists on a class (including inherited signals).
pub fn signal_exists(class: &str, signal: &str) -> bool {
    let mut current = class;
    loop {
        let key = format!("{current}.{signal}");
        if generated::SIGNALS.binary_search(&key.as_str()).is_ok() {
            return true;
        }
        match parent_class(current) {
            Some(parent) => current = parent,
            None => return false,
        }
    }
}

/// Check if a property exists on a class (including inherited properties).
pub fn property_exists(class: &str, property: &str) -> bool {
    let mut current = class;
    loop {
        let key = format!("{current}.{property}");
        if generated::PROPERTIES
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

/// Look up the type of a property on a class, walking the inheritance chain.
/// Returns the raw type string (e.g. `"Vector2"`, `"int"`, `"Color"`).
pub fn property_type(class: &str, property: &str) -> Option<&'static str> {
    let mut current = class;
    loop {
        let key = format!("{current}.{property}");
        if let Ok(i) = generated::PROPERTIES.binary_search_by_key(&key.as_str(), |&(k, _)| k) {
            return Some(generated::PROPERTIES[i].1);
        }
        match parent_class(current) {
            Some(parent) => current = parent,
            None => return None,
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

/// Return all methods for a class, walking the inheritance chain.
/// Each entry is `(method_name, return_type, defining_class)`.
pub fn class_methods(class: &str) -> Vec<(&'static str, &'static str, &'static str)> {
    // Resolve to a &'static str from CLASSES so all borrows are 'static
    let Some(start) = generated::CLASSES
        .binary_search_by_key(&class, |c| c.name)
        .ok()
        .map(|i| generated::CLASSES[i].name)
    else {
        return Vec::new();
    };

    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut current: &'static str = start;
    loop {
        let prefix = format!("{current}.");
        for &(key, ret_type) in generated::METHODS {
            if let Some(method_name) = key.strip_prefix(&prefix)
                && seen.insert(method_name)
            {
                result.push((method_name, ret_type, current));
            }
        }
        match parent_class(current) {
            Some(parent) => current = parent,
            None => break,
        }
    }
    result
}

/// Return all properties for a class, walking the inheritance chain.
/// Each entry is `(property_name, type, defining_class)`.
pub fn class_properties(class: &str) -> Vec<(&'static str, &'static str, &'static str)> {
    let Some(start) = generated::CLASSES
        .binary_search_by_key(&class, |c| c.name)
        .ok()
        .map(|i| generated::CLASSES[i].name)
    else {
        return Vec::new();
    };

    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut current: &'static str = start;
    loop {
        let prefix = format!("{current}.");
        for &(key, prop_type) in generated::PROPERTIES {
            if let Some(prop_name) = key.strip_prefix(&prefix)
                && seen.insert(prop_name)
            {
                result.push((prop_name, prop_type, current));
            }
        }
        match parent_class(current) {
            Some(parent) => current = parent,
            None => break,
        }
    }
    result
}

/// Look up the return type of a method on a class, walking the inheritance chain.
/// Returns the raw return type string from the class database (e.g. `"void"`, `"int"`,
/// `"Node"`, `"typedarray::Node"`, `"enum::Error"`).
pub fn method_return_type(class: &str, method: &str) -> Option<&'static str> {
    let mut current = class;
    loop {
        let key = format!("{current}.{method}");
        if let Ok(i) = generated::METHODS.binary_search_by_key(&key.as_str(), |&(k, _)| k) {
            return Some(generated::METHODS[i].1);
        }
        match parent_class(current) {
            Some(parent) => current = parent,
            None => return None,
        }
    }
}

/// Method signature info for override checking.
pub struct MethodSignature {
    pub return_type: &'static str,
    pub required_params: u8,
    pub total_params: u8,
    /// Comma-separated list of parameter types.
    pub param_types: &'static str,
}

/// Look up the full signature of a method on a class, walking the inheritance chain.
pub fn method_signature(class: &str, method: &str) -> Option<MethodSignature> {
    let mut current = class;
    loop {
        let key = format!("{current}.{method}");
        if let Ok(i) = generated::METHOD_SIGNATURES.binary_search_by_key(&key.as_str(), |s| s.key) {
            let sig = &generated::METHOD_SIGNATURES[i];
            return Some(MethodSignature {
                return_type: sig.return_type,
                required_params: sig.required_params,
                total_params: sig.total_params,
                param_types: sig.param_types,
            });
        }
        match parent_class(current) {
            Some(parent) => current = parent,
            None => return None,
        }
    }
}

/// Look up the documentation string for a class.
pub fn class_doc(name: &str) -> Option<&'static str> {
    generated::CLASSES
        .binary_search_by_key(&name, |c| c.name)
        .ok()
        .map(|i| generated::CLASSES[i].doc)
        .filter(|d| !d.is_empty())
}

/// Look up the documentation string for a method, walking the inheritance chain.
pub fn method_doc(class: &str, method: &str) -> Option<&'static str> {
    let mut current = class;
    loop {
        let key = format!("{current}.{method}");
        if let Ok(i) =
            generated::METHOD_DOCS.binary_search_by_key(&key.as_str(), |&(k, _)| k)
        {
            let doc = generated::METHOD_DOCS[i].1;
            if !doc.is_empty() {
                return Some(doc);
            }
        }
        match parent_class(current) {
            Some(parent) => current = parent,
            None => return None,
        }
    }
}

/// Look up the documentation string for a property, walking the inheritance chain.
pub fn property_doc(class: &str, property: &str) -> Option<&'static str> {
    let mut current = class;
    loop {
        let key = format!("{current}.{property}");
        if let Ok(i) =
            generated::PROPERTY_DOCS.binary_search_by_key(&key.as_str(), |&(k, _)| k)
        {
            let doc = generated::PROPERTY_DOCS[i].1;
            if !doc.is_empty() {
                return Some(doc);
            }
        }
        match parent_class(current) {
            Some(parent) => current = parent,
            None => return None,
        }
    }
}

/// Look up all values of an enum on a class (walks inheritance).
/// Returns values from the first class in the hierarchy that defines the enum.
pub fn enum_values(class: &str, enum_name: &str) -> &'static [generated::EnumValue] {
    let mut current = class;
    loop {
        // Find the range of values for this class+enum in the sorted table
        let start = generated::ENUM_VALUES
            .partition_point(|v| (v.class, v.enum_name) < (current, enum_name));
        let end = generated::ENUM_VALUES[start..]
            .partition_point(|v| (v.class, v.enum_name) == (current, enum_name));
        if end > 0 {
            return &generated::ENUM_VALUES[start..start + end];
        }
        match parent_class(current) {
            Some(parent) => current = parent,
            None => return &[],
        }
    }
}

/// Look up the doc for a specific enum value (e.g. class="Object", value="CONNECT_DEFERRED").
/// Walks the inheritance chain.
pub fn enum_value_doc(class: &str, value_name: &str) -> Option<&'static generated::EnumValue> {
    let mut current = class;
    loop {
        for v in generated::ENUM_VALUES {
            if v.class == current && v.name == value_name {
                return Some(v);
            }
        }
        match parent_class(current) {
            Some(parent) => current = parent,
            None => return None,
        }
    }
}

/// Look up a utility function by name (print, lerp, sin, etc.).
pub fn utility_function(name: &str) -> Option<&'static generated::UtilityFunction> {
    generated::UTILITY_FUNCTIONS
        .binary_search_by_key(&name, |f| f.name)
        .ok()
        .map(|i| &generated::UTILITY_FUNCTIONS[i])
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

/// Check if a method name returns void on any class in the database.
/// Used to detect void-returning calls in eval expressions where we
/// don't have full type inference on the receiver.
pub fn is_method_void_anywhere(method: &str) -> bool {
    let suffix = format!(".{method}");
    generated::METHODS
        .iter()
        .any(|(key, ret)| key.ends_with(&suffix) && *ret == "void")
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

    #[test]
    fn test_class_methods() {
        let methods = class_methods("Node2D");
        let names: Vec<&str> = methods.iter().map(|(name, _, _)| *name).collect();
        // Node2D own methods
        assert!(names.contains(&"apply_scale"));
        // Inherited from Node
        assert!(names.contains(&"add_child"));
        // Each entry should have a return type and defining class
        let apply_scale = methods
            .iter()
            .find(|(n, _, _)| *n == "apply_scale")
            .unwrap();
        assert_eq!(apply_scale.2, "Node2D");
        let add_child = methods.iter().find(|(n, _, _)| *n == "add_child").unwrap();
        assert_eq!(add_child.2, "Node");
    }

    #[test]
    fn test_class_methods_unknown_class() {
        let methods = class_methods("NonExistentClass");
        assert!(methods.is_empty());
    }

    #[test]
    fn test_method_return_type_direct() {
        assert_eq!(method_return_type("Node", "add_child"), Some("void"));
        assert_eq!(method_return_type("Node", "get_child"), Some("Node"));
        assert_eq!(method_return_type("Node", "get_child_count"), Some("int"));
    }

    #[test]
    fn test_method_return_type_inherited() {
        // Node2D inherits add_child from Node
        assert_eq!(method_return_type("Node2D", "add_child"), Some("void"));
        assert_eq!(
            method_return_type("CharacterBody2D", "get_child"),
            Some("Node")
        );
    }

    #[test]
    fn test_method_return_type_unknown() {
        assert_eq!(method_return_type("Node", "nonexistent_method"), None);
        assert_eq!(method_return_type("FakeClass", "method"), None);
    }

    #[test]
    fn test_is_method_void_anywhere() {
        assert!(is_method_void_anywhere("add_child"));
        assert!(is_method_void_anywhere("set_pause"));
        assert!(is_method_void_anywhere("queue_free"));
        assert!(!is_method_void_anywhere("get_child_count"));
        assert!(!is_method_void_anywhere("get_child"));
        assert!(!is_method_void_anywhere("nonexistent_xyz_method"));
    }

    #[test]
    fn test_method_return_type_special_types() {
        // Verify enum and typedarray return types are returned as-is
        let ret = method_return_type("AESContext", "start");
        assert_eq!(ret, Some("enum::Error"));
    }

    #[test]
    fn test_class_properties_node2d() {
        let props = class_properties("Node2D");
        let names: Vec<&str> = props.iter().map(|(name, _, _)| *name).collect();
        assert!(names.contains(&"position"));
        assert!(names.contains(&"rotation"));
        assert!(names.contains(&"global_position"));
        // Check type
        let pos = props.iter().find(|(n, _, _)| *n == "position").unwrap();
        assert_eq!(pos.1, "Vector2");
        assert_eq!(pos.2, "Node2D");
    }

    #[test]
    fn test_class_properties_inherited() {
        let props = class_properties("CharacterBody2D");
        let names: Vec<&str> = props.iter().map(|(name, _, _)| *name).collect();
        // Own property
        assert!(names.contains(&"velocity"));
        // Inherited from Node2D
        assert!(names.contains(&"position"));
    }

    #[test]
    fn test_class_properties_unknown_class() {
        let props = class_properties("NonExistentClass");
        assert!(props.is_empty());
    }

    #[test]
    fn test_property_type_direct() {
        assert_eq!(property_type("Node2D", "position"), Some("Vector2"));
        assert_eq!(property_type("Node2D", "rotation"), Some("float"));
    }

    #[test]
    fn test_property_type_inherited() {
        // CharacterBody2D inherits position from Node2D
        assert_eq!(
            property_type("CharacterBody2D", "position"),
            Some("Vector2")
        );
    }

    #[test]
    fn test_property_type_unknown() {
        assert_eq!(property_type("Node", "nonexistent_prop"), None);
        assert_eq!(property_type("FakeClass", "prop"), None);
    }
}
