use std::io::Write;
use std::path::{Path, PathBuf};

use crate::cli::test_cmd::gdunit::{
    build_gdunit4_filter_args, decode_xml_entities, extract_xml_attr, parse_gdunit4_xml,
};
use crate::cli::test_cmd::gut::parse_gut_counts;
use crate::cli::test_cmd::{
    TestError, TestListClass, TestListEntry, TestReport, TestResult, TestStatus, TestSummary,
    extract_errors, filter_files_by_tests, filter_noise, group_results_by_file, is_engine_noise,
    is_test_file, parse_res_location, strip_res_prefix,
};
use crate::core::gd_ast;

#[test]
fn test_is_test_file() {
    assert!(is_test_file(Path::new("test_player.gd")));
    assert!(is_test_file(Path::new("enemy_test.gd")));
    assert!(is_test_file(Path::new("tests/test_health.gd")));
    assert!(!is_test_file(Path::new("player.gd")));
    assert!(!is_test_file(Path::new("test_player.tscn")));
}

#[test]
fn test_parse_gut_counts_9x() {
    let output = r"
--- Run Summary ---
Passing Tests         3
Failing Tests         1
";
    assert_eq!(parse_gut_counts(output), (3, 1));
}

#[test]
fn test_parse_gut_counts_old() {
    let output = "Passed: 5 Failed: 2";
    assert_eq!(parse_gut_counts(output), (5, 2));
}

#[test]
fn test_parse_gut_counts_no_failures() {
    let output = r"
--- Run Summary ---
Passing Tests         10
";
    assert_eq!(parse_gut_counts(output), (10, 0));
}

#[test]
fn test_parse_gut_counts_unparseable() {
    assert_eq!(parse_gut_counts("no useful output"), (0, 0));
}

#[test]
fn test_is_engine_noise() {
    assert!(is_engine_noise(
        "WARNING: ObjectDB instances leaked at exit"
    ));
    assert!(is_engine_noise("  Orphan StringName: @icon"));
    assert!(is_engine_noise("Vulkan: vkCreateInstance failed"));
    assert!(is_engine_noise("GLES3: shader compilation error"));
    assert!(is_engine_noise("SCRIPT ERROR: gut_loader.gd:35 something"));
    assert!(!is_engine_noise("SCRIPT ERROR: Assertion failed."));
    assert!(!is_engine_noise("my normal output line"));
}

#[test]
fn test_filter_noise() {
    let input = "line one\nOrphan StringName: @icon\nline two\nVulkan init\nline three";
    let filtered = filter_noise(input);
    assert_eq!(filtered, "line one\nline two\nline three");
}

#[test]
fn test_extract_errors_script_error() {
    let stderr = "\
SCRIPT ERROR: Assertion failed.
   at: test_health (res://tests/test_enemy.gd:42)
";
    let errors = extract_errors(stderr);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].file, "tests/test_enemy.gd");
    assert_eq!(errors[0].line, Some(42));
    assert_eq!(errors[0].message, "Assertion failed.");
}

#[test]
fn test_extract_errors_multiple() {
    let stderr = "\
SCRIPT ERROR: First error
   at: func_a (res://tests/test_a.gd:10)
some other output
SCRIPT ERROR: Second error
   at: func_b (res://tests/test_b.gd:20)
";
    let errors = extract_errors(stderr);
    assert_eq!(errors.len(), 2);
    assert_eq!(errors[0].file, "tests/test_a.gd");
    assert_eq!(errors[0].line, Some(10));
    assert_eq!(errors[1].file, "tests/test_b.gd");
    assert_eq!(errors[1].line, Some(20));
}

#[test]
fn test_extract_errors_no_line() {
    let stderr = "\
SCRIPT ERROR: Something went wrong
   at: some_func (res://scripts/main.gd)
";
    let errors = extract_errors(stderr);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].file, "scripts/main.gd");
    assert_eq!(errors[0].line, None);
}

#[test]
fn test_extract_errors_empty() {
    assert!(extract_errors("").is_empty());
    assert!(extract_errors("normal output\nno errors here").is_empty());
}

#[test]
fn test_parse_res_location() {
    assert_eq!(
        parse_res_location("res://tests/test_player.gd:42"),
        (Some("tests/test_player.gd".to_string()), Some(42))
    );
    assert_eq!(
        parse_res_location("res://tests/test_player.gd"),
        (Some("tests/test_player.gd".to_string()), None)
    );
    assert_eq!(
        parse_res_location("scripts/main.gd:10"),
        (Some("scripts/main.gd".to_string()), Some(10))
    );
}

#[test]
fn test_strip_res_prefix() {
    assert_eq!(strip_res_prefix("res://tests/test.gd"), "tests/test.gd");
    assert_eq!(strip_res_prefix("tests/test.gd"), "tests/test.gd");
}

#[test]
fn test_status_serialization() {
    assert_eq!(
        serde_json::to_string(&TestStatus::Pass).unwrap(),
        "\"pass\""
    );
    assert_eq!(
        serde_json::to_string(&TestStatus::Fail).unwrap(),
        "\"fail\""
    );
    assert_eq!(
        serde_json::to_string(&TestStatus::Error).unwrap(),
        "\"error\""
    );
    assert_eq!(
        serde_json::to_string(&TestStatus::Timeout).unwrap(),
        "\"timeout\""
    );
}

#[test]
fn test_report_serialization() {
    let report = TestReport {
        mode: "script",
        results: vec![
            TestResult {
                file: Some("tests/test_player.gd".to_string()),
                status: TestStatus::Pass,
                duration_ms: 1234,
                errors: vec![],
                stderr: None,
                stdout: None,
            },
            TestResult {
                file: Some("tests/test_enemy.gd".to_string()),
                status: TestStatus::Fail,
                duration_ms: 567,
                errors: vec![TestError {
                    file: "tests/test_enemy.gd".to_string(),
                    line: Some(42),
                    message: "Assertion failed.".to_string(),
                }],
                stderr: Some("SCRIPT ERROR: Assertion failed.\n".to_string()),
                stdout: None,
            },
        ],
        summary: TestSummary {
            passed: 1,
            failed: 1,
            errors: 1,
            skipped: 0,
            total: 2,
        },
        duration_ms: 1801,
    };
    let json = serde_json::to_string_pretty(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["mode"], "script");
    assert_eq!(parsed["results"].as_array().unwrap().len(), 2);
    assert_eq!(parsed["results"][0]["status"], "pass");
    assert_eq!(parsed["results"][1]["status"], "fail");
    assert_eq!(parsed["results"][1]["errors"][0]["line"], 42);
    assert_eq!(parsed["summary"]["passed"], 1);
    assert_eq!(parsed["summary"]["failed"], 1);
    assert_eq!(parsed["summary"]["total"], 2);
    assert_eq!(parsed["duration_ms"], 1801);

    // Verify skip_serializing_if works: passing test has no errors/stderr/stdout keys
    assert!(parsed["results"][0].get("errors").is_none());
    assert!(parsed["results"][0].get("stderr").is_none());
    assert!(parsed["results"][0].get("stdout").is_none());
}

#[test]
fn test_report_empty() {
    let report = TestReport {
        mode: "script",
        results: vec![],
        summary: TestSummary {
            passed: 0,
            failed: 0,
            errors: 0,
            skipped: 0,
            total: 0,
        },
        duration_ms: 0,
    };
    let json = serde_json::to_string(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["results"].as_array().unwrap().len(), 0);
    assert_eq!(parsed["summary"]["total"], 0);
}

// --- gdUnit4 XML parsing tests ---

#[test]
fn test_parse_gdunit4_xml_passing() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites tests="3" failures="0" errors="0" skipped="0" time="0.5">
  <testsuite name="res://test/test_example.gd" tests="3" failures="0" time="0.5">
<testcase name="test_one" classname="res://test/test_example.gd" time="0.1">
</testcase>
<testcase name="test_two" classname="res://test/test_example.gd" time="0.2">
</testcase>
<testcase name="test_three" classname="res://test/test_example.gd" time="0.2">
</testcase>
  </testsuite>
</testsuites>"#;
    let (results, summary) = parse_gdunit4_xml(xml);
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|r| r.status == TestStatus::Pass));
    assert_eq!(summary.passed, 3);
    assert_eq!(summary.failed, 0);
    assert_eq!(summary.skipped, 0);
    assert_eq!(summary.total, 3);
}

#[test]
fn test_parse_gdunit4_xml_failures() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites tests="3" failures="1" errors="1" skipped="0" time="0.5">
  <testsuite name="res://test/test_example.gd" tests="3" failures="1" errors="1" time="0.5">
<testcase name="test_pass" classname="res://test/test_example.gd" time="0.1">
</testcase>
<testcase name="test_fail" classname="res://test/test_example.gd" time="0.2">
  <failure message="Expected &apos;10&apos; but was &apos;5&apos;" type="FAILURE">stack trace</failure>
</testcase>
<testcase name="test_error" classname="res://test/test_example.gd" time="0.1">
  <error message="Null reference" type="ERROR">stack trace</error>
</testcase>
  </testsuite>
</testsuites>"#;
    let (results, summary) = parse_gdunit4_xml(xml);
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].status, TestStatus::Pass);
    assert_eq!(results[1].status, TestStatus::Fail);
    assert_eq!(results[1].errors[0].message, "Expected '10' but was '5'");
    assert_eq!(results[2].status, TestStatus::Fail);
    assert_eq!(results[2].errors[0].message, "Null reference");
    assert_eq!(summary.passed, 1);
    assert_eq!(summary.failed, 2);
    assert_eq!(summary.total, 3);
}

#[test]
fn test_parse_gdunit4_xml_empty() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites tests="0" failures="0" errors="0" skipped="0" time="0.0">
</testsuites>"#;
    let (results, summary) = parse_gdunit4_xml(xml);
    assert!(results.is_empty());
    assert_eq!(summary.total, 0);
}

#[test]
fn test_parse_gdunit4_xml_skipped() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites tests="2" failures="0" errors="0" skipped="1" time="0.3">
  <testsuite name="res://test/test_example.gd" tests="2" failures="0" skipped="1" time="0.3">
<testcase name="test_pass" classname="res://test/test_example.gd" time="0.2">
</testcase>
<testcase name="test_skip" classname="res://test/test_example.gd" time="0.0">
  <skipped message="Not implemented yet"/>
</testcase>
  </testsuite>
</testsuites>"#;
    let (results, summary) = parse_gdunit4_xml(xml);
    assert_eq!(results.len(), 1); // skipped tests excluded from results
    assert_eq!(results[0].status, TestStatus::Pass);
    assert_eq!(summary.passed, 1);
    assert_eq!(summary.failed, 0);
    assert_eq!(summary.skipped, 1);
    assert_eq!(summary.total, 1);
}

#[test]
fn test_parse_gdunit4_xml_malformed() {
    let (results, summary) = parse_gdunit4_xml("not xml at all");
    assert!(results.is_empty());
    assert_eq!(summary.total, 0);
    assert_eq!(summary.passed, 0);
    assert_eq!(summary.failed, 0);
}

#[test]
fn test_extract_xml_attr() {
    assert_eq!(
        extract_xml_attr(r#"name="test_foo" classname="TestSuite""#, "name"),
        Some("test_foo".to_string())
    );
    assert_eq!(
        extract_xml_attr(r#"name="test_foo" classname="TestSuite""#, "classname"),
        Some("TestSuite".to_string())
    );
    assert_eq!(
        extract_xml_attr(r#"time="1.234""#, "time"),
        Some("1.234".to_string())
    );
    assert_eq!(extract_xml_attr("no attrs here", "name"), None);
}

#[test]
fn test_decode_xml_entities() {
    assert_eq!(decode_xml_entities("a &amp; b"), "a & b");
    assert_eq!(decode_xml_entities("&lt;tag&gt;"), "<tag>");
    assert_eq!(
        decode_xml_entities("he said &quot;hi&quot;"),
        r#"he said "hi""#
    );
    assert_eq!(decode_xml_entities("it&apos;s"), "it's");
    assert_eq!(decode_xml_entities("no entities"), "no entities");
}

#[test]
fn test_parse_gdunit4_xml_self_closing() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites tests="2" failures="0">
  <testsuite name="Suite">
<testcase name="test_a" classname="res://test/test_a.gd" time="0.05"/>
<testcase name="test_b" classname="res://test/test_a.gd" time="0.03"/>
  </testsuite>
</testsuites>"#;
    let (results, summary) = parse_gdunit4_xml(xml);
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| r.status == TestStatus::Pass));
    assert_eq!(summary.passed, 2);
    assert_eq!(summary.total, 2);
}

#[test]
fn test_parse_gdunit4_xml_classname_label() {
    let xml = r#"<testsuites>
  <testsuite name="Suite">
<testcase name="test_method" classname="res://test/test_player.gd" time="0.1">
</testcase>
  </testsuite>
</testsuites>"#;
    let (results, _) = parse_gdunit4_xml(xml);
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].file.as_deref(),
        Some("test/test_player.gd::test_method")
    );
}

#[test]
fn test_skipped_not_in_json_when_zero() {
    let summary = TestSummary {
        passed: 5,
        failed: 0,
        errors: 0,
        skipped: 0,
        total: 5,
    };
    let json = serde_json::to_string(&summary).unwrap();
    assert!(!json.contains("skipped"));
}

#[test]
fn test_skipped_in_json_when_nonzero() {
    let summary = TestSummary {
        passed: 4,
        failed: 0,
        errors: 0,
        skipped: 1,
        total: 4,
    };
    let json = serde_json::to_string(&summary).unwrap();
    assert!(json.contains("\"skipped\":1"));
}

// --- list_tests symbol table discovery tests ---

/// Parse GDScript source and collect test functions + inner class tests (mirrors list_tests logic).
fn collect_tests_from_source(source: &str) -> (Vec<String>, Vec<TestListClass>) {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_gdscript::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(source, None).unwrap();
    let gd_file = gd_ast::convert(&tree, source);

    let tests: Vec<String> = gd_file
        .funcs()
        .filter(|f| f.name.starts_with("test_"))
        .map(|f| f.name.to_string())
        .collect();

    let classes: Vec<TestListClass> = gd_file
        .inner_classes()
        .filter_map(|cls| {
            let class_tests: Vec<String> = cls
                .declarations
                .iter()
                .filter_map(|d| {
                    if let gd_ast::GdDecl::Func(f) = d {
                        Some(f)
                    } else {
                        None
                    }
                })
                .filter(|f| f.name.starts_with("test_"))
                .map(|f| f.name.to_string())
                .collect();
            if class_tests.is_empty() {
                None
            } else {
                Some(TestListClass {
                    name: cls.name.to_string(),
                    tests: class_tests,
                })
            }
        })
        .collect();

    (tests, classes)
}

#[test]
fn test_list_discovers_top_level_test_functions() {
    let source = r"
extends Node

func test_health():
    pass

func test_damage():
    pass

func helper():
    pass

func _ready():
    pass
";
    let (tests, classes) = collect_tests_from_source(source);
    assert_eq!(tests, vec!["test_health", "test_damage"]);
    assert!(classes.is_empty());
}

#[test]
fn test_list_discovers_inner_class_tests() {
    let source = r"
extends Node

func test_top():
    pass

class TestMovement:
    extends Node

    func test_walk_speed():
        pass

    func test_jump_height():
        pass

    func helper():
        pass

class TestCombat:
    extends Node

    func test_melee():
        pass
";
    let (tests, classes) = collect_tests_from_source(source);
    assert_eq!(tests, vec!["test_top"]);
    assert_eq!(classes.len(), 2);
    assert_eq!(classes[0].name, "TestMovement");
    assert_eq!(
        classes[0].tests,
        vec!["test_walk_speed", "test_jump_height"]
    );
    assert_eq!(classes[1].name, "TestCombat");
    assert_eq!(classes[1].tests, vec!["test_melee"]);
}

#[test]
fn test_list_no_test_functions() {
    let source = r"
extends Node

func _ready():
    pass

func helper():
    pass
";
    let (tests, classes) = collect_tests_from_source(source);
    assert!(tests.is_empty());
    assert!(classes.is_empty());
}

#[test]
fn test_list_entry_json_serialization() {
    let entry = TestListEntry {
        file: "tests/test_player.gd".to_string(),
        tests: vec!["test_health".to_string(), "test_damage".to_string()],
        classes: vec![TestListClass {
            name: "TestMovement".to_string(),
            tests: vec!["test_walk".to_string()],
        }],
    };
    let json = serde_json::to_string_pretty(&entry).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["file"], "tests/test_player.gd");
    assert_eq!(parsed["tests"].as_array().unwrap().len(), 2);
    assert_eq!(parsed["classes"][0]["name"], "TestMovement");
    assert_eq!(parsed["classes"][0]["tests"][0], "test_walk");
}

#[test]
fn test_list_entry_json_no_classes_omitted() {
    let entry = TestListEntry {
        file: "tests/test_simple.gd".to_string(),
        tests: vec!["test_one".to_string()],
        classes: vec![],
    };
    let json = serde_json::to_string(&entry).unwrap();
    assert!(!json.contains("classes"));
}

// --- group_results_by_file tests ---

#[test]
fn test_group_results_by_file_mixed() {
    let results = vec![
        TestResult {
            file: Some("test/test_a.gd::test_one".to_string()),
            status: TestStatus::Pass,
            duration_ms: 100,
            errors: vec![],
            stderr: None,
            stdout: None,
        },
        TestResult {
            file: Some("test/test_a.gd::test_two".to_string()),
            status: TestStatus::Fail,
            duration_ms: 200,
            errors: vec![],
            stderr: None,
            stdout: None,
        },
        TestResult {
            file: Some("test/test_b.gd::test_three".to_string()),
            status: TestStatus::Pass,
            duration_ms: 50,
            errors: vec![],
            stderr: None,
            stdout: None,
        },
    ];
    let groups = group_results_by_file(&results);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0], ("test/test_a.gd".to_string(), 1, 1));
    assert_eq!(groups[1], ("test/test_b.gd".to_string(), 1, 0));
}

#[test]
fn test_group_results_by_file_none_entries() {
    let results = vec![
        TestResult {
            file: None,
            status: TestStatus::Pass,
            duration_ms: 100,
            errors: vec![],
            stderr: None,
            stdout: None,
        },
        TestResult {
            file: Some("test/test_a.gd".to_string()),
            status: TestStatus::Fail,
            duration_ms: 200,
            errors: vec![],
            stderr: None,
            stdout: None,
        },
    ];
    let groups = group_results_by_file(&results);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0], ("unknown".to_string(), 1, 0));
    assert_eq!(groups[1], ("test/test_a.gd".to_string(), 0, 1));
}

#[test]
fn test_group_results_by_file_single_file() {
    let results = vec![TestResult {
        file: Some("test/test_only.gd::test_x".to_string()),
        status: TestStatus::Pass,
        duration_ms: 50,
        errors: vec![],
        stderr: None,
        stdout: None,
    }];
    let groups = group_results_by_file(&results);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0], ("test/test_only.gd".to_string(), 1, 0));
}

#[test]
fn test_group_results_by_file_empty() {
    let groups = group_results_by_file(&[]);
    assert!(groups.is_empty());
}

#[test]
fn test_group_results_preserves_order() {
    let results = vec![
        TestResult {
            file: Some("test/test_c.gd::test_1".to_string()),
            status: TestStatus::Pass,
            duration_ms: 10,
            errors: vec![],
            stderr: None,
            stdout: None,
        },
        TestResult {
            file: Some("test/test_a.gd::test_1".to_string()),
            status: TestStatus::Pass,
            duration_ms: 10,
            errors: vec![],
            stderr: None,
            stdout: None,
        },
        TestResult {
            file: Some("test/test_b.gd::test_1".to_string()),
            status: TestStatus::Fail,
            duration_ms: 10,
            errors: vec![],
            stderr: None,
            stdout: None,
        },
    ];
    let groups = group_results_by_file(&results);
    assert_eq!(groups[0].0, "test/test_c.gd");
    assert_eq!(groups[1].0, "test/test_a.gd");
    assert_eq!(groups[2].0, "test/test_b.gd");
}

// --- filter_files_by_tests tests ---

/// Helper to create a temp GDScript file and return its path.
fn write_temp_gd(dir: &Path, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
    path
}

#[test]
fn test_filter_files_no_filters_returns_all() {
    let dir = tempfile::tempdir().unwrap();
    let a = write_temp_gd(
        dir.path(),
        "test_a.gd",
        "extends Node\nfunc test_one():\n\tpass\n",
    );
    let b = write_temp_gd(
        dir.path(),
        "test_b.gd",
        "extends Node\nfunc test_two():\n\tpass\n",
    );
    let files = vec![a.clone(), b.clone()];
    let result = filter_files_by_tests(&files, None, None);
    assert_eq!(result.len(), 2);
}

#[test]
fn test_filter_files_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let a = write_temp_gd(
        dir.path(),
        "test_player.gd",
        "extends Node\nfunc test_health():\n\tpass\nfunc test_damage():\n\tpass\n",
    );
    let b = write_temp_gd(
        dir.path(),
        "test_enemy.gd",
        "extends Node\nfunc test_spawn():\n\tpass\n",
    );
    let files = vec![a.clone(), b.clone()];

    // Filter by "health" — only test_player.gd has test_health
    let result = filter_files_by_tests(&files, Some("health"), None);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].path, a);
    // tests field contains ALL test functions in the file (for exclusion lists)
    assert_eq!(result[0].tests, vec!["test_health", "test_damage"]);
}

#[test]
fn test_filter_files_by_name_no_match() {
    let dir = tempfile::tempdir().unwrap();
    let a = write_temp_gd(
        dir.path(),
        "test_player.gd",
        "extends Node\nfunc test_health():\n\tpass\n",
    );
    let files = vec![a];

    let result = filter_files_by_tests(&files, Some("nonexistent"), None);
    assert!(result.is_empty());
}

#[test]
fn test_filter_files_by_class() {
    let dir = tempfile::tempdir().unwrap();
    let a = write_temp_gd(
        dir.path(),
        "test_movement.gd",
        "extends Node\n\nfunc test_top():\n\tpass\n\nclass TestWalking:\n\textends Node\n\tfunc test_walk():\n\t\tpass\n",
    );
    let b = write_temp_gd(
        dir.path(),
        "test_combat.gd",
        "extends Node\nfunc test_attack():\n\tpass\n",
    );
    let files = vec![a.clone(), b];

    // Filter by class "Walking" — only test_movement.gd has inner class TestWalking
    let result = filter_files_by_tests(&files, None, Some("Walking"));
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].path, a);
    assert_eq!(result[0].classes, vec!["TestWalking"]);
}

// --- build_gdunit4_filter_args tests ---

#[test]
fn test_gdunit4_filter_args_no_filters() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("test")).unwrap();
    let args = default_run_args();
    let (add, ignore) = build_gdunit4_filter_args(&[], dir.path(), &args, false);
    assert_eq!(add, vec!["res://test"]);
    assert!(ignore.is_empty());
}

#[test]
fn test_gdunit4_filter_args_with_name() {
    let dir = tempfile::tempdir().unwrap();
    let test_dir = dir.path().join("test");
    std::fs::create_dir(&test_dir).unwrap();
    let file = write_temp_gd(
        &test_dir,
        "test_player.gd",
        "extends Node\nfunc test_health():\n\tpass\nfunc test_damage():\n\tpass\nfunc test_speed():\n\tpass\n",
    );
    let files = vec![file];

    let mut args = default_run_args();
    args.name = Some("health".to_string());

    let (add, ignore) = build_gdunit4_filter_args(&files, dir.path(), &args, true);

    // Should add the specific file
    assert_eq!(add.len(), 1);
    assert!(add[0].starts_with("res://"));
    assert!(add[0].ends_with("test_player.gd"));

    // Should ignore non-matching tests
    assert_eq!(ignore.len(), 2);
    assert!(ignore.iter().any(|i| i.ends_with(":test_damage")));
    assert!(ignore.iter().any(|i| i.ends_with(":test_speed")));
    // test_health should NOT be in ignore list
    assert!(!ignore.iter().any(|i| i.ends_with(":test_health")));
}

/// Build a minimal `RunArgs` for testing.
fn default_run_args() -> super::super::test_cmd::RunArgs {
    super::super::test_cmd::RunArgs {
        name: None,
        path: vec![],
        filter: None,
        class: None,
        list: false,
        junit: None,
        verbose: false,
        headless: true,
        timeout: 60,
        format: "text".to_string(),
        quiet: false,
        clean: false,
        runner: None,
        extra: vec![],
    }
}
