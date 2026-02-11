## A comprehensive, lint-clean GDScript file.
## Exercises every major language feature with zero expected warnings.

@tool
class_name CleanExample

extends Node2D

signal health_changed(new_health: int)
signal died

enum Direction { UP, DOWN, LEFT, RIGHT,  }
enum State { IDLE, RUNNING, JUMPING,  }

const MAX_SPEED: float = 200.0
const GRAVITY: int = 980
const JUMP_FORCE: float = -400.0

var speed: float = 0.0
var velocity: Vector2 = Vector2.ZERO
var _health: int = 100
var _is_alive: bool = true

@export var max_health: int = 100
@export var damage_multiplier: float = 1.0
@onready var sprite: Sprite2D = $Sprite2D
@onready var collision: CollisionShape2D = $CollisionShape2D


func _ready() -> void:
	_health = max_health
	health_changed.emit(_health)
	_setup_signals()


func _process(delta: float) -> void:
	speed = velocity.length()
	if speed > MAX_SPEED:
		velocity = velocity.normalized() * MAX_SPEED
	_update_animation(delta)


func _physics_process(delta: float) -> void:
	velocity.y += GRAVITY * delta
	move_and_slide()
	if is_on_floor() and velocity.y > 0:
		velocity.y = 0


func take_damage(amount: int) -> void:
	var actual_damage: int = int(amount * damage_multiplier)
	_health -= actual_damage
	health_changed.emit(_health)
	if _health <= 0:
		_die()


func heal(amount: int) -> void:
	_health = mini(_health + amount, max_health)
	health_changed.emit(_health)


func get_health() -> int:
	return _health


func is_alive() -> bool:
	return _is_alive


func get_direction_name(dir: Direction) -> String:
	match dir:
		Direction.UP:
			return "up"
		Direction.DOWN:
			return "down"
		Direction.LEFT:
			return "left"
		Direction.RIGHT:
			return "right"
		_:
			return "unknown"


func _die() -> void:
	_is_alive = false
	died.emit()
	queue_free()


func _setup_signals() -> void:
	health_changed.connect(_on_health_changed)


func _on_health_changed(new_health: int) -> void:
	if new_health < max_health / 4:
		sprite.modulate = Color.RED


func _update_animation(_delta: float) -> void:
	if not _is_alive:
		return
	if speed > 0:
		sprite.flip_h = velocity.x < 0


## Typed arrays and dictionaries.


func get_inventory() -> Array[String]:
	var items: Array[String] = ["sword", "shield", "potion"]
	return items


func get_stats() -> Dictionary:
	return {"health": _health, "speed": speed, "alive": _is_alive, }


## For loop with proper snake_case variable.


func sum_values(values: Array[int]) -> int:
	var total: int = 0
	for value in values:
		total += value
	return total


## While loop.


func countdown(from: int) -> void:
	var count: int = from
	while count > 0:
		count -= 1


## Ternary and as cast.


func get_label() -> String:
	var text: String = "alive" if _is_alive else "dead"
	return text


func try_cast(node: Node) -> Sprite2D:
	var result: Sprite2D = node as Sprite2D
	return result


## Static members.

static var instance_count: int = 0


static func get_instance_count() -> int:
	return instance_count


## Inner class.


class HealthBar:
	var current: int = 0
	var maximum: int = 100

	func set_health(value: int) -> void:
		current = clampi(value, 0, maximum)

	func get_percentage() -> float:
		if maximum == 0:
			return 0.0
		return float(current) / float(maximum)
