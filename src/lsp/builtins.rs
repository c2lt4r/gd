//! Built-in Godot type and function documentation for LSP hover/completion.

pub struct BuiltinDoc {
    pub name: &'static str,
    pub brief: &'static str,
    pub description: &'static str,
}

// ── Built-in types ──────────────────────────────────────────────────

const BUILTIN_TYPE_DOCS: &[BuiltinDoc] = &[
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
    BuiltinDoc {
        name: "String",
        brief: "String type",
        description: "Built-in string type using Unicode.",
    },
    BuiltinDoc {
        name: "Vector2",
        brief: "2D vector (float)",
        description: "2-element structure for 2D coordinates and 2D math using floating-point.",
    },
    BuiltinDoc {
        name: "Vector2i",
        brief: "2D vector (int)",
        description: "2-element structure for 2D coordinates using integers.",
    },
    BuiltinDoc {
        name: "Vector3",
        brief: "3D vector (float)",
        description: "3-element structure for 3D coordinates and 3D math using floating-point.",
    },
    BuiltinDoc {
        name: "Vector3i",
        brief: "3D vector (int)",
        description: "3-element structure for 3D coordinates using integers.",
    },
    BuiltinDoc {
        name: "Vector4",
        brief: "4D vector (float)",
        description: "4-element structure for 4D math using floating-point.",
    },
    BuiltinDoc {
        name: "Vector4i",
        brief: "4D vector (int)",
        description: "4-element structure for 4D math using integers.",
    },
    BuiltinDoc {
        name: "Array",
        brief: "Generic array",
        description: "Generic sequence of arbitrary object types, including other arrays.",
    },
    BuiltinDoc {
        name: "Dictionary",
        brief: "Key-value store",
        description: "Associative container mapping unique keys to values.",
    },
    BuiltinDoc {
        name: "NodePath",
        brief: "Node path",
        description: "Pre-parsed path to a node or node property for efficient access.",
    },
    BuiltinDoc {
        name: "StringName",
        brief: "Interned string",
        description: "Optimized immutable string type for fast comparison. Used for identifiers.",
    },
    BuiltinDoc {
        name: "Color",
        brief: "RGBA color",
        description: "Color represented in RGBA format with floats on the range of 0 to 1.",
    },
    BuiltinDoc {
        name: "Rect2",
        brief: "2D rectangle",
        description: "2D axis-aligned bounding box using floating-point coordinates.",
    },
    BuiltinDoc {
        name: "Transform2D",
        brief: "2D transform",
        description: "2x3 matrix representing a 2D transformation (translation, rotation, scale).",
    },
    BuiltinDoc {
        name: "Transform3D",
        brief: "3D transform",
        description: "3x4 matrix representing a 3D transformation (translation, rotation, scale).",
    },
    BuiltinDoc {
        name: "Basis",
        brief: "3x3 rotation matrix",
        description: "3x3 matrix for representing 3D rotation and scale.",
    },
    BuiltinDoc {
        name: "AABB",
        brief: "3D bounding box",
        description: "3D axis-aligned bounding box.",
    },
    BuiltinDoc {
        name: "Plane",
        brief: "3D plane",
        description: "Infinite plane in 3D space, defined by a normal and distance from origin.",
    },
    BuiltinDoc {
        name: "Quaternion",
        brief: "Quaternion rotation",
        description: "Rotation representation using a quaternion. Useful for smooth interpolation.",
    },
    BuiltinDoc {
        name: "PackedByteArray",
        brief: "Packed byte array",
        description: "Packed array of bytes. Memory-efficient for binary data.",
    },
    BuiltinDoc {
        name: "PackedInt32Array",
        brief: "Packed int32 array",
        description: "Packed array of 32-bit integers.",
    },
    BuiltinDoc {
        name: "PackedInt64Array",
        brief: "Packed int64 array",
        description: "Packed array of 64-bit integers.",
    },
    BuiltinDoc {
        name: "PackedFloat32Array",
        brief: "Packed float32 array",
        description: "Packed array of 32-bit floats.",
    },
    BuiltinDoc {
        name: "PackedFloat64Array",
        brief: "Packed float64 array",
        description: "Packed array of 64-bit floats.",
    },
    BuiltinDoc {
        name: "PackedStringArray",
        brief: "Packed string array",
        description: "Packed array of strings.",
    },
    BuiltinDoc {
        name: "PackedVector2Array",
        brief: "Packed Vector2 array",
        description: "Packed array of Vector2 values.",
    },
    BuiltinDoc {
        name: "PackedVector3Array",
        brief: "Packed Vector3 array",
        description: "Packed array of Vector3 values.",
    },
    // Common node classes
    BuiltinDoc {
        name: "Node",
        brief: "class Node",
        description: "Base class for all scene tree nodes. Provides lifecycle callbacks and tree management.",
    },
    BuiltinDoc {
        name: "Node2D",
        brief: "class Node2D extends CanvasItem",
        description: "Base class for 2D game objects. Provides 2D transform, z-index, and visibility.",
    },
    BuiltinDoc {
        name: "Node3D",
        brief: "class Node3D extends Node",
        description: "Base class for 3D game objects. Provides transform, visibility, and scene tree functionality.",
    },
    BuiltinDoc {
        name: "Control",
        brief: "class Control extends CanvasItem",
        description: "Base class for all UI-related nodes. Provides anchors, margins, and input handling.",
    },
    BuiltinDoc {
        name: "Sprite2D",
        brief: "class Sprite2D extends Node2D",
        description: "Displays a 2D texture. Can be used for characters, items, and other visual elements.",
    },
    BuiltinDoc {
        name: "Sprite3D",
        brief: "class Sprite3D extends SpriteBase3D",
        description: "Displays a 2D texture in 3D space as a billboard or flat sprite.",
    },
    BuiltinDoc {
        name: "CharacterBody2D",
        brief: "class CharacterBody2D extends PhysicsBody2D",
        description: "Kinematic body for 2D character movement with built-in collision response.",
    },
    BuiltinDoc {
        name: "CharacterBody3D",
        brief: "class CharacterBody3D extends PhysicsBody3D",
        description: "Kinematic body for 3D character movement with built-in collision response.",
    },
    BuiltinDoc {
        name: "RigidBody2D",
        brief: "class RigidBody2D extends PhysicsBody2D",
        description: "Physics body driven by the 2D physics simulation.",
    },
    BuiltinDoc {
        name: "RigidBody3D",
        brief: "class RigidBody3D extends PhysicsBody3D",
        description: "Physics body driven by the 3D physics simulation.",
    },
    BuiltinDoc {
        name: "Area2D",
        brief: "class Area2D extends CollisionObject2D",
        description: "2D area for detection and physics influence (gravity, damping).",
    },
    BuiltinDoc {
        name: "Area3D",
        brief: "class Area3D extends CollisionObject3D",
        description: "3D area for detection and physics influence (gravity, damping).",
    },
    BuiltinDoc {
        name: "Camera2D",
        brief: "class Camera2D extends Node2D",
        description: "Camera node for 2D scenes. Controls viewport scrolling.",
    },
    BuiltinDoc {
        name: "Camera3D",
        brief: "class Camera3D extends Node3D",
        description: "Camera node for 3D scenes. Defines the viewpoint for rendering.",
    },
    BuiltinDoc {
        name: "AnimationPlayer",
        brief: "class AnimationPlayer extends AnimationMixer",
        description: "Plays animations from an AnimationLibrary. Can animate any property.",
    },
    BuiltinDoc {
        name: "Timer",
        brief: "class Timer extends Node",
        description: "Countdown timer node. Emits `timeout` signal when time runs out.",
    },
    BuiltinDoc {
        name: "TileMap",
        brief: "class TileMap extends Node2D",
        description: "Node for 2D tile-based maps. Uses TileSet resources for tile data.",
    },
    BuiltinDoc {
        name: "Label",
        brief: "class Label extends Control",
        description: "Displays plain text. Supports alignment and text wrapping.",
    },
    BuiltinDoc {
        name: "Button",
        brief: "class Button extends BaseButton",
        description: "Standard themed button that can contain text and an icon.",
    },
    BuiltinDoc {
        name: "TextureRect",
        brief: "class TextureRect extends Control",
        description: "Displays a texture inside a Control. Supports various stretch modes.",
    },
    BuiltinDoc {
        name: "ColorRect",
        brief: "class ColorRect extends Control",
        description: "Displays a solid color rectangle. Useful for backgrounds and UI elements.",
    },
    BuiltinDoc {
        name: "RichTextLabel",
        brief: "class RichTextLabel extends Control",
        description: "Label that displays rich text using BBCode markup.",
    },
    BuiltinDoc {
        name: "LineEdit",
        brief: "class LineEdit extends Control",
        description: "Single-line text input field.",
    },
    BuiltinDoc {
        name: "TextEdit",
        brief: "class TextEdit extends Control",
        description: "Multi-line text editing control.",
    },
    BuiltinDoc {
        name: "AudioStreamPlayer",
        brief: "class AudioStreamPlayer extends Node",
        description: "Plays audio non-positionally. For background music and UI sounds.",
    },
    BuiltinDoc {
        name: "Resource",
        brief: "class Resource extends RefCounted",
        description: "Base class for serializable data containers.",
    },
    BuiltinDoc {
        name: "PackedScene",
        brief: "class PackedScene extends Resource",
        description: "Serialized scene that can be instantiated at runtime.",
    },
    BuiltinDoc {
        name: "SceneTree",
        brief: "class SceneTree extends MainLoop",
        description: "Manages the game loop, scene hierarchy, and groups.",
    },
    BuiltinDoc {
        name: "Tween",
        brief: "class Tween extends RefCounted",
        description: "Lightweight animation tool for interpolating properties over time.",
    },
    BuiltinDoc {
        name: "InputEvent",
        brief: "class InputEvent extends Resource",
        description: "Base class for all input events (key, mouse, touch, etc.).",
    },
    BuiltinDoc {
        name: "CollisionShape2D",
        brief: "class CollisionShape2D extends Node2D",
        description: "Defines a collision shape for a CollisionObject2D parent.",
    },
    BuiltinDoc {
        name: "CollisionShape3D",
        brief: "class CollisionShape3D extends Node3D",
        description: "Defines a collision shape for a CollisionObject3D parent.",
    },
    BuiltinDoc {
        name: "RayCast2D",
        brief: "class RayCast2D extends Node2D",
        description: "Casts a ray to detect 2D collision objects along its path.",
    },
    BuiltinDoc {
        name: "RayCast3D",
        brief: "class RayCast3D extends Node3D",
        description: "Casts a ray to detect 3D collision objects along its path.",
    },
    BuiltinDoc {
        name: "NavigationAgent2D",
        brief: "class NavigationAgent2D extends Node",
        description: "Agent for 2D pathfinding navigation.",
    },
    BuiltinDoc {
        name: "NavigationAgent3D",
        brief: "class NavigationAgent3D extends Node",
        description: "Agent for 3D pathfinding navigation.",
    },
];

// ── Built-in functions ──────────────────────────────────────────────

const BUILTIN_FUNCTION_DOCS: &[BuiltinDoc] = &[
    BuiltinDoc {
        name: "print",
        brief: "print(...) -> void",
        description: "Prints values to the console.",
    },
    BuiltinDoc {
        name: "prints",
        brief: "prints(...) -> void",
        description: "Prints values to the console separated by spaces.",
    },
    BuiltinDoc {
        name: "printt",
        brief: "printt(...) -> void",
        description: "Prints values to the console separated by tabs.",
    },
    BuiltinDoc {
        name: "printerr",
        brief: "printerr(...) -> void",
        description: "Prints values to stderr.",
    },
    BuiltinDoc {
        name: "push_error",
        brief: "push_error(...) -> void",
        description: "Pushes an error message to the Godot error log.",
    },
    BuiltinDoc {
        name: "push_warning",
        brief: "push_warning(...) -> void",
        description: "Pushes a warning message to the Godot error log.",
    },
    BuiltinDoc {
        name: "str",
        brief: "str(value) -> String",
        description: "Converts a value to its string representation.",
    },
    BuiltinDoc {
        name: "len",
        brief: "len(value) -> int",
        description: "Returns the length of a string, array, or dictionary.",
    },
    BuiltinDoc {
        name: "range",
        brief: "range(...) -> Array[int]",
        description: "Returns an integer sequence. Accepts (end), (start, end), or (start, end, step).",
    },
    BuiltinDoc {
        name: "typeof",
        brief: "typeof(value) -> int",
        description: "Returns the internal type index of a value.",
    },
    BuiltinDoc {
        name: "is_instance_of",
        brief: "is_instance_of(value, type) -> bool",
        description: "Returns true if value is an instance of the given type.",
    },
    BuiltinDoc {
        name: "abs",
        brief: "abs(x) -> Variant",
        description: "Returns the absolute value of a number.",
    },
    BuiltinDoc {
        name: "sign",
        brief: "sign(x) -> Variant",
        description: "Returns -1, 0, or 1 depending on the sign of x.",
    },
    BuiltinDoc {
        name: "min",
        brief: "min(...) -> Variant",
        description: "Returns the smallest of the given values.",
    },
    BuiltinDoc {
        name: "max",
        brief: "max(...) -> Variant",
        description: "Returns the largest of the given values.",
    },
    BuiltinDoc {
        name: "clamp",
        brief: "clamp(value, min, max) -> Variant",
        description: "Clamps a value between a minimum and maximum.",
    },
    BuiltinDoc {
        name: "lerp",
        brief: "lerp(from, to, weight) -> Variant",
        description: "Linearly interpolates between two values by a weight (0.0 to 1.0).",
    },
    BuiltinDoc {
        name: "smoothstep",
        brief: "smoothstep(from, to, x) -> float",
        description: "Returns a smooth Hermite interpolation between 0 and 1.",
    },
    BuiltinDoc {
        name: "sqrt",
        brief: "sqrt(x) -> float",
        description: "Returns the square root of x.",
    },
    BuiltinDoc {
        name: "pow",
        brief: "pow(base, exp) -> float",
        description: "Returns base raised to the power of exp.",
    },
    BuiltinDoc {
        name: "sin",
        brief: "sin(angle) -> float",
        description: "Returns the sine of an angle in radians.",
    },
    BuiltinDoc {
        name: "cos",
        brief: "cos(angle) -> float",
        description: "Returns the cosine of an angle in radians.",
    },
    BuiltinDoc {
        name: "tan",
        brief: "tan(angle) -> float",
        description: "Returns the tangent of an angle in radians.",
    },
    BuiltinDoc {
        name: "floor",
        brief: "floor(x) -> Variant",
        description: "Rounds x downward to the nearest integer.",
    },
    BuiltinDoc {
        name: "ceil",
        brief: "ceil(x) -> Variant",
        description: "Rounds x upward to the nearest integer.",
    },
    BuiltinDoc {
        name: "round",
        brief: "round(x) -> Variant",
        description: "Rounds x to the nearest integer.",
    },
    BuiltinDoc {
        name: "randi",
        brief: "randi() -> int",
        description: "Returns a random 32-bit unsigned integer.",
    },
    BuiltinDoc {
        name: "randf",
        brief: "randf() -> float",
        description: "Returns a random float between 0.0 and 1.0.",
    },
    BuiltinDoc {
        name: "randomize",
        brief: "randomize() -> void",
        description: "Randomizes the seed of the random number generator.",
    },
    BuiltinDoc {
        name: "seed",
        brief: "seed(value: int) -> void",
        description: "Sets the seed for the random number generator.",
    },
    BuiltinDoc {
        name: "hash",
        brief: "hash(value) -> int",
        description: "Returns the integer hash of a variable.",
    },
    BuiltinDoc {
        name: "is_equal_approx",
        brief: "is_equal_approx(a, b) -> bool",
        description: "Returns true if a and b are approximately equal (within float tolerance).",
    },
    BuiltinDoc {
        name: "is_zero_approx",
        brief: "is_zero_approx(x) -> bool",
        description: "Returns true if x is approximately zero.",
    },
];

// ── Lifecycle methods ───────────────────────────────────────────────

const LIFECYCLE_METHOD_DOCS: &[BuiltinDoc] = &[
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

/// Look up a built-in type by name.
pub fn lookup_type(name: &str) -> Option<&'static BuiltinDoc> {
    BUILTIN_TYPE_DOCS.iter().find(|d| d.name == name)
}

/// Look up a built-in function by name.
pub fn lookup_function(name: &str) -> Option<&'static BuiltinDoc> {
    BUILTIN_FUNCTION_DOCS
        .iter()
        .chain(LIFECYCLE_METHOD_DOCS.iter())
        .find(|d| d.name == name)
}

/// Generate a link to the Godot documentation for a class.
pub fn godot_docs_url(class_name: &str) -> String {
    format!(
        "https://docs.godotengine.org/en/stable/classes/class_{}.html",
        class_name.to_lowercase()
    )
}

/// Format a hover string for a built-in type.
pub fn format_type_hover(doc: &BuiltinDoc) -> String {
    let mut result = format!("```gdscript\n{}\n```\n{}", doc.brief, doc.description);
    // Add docs link for classes (types that start with uppercase, not primitives)
    let first_char = doc.name.chars().next().unwrap_or('a');
    if first_char.is_uppercase() {
        result.push_str(&format!("\n\n[Godot docs]({})", godot_docs_url(doc.name)));
    }
    result
}

/// Format a hover string for a built-in function.
pub fn format_function_hover(doc: &BuiltinDoc) -> String {
    format!("```gdscript\n{}\n```\n{}", doc.brief, doc.description)
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
    fn lookup_known_class() {
        let doc = lookup_type("Node3D").unwrap();
        assert_eq!(doc.name, "Node3D");
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
        let doc = lookup_type("Node3D").unwrap();
        let hover = format_type_hover(doc);
        assert!(hover.contains("Godot docs"));
        assert!(hover.contains("class_node3d.html"));
    }

    #[test]
    fn primitive_hover_no_docs_link() {
        let doc = lookup_type("int").unwrap();
        let hover = format_type_hover(doc);
        assert!(!hover.contains("Godot docs"));
    }

    #[test]
    fn function_hover_format() {
        let doc = lookup_function("lerp").unwrap();
        let hover = format_function_hover(doc);
        assert!(hover.contains("lerp"));
        assert!(hover.contains("interpolates"));
    }
}
