use crate::core::parser;
use crate::fmt::printer::Printer;

fn format_source(source: &str) -> String {
    let tree = parser::parse(source).unwrap();
    let mut printer = Printer::new(true, 4);
    printer.format(&tree.root_node(), source);
    printer.finish()
}

#[test]
fn test_basic_function() {
    let input = "func hello() -> void:\n\tpass\n";
    let output = format_source(input);
    assert_eq!(output, "func hello() -> void:\n\tpass\n");
}

#[test]
fn test_string_preserved() {
    let input = "func f():\n\tprint(\"hello world\")\n";
    let output = format_source(input);
    assert!(output.contains("\"hello world\""), "got: {output}");
}

#[test]
fn test_variable_with_annotation() {
    let input = "@export var health: int = 100\n";
    let output = format_source(input);
    assert_eq!(output, "@export var health: int = 100\n");
}

#[test]
fn test_dictionary() {
    let input = "func f():\n\tvar d = {\"key\": \"value\", \"k2\": \"v2\"}\n";
    let output = format_source(input);
    assert!(
        output.contains("{\"key\": \"value\", \"k2\": \"v2\"}"),
        "got: {output}"
    );
}

#[test]
fn test_binary_operator_spacing() {
    let input = "func f():\n\tvar x = 1 + 2 * 3\n";
    let output = format_source(input);
    assert!(output.contains("1 + 2 * 3"), "got: {output}");
}

#[test]
fn test_trailing_whitespace_removed() {
    let input = "func hello() -> void:\n\tpass\n";
    let output = format_source(input);
    assert!(!output.lines().any(|line| line.ends_with(' ')));
}

#[test]
fn test_single_trailing_newline() {
    let input = "func hello() -> void:\n\tpass\n\n\n";
    let output = format_source(input);
    assert!(output.ends_with('\n'));
    assert!(!output.ends_with("\n\n"));
}

#[test]
fn test_two_blank_lines_between_functions() {
    let input = "func a():\n\tpass\nfunc b():\n\tpass\n";
    let output = format_source(input);
    assert!(output.contains("pass\n\n\nfunc b"), "got: {output}");
}

#[test]
fn test_if_elif_else() {
    let input = "func f():\n\tif x > 0:\n\t\tpass\n\telif x < 0:\n\t\tpass\n\telse:\n\t\tpass\n";
    let output = format_source(input);
    assert!(output.contains("if x > 0:"), "got: {output}");
    assert!(output.contains("\telif x < 0:"), "got: {output}");
    assert!(output.contains("\telse:"), "got: {output}");
}

#[test]
fn test_enum() {
    let input = "enum State { IDLE, RUNNING, JUMPING }\n";
    let output = format_source(input);
    assert_eq!(output, "enum State { IDLE, RUNNING, JUMPING }\n");
}

#[test]
fn test_for_loop() {
    let input = "func f():\n\tfor i in range(10):\n\t\tprint(i)\n";
    let output = format_source(input);
    assert!(output.contains("for i in range(10):"), "got: {output}");
}

#[test]
fn test_get_node_preserved() {
    let input = "@onready var sprite: Sprite2D = $Sprite2D\n";
    let output = format_source(input);
    assert!(output.contains("$Sprite2D"), "got: {output}");
}

// ── Edge case tests ───────────────────────────────────────────────

#[test]
fn test_onready_annotation_on_same_line() {
    let input = "@onready var sprite: Sprite2D = $Sprite2D\n";
    let output = format_source(input);
    assert_eq!(output, "@onready var sprite: Sprite2D = $Sprite2D\n");
}

#[test]
fn test_multi_annotation_var() {
    let input = "@export @onready var sprite: Sprite2D = $Sprite2D\n";
    let output = format_source(input);
    assert!(
        output.contains("@export\n@onready var sprite"),
        "got: {output}"
    );
}

#[test]
fn test_annotation_on_function() {
    let input = "@rpc(\"any_peer\") func sync():\n\tpass\n";
    let output = format_source(input);
    assert!(
        output.contains("@rpc(\"any_peer\") func sync()"),
        "got: {output}"
    );
}

#[test]
fn test_tool_annotation_no_blank_line() {
    let input = "@tool\nextends Node2D\n";
    let output = format_source(input);
    assert_eq!(output, "@tool\nextends Node2D\n");
}

#[test]
fn test_await_expression() {
    let input = "func f():\n\tawait get_tree().create_timer(1.0).timeout\n";
    let output = format_source(input);
    assert!(
        output.contains("await get_tree().create_timer(1.0).timeout"),
        "got: {output}"
    );
}

#[test]
fn test_as_cast() {
    let input = "func f():\n\tvar node = get_node(\"path\") as Node2D\n";
    let output = format_source(input);
    assert!(
        output.contains("get_node(\"path\") as Node2D"),
        "got: {output}"
    );
}

#[test]
fn test_is_type_check() {
    let input = "func f():\n\tif enemy is Boss:\n\t\tpass\n";
    let output = format_source(input);
    assert!(output.contains("if enemy is Boss:"), "got: {output}");
}

#[test]
fn test_not_keyword() {
    let input = "func f():\n\tif not ready:\n\t\tpass\n";
    let output = format_source(input);
    assert!(output.contains("if not ready:"), "got: {output}");
}

#[test]
fn test_preload_call() {
    let input = "func f():\n\tvar s = preload(\"res://scene.tscn\")\n";
    let output = format_source(input);
    assert!(
        output.contains("preload(\"res://scene.tscn\")"),
        "got: {output}"
    );
}

#[test]
fn test_typed_array() {
    let input = "var arr: Array[int] = []\n";
    let output = format_source(input);
    assert!(output.contains("arr: Array[int]"), "got: {output}");
}

#[test]
fn test_inferred_type() {
    let input = "func f():\n\tvar x := 42\n";
    let output = format_source(input);
    assert!(output.contains("var x := 42"), "got: {output}");
}

#[test]
fn test_idempotency_basic() {
    let input = "extends Node2D\n\nvar health: int = 100\n\n\nfunc _ready():\n\tpass\n";
    let first = format_source(input);
    let second = format_source(&first);
    assert_eq!(
        first, second,
        "Format is not idempotent!\nFirst:\n{first}\nSecond:\n{second}"
    );
}

#[test]
fn test_idempotency_annotations() {
    let input = "@export var health: int = 100\n@onready var sprite: Sprite2D = $Sprite2D\n";
    let first = format_source(input);
    let second = format_source(&first);
    assert_eq!(
        first, second,
        "Format is not idempotent!\nFirst:\n{first}\nSecond:\n{second}"
    );
}

#[test]
fn test_idempotency_tool() {
    let input = "@tool\nextends Node2D\n\nvar x: int = 0\n";
    let first = format_source(input);
    let second = format_source(&first);
    assert_eq!(
        first, second,
        "Format is not idempotent!\nFirst:\n{first}\nSecond:\n{second}"
    );
}

#[test]
fn test_annotation_var_grouping() {
    let input = "@export var health: int = 100\n@export var mana: int = 50\n@onready var sprite = $Sprite2D\n\nvar speed = 200\n";
    let output = format_source(input);
    // No blank lines between annotated vars
    assert!(!output.contains("100\n\n@export"), "got: {output}");
    // No blank line between regular vars and previous group
    assert!(
        !output.contains("$Sprite2D\n\n\nvar speed"),
        "got: {output}"
    );
    // One blank line between different groups
    assert!(output.contains("$Sprite2D\n\nvar speed"), "got: {output}");
}

#[test]
fn test_class_body_formatting() {
    let input = "class_name Player\n\nextends Node2D\n\nsignal died\n\nvar health: int = 100\nvar mana: int = 50\n\n\nfunc _ready() -> void:\n\tpass\n\n\nfunc _process(delta: float) -> void:\n\tpass\n";
    let output = format_source(input);
    // One blank line between different declaration groups
    assert!(
        output.contains("signal died\n\nvar health"),
        "got: {output}"
    );
    // No blank line between consecutive vars
    assert!(
        output.contains("health: int = 100\nvar mana"),
        "got: {output}"
    );
    // Two blank lines before first function
    assert!(
        output.contains("mana: int = 50\n\n\nfunc _ready"),
        "got: {output}"
    );
    // Two blank lines between functions
    assert!(output.contains("pass\n\n\nfunc _process"), "got: {output}");
}

#[test]
fn test_trailing_comma_preserved() {
    let input = "func f():\n\tvar items = [\n\t\t\"a\",\n\t\t\"b\",\n\t]\n";
    let output = format_source(input);
    // Trailing comma should be preserved (though spacing may be normalized)
    assert!(
        output.contains("\"b\","),
        "Trailing comma should be preserved, got: {output}"
    );
}

// ── Config option tests ───────────────────────────────────────────

fn format_with_config(source: &str, config: &crate::core::config::FmtConfig) -> String {
    let tree = parser::parse(source).unwrap();
    let mut printer = Printer::from_config(config);
    printer.format(&tree.root_node(), source);
    printer.finish()
}

#[test]
fn test_blank_lines_around_functions_one() {
    let config = crate::core::config::FmtConfig {
        blank_lines_around_functions: 1,
        ..Default::default()
    };
    let input = "var x = 1\n\n\nfunc a():\n\tpass\n\n\nfunc b():\n\tpass\n";
    let output = format_with_config(input, &config);
    // Only 1 blank line before/between functions
    assert!(output.contains("x = 1\n\nfunc a"), "got: {output}");
    assert!(output.contains("pass\n\nfunc b"), "got: {output}");
    // Not 2 blank lines
    assert!(!output.contains("\n\n\nfunc"), "got: {output}");
}

#[test]
fn test_trailing_newline_false() {
    let config = crate::core::config::FmtConfig {
        trailing_newline: false,
        ..Default::default()
    };
    let input = "func f():\n\tpass\n";
    let output = format_with_config(input, &config);
    assert!(!output.ends_with('\n'), "got: {output:?}");
    assert!(output.ends_with("pass"), "got: {output:?}");
}

#[test]
fn test_extends_string_path() {
    let input = "extends \"res://src/Tools/Base.gd\"\n";
    let output = format_source(input);
    assert_eq!(output, "extends \"res://src/Tools/Base.gd\"\n");
}

#[test]
fn test_extends_string_idempotent() {
    let input = "extends \"res://src/Tools/Base.gd\"\n\n\nfunc f():\n\tpass\n";
    let first = format_source(input);
    let second = format_source(&first);
    assert_eq!(
        first, second,
        "extends string not idempotent!\nFirst:\n{first}\nSecond:\n{second}"
    );
}

#[test]
fn test_comment_between_if_else() {
    let input = "func f():\n\tif x:\n\t\tpass\n\t# comment\n\telse:\n\t\tpass\n";
    let first = format_source(input);
    // Comment should be at body indent level for valid GDScript
    assert!(
        first.contains("\t\t# comment"),
        "comment should be at body indent, got:\n{first}"
    );
    let second = format_source(&first);
    assert_eq!(
        first, second,
        "if/comment/else not idempotent!\nFirst:\n{first}\nSecond:\n{second}"
    );
}

#[test]
fn test_comment_after_colon_in_func() {
    let input = "func f(): # comment\n\tpass\n";
    let first = format_source(input);
    let second = format_source(&first);
    assert_eq!(
        first, second,
        "func comment not idempotent!\nFirst:\n{first}\nSecond:\n{second}"
    );
}

#[test]
fn test_comment_after_colon_in_for() {
    let input = "func f():\n\tfor i in range(10): # loop\n\t\tpass\n";
    let first = format_source(input);
    let second = format_source(&first);
    assert_eq!(
        first, second,
        "for comment not idempotent!\nFirst:\n{first}\nSecond:\n{second}"
    );
}

#[test]
fn test_static_var() {
    let input = "static var count: int = 0\n";
    let output = format_source(input);
    assert_eq!(output, "static var count: int = 0\n");
}

#[test]
fn test_static_var_inferred() {
    let input = "static var count := 0\n";
    let output = format_source(input);
    assert_eq!(output, "static var count := 0\n");
}

#[test]
fn test_static_func() {
    let input = "static func create() -> void:\n\tpass\n";
    let output = format_source(input);
    assert_eq!(output, "static func create() -> void:\n\tpass\n");
}

#[test]
fn test_match_pattern_guard() {
    let input = "func f():\n\tmatch typeof(x):\n\t\tTYPE_INT when x > 0:\n\t\t\tpass\n";
    let output = format_source(input);
    assert!(
        output.contains("TYPE_INT when x > 0:"),
        "pattern guard spacing lost, got: {output}"
    );
    let second = format_source(&output);
    assert_eq!(
        output, second,
        "not idempotent!\nFirst:\n{output}\nSecond:\n{second}"
    );
}

#[test]
fn test_from_config_defaults_match_new() {
    let config = crate::core::config::FmtConfig::default();
    let input = "func a():\n\tpass\nfunc b():\n\tpass\n";
    let output_new = format_source(input);
    let output_config = format_with_config(input, &config);
    assert_eq!(output_new, output_config);
}

#[test]
fn test_multiline_dict_preserved() {
    let input = "const D = {\n\t\"a\": 1,\n\t\"b\": 2,\n}\n";
    let output = format_source(input);
    assert!(
        output.contains("{\n\t\"a\": 1,\n\t\"b\": 2,\n}"),
        "multiline dict should stay multiline, got:\n{output}"
    );
    let second = format_source(&output);
    assert_eq!(
        output, second,
        "not idempotent!\nFirst:\n{output}\nSecond:\n{second}"
    );
}

#[test]
fn test_multiline_array_preserved() {
    let input = "var a = [\n\t1,\n\t2,\n\t3,\n]\n";
    let output = format_source(input);
    assert!(
        output.contains("[\n\t1,\n\t2,\n\t3,\n]"),
        "multiline array should stay multiline, got:\n{output}"
    );
    let second = format_source(&output);
    assert_eq!(
        output, second,
        "not idempotent!\nFirst:\n{output}\nSecond:\n{second}"
    );
}

#[test]
fn test_array_with_comments_preserved() {
    let input = "var a = [\n\t\"foo\",  # comment\n\t\"bar\",\n]\n";
    let output = format_source(input);
    assert!(
        output.contains("# comment"),
        "comment should be preserved, got:\n{output}"
    );
    assert!(
        output.contains('\n'),
        "should stay multiline with comments, got:\n{output}"
    );
}

#[test]
fn test_inline_dict_stays_inline() {
    let input = "func f():\n\tvar d = {\"a\": 1, \"b\": 2}\n";
    let output = format_source(input);
    assert!(
        output.contains("{\"a\": 1, \"b\": 2}"),
        "inline dict should stay inline, got:\n{output}"
    );
}

#[test]
fn test_line_continuation_in_array() {
    let input = "func f() -> String:\n\treturn \"%s %s\" % [a,\\\n\t\tb]\n";
    let output = format_source(input);
    let tree = crate::core::parser::parse(&output).unwrap();
    assert!(
        !tree.root_node().has_error(),
        "formatted output has parse errors:\n{output}"
    );
    assert!(output.contains("[a, b]"), "got:\n{output}");
}

#[test]
fn test_typed_array_const_with_comments() {
    let input = "const X: PackedStringArray = [\n\t\"A\",  # comment\n\t\"B\",  # comment\n]\n";
    let output = format_source(input);
    let tree = crate::core::parser::parse(&output).unwrap();
    assert!(
        !tree.root_node().has_error(),
        "formatted output has parse errors:\n{output}"
    );
    assert!(output.contains("\"A\",  # comment"), "got:\n{output}");
}

#[test]
fn test_line_continuation_binary_op_idempotent() {
    let input = "func f():\n\tvar x = a \\\n\t\t\t+ b \\\n\t\t\t+ c\n";
    let pass1 = format_source(input);
    let pass2 = format_source(&pass1);
    eprintln!("=== PASS 1 ===\n{pass1}\n=== PASS 2 ===\n{pass2}\n=== END ===");
    assert_eq!(pass1, pass2, "not idempotent");
}

#[test]
fn test_line_continuation_chain_with_comment() {
    let input = "func f():\n\tvar x := a\\\n\t\t.b()\\\n\t\t# comment\n\t\t.c()\n";
    let pass1 = format_source(input);
    let pass2 = format_source(&pass1);
    assert_eq!(pass1, pass2, "not idempotent");
}

#[test]
fn test_line_continuation_in_assignment() {
    let input = "func f():\n\tx = \\\n\t\ta + b\n";
    let pass1 = format_source(input);
    let pass2 = format_source(&pass1);
    assert_eq!(pass1, pass2, "not idempotent");
    assert!(pass1.contains("x = a + b"), "got:\n{pass1}");
}

#[test]
fn test_line_continuation_in_params_idempotent() {
    let input = "func f(a: int, b: int, \\\n\t\tc: int) -> void:\n\tpass\n";
    let pass1 = format_source(input);
    let pass2 = format_source(&pass1);
    eprintln!("=== PASS 1 ===\n{pass1}\n=== PASS 2 ===\n{pass2}\n=== END ===");
    assert_eq!(pass1, pass2, "not idempotent");
}

#[test]
fn test_paren_expr_with_comments() {
    let input = "func f():\n\tvar x = (\n\t\t\ta + b\n\t\t\t# comment\n\t\t\t+ c\n\t\t)\n";
    let output = format_source(input);
    eprintln!("=== FORMATTED OUTPUT ===\n{output}\n=== END ===");
    // Should not introduce parse errors
    let tree = crate::core::parser::parse(&output).unwrap();
    assert!(
        !tree.root_node().has_error(),
        "formatted output has parse errors:\n{output}"
    );
}
