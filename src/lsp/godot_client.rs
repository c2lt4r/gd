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
}

impl GodotClient {
    /// Try to connect to Godot's LSP server. Returns None if connection fails
    /// (e.g., Godot editor is not running).
    pub fn connect(host: &str, port: u16) -> Option<Self> {
        let addr = format!("{host}:{port}");
        let stream =
            TcpStream::connect_timeout(&addr.parse().ok()?, Duration::from_secs(2)).ok()?;

        stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .ok()?;

        Some(Self {
            stream: Mutex::new(BufReader::new(stream)),
            next_id: Mutex::new(1),
        })
    }

    /// Send the LSP initialize handshake.
    pub fn initialize(&self) -> Option<Value> {
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

        Some(result)
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
            "position": { "line": line, "character": character }
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

        // Read responses until we find the one matching our request ID.
        // Godot may send notifications (diagnostics, etc.) between our
        // request and its response.
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
            // Otherwise it's a notification — skip it
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
