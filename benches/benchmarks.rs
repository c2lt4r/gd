use criterion::{criterion_group, criterion_main, Criterion};
use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Generate realistic GDScript source with the given number of functions.
/// Also includes variables, signals, enums, and class structures.
fn generate_gdscript(num_functions: usize) -> String {
    let mut lines = Vec::new();

    lines.push("class_name BenchScript".to_string());
    lines.push("extends Node".to_string());
    lines.push(String::new());

    // Signals
    for i in 0..num_functions.min(10) {
        lines.push(format!("signal value_changed_{i}(old_value, new_value)"));
    }
    lines.push(String::new());

    // Enums
    lines.push("enum State { IDLE, RUNNING, PAUSED, STOPPED }".to_string());
    lines.push(String::new());

    // Variables
    for i in 0..num_functions.min(20) {
        match i % 4 {
            0 => lines.push(format!("var counter_{i}: int = {i}")),
            1 => lines.push(format!("@export var speed_{i}: float = {}.5", i)),
            2 => lines.push(format!("var label_{i}: String = \"item_{i}\"")),
            _ => lines.push(format!("@onready var node_{i} = $Node{i}")),
        }
    }
    lines.push(String::new());

    // Functions
    for i in 0..num_functions {
        lines.push(format!("func do_something_{i}(value: int) -> int:"));
        lines.push(format!("\tvar result: int = value + {i}"));
        lines.push("\tif result > 100:".to_string());
        lines.push("\t\tresult = 100".to_string());
        lines.push("\telif result < 0:".to_string());
        lines.push("\t\tresult = 0".to_string());
        lines.push("\tfor j in range(10):".to_string());
        lines.push("\t\tresult += j".to_string());
        lines.push(format!("\tprint(\"Function {i} result: \", result)"));
        lines.push("\treturn result".to_string());
        lines.push(String::new());
    }

    // Ready and process functions
    lines.push("func _ready() -> void:".to_string());
    for i in 0..num_functions.min(5) {
        lines.push(format!("\tdo_something_{i}({i})"));
    }
    lines.push(String::new());

    lines.push("func _process(delta: float) -> void:".to_string());
    lines.push("\tpass".to_string());
    lines.push(String::new());

    lines.join("\n")
}

fn bench_parse(c: &mut Criterion) {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_gdscript::LANGUAGE.into())
        .unwrap();

    let small = generate_gdscript(10);
    let medium = generate_gdscript(50);
    let large = generate_gdscript(200);

    c.bench_function("parse_small_10_funcs", |b| {
        b.iter(|| parser.parse(&small, None).unwrap());
    });

    c.bench_function("parse_medium_50_funcs", |b| {
        b.iter(|| parser.parse(&medium, None).unwrap());
    });

    c.bench_function("parse_large_200_funcs", |b| {
        b.iter(|| parser.parse(&large, None).unwrap());
    });
}

fn bench_fmt(c: &mut Criterion) {
    let temp = TempDir::new().unwrap();
    let small_path = temp.path().join("small.gd");
    let medium_path = temp.path().join("medium.gd");
    let large_path = temp.path().join("large.gd");

    fs::write(&small_path, generate_gdscript(10)).unwrap();
    fs::write(&medium_path, generate_gdscript(50)).unwrap();
    fs::write(&large_path, generate_gdscript(200)).unwrap();

    fs::write(temp.path().join("project.godot"), "[gd_resource]\n").unwrap();

    let gd = env!("CARGO_BIN_EXE_gd");

    c.bench_function("fmt_small_10_funcs", |b| {
        b.iter(|| {
            Command::new(gd)
                .args(["fmt", "--check"])
                .arg(&small_path)
                .output()
                .unwrap();
        });
    });

    c.bench_function("fmt_medium_50_funcs", |b| {
        b.iter(|| {
            Command::new(gd)
                .args(["fmt", "--check"])
                .arg(&medium_path)
                .output()
                .unwrap();
        });
    });

    c.bench_function("fmt_large_200_funcs", |b| {
        b.iter(|| {
            Command::new(gd)
                .args(["fmt", "--check"])
                .arg(&large_path)
                .output()
                .unwrap();
        });
    });
}

fn bench_lint(c: &mut Criterion) {
    let temp = TempDir::new().unwrap();
    let small_path = temp.path().join("small.gd");
    let medium_path = temp.path().join("medium.gd");
    let large_path = temp.path().join("large.gd");

    fs::write(&small_path, generate_gdscript(10)).unwrap();
    fs::write(&medium_path, generate_gdscript(50)).unwrap();
    fs::write(&large_path, generate_gdscript(200)).unwrap();

    fs::write(temp.path().join("project.godot"), "[gd_resource]\n").unwrap();

    let gd = env!("CARGO_BIN_EXE_gd");

    c.bench_function("lint_small_10_funcs", |b| {
        b.iter(|| {
            Command::new(gd)
                .args(["lint"])
                .arg(&small_path)
                .output()
                .unwrap();
        });
    });

    c.bench_function("lint_medium_50_funcs", |b| {
        b.iter(|| {
            Command::new(gd)
                .args(["lint"])
                .arg(&medium_path)
                .output()
                .unwrap();
        });
    });

    c.bench_function("lint_large_200_funcs", |b| {
        b.iter(|| {
            Command::new(gd)
                .args(["lint"])
                .arg(&large_path)
                .output()
                .unwrap();
        });
    });
}

criterion_group!(benches, bench_parse, bench_fmt, bench_lint);
criterion_main!(benches);
