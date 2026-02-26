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
        .args(["query", "symbols", "player.gd", "--format", "json"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd query symbols");

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
            "query",
            "--no-godot-proxy",
            "hover",
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
        .expect("Failed to run gd query hover");

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
            "query",
            "--no-godot-proxy",
            "hover",
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
        .expect("Failed to run gd query hover");

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
            "query",
            "--no-godot-proxy",
            "hover",
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
        .expect("Failed to run gd query hover");

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
            "query",
            "--no-godot-proxy",
            "hover",
            "player.gd",
            "--line",
            "2",
            "--column",
            "2",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd query hover");

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
            "query",
            "--no-godot-proxy",
            "hover",
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
        .expect("Failed to run gd query hover");

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
            "query",
            "references",
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
        .expect("Failed to run gd query references");

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
            "query",
            "--no-godot-proxy",
            "definition",
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
        .expect("Failed to run gd query definition");

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
            "refactor",
            "rename",
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
        .expect("Failed to run gd refactor rename --dry-run");

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
            "refactor",
            "rename",
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
        .expect("Failed to run gd refactor rename");

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
            "refactor",
            "rename",
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
        .expect("Failed to run gd refactor rename");

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
            "query",
            "--no-godot-proxy",
            "completions",
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
        .expect("Failed to run gd query completions");

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
            "query",
            "--no-godot-proxy",
            "completions",
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
        .expect("Failed to run gd query completions --limit");

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
        .args(["lint", "--format", "json"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lint");

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
            "query",
            "code-actions",
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
        .expect("Failed to run gd query code-actions");

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
            "refactor",
            "rename",
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
        .expect("Failed to run gd refactor rename");

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
            "query",
            "references",
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
        .expect("Failed to run gd query references");

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
            "query",
            "--no-godot-proxy",
            "definition",
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
        .expect("Failed to run gd query definition");

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
        .args(["query", "references", "--name", "speed", "--format", "json"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd query references --name");

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
            "query",
            "references",
            "player.gd",
            "--name",
            "speed",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd query references --name --file");

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
            "query",
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
        .expect("Failed to run gd query references --name --class");

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
            "query",
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
        .expect("Failed to run gd query references --name --class inner");

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
            "query",
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
        .expect("Failed to run gd query references --class autoload");

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
            "query",
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
        .expect("Failed to run gd query references --class property");

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
            "query",
            "references",
            "--name",
            "nonexistent_symbol",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd query references --name nonexistent");

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
            "query",
            "references",
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
        .expect("Failed to run gd query references positional");

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
            "query",
            "symbols",
            "player.gd",
            "--kind",
            "function",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd query symbols --kind");

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
            "query",
            "symbols",
            "player.gd",
            "--kind",
            "function,constant",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd query symbols --kind multiple");

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
            "query",
            "symbols",
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
        .expect("Failed to run gd query symbols --kind repeatable");

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
            "query",
            "symbols",
            "player.gd",
            "--kind",
            "field",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd query symbols --kind field");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let arr = json.as_array().expect("should be array");
    assert_eq!(arr.len(), 2, "field alias should match var + onready var");
    let names: Vec<&str> = arr.iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"health"), "should include regular var");
    assert!(names.contains(&"label"), "should include @onready var");
}

// ─── Safe-delete-file tests ────────────────────────────────────────────────

#[test]
fn test_lsp_safe_delete_file_does_not_delete_without_force() {
    let temp = setup_gd_project(&[
        ("main.gd", "extends Node\n\nfunc _ready():\n\tpass\n"),
        ("helper.gd", "extends Node\n\nfunc help():\n\tpass\n"),
    ]);

    // Run safe-delete-file WITHOUT --force — should only report, not delete
    let output = gd_bin()
        .args([
            "refactor",
            "safe-delete-file",
            "helper.gd",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd refactor safe-delete-file");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["deleted"], false, "should NOT delete without --force");

    // File must still exist on disk
    assert!(
        temp.path().join("helper.gd").exists(),
        "file must NOT be deleted without --force flag"
    );
}

#[test]
fn test_lsp_safe_delete_file_does_not_delete_unreferenced_without_force() {
    // This is the exact bug scenario: unreferenced file was auto-deleted
    let temp = setup_gd_project(&[("orphan.gd", "extends Node\n\nfunc unused():\n\tpass\n")]);

    // No other file references orphan.gd — previously this would delete it!
    let output = gd_bin()
        .args([
            "refactor",
            "safe-delete-file",
            "orphan.gd",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd refactor safe-delete-file");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json["deleted"], false,
        "must NOT auto-delete unreferenced files"
    );
    assert!(
        temp.path().join("orphan.gd").exists(),
        "unreferenced file must NOT be deleted without --force"
    );
}

#[test]
fn test_lsp_safe_delete_file_deletes_with_force() {
    let temp = setup_gd_project(&[("deleteme.gd", "extends Node\n\nfunc bye():\n\tpass\n")]);

    let output = gd_bin()
        .args([
            "refactor",
            "safe-delete-file",
            "deleteme.gd",
            "--force",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd refactor safe-delete-file --force");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["deleted"], true, "should delete with --force");
    assert!(
        !temp.path().join("deleteme.gd").exists(),
        "file should be deleted with --force"
    );
}

#[test]
fn test_lsp_safe_delete_file_dry_run_with_force_does_not_delete() {
    let temp = setup_gd_project(&[("keepme.gd", "extends Node\n\nfunc stay():\n\tpass\n")]);

    let output = gd_bin()
        .args([
            "refactor",
            "safe-delete-file",
            "keepme.gd",
            "--force",
            "--dry-run",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd refactor safe-delete-file --force --dry-run");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["deleted"], false, "--dry-run should prevent deletion");
    assert!(
        temp.path().join("keepme.gd").exists(),
        "--dry-run should prevent deletion even with --force"
    );
}

#[test]
fn test_lsp_safe_delete_file_reports_references() {
    let temp = setup_gd_project(&[
        ("base.gd", "class_name Base\n\nfunc run():\n\tpass\n"),
        (
            "child.gd",
            "extends \"res://base.gd\"\n\nfunc run():\n\tprint(\"child\")\n",
        ),
    ]);

    let output = gd_bin()
        .args([
            "refactor",
            "safe-delete-file",
            "base.gd",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd refactor safe-delete-file");

    // Exit code 1 is expected when references exist (signals "unsafe to delete")
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("should output valid JSON");
    let refs = json["references"].as_array().unwrap();
    assert!(
        !refs.is_empty(),
        "should find extends reference from child.gd"
    );
    let files: Vec<&str> = refs.iter().filter_map(|r| r["file"].as_str()).collect();
    assert!(
        files.contains(&"child.gd"),
        "should reference child.gd, got: {files:?}"
    );
    assert_eq!(json["deleted"], false, "should not delete");
    assert!(temp.path().join("base.gd").exists());
}

// ─── Symbols detail tests ──────────────────────────────────────────────────

#[test]
fn test_lsp_symbols_detail_shows_declarations() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var health: int = 100\nconst MAX_HP = 200\nsignal died(player_name: String)\nenum State { IDLE, RUN, JUMP }\n\n\nfunc attack(target: Node, damage: int) -> void:\n\tpass\n",
    )]);

    let output = gd_bin()
        .args(["query", "symbols", "player.gd", "--format", "json"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd query symbols");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let arr = json.as_array().unwrap();
    let by_name: std::collections::HashMap<&str, &serde_json::Value> = arr
        .iter()
        .map(|s| (s["name"].as_str().unwrap(), s))
        .collect();

    // Function detail should show signature
    let attack = by_name["attack"];
    let detail = attack["detail"].as_str().unwrap_or("");
    assert!(
        detail.contains("target"),
        "func detail should show params, got: {detail}"
    );

    // Enum detail should show members
    let state = by_name["State"];
    let detail = state["detail"].as_str().unwrap_or("");
    assert!(
        detail.contains("IDLE"),
        "enum detail should show members, got: {detail}"
    );

    // Variable detail should show declaration
    let health = by_name["health"];
    let detail = health["detail"].as_str().unwrap_or("");
    assert!(
        detail.contains("var health"),
        "var detail should show declaration, got: {detail}"
    );
}

// ─── Cross-file hover tests ────────────────────────────────────────────────

#[test]
fn test_lsp_hover_cross_file_class_name() {
    let temp = setup_gd_project(&[
        (
            "player.gd",
            "class_name Player\nextends Node\n\nvar health := 100\n",
        ),
        ("game.gd", "var p: Player\n\nfunc run():\n\tprint(p)\n"),
    ]);

    // Hover on "Player" type annotation in game.gd (line 1, column 8)
    let output = gd_bin()
        .args([
            "query",
            "--no-godot-proxy",
            "hover",
            "game.gd",
            "--line",
            "1",
            "--column",
            "8",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd query hover");

    assert!(
        output.status.success(),
        "hover on cross-file class_name should work, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let content = json["content"].as_str().unwrap();
    assert!(
        content.contains("Player"),
        "should show class name, got: {content}"
    );
}

// ─── Find implementations tests ────────────────────────────────────────────

#[test]
fn test_lsp_find_implementations_method() {
    let temp = setup_gd_project(&[
        ("base.gd", "class_name Base\n\nfunc setup():\n\tpass\n"),
        (
            "child_a.gd",
            "extends Base\n\nfunc setup():\n\tprint(\"A\")\n",
        ),
        (
            "child_b.gd",
            "extends Base\n\nfunc setup():\n\tprint(\"B\")\n",
        ),
    ]);

    let output = gd_bin()
        .args([
            "query",
            "find-implementations",
            "--name",
            "setup",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd query find-implementations");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let impls = json.as_array().unwrap_or_else(|| {
        // May be wrapped in { "method": ..., "implementations": [...] }
        json["implementations"].as_array().unwrap()
    });
    assert!(
        impls.len() >= 3,
        "should find setup in base + 2 children, got {}",
        impls.len()
    );
}

#[test]
fn test_lsp_find_implementations_with_base_filter() {
    let temp = setup_gd_project(&[
        ("base.gd", "class_name Base\n\nfunc run():\n\tpass\n"),
        (
            "child.gd",
            "extends Base\n\nfunc run():\n\tprint(\"child\")\n",
        ),
        (
            "other.gd",
            "extends Node\n\nfunc run():\n\tprint(\"other\")\n",
        ),
    ]);

    let output = gd_bin()
        .args([
            "query",
            "find-implementations",
            "--name",
            "run",
            "--base",
            "Base",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd query find-implementations --base");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let impls = json
        .as_array()
        .unwrap_or_else(|| json["implementations"].as_array().unwrap());
    // Only child.gd extends Base — other.gd extends Node
    assert_eq!(
        impls.len(),
        1,
        "should only find child extending Base, got: {impls:?}"
    );
}

// ─── LSP protocol tests for new features ───────────────────────────────────

/// Helper: spawn LSP, send initialize, return (child, stdin, stdout)
fn spawn_lsp(
    temp: &tempfile::TempDir,
) -> (
    std::process::Child,
    std::process::ChildStdin,
    std::process::ChildStdout,
) {
    use std::process::Stdio;

    let mut child = Command::new(env!("CARGO_BIN_EXE_gd"))
        .arg("lsp")
        .current_dir(temp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to spawn gd lsp");

    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();

    (child, stdin, stdout)
}

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

fn lsp_initialize(
    stdin: &mut impl Write,
    stdout: &mut impl Read,
    root_uri: &str,
) -> serde_json::Value {
    let init = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": {},
            "rootUri": root_uri
        }
    });
    stdin.write_all(&lsp_msg(&init)).unwrap();
    stdin.flush().unwrap();
    let resp = read_lsp_response(stdout);

    // Send initialized notification
    let initialized = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "initialized",
        "params": {}
    });
    stdin.write_all(&lsp_msg(&initialized)).unwrap();
    stdin.flush().unwrap();

    resp
}

fn lsp_open_doc(stdin: &mut impl Write, uri: &str, content: &str) {
    let did_open = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": uri,
                "languageId": "gdscript",
                "version": 1,
                "text": content
            }
        }
    });
    stdin.write_all(&lsp_msg(&did_open)).unwrap();
    stdin.flush().unwrap();
}

#[allow(clippy::needless_pass_by_value)]
fn lsp_request(
    stdin: &mut impl Write,
    stdout: &mut impl Read,
    id: u64,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params
    });
    stdin.write_all(&lsp_msg(&req)).unwrap();
    stdin.flush().unwrap();

    // Read responses until we get the one with our id
    for _ in 0..10 {
        let resp = read_lsp_response(stdout);
        if resp.get("id") == Some(&serde_json::json!(id)) {
            return resp;
        }
        // Otherwise it's a notification (e.g. publishDiagnostics), skip it
    }
    panic!("Did not receive response for request id {id}");
}

#[test]
fn test_lsp_initialize_reports_new_capabilities() {
    let temp = setup_gd_project(&[("main.gd", "extends Node\n")]);
    let (mut child, mut stdin, mut stdout) = spawn_lsp(&temp);
    let root_uri = format!("file://{}", temp.path().display());

    let resp = lsp_initialize(&mut stdin, &mut stdout, &root_uri);
    let caps = &resp["result"]["capabilities"];

    // Verify new capabilities are registered
    assert_eq!(
        caps["inlayHintProvider"], true,
        "should advertise inlay hints"
    );
    assert!(
        caps["signatureHelpProvider"].is_object(),
        "should advertise signature help"
    );
    assert!(
        caps["callHierarchyProvider"].as_bool() == Some(true)
            || caps["callHierarchyProvider"].is_object(),
        "should advertise call hierarchy"
    );
    assert!(
        caps["implementationProvider"].as_bool() == Some(true)
            || caps["implementationProvider"].is_object(),
        "should advertise implementation provider"
    );
    assert!(
        caps["semanticTokensProvider"].is_object(),
        "should advertise semantic tokens"
    );
    assert!(
        caps["workspaceSymbolProvider"].as_bool() == Some(true)
            || caps["workspaceSymbolProvider"].is_object(),
        "should advertise workspace symbol provider"
    );

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn test_lsp_inlay_hints() {
    let temp = setup_gd_project(&[(
        "main.gd",
        "extends Node\n\nvar x := Vector2(1, 2)\nvar y := 42\nvar z: int = 10\n",
    )]);
    let (mut child, mut stdin, mut stdout) = spawn_lsp(&temp);
    let root_uri = format!("file://{}", temp.path().display());
    let doc_uri = format!("file://{}/main.gd", temp.path().display());

    lsp_initialize(&mut stdin, &mut stdout, &root_uri);
    std::thread::sleep(std::time::Duration::from_millis(200));

    lsp_open_doc(
        &mut stdin,
        &doc_uri,
        "extends Node\n\nvar x := Vector2(1, 2)\nvar y := 42\nvar z: int = 10\n",
    );
    std::thread::sleep(std::time::Duration::from_millis(200));

    let resp = lsp_request(
        &mut stdin,
        &mut stdout,
        10,
        "textDocument/inlayHint",
        serde_json::json!({
            "textDocument": { "uri": doc_uri },
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 10, "character": 0 }
            }
        }),
    );

    let result = &resp["result"];
    assert!(
        result.is_array(),
        "inlay hints should return array, got: {result}"
    );
    let hints = result.as_array().unwrap();
    // Should have hints for x (Vector2) and y (int) but NOT z (explicit type)
    assert!(
        !hints.is_empty(),
        "should have at least one inlay hint for inferred types"
    );

    // Check that hints contain type information
    let labels: Vec<String> = hints
        .iter()
        .filter_map(|h| {
            h["label"].as_str().map(String::from).or_else(|| {
                h["label"].as_array().map(|parts| {
                    parts
                        .iter()
                        .filter_map(|p| p["value"].as_str())
                        .collect::<Vec<_>>()
                        .join("")
                })
            })
        })
        .collect();
    assert!(
        labels.iter().any(|l| l.contains("Vector2")),
        "should have Vector2 type hint, got: {labels:?}"
    );

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn test_lsp_signature_help() {
    let source = "extends Node\n\nfunc add(a: int, b: int) -> int:\n\treturn a + b\n\nfunc _ready():\n\tadd(1, 2)\n";
    let temp = setup_gd_project(&[("main.gd", source)]);
    let (mut child, mut stdin, mut stdout) = spawn_lsp(&temp);
    let root_uri = format!("file://{}", temp.path().display());
    let doc_uri = format!("file://{}/main.gd", temp.path().display());

    lsp_initialize(&mut stdin, &mut stdout, &root_uri);
    std::thread::sleep(std::time::Duration::from_millis(200));

    lsp_open_doc(&mut stdin, &doc_uri, source);
    std::thread::sleep(std::time::Duration::from_millis(200));

    // Cursor after "add(" — line 6, character 5 (inside the call parens)
    let resp = lsp_request(
        &mut stdin,
        &mut stdout,
        11,
        "textDocument/signatureHelp",
        serde_json::json!({
            "textDocument": { "uri": doc_uri },
            "position": { "line": 6, "character": 5 }
        }),
    );

    let result = &resp["result"];
    assert!(
        !result.is_null(),
        "should return signature help inside function call, resp: {resp}"
    );
    let sigs = result["signatures"].as_array().unwrap();
    assert!(!sigs.is_empty(), "should have at least one signature");
    let label = sigs[0]["label"].as_str().unwrap();
    assert!(
        label.contains("add"),
        "signature label should contain function name, got: {label}"
    );
    assert!(
        label.contains("a: int"),
        "signature label should contain params, got: {label}"
    );

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn test_lsp_semantic_tokens() {
    let source =
        "extends Node\n\nvar speed: float = 10.0\nconst MAX := 100\n\nfunc run():\n\tpass\n";
    let temp = setup_gd_project(&[("main.gd", source)]);
    let (mut child, mut stdin, mut stdout) = spawn_lsp(&temp);
    let root_uri = format!("file://{}", temp.path().display());
    let doc_uri = format!("file://{}/main.gd", temp.path().display());

    lsp_initialize(&mut stdin, &mut stdout, &root_uri);
    std::thread::sleep(std::time::Duration::from_millis(200));

    lsp_open_doc(&mut stdin, &doc_uri, source);
    std::thread::sleep(std::time::Duration::from_millis(200));

    let resp = lsp_request(
        &mut stdin,
        &mut stdout,
        12,
        "textDocument/semanticTokens/full",
        serde_json::json!({
            "textDocument": { "uri": doc_uri }
        }),
    );

    let result = &resp["result"];
    assert!(
        !result.is_null(),
        "should return semantic tokens, resp: {resp}"
    );
    let data = result["data"].as_array().unwrap();
    // Semantic tokens data is encoded as groups of 5 integers
    assert!(
        data.len() >= 5,
        "should have at least one token (5 ints per token), got {} ints",
        data.len()
    );
    assert_eq!(
        data.len() % 5,
        0,
        "token data length should be multiple of 5"
    );

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn test_lsp_workspace_symbol() {
    let temp = setup_gd_project(&[
        (
            "player.gd",
            "class_name Player\n\nvar health := 100\n\nfunc attack():\n\tpass\n",
        ),
        (
            "enemy.gd",
            "class_name Enemy\n\nvar damage := 50\n\nfunc chase():\n\tpass\n",
        ),
    ]);
    let (mut child, mut stdin, mut stdout) = spawn_lsp(&temp);
    let root_uri = format!("file://{}", temp.path().display());

    lsp_initialize(&mut stdin, &mut stdout, &root_uri);
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Search for "attack"
    let resp = lsp_request(
        &mut stdin,
        &mut stdout,
        13,
        "workspace/symbol",
        serde_json::json!({
            "query": "attack"
        }),
    );

    let result = &resp["result"];
    assert!(
        result.is_array(),
        "workspace/symbol should return array, got: {resp}"
    );
    let symbols = result.as_array().unwrap();
    assert!(
        !symbols.is_empty(),
        "should find 'attack' symbol in workspace"
    );
    let names: Vec<&str> = symbols.iter().filter_map(|s| s["name"].as_str()).collect();
    assert!(
        names.contains(&"attack"),
        "should contain 'attack', got: {names:?}"
    );

    // Empty query should return all symbols
    let resp2 = lsp_request(
        &mut stdin,
        &mut stdout,
        14,
        "workspace/symbol",
        serde_json::json!({
            "query": ""
        }),
    );
    let all_symbols = resp2["result"].as_array().unwrap();
    assert!(
        all_symbols.len() >= 4,
        "empty query should return many symbols (Player, Enemy, health, damage, attack, chase), got {}",
        all_symbols.len()
    );

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn test_lsp_call_hierarchy_prepare() {
    let source = "extends Node\n\nfunc helper():\n\tpass\n\nfunc _ready():\n\thelper()\n";
    let temp = setup_gd_project(&[("main.gd", source)]);
    let (mut child, mut stdin, mut stdout) = spawn_lsp(&temp);
    let root_uri = format!("file://{}", temp.path().display());
    let doc_uri = format!("file://{}/main.gd", temp.path().display());

    lsp_initialize(&mut stdin, &mut stdout, &root_uri);
    std::thread::sleep(std::time::Duration::from_millis(200));

    lsp_open_doc(&mut stdin, &doc_uri, source);
    std::thread::sleep(std::time::Duration::from_millis(200));

    // Prepare call hierarchy on "helper" function definition (line 2, char 5)
    let resp = lsp_request(
        &mut stdin,
        &mut stdout,
        15,
        "textDocument/prepareCallHierarchy",
        serde_json::json!({
            "textDocument": { "uri": doc_uri },
            "position": { "line": 2, "character": 5 }
        }),
    );

    let result = &resp["result"];
    assert!(
        result.is_array(),
        "prepareCallHierarchy should return array, got: {resp}"
    );
    let items = result.as_array().unwrap();
    assert!(
        !items.is_empty(),
        "should find call hierarchy item for 'helper'"
    );
    assert_eq!(
        items[0]["name"].as_str().unwrap(),
        "helper",
        "should resolve to 'helper' function"
    );

    child.kill().ok();
    child.wait().ok();
}

// ── Enum member rename cross-file ───────────────────────────────────────────

#[test]
fn test_rename_enum_member_cross_file_qualified() {
    // Renaming an enum member should update qualified references in other files.
    let temp = setup_gd_project(&[
        (
            "types.gd",
            "class_name Types\n\nenum State { IDLE, RUNNING, DEAD }\n",
        ),
        (
            "player.gd",
            "extends Node\n\nfunc update():\n\tvar s = Types.State.IDLE\n\tif s == Types.State.RUNNING:\n\t\tpass\n",
        ),
    ]);

    let output = gd_bin()
        .args([
            "refactor",
            "rename",
            "types.gd",
            "--line",
            "3",
            "--column",
            "14",
            "--new-name",
            "WAITING",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd refactor rename");

    assert!(output.status.success());

    let types = fs::read_to_string(temp.path().join("types.gd")).unwrap();
    assert!(
        types.contains("WAITING"),
        "types.gd enum member should be renamed"
    );
    assert!(
        !types.contains("IDLE"),
        "types.gd should no longer contain IDLE"
    );

    let player = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        player.contains("Types.State.WAITING"),
        "player.gd qualified reference should be updated"
    );
    assert!(
        !player.contains("IDLE"),
        "player.gd should no longer contain IDLE"
    );
}

#[test]
fn test_rename_enum_member_cross_file_by_name() {
    // The --name flag should also find and rename qualified enum member refs.
    let temp = setup_gd_project(&[
        (
            "types.gd",
            "enum Direction { UP, DOWN, LEFT, RIGHT }\n",
        ),
        (
            "player.gd",
            "extends Node\n\nfunc move():\n\tvar dir = Direction.UP\n",
        ),
    ]);

    let output = gd_bin()
        .args([
            "refactor",
            "rename",
            "--name",
            "UP",
            "--new-name",
            "NORTH",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd refactor rename --name");

    assert!(output.status.success());

    let types = fs::read_to_string(temp.path().join("types.gd")).unwrap();
    assert!(
        types.contains("NORTH"),
        "types.gd enum member should be renamed"
    );

    let player = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        player.contains("Direction.NORTH"),
        "player.gd qualified reference should be updated"
    );
}
