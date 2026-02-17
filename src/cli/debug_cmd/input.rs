use std::time::Duration;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use super::args::{ClickArgs, KeyArgs, OutputFormat, PressArgs, TypeTextArgs, WaitArgs};
use crate::core::live_eval::send_eval;
use crate::core::project::GodotProject;

/// Default timeout for input eval commands.
const INPUT_TIMEOUT: Duration = Duration::from_secs(10);

/// Convert an optional hold duration in seconds to milliseconds.
fn hold_to_ms(hold: Option<f64>) -> u64 {
    hold.map_or(0, |s| (s * 1000.0) as u64)
}

/// Resolve the project root for input commands.
fn project_root() -> Result<std::path::PathBuf> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let project = GodotProject::discover(&cwd)?;
    Ok(project.root)
}

/// Sleep for `ms` milliseconds, but exit early if the game dies.
/// Checks the eval-ready file every 100ms. Returns true if the full
/// duration elapsed, false if the game exited early.
fn hold_sleep(ms: u64, root: &std::path::Path) -> bool {
    let ready_path = root.join(".godot").join("gd-eval-ready");
    let interval = Duration::from_millis(100);
    let start = std::time::Instant::now();
    let target = Duration::from_millis(ms);

    while start.elapsed() < target {
        if !ready_path.is_file() {
            return false;
        }
        let remaining = target.saturating_sub(start.elapsed());
        std::thread::sleep(remaining.min(interval));
    }
    true
}

/// Run a generated GDScript via live eval and print the result.
fn run_input_script(script: &str, format: &OutputFormat) -> Result<()> {
    let root = project_root()?;
    let result = send_eval(script, &root, INPUT_TIMEOUT)?;
    if result.starts_with("ERROR:") {
        return Err(miette!("{result}"));
    }
    match format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"result": result})).unwrap()
            );
        }
        OutputFormat::Text => println!("{}", result.green()),
    }
    Ok(())
}

// ── Click ────────────────────────────────────────────────────────────

fn mouse_button_constant(button: &str) -> Result<&'static str> {
    match button.to_lowercase().as_str() {
        "left" => Ok("MOUSE_BUTTON_LEFT"),
        "right" => Ok("MOUSE_BUTTON_RIGHT"),
        "middle" => Ok("MOUSE_BUTTON_MIDDLE"),
        other => Err(miette!(
            "Unknown mouse button '{other}' (expected: left, right, middle)"
        )),
    }
}

fn generate_click_pos_script(x: &str, y: &str, button: &str, double: bool) -> Result<String> {
    let btn = mouse_button_constant(button)?;
    let double_line = if double {
        "\n\tev.double_click = true"
    } else {
        ""
    };
    Ok(format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar pos = Vector2({x}, {y})\n\
         \tvar ev = InputEventMouseButton.new()\n\
         \tev.button_index = {btn}\n\
         \tev.pressed = true\n\
         \tev.position = pos\n\
         \tev.global_position = pos{double_line}\n\
         \tInput.parse_input_event(ev)\n\
         \tvar rel = ev.duplicate()\n\
         \trel.pressed = false\n\
         \tInput.parse_input_event(rel)\n\
         \treturn \"clicked at (%s, %s)\" % [pos.x, pos.y]\n"
    ))
}

fn generate_click_pos_down_script(x: &str, y: &str, button: &str) -> Result<String> {
    let btn = mouse_button_constant(button)?;
    Ok(format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar pos = Vector2({x}, {y})\n\
         \tvar ev = InputEventMouseButton.new()\n\
         \tev.button_index = {btn}\n\
         \tev.pressed = true\n\
         \tev.position = pos\n\
         \tev.global_position = pos\n\
         \tInput.parse_input_event(ev)\n\
         \treturn \"holding click at (%s, %s)\" % [pos.x, pos.y]\n"
    ))
}

fn generate_click_pos_up_script(x: &str, y: &str, button: &str) -> Result<String> {
    let btn = mouse_button_constant(button)?;
    Ok(format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar pos = Vector2({x}, {y})\n\
         \tvar ev = InputEventMouseButton.new()\n\
         \tev.button_index = {btn}\n\
         \tev.pressed = false\n\
         \tev.position = pos\n\
         \tev.global_position = pos\n\
         \tInput.parse_input_event(ev)\n\
         \treturn \"released click at (%s, %s)\" % [pos.x, pos.y]\n"
    ))
}

fn generate_click_node_script(node: &str, button: &str, double: bool) -> Result<String> {
    let btn = mouse_button_constant(button)?;
    let double_line = if double {
        "\n\tev.double_click = true"
    } else {
        ""
    };
    // Absolute path → get_node(), relative name → find_child()
    let lookup = if node.starts_with('/') {
        format!(
            "\tvar node = get_tree().get_root().get_node_or_null(\"{node}\")\n\
             \tif node == null: return \"ERROR: node '{node}' not found\""
        )
    } else {
        format!(
            "\tvar node = get_tree().get_root().find_child(\"{node}\", true, false)\n\
             \tif node == null: return \"ERROR: node '{node}' not found\""
        )
    };
    Ok(format!(
        "extends Node\n\
         \n\
         func run():\n\
         {lookup}\n\
         \tif not node is Control: return \"ERROR: '\" + node.name + \"' is not a Control\"\n\
         \tvar pos = node.get_global_rect().get_center()\n\
         \tvar ev = InputEventMouseButton.new()\n\
         \tev.button_index = {btn}\n\
         \tev.pressed = true\n\
         \tev.position = pos\n\
         \tev.global_position = pos{double_line}\n\
         \tInput.parse_input_event(ev)\n\
         \tvar rel = ev.duplicate()\n\
         \trel.pressed = false\n\
         \tInput.parse_input_event(rel)\n\
         \treturn \"clicked at (%s, %s) on %s\" % [pos.x, pos.y, node.name]\n"
    ))
}

pub fn cmd_click(args: &ClickArgs) -> Result<()> {
    let hold_ms = hold_to_ms(args.hold);
    match (&args.pos, &args.node) {
        (Some(pos), None) => {
            let parts: Vec<&str> = pos.split(',').collect();
            if parts.len() != 2 {
                return Err(miette!(
                    "Invalid position '{pos}' — expected format: X,Y (e.g. 100,200)"
                ));
            }
            let x = parts[0].trim();
            let y = parts[1].trim();
            x.parse::<f64>()
                .map_err(|_| miette!("Invalid X coordinate: {x}"))?;
            y.parse::<f64>()
                .map_err(|_| miette!("Invalid Y coordinate: {y}"))?;
            if hold_ms > 0 {
                let root = project_root()?;
                let down = generate_click_pos_down_script(x, y, &args.button)?;
                run_input_script(&down, &args.format)?;
                if !hold_sleep(hold_ms, &root) {
                    return Err(miette!("Game exited during hold"));
                }
                let up = generate_click_pos_up_script(x, y, &args.button)?;
                run_input_script(&up, &args.format)
            } else {
                let script = generate_click_pos_script(x, y, &args.button, args.double)?;
                run_input_script(&script, &args.format)
            }
        }
        (None, Some(node)) => {
            // Node click with hold not supported (node position may change)
            let script = generate_click_node_script(node, &args.button, args.double)?;
            run_input_script(&script, &args.format)
        }
        (Some(_), Some(_)) => Err(miette!("Specify either --pos or --node, not both")),
        (None, None) => Err(miette!("Specify --pos X,Y or --node <name>")),
    }
}

// ── Press ────────────────────────────────────────────────────────────

fn generate_press_script(action: &str, release: bool) -> String {
    if release {
        // Release-only script
        format!(
            "extends Node\n\
             \n\
             func run():\n\
             \tvar ev = InputEventAction.new()\n\
             \tev.action = \"{action}\"\n\
             \tev.pressed = false\n\
             \tev.strength = 0.0\n\
             \tInput.parse_input_event(ev)\n\
             \treturn \"released action: {action}\"\n"
        )
    } else {
        // Press + immediate release (original behavior)
        format!(
            "extends Node\n\
             \n\
             func run():\n\
             \tvar ev = InputEventAction.new()\n\
             \tev.action = \"{action}\"\n\
             \tev.pressed = true\n\
             \tev.strength = 1.0\n\
             \tInput.parse_input_event(ev)\n\
             \tvar rel = ev.duplicate()\n\
             \trel.pressed = false\n\
             \tInput.parse_input_event(rel)\n\
             \treturn \"pressed action: {action}\"\n"
        )
    }
}

fn generate_press_down_script(action: &str) -> String {
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar ev = InputEventAction.new()\n\
         \tev.action = \"{action}\"\n\
         \tev.pressed = true\n\
         \tev.strength = 1.0\n\
         \tInput.parse_input_event(ev)\n\
         \treturn \"holding action: {action}\"\n"
    )
}

pub fn cmd_press(args: &PressArgs) -> Result<()> {
    let hold_ms = hold_to_ms(args.hold);
    if hold_ms > 0 {
        // Hold mode: press → sleep on Rust side → release (game runs during sleep)
        let root = project_root()?;
        let press = generate_press_down_script(&args.action);
        run_input_script(&press, &args.format)?;
        if !hold_sleep(hold_ms, &root) {
            return Err(miette!("Game exited during hold"));
        }
        let release = generate_press_script(&args.action, true);
        run_input_script(&release, &args.format)
    } else {
        let script = generate_press_script(&args.action, false);
        run_input_script(&script, &args.format)
    }
}

// ── Key ──────────────────────────────────────────────────────────────

fn key_name_to_godot(name: &str) -> Result<&'static str> {
    match name.to_lowercase().as_str() {
        "space" => Ok("KEY_SPACE"),
        "enter" | "return" => Ok("KEY_ENTER"),
        "escape" | "esc" => Ok("KEY_ESCAPE"),
        "tab" => Ok("KEY_TAB"),
        "backspace" => Ok("KEY_BACKSPACE"),
        "delete" | "del" => Ok("KEY_DELETE"),
        "insert" => Ok("KEY_INSERT"),
        "home" => Ok("KEY_HOME"),
        "end" => Ok("KEY_END"),
        "pageup" => Ok("KEY_PAGEUP"),
        "pagedown" => Ok("KEY_PAGEDOWN"),
        "up" => Ok("KEY_UP"),
        "down" => Ok("KEY_DOWN"),
        "left" => Ok("KEY_LEFT"),
        "right" => Ok("KEY_RIGHT"),
        "shift" => Ok("KEY_SHIFT"),
        "ctrl" | "control" => Ok("KEY_CTRL"),
        "alt" => Ok("KEY_ALT"),
        "capslock" => Ok("KEY_CAPSLOCK"),
        "f1" => Ok("KEY_F1"),
        "f2" => Ok("KEY_F2"),
        "f3" => Ok("KEY_F3"),
        "f4" => Ok("KEY_F4"),
        "f5" => Ok("KEY_F5"),
        "f6" => Ok("KEY_F6"),
        "f7" => Ok("KEY_F7"),
        "f8" => Ok("KEY_F8"),
        "f9" => Ok("KEY_F9"),
        "f10" => Ok("KEY_F10"),
        "f11" => Ok("KEY_F11"),
        "f12" => Ok("KEY_F12"),
        "a" => Ok("KEY_A"),
        "b" => Ok("KEY_B"),
        "c" => Ok("KEY_C"),
        "d" => Ok("KEY_D"),
        "e" => Ok("KEY_E"),
        "f" => Ok("KEY_F"),
        "g" => Ok("KEY_G"),
        "h" => Ok("KEY_H"),
        "i" => Ok("KEY_I"),
        "j" => Ok("KEY_J"),
        "k" => Ok("KEY_K"),
        "l" => Ok("KEY_L"),
        "m" => Ok("KEY_M"),
        "n" => Ok("KEY_N"),
        "o" => Ok("KEY_O"),
        "p" => Ok("KEY_P"),
        "q" => Ok("KEY_Q"),
        "r" => Ok("KEY_R"),
        "s" => Ok("KEY_S"),
        "t" => Ok("KEY_T"),
        "u" => Ok("KEY_U"),
        "v" => Ok("KEY_V"),
        "w" => Ok("KEY_W"),
        "x" => Ok("KEY_X"),
        "y" => Ok("KEY_Y"),
        "z" => Ok("KEY_Z"),
        "0" => Ok("KEY_0"),
        "1" => Ok("KEY_1"),
        "2" => Ok("KEY_2"),
        "3" => Ok("KEY_3"),
        "4" => Ok("KEY_4"),
        "5" => Ok("KEY_5"),
        "6" => Ok("KEY_6"),
        "7" => Ok("KEY_7"),
        "8" => Ok("KEY_8"),
        "9" => Ok("KEY_9"),
        other => Err(miette!("Unknown key: '{other}'")),
    }
}

fn generate_key_script(key_constant: &str, key_name: &str) -> String {
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar ev = InputEventKey.new()\n\
         \tev.keycode = {key_constant}\n\
         \tev.physical_keycode = {key_constant}\n\
         \tev.pressed = true\n\
         \tInput.parse_input_event(ev)\n\
         \tvar rel = ev.duplicate()\n\
         \trel.pressed = false\n\
         \tInput.parse_input_event(rel)\n\
         \treturn \"pressed key: {key_name}\"\n"
    )
}

fn generate_key_down_script(key_constant: &str, key_name: &str) -> String {
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar ev = InputEventKey.new()\n\
         \tev.keycode = {key_constant}\n\
         \tev.physical_keycode = {key_constant}\n\
         \tev.pressed = true\n\
         \tInput.parse_input_event(ev)\n\
         \treturn \"holding key: {key_name}\"\n"
    )
}

fn generate_key_up_script(key_constant: &str, key_name: &str) -> String {
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar ev = InputEventKey.new()\n\
         \tev.keycode = {key_constant}\n\
         \tev.physical_keycode = {key_constant}\n\
         \tev.pressed = false\n\
         \tInput.parse_input_event(ev)\n\
         \treturn \"released key: {key_name}\"\n"
    )
}

pub fn cmd_key(args: &KeyArgs) -> Result<()> {
    let constant = key_name_to_godot(&args.key)?;
    let key_name = args.key.to_lowercase();
    let hold_ms = hold_to_ms(args.hold);
    if hold_ms > 0 {
        let root = project_root()?;
        let press = generate_key_down_script(constant, &key_name);
        run_input_script(&press, &args.format)?;
        if !hold_sleep(hold_ms, &root) {
            return Err(miette!("Game exited during hold"));
        }
        let release = generate_key_up_script(constant, &key_name);
        run_input_script(&release, &args.format)
    } else {
        let script = generate_key_script(constant, &key_name);
        run_input_script(&script, &args.format)
    }
}

// ── Type ─────────────────────────────────────────────────────────────

fn generate_type_script(text: &str) -> String {
    // Escape backslashes and quotes for GDScript string literal
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    let len = text.len();
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tfor ch in \"{escaped}\":\n\
         \t\tvar ev = InputEventKey.new()\n\
         \t\tev.unicode = ch.unicode_at(0)\n\
         \t\tev.pressed = true\n\
         \t\tInput.parse_input_event(ev)\n\
         \t\tvar rel = ev.duplicate()\n\
         \t\trel.pressed = false\n\
         \t\tInput.parse_input_event(rel)\n\
         \treturn \"typed {len} characters\"\n"
    )
}

fn generate_type_char_script(ch: char) -> String {
    let code = u32::from(ch);
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar ev = InputEventKey.new()\n\
         \tev.unicode = {code}\n\
         \tev.pressed = true\n\
         \tInput.parse_input_event(ev)\n\
         \tvar rel = ev.duplicate()\n\
         \trel.pressed = false\n\
         \tInput.parse_input_event(rel)\n\
         \treturn \"typed char\"\n"
    )
}

pub fn cmd_type_text(args: &TypeTextArgs) -> Result<()> {
    let delay_ms = args.delay.unwrap_or(0);
    if delay_ms > 0 {
        // Per-character with Rust-side delay (game keeps running between chars)
        let root = project_root()?;
        for ch in args.text.chars() {
            let script = generate_type_char_script(ch);
            send_eval(&script, &root, INPUT_TIMEOUT)?;
            if !hold_sleep(delay_ms, &root) {
                return Err(miette!("Game exited during typing"));
            }
        }
        let len = args.text.len();
        match args.format {
            OutputFormat::Json => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &serde_json::json!({"result": format!("typed {len} characters")})
                    )
                    .unwrap()
                );
            }
            OutputFormat::Text => println!("{}", format!("typed {len} characters").green()),
        }
        Ok(())
    } else {
        let script = generate_type_script(&args.text);
        run_input_script(&script, &args.format)
    }
}

// ── Wait ─────────────────────────────────────────────────────────────

fn generate_wait_script(ms: u64, description: &str) -> String {
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tOS.delay_msec({ms})\n\
         \treturn \"waited {description}\"\n"
    )
}

pub fn cmd_wait(args: &WaitArgs) -> Result<()> {
    let (ms, desc) = match (args.frames, args.seconds) {
        (Some(frames), None) => {
            let ms = (f64::from(u32::try_from(frames).unwrap_or(u32::MAX)) * 16.667) as u64;
            (ms, format!("{frames} frames (~{ms}ms)"))
        }
        (None, Some(secs)) => {
            let ms = (secs * 1000.0) as u64;
            (ms, format!("{secs}s"))
        }
        (Some(_), Some(_)) => {
            return Err(miette!("Specify either --frames or --seconds, not both"));
        }
        (None, None) => return Err(miette!("Specify --frames <n> or --seconds <f>")),
    };
    let script = generate_wait_script(ms, &desc);
    run_input_script(&script, &args.format)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_pos_script_parses() {
        let script = generate_click_pos_script("100", "200", "left", false).unwrap();
        let tree = crate::core::parser::parse(&script).unwrap();
        assert!(
            !tree.root_node().has_error(),
            "Click pos script has parse errors:\n{script}"
        );
    }

    #[test]
    fn click_pos_double_script_parses() {
        let script = generate_click_pos_script("50", "75", "right", true).unwrap();
        let tree = crate::core::parser::parse(&script).unwrap();
        assert!(
            !tree.root_node().has_error(),
            "Click double script has parse errors:\n{script}"
        );
    }

    #[test]
    fn click_pos_hold_scripts_parse() {
        let down = generate_click_pos_down_script("100", "200", "left").unwrap();
        let tree = crate::core::parser::parse(&down).unwrap();
        assert!(
            !tree.root_node().has_error(),
            "Down script has errors:\n{down}"
        );
        let up = generate_click_pos_up_script("100", "200", "left").unwrap();
        let tree = crate::core::parser::parse(&up).unwrap();
        assert!(!tree.root_node().has_error(), "Up script has errors:\n{up}");
    }

    #[test]
    fn click_node_script_parses() {
        let script = generate_click_node_script("Button", "left", false).unwrap();
        let tree = crate::core::parser::parse(&script).unwrap();
        assert!(
            !tree.root_node().has_error(),
            "Click node script has parse errors:\n{script}"
        );
    }

    #[test]
    fn click_node_path_script_parses() {
        let script = generate_click_node_script("/root/UI/Button", "middle", true).unwrap();
        let tree = crate::core::parser::parse(&script).unwrap();
        assert!(
            !tree.root_node().has_error(),
            "Click node path script has parse errors:\n{script}"
        );
    }

    #[test]
    fn press_script_parses() {
        let script = generate_press_script("ui_accept", false);
        let tree = crate::core::parser::parse(&script).unwrap();
        assert!(
            !tree.root_node().has_error(),
            "Press script has parse errors:\n{script}"
        );
    }

    #[test]
    fn press_hold_scripts_parse() {
        let down = generate_press_down_script("accelerate");
        let tree = crate::core::parser::parse(&down).unwrap();
        assert!(
            !tree.root_node().has_error(),
            "Down script has errors:\n{down}"
        );
        let up = generate_press_script("accelerate", true);
        let tree = crate::core::parser::parse(&up).unwrap();
        assert!(!tree.root_node().has_error(), "Up script has errors:\n{up}");
    }

    #[test]
    fn key_script_parses() {
        let script = generate_key_script("KEY_SPACE", "space");
        let tree = crate::core::parser::parse(&script).unwrap();
        assert!(
            !tree.root_node().has_error(),
            "Key script has parse errors:\n{script}"
        );
    }

    #[test]
    fn key_hold_scripts_parse() {
        let down = generate_key_down_script("KEY_UP", "up");
        let tree = crate::core::parser::parse(&down).unwrap();
        assert!(
            !tree.root_node().has_error(),
            "Down script has errors:\n{down}"
        );
        let up = generate_key_up_script("KEY_UP", "up");
        let tree = crate::core::parser::parse(&up).unwrap();
        assert!(!tree.root_node().has_error(), "Up script has errors:\n{up}");
    }

    #[test]
    fn type_script_parses() {
        let script = generate_type_script("hello world");
        let tree = crate::core::parser::parse(&script).unwrap();
        assert!(
            !tree.root_node().has_error(),
            "Type script has parse errors:\n{script}"
        );
    }

    #[test]
    fn type_char_script_parses() {
        let script = generate_type_char_script('a');
        let tree = crate::core::parser::parse(&script).unwrap();
        assert!(
            !tree.root_node().has_error(),
            "Type char script has errors:\n{script}"
        );
    }

    #[test]
    fn type_script_escapes_quotes() {
        let script = generate_type_script("say \"hi\"");
        assert!(script.contains("say \\\"hi\\\""));
        let tree = crate::core::parser::parse(&script).unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn wait_script_parses() {
        let script = generate_wait_script(1000, "1s");
        let tree = crate::core::parser::parse(&script).unwrap();
        assert!(
            !tree.root_node().has_error(),
            "Wait script has parse errors:\n{script}"
        );
    }

    #[test]
    fn key_mapping_valid_names() {
        assert_eq!(key_name_to_godot("space").unwrap(), "KEY_SPACE");
        assert_eq!(key_name_to_godot("Space").unwrap(), "KEY_SPACE");
        assert_eq!(key_name_to_godot("ENTER").unwrap(), "KEY_ENTER");
        assert_eq!(key_name_to_godot("return").unwrap(), "KEY_ENTER");
        assert_eq!(key_name_to_godot("esc").unwrap(), "KEY_ESCAPE");
        assert_eq!(key_name_to_godot("a").unwrap(), "KEY_A");
        assert_eq!(key_name_to_godot("Z").unwrap(), "KEY_Z");
        assert_eq!(key_name_to_godot("0").unwrap(), "KEY_0");
        assert_eq!(key_name_to_godot("f1").unwrap(), "KEY_F1");
        assert_eq!(key_name_to_godot("F12").unwrap(), "KEY_F12");
    }

    #[test]
    fn key_mapping_unknown() {
        assert!(key_name_to_godot("unknown").is_err());
        assert!(key_name_to_godot("f13").is_err());
    }

    #[test]
    fn mouse_button_valid() {
        assert_eq!(mouse_button_constant("left").unwrap(), "MOUSE_BUTTON_LEFT");
        assert_eq!(
            mouse_button_constant("RIGHT").unwrap(),
            "MOUSE_BUTTON_RIGHT"
        );
        assert_eq!(
            mouse_button_constant("Middle").unwrap(),
            "MOUSE_BUTTON_MIDDLE"
        );
    }

    #[test]
    fn mouse_button_invalid() {
        assert!(mouse_button_constant("back").is_err());
    }

    #[test]
    fn click_pos_contains_coordinates() {
        let script = generate_click_pos_script("42", "99", "left", false).unwrap();
        assert!(script.contains("Vector2(42, 99)"));
        assert!(script.contains("MOUSE_BUTTON_LEFT"));
    }

    #[test]
    fn click_node_find_child() {
        let script = generate_click_node_script("PlayButton", "left", false).unwrap();
        assert!(script.contains("find_child(\"PlayButton\""));
    }

    #[test]
    fn click_node_absolute_path() {
        let script = generate_click_node_script("/root/UI/Btn", "left", false).unwrap();
        assert!(script.contains("get_node_or_null(\"/root/UI/Btn\")"));
    }

    #[test]
    fn wait_frames_conversion() {
        // 60 frames × 16.667ms ≈ 1000ms
        let ms = (60.0_f64 * 16.667) as u64;
        assert_eq!(ms, 1000);
    }
}
