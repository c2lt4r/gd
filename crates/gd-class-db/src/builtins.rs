//! Built-in Godot type and function documentation for LSP hover/completion.

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

// ── Lifecycle methods ───────────────────────────────────────────────

const LIFECYCLE_METHOD_DOCS: &[BuiltinDoc<'static>] = &[
    BuiltinDoc {
        name: "_ready",
        brief: "func _ready() -> void",
        description: "Called when the node enters the scene tree for the first time.",
    },
    BuiltinDoc {
        name: "_process",
        brief: "func _process(delta: float) -> void",
        description: "Called every frame. `delta` is the elapsed time since the previous frame.",
    },
    BuiltinDoc {
        name: "_physics_process",
        brief: "func _physics_process(delta: float) -> void",
        description: "Called every physics frame. Use for physics-related code.",
    },
    BuiltinDoc {
        name: "_input",
        brief: "func _input(event: InputEvent) -> void",
        description: "Called on any input event (key, mouse, touch, etc.).",
    },
    BuiltinDoc {
        name: "_unhandled_input",
        brief: "func _unhandled_input(event: InputEvent) -> void",
        description: "Called for input events not handled by `_input` or UI nodes.",
    },
    BuiltinDoc {
        name: "_enter_tree",
        brief: "func _enter_tree() -> void",
        description: "Called when the node enters the scene tree (before `_ready`).",
    },
    BuiltinDoc {
        name: "_exit_tree",
        brief: "func _exit_tree() -> void",
        description: "Called when the node is about to leave the scene tree.",
    },
    BuiltinDoc {
        name: "_init",
        brief: "func _init() -> void",
        description: "Object constructor. Called on object creation, before `_enter_tree`.",
    },
    BuiltinDoc {
        name: "_notification",
        brief: "func _notification(what: int) -> void",
        description: "Called for engine notifications (e.g. NOTIFICATION_READY).",
    },
    BuiltinDoc {
        name: "_draw",
        brief: "func _draw() -> void",
        description: "Called when the CanvasItem needs to be redrawn. Use drawing functions here.",
    },
    BuiltinDoc {
        name: "_gui_input",
        brief: "func _gui_input(event: InputEvent) -> void",
        description: "Called for input events on a Control node.",
    },
];

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

    // 2. Lifecycle methods (_ready, _process, etc.)
    LIFECYCLE_METHOD_DOCS
        .iter()
        .find(|d| d.name == name)
        .map(|d| BuiltinDoc {
            name: d.name,
            brief: d.brief,
            description: d.description,
        })
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

pub struct BuiltinMember {
    pub class: &'static str,
    pub name: &'static str,
    pub brief: &'static str,
    pub description: &'static str,
    pub kind: MemberKind,
}

use MemberKind::{Method, Property};

const BUILTIN_MEMBER_DOCS: &[BuiltinMember] = &[
    // ── Object ──────────────────────────────────────────────────────
    BuiltinMember {
        class: "Object",
        name: "connect",
        brief: "connect(signal: StringName, callable: Callable, flags: int = 0) -> Error",
        description: "Connects a signal to a callable.",
        kind: Method,
    },
    BuiltinMember {
        class: "Object",
        name: "disconnect",
        brief: "disconnect(signal: StringName, callable: Callable) -> void",
        description: "Disconnects a signal from a callable.",
        kind: Method,
    },
    BuiltinMember {
        class: "Object",
        name: "emit_signal",
        brief: "emit_signal(signal: StringName, ...) -> Error",
        description: "Emits the given signal by name.",
        kind: Method,
    },
    BuiltinMember {
        class: "Object",
        name: "is_connected",
        brief: "is_connected(signal: StringName, callable: Callable) -> bool",
        description: "Returns true if the signal is connected to the callable.",
        kind: Method,
    },
    BuiltinMember {
        class: "Object",
        name: "set",
        brief: "set(property: StringName, value: Variant) -> void",
        description: "Sets a property by name.",
        kind: Method,
    },
    BuiltinMember {
        class: "Object",
        name: "get",
        brief: "get(property: StringName) -> Variant",
        description: "Returns a property value by name.",
        kind: Method,
    },
    BuiltinMember {
        class: "Object",
        name: "has_method",
        brief: "has_method(method: StringName) -> bool",
        description: "Returns true if the object has the given method.",
        kind: Method,
    },
    BuiltinMember {
        class: "Object",
        name: "call",
        brief: "call(method: StringName, ...) -> Variant",
        description: "Calls a method by name with arguments.",
        kind: Method,
    },
    BuiltinMember {
        class: "Object",
        name: "call_deferred",
        brief: "call_deferred(method: StringName, ...) -> void",
        description: "Calls a method at the end of the current frame.",
        kind: Method,
    },
    BuiltinMember {
        class: "Object",
        name: "set_deferred",
        brief: "set_deferred(property: StringName, value: Variant) -> void",
        description: "Sets a property at the end of the current frame.",
        kind: Method,
    },
    BuiltinMember {
        class: "Object",
        name: "has_signal",
        brief: "has_signal(signal: StringName) -> bool",
        description: "Returns true if the object has the given signal.",
        kind: Method,
    },
    BuiltinMember {
        class: "Object",
        name: "is_class",
        brief: "is_class(class: String) -> bool",
        description: "Returns true if the object is the given class or a subclass.",
        kind: Method,
    },
    BuiltinMember {
        class: "Object",
        name: "get_class",
        brief: "get_class() -> String",
        description: "Returns the class name of the object.",
        kind: Method,
    },
    BuiltinMember {
        class: "Object",
        name: "free",
        brief: "free() -> void",
        description: "Destroys the object immediately. Use queue_free() for nodes.",
        kind: Method,
    },
    // ── Node (properties) ───────────────────────────────────────────
    BuiltinMember {
        class: "Node",
        name: "name",
        brief: "StringName Node.name",
        description: "The name of the node. Unique within the parent's children.",
        kind: Property,
    },
    BuiltinMember {
        class: "Node",
        name: "owner",
        brief: "Node Node.owner",
        description: "The owner of this node (the root of the saved scene).",
        kind: Property,
    },
    BuiltinMember {
        class: "Node",
        name: "process_mode",
        brief: "ProcessMode Node.process_mode",
        description: "Controls whether processing callbacks are enabled.",
        kind: Property,
    },
    BuiltinMember {
        class: "Node",
        name: "unique_name_in_owner",
        brief: "bool Node.unique_name_in_owner",
        description: "If true, the node can be accessed with %Name syntax.",
        kind: Property,
    },
    // ── Node (methods) ──────────────────────────────────────────────
    BuiltinMember {
        class: "Node",
        name: "get_node",
        brief: "get_node(path: NodePath) -> Node",
        description: "Returns a node by its path relative to this node.",
        kind: Method,
    },
    BuiltinMember {
        class: "Node",
        name: "get_parent",
        brief: "get_parent() -> Node",
        description: "Returns the parent node, or null if this is the root.",
        kind: Method,
    },
    BuiltinMember {
        class: "Node",
        name: "get_children",
        brief: "get_children(include_internal: bool = false) -> Array[Node]",
        description: "Returns an array of this node's children.",
        kind: Method,
    },
    BuiltinMember {
        class: "Node",
        name: "add_child",
        brief: "add_child(node: Node, force_readable_name: bool = false, internal: InternalMode = 0) -> void",
        description: "Adds a child node.",
        kind: Method,
    },
    BuiltinMember {
        class: "Node",
        name: "remove_child",
        brief: "remove_child(node: Node) -> void",
        description: "Removes a child node (does not free it).",
        kind: Method,
    },
    BuiltinMember {
        class: "Node",
        name: "queue_free",
        brief: "queue_free() -> void",
        description: "Queues the node for deletion at the end of the current frame.",
        kind: Method,
    },
    BuiltinMember {
        class: "Node",
        name: "is_inside_tree",
        brief: "is_inside_tree() -> bool",
        description: "Returns true if the node is inside the scene tree.",
        kind: Method,
    },
    BuiltinMember {
        class: "Node",
        name: "get_tree",
        brief: "get_tree() -> SceneTree",
        description: "Returns the scene tree this node belongs to.",
        kind: Method,
    },
    BuiltinMember {
        class: "Node",
        name: "has_node",
        brief: "has_node(path: NodePath) -> bool",
        description: "Returns true if a node exists at the given path.",
        kind: Method,
    },
    BuiltinMember {
        class: "Node",
        name: "find_child",
        brief: "find_child(pattern: String, recursive: bool = true, owned: bool = true) -> Node",
        description: "Finds a descendant by name pattern (supports wildcards).",
        kind: Method,
    },
    BuiltinMember {
        class: "Node",
        name: "get_path",
        brief: "get_path() -> NodePath",
        description: "Returns the absolute path of this node.",
        kind: Method,
    },
    BuiltinMember {
        class: "Node",
        name: "set_process",
        brief: "set_process(enable: bool) -> void",
        description: "Enables or disables _process() callback.",
        kind: Method,
    },
    BuiltinMember {
        class: "Node",
        name: "set_physics_process",
        brief: "set_physics_process(enable: bool) -> void",
        description: "Enables or disables _physics_process() callback.",
        kind: Method,
    },
    BuiltinMember {
        class: "Node",
        name: "duplicate",
        brief: "duplicate(flags: int = 15) -> Node",
        description: "Duplicates this node and its children.",
        kind: Method,
    },
    BuiltinMember {
        class: "Node",
        name: "reparent",
        brief: "reparent(new_parent: Node, keep_global_transform: bool = true) -> void",
        description: "Moves this node to a new parent.",
        kind: Method,
    },
    // ── CanvasItem ──────────────────────────────────────────────────
    BuiltinMember {
        class: "CanvasItem",
        name: "visible",
        brief: "bool CanvasItem.visible",
        description: "If true, the node is drawn. Hidden nodes are not processed for rendering.",
        kind: Property,
    },
    BuiltinMember {
        class: "CanvasItem",
        name: "modulate",
        brief: "Color CanvasItem.modulate",
        description: "The color applied to this node and its children.",
        kind: Property,
    },
    BuiltinMember {
        class: "CanvasItem",
        name: "self_modulate",
        brief: "Color CanvasItem.self_modulate",
        description: "The color applied to this node only (not children).",
        kind: Property,
    },
    BuiltinMember {
        class: "CanvasItem",
        name: "show",
        brief: "show() -> void",
        description: "Makes the node visible.",
        kind: Method,
    },
    BuiltinMember {
        class: "CanvasItem",
        name: "hide",
        brief: "hide() -> void",
        description: "Makes the node invisible.",
        kind: Method,
    },
    BuiltinMember {
        class: "CanvasItem",
        name: "queue_redraw",
        brief: "queue_redraw() -> void",
        description: "Queues the CanvasItem for redraw (triggers _draw).",
        kind: Method,
    },
    BuiltinMember {
        class: "CanvasItem",
        name: "get_global_mouse_position",
        brief: "get_global_mouse_position() -> Vector2",
        description: "Returns the mouse position in global coordinates.",
        kind: Method,
    },
    // ── Node2D ──────────────────────────────────────────────────────
    BuiltinMember {
        class: "Node2D",
        name: "position",
        brief: "Vector2 Node2D.position",
        description: "Position relative to the parent node.",
        kind: Property,
    },
    BuiltinMember {
        class: "Node2D",
        name: "global_position",
        brief: "Vector2 Node2D.global_position",
        description: "Global position in world coordinates.",
        kind: Property,
    },
    BuiltinMember {
        class: "Node2D",
        name: "rotation",
        brief: "float Node2D.rotation",
        description: "Rotation in radians relative to the parent.",
        kind: Property,
    },
    BuiltinMember {
        class: "Node2D",
        name: "rotation_degrees",
        brief: "float Node2D.rotation_degrees",
        description: "Rotation in degrees relative to the parent.",
        kind: Property,
    },
    BuiltinMember {
        class: "Node2D",
        name: "scale",
        brief: "Vector2 Node2D.scale",
        description: "Scale of the node relative to the parent.",
        kind: Property,
    },
    BuiltinMember {
        class: "Node2D",
        name: "global_rotation",
        brief: "float Node2D.global_rotation",
        description: "Global rotation in radians.",
        kind: Property,
    },
    BuiltinMember {
        class: "Node2D",
        name: "global_rotation_degrees",
        brief: "float Node2D.global_rotation_degrees",
        description: "Global rotation in degrees.",
        kind: Property,
    },
    BuiltinMember {
        class: "Node2D",
        name: "global_scale",
        brief: "Vector2 Node2D.global_scale",
        description: "Global scale.",
        kind: Property,
    },
    BuiltinMember {
        class: "Node2D",
        name: "skew",
        brief: "float Node2D.skew",
        description: "Skew angle in radians.",
        kind: Property,
    },
    BuiltinMember {
        class: "Node2D",
        name: "z_index",
        brief: "int Node2D.z_index",
        description: "Controls the drawing order. Higher values are drawn on top.",
        kind: Property,
    },
    BuiltinMember {
        class: "Node2D",
        name: "look_at",
        brief: "look_at(point: Vector2) -> void",
        description: "Rotates the node to point toward the given position.",
        kind: Method,
    },
    BuiltinMember {
        class: "Node2D",
        name: "to_local",
        brief: "to_local(global_point: Vector2) -> Vector2",
        description: "Converts a global position to a local position.",
        kind: Method,
    },
    BuiltinMember {
        class: "Node2D",
        name: "to_global",
        brief: "to_global(local_point: Vector2) -> Vector2",
        description: "Converts a local position to a global position.",
        kind: Method,
    },
    BuiltinMember {
        class: "Node2D",
        name: "get_relative_transform_to_parent",
        brief: "get_relative_transform_to_parent(parent: Node) -> Transform2D",
        description: "Returns the transform relative to the given parent.",
        kind: Method,
    },
    // ── Node3D ──────────────────────────────────────────────────────
    BuiltinMember {
        class: "Node3D",
        name: "global_position",
        brief: "Vector3 Node3D.global_position",
        description: "Global position in world coordinates.",
        kind: Property,
    },
    BuiltinMember {
        class: "Node3D",
        name: "global_rotation",
        brief: "Vector3 Node3D.global_rotation",
        description: "Global rotation in radians (Euler angles).",
        kind: Property,
    },
    BuiltinMember {
        class: "Node3D",
        name: "global_rotation_degrees",
        brief: "Vector3 Node3D.global_rotation_degrees",
        description: "Global rotation in degrees.",
        kind: Property,
    },
    BuiltinMember {
        class: "Node3D",
        name: "global_transform",
        brief: "Transform3D Node3D.global_transform",
        description: "Global transform including position, rotation, and scale.",
        kind: Property,
    },
    BuiltinMember {
        class: "Node3D",
        name: "transform",
        brief: "Transform3D Node3D.transform",
        description: "Local transform relative to the parent.",
        kind: Property,
    },
    BuiltinMember {
        class: "Node3D",
        name: "basis",
        brief: "Basis Node3D.basis",
        description: "Local rotation and scale as a Basis matrix.",
        kind: Property,
    },
    BuiltinMember {
        class: "Node3D",
        name: "quaternion",
        brief: "Quaternion Node3D.quaternion",
        description: "Local rotation as a quaternion.",
        kind: Property,
    },
    BuiltinMember {
        class: "Node3D",
        name: "top_level",
        brief: "bool Node3D.top_level",
        description: "If true, the node is not affected by its parent's transform.",
        kind: Property,
    },
    // ── Control ─────────────────────────────────────────────────────
    BuiltinMember {
        class: "Control",
        name: "size",
        brief: "Vector2 Control.size",
        description: "The size of the control in pixels.",
        kind: Property,
    },
    BuiltinMember {
        class: "Control",
        name: "custom_minimum_size",
        brief: "Vector2 Control.custom_minimum_size",
        description: "The minimum size the control can shrink to.",
        kind: Property,
    },
    BuiltinMember {
        class: "Control",
        name: "anchor_left",
        brief: "float Control.anchor_left",
        description: "Left anchor (0.0 to 1.0) relative to parent.",
        kind: Property,
    },
    BuiltinMember {
        class: "Control",
        name: "anchor_right",
        brief: "float Control.anchor_right",
        description: "Right anchor (0.0 to 1.0) relative to parent.",
        kind: Property,
    },
    BuiltinMember {
        class: "Control",
        name: "anchor_top",
        brief: "float Control.anchor_top",
        description: "Top anchor (0.0 to 1.0) relative to parent.",
        kind: Property,
    },
    BuiltinMember {
        class: "Control",
        name: "anchor_bottom",
        brief: "float Control.anchor_bottom",
        description: "Bottom anchor (0.0 to 1.0) relative to parent.",
        kind: Property,
    },
    BuiltinMember {
        class: "Control",
        name: "mouse_filter",
        brief: "MouseFilter Control.mouse_filter",
        description: "Controls how the control handles mouse events (Stop, Pass, Ignore).",
        kind: Property,
    },
    BuiltinMember {
        class: "Control",
        name: "size_flags_horizontal",
        brief: "SizeFlags Control.size_flags_horizontal",
        description: "Horizontal size flags for Container layouts.",
        kind: Property,
    },
    BuiltinMember {
        class: "Control",
        name: "size_flags_vertical",
        brief: "SizeFlags Control.size_flags_vertical",
        description: "Vertical size flags for Container layouts.",
        kind: Property,
    },
    BuiltinMember {
        class: "Control",
        name: "tooltip_text",
        brief: "String Control.tooltip_text",
        description: "Text shown when hovering the control.",
        kind: Property,
    },
    BuiltinMember {
        class: "Control",
        name: "focus_mode",
        brief: "FocusMode Control.focus_mode",
        description: "Controls whether the control can receive keyboard focus.",
        kind: Property,
    },
    BuiltinMember {
        class: "Control",
        name: "grab_focus",
        brief: "grab_focus() -> void",
        description: "Gives keyboard focus to this control.",
        kind: Method,
    },
    BuiltinMember {
        class: "Control",
        name: "release_focus",
        brief: "release_focus() -> void",
        description: "Releases keyboard focus from this control.",
        kind: Method,
    },
    BuiltinMember {
        class: "Control",
        name: "has_focus",
        brief: "has_focus() -> bool",
        description: "Returns true if the control has keyboard focus.",
        kind: Method,
    },
    BuiltinMember {
        class: "Control",
        name: "get_rect",
        brief: "get_rect() -> Rect2",
        description: "Returns the control's position and size as a Rect2.",
        kind: Method,
    },
    BuiltinMember {
        class: "Control",
        name: "set_anchors_preset",
        brief: "set_anchors_preset(preset: LayoutPreset, keep_offsets: bool = false) -> void",
        description: "Sets anchors to a LayoutPreset (e.g. full rect, center).",
        kind: Method,
    },
    // ── CharacterBody2D ─────────────────────────────────────────────
    BuiltinMember {
        class: "CharacterBody2D",
        name: "velocity",
        brief: "Vector2 CharacterBody2D.velocity",
        description: "Current velocity vector used by move_and_slide().",
        kind: Property,
    },
    BuiltinMember {
        class: "CharacterBody2D",
        name: "floor_normal",
        brief: "Vector2 CharacterBody2D.floor_normal",
        description: "The floor's normal vector from the last move_and_slide().",
        kind: Property,
    },
    BuiltinMember {
        class: "CharacterBody2D",
        name: "motion_mode",
        brief: "MotionMode CharacterBody2D.motion_mode",
        description: "Grounded or Floating motion mode.",
        kind: Property,
    },
    BuiltinMember {
        class: "CharacterBody2D",
        name: "up_direction",
        brief: "Vector2 CharacterBody2D.up_direction",
        description: "Direction considered as up for floor/wall detection.",
        kind: Property,
    },
    BuiltinMember {
        class: "CharacterBody2D",
        name: "floor_max_angle",
        brief: "float CharacterBody2D.floor_max_angle",
        description: "Maximum angle (radians) for a surface to be considered a floor.",
        kind: Property,
    },
    BuiltinMember {
        class: "CharacterBody2D",
        name: "move_and_slide",
        brief: "move_and_slide() -> bool",
        description: "Moves the body using velocity. Returns true if a collision occurred.",
        kind: Method,
    },
    BuiltinMember {
        class: "CharacterBody2D",
        name: "is_on_floor",
        brief: "is_on_floor() -> bool",
        description: "Returns true if the body is on the floor after move_and_slide().",
        kind: Method,
    },
    BuiltinMember {
        class: "CharacterBody2D",
        name: "is_on_wall",
        brief: "is_on_wall() -> bool",
        description: "Returns true if the body is touching a wall.",
        kind: Method,
    },
    BuiltinMember {
        class: "CharacterBody2D",
        name: "is_on_ceiling",
        brief: "is_on_ceiling() -> bool",
        description: "Returns true if the body is touching a ceiling.",
        kind: Method,
    },
    BuiltinMember {
        class: "CharacterBody2D",
        name: "get_floor_normal",
        brief: "get_floor_normal() -> Vector2",
        description: "Returns the floor normal from the last collision.",
        kind: Method,
    },
    BuiltinMember {
        class: "CharacterBody2D",
        name: "get_slide_collision",
        brief: "get_slide_collision(index: int) -> KinematicCollision2D",
        description: "Returns collision data from move_and_slide() by index.",
        kind: Method,
    },
    // ── CharacterBody3D ─────────────────────────────────────────────
    BuiltinMember {
        class: "CharacterBody3D",
        name: "velocity",
        brief: "Vector3 CharacterBody3D.velocity",
        description: "Current velocity vector used by move_and_slide().",
        kind: Property,
    },
    BuiltinMember {
        class: "CharacterBody3D",
        name: "floor_normal",
        brief: "Vector3 CharacterBody3D.floor_normal",
        description: "The floor's normal vector from the last move_and_slide().",
        kind: Property,
    },
    BuiltinMember {
        class: "CharacterBody3D",
        name: "motion_mode",
        brief: "MotionMode CharacterBody3D.motion_mode",
        description: "Grounded or Floating motion mode.",
        kind: Property,
    },
    BuiltinMember {
        class: "CharacterBody3D",
        name: "up_direction",
        brief: "Vector3 CharacterBody3D.up_direction",
        description: "Direction considered as up for floor/wall detection.",
        kind: Property,
    },
    BuiltinMember {
        class: "CharacterBody3D",
        name: "floor_max_angle",
        brief: "float CharacterBody3D.floor_max_angle",
        description: "Maximum angle (radians) for a surface to be considered a floor.",
        kind: Property,
    },
    BuiltinMember {
        class: "CharacterBody3D",
        name: "move_and_slide",
        brief: "move_and_slide() -> bool",
        description: "Moves the body using velocity. Returns true if a collision occurred.",
        kind: Method,
    },
    BuiltinMember {
        class: "CharacterBody3D",
        name: "is_on_floor",
        brief: "is_on_floor() -> bool",
        description: "Returns true if the body is on the floor after move_and_slide().",
        kind: Method,
    },
    BuiltinMember {
        class: "CharacterBody3D",
        name: "is_on_wall",
        brief: "is_on_wall() -> bool",
        description: "Returns true if the body is touching a wall.",
        kind: Method,
    },
    BuiltinMember {
        class: "CharacterBody3D",
        name: "is_on_ceiling",
        brief: "is_on_ceiling() -> bool",
        description: "Returns true if the body is touching a ceiling.",
        kind: Method,
    },
    BuiltinMember {
        class: "CharacterBody3D",
        name: "get_floor_normal",
        brief: "get_floor_normal() -> Vector3",
        description: "Returns the floor normal from the last collision.",
        kind: Method,
    },
    BuiltinMember {
        class: "CharacterBody3D",
        name: "get_slide_collision",
        brief: "get_slide_collision(index: int) -> KinematicCollision3D",
        description: "Returns collision data from move_and_slide() by index.",
        kind: Method,
    },
    // ── RigidBody2D ─────────────────────────────────────────────────
    BuiltinMember {
        class: "RigidBody2D",
        name: "mass",
        brief: "float RigidBody2D.mass",
        description: "The body's mass in kilograms.",
        kind: Property,
    },
    BuiltinMember {
        class: "RigidBody2D",
        name: "linear_velocity",
        brief: "Vector2 RigidBody2D.linear_velocity",
        description: "The body's linear velocity in pixels per second.",
        kind: Property,
    },
    BuiltinMember {
        class: "RigidBody2D",
        name: "angular_velocity",
        brief: "float RigidBody2D.angular_velocity",
        description: "The body's angular velocity in radians per second.",
        kind: Property,
    },
    BuiltinMember {
        class: "RigidBody2D",
        name: "gravity_scale",
        brief: "float RigidBody2D.gravity_scale",
        description: "Multiplier for gravity affecting this body.",
        kind: Property,
    },
    BuiltinMember {
        class: "RigidBody2D",
        name: "apply_force",
        brief: "apply_force(force: Vector2, position: Vector2 = Vector2(0, 0)) -> void",
        description: "Applies a continuous force at a position (in local coords).",
        kind: Method,
    },
    BuiltinMember {
        class: "RigidBody2D",
        name: "apply_impulse",
        brief: "apply_impulse(impulse: Vector2, position: Vector2 = Vector2(0, 0)) -> void",
        description: "Applies an instant impulse at a position.",
        kind: Method,
    },
    BuiltinMember {
        class: "RigidBody2D",
        name: "apply_central_impulse",
        brief: "apply_central_impulse(impulse: Vector2) -> void",
        description: "Applies an instant impulse at the center of mass.",
        kind: Method,
    },
    // ── RigidBody3D ─────────────────────────────────────────────────
    BuiltinMember {
        class: "RigidBody3D",
        name: "mass",
        brief: "float RigidBody3D.mass",
        description: "The body's mass in kilograms.",
        kind: Property,
    },
    BuiltinMember {
        class: "RigidBody3D",
        name: "linear_velocity",
        brief: "Vector3 RigidBody3D.linear_velocity",
        description: "The body's linear velocity.",
        kind: Property,
    },
    BuiltinMember {
        class: "RigidBody3D",
        name: "angular_velocity",
        brief: "Vector3 RigidBody3D.angular_velocity",
        description: "The body's angular velocity in radians per second.",
        kind: Property,
    },
    BuiltinMember {
        class: "RigidBody3D",
        name: "gravity_scale",
        brief: "float RigidBody3D.gravity_scale",
        description: "Multiplier for gravity affecting this body.",
        kind: Property,
    },
    BuiltinMember {
        class: "RigidBody3D",
        name: "apply_force",
        brief: "apply_force(force: Vector3, position: Vector3 = Vector3(0, 0, 0)) -> void",
        description: "Applies a continuous force at a position (in local coords).",
        kind: Method,
    },
    BuiltinMember {
        class: "RigidBody3D",
        name: "apply_impulse",
        brief: "apply_impulse(impulse: Vector3, position: Vector3 = Vector3(0, 0, 0)) -> void",
        description: "Applies an instant impulse at a position.",
        kind: Method,
    },
    BuiltinMember {
        class: "RigidBody3D",
        name: "apply_central_impulse",
        brief: "apply_central_impulse(impulse: Vector3) -> void",
        description: "Applies an instant impulse at the center of mass.",
        kind: Method,
    },
    // ── Sprite2D ────────────────────────────────────────────────────
    BuiltinMember {
        class: "Sprite2D",
        name: "texture",
        brief: "Texture2D Sprite2D.texture",
        description: "The texture displayed by this sprite.",
        kind: Property,
    },
    BuiltinMember {
        class: "Sprite2D",
        name: "offset",
        brief: "Vector2 Sprite2D.offset",
        description: "The texture's drawing offset.",
        kind: Property,
    },
    BuiltinMember {
        class: "Sprite2D",
        name: "flip_h",
        brief: "bool Sprite2D.flip_h",
        description: "If true, the texture is flipped horizontally.",
        kind: Property,
    },
    BuiltinMember {
        class: "Sprite2D",
        name: "flip_v",
        brief: "bool Sprite2D.flip_v",
        description: "If true, the texture is flipped vertically.",
        kind: Property,
    },
    BuiltinMember {
        class: "Sprite2D",
        name: "frame",
        brief: "int Sprite2D.frame",
        description: "Current frame index for spritesheet animation.",
        kind: Property,
    },
    BuiltinMember {
        class: "Sprite2D",
        name: "hframes",
        brief: "int Sprite2D.hframes",
        description: "Number of horizontal frames in the spritesheet.",
        kind: Property,
    },
    BuiltinMember {
        class: "Sprite2D",
        name: "vframes",
        brief: "int Sprite2D.vframes",
        description: "Number of vertical frames in the spritesheet.",
        kind: Property,
    },
    // ── Timer ───────────────────────────────────────────────────────
    BuiltinMember {
        class: "Timer",
        name: "wait_time",
        brief: "float Timer.wait_time",
        description: "The wait time in seconds.",
        kind: Property,
    },
    BuiltinMember {
        class: "Timer",
        name: "one_shot",
        brief: "bool Timer.one_shot",
        description: "If true, the timer stops after timing out (no repeat).",
        kind: Property,
    },
    BuiltinMember {
        class: "Timer",
        name: "autostart",
        brief: "bool Timer.autostart",
        description: "If true, the timer starts automatically when entering the tree.",
        kind: Property,
    },
    BuiltinMember {
        class: "Timer",
        name: "time_left",
        brief: "float Timer.time_left",
        description: "The remaining time in seconds (read-only).",
        kind: Property,
    },
    BuiltinMember {
        class: "Timer",
        name: "start",
        brief: "start(time_sec: float = -1) -> void",
        description: "Starts the timer. Optionally overrides wait_time.",
        kind: Method,
    },
    BuiltinMember {
        class: "Timer",
        name: "stop",
        brief: "stop() -> void",
        description: "Stops the timer.",
        kind: Method,
    },
    BuiltinMember {
        class: "Timer",
        name: "is_stopped",
        brief: "is_stopped() -> bool",
        description: "Returns true if the timer is not running.",
        kind: Method,
    },
    // ── AnimationPlayer ─────────────────────────────────────────────
    BuiltinMember {
        class: "AnimationPlayer",
        name: "current_animation",
        brief: "String AnimationPlayer.current_animation",
        description: "The name of the currently playing animation.",
        kind: Property,
    },
    BuiltinMember {
        class: "AnimationPlayer",
        name: "speed_scale",
        brief: "float AnimationPlayer.speed_scale",
        description: "Playback speed multiplier.",
        kind: Property,
    },
    BuiltinMember {
        class: "AnimationPlayer",
        name: "play",
        brief: "play(name: StringName = \"\", custom_blend: float = -1, custom_speed: float = 1.0, from_end: bool = false) -> void",
        description: "Plays an animation by name.",
        kind: Method,
    },
    BuiltinMember {
        class: "AnimationPlayer",
        name: "stop",
        brief: "stop(keep_state: bool = false) -> void",
        description: "Stops the current animation.",
        kind: Method,
    },
    BuiltinMember {
        class: "AnimationPlayer",
        name: "pause",
        brief: "pause() -> void",
        description: "Pauses the current animation.",
        kind: Method,
    },
    BuiltinMember {
        class: "AnimationPlayer",
        name: "is_playing",
        brief: "is_playing() -> bool",
        description: "Returns true if an animation is currently playing.",
        kind: Method,
    },
    BuiltinMember {
        class: "AnimationPlayer",
        name: "get_animation",
        brief: "get_animation(name: StringName) -> Animation",
        description: "Returns the Animation resource with the given name.",
        kind: Method,
    },
    BuiltinMember {
        class: "AnimationPlayer",
        name: "has_animation",
        brief: "has_animation(name: StringName) -> bool",
        description: "Returns true if an animation with the given name exists.",
        kind: Method,
    },
    // ── Tween ───────────────────────────────────────────────────────
    BuiltinMember {
        class: "Tween",
        name: "tween_property",
        brief: "tween_property(object: Object, property: NodePath, final_val: Variant, duration: float) -> PropertyTweener",
        description: "Tweens a property from its current value to final_val.",
        kind: Method,
    },
    BuiltinMember {
        class: "Tween",
        name: "tween_callback",
        brief: "tween_callback(callback: Callable) -> CallbackTweener",
        description: "Calls a method when the tween reaches this point.",
        kind: Method,
    },
    BuiltinMember {
        class: "Tween",
        name: "tween_interval",
        brief: "tween_interval(time: float) -> IntervalTweener",
        description: "Adds a delay to the tween sequence.",
        kind: Method,
    },
    BuiltinMember {
        class: "Tween",
        name: "tween_method",
        brief: "tween_method(method: Callable, from: Variant, to: Variant, duration: float) -> MethodTweener",
        description: "Calls a method with an interpolated value over time.",
        kind: Method,
    },
    BuiltinMember {
        class: "Tween",
        name: "set_ease",
        brief: "set_ease(ease: EaseType) -> Tween",
        description: "Sets the default ease type for subsequent tweeners.",
        kind: Method,
    },
    BuiltinMember {
        class: "Tween",
        name: "set_trans",
        brief: "set_trans(trans: TransitionType) -> Tween",
        description: "Sets the default transition type for subsequent tweeners.",
        kind: Method,
    },
    BuiltinMember {
        class: "Tween",
        name: "parallel",
        brief: "parallel() -> Tween",
        description: "Makes the next tweener run in parallel with the current one.",
        kind: Method,
    },
    BuiltinMember {
        class: "Tween",
        name: "chain",
        brief: "chain() -> Tween",
        description: "Makes the next tweener run after the current one (default).",
        kind: Method,
    },
    BuiltinMember {
        class: "Tween",
        name: "kill",
        brief: "kill() -> void",
        description: "Aborts all tweening operations.",
        kind: Method,
    },
    BuiltinMember {
        class: "Tween",
        name: "is_running",
        brief: "is_running() -> bool",
        description: "Returns true if any tweeners are still active.",
        kind: Method,
    },
    // Builtin value types (Vector2, String, Array, etc.) are in GENERATED_MEMBERS.
];

/// Return all built-in members for the given exact class name.
/// Caller should walk the inheritance chain and call this per ancestor.
/// Checks both hand-written engine class docs and auto-generated builtin type docs.
pub fn members_for_class(class: &str) -> Vec<&'static BuiltinMember> {
    BUILTIN_MEMBER_DOCS
        .iter()
        .chain(super::builtin_generated::GENERATED_MEMBERS.iter())
        .filter(|m| m.class == class)
        .collect()
}

/// Look up a built-in member by name (e.g. `global_position`, `move_and_slide`).
/// Returns the first match — hand-written docs take priority over generated.
pub fn lookup_member(name: &str) -> Option<&'static BuiltinMember> {
    BUILTIN_MEMBER_DOCS
        .iter()
        .chain(super::builtin_generated::GENERATED_MEMBERS.iter())
        .find(|m| m.name == name)
}

/// Look up a built-in member by class and name (exact class match).
/// Hand-written docs take priority over generated.
pub fn lookup_member_for(class: &str, name: &str) -> Option<&'static BuiltinMember> {
    BUILTIN_MEMBER_DOCS
        .iter()
        .chain(super::builtin_generated::GENERATED_MEMBERS.iter())
        .find(|m| m.class == class && m.name == name)
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
    format!(
        "```gdscript\n{}\n```\n({} {}) {}\n\n[Godot docs]({})",
        doc.brief, doc.class, kind_label, doc.description, url
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
        let hover = format_member_hover(doc);
        assert!(hover.contains("global_position"));
        assert!(hover.contains("property"));
        assert!(hover.contains("Godot docs"));
        assert!(hover.contains("class-node2d-property-global-position"));
    }

    #[test]
    fn member_hover_method_format() {
        let doc = lookup_member("queue_free").unwrap();
        let hover = format_member_hover(doc);
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
        assert!(names.contains(&"look_at"));
        // Should NOT include Node members (exact class match)
        assert!(!names.contains(&"add_child"));
    }

    #[test]
    fn members_for_class_empty() {
        let members = members_for_class("NonExistentClass");
        assert!(members.is_empty());
    }
}
