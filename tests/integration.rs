use std::io::{Read, Write};
use std::process::Command;
use std::fs;
use tempfile::TempDir;

fn gd_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_gd"))
}

#[test]
fn test_new_creates_project() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let project_path = temp.path().join("test-proj");

    let output = gd_bin()
        .arg("new")
        .arg(&project_path)
        .output()
        .expect("Failed to run gd new");

    assert!(output.status.success(), "gd new should succeed");

    // Verify all expected files exist
    assert!(
        project_path.join("project.godot").exists(),
        "project.godot should exist"
    );
    assert!(
        project_path.join("main.gd").exists(),
        "main.gd should exist"
    );
    assert!(
        project_path.join("main.tscn").exists(),
        "main.tscn should exist"
    );
    assert!(
        project_path.join("gd.toml").exists(),
        "gd.toml should exist"
    );
    assert!(
        project_path.join(".gitignore").exists(),
        ".gitignore should exist"
    );

    // Verify project.godot contains the project name
    let project_godot = fs::read_to_string(project_path.join("project.godot"))
        .expect("Failed to read project.godot");
    assert!(
        project_godot.contains("test-proj"),
        "project.godot should contain project name"
    );
}

#[test]
fn test_fmt_check_clean_file() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("clean.gd");

    // Write a well-formatted file (using real tabs)
    fs::write(&file_path, "extends Node\n\n\nfunc _ready() -> void:\n\tpass\n")
        .expect("Failed to write file");

    let output = gd_bin()
        .arg("fmt")
        .arg("--check")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd fmt --check");

    assert_eq!(
        output.status.code(),
        Some(0),
        "gd fmt --check should return 0 for clean file"
    );
}

#[test]
fn test_fmt_check_unformatted_file() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("messy.gd");

    // Write an unformatted file (extra spaces, wrong indentation)
    fs::write(&file_path, "extends Node\n\n\n\n\nfunc _ready()->void:\n  pass\n")
        .expect("Failed to write file");

    let output = gd_bin()
        .arg("fmt")
        .arg("--check")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd fmt --check");

    assert_ne!(
        output.status.code(),
        Some(0),
        "gd fmt --check should return non-zero for unformatted file"
    );
}

#[test]
fn test_fmt_diff() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("diff_test.gd");

    // Write an unformatted file
    fs::write(&file_path, "extends Node\n\n\n\n\nfunc _ready()->void:\n  pass\n")
        .expect("Failed to write file");

    let output = gd_bin()
        .arg("fmt")
        .arg("--diff")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd fmt --diff");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("---") && stdout.contains("+++"),
        "gd fmt --diff should show unified diff markers"
    );
}

#[test]
fn test_lint_detects_issues() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("issues.gd");

    // Write a file with duplicate signals and bad naming
    fs::write(
        &file_path,
        "extends Node\n\nsignal died\nsignal died\n\nfunc BadName():\n\tpass\n"
    )
    .expect("Failed to write file");

    let output = gd_bin()
        .arg("lint")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint");

    assert_ne!(
        output.status.code(),
        Some(0),
        "gd lint should return non-zero for file with issues"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("duplicate-signal"),
        "Should detect duplicate-signal issue"
    );
    assert!(
        stderr.contains("naming-convention"),
        "Should detect naming-convention issue"
    );
}

#[test]
fn test_lint_clean_file() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("clean.gd");

    // Write a clean file
    fs::write(&file_path, "extends Node\n\n\nfunc _ready() -> void:\n\tpass\n")
        .expect("Failed to write file");

    let output = gd_bin()
        .arg("lint")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint");

    assert_eq!(
        output.status.code(),
        Some(0),
        "gd lint should return 0 for clean file"
    );
}

#[test]
fn test_lint_json_output() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("issues.gd");

    // Write a file with issues
    fs::write(
        &file_path,
        "extends Node\n\nfunc BadName():\n\tpass\n"
    )
    .expect("Failed to write file");

    let output = gd_bin()
        .arg("lint")
        .arg("--format")
        .arg("json")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint --format json");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Verify it's valid JSON by attempting to parse it
    let parse_result = serde_json::from_str::<serde_json::Value>(&stdout);
    assert!(
        parse_result.is_ok(),
        "gd lint --format json should output valid JSON"
    );
}

#[test]
fn test_lint_fix() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("fix_test.gd");

    // Write a file with naming convention issue
    fs::write(&file_path, "func BadName():\n\tpass\n")
        .expect("Failed to write file");

    let _output = gd_bin()
        .arg("lint")
        .arg("--fix")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint --fix");

    // Read the file back
    let fixed_content = fs::read_to_string(&file_path)
        .expect("Failed to read fixed file");

    assert!(
        fixed_content.contains("bad_name"),
        "Function name should be fixed to snake_case"
    );
    assert!(
        !fixed_content.contains("BadName"),
        "Old PascalCase name should be replaced"
    );
}

#[test]
fn test_completions_bash() {
    let output = gd_bin()
        .arg("completions")
        .arg("bash")
        .output()
        .expect("Failed to run gd completions bash");

    assert_eq!(
        output.status.code(),
        Some(0),
        "gd completions bash should succeed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("_gd"),
        "Bash completions should contain _gd function"
    );
}

#[test]
fn test_stats_output() {
    // Use a prefix that doesn't start with '.' to avoid being filtered as hidden
    let temp = tempfile::Builder::new()
        .prefix("gdtest")
        .tempdir()
        .expect("Failed to create temp dir");

    // Create a simple project structure
    fs::write(
        temp.path().join("project.godot"),
        "[application]\nconfig/name=\"test\"\n"
    )
    .expect("Failed to write project.godot");

    fs::write(
        temp.path().join("test.gd"),
        "extends Node\n\nfunc _ready() -> void:\n\tpass\n"
    )
    .expect("Failed to write test.gd");

    let output = gd_bin()
        .arg("stats")
        .arg(temp.path())
        .output()
        .expect("Failed to run gd stats");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!(
            "gd stats failed with exit code {:?}:\nstderr: {}",
            output.status.code(),
            stderr
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Files:"),
        "Stats output should contain 'Files:', got: {}",
        stdout
    );
    assert!(
        stdout.contains("Functions:"),
        "Stats output should contain 'Functions:', got: {}",
        stdout
    );
}

#[test]
fn test_lint_suppression_ignore() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("suppress.gd");

    // Write a file with a naming issue suppressed on the same line
    fs::write(
        &file_path,
        "func BadName():  # gd:ignore[naming-convention]\n\tpass\n"
    )
    .expect("Failed to write file");

    let output = gd_bin()
        .arg("lint")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("naming-convention"),
        "naming-convention should be suppressed by # gd:ignore"
    );
}

#[test]
fn test_lint_suppression_ignore_next_line() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("suppress_next.gd");

    // gd:ignore-next-line suppresses the following line
    fs::write(
        &file_path,
        "# gd:ignore-next-line[naming-convention]\nfunc BadName():\n\tpass\n"
    )
    .expect("Failed to write file");

    let output = gd_bin()
        .arg("lint")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("naming-convention"),
        "naming-convention should be suppressed by # gd:ignore-next-line"
    );
}

#[test]
fn test_lint_sarif_output() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("sarif_test.gd");

    fs::write(&file_path, "func BadName():\n\tpass\n")
        .expect("Failed to write file");

    let output = gd_bin()
        .arg("lint")
        .arg("--format")
        .arg("sarif")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint --format sarif");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let sarif: serde_json::Value = serde_json::from_str(&stdout)
        .expect("SARIF output should be valid JSON");

    assert_eq!(sarif["version"], "2.1.0", "SARIF version should be 2.1.0");
    assert!(
        sarif["runs"][0]["tool"]["driver"]["name"] == "gd",
        "SARIF tool name should be gd"
    );
    assert!(
        sarif["runs"][0]["results"].as_array().unwrap().len() > 0,
        "SARIF should contain results"
    );
}

#[test]
fn test_lsp_initialize() {
    use std::process::Stdio;

    fn lsp_msg(data: &serde_json::Value) -> Vec<u8> {
        let body = serde_json::to_string(data).unwrap();
        format!("Content-Length: {}\r\n\r\n{}", body.len(), body)
            .into_bytes()
    }

    fn read_lsp_response(stdout: &mut impl Read) -> serde_json::Value {
        let mut header = Vec::new();
        let mut buf = [0u8; 1];
        while !header.ends_with(b"\r\n\r\n") {
            stdout.read_exact(&mut buf).expect("Failed to read header byte");
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
}
