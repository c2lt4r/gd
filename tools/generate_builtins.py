#!/usr/bin/env python3
"""Generate src/lsp/builtin_generated.rs from Godot's extension_api.json.

Extracts methods, properties, and constants from builtin_classes (Vector2, Color,
String, Array, Dictionary, etc.) into BuiltinMember entries for LSP completions.

Usage:
    godot --headless --dump-extension-api-with-docs
    python tools/generate_builtins.py extension_api.json > src/lsp/builtin_generated.rs
"""
import json
import sys

from bbcode import bbcode_to_markdown, truncate_doc


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
    return s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n")


def format_doc(raw_text):
    """Convert BBCode to markdown and escape for Rust."""
    if not raw_text:
        return ""
    md = bbcode_to_markdown(raw_text)
    return escape_rust_str(md)


def main():
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} extension_api.json", file=sys.stderr)
        sys.exit(1)

    with open(sys.argv[1]) as f:
        api = json.load(f)

    entries = []  # (class, name, brief, description, kind)
    type_docs = []  # (name, brief, description)

    for cls in api.get("builtin_classes", []):
        class_name = cls["name"]

        # Skip primitive types that don't have dot-accessible members
        if class_name in ("int", "float", "bool", "Nil"):
            continue

        # Collect class-level docs
        brief = format_doc(cls.get("brief_description", ""))
        desc = format_doc(cls.get("description", ""))
        if brief or desc:
            type_docs.append((class_name, brief, desc))

        # Properties (members)
        for member in cls.get("members", []):
            member_name = member["name"]
            member_type = member["type"]
            brief_str = f"{member_name}: {member_type}"
            doc = format_doc(member.get("description", ""))
            entries.append((class_name, member_name, brief_str, doc, "Property"))

        # Methods
        for method in cls.get("methods", []):
            method_name = method["name"]
            brief_str = format_signature(method)
            doc = format_doc(method.get("description", ""))
            entries.append((class_name, method_name, brief_str, doc, "Method"))

    # Sort by (class, kind, name) for clean output
    entries.sort(key=lambda e: (e[0], e[4], e[1]))
    type_docs.sort(key=lambda e: e[0])

    # Count stats
    classes = sorted(set(e[0] for e in entries))
    n_methods = sum(1 for e in entries if e[4] == "Method")
    n_props = sum(1 for e in entries if e[4] == "Property")

    print("//! Auto-generated builtin type members from extension_api.json.")
    print(f"//! {len(classes)} types, {n_methods} methods, {n_props} properties.")
    print("//! Regenerate: python tools/generate_builtins.py extension_api.json > src/lsp/builtin_generated.rs")
    print()
    print("use super::builtins::{BuiltinMember, MemberKind};")
    print()
    print(f"pub static GENERATED_MEMBERS: &[BuiltinMember] = &[")

    current_class = None
    for class_name, name, brief, desc, kind in entries:
        if class_name != current_class:
            if current_class is not None:
                print()
            print(f"    // ── {class_name} ──")
            current_class = class_name

        brief_escaped = escape_rust_str(brief)
        # Use real description if available, fall back to signature
        desc_str = desc if desc else brief_escaped
        print(f'    BuiltinMember {{ class: "{class_name}", name: "{name}", '
              f'brief: "{brief_escaped}", description: "{desc_str}", '
              f'kind: MemberKind::{kind} }},')

    print("];")
    print()

    # Builtin type docs
    print("pub struct BuiltinTypeDoc {")
    print("    pub name: &'static str,")
    print("    pub brief: &'static str,")
    print("    pub description: &'static str,")
    print("}")
    print()
    print(f"pub static BUILTIN_TYPE_DOCS: &[BuiltinTypeDoc] = &[")
    for name, brief, desc in type_docs:
        print(f'    BuiltinTypeDoc {{ name: "{name}", brief: "{brief}", description: "{desc}" }},')
    print("];")


if __name__ == "__main__":
    main()
