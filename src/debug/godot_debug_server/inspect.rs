use std::time::{Duration, Instant};

use crate::debug::variant::GodotVariant;

use super::parsers::{parse_object_info, parse_scene_tree, variant_as_string};
use super::{GodotDebugServer, ObjectInfo, SceneTree};

impl GodotDebugServer {
    // ═══════════════════════════════════════════════════════════════════
    // Scene debugger commands (scene_debugger.cpp, prefix "scene:")
    // ═══════════════════════════════════════════════════════════════════

    // ── Scene tree ──

    pub fn cmd_request_scene_tree(&self) -> Option<SceneTree> {
        if !self.send_command("scene:request_scene_tree", &[]) {
            return None;
        }
        let msg = self.wait_message("scene:scene_tree", Duration::from_secs(5))?;
        Some(parse_scene_tree(&msg))
    }

    // ── Object inspection ──

    pub fn cmd_inspect_object(&self, object_id: u64) -> Option<ObjectInfo> {
        // Godot 4.2+: use inspect_objects (plural).
        // Format: [Array([id1, ...]), Bool(update_selection)]
        // Success response: "scene:inspect_objects" with serialized object data
        // Missing object: "remote_selection_invalidated" + "remote_nothing_selected"
        let ids_array = GodotVariant::Array(vec![GodotVariant::Int(object_id as i64)]);
        if !self.send_command(
            "scene:inspect_objects",
            &[ids_array, GodotVariant::Bool(false)],
        ) {
            return None;
        }
        // Drain stale responses until we get the one matching our object_id.
        // The inbox may contain leftover inspect_objects responses from prior requests.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            let msg = self.wait_message_any(
                &[
                    "scene:inspect_objects",
                    "remote_selection_invalidated",
                    "remote_nothing_selected",
                ],
                remaining,
            )?;

            match msg.first() {
                Some(GodotVariant::String(cmd)) if cmd == "scene:inspect_objects" => {
                    if let Some(info) = parse_object_info(&msg) {
                        if info.object_id == object_id {
                            return Some(info);
                        }
                        // Stale response for a different object — discard and keep waiting
                        continue;
                    }
                    return None;
                }
                Some(GodotVariant::String(cmd))
                    if cmd == "remote_selection_invalidated"
                        || cmd == "remote_nothing_selected" =>
                {
                    // Object not found in Godot's ObjectDB. Drain the paired message
                    // (invalidated comes with nothing_selected, or vice versa).
                    let _ = self.wait_message_any(
                        &["remote_nothing_selected", "remote_selection_invalidated"],
                        Duration::from_millis(500),
                    );
                    eprintln!("debug_server: object {object_id} not found in Godot's ObjectDB");
                    return None;
                }
                _ => return None,
            }
        }
    }

    /// Inspect multiple objects by issuing individual inspect commands.
    /// More reliable than sending all IDs in one batch — each object gets
    /// its own send/receive cycle so a missing object doesn't break the rest.
    #[allow(clippy::unnecessary_wraps)]
    pub fn cmd_inspect_objects(&self, ids: &[u64], _selection: bool) -> Option<Vec<ObjectInfo>> {
        let mut results = Vec::new();
        for &id in ids {
            if let Some(info) = self.cmd_inspect_object(id) {
                results.push(info);
            }
        }
        Some(results)
    }

    pub fn cmd_clear_selection(&self) -> bool {
        self.send_command("scene:clear_selection", &[])
    }

    /// Save a node to a file. Returns the saved file path from Godot's confirmation.
    pub fn cmd_save_node(&self, object_id: u64, path: &str) -> Option<String> {
        if !self.send_command(
            "scene:save_node",
            &[
                GodotVariant::Int(object_id as i64),
                GodotVariant::String(path.to_string()),
            ],
        ) {
            return None;
        }
        // Godot responds with "filesystem:update_file" [path]
        let msg = self.wait_message_any(&["filesystem:update_file"], Duration::from_secs(5));
        match msg {
            Some(ref m) if m.len() >= 2 => variant_as_string(&m[1]),
            _ => Some(path.to_string()), // Command sent, assume success
        }
    }

    // ── Property modification ──

    pub fn cmd_set_object_property(
        &self,
        object_id: u64,
        property: &str,
        value: GodotVariant,
    ) -> bool {
        self.send_command(
            "scene:set_object_property",
            &[
                GodotVariant::Int(object_id as i64),
                GodotVariant::String(property.to_string()),
                value,
            ],
        )
    }

    /// Set a specific field within a property (e.g. Vector3.x).
    pub fn cmd_set_object_property_field(
        &self,
        object_id: u64,
        property: &str,
        value: GodotVariant,
        field: &str,
    ) -> bool {
        self.send_command(
            "scene:set_object_property_field",
            &[
                GodotVariant::Int(object_id as i64),
                GodotVariant::String(property.to_string()),
                value,
                GodotVariant::String(field.to_string()),
            ],
        )
    }
}
