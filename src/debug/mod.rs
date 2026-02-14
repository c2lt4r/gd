pub mod dap_client;
pub mod enrich;
pub mod godot_debug_server;
pub mod variant;

use serde::Serialize;

/// A thread reported by the DAP server.
#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct DebugThread {
    pub id: i64,
    pub name: String,
}

/// A single stack frame.
#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct StackFrame {
    pub id: i64,
    pub name: String,
    pub file: String,
    pub line: u32,
}

/// A variable scope (Locals, Members, Globals).
#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct Scope {
    pub name: String,
    pub variables_reference: i64,
}

/// A runtime variable.
#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct Variable {
    pub name: String,
    pub value: String,
    #[serde(rename = "type")]
    pub type_name: String,
    /// Non-zero if this variable can be expanded (has children).
    #[serde(skip_serializing_if = "is_zero")]
    pub variables_reference: i64,
}

#[allow(dead_code)]
fn is_zero(v: &i64) -> bool {
    *v == 0
}

/// Result of setting a breakpoint.
#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct BreakpointResult {
    pub verified: bool,
    pub line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
}
