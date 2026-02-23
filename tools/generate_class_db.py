#!/usr/bin/env python3
"""Generate src/class_db/generated.rs from Godot's extension_api.json.

Usage:
    godot --headless --dump-extension-api-with-docs
    python tools/generate_class_db.py extension_api.json > src/class_db/generated.rs
"""
import json
import sys

from bbcode import bbcode_to_markdown, truncate_doc


def escape_rust_str(s):
    """Escape a string for use in a Rust string literal."""
    return s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n")


def format_i64(n):
    """Format an integer with Rust-style underscore separators for clippy."""
    s = str(abs(n))
    if len(s) <= 4:
        return str(n)
    # Insert underscores every 3 digits from the right
    parts = []
    while len(s) > 3:
        parts.append(s[-3:])
        s = s[:-3]
    parts.append(s)
    formatted = "_".join(reversed(parts))
    return f"-{formatted}" if n < 0 else formatted


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

    classes = []
    constants = []
    enum_members = []
    methods = []
    method_signatures = []
    properties = []
    signals = []
    method_docs = []
    property_docs = []
    signal_docs = []
    enum_values = []  # (class, enum_name, value_name, int_value, is_bitfield, doc)

    for cls in api.get("classes", []):
        name = cls["name"]
        parent = cls.get("inherits", "")
        is_virtual = cls.get("is_virtual", False)
        brief = format_doc(cls.get("brief_description", ""))
        classes.append((name, parent, is_virtual, brief))

        for enum in cls.get("enums", []):
            enum_name = enum["name"]
            is_bitfield = enum.get("is_bitfield", False)
            for val in enum.get("values", []):
                member_name = val["name"]
                enum_members.append((f"{name}.{member_name}", enum_name))
                doc = format_doc(val.get("description", ""))
                enum_values.append((name, enum_name, member_name, val["value"], is_bitfield, doc))

        for const in cls.get("constants", []):
            const_name = const["name"]
            const_type = const.get("type", "int")
            constants.append((f"{name}.{const_name}", const_type))

        for method in cls.get("methods", []):
            method_name = method["name"]
            ret = method.get("return_value", {}).get("type", "void")
            methods.append((f"{name}.{method_name}", ret))
            # Build signature info for override checking
            args = method.get("arguments", [])
            param_types = ",".join(a["type"] for a in args)
            required = sum(1 for a in args if "default_value" not in a)
            method_signatures.append((f"{name}.{method_name}", ret, required, len(args), param_types))
            # Doc
            doc = format_doc(method.get("description", ""))
            if doc:
                method_docs.append((f"{name}.{method_name}", doc))

        for prop in cls.get("properties", []):
            prop_name = prop["name"]
            prop_type = prop.get("type", "Variant")
            properties.append((f"{name}.{prop_name}", prop_type))
            # Doc
            doc = format_doc(prop.get("description", ""))
            if doc:
                property_docs.append((f"{name}.{prop_name}", doc))

        for sig in cls.get("signals", []):
            sig_name = sig["name"]
            signals.append(f"{name}.{sig_name}")
            # Doc
            doc = format_doc(sig.get("description", ""))
            if doc:
                signal_docs.append((f"{name}.{sig_name}", doc))

    # Also extract global enums/constants
    for enum in api.get("global_enums", []):
        enum_name = enum["name"]
        is_bitfield = enum.get("is_bitfield", False)
        for val in enum.get("values", []):
            member_name = val["name"]
            enum_members.append((f"@GlobalScope.{member_name}", enum_name))
            constants.append((f"@GlobalScope.{member_name}", "int"))
            doc = format_doc(val.get("description", ""))
            enum_values.append(("@GlobalScope", enum_name, member_name, val["value"], is_bitfield, doc))

    for const in api.get("global_constants", []):
        const_name = const["name"]
        constants.append((f"@GlobalScope.{const_name}", "int"))

    # Utility functions (print, lerp, sin, etc.)
    utility_functions = []
    for uf in api.get("utility_functions", []):
        uf_name = uf["name"]
        ret = uf.get("return_type", "void")
        args = uf.get("arguments", [])
        parts = []
        for arg in args:
            arg_name = arg["name"]
            arg_type = arg["type"]
            parts.append(f"{arg_name}: {arg_type}")
        params = ", ".join(parts)
        if uf.get("is_vararg", False):
            params = "..." if not params else f"{params}, ..."
        signature = f"{uf_name}({params}) -> {ret}"
        doc = format_doc(uf.get("description", ""))
        utility_functions.append((uf_name, ret, escape_rust_str(signature), doc))

    utility_functions.sort(key=lambda x: x[0])

    classes.sort(key=lambda x: x[0])
    constants.sort(key=lambda x: x[0])
    # Deduplicate constants (global enums also appear as constants)
    constants = list(dict(constants).items())
    constants.sort(key=lambda x: x[0])
    enum_members.sort(key=lambda x: x[0])
    methods.sort(key=lambda x: x[0])
    method_signatures.sort(key=lambda x: x[0])
    properties.sort(key=lambda x: x[0])
    signals.sort()
    method_docs.sort(key=lambda x: x[0])
    property_docs.sort(key=lambda x: x[0])
    signal_docs.sort(key=lambda x: x[0])
    enum_values.sort(key=lambda x: (x[0], x[1], x[3]))  # class, enum, int_value

    print('//! Auto-generated Godot class database.')
    print('//! Regenerate: python tools/generate_class_db.py extension_api.json > src/class_db/generated.rs')
    print()
    print('pub struct ClassInfo {')
    print('    pub name: &\'static str,')
    print('    pub parent: &\'static str,')
    print('    pub is_virtual: bool,')
    print('    pub doc: &\'static str,')
    print('}')
    print()

    print(f'pub static CLASSES: &[ClassInfo] = &[')
    for name, parent, is_virtual, doc in classes:
        v = "true" if is_virtual else "false"
        print(f'    ClassInfo {{ name: "{name}", parent: "{parent}", is_virtual: {v}, doc: "{doc}" }},')
    print('];')
    print()

    print(f'pub static CONSTANTS: &[(&str, &str)] = &[')
    for key, val in constants:
        print(f'    ("{key}", "{val}"),')
    print('];')
    print()

    print(f'pub static ENUM_MEMBERS: &[(&str, &str)] = &[')
    for key, val in enum_members:
        print(f'    ("{key}", "{val}"),')
    print('];')
    print()

    print(f'pub static METHODS: &[(&str, &str)] = &[')
    for key, val in methods:
        print(f'    ("{key}", "{val}"),')
    print('];')
    print()

    # Method signatures: (key, return_type, required_params, total_params, param_types)
    print('pub struct MethodSig {')
    print('    pub key: &\'static str,')
    print('    pub return_type: &\'static str,')
    print('    pub required_params: u8,')
    print('    pub total_params: u8,')
    print('    pub param_types: &\'static str,')
    print('}')
    print()
    print(f'pub static METHOD_SIGNATURES: &[MethodSig] = &[')
    for key, ret, required, total, params in method_signatures:
        print(f'    MethodSig {{ key: "{key}", return_type: "{ret}", required_params: {required}, total_params: {total}, param_types: "{params}" }},')
    print('];')
    print()

    print(f'pub static PROPERTIES: &[(&str, &str)] = &[')
    for key, val in properties:
        print(f'    ("{key}", "{val}"),')
    print('];')
    print()

    print(f'pub static SIGNALS: &[&str] = &[')
    for key in signals:
        print(f'    "{key}",')
    print('];')
    print()

    # Doc tables
    print(f'pub static METHOD_DOCS: &[(&str, &str)] = &[')
    for key, doc in method_docs:
        print(f'    ("{key}", "{doc}"),')
    print('];')
    print()

    print(f'pub static PROPERTY_DOCS: &[(&str, &str)] = &[')
    for key, doc in property_docs:
        print(f'    ("{key}", "{doc}"),')
    print('];')
    print()

    print(f'pub static SIGNAL_DOCS: &[(&str, &str)] = &[')
    for key, doc in signal_docs:
        print(f'    ("{key}", "{doc}"),')
    print('];')
    print()

    # Utility functions
    print('pub struct UtilityFunction {')
    print('    pub name: &\'static str,')
    print('    pub return_type: &\'static str,')
    print('    pub signature: &\'static str,')
    print('    pub doc: &\'static str,')
    print('}')
    print()
    print(f'pub static UTILITY_FUNCTIONS: &[UtilityFunction] = &[')
    for name, ret, sig, doc in utility_functions:
        print(f'    UtilityFunction {{ name: "{name}", return_type: "{ret}", signature: "{sig}", doc: "{doc}" }},')
    print('];')
    print()

    # Enum values with descriptions
    print('pub struct EnumValue {')
    print('    pub class: &\'static str,')
    print('    pub enum_name: &\'static str,')
    print('    pub name: &\'static str,')
    print('    pub value: i64,')
    print('    pub is_bitfield: bool,')
    print('    pub doc: &\'static str,')
    print('}')
    print()
    print(f'pub static ENUM_VALUES: &[EnumValue] = &[')
    for cls, enum_name, val_name, int_val, is_bitfield, doc in enum_values:
        bf = "true" if is_bitfield else "false"
        print(f'    EnumValue {{ class: "{cls}", enum_name: "{enum_name}", name: "{val_name}", value: {format_i64(int_val)}, is_bitfield: {bf}, doc: "{doc}" }},')
    print('];')


if __name__ == "__main__":
    main()
