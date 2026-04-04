//! Built-in Godot type and function documentation for LSP hover/completion.
//!
//! Engine class members (Object, Node, Node2D, etc.) are sourced from ClassDB
//! via `class_methods()`/`class_properties()`/`method_doc()`/`property_doc()`.
//! Builtin variant type members (Vector2, String, Color, etc.) come from the
//! generated `GENERATED_MEMBERS` static table.

pub struct BuiltinDoc<'a> {
    pub name: &'a str,
    pub brief: &'a str,
    pub description: &'a str,
}

// ── Built-in types (primitives not in the API dump) ─────────────────

const PRIMITIVE_TYPE_DOCS: &[BuiltinDoc<'static>] = &[
    BuiltinDoc {
        name: "int",
        brief: "Integer type",
        description: "64-bit signed integer.",
    },
    BuiltinDoc {
        name: "float",
        brief: "Float type",
        description: "64-bit double-precision floating-point number.",
    },
    BuiltinDoc {
        name: "bool",
        brief: "Boolean type",
        description: "Boolean value: `true` or `false`.",
    },
];

// Built-in functions are now sourced from class_db::utility_function().

/// Look up a type by name.
///
/// Checks: primitives → generated builtin type docs → ClassDB class docs.
pub fn lookup_type(name: &str) -> Option<BuiltinDoc<'_>> {
    // 1. Primitives (int, float, bool — not in API dumps)
    if let Some(doc) = PRIMITIVE_TYPE_DOCS.iter().find(|d| d.name == name) {
        return Some(BuiltinDoc {
            name: doc.name,
            brief: doc.brief,
            description: doc.description,
        });
    }

    // 2. Generated builtin type docs (Vector2, Color, String, etc.)
    for td in super::builtin_generated::BUILTIN_TYPE_DOCS {
        if td.name == name {
            let desc = if td.description.is_empty() {
                td.brief
            } else {
                td.description
            };
            return Some(BuiltinDoc {
                name: td.name,
                brief: td.brief,
                description: desc,
            });
        }
    }

    None
}

/// Look up a built-in/utility function by name.
///
/// Checks: ClassDB utility functions → lifecycle methods.
pub fn lookup_function(name: &str) -> Option<BuiltinDoc<'_>> {
    // 1. ClassDB utility functions (print, lerp, sin, etc.)
    if let Some(uf) = super::utility_function(name) {
        return Some(BuiltinDoc {
            name: uf.name,
            brief: uf.signature,
            description: if uf.doc.is_empty() {
                uf.signature
            } else {
                uf.doc
            },
        });
    }

    // 2. Virtual/lifecycle methods (_ready, _process, etc.) from ClassDB
    if super::is_godot_virtual_method(name) {
        // Find the first class that defines this virtual method
        let suffix = format!(".{name}");
        for entry in super::generated::METHODS {
            if let Some(class) = entry.key.strip_suffix(suffix.as_str()) {
                let method_name = &entry.key[class.len() + 1..]; // &'static str slice
                let doc = super::method_doc(class, method_name).unwrap_or(entry.return_type);
                return Some(BuiltinDoc {
                    name: method_name,
                    brief: entry.return_type,
                    description: doc,
                });
            }
        }
    }

    None
}

/// Generate a link to the Godot documentation for a class.
pub fn godot_docs_url(class_name: &str) -> String {
    format!(
        "https://docs.godotengine.org/en/stable/classes/class_{}.html",
        class_name.to_lowercase()
    )
}

/// Format a hover string for a built-in type.
pub fn format_type_hover(doc: &BuiltinDoc<'_>) -> String {
    use std::fmt::Write;
    let mut result = format!("```gdscript\n{}\n```\n{}", doc.brief, doc.description);
    // Add docs link for classes (types that start with uppercase, not primitives)
    let first_char = doc.name.chars().next().unwrap_or('a');
    if first_char.is_uppercase() {
        let _ = write!(result, "\n\n[Godot docs]({})", godot_docs_url(doc.name));
    }
    result
}

/// Format a hover string for a built-in function.
pub fn format_function_hover(doc: &BuiltinDoc<'_>) -> String {
    format!("```gdscript\n{}\n```\n{}", doc.brief, doc.description)
}

// ── Built-in member documentation ───────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberKind {
    Property,
    Method,
}

#[derive(Debug, Clone)]
pub struct BuiltinMember {
    pub class: &'static str,
    pub name: &'static str,
    pub brief: &'static str,
    pub description: &'static str,
    pub kind: MemberKind,
}

use MemberKind::{Method, Property};

// ── ClassDB-backed member lookup ────────────────────────────────────
//
// Engine class members are queried from ClassDB at runtime rather than
// maintained as a hand-written static array. This covers 16k+ methods and
// 5.7k+ properties across 1023 engine classes with full documentation.
//
// Builtin variant type members (Vector2.x, String.length, etc.) come from
// the generated GENERATED_MEMBERS table in builtin_generated.rs.

/// Resolve a class name to its `&'static str` equivalent from the CLASSES table.
fn static_class_name(class: &str) -> Option<&'static str> {
    super::generated::CLASSES
        .binary_search_by_key(&class, |c| c.name)
        .ok()
        .map(|i| super::generated::CLASSES[i].name)
}

/// Return all members for the given exact class name (does NOT walk inheritance).
///
/// For engine classes (Node, Object, etc.), queries ClassDB methods + properties.
/// For builtin types (Vector2, Color, etc.), returns entries from `GENERATED_MEMBERS`.
pub fn members_for_class(class: &str) -> Vec<BuiltinMember> {
    let mut result: Vec<BuiltinMember> = Vec::new();

    // 1. Generated builtin type members (Vector2.x, String.length, etc.)
    for m in super::builtin_generated::GENERATED_MEMBERS {
        if m.class == class {
            result.push(m.clone());
        }
    }

    // 2. ClassDB engine class methods + properties (exact class, no inheritance walk)
    let static_class = static_class_name(class);
    if let Some(sc) = static_class {
        let prefix = format!("{sc}.");
        for entry in super::generated::METHODS {
            if let Some(method_name) = entry.key.strip_prefix(&prefix) {
                let doc = super::method_doc(sc, method_name).unwrap_or("");
                result.push(BuiltinMember {
                    class: sc,
                    name: method_name,
                    brief: entry.return_type,
                    description: doc,
                    kind: Method,
                });
            }
        }
        for entry in super::generated::PROPERTIES {
            if let Some(prop_name) = entry.key.strip_prefix(&prefix) {
                let doc = super::property_doc(sc, prop_name).unwrap_or("");
                result.push(BuiltinMember {
                    class: sc,
                    name: prop_name,
                    brief: entry.type_name,
                    description: doc,
                    kind: Property,
                });
            }
        }
    }

    result
}

/// Look up a member by name across all engine classes and builtin types.
///
/// Checks generated builtin type members first, then ClassDB methods, then
/// ClassDB properties. Returns the first match found.
pub fn lookup_member(name: &str) -> Option<BuiltinMember> {
    // 1. Generated builtin type members
    if let Some(m) = super::builtin_generated::GENERATED_MEMBERS
        .iter()
        .find(|m| m.name == name)
    {
        return Some(m.clone());
    }

    // 2. ClassDB methods — scan for ".name" suffix
    let suffix = format!(".{name}");
    for entry in super::generated::METHODS {
        if let Some(class_str) = entry.key.strip_suffix(suffix.as_str())
            && let Some(sc) = static_class_name(class_str)
        {
            // key is "Class.method" (&'static str); split after the dot to get the method name
            let method_name = &entry.key[sc.len() + 1..];
            let doc = super::method_doc(sc, method_name).unwrap_or("");
            return Some(BuiltinMember {
                class: sc,
                name: method_name,
                brief: entry.return_type,
                description: doc,
                kind: Method,
            });
        }
    }

    // 3. ClassDB properties
    for entry in super::generated::PROPERTIES {
        if let Some(class_str) = entry.key.strip_suffix(suffix.as_str())
            && let Some(sc) = static_class_name(class_str)
        {
            let prop_name = &entry.key[sc.len() + 1..];
            let doc = super::property_doc(sc, prop_name).unwrap_or("");
            return Some(BuiltinMember {
                class: sc,
                name: prop_name,
                brief: entry.type_name,
                description: doc,
                kind: Property,
            });
        }
    }

    None
}

/// Look up a member by class and name (exact class match).
///
/// Checks generated builtin type members first, then ClassDB.
pub fn lookup_member_for(class: &str, name: &str) -> Option<BuiltinMember> {
    // 1. Generated builtin type members
    if let Some(m) = super::builtin_generated::GENERATED_MEMBERS
        .iter()
        .find(|m| m.class == class && m.name == name)
    {
        return Some(m.clone());
    }

    // 2. ClassDB method
    let key = format!("{class}.{name}");
    if let Ok(i) = super::generated::METHODS.binary_search_by_key(&key.as_str(), |m| m.key) {
        let entry = &super::generated::METHODS[i];
        let sc = static_class_name(class)?;
        let method_name = &entry.key[sc.len() + 1..];
        let doc = super::method_doc(sc, method_name).unwrap_or("");
        return Some(BuiltinMember {
            class: sc,
            name: method_name,
            brief: entry.return_type,
            description: doc,
            kind: Method,
        });
    }

    // 3. ClassDB property
    if let Ok(i) = super::generated::PROPERTIES.binary_search_by_key(&key.as_str(), |p| p.key) {
        let entry = &super::generated::PROPERTIES[i];
        let sc = static_class_name(class)?;
        let prop_name = &entry.key[sc.len() + 1..];
        let doc = super::property_doc(sc, prop_name).unwrap_or("");
        return Some(BuiltinMember {
            class: sc,
            name: prop_name,
            brief: entry.type_name,
            description: doc,
            kind: Property,
        });
    }

    None
}

/// Format a hover string for a built-in member.
pub fn format_member_hover(doc: &BuiltinMember) -> String {
    let kind_label = match doc.kind {
        MemberKind::Property => "property",
        MemberKind::Method => "method",
    };
    let anchor = match doc.kind {
        MemberKind::Property => format!(
            "class-{}-property-{}",
            doc.class.to_lowercase(),
            doc.name.replace('_', "-")
        ),
        MemberKind::Method => format!(
            "class-{}-method-{}",
            doc.class.to_lowercase(),
            doc.name.replace('_', "-")
        ),
    };
    let url = format!(
        "https://docs.godotengine.org/en/stable/classes/class_{}.html#{}",
        doc.class.to_lowercase(),
        anchor
    );
    let display_brief = match doc.kind {
        MemberKind::Property => format!("{}: {}", doc.name, doc.brief),
        MemberKind::Method => {
            if doc.brief.contains('(') {
                doc.brief.to_string()
            } else {
                format!("{}() -> {}", doc.name, doc.brief)
            }
        }
    };
    format!(
        "```gdscript\n{display_brief}\n```\n({} {}) {}\n\n[Godot docs]({})",
        doc.class, kind_label, doc.description, url
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_known_type() {
        let doc = lookup_type("Vector2").unwrap();
        assert_eq!(doc.name, "Vector2");
        assert!(!doc.description.is_empty());
    }

    #[test]
    fn lookup_known_class_via_class_db() {
        // Engine classes (Node3D) are not returned by lookup_type;
        // they are handled directly via class_db in hover.rs step 9.
        assert!(lookup_type("Node3D").is_none());
        // But class_doc should work
        assert!(crate::class_doc("Node3D").is_some());
    }

    #[test]
    fn lookup_unknown_type() {
        assert!(lookup_type("NonExistent").is_none());
    }

    #[test]
    fn lookup_known_function() {
        let doc = lookup_function("lerp").unwrap();
        assert_eq!(doc.name, "lerp");
    }

    #[test]
    fn lookup_lifecycle_method() {
        let doc = lookup_function("_ready").unwrap();
        assert_eq!(doc.name, "_ready");
    }

    #[test]
    fn lookup_unknown_function() {
        assert!(lookup_function("nonexistent").is_none());
    }

    #[test]
    fn docs_url_lowercase() {
        assert_eq!(
            godot_docs_url("Node3D"),
            "https://docs.godotengine.org/en/stable/classes/class_node3d.html"
        );
    }

    #[test]
    fn type_hover_includes_docs_link() {
        let doc = lookup_type("Vector2").unwrap();
        let hover = format_type_hover(&doc);
        assert!(hover.contains("Godot docs"));
        assert!(hover.contains("class_vector2.html"));
    }

    #[test]
    fn primitive_hover_no_docs_link() {
        let doc = lookup_type("int").unwrap();
        let hover = format_type_hover(&doc);
        assert!(!hover.contains("Godot docs"));
    }

    #[test]
    fn function_hover_format() {
        let doc = lookup_function("lerp").unwrap();
        let hover = format_function_hover(&doc);
        assert!(hover.contains("lerp"));
        assert!(hover.contains("interpolates"));
    }

    #[test]
    fn lookup_known_member_property() {
        let doc = lookup_member("global_position").unwrap();
        assert_eq!(doc.name, "global_position");
        assert_eq!(doc.kind, MemberKind::Property);
    }

    #[test]
    fn lookup_known_member_method() {
        let doc = lookup_member("move_and_slide").unwrap();
        assert_eq!(doc.name, "move_and_slide");
        assert_eq!(doc.kind, MemberKind::Method);
    }

    #[test]
    fn lookup_unknown_member() {
        assert!(lookup_member("nonexistent_member").is_none());
    }

    #[test]
    fn member_hover_property_format() {
        let doc = lookup_member("global_position").unwrap();
        let hover = format_member_hover(&doc);
        assert!(hover.contains("global_position"));
        assert!(hover.contains("property"));
        assert!(hover.contains("Godot docs"));
        // lookup_member returns the first class that has this property (alphabetical),
        // which may be Control rather than Node2D
        assert!(hover.contains("property-global-position"));
    }

    #[test]
    fn member_hover_method_format() {
        let doc = lookup_member("queue_free").unwrap();
        let hover = format_member_hover(&doc);
        assert!(hover.contains("queue_free"));
        assert!(hover.contains("method"));
        assert!(hover.contains("class-node-method-queue-free"));
    }

    #[test]
    fn members_for_class_node2d() {
        let members = members_for_class("Node2D");
        let names: Vec<&str> = members.iter().map(|m| m.name).collect();
        assert!(names.contains(&"position"));
        assert!(names.contains(&"global_position"));
        // Should NOT include Node members (exact class match)
        assert!(!names.contains(&"add_child"));
    }

    #[test]
    fn members_for_class_empty() {
        let members = members_for_class("NonExistentClass");
        assert!(members.is_empty());
    }

    #[test]
    fn lookup_member_for_engine_class() {
        let doc = lookup_member_for("Node", "add_child").unwrap();
        assert_eq!(doc.name, "add_child");
        assert_eq!(doc.class, "Node");
        assert_eq!(doc.kind, MemberKind::Method);
    }

    #[test]
    fn lookup_member_for_builtin_type() {
        let doc = lookup_member_for("Vector2", "x").unwrap();
        assert_eq!(doc.name, "x");
        assert_eq!(doc.class, "Vector2");
        assert_eq!(doc.kind, MemberKind::Property);
    }

    #[test]
    fn members_for_class_object_has_methods() {
        let members = members_for_class("Object");
        let names: Vec<&str> = members.iter().map(|m| m.name).collect();
        assert!(names.contains(&"connect"));
        assert!(names.contains(&"emit_signal"));
    }
}
