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
    // ── Vector2 ─────────────────────────────────────────────────────
    BuiltinMember {
        class: "Vector2",
        name: "normalized",
        brief: "normalized() -> Vector2",
        description: "Returns a unit-length vector with the same direction.",
        kind: Method,
    },
    BuiltinMember {
        class: "Vector2",
        name: "length",
        brief: "length() -> float",
        description: "Returns the length (magnitude) of the vector.",
        kind: Method,
    },
    BuiltinMember {
        class: "Vector2",
        name: "distance_to",
        brief: "distance_to(to: Vector2) -> float",
        description: "Returns the distance to another point.",
        kind: Method,
    },
    BuiltinMember {
        class: "Vector2",
        name: "angle_to",
        brief: "angle_to(to: Vector2) -> float",
        description: "Returns the angle to another vector in radians.",
        kind: Method,
    },
    BuiltinMember {
        class: "Vector2",
        name: "lerp",
        brief: "lerp(to: Vector2, weight: float) -> Vector2",
        description: "Linearly interpolates toward another vector.",
        kind: Method,
    },
    BuiltinMember {
        class: "Vector2",
        name: "move_toward",
        brief: "move_toward(to: Vector2, delta: float) -> Vector2",
        description: "Moves toward another vector by a fixed amount.",
        kind: Method,
    },
    BuiltinMember {
        class: "Vector2",
        name: "rotated",
        brief: "rotated(angle: float) -> Vector2",
        description: "Returns the vector rotated by the given angle in radians.",
        kind: Method,
    },
    BuiltinMember {
        class: "Vector2",
        name: "dot",
        brief: "dot(with: Vector2) -> float",
        description: "Returns the dot product with another vector.",
        kind: Method,
    },
    BuiltinMember {
        class: "Vector2",
        name: "cross",
        brief: "cross(with: Vector2) -> float",
        description: "Returns the 2D cross product with another vector.",
        kind: Method,
    },
    // ── Vector3 ─────────────────────────────────────────────────────
    BuiltinMember {
        class: "Vector3",
        name: "normalized",
        brief: "normalized() -> Vector3",
        description: "Returns a unit-length vector with the same direction.",
        kind: Method,
    },
    BuiltinMember {
        class: "Vector3",
        name: "length",
        brief: "length() -> float",
        description: "Returns the length (magnitude) of the vector.",
        kind: Method,
    },
    BuiltinMember {
        class: "Vector3",
        name: "distance_to",
        brief: "distance_to(to: Vector3) -> float",
        description: "Returns the distance to another point.",
        kind: Method,
    },
    // ── String ──────────────────────────────────────────────────────
    BuiltinMember {
        class: "String",
        name: "length",
        brief: "length() -> int",
        description: "Returns the number of characters in the string.",
        kind: Method,
    },
    BuiltinMember {
        class: "String",
        name: "substr",
        brief: "substr(from: int, len: int = -1) -> String",
        description: "Returns a substring starting at the given position.",
        kind: Method,
    },
    BuiltinMember {
        class: "String",
        name: "find",
        brief: "find(what: String, from: int = 0) -> int",
        description: "Returns the index of the first occurrence, or -1.",
        kind: Method,
    },
    BuiltinMember {
        class: "String",
        name: "begins_with",
        brief: "begins_with(text: String) -> bool",
        description: "Returns true if the string starts with the given text.",
        kind: Method,
    },
    BuiltinMember {
        class: "String",
        name: "ends_with",
        brief: "ends_with(text: String) -> bool",
        description: "Returns true if the string ends with the given text.",
        kind: Method,
    },
    BuiltinMember {
        class: "String",
        name: "split",
        brief: "split(delimiter: String = \"\", allow_empty: bool = true, maxsplit: int = 0) -> PackedStringArray",
        description: "Splits the string by delimiter.",
        kind: Method,
    },
    BuiltinMember {
        class: "String",
        name: "strip_edges",
        brief: "strip_edges(left: bool = true, right: bool = true) -> String",
        description: "Strips whitespace from the beginning and/or end.",
        kind: Method,
    },
    BuiltinMember {
        class: "String",
        name: "to_lower",
        brief: "to_lower() -> String",
        description: "Returns the string in lowercase.",
        kind: Method,
    },
    BuiltinMember {
        class: "String",
        name: "to_upper",
        brief: "to_upper() -> String",
        description: "Returns the string in uppercase.",
        kind: Method,
    },
    // ── Array ───────────────────────────────────────────────────────
    BuiltinMember {
        class: "Array",
        name: "append",
        brief: "append(value: Variant) -> void",
        description: "Appends a value at the end of the array.",
        kind: Method,
    },
    BuiltinMember {
        class: "Array",
        name: "push_back",
        brief: "push_back(value: Variant) -> void",
        description: "Appends a value at the end of the array (alias of append).",
        kind: Method,
    },
    BuiltinMember {
        class: "Array",
        name: "pop_back",
        brief: "pop_back() -> Variant",
        description: "Removes and returns the last element.",
        kind: Method,
    },
    BuiltinMember {
        class: "Array",
        name: "size",
        brief: "size() -> int",
        description: "Returns the number of elements.",
        kind: Method,
    },
    BuiltinMember {
        class: "Array",
        name: "is_empty",
        brief: "is_empty() -> bool",
        description: "Returns true if the array is empty.",
        kind: Method,
    },
    BuiltinMember {
        class: "Array",
        name: "has",
        brief: "has(value: Variant) -> bool",
        description: "Returns true if the array contains the value.",
        kind: Method,
    },
    BuiltinMember {
        class: "Array",
        name: "find",
        brief: "find(value: Variant, from: int = 0) -> int",
        description: "Returns the index of the first matching element, or -1.",
        kind: Method,
    },
    BuiltinMember {
        class: "Array",
        name: "erase",
        brief: "erase(value: Variant) -> void",
        description: "Removes the first occurrence of a value.",
        kind: Method,
    },
    BuiltinMember {
        class: "Array",
        name: "sort",
        brief: "sort() -> void",
        description: "Sorts the array in ascending order.",
        kind: Method,
    },
    BuiltinMember {
        class: "Array",
        name: "filter",
        brief: "filter(method: Callable) -> Array",
        description: "Returns a new array with elements for which method returns true.",
        kind: Method,
    },
    BuiltinMember {
        class: "Array",
        name: "map",
        brief: "map(method: Callable) -> Array",
        description: "Returns a new array with each element transformed by method.",
        kind: Method,
    },
    BuiltinMember {
        class: "Array",
        name: "reduce",
        brief: "reduce(method: Callable, accum: Variant = null) -> Variant",
        description: "Reduces the array to a single value using method.",
        kind: Method,
    },
    // ── Dictionary ──────────────────────────────────────────────────
    BuiltinMember {
        class: "Dictionary",
        name: "has",
        brief: "has(key: Variant) -> bool",
        description: "Returns true if the dictionary contains the key.",
        kind: Method,
    },
    BuiltinMember {
        class: "Dictionary",
        name: "keys",
        brief: "keys() -> Array",
        description: "Returns an array of all keys.",
        kind: Method,
    },
    BuiltinMember {
        class: "Dictionary",
        name: "values",
        brief: "values() -> Array",
        description: "Returns an array of all values.",
        kind: Method,
    },
    BuiltinMember {
        class: "Dictionary",
        name: "size",
        brief: "size() -> int",
        description: "Returns the number of key-value pairs.",
        kind: Method,
    },
    BuiltinMember {
        class: "Dictionary",
        name: "is_empty",
        brief: "is_empty() -> bool",
        description: "Returns true if the dictionary is empty.",
        kind: Method,
    },
    BuiltinMember {
        class: "Dictionary",
        name: "get",
        brief: "get(key: Variant, default: Variant = null) -> Variant",
        description: "Returns the value for the key, or default if not found.",
        kind: Method,
    },
    BuiltinMember {
        class: "Dictionary",
        name: "erase",
        brief: "erase(key: Variant) -> bool",
        description: "Removes the key-value pair. Returns true if the key existed.",
        kind: Method,
    },
    BuiltinMember {
        class: "Dictionary",
        name: "merge",
        brief: "merge(dictionary: Dictionary, overwrite: bool = false) -> void",
        description: "Merges another dictionary into this one.",
        kind: Method,
    },
];

/// Look up a built-in member by name (e.g. `global_position`, `move_and_slide`).
/// Returns the first match — most commonly used class listed first.
pub fn lookup_member(name: &str) -> Option<&'static BuiltinMember> {
    BUILTIN_MEMBER_DOCS.iter().find(|m| m.name == name)
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
}
