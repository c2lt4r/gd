use crate::debug::variant::GodotVariant;

use super::{
    DebugVariable, EvalResult, ObjectInfo, ObjectProperty, SceneNode, SceneTree, StackFrameInfo,
};

// ---------------------------------------------------------------------------
// Response parsers
// ---------------------------------------------------------------------------

/// Parse stack_dump response: [String("stack_dump"), String(file), Int(line), String(func), ...]
pub(super) fn parse_stack_dump(msg: &[GodotVariant]) -> Vec<StackFrameInfo> {
    let mut frames = Vec::new();
    // Skip the command name at index 0
    let args = if msg
        .first()
        .is_some_and(|v| matches!(v, GodotVariant::String(s) if s == "stack_dump"))
    {
        &msg[1..]
    } else {
        msg
    };

    // Triplets: [String(file), Int(line), String(function)]
    for chunk in args.chunks(3) {
        if chunk.len() < 3 {
            break;
        }
        if let (GodotVariant::String(file), GodotVariant::Int(line), GodotVariant::String(func)) =
            (&chunk[0], &chunk[1], &chunk[2])
        {
            frames.push(StackFrameInfo {
                file: file.clone(),
                line: *line as u32,
                function: func.clone(),
            });
        }
    }
    frames
}

/// Parse var counts: [String("stack_frame_vars"), Int(local), Int(member), Int(global)]
pub(super) fn parse_var_counts(msg: &[GodotVariant]) -> Option<(usize, usize, usize)> {
    // Skip command name
    let args = if msg
        .first()
        .is_some_and(|v| matches!(v, GodotVariant::String(s) if s == "stack_frame_vars"))
    {
        &msg[1..]
    } else {
        msg
    };
    if args.len() < 3 {
        return None;
    }
    let local = variant_as_usize(&args[0])?;
    let member = variant_as_usize(&args[1])?;
    let global = variant_as_usize(&args[2])?;
    Some((local, member, global))
}

/// Parse a single variable message.
/// Godot ScriptStackVariable.serialize() format (4 fields):
///   [String(name), Int(scope_type), Int(variant_type_id), Variant(value)]
/// Wire message: [String("stack_frame_var"), name, scope_type, variant_type_id, value]
pub(super) fn parse_debug_variable(msg: &[GodotVariant]) -> Option<DebugVariable> {
    let args = if msg
        .first()
        .is_some_and(|v| matches!(v, GodotVariant::String(s) if s == "stack_frame_var"))
    {
        &msg[1..]
    } else {
        msg
    };
    if args.len() < 4 {
        return None;
    }
    let name = variant_as_string(&args[0])?;
    // args[1] = scope type (0=local, 1=member, 2=global) — unused
    let var_type = variant_as_i32(&args[2])?;
    let value = args.get(3).cloned().unwrap_or(GodotVariant::Nil);
    Some(DebugVariable {
        name,
        value,
        var_type,
    })
}

/// Parse evaluation_return response.
/// Godot ScriptStackVariable.serialize() format (4 fields):
///   [String(name), Int(scope_type=3), Int(variant_type_id), Variant(value)]
/// Wire message: [String("evaluation_return"), name, scope_type, variant_type_id, value]
pub(super) fn parse_eval_result(msg: &[GodotVariant]) -> Option<EvalResult> {
    let args = if msg
        .first()
        .is_some_and(|v| matches!(v, GodotVariant::String(s) if s == "evaluation_return"))
    {
        &msg[1..]
    } else {
        msg
    };
    if args.len() < 4 {
        return None;
    }
    let name = variant_as_string(&args[0])?;
    // args[1] = scope type (always 3 for eval) — unused
    let var_type = variant_as_i32(&args[2])?;
    let value = args.get(3).cloned().unwrap_or(GodotVariant::Nil);
    Some(EvalResult {
        name,
        value,
        var_type,
    })
}

/// Parse scene:scene_tree response.
/// Wire format (after normalization): [String("scene:scene_tree"), ...node data...]
/// Each node is 6 sequential fields followed by its children (recursive):
///   Int(child_count), String(name), String(class), Int(object_id),
///   String(scene_file_path), Int(view_flags)
/// The root node is a single node (not a list), matching the VS Code plugin's
/// `parse_next_scene_node` in helpers.ts.
pub(super) fn parse_scene_tree(msg: &[GodotVariant]) -> SceneTree {
    let args = if msg
        .first()
        .is_some_and(|v| matches!(v, GodotVariant::String(s) if s == "scene:scene_tree"))
    {
        &msg[1..]
    } else {
        msg
    };
    let mut offset = 0;
    if let Some(root) = parse_scene_node(args, &mut offset) {
        SceneTree { nodes: vec![root] }
    } else {
        SceneTree { nodes: Vec::new() }
    }
}

/// Parse a single scene node and its children recursively.
/// Each node: [child_count, name, class_name, object_id, scene_file_path, view_flags]
fn parse_scene_node(args: &[GodotVariant], offset: &mut usize) -> Option<SceneNode> {
    if *offset >= args.len() {
        return None;
    }

    let child_count = variant_as_usize(&args[*offset])?;
    *offset += 1;

    let name = variant_as_string(args.get(*offset)?).unwrap_or_default();
    *offset += 1;
    let class_name = variant_as_string(args.get(*offset)?).unwrap_or_default();
    *offset += 1;
    let object_id = variant_as_u64(args.get(*offset)?).unwrap_or(0);
    *offset += 1;
    let scene_file_path = variant_as_string(args.get(*offset)?);
    *offset += 1;
    // view_flags — skip
    *offset += 1;

    let mut children = Vec::new();
    for _ in 0..child_count {
        if let Some(child) = parse_scene_node(args, offset) {
            children.push(child);
        } else {
            break;
        }
    }

    let scene_file_path = scene_file_path.filter(|s| !s.is_empty());
    Some(SceneNode {
        name,
        class_name,
        object_id,
        scene_file_path,
        children,
    })
}

/// Parse object info from an inspect response.
///
/// Godot 4.6 format (after normalization):
/// `[String(cmd), Array([Int(id), String(class), Array([Array([prop_fields...]), ...])])]`
///
/// Each property is an Array of 6 elements:
/// `[String(name), Int(type), Int(hint), String(hint_string), Int(usage), Variant(value)]`
pub(super) fn parse_object_info(msg: &[GodotVariant]) -> Option<ObjectInfo> {
    // Strip command prefix
    let after_cmd = match msg.first() {
        Some(GodotVariant::String(s))
            if s == "scene:inspect_object" || s == "scene:inspect_objects" =>
        {
            &msg[1..]
        }
        _ => msg,
    };

    // Unwrap outer Array: [Array([id, class, Array([props...])])]
    let args: &[GodotVariant];
    let owned;
    if let Some(GodotVariant::Array(inner)) = after_cmd.first() {
        owned = inner.clone();
        args = &owned;
    } else {
        args = after_cmd;
    }

    if args.len() < 2 {
        return None;
    }
    let object_id = variant_as_u64(&args[0])?;
    let class_name = variant_as_string(&args[1])?;

    let mut properties = Vec::new();

    // Godot 4.6: args[2] is Array([Array([prop1...]), Array([prop2...]), ...])
    if let Some(GodotVariant::Array(prop_arrays)) = args.get(2) {
        for prop_arr in prop_arrays {
            if let GodotVariant::Array(fields) = prop_arr
                && fields.len() >= 6
            {
                let raw_name = variant_as_string(&fields[0]).unwrap_or_default();
                let name = strip_property_prefix(&raw_name);
                let type_id = variant_as_u32(&fields[1]).unwrap_or(0);
                let hint = variant_as_u32(&fields[2]).unwrap_or(0);
                let hint_string = variant_as_string(&fields[3]).unwrap_or_default();
                let usage = variant_as_u32(&fields[4]).unwrap_or(0);
                let value = fields[5].clone();
                properties.push(ObjectProperty {
                    name,
                    value,
                    type_id,
                    hint,
                    hint_string,
                    usage,
                });
            }
        }
    } else {
        // Fallback: flat format [name, type, hint, hint_string, usage, value, ...]
        let prop_data = &args[2..];
        for chunk in prop_data.chunks(6) {
            if chunk.len() < 6 {
                break;
            }
            let raw_name = variant_as_string(&chunk[0]).unwrap_or_default();
            let name = strip_property_prefix(&raw_name);
            let type_id = variant_as_u32(&chunk[1]).unwrap_or(0);
            let hint = variant_as_u32(&chunk[2]).unwrap_or(0);
            let hint_string = variant_as_string(&chunk[3]).unwrap_or_default();
            let usage = variant_as_u32(&chunk[4]).unwrap_or(0);
            let value = chunk[5].clone();
            properties.push(ObjectProperty {
                name,
                value,
                type_id,
                hint,
                hint_string,
                usage,
            });
        }
    }

    Some(ObjectInfo {
        object_id,
        class_name,
        properties,
    })
}

/// Strip Godot section prefixes from property names.
/// Godot's binary protocol sends names like "Members/position", "Constants/STATE_IDLE".
/// These prefixes are for categorization only — `set-prop` doesn't use them.
fn strip_property_prefix(name: &str) -> String {
    for prefix in &["Members/", "Constants/"] {
        if let Some(stripped) = name.strip_prefix(prefix) {
            return stripped.to_string();
        }
    }
    name.to_string()
}

// ---------------------------------------------------------------------------
// Variant helpers
// ---------------------------------------------------------------------------

pub(super) fn variant_as_string(v: &GodotVariant) -> Option<String> {
    match v {
        GodotVariant::String(s) | GodotVariant::StringName(s) => Some(s.clone()),
        _ => None,
    }
}

pub(super) fn variant_as_i32(v: &GodotVariant) -> Option<i32> {
    match v {
        GodotVariant::Int(i) => Some(*i as i32),
        _ => None,
    }
}

pub(super) fn variant_as_u32(v: &GodotVariant) -> Option<u32> {
    match v {
        GodotVariant::Int(i) => Some(*i as u32),
        _ => None,
    }
}

pub(super) fn variant_as_u64(v: &GodotVariant) -> Option<u64> {
    match v {
        GodotVariant::Int(i) => Some(*i as u64),
        GodotVariant::ObjectId(id) => Some(*id),
        _ => None,
    }
}

pub(super) fn variant_as_usize(v: &GodotVariant) -> Option<usize> {
    match v {
        GodotVariant::Int(i) => Some(*i as usize),
        _ => None,
    }
}
