use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::Mutex;
use std::time::Duration;

/// Synchronous TCP client for Godot's built-in LSP server.
///
/// Godot runs an LSP server on port 6005 (by default) when the editor is open.
/// This client sends JSON-RPC requests and reads responses over the LSP wire
/// protocol (Content-Length framing).
pub struct GodotClient {
    stream: Mutex<BufReader<TcpStream>>,
    next_id: Mutex<i64>,
    /// Our local URI prefix (e.g. `file:///mnt/c/users/.../project`).
    local_prefix: Mutex<String>,
    /// Godot's URI prefix (e.g. `file:///C:/Users/.../project`).
    godot_prefix: Mutex<String>,
}

impl GodotClient {
    /// Try to connect to Godot's LSP server. Returns None if connection fails
    /// (e.g., Godot editor is not running).
    pub fn connect(host: &str, port: u16) -> Option<Self> {
        Self::connect_with_timeout(host, port, Duration::from_secs(2))
    }

    fn connect_with_timeout(host: &str, port: u16, timeout: Duration) -> Option<Self> {
        let addr = format!("{host}:{port}");
        let stream = TcpStream::connect_timeout(&addr.parse().ok()?, timeout).ok()?;

        stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .ok()?;

        Some(Self {
            stream: Mutex::new(BufReader::new(stream)),
            next_id: Mutex::new(1),
            local_prefix: Mutex::new(String::new()),
            godot_prefix: Mutex::new(String::new()),
        })
    }

    /// Send the LSP initialize handshake. `local_root` is our project root
    /// as a local filesystem path (e.g. `/mnt/c/users/.../project`).
    pub fn initialize(&self, local_root: &std::path::Path) -> Option<Value> {
        let params = serde_json::json!({
            "processId": std::process::id(),
            "rootUri": null,
            "capabilities": {},
        });
        let result = self.send_request("initialize", params)?;

        // Send initialized notification (no response expected)
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        });
        self.send_raw(&notification).ok()?;

        // Capture Godot's workspace path from the changeWorkspace notification
        // that arrives during init, and cache the URI mapping.
        self.discover_uri_mapping(local_root);

        Some(result)
    }

    /// Return the Godot-side project root path (e.g. `C:/Users/.../project`).
    /// Strips the `file:///` prefix from the stored `godot_prefix`.
    /// Returns `None` if the URI mapping hasn't been established yet.
    pub fn godot_project_path(&self) -> Option<String> {
        let pfx = self.godot_prefix.lock().unwrap();
        if pfx.is_empty() {
            return None;
        }
        Some(pfx.strip_prefix("file:///").unwrap_or(&pfx).to_string())
    }

    /// Convert a local URI to a Godot-compatible URI by replacing the
    /// path prefix. Returns the URI unchanged if no mapping is needed.
    pub fn to_godot_uri(&self, local_uri: &str) -> String {
        let local_pfx = self.local_prefix.lock().unwrap();
        let godot_pfx = self.godot_prefix.lock().unwrap();
        if !local_pfx.is_empty()
            && !godot_pfx.is_empty()
            && *local_pfx != *godot_pfx
            && let Some(rest) = local_uri.strip_prefix(local_pfx.as_str())
        {
            return format!("{godot_pfx}{rest}");
        }
        local_uri.to_string()
    }

    /// Set the local URI prefix and finalize the URI mapping.
    /// The `godot_prefix` may already be set from a `changeWorkspace` notification
    /// captured during `read_response`. If not, assume paths match (native platform).
    fn discover_uri_mapping(&self, local_root: &std::path::Path) {
        use path_slash::PathExt;

        let local_slash = local_root.to_slash_lossy();
        let local_uri_prefix = format!("file:///{}", local_slash.trim_start_matches('/'));
        *self.local_prefix.lock().unwrap() = local_uri_prefix.clone();

        // If godot_prefix was already set from a changeWorkspace notification
        // during read_response, keep it. Otherwise drain remaining init
        // notifications to look for it.
        if !self.godot_prefix.lock().unwrap().is_empty() {
            return;
        }

        // Drain remaining init notifications (gdscript/capabilities, etc.)
        // Use a short timeout — changeWorkspace usually arrives in the first few messages.
        {
            let mut stream = self.stream.lock().unwrap();
            stream
                .get_mut()
                .set_read_timeout(Some(Duration::from_millis(500)))
                .ok();

            for _ in 0..10 {
                let Some(cl) = read_content_length(&mut *stream) else {
                    break;
                };
                let mut body = vec![0u8; cl];
                if stream.read_exact(&mut body).is_err() {
                    break;
                }
                if let Ok(msg) = serde_json::from_slice::<Value>(&body)
                    && msg.get("method").and_then(|m| m.as_str())
                        == Some("gdscript_client/changeWorkspace")
                    && let Some(path) = msg.pointer("/params/path").and_then(|p| p.as_str())
                {
                    *self.godot_prefix.lock().unwrap() = format!("file:///{path}");
                    return;
                }
            }

            stream
                .get_mut()
                .set_read_timeout(Some(Duration::from_secs(5)))
                .ok();
        }

        // No changeWorkspace — paths likely match (native platform).
        *self.godot_prefix.lock().unwrap() = local_uri_prefix;
    }

    /// Notify Godot that a file was opened.
    #[allow(dead_code)]
    pub fn did_open(&self, uri: &str, content: &str) {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "languageId": "gdscript",
                    "version": 1,
                    "text": content
                }
            }
        });
        let _ = self.send_raw(&notification);
    }

    /// Request hover information from Godot.
    pub fn hover(&self, uri: &str, line: u32, character: u32) -> Option<Value> {
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character }
        });
        self.send_request("textDocument/hover", params)
    }

    /// Request completions from Godot.
    pub fn completion(&self, uri: &str, line: u32, character: u32) -> Option<Value> {
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character },
            "context": { "triggerKind": 1 }
        });
        self.send_request("textDocument/completion", params)
    }

    /// Request go-to-definition from Godot.
    pub fn definition(&self, uri: &str, line: u32, character: u32) -> Option<Value> {
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character }
        });
        self.send_request("textDocument/definition", params)
    }

    // ── Internal ──────────────────────────────────────────────────────

    fn next_id(&self) -> i64 {
        let mut id = self.next_id.lock().unwrap();
        let current = *id;
        *id += 1;
        current
    }

    fn send_request(&self, method: &str, params: Value) -> Option<Value> {
        let id = self.next_id();
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        self.send_raw(&msg).ok()?;
        self.read_response(id)
    }

    fn send_raw(&self, msg: &Value) -> std::io::Result<()> {
        let body = serde_json::to_string(msg)?;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());

        let mut stream = self.stream.lock().unwrap();
        let writer = stream.get_mut();
        writer.write_all(header.as_bytes())?;
        writer.write_all(body.as_bytes())?;
        writer.flush()
    }

    fn read_response(&self, expected_id: i64) -> Option<Value> {
        let mut stream = self.stream.lock().unwrap();

        // Read messages until we find the response matching our request ID.
        // Godot may send notifications (diagnostics, changeWorkspace, etc.)
        // between our request and its response — capture workspace path if seen.
        for _ in 0..20 {
            let content_length = read_content_length(&mut *stream)?;
            let mut body = vec![0u8; content_length];
            stream.read_exact(&mut body).ok()?;

            let msg: Value = serde_json::from_slice(&body).ok()?;

            // Check if this is the response to our request
            if let Some(id) = msg.get("id")
                && id.as_i64() == Some(expected_id)
            {
                return msg.get("result").cloned();
            }

            // Capture workspace path from changeWorkspace notification
            if msg.get("method").and_then(|m| m.as_str()) == Some("gdscript_client/changeWorkspace")
                && let Some(path) = msg.pointer("/params/path").and_then(|p| p.as_str())
            {
                *self.godot_prefix.lock().unwrap() = format!("file:///{path}");
            }
        }

        None
    }
}

/// Read the Content-Length header from the LSP stream.
fn read_content_length(reader: &mut impl BufRead) -> Option<usize> {
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            // End of headers — but we haven't found Content-Length yet
            // This shouldn't happen in valid LSP, but bail
            return None;
        }
        if let Some(len_str) = trimmed.strip_prefix("Content-Length:") {
            let len: usize = len_str.trim().parse().ok()?;
            // Consume remaining headers until empty line
            loop {
                line.clear();
                if reader.read_line(&mut line).ok()? == 0 {
                    return Some(len);
                }
                if line.trim().is_empty() {
                    return Some(len);
                }
            }
        }
    }
}
