## Formatter edge cases — all constructs that previously caused
## non-idempotent formatting. Must be idempotent after formatting.

extends "res://base/entity.gd"

signal health_changed(amount: int)
signal died

const SPEED: float = 100.0

## Static variables and functions.

static var count: int = 0
static var _registry: Dictionary = {}


static func get_count() -> int:
	return count


static func register(name: String) -> void:
	_registry[name] = true
	count += 1


## Comments between if/elif/else clauses.


func branching(x: int) -> String:
	if x > 0:
		return "positive"
		# Comment between if and elif
	elif x == 0:
		return "zero"
		# Comment between elif and else
	else:
		return "negative"


## Comments inside for loops.


func loop_with_comments() -> void:
	for i in range(10):
		print(i)
	# Comment after for body
	for j in range(5):
		print(j)


## Comments inside while loops.


func while_with_comments() -> void:
	var i: int = 0
	while i < 10:
		i += 1
	# Comment after while body


## Match with pattern guards (when clauses).


func match_with_guard(x: Variant) -> String:
	match typeof(x):
		TYPE_INT when x > 0:
			return "positive int"
		TYPE_INT when x == 0:
			return "zero"
		TYPE_INT:
			return "negative int"
		TYPE_STRING:
			return "string"
		_:
			return "other"


## Match with comments between arms.


func match_with_comments(state: int) -> String:
	match state:
		0:
			return "idle"
		# Transition states
		1:
			return "walking"
		# Fast states
		2:
			return "running"
		_:
			return "unknown"


## Annotations on variables and functions.

@tool
@export var speed: float = 100.0
@export var jump_height: float = 200.0
@onready var sprite: Sprite2D = $Sprite2D
@onready var label: Label = $Label

@export_group("Movement")
@export var acceleration: float = 50.0


func _ready() -> void:
	pass


## Type casts and is checks.


func type_operations(node: Node) -> void:
	var sprite_ref: Sprite2D = node as Sprite2D
	if node is CharacterBody2D:
		print("is character")
	if sprite_ref != null:
		print(sprite_ref)


## Typed arrays with complex types.


func typed_arrays() -> void:
	var nodes: Array[Node2D] = []
	var names: Array[StringName] = [&"one", &"two"]
	var points: Array[Vector2] = [Vector2.ZERO, Vector2.ONE]
	print(nodes, names, points)


## Inferred types.


func inferred_types() -> void:
	var count := 42
	var name := "hello"
	var pos := Vector2(1, 2)
	print(count, name, pos)


## Await and signals.


func async_work() -> void:
	await get_tree().create_timer(1.0).timeout
	print("done")


## Preload and load.

const Scene: PackedScene = preload("res://scene.tscn")
const Script: GDScript = preload("res://script.gd")

## Multiline function calls.


func multiline_calls() -> void:
	var result: String = get_direction_string(
		Vector2.UP,
		true,
		"prefix"
	)
	print(result)


func get_direction_string(dir: Vector2, normalize: bool, prefix: String) -> String:
	if normalize:
		dir = dir.normalized()
	return prefix + str(dir)


## Not keyword and boolean operators.


func boolean_ops(a: bool, b: bool) -> bool:
	if not a:
		return false
	if a and b:
		return true
	if a or not b:
		return true
	return false


## Dictionary literals.


func make_dict() -> Dictionary:
	var simple: Dictionary = {"key": "value"}
	var multi: Dictionary = {"name": "player", "health": 100, "position": Vector2.ZERO, }
	return simple.merged(multi)


## Enum with values.

enum Element { FIRE = 0, WATER = 1, EARTH = 2, AIR = 3,  }

## Inner class with multiple members.


class Particle:
	var position: Vector2 = Vector2.ZERO
	var velocity: Vector2 = Vector2.ZERO
	var lifetime: float = 1.0

	func update(delta: float) -> void:
		position += velocity * delta
		lifetime -= delta

	func is_dead() -> bool:
		return lifetime <= 0.0

	func reset(pos: Vector2, vel: Vector2) -> void:
		position = pos
		velocity = vel
		lifetime = 1.0


## Nested match.


func nested_match(a: int, b: int) -> String:
	match a:
		0:
			match b:
				0:
					return "both zero"
				_:
					return "a zero"
		_:
			return "a nonzero"
