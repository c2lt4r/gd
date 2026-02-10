/// Built-in project templates.

pub const PROJECT_GODOT_TEMPLATE: &str = r#"; Engine configuration file.
; It's best edited using the editor UI and not directly,
; since the parameters that go here are not all obvious.
;
; Format:
;   [section] ; section goes between []
;   param=value ; assign values to parameters

config_version=5

[application]

config/name="{name}"
run/main_scene="res://main.tscn"
config/features=PackedStringArray("4.4")

[rendering]

renderer/rendering_method="forward_plus"
"#;

pub const MAIN_SCENE_TEMPLATE: &str = r#"[gd_scene format=3 uid="uid://main"]

[node name="Main" type="Node"]
"#;

pub const MAIN_SCRIPT_TEMPLATE: &str = r#"extends Node

func _ready() -> void:
	pass

func _process(delta: float) -> void:
	pass
"#;

pub const GITIGNORE_TEMPLATE: &str = r#"# Godot 4+ specific ignores
.godot/
build/

# gd toolchain
gd.toml

# OS
.DS_Store
Thumbs.db
"#;

pub const GD_TOML_TEMPLATE: &str = r#"# gd toolchain configuration

[fmt]
use_tabs = true
indent_size = 4
max_line_length = 100

[lint]
disabled_rules = []

[build]
output_dir = "build"

[run]
# godot_path = "/usr/bin/godot"
extra_args = []
"#;
