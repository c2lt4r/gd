use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;

use crate::debug::variant::GodotVariant;

use super::inbox::{Inbox, msg_matches};
use super::parsers::{
    parse_debug_variable, parse_eval_result, parse_object_info, parse_scene_tree, parse_stack_dump,
    parse_var_counts, variant_as_i32, variant_as_string, variant_as_u32, variant_as_u64,
    variant_as_usize,
};
use super::{GodotDebugServer, normalize_message};

#[test]
fn test_parse_stack_dump() {
    let msg = vec![
        GodotVariant::String("stack_dump".into()),
        GodotVariant::String("res://main.gd".into()),
        GodotVariant::Int(15),
        GodotVariant::String("_ready".into()),
        GodotVariant::String("res://player.gd".into()),
        GodotVariant::Int(42),
        GodotVariant::String("move".into()),
    ];
    let frames = parse_stack_dump(&msg);
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].file, "res://main.gd");
    assert_eq!(frames[0].line, 15);
    assert_eq!(frames[0].function, "_ready");
    assert_eq!(frames[1].file, "res://player.gd");
    assert_eq!(frames[1].line, 42);
    assert_eq!(frames[1].function, "move");
}

#[test]
fn test_parse_stack_dump_empty() {
    let msg = vec![GodotVariant::String("stack_dump".into())];
    let frames = parse_stack_dump(&msg);
    assert!(frames.is_empty());
}

#[test]
fn test_parse_var_counts() {
    let msg = vec![
        GodotVariant::String("stack_frame_vars".into()),
        GodotVariant::Int(3),
        GodotVariant::Int(2),
        GodotVariant::Int(1),
    ];
    let (l, m, g) = parse_var_counts(&msg).unwrap();
    assert_eq!(l, 3);
    assert_eq!(m, 2);
    assert_eq!(g, 1);
}

#[test]
fn test_parse_var_counts_missing_returns_none() {
    let msg = vec![
        GodotVariant::String("stack_frame_vars".into()),
        GodotVariant::Int(3),
    ];
    assert!(parse_var_counts(&msg).is_none());
}

#[test]
fn test_parse_debug_variable() {
    // Godot ScriptStackVariable.serialize(): [name, scope_type, variant_type_id, value]
    let msg = vec![
        GodotVariant::String("stack_frame_var".into()),
        GodotVariant::String("health".into()),
        GodotVariant::Int(0), // scope_type (0=local)
        GodotVariant::Int(2), // variant_type_id (2=Int)
        GodotVariant::Int(100),
    ];
    let v = parse_debug_variable(&msg).unwrap();
    assert_eq!(v.name, "health");
    assert_eq!(v.var_type, 2);
    assert_eq!(v.value, GodotVariant::Int(100));
}

#[test]
fn test_parse_eval_result() {
    // Godot ScriptStackVariable.serialize(): [name, scope_type=3, variant_type_id, value]
    let msg = vec![
        GodotVariant::String("evaluation_return".into()),
        GodotVariant::String("2 + 2".into()),
        GodotVariant::Int(3), // scope_type (3=eval)
        GodotVariant::Int(2), // variant_type_id (2=Int)
        GodotVariant::Int(4),
    ];
    let r = parse_eval_result(&msg).unwrap();
    assert_eq!(r.name, "2 + 2");
    assert_eq!(r.var_type, 2);
    assert_eq!(r.value, GodotVariant::Int(4));
}

#[test]
fn test_parse_eval_result_vector3() {
    let msg = vec![
        GodotVariant::String("evaluation_return".into()),
        GodotVariant::String("Vector3(1,2,3)".into()),
        GodotVariant::Int(3), // scope_type
        GodotVariant::Int(9), // variant_type_id (9=Vector3)
        GodotVariant::Vector3(1.0, 2.0, 3.0),
    ];
    let r = parse_eval_result(&msg).unwrap();
    assert_eq!(r.name, "Vector3(1,2,3)");
    assert_eq!(r.var_type, 9);
    assert_eq!(r.value, GodotVariant::Vector3(1.0, 2.0, 3.0));
}

#[test]
fn test_parse_object_info_nested() {
    // Godot 4.6 format: [cmd, Array([id, class, Array([Array([prop...]), ...])])]
    let msg = vec![
        GodotVariant::String("scene:inspect_objects".into()),
        GodotVariant::Array(vec![
            GodotVariant::Int(1234),
            GodotVariant::String("Node2D".into()),
            GodotVariant::Array(vec![GodotVariant::Array(vec![
                GodotVariant::String("position".into()),
                GodotVariant::Int(5), // TYPE_VECTOR2
                GodotVariant::Int(0),
                GodotVariant::String(String::new()),
                GodotVariant::Int(6),
                GodotVariant::Vector2(10.0, 20.0),
            ])]),
        ]),
    ];
    let info = parse_object_info(&msg).unwrap();
    assert_eq!(info.object_id, 1234);
    assert_eq!(info.class_name, "Node2D");
    assert_eq!(info.properties.len(), 1);
    assert_eq!(info.properties[0].name, "position");
    assert_eq!(info.properties[0].type_id, 5);
}

#[test]
fn test_parse_object_info_flat_fallback() {
    // Legacy flat format
    let msg = vec![
        GodotVariant::String("scene:inspect_object".into()),
        GodotVariant::Int(1234),
        GodotVariant::String("Node2D".into()),
        GodotVariant::String("position".into()),
        GodotVariant::Int(5),
        GodotVariant::Int(0),
        GodotVariant::String(String::new()),
        GodotVariant::Int(6),
        GodotVariant::Vector2(10.0, 20.0),
    ];
    let info = parse_object_info(&msg).unwrap();
    assert_eq!(info.object_id, 1234);
    assert_eq!(info.class_name, "Node2D");
    assert_eq!(info.properties.len(), 1);
    assert_eq!(info.properties[0].name, "position");
}

#[test]
fn test_normalize_message() {
    // Godot 4.2+ wire format: [cmd, thread_id, Array(data)]
    let msg = vec![
        GodotVariant::String("debug_enter".into()),
        GodotVariant::Int(1), // thread_id
        GodotVariant::Array(vec![
            GodotVariant::Bool(true),
            GodotVariant::String("Breakpoint".into()),
            GodotVariant::Bool(true),
        ]),
    ];
    let normalized = normalize_message(msg);
    assert_eq!(normalized.len(), 4);
    assert_eq!(normalized[0], GodotVariant::String("debug_enter".into()));
    assert_eq!(normalized[1], GodotVariant::Bool(true));
    assert_eq!(normalized[2], GodotVariant::String("Breakpoint".into()));
}

#[test]
fn test_normalize_message_empty_data() {
    // scene:scene_tree with actual tree data in the inner array
    let msg = vec![
        GodotVariant::String("scene:scene_tree".into()),
        GodotVariant::Int(1),
        GodotVariant::Array(vec![GodotVariant::Int(2)]), // child count
    ];
    let normalized = normalize_message(msg);
    assert_eq!(normalized.len(), 2);
    assert_eq!(
        normalized[0],
        GodotVariant::String("scene:scene_tree".into())
    );
    assert_eq!(normalized[1], GodotVariant::Int(2));
}

#[test]
fn test_msg_matches() {
    let msg = vec![
        GodotVariant::String("stack_dump".into()),
        GodotVariant::Int(1),
    ];
    assert!(msg_matches(&msg, "stack_dump"));
    assert!(!msg_matches(&msg, "debug_enter"));
}

#[test]
fn test_server_new_and_port() {
    let server = GodotDebugServer::new(0).unwrap();
    assert!(server.port() > 0);
    assert!(!server.is_connected());
}

#[test]
fn test_accept_timeout() {
    let server = GodotDebugServer::new(0).unwrap();
    // Accept with very short timeout should return false (no one connecting)
    assert!(!server.accept(Duration::from_millis(10)));
}

#[test]
fn test_send_without_connection() {
    let server = GodotDebugServer::new(0).unwrap();
    assert!(!server.send_command("continue", &[]));
}

#[test]
fn test_connection_and_send() {
    let server = GodotDebugServer::new(0).unwrap();
    let port = server.port();

    // Simulate a game connecting
    let handle = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        TcpStream::connect(format!("127.0.0.1:{port}")).unwrap()
    });

    assert!(server.accept(Duration::from_secs(2)));
    assert!(server.is_connected());
    assert!(server.send_command("continue", &[]));

    let _client = handle.join().unwrap();
}

#[test]
fn test_inbox_push_and_wait() {
    let inbox = Inbox::new();
    inbox.push(vec![
        GodotVariant::String("stack_dump".into()),
        GodotVariant::String("res://main.gd".into()),
        GodotVariant::Int(10),
        GodotVariant::String("_ready".into()),
    ]);

    let msg = inbox
        .wait_for("stack_dump", Duration::from_secs(1))
        .unwrap();
    assert_eq!(msg.len(), 4);
}

#[test]
fn test_inbox_wait_timeout() {
    let inbox = Inbox::new();
    let result = inbox.wait_for("stack_dump", Duration::from_millis(10));
    assert!(result.is_none());
}

#[test]
fn test_inbox_wait_concurrent() {
    let inbox = Arc::new(Inbox::new());
    let inbox2 = Arc::clone(&inbox);

    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        inbox2.push(vec![GodotVariant::String("debug_enter".into())]);
    });

    let msg = inbox
        .wait_for("debug_enter", Duration::from_secs(2))
        .unwrap();
    assert_eq!(msg.len(), 1);
}

#[test]
fn test_inbox_wait_for_any() {
    let inbox = Inbox::new();
    // Push a "remote_nothing_selected" message
    inbox.push(vec![
        GodotVariant::String("remote_nothing_selected".into()),
        GodotVariant::Int(0),
    ]);
    // wait_for_any should match it
    let msg = inbox
        .wait_for_any(
            &[
                "scene:inspect_objects",
                "remote_nothing_selected",
                "remote_selection_invalidated",
            ],
            Duration::from_secs(1),
        )
        .unwrap();
    assert_eq!(
        msg[0],
        GodotVariant::String("remote_nothing_selected".into())
    );
}

#[test]
fn test_inbox_wait_for_any_prefers_first_match() {
    let inbox = Inbox::new();
    // Push two messages
    inbox.push(vec![GodotVariant::String(
        "remote_selection_invalidated".into(),
    )]);
    inbox.push(vec![GodotVariant::String("scene:inspect_objects".into())]);
    // Should return the first one found (insertion order)
    let msg = inbox
        .wait_for_any(
            &["scene:inspect_objects", "remote_selection_invalidated"],
            Duration::from_secs(1),
        )
        .unwrap();
    assert_eq!(
        msg[0],
        GodotVariant::String("remote_selection_invalidated".into())
    );
}

#[test]
fn test_parse_scene_tree() {
    // Root node with 1 child, that child has 0 children
    // Format per node: [child_count, name, class, id, scene_file, view_flags]
    let msg = vec![
        GodotVariant::String("scene:scene_tree".into()),
        // Root node
        GodotVariant::Int(1),                  // child_count
        GodotVariant::String("root".into()),   // name
        GodotVariant::String("Window".into()), // class
        GodotVariant::Int(1234),               // object_id
        GodotVariant::String(String::new()),   // scene_file_path
        GodotVariant::Int(0),                  // view_flags
        // Child node
        GodotVariant::Int(0),                             // child_count
        GodotVariant::String("Player".into()),            // name
        GodotVariant::String("CharacterBody3D".into()),   // class
        GodotVariant::Int(5678),                          // object_id
        GodotVariant::String("res://player.tscn".into()), // scene_file_path
        GodotVariant::Int(0),                             // view_flags
    ];
    let tree = parse_scene_tree(&msg);
    assert_eq!(tree.nodes.len(), 1); // single root
    let root = &tree.nodes[0];
    assert_eq!(root.name, "root");
    assert_eq!(root.class_name, "Window");
    assert_eq!(root.object_id, 1234);
    assert_eq!(root.children.len(), 1);
    let child = &root.children[0];
    assert_eq!(child.name, "Player");
    assert_eq!(child.class_name, "CharacterBody3D");
    assert_eq!(child.scene_file_path.as_deref(), Some("res://player.tscn"));
    assert!(child.children.is_empty());
}

#[test]
fn test_variant_helpers() {
    assert_eq!(
        variant_as_string(&GodotVariant::String("hello".into())),
        Some("hello".to_string())
    );
    assert_eq!(variant_as_i32(&GodotVariant::Int(42)), Some(42));
    assert_eq!(variant_as_u32(&GodotVariant::Int(42)), Some(42));
    assert_eq!(variant_as_u64(&GodotVariant::Int(42)), Some(42));
    assert_eq!(variant_as_u64(&GodotVariant::ObjectId(99)), Some(99));
    assert_eq!(variant_as_usize(&GodotVariant::Int(5)), Some(5));
    assert!(variant_as_string(&GodotVariant::Int(1)).is_none());
    assert!(variant_as_i32(&GodotVariant::Nil).is_none());
}
