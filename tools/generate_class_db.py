#!/usr/bin/env python3
"""Generate src/class_db/generated.rs from Godot's extension_api.json.

Usage:
    godot --headless --dump-extension-api
    python tools/generate_class_db.py extension_api.json > src/class_db/generated.rs
"""
import json
import sys


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

    for cls in api.get("classes", []):
        name = cls["name"]
        parent = cls.get("inherits", "")
        is_virtual = cls.get("is_virtual", False)
        classes.append((name, parent, is_virtual))

        for enum in cls.get("enums", []):
            enum_name = enum["name"]
            for val in enum.get("values", []):
                member_name = val["name"]
                enum_members.append((f"{name}.{member_name}", enum_name))

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

        for prop in cls.get("properties", []):
            prop_name = prop["name"]
            prop_type = prop.get("type", "Variant")
            properties.append((f"{name}.{prop_name}", prop_type))

        for sig in cls.get("signals", []):
            sig_name = sig["name"]
            signals.append(f"{name}.{sig_name}")

    # Also extract global enums/constants
    for enum in api.get("global_enums", []):
        enum_name = enum["name"]
        for val in enum.get("values", []):
            member_name = val["name"]
            enum_members.append((f"@GlobalScope.{member_name}", enum_name))
            constants.append((f"@GlobalScope.{member_name}", "int"))

    for const in api.get("global_constants", []):
        const_name = const["name"]
        constants.append((f"@GlobalScope.{const_name}", "int"))

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

    print('//! Auto-generated Godot class database.')
    print('//! Regenerate: python tools/generate_class_db.py extension_api.json > src/class_db/generated.rs')
    print()
    print('pub struct ClassInfo {')
    print('    pub name: &\'static str,')
    print('    pub parent: &\'static str,')
    print('    pub is_virtual: bool,')
    print('}')
    print()

    print(f'pub static CLASSES: &[ClassInfo] = &[')
    for name, parent, is_virtual in classes:
        v = "true" if is_virtual else "false"
        print(f'    ClassInfo {{ name: "{name}", parent: "{parent}", is_virtual: {v} }},')
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


if __name__ == "__main__":
    main()
