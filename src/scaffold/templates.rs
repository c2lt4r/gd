//! Built-in project templates.

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
config/features=PackedStringArray("{version}", "{renderer_feature}")

[rendering]

renderer/rendering_method="{renderer}"
"#;

pub const GITIGNORE_TEMPLATE: &str = r"# Godot 4+ specific ignores
.godot/
build/

# gd toolchain
gd.toml

# OS
.DS_Store
Thumbs.db
";

pub const GD_TOML_TEMPLATE: &str = r#"# gd toolchain configuration
# See: https://github.com/c2lt4r/gd

[fmt]
use_tabs = true
indent_size = 4
max_line_length = 100

[lint]
# ignore_patterns = ["addons/**"]

# Category-level controls: "off" | "info" | "warning" | "error"
# correctness = "error"      # definite bugs (default: on)
# suspicious = "warning"     # likely bugs (default: on)
# style = "warning"          # naming and code style (default: on)
# type_safety = "warning"    # enable all type-safety rules incl. opt-in
# maintenance = "off"        # disable unused-code/debug rules

# Per-rule overrides (take precedence over category)
# [lint.rules.naming-convention]
# severity = "error"

# [lint.rules.print-statement]
# severity = "off"

[build]
output_dir = "build"

[run]
# godot_path = "/usr/bin/godot"
extra_args = []
"#;

pub struct TemplateSet {
    pub node_type: &'static str,
    pub renderer: &'static str,
    pub renderer_feature: &'static str,
}

pub fn template_for(template: &str) -> Option<TemplateSet> {
    match template {
        "default" => Some(TemplateSet {
            node_type: "Node",
            renderer: "forward_plus",
            renderer_feature: "Forward Plus",
        }),
        "2d" => Some(TemplateSet {
            node_type: "Node2D",
            renderer: "gl_compatibility",
            renderer_feature: "GL Compatibility",
        }),
        "3d" => Some(TemplateSet {
            node_type: "Node3D",
            renderer: "forward_plus",
            renderer_feature: "Forward Plus",
        }),
        _ => None,
    }
}

pub fn scene_content(node_type: &str) -> String {
    format!(
        "\
[gd_scene load_steps=2 format=3 uid=\"uid://main\"]

[ext_resource type=\"Script\" path=\"res://main.gd\" id=\"1\"]

[node name=\"Main\" type=\"{node_type}\"]
script = ExtResource(\"1\")
"
    )
}

pub fn script_content(node_type: &str) -> String {
    format!(
        "\
extends {node_type}


func _ready() -> void:
\tpass


func _process(delta: float) -> void:
\tpass
"
    )
}

pub fn project_godot_content(
    name: &str,
    renderer: &str,
    renderer_feature: &str,
    godot_version: &str,
) -> String {
    PROJECT_GODOT_TEMPLATE
        .replace("{name}", name)
        .replace("{renderer}", renderer)
        .replace("{renderer_feature}", renderer_feature)
        .replace("{version}", godot_version)
}
