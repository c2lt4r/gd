use gd_core::workspace_index::ProjectIndex;
use gd_core::{gd_ast, parser};

use super::*;

fn structural_errors(source: &str) -> Vec<StructuralError> {
    let tree = parser::parse(source).unwrap();
    let file = gd_ast::convert(&tree, source);
    let project = ProjectIndex::build(std::path::Path::new("/nonexistent"));
    validate_structure(&tree.root_node(), source, &file, None, &project)
}

// -- Top-level statement checks --

#[test]
fn valid_top_level_no_errors() {
    let source = "extends Node\n\nvar x := 1\nconst Y = 2\n\nfunc _ready():\n\tpass\n";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn top_level_for_loop_is_error() {
    let source = "extends Node\n\nfor i in range(10):\n\tprint(i)\n";
    let errs = structural_errors(source);
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message.contains("top level"));
}

#[test]
fn top_level_expression_is_error() {
    let source = "extends Node\n\nprint(\"hello\")\n";
    let errs = structural_errors(source);
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message.contains("top level"));
}

#[test]
fn top_level_if_is_error() {
    let source = "extends Node\n\nif true:\n\tpass\n";
    let errs = structural_errors(source);
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message.contains("top level"));
}

#[test]
fn top_level_return_is_error() {
    let source = "return 42\n";
    let errs = structural_errors(source);
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message.contains("top level"));
}

// -- Indentation consistency checks --

#[test]
fn consistent_indentation_no_errors() {
    let source = "func f(x: int) -> int:\n\tif x > 0:\n\t\treturn x\n\telse:\n\t\treturn -x\n";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn orphaned_block_after_return_detected() {
    // Simulates removing else: but leaving body indented too deep
    let source = "func f(m: int) -> int:\n\tmatch m:\n\t\t0:\n\t\t\tif m == 1:\n\t\t\t\treturn 1\n\t\t\t# comment\n\t\t\t\tvar q := 2\n\t\t\t\treturn q\n\t\t_:\n\t\t\treturn 0\n";
    let errs = structural_errors(source);
    assert!(!errs.is_empty(), "should detect orphaned indented block");
    assert!(errs[0].message.contains("indentation"));
}

#[test]
fn dedented_body_code_at_top_level_detected() {
    // Function body code accidentally at column 0
    let source = "extends Node\n\nvar items: Array = []\n\nfor i in range(10):\n\titems.append(i)\n\nfunc _ready():\n\tpass\n";
    let errs = structural_errors(source);
    assert!(!errs.is_empty());
}

#[test]
fn multiline_expression_not_false_positive() {
    // Continuation lines inside a single statement node are fine
    let source = "func f() -> Quaternion:\n\tvar result := Quaternion(\n\t\t1.0,\n\t\t2.0,\n\t\t3.0,\n\t\t4.0\n\t).normalized()\n\treturn result\n";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn multiline_function_call_not_false_positive() {
    let source = "func f() -> void:\n\tsome_function(\n\t\targ1,\n\t\targ2,\n\t\targ3\n\t)\n\tprint(\"done\")\n";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn multiline_array_not_false_positive() {
    let source = "func f() -> Array:\n\tvar arr := [\n\t\t1,\n\t\t2,\n\t\t3,\n\t]\n\treturn arr\n";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn multiline_dict_not_false_positive() {
    let source =
        "func f() -> Dictionary:\n\tvar d := {\n\t\t\"a\": 1,\n\t\t\"b\": 2,\n\t}\n\treturn d\n";
    assert!(structural_errors(source).is_empty());
}

// -- Class constant validation checks --

#[test]
fn valid_class_constant_no_error() {
    let source = "func f():\n\tvar mode := Environment.TONE_MAPPER_LINEAR\n";
    let errs = structural_errors(source);
    assert!(
        errs.is_empty(),
        "valid constant should not produce errors, got: {:?}",
        errs.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn invalid_class_constant_detected() {
    let source = "func f():\n\tvar mode := Environment.TONE_MAP_ACES\n";
    let errs = structural_errors(source);
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message.contains("unknown constant"));
}

#[test]
fn user_class_not_validated() {
    // Only Godot built-in classes should be validated
    let source = "func f():\n\tvar x := MyClass.SOME_CONST\n";
    let errs = structural_errors(source);
    assert!(errs.is_empty());
}

// -- Variant inference checks --

#[test]
fn variant_infer_from_subscript() {
    // Subscript on explicitly-typed Dictionary flags as Variant
    let source = "var dict: Dictionary\nfunc f():\n\tvar x := dict[\"key\"]\n";
    let errs = structural_errors(source);
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message.contains("Variant"));
}

#[test]
fn no_variant_for_unresolved_subscript() {
    // When we can't determine the receiver type, don't flag — user likely knows
    let source = "func f(data):\n\tvar x := data[\"key\"]\n";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn variant_for_dict_subscript() {
    // Dictionary subscript produces Variant — flag it
    let source = "var dict := {}\nfunc f():\n\tvar x := dict[\"key\"]\n";
    let errs = structural_errors(source);
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message.contains("Variant"));
}

#[test]
fn variant_infer_from_dict_get() {
    let source = "var dict := {}\nfunc f():\n\tvar x := dict.get(\"key\")\n";
    let errs = structural_errors(source);
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message.contains("Variant"));
}

#[test]
fn no_variant_warning_with_explicit_type() {
    let source = "var dict := {}\nfunc f():\n\tvar x: String = dict[\"key\"]\n";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn no_variant_warning_simple_infer() {
    let source = "func f():\n\tvar x := 42\n";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn variant_infer_from_max() {
    let source = "func f():\n\tvar x := max(1, 2)\n";
    let errs = structural_errors(source);
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message.contains("Variant"));
}

#[test]
fn variant_infer_from_min() {
    let source = "func f():\n\tvar x := min(1, 2)\n";
    let errs = structural_errors(source);
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message.contains("Variant"));
}

#[test]
fn variant_infer_from_clamp() {
    let source = "func f():\n\tvar x := clamp(5, 1, 10)\n";
    let errs = structural_errors(source);
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message.contains("Variant"));
}

#[test]
fn no_variant_from_maxi() {
    let source = "func f():\n\tvar x := maxi(1, 2)\n";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn no_variant_from_maxf() {
    let source = "func f():\n\tvar x := maxf(1.0, 2.0)\n";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn enum_type_as_cast_not_flagged() {
    let source = "func f(index: int):\n\tvar msaa := index as Viewport.MSAA\n";
    let errs = structural_errors(source);
    assert!(
        errs.iter().all(|e| !e.message.contains("unknown constant")),
        "enum type name used for casting should not be flagged: {:?}",
        errs.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn region_markers_valid_at_top_level() {
    let source =
        "extends Node\n\n#region Signals\nsignal foo\n#endregion\n\nfunc _ready():\n\tpass\n";
    assert!(structural_errors(source).is_empty());
}

// -- `in` / `not in` variant inference --

#[test]
fn variant_infer_from_in_operator() {
    let source = "\
var ACTIONS := [\"move_left\", \"move_right\"]
func f(action: String):
\tvar is_move := action in ACTIONS
";
    let errs = structural_errors(source);
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message.contains("Variant"));
    assert!(errs[0].message.contains("is_move"));
}

#[test]
fn variant_infer_from_not_in() {
    let source = "\
var ACTIONS := [\"move_left\", \"move_right\"]
func f(action: String):
\tvar missing := action not in ACTIONS
";
    let errs = structural_errors(source);
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message.contains("Variant"));
    assert!(errs[0].message.contains("missing"));
}

#[test]
fn no_variant_from_in_with_explicit_type() {
    let source = "\
var ACTIONS := [\"move_left\", \"move_right\"]
func f(action: String):
\tvar is_move: bool = action in ACTIONS
";
    assert!(structural_errors(source).is_empty());
}

// -- Unresolvable property access variant inference --

#[test]
fn variant_infer_from_base_class_property() {
    let source = "\
func handle(event: InputEvent):
\tvar keycode := event.physical_keycode
";
    let errs = structural_errors(source);
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message.contains("Variant"));
    assert!(errs[0].message.contains("keycode"));
}

#[test]
fn no_variant_self_property() {
    let source = "\
var speed := 10.0
func f():
\tvar s := self.speed
";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn no_variant_explicit_type_on_property() {
    let source = "\
func handle(event: InputEvent):
\tvar keycode: int = event.physical_keycode
";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn no_variant_from_known_type_property() {
    // Vector2.x is a known float — should not be flagged
    let source = "\
func f(pos: Vector2):
\tvar x := pos.x
";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn no_variant_from_method_call() {
    // Method calls should not trigger the property access check
    let source = "\
func f(node: Node):
\tvar name := node.get_name()
";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn no_variant_from_class_constant() {
    let source = "\
func f():
\tvar zero := Vector2.ZERO
";
    assert!(structural_errors(source).is_empty());
}

// -- load().instantiate() variant inference --

#[test]
fn variant_infer_from_load_instantiate() {
    let source = "\
func f():
\tvar popup := load(\"res://popup.tscn\").instantiate()
";
    let errs = structural_errors(source);
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message.contains("Variant"));
    assert!(errs[0].message.contains("popup"));
}

#[test]
fn no_variant_from_preload_instantiate() {
    let source = "\
func f():
\tvar popup := preload(\"res://popup.tscn\").instantiate()
";
    assert!(structural_errors(source).is_empty());
}

// -- ClassDB Variant-return method inference --

#[test]
fn variant_infer_from_classdb_variant_method() {
    let source = "\
func f(node: Node):
\tvar meta := node.get_meta(\"key\")
";
    let errs = structural_errors(source);
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message.contains("Variant"));
    assert!(errs[0].message.contains("meta"));
}

#[test]
fn no_variant_from_classdb_concrete_method() {
    let source = "\
func f(node: Node):
\tvar child := node.get_child(0)
";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn no_variant_from_untyped_receiver_method() {
    let source = "\
func f(node):
\tvar meta := node.get_meta(\"key\")
";
    assert!(structural_errors(source).is_empty());
}

// -- Type narrowing after `is` checks --

#[test]
fn no_variant_after_direct_is_guard() {
    let source = "\
func f(event: InputEvent):
\tif event is InputEventKey:
\t\tvar k := event.keycode
";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn no_variant_after_early_exit_is_guard() {
    let source = "\
func f(event: InputEvent):
\tif not event is InputEventKey:
\t\treturn
\tvar k := event.keycode
";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn no_variant_after_early_exit_continue() {
    let source = "\
func f(events: Array):
\tfor event in events:
\t\tif not event is InputEventKey:
\t\t\tcontinue
\t\tvar k := event.keycode
";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn variant_still_flagged_without_is_guard() {
    let source = "\
func f(event: InputEvent):
\tvar k := event.keycode
";
    let errs = structural_errors(source);
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message.contains("Variant"));
}

// -- := initializer type inference --

#[test]
fn infer_constructor_new() {
    let source = "\
func f():
\tvar target := Node3D.new()
\tvar d := target.position
";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn infer_constructor_call() {
    let source = "\
func f():
\tvar v := Vector2(1, 2)
\tvar x := v.x
";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn infer_same_file_function_return() {
    let source = "\
func _find_node() -> Node3D:
\treturn null
func f():
\tvar target := _find_node()
\tvar d := target.position
";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn infer_cast_as_type() {
    let source = "\
func f(obj):
\tvar node := obj as Node3D
\tvar d := node.position
";
    assert!(structural_errors(source).is_empty());
}

// -- Scene validation --

#[test]
fn extract_ext_resource_id_basic() {
    assert_eq!(
        super::extract_ext_resource_id(r#"ExtResource("1_abc")"#),
        Some("1_abc")
    );
}

#[test]
fn extract_ext_resource_id_none() {
    assert_eq!(super::extract_ext_resource_id("not_a_reference"), None);
}

#[test]
fn validate_scene_orphaned_ext_resource() {
    let source = r#"[gd_scene format=3]

[ext_resource type="Texture2D" path="res://icon.png" id="unused_1"]

[node name="Root" type="Node2D"]
"#;
    let data = gd_core::scene::parse_scene(source).unwrap();
    let root = std::path::Path::new("/nonexistent");
    let cwd = std::path::Path::new("/cwd");
    let file = std::path::Path::new("/cwd/test.tscn");
    let errors = super::validate_scene(&data, root, file, cwd);
    assert!(
        errors.iter().any(|e| e.message.contains("orphaned")),
        "should detect orphaned ext_resource"
    );
}

// ====================================================================
// Batch 2: Declaration constraint checks
// ====================================================================

// -- G4: _init cannot have non-void return type --

#[test]
fn init_with_non_void_return_type() {
    let source = "func _init() -> int:\n\tpass\n";
    let errs = structural_errors(source);
    assert!(errs.iter().any(|e| e.message.contains("_init")));
}

#[test]
fn init_with_void_return_type_ok() {
    let source = "func _init() -> void:\n\tpass\n";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn init_without_return_type_ok() {
    let source = "func _init():\n\tpass\n";
    assert!(structural_errors(source).is_empty());
}

// -- G3: Mandatory parameter after optional --

#[test]
fn mandatory_after_optional() {
    let source = "func f(a: int = 1, b: int):\n\tpass\n";
    let errs = structural_errors(source);
    assert!(
        errs.iter()
            .any(|e| e.message.contains("required parameter"))
    );
}

#[test]
fn all_optional_ok() {
    let source = "func f(a: int = 1, b: int = 2):\n\tpass\n";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn all_mandatory_ok() {
    let source = "func f(a: int, b: int):\n\tpass\n";
    assert!(structural_errors(source).is_empty());
}

// -- G2: Signal params cannot have defaults --

#[test]
fn signal_with_default_param() {
    let source = "signal my_signal(a: int = 5)\n";
    let errs = structural_errors(source);
    assert!(errs.iter().any(|e| e.message.contains("signal parameter")));
}

#[test]
fn signal_without_default_ok() {
    let source = "signal my_signal(a: int)\n";
    assert!(structural_errors(source).is_empty());
}

// -- G6: Duplicate class_name / extends --

#[test]
fn duplicate_class_name() {
    let source = "class_name Foo\nclass_name Bar\n";
    let errs = structural_errors(source);
    assert!(
        errs.iter()
            .any(|e| e.message.contains("duplicate `class_name`"))
    );
}

#[test]
fn duplicate_extends() {
    let source = "extends Node\nextends Node2D\n";
    let errs = structural_errors(source);
    assert!(
        errs.iter()
            .any(|e| e.message.contains("duplicate `extends`"))
    );
}

#[test]
fn single_class_name_ok() {
    let source = "class_name Foo\nextends Node\n";
    assert!(structural_errors(source).is_empty());
}

// -- G7: Duplicate parameter names --

#[test]
fn duplicate_param_name() {
    let source = "func f(a: int, a: int):\n\tpass\n";
    let errs = structural_errors(source);
    assert!(
        errs.iter()
            .any(|e| e.message.contains("duplicate parameter"))
    );
}

#[test]
fn unique_params_ok() {
    let source = "func f(a: int, b: int):\n\tpass\n";
    assert!(structural_errors(source).is_empty());
}

// -- G1: yield keyword --

#[test]
fn yield_keyword_detected() {
    let source = "func f():\n\tyield(get_tree(), \"idle_frame\")\n";
    let errs = structural_errors(source);
    assert!(errs.iter().any(|e| e.message.contains("yield")));
}

// -- H7: _static_init cannot have params --

#[test]
fn static_init_with_params() {
    let source = "static func _static_init(x: int):\n\tpass\n";
    let errs = structural_errors(source);
    assert!(errs.iter().any(|e| e.message.contains("_static_init")));
}

#[test]
fn static_init_no_params_ok() {
    let source = "static func _static_init():\n\tpass\n";
    assert!(structural_errors(source).is_empty());
}

// -- E8: Duplicate @tool --

#[test]
fn duplicate_tool_annotation() {
    let source = "@tool\n@tool\nextends Node\n";
    let errs = structural_errors(source);
    assert!(errs.iter().any(|e| e.message.contains("duplicate `@tool`")));
}

#[test]
fn single_tool_ok() {
    let source = "@tool\nextends Node\n";
    assert!(structural_errors(source).is_empty());
}

// ====================================================================
// Batch 3: Semantic checks
// ====================================================================

// -- C1: Static context violations --

#[test]
fn static_func_uses_self() {
    let source = "\
extends Node
static func foo():
\tprint(self)
";
    let errs = structural_errors(source);
    assert!(
        errs.iter()
            .any(|e| e.message.contains("self") && e.message.contains("static"))
    );
}

#[test]
fn static_func_accesses_instance_var() {
    let source = "\
extends Node
var health := 100
static func foo():
\tprint(health)
";
    let errs = structural_errors(source);
    assert!(
        errs.iter()
            .any(|e| e.message.contains("health") && e.message.contains("static"))
    );
}

#[test]
fn static_func_calls_instance_method() {
    let source = "\
extends Node
func bar():
\tpass
static func foo():
\tbar()
";
    let errs = structural_errors(source);
    assert!(
        errs.iter()
            .any(|e| e.message.contains("bar") && e.message.contains("static"))
    );
}

#[test]
fn non_static_func_uses_self_ok() {
    let source = "\
extends Node
func foo():
\tprint(self)
";
    assert!(structural_errors(source).is_empty());
}

// -- C2: Assign to constant --

#[test]
fn assign_to_constant() {
    let source = "\
const MAX := 100
func f():
\tMAX = 200
";
    let errs = structural_errors(source);
    assert!(
        errs.iter()
            .any(|e| e.message.contains("constant") && e.message.contains("MAX"))
    );
}

#[test]
fn assign_to_enum_member() {
    let source = "\
enum State { IDLE, RUNNING }
func f():
\tIDLE = 5
";
    let errs = structural_errors(source);
    assert!(
        errs.iter()
            .any(|e| e.message.contains("enum value") && e.message.contains("IDLE"))
    );
}

#[test]
fn assign_to_var_ok() {
    let source = "\
var x := 100
func f():
\tx = 200
";
    assert!(structural_errors(source).is_empty());
}

// -- C3: Void function returns value --

#[test]
fn void_func_returns_value() {
    let source = "func f() -> void:\n\treturn 42\n";
    let errs = structural_errors(source);
    assert!(
        errs.iter()
            .any(|e| e.message.contains("void") && e.message.contains("return"))
    );
}

#[test]
fn void_func_bare_return_ok() {
    let source = "func f() -> void:\n\treturn\n";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn typed_func_returns_value_ok() {
    let source = "func f() -> int:\n\treturn 42\n";
    assert!(structural_errors(source).is_empty());
}

// -- H14: get_node in static --

#[test]
fn get_node_in_static_func() {
    let source = "static func f():\n\tvar x = $Sprite2D\n";
    let errs = structural_errors(source);
    assert!(
        errs.iter()
            .any(|e| e.message.contains("get_node") && e.message.contains("static"))
    );
}

#[test]
fn get_node_in_non_static_ok() {
    let source = "func f():\n\tvar x = $Sprite2D\n";
    assert!(structural_errors(source).is_empty());
}

// -- E1: @export without type or initializer --

#[test]
fn export_without_type_or_default() {
    let source = "@export var x\n";
    let errs = structural_errors(source);
    assert!(
        errs.iter()
            .any(|e| e.message.contains("export") && e.message.contains("no type"))
    );
}

#[test]
fn export_with_type_ok() {
    let source = "@export var x: int\n";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn export_with_default_ok() {
    let source = "@export var x = 10\n";
    assert!(structural_errors(source).is_empty());
}

// -- E3: @export on static --

#[test]
fn export_on_static() {
    let source = "@export\nstatic var x: int = 0\n";
    let errs = structural_errors(source);
    assert!(
        errs.iter()
            .any(|e| e.message.contains("export") && e.message.contains("static"))
    );
}

// -- E4: Duplicate @export --

#[test]
fn duplicate_export() {
    let source = "@export\n@export\nvar x: int = 0\n";
    let errs = structural_errors(source);
    assert!(
        errs.iter()
            .any(|e| e.message.contains("duplicate") && e.message.contains("export"))
    );
}

// -- H17: Object() constructor --

#[test]
fn object_direct_constructor() {
    let source = "func f():\n\tvar o = Object()\n";
    let errs = structural_errors(source);
    assert!(
        errs.iter()
            .any(|e| e.message.contains("Object()") && e.message.contains("Object.new()"))
    );
}

#[test]
fn object_new_ok() {
    let source = "func f():\n\tvar o = Object.new()\n";
    assert!(structural_errors(source).is_empty());
}

// ====================================================================
// Batch 4: Preload & misc checks
// ====================================================================

// -- F2: preload() argument not a constant string --

#[test]
fn preload_non_string_arg() {
    let source = "func f():\n\tvar path = \"res://foo.gd\"\n\tvar x = preload(path)\n";
    let errs = structural_errors(source);
    assert!(
        errs.iter()
            .any(|e| e.message.contains("preload") && e.message.contains("constant string"))
    );
}

#[test]
fn preload_string_arg_ok() {
    let source = "func f():\n\tvar x = preload(\"res://foo.gd\")\n";
    assert!(structural_errors(source).is_empty());
}

// -- H15: range() too many arguments --

#[test]
fn range_too_many_args() {
    let source = "func f():\n\tfor i in range(1, 2, 3, 4):\n\t\tpass\n";
    let errs = structural_errors(source);
    assert!(
        errs.iter()
            .any(|e| e.message.contains("range") && e.message.contains("at most 3"))
    );
}

#[test]
fn range_three_args_ok() {
    let source = "func f():\n\tfor i in range(0, 10, 2):\n\t\tpass\n";
    assert!(structural_errors(source).is_empty());
}

// ====================================================================
// Batch 5: ClassDB checks
// ====================================================================

fn classdb_errors(source: &str) -> Vec<StructuralError> {
    let tree = parser::parse(source).unwrap();
    let file = gd_ast::convert(&tree, source);
    let project = ProjectIndex::build(std::path::Path::new("/nonexistent"));
    check_classdb_errors(&file, source, &project)
}

// -- H5: class_name shadows native class --

#[test]
fn class_name_shadows_native() {
    let source = "class_name Node\n";
    let errs = classdb_errors(source);
    assert!(errs.iter().any(|e| e.message.contains("shadows")));
}

#[test]
fn class_name_custom_ok() {
    let source = "class_name MyPlayer\n";
    assert!(classdb_errors(source).is_empty());
}

// -- G5: Enum shadows builtin type --

#[test]
fn enum_shadows_builtin() {
    let source = "enum int { A, B, C }\n";
    let errs = classdb_errors(source);
    assert!(
        errs.iter()
            .any(|e| e.message.contains("shadows") && e.message.contains("int"))
    );
}

#[test]
fn enum_custom_name_ok() {
    let source = "enum MyState { A, B, C }\n";
    assert!(classdb_errors(source).is_empty());
}

// -- A4: Unknown type in annotation --

#[test]
fn unknown_type_annotation() {
    let source = "var x: NonExistentType\n";
    let errs = classdb_errors(source);
    assert!(errs.iter().any(|e| e.message.contains("unknown type")));
}

#[test]
fn known_type_annotation_ok() {
    let source = "var x: int\n";
    assert!(classdb_errors(source).is_empty());
}

#[test]
fn classdb_type_annotation_ok() {
    let source = "var x: Node2D\n";
    assert!(classdb_errors(source).is_empty());
}

#[test]
fn same_file_enum_type_ok() {
    let source = "enum State { A, B }\nvar x: State\n";
    assert!(classdb_errors(source).is_empty());
}

#[test]
fn h16_callable_direct_call() {
    let source = "extends Node\nfunc _ready():\n\tvar f: Callable = func(): pass\n\tf()\n";
    let errs = classdb_errors(source);
    assert!(
        !errs.is_empty(),
        "expected callable direct call error, got none"
    );
    assert!(
        errs[0].message.contains("not found"),
        "msg: {}",
        errs[0].message
    );
}

#[test]
fn b4_too_few_user_func() {
    let source = "extends Node\nfunc my_func(a: int, b: int, c: int) -> int:\n\treturn a + b + c\nfunc _ready():\n\tmy_func(1)\n";
    let errs = classdb_errors(source);
    assert!(errs.iter().any(|e| e.message.contains("too few")));
}

#[test]
fn b4_too_many_user_func() {
    let source = "extends Node\nfunc my_func(a: int) -> int:\n\treturn a\nfunc _ready():\n\tmy_func(1, 2, 3)\n";
    let errs = classdb_errors(source);
    assert!(errs.iter().any(|e| e.message.contains("too many")));
}

#[test]
fn b4_too_few_builtin() {
    let source = "extends Node\nfunc _ready():\n\tlerp(1.0, 2.0)\n";
    let errs = classdb_errors(source);
    assert!(errs.iter().any(|e| e.message.contains("too few")));
}

#[test]
fn b5_bool_multiply() {
    let source = "extends Node\nfunc _ready():\n\tvar x = true * false\n";
    let errs = classdb_errors(source);
    assert!(
        errs.iter().any(|e| e.message.contains("invalid operands")),
        "expected operator error, got: {errs:?}"
    );
}

#[test]
fn b5_array_minus_int() {
    let source = "extends Node\nfunc _ready():\n\tvar x = [] - 5\n";
    let errs = classdb_errors(source);
    assert!(
        errs.iter().any(|e| e.message.contains("invalid operands")),
        "expected operator error, got: {errs:?}"
    );
}

#[test]
fn b1_assign_type_mismatch() {
    let source = "extends Node\nvar health: int = \"hello\"\n";
    let errs = classdb_errors(source);
    assert!(
        errs.iter().any(|e| e.message.contains("cannot assign")),
        "expected type mismatch, got: {errs:?}"
    );
}

#[test]
fn b2_return_type_mismatch() {
    let source = "extends Node\nfunc f() -> int:\n\treturn \"hello\"\n";
    let errs = classdb_errors(source);
    assert!(
        errs.iter().any(|e| e.message.contains("cannot return")),
        "expected return type error, got: {errs:?}"
    );
}

#[test]
fn b6_invalid_cast_int_to_node() {
    let source = "extends Node\nfunc _ready():\n\tvar x: int = 42\n\tvar n := x as Node\n";
    let errs = classdb_errors(source);
    assert!(
        errs.iter().any(|e| e.message.contains("invalid cast")),
        "expected cast error, got: {errs:?}"
    );
}

#[test]
fn b1_reassign_local_wrong_type() {
    let source = "extends Node\nfunc _ready():\n\tvar x: int = 10\n\tx = \"now a string\"\n";
    let errs = classdb_errors(source);
    assert!(
        errs.iter().any(|e| e.message.contains("cannot assign")),
        "expected type mismatch, got: {errs:?}"
    );
}

#[test]
fn b2_return_node_as_string() {
    let source = "extends Node\nfunc get_name_str() -> String:\n\treturn Node.new()\n";
    let errs = classdb_errors(source);
    assert!(
        errs.iter().any(|e| e.message.contains("cannot return")),
        "expected return type error, got: {errs:?}"
    );
}

// ====================================================================
// Corpus fix tests
// ====================================================================

// -- Indentation: nested control flow should not be flagged --

#[test]
fn nested_if_elif_not_false_positive() {
    let source = "\
func f(x: int) -> int:
\tif x > 10:
\t\treturn 10
\telif x > 5:
\t\treturn 5
\telse:
\t\treturn 0
";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn nested_match_arms_not_false_positive() {
    let source = "\
func f(x: int) -> void:
\tmatch x:
\t\t0:
\t\t\tprint(\"zero\")
\t\t1:
\t\t\tprint(\"one\")
\t\t_:
\t\t\tprint(\"other\")
";
    let errs = structural_errors(source);
    let indent_errs: Vec<_> = errs
        .iter()
        .filter(|e| e.message.contains("indentation"))
        .collect();
    assert!(
        indent_errs.is_empty(),
        "unexpected indentation errors: {:?}",
        indent_errs
            .iter()
            .map(|e| format!("L{}: {}", e.line, &e.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn deeply_nested_if_in_match_not_false_positive() {
    let source = "\
func f(x: int) -> void:
\tmatch x:
\t\t0:
\t\t\tif true:
\t\t\t\tprint(\"a\")
\t\t\telse:
\t\t\t\tprint(\"b\")
\t\t_:
\t\t\tprint(\"c\")
";
    let errs = structural_errors(source);
    let indent_errs: Vec<_> = errs
        .iter()
        .filter(|e| e.message.contains("indentation"))
        .collect();
    assert!(
        indent_errs.is_empty(),
        "unexpected indentation errors: {:?}",
        indent_errs
            .iter()
            .map(|e| format!("L{}: {}", e.line, &e.message))
            .collect::<Vec<_>>()
    );
}

// -- Void return value usage as argument --

#[test]
fn void_func_as_argument() {
    let source = "\
extends Node
func do_nothing() -> void:
\tpass
func _ready():
\tprint(do_nothing())
";
    let errs = classdb_errors(source);
    assert!(
        errs.iter()
            .any(|e| e.message.contains("returns void") && e.message.contains("argument")),
        "expected void-as-argument error, got: {errs:?}"
    );
}

#[test]
fn void_utility_as_value() {
    let source = "\
extends Node
func _ready():
\tvar x = print(\"hello\")
";
    let errs = classdb_errors(source);
    assert!(
        errs.iter().any(|e| e.message.contains("returns void")),
        "expected void return error, got: {errs:?}"
    );
}

// -- Invalid cast: primitive → container, class → primitive --

#[test]
fn b6_invalid_cast_int_to_array() {
    let source = "extends Node\nfunc _ready():\n\tvar x: int = 42\n\tvar a := x as Array\n";
    let errs = classdb_errors(source);
    assert!(
        errs.iter().any(|e| e.message.contains("invalid cast")),
        "expected cast error for int→Array, got: {errs:?}"
    );
}

#[test]
fn b6_invalid_cast_node_to_int() {
    let source = "extends Node\nfunc _ready():\n\tvar n := Node.new()\n\tvar x := n as int\n";
    let errs = classdb_errors(source);
    assert!(
        errs.iter().any(|e| e.message.contains("invalid cast")),
        "expected cast error for Node→int, got: {errs:?}"
    );
}

// -- Const assignment: subscript and signal --

#[test]
fn assign_to_const_subscript() {
    let source = "\
const ARR := [1, 2, 3]
func f():
\tARR[0] = 99
";
    let errs = structural_errors(source);
    assert!(
        errs.iter()
            .any(|e| e.message.contains("constant") && e.message.contains("ARR")),
        "expected const subscript error, got: {errs:?}"
    );
}

#[test]
fn assign_to_signal() {
    let source = "\
signal my_signal
func f():
\tmy_signal = null
";
    let errs = structural_errors(source);
    assert!(
        errs.iter()
            .any(|e| e.message.contains("signal") && e.message.contains("my_signal")),
        "expected signal assign error, got: {errs:?}"
    );
}

// -- Static context: static var initializer --

#[test]
fn static_var_uses_instance_var() {
    let source = "\
extends Node
var health := 100
static var cached = health
";
    let errs = structural_errors(source);
    assert!(
        errs.iter()
            .any(|e| e.message.contains("health") && e.message.contains("static")),
        "expected static context error, got: {errs:?}"
    );
}

#[test]
fn static_var_const_init_ok() {
    let source = "\
extends Node
static var count = 0
";
    assert!(structural_errors(source).is_empty());
}

// -- Typed array check on const_statement --

#[test]
fn const_typed_array_wrong_element() {
    let source = "extends Node\nconst ARR: Array[int] = [\"hello\"]\n";
    let errs = classdb_errors(source);
    assert!(
        errs.iter().any(|e| e.message.contains("Array[int]")),
        "expected typed array error on const, got: {errs:?}"
    );
}

// -- Augmented assignment type check --

#[test]
fn augmented_assign_bool_plus_string() {
    let source = "extends Node\nfunc _ready():\n\tvar x: bool = true\n\tx += \"hello\"\n";
    let errs = classdb_errors(source);
    assert!(
        errs.iter().any(|e| e.message.contains("invalid operands")),
        "expected operator error on +=, got: {errs:?}"
    );
}

// -- Return void_func() in void function is OK --

#[test]
fn void_return_void_call_ok() {
    let source = "\
func helper() -> void:
\tpass
func f() -> void:
\treturn helper()
";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn void_return_value_still_flagged() {
    let source = "func f() -> void:\n\treturn 42\n";
    let errs = structural_errors(source);
    assert!(
        errs.iter()
            .any(|e| e.message.contains("void") && e.message.contains("return")),
    );
}

// -- Const expression: Type.new() --

#[test]
fn const_with_type_new_ok() {
    let source = "const OBJ = RefCounted.new()\n";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn const_with_constructor_call_ok() {
    let source = "const V = Vector2(1, 2)\n";
    assert!(structural_errors(source).is_empty());
}

#[test]
fn const_array_with_constructors_ok() {
    let source = "const POINTS = [Vector2(0, 0), Vector2(1, 1)]\n";
    assert!(structural_errors(source).is_empty());
}
