use crate::debug::variant::GodotVariant;

use super::GodotDebugServer;

impl GodotDebugServer {
    // ═══════════════════════════════════════════════════════════════════
    // Live editing commands (scene_debugger.cpp live editor)
    // ═══════════════════════════════════════════════════════════════════

    /// Set the root scene for live editing.
    pub fn cmd_live_set_root(&self, scene_path: &str, scene_file: &str) -> bool {
        self.send_command(
            "scene:live_set_root",
            &[
                GodotVariant::String(scene_path.to_string()),
                GodotVariant::String(scene_file.to_string()),
            ],
        )
    }

    /// Map a node path to an integer ID for live editing.
    pub fn cmd_live_node_path(&self, path: &str, id: i32) -> bool {
        self.send_command(
            "scene:live_node_path",
            &[
                GodotVariant::String(path.to_string()),
                GodotVariant::Int(i64::from(id)),
            ],
        )
    }

    /// Map a resource path to an integer ID for live editing.
    pub fn cmd_live_res_path(&self, path: &str, id: i32) -> bool {
        self.send_command(
            "scene:live_res_path",
            &[
                GodotVariant::String(path.to_string()),
                GodotVariant::Int(i64::from(id)),
            ],
        )
    }

    /// Set a property on a live-edited node by ID.
    pub fn cmd_live_node_prop(&self, id: i32, property: &str, value: GodotVariant) -> bool {
        self.send_command(
            "scene:live_node_prop",
            &[
                GodotVariant::Int(i64::from(id)),
                GodotVariant::String(property.to_string()),
                value,
            ],
        )
    }

    /// Set a property on a live-edited node to a resource path.
    pub fn cmd_live_node_prop_res(&self, id: i32, property: &str, res_path: &str) -> bool {
        self.send_command(
            "scene:live_node_prop_res",
            &[
                GodotVariant::Int(i64::from(id)),
                GodotVariant::String(property.to_string()),
                GodotVariant::String(res_path.to_string()),
            ],
        )
    }

    /// Set a property on a live-edited resource by ID.
    pub fn cmd_live_res_prop(&self, id: i32, property: &str, value: GodotVariant) -> bool {
        self.send_command(
            "scene:live_res_prop",
            &[
                GodotVariant::Int(i64::from(id)),
                GodotVariant::String(property.to_string()),
                value,
            ],
        )
    }

    /// Set a property on a live-edited resource to another resource path.
    pub fn cmd_live_res_prop_res(&self, id: i32, property: &str, res_path: &str) -> bool {
        self.send_command(
            "scene:live_res_prop_res",
            &[
                GodotVariant::Int(i64::from(id)),
                GodotVariant::String(property.to_string()),
                GodotVariant::String(res_path.to_string()),
            ],
        )
    }

    /// Call a method on a live-edited node.
    pub fn cmd_live_node_call(&self, id: i32, method: &str, args: &[GodotVariant]) -> bool {
        let mut cmd_args = vec![
            GodotVariant::Int(i64::from(id)),
            GodotVariant::String(method.to_string()),
        ];
        cmd_args.extend_from_slice(args);
        self.send_command("scene:live_node_call", &cmd_args)
    }

    /// Call a method on a live-edited resource.
    pub fn cmd_live_res_call(&self, id: i32, method: &str, args: &[GodotVariant]) -> bool {
        let mut cmd_args = vec![
            GodotVariant::Int(i64::from(id)),
            GodotVariant::String(method.to_string()),
        ];
        cmd_args.extend_from_slice(args);
        self.send_command("scene:live_res_call", &cmd_args)
    }

    /// Create a new node in the live scene.
    pub fn cmd_live_create_node(&self, parent_path: &str, class: &str, name: &str) -> bool {
        self.send_command(
            "scene:live_create_node",
            &[
                GodotVariant::String(parent_path.to_string()),
                GodotVariant::String(class.to_string()),
                GodotVariant::String(name.to_string()),
            ],
        )
    }

    /// Instantiate a packed scene as a child of a node.
    pub fn cmd_live_instantiate_node(
        &self,
        parent_path: &str,
        scene_path: &str,
        name: &str,
    ) -> bool {
        self.send_command(
            "scene:live_instantiate_node",
            &[
                GodotVariant::String(parent_path.to_string()),
                GodotVariant::String(scene_path.to_string()),
                GodotVariant::String(name.to_string()),
            ],
        )
    }

    /// Remove a node from the live scene.
    pub fn cmd_live_remove_node(&self, path: &str) -> bool {
        self.send_command(
            "scene:live_remove_node",
            &[GodotVariant::String(path.to_string())],
        )
    }

    /// Remove a node but keep it (for later restore).
    pub fn cmd_live_remove_and_keep_node(&self, path: &str, object_id: u64) -> bool {
        self.send_command(
            "scene:live_remove_and_keep_node",
            &[
                GodotVariant::String(path.to_string()),
                GodotVariant::Int(object_id as i64),
            ],
        )
    }

    /// Restore a previously removed-and-kept node.
    pub fn cmd_live_restore_node(&self, object_id: u64, path: &str, pos: i32) -> bool {
        self.send_command(
            "scene:live_restore_node",
            &[
                GodotVariant::Int(object_id as i64),
                GodotVariant::String(path.to_string()),
                GodotVariant::Int(i64::from(pos)),
            ],
        )
    }

    /// Duplicate a node in the live scene.
    pub fn cmd_live_duplicate_node(&self, path: &str, new_name: &str) -> bool {
        self.send_command(
            "scene:live_duplicate_node",
            &[
                GodotVariant::String(path.to_string()),
                GodotVariant::String(new_name.to_string()),
            ],
        )
    }

    /// Reparent a node in the live scene.
    pub fn cmd_live_reparent_node(
        &self,
        path: &str,
        new_parent: &str,
        new_name: &str,
        pos: i32,
    ) -> bool {
        self.send_command(
            "scene:live_reparent_node",
            &[
                GodotVariant::String(path.to_string()),
                GodotVariant::String(new_parent.to_string()),
                GodotVariant::String(new_name.to_string()),
                GodotVariant::Int(i64::from(pos)),
            ],
        )
    }
}
