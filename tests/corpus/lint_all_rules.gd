## Intentionally bad file that triggers every default-enabled lint rule.
## Each section is labeled with the rule(s) it exercises.

extends Node

# --- duplicate-signal ---

signal player_died
signal player_died

# --- unused-signal ---

signal never_used

# --- naming-convention ---


func BadlyNamedFunction() -> void:
	pass


var MyBadVar: int = 0

# --- unused-variable ---


func test_unused() -> void:
	var unused_local: int = 42
	pass


# --- missing-type-hint ---


func no_types(x, y):
	return x + y


# --- empty-function ---


func do_nothing() -> void:
	pass


# --- long-function (51 lines with default threshold 50) ---


func very_long_function() -> void:
	var a1: int = 1
	var a2: int = 2
	var a3: int = 3
	var a4: int = 4
	var a5: int = 5
	var a6: int = 6
	var a7: int = 7
	var a8: int = 8
	var a9: int = 9
	var a10: int = 10
	var a11: int = 11
	var a12: int = 12
	var a13: int = 13
	var a14: int = 14
	var a15: int = 15
	var a16: int = 16
	var a17: int = 17
	var a18: int = 18
	var a19: int = 19
	var a20: int = 20
	var a21: int = 21
	var a22: int = 22
	var a23: int = 23
	var a24: int = 24
	var a25: int = 25
	var a26: int = 26
	var a27: int = 27
	var a28: int = 28
	var a29: int = 29
	var a30: int = 30
	var a31: int = 31
	var a32: int = 32
	var a33: int = 33
	var a34: int = 34
	var a35: int = 35
	var a36: int = 36
	var a37: int = 37
	var a38: int = 38
	var a39: int = 39
	var a40: int = 40
	var a41: int = 41
	var a42: int = 42
	var a43: int = 43
	var a44: int = 44
	var a45: int = 45
	var a46: int = 46
	var a47: int = 47
	var a48: int = 48
	print(a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8 + a9 + a10)
	print(a11 + a12 + a13 + a14 + a15 + a16 + a17 + a18 + a19 + a20)
	print(a21 + a22 + a23 + a24 + a25 + a26 + a27 + a28 + a29 + a30)
	print(a31 + a32 + a33 + a34 + a35 + a36 + a37 + a38 + a39 + a40)
	print(a41 + a42 + a43 + a44 + a45 + a46 + a47 + a48)


# --- self-assignment ---

var value: int = 10


func test_self_assign() -> void:
	value = value


# --- unreachable-code ---


func test_unreachable() -> int:
	return 1
	var dead: int = 2
	return dead


# --- shadowed-variable (must shadow a parameter) ---


func test_shadow(score: int) -> void:
	var score: int = 100
	print(score)


# --- comparison-with-boolean ---


func test_bool_compare(flag: bool) -> void:
	if flag == true:
		pass
	if flag == false:
		pass


# --- unnecessary-pass ---


func test_unnecessary_pass() -> void:
	var x: int = 1
	pass


# --- preload-type-hint (var with no type annotation) ---

var my_scene = preload("res://scene.tscn")

# --- integer-division ---


func test_int_div() -> void:
	var result: int = 10 / 3
	print(result)


# --- signal-name-convention ---

signal on_player_hit

# --- float-comparison ---


func test_float_cmp() -> void:
	var a: float = 0.1 + 0.2
	if a == 0.3:
		pass


# --- return-type-mismatch (bare return in non-void) ---


func test_return_mismatch() -> int:
	return


# --- private-method-access ---

var other_node: Node = null


func test_private_access() -> void:
	other_node._private_method()


# --- untyped-array (no type annotation at all) ---


func test_untyped_array() -> void:
	var items = [1, 2, 3]
	print(items)


# --- duplicate-function ---


func duplicate_me() -> void:
	pass


func duplicate_me() -> void:
	pass


# --- duplicate-key ---


func test_dup_key() -> void:
	var dict: Dictionary = {"a": 1, "a": 2}
	print(dict)


# --- await-in-ready ---


func _ready() -> void:
	await get_tree().create_timer(1.0).timeout


# --- missing-return ---


func must_return() -> String:
	var x: int = 1
	if x > 0:
		return "positive"


# --- static-type-inference (var = literal without type) ---

var inferred_int = 42
var inferred_str = "hello"
var inferred_bool = true

# --- enum-naming ---

enum bad_enum { camelCase, another_bad,  }

# --- parameter-naming ---


func bad_params(FirstParam: int, secondParam: int) -> void:
	print(FirstParam + secondParam)


# --- too-many-parameters ---


func too_many(a: int, b: int, c: int, d: int, e: int, f: int) -> void:
	print(a + b + c + d + e + f)


# --- cyclomatic-complexity (many branches) ---


func complex_logic(x: int) -> String:
	if x == 1:
		return "one"
	elif x == 2:
		return "two"
	elif x == 3:
		return "three"
	elif x == 4:
		return "four"
	elif x == 5:
		return "five"
	elif x == 6:
		return "six"
	elif x == 7:
		return "seven"
	elif x == 8:
		return "eight"
	elif x == 9:
		return "nine"
	elif x == 10:
		return "ten"
	elif x == 11:
		return "eleven"
	else:
		return "other"


# --- deeply-nested-code ---


func deep_nesting(a: bool, b: bool, c: bool, d: bool, e: bool) -> void:
	if a:
		if b:
			if c:
				if d:
					if e:
						print("deep")


# --- get-node-in-process + physics-in-process (both in _process) ---


func _process(_delta: float) -> void:
	var node: Node = get_node("Child")
	move_and_slide()
	print(node)


# --- redundant-else ---


func test_redundant_else(x: int) -> String:
	if x > 0:
		return "positive"
	else:
		return "non-positive"


# --- duplicated-load ---

var res_a = preload("res://shared.tscn")
var res_b = preload("res://shared.tscn")

# --- unused-preload ---

var unused_res = preload("res://unused.tscn")

# --- standalone-expression ---


func test_standalone() -> void:
	42
	"hello"


# --- comparison-with-itself ---


func test_self_compare(x: int) -> void:
	if x == x:
		pass


# --- loop-variable-name ---


func test_loop_naming() -> void:
	for BadItem in [1, 2, 3]:
		print(BadItem)


# --- node-ready-order ---

func _init() -> void:
	var child: Node = $Something
	print(child)
