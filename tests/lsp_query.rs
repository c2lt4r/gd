mod common;

use std::fs;
use std::io::{Read, Write};
use std::process::Command;

use common::{gd_bin, setup_gd_project};

#[test]
fn test_lsp_initialize() {
    use std::process::Stdio;

    fn lsp_msg(data: &serde_json::Value) -> Vec<u8> {
        let body = serde_json::to_string(data).unwrap();
        format!("Content-Length: {}\r\n\r\n{}", body.len(), body).into_bytes()
    }

    fn read_lsp_response(stdout: &mut impl Read) -> serde_json::Value {
        let mut header = Vec::new();
        let mut buf = [0u8; 1];
        while !header.ends_with(b"\r\n\r\n") {
            stdout
                .read_exact(&mut buf)
                .expect("Failed to read header byte");
            header.push(buf[0]);
        }
        let header_str = String::from_utf8_lossy(&header);
        let length: usize = header_str
            .lines()
            .find_map(|l| l.strip_prefix("Content-Length: "))
            .expect("Missing Content-Length")
            .trim()
            .parse()
            .expect("Invalid Content-Length");
        let mut body = vec![0u8; length];
        stdout.read_exact(&mut body).expect("Failed to read body");
        serde_json::from_slice(&body).expect("Invalid JSON response")
    }

    let mut child = Command::new(env!("CARGO_BIN_EXE_gd"))
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to start gd lsp");

    let stdin = child.stdin.as_mut().unwrap();
    let stdout = child.stdout.as_mut().unwrap();

    // Send initialize request
    let init = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "capabilities": {} }
    });
    stdin.write_all(&lsp_msg(&init)).unwrap();
    stdin.flush().unwrap();

    // Read response
    let resp = read_lsp_response(stdout);

    // Verify LSP spec compliance
    assert_eq!(resp["jsonrpc"], "2.0");
    assert_eq!(resp["id"], 1);

    let caps = &resp["result"]["capabilities"];
    assert_eq!(caps["textDocumentSync"], 1, "Should use FULL sync");
    assert_eq!(caps["documentFormattingProvider"], true);
    assert_eq!(caps["codeActionProvider"], true);
    assert_eq!(caps["documentSymbolProvider"], true);

    let info = &resp["result"]["serverInfo"];
    assert_eq!(info["name"], "gd-lsp");

    // Clean up
    child.kill().ok();
    child.wait().ok();
}

// ─── LSP formatting ────────────────────────────────────────────────────────

#[test]
#[allow(clippy::too_many_lines)]
fn test_lsp_formatting() {
    use std::process::Stdio;

    fn lsp_msg(data: &serde_json::Value) -> Vec<u8> {
        let body = serde_json::to_string(data).unwrap();
        format!("Content-Length: {}\r\n\r\n{}", body.len(), body).into_bytes()
    }

    fn read_lsp_response(stdout: &mut impl Read) -> serde_json::Value {
        let mut header = Vec::new();
        let mut buf = [0u8; 1];
        while !header.ends_with(b"\r\n\r\n") {
            stdout
                .read_exact(&mut buf)
                .expect("Failed to read header byte");
            header.push(buf[0]);
        }
        let header_str = String::from_utf8_lossy(&header);
        let length: usize = header_str
            .lines()
            .find_map(|l| l.strip_prefix("Content-Length: "))
            .expect("Missing Content-Length")
            .trim()
            .parse()
            .expect("Invalid Content-Length");
        let mut body = vec![0u8; length];
        stdout.read_exact(&mut body).expect("Failed to read body");
        serde_json::from_slice(&body).expect("Invalid JSON response")
    }

    let mut child = Command::new(env!("CARGO_BIN_EXE_gd"))
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to start gd lsp");

    let stdin = child.stdin.as_mut().unwrap();
    let stdout = child.stdout.as_mut().unwrap();

    // 1) Initialize
    let init = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "capabilities": {} }
    });
    stdin.write_all(&lsp_msg(&init)).unwrap();
    stdin.flush().unwrap();
    let _init_resp = read_lsp_response(stdout);

    // 2) Initialized notification
    let initialized = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "initialized",
        "params": {}
    });
    stdin.write_all(&lsp_msg(&initialized)).unwrap();
    stdin.flush().unwrap();

    // 3) Open a document with unformatted code
    let doc_uri = "file:///tmp/test_format.gd";
    let unformatted = "extends Node\n\n\n\n\nfunc _ready()->void:\n  pass\n";
    let did_open = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": doc_uri,
                "languageId": "gdscript",
                "version": 1,
                "text": unformatted
            }
        }
    });
    stdin.write_all(&lsp_msg(&did_open)).unwrap();
    stdin.flush().unwrap();

    // The server may send publishDiagnostics notification — read it
    // (we need to drain any notification before our formatting response)
    // Small sleep to let the server process
    std::thread::sleep(std::time::Duration::from_millis(200));

    // 4) Request formatting
    let format_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "textDocument/formatting",
        "params": {
            "textDocument": { "uri": doc_uri },
            "options": {
                "tabSize": 4,
                "insertSpaces": false
            }
        }
    });
    stdin.write_all(&lsp_msg(&format_req)).unwrap();
    stdin.flush().unwrap();

    // Read responses until we get our formatting response (id: 2)
    let mut format_resp = None;
    for _ in 0..5 {
        let resp = read_lsp_response(stdout);
        if resp.get("id") == Some(&serde_json::json!(2)) {
            format_resp = Some(resp);
            break;
        }
        // Otherwise it's a notification (e.g. publishDiagnostics), skip it
    }

    let format_resp = format_resp.expect("Should receive formatting response with id: 2");
    assert_eq!(format_resp["jsonrpc"], "2.0");
    assert_eq!(format_resp["id"], 2);

    // The result should be an array of TextEdit
    let result = &format_resp["result"];
    assert!(
        result.is_array(),
        "Formatting result should be an array of TextEdit, got: {result}"
    );

    let edits = result.as_array().unwrap();
    assert!(
        !edits.is_empty(),
        "Formatting should produce at least one TextEdit for unformatted code"
    );

    // Each edit should have range and newText
    for edit in edits {
        assert!(edit.get("range").is_some(), "TextEdit should have range");
        assert!(
            edit.get("newText").is_some(),
            "TextEdit should have newText"
        );
    }

    child.kill().ok();
    child.wait().ok();
}

// ─── LSP query subcommand tests ─────────────────────────────────────────────

#[test]
fn test_lsp_symbols_lists_all_declarations() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var health := 100\nconst MAX_HP = 200\nsignal died\nenum State { IDLE, RUN }\n\n\nfunc attack() -> void:\n\tpass\n",
    )]);

    let output = gd_bin()
        .args(["lsp", "symbols", "--file", "player.gd", "--format", "json"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp symbols");

    assert!(output.status.success(), "should exit 0");
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("should output valid JSON");
    let arr = json.as_array().expect("should be an array");

    let names: Vec<&str> = arr.iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"health"));
    assert!(names.contains(&"MAX_HP"));
    assert!(names.contains(&"died"));
    assert!(names.contains(&"State"));
    assert!(names.contains(&"attack"));

    // Verify 1-based line numbers
    let health = arr.iter().find(|s| s["name"] == "health").unwrap();
    assert_eq!(health["line"], 1);
    assert_eq!(health["kind"], "variable");

    // Verify distinct kinds for const and enum
    let max_hp = arr.iter().find(|s| s["name"] == "MAX_HP").unwrap();
    assert_eq!(max_hp["kind"], "constant");
    let state = arr.iter().find(|s| s["name"] == "State").unwrap();
    assert_eq!(state["kind"], "enum");
}

#[test]
fn test_lsp_hover_shows_function_signature() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "func move(speed: float, dir: Vector2) -> void:\n\tpass\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "--no-godot-proxy",
            "hover",
            "--file",
            "player.gd",
            "--line",
            "1",
            "--column",
            "6",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp hover");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let content = json["content"].as_str().unwrap();
    assert!(
        content.contains("func move"),
        "hover should show function name, got: {content}"
    );
    assert!(
        content.contains("speed: float"),
        "hover should show parameters, got: {content}"
    );
}

#[test]
fn test_lsp_hover_attribute_member_shows_builtin_docs() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var projectile: Node2D\n\nfunc shoot() -> void:\n\tprojectile.global_position = Vector2.ZERO\n",
    )]);

    // Hover on "global_position" (line 4, column 15 is within "global_position")
    let output = gd_bin()
        .args([
            "lsp",
            "--no-godot-proxy",
            "hover",
            "--file",
            "player.gd",
            "--line",
            "4",
            "--column",
            "15",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp hover");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let content = json["content"].as_str().unwrap();
    assert!(
        content.contains("global_position"),
        "hover should show member name, got: {content}"
    );
    assert!(
        content.contains("Node2D"),
        "hover should show class name, got: {content}"
    );
}

#[test]
fn test_lsp_hover_self_member_resolves_to_declaration() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var speed: float = 10.0\n\nfunc run() -> void:\n\tself.speed = 20.0\n",
    )]);

    // Hover on "speed" in self.speed (line 4, column 7)
    let output = gd_bin()
        .args([
            "lsp",
            "--no-godot-proxy",
            "hover",
            "--file",
            "player.gd",
            "--line",
            "4",
            "--column",
            "7",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp hover");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let content = json["content"].as_str().unwrap();
    assert!(
        content.contains("var speed"),
        "hover on self.speed should show var declaration, got: {content}"
    );
}

#[test]
fn test_lsp_hover_unresolvable_identifier_returns_no_hover() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "func run() -> void:\n\tsome_unknown_var = 10\n",
    )]);

    // Hover on "some_unknown_var" (line 2, column 2)
    let output = gd_bin()
        .args([
            "lsp",
            "--no-godot-proxy",
            "hover",
            "--file",
            "player.gd",
            "--line",
            "2",
            "--column",
            "2",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp hover");

    // Should fail (no hover info) instead of showing enclosing function
    assert!(
        !output.status.success(),
        "hover on unresolvable identifier should return no hover"
    );
}

#[test]
fn test_lsp_hover_on_function_name_still_works() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "func move(speed: float, dir: Vector2) -> void:\n\tpass\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "--no-godot-proxy",
            "hover",
            "--file",
            "player.gd",
            "--line",
            "1",
            "--column",
            "6",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp hover");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let content = json["content"].as_str().unwrap();
    assert!(
        content.contains("func move"),
        "hover on function name should still work, got: {content}"
    );
}

#[test]
fn test_lsp_references_finds_all_usages() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var speed = 10\n\n\nfunc run() -> void:\n\tprint(speed)\n\tspeed = 20\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "references",
            "--file",
            "player.gd",
            "--line",
            "1",
            "--column",
            "5",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp references");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["symbol"], "speed");
    let refs = json["references"].as_array().unwrap();
    // Declaration + 2 usages = 3
    assert_eq!(refs.len(), 3, "should find declaration + 2 usages");
    // All references should point to the same file
    for r in refs {
        assert_eq!(r["file"], "player.gd");
    }
}

#[test]
fn test_lsp_definition_jumps_to_declaration() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var speed = 10\n\n\nfunc run() -> void:\n\tprint(speed)\n",
    )]);

    // Ask for definition of `speed` on the usage line (line 5, inside print)
    let output = gd_bin()
        .args([
            "lsp",
            "--no-godot-proxy",
            "definition",
            "--file",
            "player.gd",
            "--line",
            "5",
            "--column",
            "8",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp definition");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["symbol"], "speed");
    assert_eq!(json["file"], "player.gd");
    assert_eq!(json["line"], 1, "definition should be on line 1");
}

#[test]
fn test_lsp_rename_dry_run_does_not_modify_file() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var speed = 10\n\n\nfunc run() -> void:\n\tspeed = 20\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "rename",
            "--file",
            "player.gd",
            "--line",
            "1",
            "--column",
            "5",
            "--new-name",
            "velocity",
            "--dry-run",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp rename --dry-run");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["symbol"], "speed");
    assert_eq!(json["new_name"], "velocity");
    let changes = json["changes"].as_array().unwrap();
    assert!(!changes.is_empty(), "should have edit entries");

    // File should NOT be modified
    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        content.contains("var speed"),
        "dry-run should not modify file on disk"
    );
}

#[test]
fn test_lsp_rename_applies_changes_to_disk() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var speed = 10\n\n\nfunc run() -> void:\n\tprint(speed)\n\tspeed = 20\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "rename",
            "--file",
            "player.gd",
            "--line",
            "1",
            "--column",
            "5",
            "--new-name",
            "velocity",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp rename");

    assert!(output.status.success());

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        !content.contains("speed"),
        "all occurrences of 'speed' should be renamed"
    );
    assert!(
        content.contains("var velocity"),
        "declaration should be renamed"
    );
    assert!(
        content.contains("print(velocity)"),
        "usage in print should be renamed"
    );
    assert!(
        content.contains("velocity = 20"),
        "assignment should be renamed"
    );
}

#[test]
fn test_lsp_rename_cross_file() {
    let temp = setup_gd_project(&[
        (
            "base.gd",
            "var speed = 10\n\n\nfunc get_speed() -> int:\n\treturn speed\n",
        ),
        (
            "child.gd",
            "var speed = 5\n\n\nfunc run() -> void:\n\tspeed = 20\n",
        ),
    ]);

    let output = gd_bin()
        .args([
            "lsp",
            "rename",
            "--file",
            "base.gd",
            "--line",
            "1",
            "--column",
            "5",
            "--new-name",
            "velocity",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp rename");

    assert!(output.status.success());

    // base.gd should be renamed
    let base = fs::read_to_string(temp.path().join("base.gd")).unwrap();
    assert!(base.contains("var velocity"), "base.gd should be renamed");

    // child.gd has its own `speed` — cross-file rename finds matching identifiers
    let child = fs::read_to_string(temp.path().join("child.gd")).unwrap();
    // Both files have `speed` as a top-level identifier, so cross-file rename affects both
    assert!(
        child.contains("velocity"),
        "cross-file rename should affect matching symbols in other files"
    );
}

#[test]
fn test_lsp_completions_includes_keywords_and_symbols() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var health := 100\n\n\nfunc attack() -> void:\n\tpass\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "--no-godot-proxy",
            "completions",
            "--file",
            "player.gd",
            "--line",
            "5",
            "--column",
            "1",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp completions");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let arr = json.as_array().expect("completions should be an array");
    let labels: Vec<&str> = arr.iter().map(|c| c["label"].as_str().unwrap()).collect();

    // Should include keywords
    assert!(labels.contains(&"func"), "should include keyword 'func'");
    assert!(labels.contains(&"var"), "should include keyword 'var'");
    // Should include file symbols
    assert!(
        labels.contains(&"health"),
        "should include variable 'health'"
    );
    assert!(
        labels.contains(&"attack"),
        "should include function 'attack'"
    );
    // Should include builtins
    assert!(labels.contains(&"print"), "should include builtin 'print'");

    // Verify kind field is present
    let func_item = arr.iter().find(|c| c["label"] == "func").unwrap();
    assert_eq!(func_item["kind"], "keyword");
}

#[test]
fn test_lsp_completions_limit_caps_results() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var health := 100\n\n\nfunc attack() -> void:\n\tpass\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "--no-godot-proxy",
            "completions",
            "--file",
            "player.gd",
            "--line",
            "5",
            "--column",
            "1",
            "--limit",
            "3",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp completions --limit");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let arr = json.as_array().expect("completions should be an array");
    assert_eq!(arr.len(), 3, "should cap at 3 results");
}

#[test]
fn test_lsp_diagnostics_outputs_json() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var x = 10\n\n\nfunc test() -> void:\n\tprint(x)\n",
    )]);

    let output = gd_bin()
        .args(["lsp", "diagnostics", "--format", "json"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp diagnostics");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let arr = json.as_array().expect("diagnostics should be an array");
    // Should have at least one file result
    assert!(!arr.is_empty(), "should produce diagnostics");
    // Each entry has file + diagnostics
    let first = &arr[0];
    assert!(first["file"].is_string());
    assert!(first["diagnostics"].is_array());
}

#[test]
fn test_lsp_code_actions_returns_fixes() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "extends Node\n\n\nfunc test() -> void:\n\tif x == true:\n\t\tpass\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "code-actions",
            "--file",
            "player.gd",
            "--line",
            "5",
            "--column",
            "1",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp code-actions");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let arr = json.as_array().expect("code-actions should be an array");
    assert!(
        !arr.is_empty(),
        "should have at least one code action for == true"
    );
    let action = &arr[0];
    assert!(
        action["title"]
            .as_str()
            .unwrap()
            .contains("comparison-with-boolean"),
        "should reference the rule name"
    );
    assert!(action["edits"].is_array());
}

#[test]
fn test_lsp_server_still_starts_without_subcommand() {
    // `gd lsp` (no subcommand) should start the LSP server.
    // We verify by sending an initialize request and getting a response.
    use std::process::Stdio;

    let temp = setup_gd_project(&[("main.gd", "extends Node\n")]);

    let mut child = Command::new(env!("CARGO_BIN_EXE_gd"))
        .arg("lsp")
        .current_dir(temp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to spawn gd lsp");

    let stdin = child.stdin.as_mut().unwrap();
    let init_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": {},
            "rootUri": format!("file://{}", temp.path().display())
        }
    });
    let body = serde_json::to_string(&init_req).unwrap();
    let msg = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
    stdin.write_all(msg.as_bytes()).unwrap();
    stdin.flush().unwrap();

    // Read the response header + body
    let stdout = child.stdout.as_mut().unwrap();
    let mut header = Vec::new();
    let mut buf = [0u8; 1];
    while !header.ends_with(b"\r\n\r\n") {
        stdout.read_exact(&mut buf).expect("read header byte");
        header.push(buf[0]);
    }
    let header_str = String::from_utf8_lossy(&header);
    let length: usize = header_str
        .lines()
        .find_map(|l| l.strip_prefix("Content-Length: "))
        .expect("Content-Length header")
        .parse()
        .expect("parse length");
    let mut body_buf = vec![0u8; length];
    stdout.read_exact(&mut body_buf).expect("read body");

    let resp: serde_json::Value = serde_json::from_slice(&body_buf).unwrap();
    assert_eq!(resp["id"], 1);
    assert!(resp["result"]["capabilities"].is_object());

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn test_lsp_rename_local_variable_scoped() {
    // Two functions each with `var x` — renaming `x` in foo should not affect bar
    let temp = setup_gd_project(&[(
        "player.gd",
        "func foo():\n\tvar x = 1\n\tprint(x)\n\tx = 2\n\n\nfunc bar():\n\tvar x = 10\n\tprint(x)\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "rename",
            "--file",
            "player.gd",
            "--line",
            "3", // print(x) in foo
            "--column",
            "8", // on `x`
            "--new-name",
            "y",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp rename");

    assert!(output.status.success());

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    // foo's x should be renamed to y
    assert!(
        content.contains("var y = 1"),
        "foo's var x should be renamed"
    );
    assert!(
        content.contains("print(y)"),
        "foo's print(x) should be renamed"
    );
    // bar's x should be untouched
    assert!(
        content.contains("var x = 10"),
        "bar's var x should NOT be renamed"
    );
}

#[test]
fn test_lsp_references_local_variable_scoped() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "func foo(speed):\n\tprint(speed)\n\n\nfunc bar(speed):\n\tspeed = 20\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "references",
            "--file",
            "player.gd",
            "--line",
            "2", // print(speed) in foo
            "--column",
            "8", // on `speed`
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp references");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let refs = json["references"].as_array().unwrap();
    // Should only find refs in foo: param `speed` + print(speed) = 2
    assert_eq!(
        refs.len(),
        2,
        "should find 2 refs in foo() only, got {}",
        refs.len()
    );
    // All should be on lines 1-2
    for r in refs {
        let line = r["line"].as_u64().unwrap();
        assert!(line <= 2, "all refs should be in foo(), got line {line}");
    }
}

#[test]
fn test_lsp_definition_local_variable() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var speed = 10\n\n\nfunc foo():\n\tvar speed = 20\n\tprint(speed)\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "--no-godot-proxy",
            "definition",
            "--file",
            "player.gd",
            "--line",
            "6", // print(speed) in foo
            "--column",
            "8", // on `speed`
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp definition");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // Should jump to local var speed on line 5, not global var speed on line 1
    assert_eq!(json["line"], 5, "should jump to local var, not global");
}

// ─── Name-based references tests ─────────────────────────────────────────

#[test]
fn test_lsp_references_by_name() {
    let temp = setup_gd_project(&[
        (
            "player.gd",
            "var speed = 10\n\n\nfunc run() -> void:\n\tprint(speed)\n\tspeed = 20\n",
        ),
        (
            "enemy.gd",
            "var speed = 5\n\n\nfunc chase() -> void:\n\tspeed = 30\n",
        ),
    ]);

    let output = gd_bin()
        .args(["lsp", "references", "--name", "speed", "--format", "json"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp references --name");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["symbol"], "speed");
    let refs = json["references"].as_array().unwrap();
    // player.gd: var speed + print(speed) + speed = 20 = 3
    // enemy.gd: var speed + speed = 30 = 2
    assert_eq!(refs.len(), 5, "should find 5 total refs across both files");
}

#[test]
fn test_lsp_references_by_name_with_file_filter() {
    let temp = setup_gd_project(&[
        (
            "player.gd",
            "var speed = 10\n\n\nfunc run() -> void:\n\tspeed = 20\n",
        ),
        (
            "enemy.gd",
            "var speed = 5\n\n\nfunc chase() -> void:\n\tspeed = 30\n",
        ),
    ]);

    let output = gd_bin()
        .args([
            "lsp",
            "references",
            "--name",
            "speed",
            "--file",
            "player.gd",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp references --name --file");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let refs = json["references"].as_array().unwrap();
    // Only player.gd: var speed + speed = 20 = 2
    assert_eq!(refs.len(), 2, "should only find refs in player.gd");
    for r in refs {
        assert_eq!(r["file"], "player.gd");
    }
}

#[test]
fn test_lsp_references_by_name_with_class_filter() {
    let temp = setup_gd_project(&[
        (
            "player.gd",
            "class_name Player\n\nvar speed = 10\n\n\nfunc run() -> void:\n\tspeed = 20\n",
        ),
        (
            "enemy.gd",
            "class_name Enemy\n\nvar speed = 5\n\n\nfunc chase() -> void:\n\tspeed = 30\n",
        ),
    ]);

    let output = gd_bin()
        .args([
            "lsp",
            "references",
            "--name",
            "speed",
            "--class",
            "Player",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp references --name --class");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let refs = json["references"].as_array().unwrap();
    // Only Player class (player.gd): var speed + speed = 20 = 2
    assert_eq!(
        refs.len(),
        2,
        "should only find refs in Player class, got {refs:?}"
    );
    for r in refs {
        assert_eq!(r["file"], "player.gd");
    }
}

#[test]
fn test_lsp_references_by_name_inner_class() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var speed = 10\n\n\nclass Stats:\n\tvar speed = 5\n\n\tfunc get_speed() -> int:\n\t\treturn speed\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "references",
            "--name",
            "speed",
            "--class",
            "Stats",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp references --name --class inner");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let refs = json["references"].as_array().unwrap();
    // Only inside class Stats: var speed + return speed = 2
    assert_eq!(
        refs.len(),
        2,
        "should only find refs inside inner class Stats, got {refs:?}"
    );
}

#[test]
fn test_lsp_references_by_name_class_finds_autoload_callers() {
    let temp = setup_gd_project(&[
        (
            "game_manager.gd",
            "class_name GameManager\n\nvar score = 0\n\n\nfunc submit_vote(choice: int) -> void:\n\tscore += choice\n",
        ),
        (
            "lobby_screen.gd",
            "func _on_button_pressed() -> void:\n\tGameManager.submit_vote(1)\n",
        ),
        (
            "hud.gd",
            "func update() -> void:\n\tvar s = GameManager.score\n",
        ),
    ]);

    // Should find: declaration in game_manager.gd + caller in lobby_screen.gd
    let output = gd_bin()
        .args([
            "lsp",
            "references",
            "--name",
            "submit_vote",
            "--class",
            "GameManager",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp references --class autoload");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let refs = json["references"].as_array().unwrap();

    // Should find: func declaration + score += choice (in game_manager.gd) + GameManager.submit_vote() (in lobby_screen.gd)
    let files: Vec<&str> = refs.iter().filter_map(|r| r["file"].as_str()).collect();
    assert!(
        files.contains(&"lobby_screen.gd"),
        "should find autoload caller in lobby_screen.gd, got: {files:?}"
    );
    assert!(
        files.contains(&"game_manager.gd"),
        "should find declaration in game_manager.gd, got: {files:?}"
    );
    assert!(
        !files.contains(&"hud.gd"),
        "should not include hud.gd (different member), got: {files:?}"
    );
}

#[test]
fn test_lsp_references_by_name_class_finds_property_access() {
    let temp = setup_gd_project(&[
        (
            "game_manager.gd",
            "class_name GameManager\n\nvar score = 0\n",
        ),
        (
            "hud.gd",
            "func update() -> void:\n\tvar s = GameManager.score\n",
        ),
    ]);

    let output = gd_bin()
        .args([
            "lsp",
            "references",
            "--name",
            "score",
            "--class",
            "GameManager",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp references --class property");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let refs = json["references"].as_array().unwrap();

    let files: Vec<&str> = refs.iter().filter_map(|r| r["file"].as_str()).collect();
    assert!(
        files.contains(&"hud.gd"),
        "should find GameManager.score access in hud.gd, got: {files:?}"
    );
}

#[test]
fn test_lsp_references_by_name_no_match() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var health = 100\n\n\nfunc hit() -> void:\n\thealth -= 10\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "references",
            "--name",
            "nonexistent_symbol",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp references --name nonexistent");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let refs = json["references"].as_array().unwrap();
    assert_eq!(refs.len(), 0, "should find 0 refs for nonexistent symbol");
}

#[test]
fn test_lsp_references_position_mode_still_works() {
    // Existing position-based mode should still work
    let temp = setup_gd_project(&[(
        "player.gd",
        "var speed = 10\n\n\nfunc run() -> void:\n\tprint(speed)\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "references",
            "--file",
            "player.gd",
            "--line",
            "1",
            "--column",
            "5",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp references positional");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["symbol"], "speed");
    let refs = json["references"].as_array().unwrap();
    assert!(refs.len() >= 2, "position-based mode should still work");
}

#[test]
fn test_lsp_symbols_kind_filter() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var health := 100\nconst MAX_HP = 200\nsignal died\nenum State { IDLE, RUN }\n\n\nfunc attack() -> void:\n\tpass\n",
    )]);

    // Filter by function only
    let output = gd_bin()
        .args([
            "lsp",
            "symbols",
            "--file",
            "player.gd",
            "--kind",
            "function",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp symbols --kind");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let arr = json.as_array().expect("should be array");
    assert_eq!(arr.len(), 1, "should only find 1 function");
    assert_eq!(arr[0]["name"], "attack");
    assert_eq!(arr[0]["kind"], "function");
}

#[test]
fn test_lsp_symbols_kind_filter_multiple() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var health := 100\nconst MAX_HP = 200\nsignal died\nenum State { IDLE, RUN }\n\n\nfunc attack() -> void:\n\tpass\n",
    )]);

    // Filter by function and constant (comma-separated)
    let output = gd_bin()
        .args([
            "lsp",
            "symbols",
            "--file",
            "player.gd",
            "--kind",
            "function,constant",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp symbols --kind multiple");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let arr = json.as_array().expect("should be array");
    assert_eq!(arr.len(), 2, "should find function + constant");
    let names: Vec<&str> = arr.iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"attack"));
    assert!(names.contains(&"MAX_HP"));
}

#[test]
fn test_lsp_symbols_kind_filter_repeatable() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var health := 100\nconst MAX_HP = 200\n\n\nfunc attack() -> void:\n\tpass\n",
    )]);

    // Filter by function and variable (repeated --kind)
    let output = gd_bin()
        .args([
            "lsp",
            "symbols",
            "--file",
            "player.gd",
            "--kind",
            "function",
            "--kind",
            "variable",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp symbols --kind repeatable");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let arr = json.as_array().expect("should be array");
    assert_eq!(arr.len(), 2, "should find function + variable");
    let names: Vec<&str> = arr.iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"attack"));
    assert!(names.contains(&"health"));
}

#[test]
fn test_lsp_symbols_kind_field_alias() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var health := 100\n@onready var label = $Label\nconst MAX_HP = 200\n\n\nfunc attack() -> void:\n\tpass\n",
    )]);

    // "field" should match both "variable" and "field" (onready)
    let output = gd_bin()
        .args([
            "lsp",
            "symbols",
            "--file",
            "player.gd",
            "--kind",
            "field",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp symbols --kind field");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let arr = json.as_array().expect("should be array");
    assert_eq!(arr.len(), 2, "field alias should match var + onready var");
    let names: Vec<&str> = arr.iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"health"), "should include regular var");
    assert!(names.contains(&"label"), "should include @onready var");
}
