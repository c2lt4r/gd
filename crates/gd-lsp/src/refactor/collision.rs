use std::collections::HashSet;
use std::sync::OnceLock;

use tree_sitter::{Node, Point};

use gd_core::gd_ast::{GdDecl, GdFile};

// ── Collision detection ─────────────────────────────────────────────────────

/// Names visible at a given AST position, grouped by scope.
pub struct ScopeNames {
    pub locals: HashSet<String>,
    pub file_level: HashSet<String>,
    pub builtins: HashSet<String>,
}

/// Which scope the collision was found in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionKind {
    Local,
    FileLevel,
    Builtin,
}

impl std::fmt::Display for CollisionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local => write!(f, "local variable"),
            Self::FileLevel => write!(f, "file-level declaration"),
            Self::Builtin => write!(f, "GDScript builtin"),
        }
    }
}

/// Collect all names visible at `position` in the file.
pub fn collect_scope_names(root: Node, source: &str, position: Point, file: &GdFile) -> ScopeNames {
    let locals = collect_locals(root, source, position);
    let file_level = collect_file_level(file);
    let builtins = gdscript_builtins().clone();
    ScopeNames {
        locals,
        file_level,
        builtins,
    }
}

/// Check if `name` collides with any visible name. Returns the collision kind.
pub fn check_collision(name: &str, scope: &ScopeNames) -> Option<CollisionKind> {
    if scope.locals.contains(name) {
        Some(CollisionKind::Local)
    } else if scope.file_level.contains(name) {
        Some(CollisionKind::FileLevel)
    } else if scope.builtins.contains(name) {
        Some(CollisionKind::Builtin)
    } else {
        None
    }
}

// ── Local names ─────────────────────────────────────────────────────────────

/// Collect params, local vars, and for-iterators within the enclosing function,
/// up to (and including) the given position row.
fn collect_locals(root: Node, source: &str, position: Point) -> HashSet<String> {
    let mut names = HashSet::new();

    let Some(func) = crate::references::enclosing_function(root, position) else {
        return names;
    };

    // Collect parameter names
    if let Some(params) = func.child_by_field_name("parameters") {
        collect_param_names_into(params, source, &mut names);
    }

    // Collect local var/for declarations in the body up to the position
    if let Some(body) = func.child_by_field_name("body") {
        collect_body_locals(body, source, position.row, &mut names);
    }

    names
}

fn collect_param_names_into(params: Node, source: &str, names: &mut HashSet<String>) {
    let mut cursor = params.walk();
    if !cursor.goto_first_child() {
        return;
    }
    loop {
        let child = cursor.node();
        match child.kind() {
            "identifier" => {
                names.insert(source[child.byte_range()].to_string());
            }
            "typed_parameter" | "default_parameter" | "typed_default_parameter" => {
                if let Some(name_node) = child.child(0)
                    && name_node.kind() == "identifier"
                {
                    names.insert(source[name_node.byte_range()].to_string());
                }
            }
            _ => {}
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn collect_body_locals(body: Node, source: &str, max_row: usize, names: &mut HashSet<String>) {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.start_position().row > max_row {
            break;
        }
        if child.kind() == "variable_statement"
            && let Some(name_node) = child.child_by_field_name("name")
            && let Ok(text) = name_node.utf8_text(source.as_bytes())
        {
            names.insert(text.to_string());
        } else if child.kind() == "for_statement"
            && let Some(left) = child.child_by_field_name("left")
            && let Ok(text) = left.utf8_text(source.as_bytes())
        {
            names.insert(text.to_string());
        }
    }
}

// ── File-level names ────────────────────────────────────────────────────────

fn collect_file_level(file: &GdFile) -> HashSet<String> {
    let mut names = HashSet::new();
    for decl in &file.declarations {
        if !decl.is_declaration() {
            continue;
        }
        let name = decl.name();
        if !name.is_empty() {
            names.insert(name.to_string());
        }
        // For enums, also collect member names
        if let GdDecl::Enum(e) = decl {
            for member in &e.members {
                if !member.name.is_empty() {
                    names.insert(member.name.to_string());
                }
            }
        }
    }
    names
}

// ── GDScript builtins ───────────────────────────────────────────────────────

fn gdscript_builtins() -> &'static HashSet<String> {
    static BUILTINS: OnceLock<HashSet<String>> = OnceLock::new();
    BUILTINS.get_or_init(|| {
        [
            // Keywords
            "if",
            "elif",
            "else",
            "for",
            "while",
            "match",
            "break",
            "continue",
            "pass",
            "return",
            "class",
            "class_name",
            "extends",
            "is",
            "in",
            "as",
            "self",
            "signal",
            "func",
            "static",
            "const",
            "enum",
            "var",
            "breakpoint",
            "preload",
            "await",
            "yield",
            "assert",
            "void",
            "PI",
            "TAU",
            "INF",
            "NAN",
            "true",
            "false",
            "null",
            "super",
            "not",
            "and",
            "or",
            // Core Godot types
            "Vector2",
            "Vector2i",
            "Vector3",
            "Vector3i",
            "Vector4",
            "Vector4i",
            "Color",
            "Rect2",
            "Rect2i",
            "Transform2D",
            "Transform3D",
            "Basis",
            "Quaternion",
            "AABB",
            "Plane",
            "Projection",
            "RID",
            "Callable",
            "Signal",
            "NodePath",
            "StringName",
            "Array",
            "Dictionary",
            "PackedByteArray",
            "PackedInt32Array",
            "PackedInt64Array",
            "PackedFloat32Array",
            "PackedFloat64Array",
            "PackedStringArray",
            "PackedVector2Array",
            "PackedVector3Array",
            "PackedColorArray",
            "PackedVector4Array",
            // Core classes
            "Node",
            "Node2D",
            "Node3D",
            "Control",
            "Resource",
            "Object",
            "RefCounted",
            "Tween",
            "Timer",
            "SceneTree",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect()
    })
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use gd_core::gd_ast;

    fn parse(source: &str) -> tree_sitter::Tree {
        gd_core::parser::parse(source).unwrap()
    }

    #[test]
    fn detects_local_collision() {
        let src = "func foo():\n\tvar speed = 10\n\tprint(speed)\n";
        let tree = parse(src);
        let file = gd_ast::convert(&tree, src);
        let scope = collect_scope_names(tree.root_node(), src, Point::new(2, 1), &file);
        assert_eq!(check_collision("speed", &scope), Some(CollisionKind::Local));
        assert_eq!(check_collision("unused", &scope), None);
    }

    #[test]
    fn detects_param_collision() {
        let src = "func foo(delta):\n\tpass\n";
        let tree = parse(src);
        let file = gd_ast::convert(&tree, src);
        let scope = collect_scope_names(tree.root_node(), src, Point::new(1, 1), &file);
        assert_eq!(check_collision("delta", &scope), Some(CollisionKind::Local));
    }

    #[test]
    fn detects_file_level_collision() {
        let src = "var health = 100\nfunc foo():\n\tpass\n";
        let tree = parse(src);
        let file = gd_ast::convert(&tree, src);
        let scope = collect_scope_names(tree.root_node(), src, Point::new(2, 1), &file);
        assert_eq!(
            check_collision("health", &scope),
            Some(CollisionKind::FileLevel)
        );
        assert_eq!(
            check_collision("foo", &scope),
            Some(CollisionKind::FileLevel)
        );
    }

    #[test]
    fn detects_builtin_collision() {
        let src = "func foo():\n\tpass\n";
        let tree = parse(src);
        let file = gd_ast::convert(&tree, src);
        let scope = collect_scope_names(tree.root_node(), src, Point::new(1, 1), &file);
        assert_eq!(
            check_collision("Vector2", &scope),
            Some(CollisionKind::Builtin)
        );
        assert_eq!(
            check_collision("Node", &scope),
            Some(CollisionKind::Builtin)
        );
        assert_eq!(
            check_collision("self", &scope),
            Some(CollisionKind::Builtin)
        );
    }

    #[test]
    fn locals_respect_position() {
        let src = "func foo():\n\tvar a = 1\n\tvar b = 2\n\tvar c = 3\n";
        let tree = parse(src);
        let file = gd_ast::convert(&tree, src);
        // At line 2 (0-based), only a and b should be visible (b declared on line 2)
        let scope = collect_scope_names(tree.root_node(), src, Point::new(2, 1), &file);
        assert!(scope.locals.contains("a"));
        assert!(scope.locals.contains("b"));
        // c is declared on line 3 — should not be visible at line 2
        assert!(!scope.locals.contains("c"));
    }

    #[test]
    fn for_iterator_is_local() {
        let src = "func foo():\n\tfor item in items:\n\t\tpass\n";
        let tree = parse(src);
        let file = gd_ast::convert(&tree, src);
        let scope = collect_scope_names(tree.root_node(), src, Point::new(2, 2), &file);
        assert!(scope.locals.contains("item"));
    }

    #[test]
    fn no_collision_returns_none() {
        let src = "func foo():\n\tvar x = 1\n";
        let tree = parse(src);
        let file = gd_ast::convert(&tree, src);
        let scope = collect_scope_names(tree.root_node(), src, Point::new(1, 1), &file);
        assert_eq!(check_collision("totally_unique_name", &scope), None);
    }

    #[test]
    fn enum_members_collected() {
        let src = "enum State { IDLE, RUN }\nfunc foo():\n\tpass\n";
        let tree = parse(src);
        let file = gd_ast::convert(&tree, src);
        let scope = collect_scope_names(tree.root_node(), src, Point::new(2, 1), &file);
        assert!(scope.file_level.contains("State"));
    }

    #[test]
    fn outside_function_no_locals() {
        let src = "var top = 1\n";
        let tree = parse(src);
        let file = gd_ast::convert(&tree, src);
        let scope = collect_scope_names(tree.root_node(), src, Point::new(0, 0), &file);
        assert!(scope.locals.is_empty());
        assert!(scope.file_level.contains("top"));
    }
}
