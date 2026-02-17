#!/usr/bin/env python3
"""Generate src/lsp/builtin_generated.rs from Godot's extension_api.json.

Extracts methods, properties, and constants from builtin_classes (Vector2, Color,
String, Array, Dictionary, etc.) into BuiltinMember entries for LSP completions.

Usage:
    godot --headless --dump-extension-api
    python tools/generate_builtins.py extension_api.json > src/lsp/builtin_generated.rs
"""
import json
import sys


def format_signature(method):
    """Build a GDScript-style signature string for a method."""
    name = method["name"]
    ret = method.get("return_type", "void")
    args = method.get("arguments", [])

    parts = []
    for arg in args:
        arg_name = arg["name"]
        arg_type = arg["type"]
        if "default_value" in arg:
            default = arg["default_value"]
            # Clean up default value representation
            if default == "":
                default = '""'
            parts.append(f"{arg_name}: {arg_type} = {default}")
        else:
            parts.append(f"{arg_name}: {arg_type}")

    params = ", ".join(parts)
    if method.get("is_vararg", False):
        params = "..." if not params else f"{params}, ..."

    return f"{name}({params}) -> {ret}"


def escape_rust_str(s):
    """Escape a string for use in a Rust string literal."""
    return s.replace("\\", "\\\\").replace('"', '\\"')


def main():
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} extension_api.json", file=sys.stderr)
        sys.exit(1)

    with open(sys.argv[1]) as f:
        api = json.load(f)

    entries = []  # (class, name, brief, kind)

    for cls in api.get("builtin_classes", []):
        class_name = cls["name"]

        # Skip primitive types that don't have dot-accessible members
        if class_name in ("int", "float", "bool", "Nil"):
            continue

        # Properties (members)
        for member in cls.get("members", []):
            member_name = member["name"]
            member_type = member["type"]
            brief = f"{member_name}: {member_type}"
            entries.append((class_name, member_name, brief, "Property"))

        # Methods
        for method in cls.get("methods", []):
            method_name = method["name"]
            brief = format_signature(method)
            entries.append((class_name, method_name, brief, "Method"))

    # Sort by (class, kind, name) for clean output
    entries.sort(key=lambda e: (e[0], e[3], e[1]))

    # Count stats
    classes = sorted(set(e[0] for e in entries))
    n_methods = sum(1 for e in entries if e[3] == "Method")
    n_props = sum(1 for e in entries if e[3] == "Property")

    print("//! Auto-generated builtin type members from extension_api.json.")
    print(f"//! {len(classes)} types, {n_methods} methods, {n_props} properties.")
    print("//! Regenerate: python tools/generate_builtins.py extension_api.json > src/lsp/builtin_generated.rs")
    print()
    print("use super::builtins::{BuiltinMember, MemberKind};")
    print()
    print(f"pub static GENERATED_MEMBERS: &[BuiltinMember] = &[")

    current_class = None
    for class_name, name, brief, kind in entries:
        if class_name != current_class:
            if current_class is not None:
                print()
            print(f"    // ── {class_name} ──")
            current_class = class_name

        brief_escaped = escape_rust_str(brief)
        print(f'    BuiltinMember {{ class: "{class_name}", name: "{name}", '
              f'brief: "{brief_escaped}", description: "{brief_escaped}", '
              f'kind: MemberKind::{kind} }},')

    print("];")


if __name__ == "__main__":
    main()
