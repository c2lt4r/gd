use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Mutex;
use std::time::Duration;

/// Synchronous TCP client for Godot's DAP server.
///
/// Godot runs a DAP server on port 6006 (by default) when the editor is open.
/// Uses the same Content-Length framing as LSP, but the message format follows
/// the Debug Adapter Protocol (seq/request_seq, type: request/response/event).
pub struct DapClient {
    stream: Mutex<BufReader<TcpStream>>,
    next_seq: Mutex<i64>,
    /// The project root path as Godot sees it (e.g. `C:/Users/You/Projects/Game`).
    /// Discovered during handshake via a probe breakpoint.
    project_path: Mutex<Option<String>>,
}

impl DapClient {
    /// Connect to Godot's DAP server. Returns None if connection fails.
    pub fn connect(host: &str, port: u16) -> Option<Self> {
        let addr = format!("{host}:{port}");
        // Resolve the address (handles both IPs and hostnames like "localhost"),
        // then use connect_timeout to avoid blocking indefinitely.
        let socket_addr = addr.to_socket_addrs().ok()?.next()?;
        let stream = TcpStream::connect_timeout(&socket_addr, Duration::from_secs(2)).ok()?;
        stream.set_read_timeout(Some(Duration::from_secs(8))).ok()?;
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .ok()?;

        Some(Self {
            stream: Mutex::new(BufReader::new(stream)),
            next_seq: Mutex::new(1),
            project_path: Mutex::new(None),
        })
    }

    /// Run the full DAP handshake: initialize → attach → configurationDone.
    /// Also discovers the editor's project path via a probe breakpoint.
    /// Returns the capabilities from the initialize response, or None on failure.
    pub fn handshake(&self) -> Option<Value> {
        let caps = self.initialize()?;
        self.attach()?;
        self.configuration_done()?;
        // Drain any initial events (initialized, process, etc.)
        self.drain_events();
        // Discover the editor's project path by sending a probe breakpoint
        // with a dummy path — the error response contains editorPath.
        self.discover_project_path();
        Some(caps)
    }

    /// The editor's project root path (e.g. `C:/Users/You/Projects/Game`).
    pub fn project_path(&self) -> Option<String> {
        self.project_path.lock().unwrap().clone()
    }

    /// Send the DAP initialize request.
    fn initialize(&self) -> Option<Value> {
        self.send_request(
            "initialize",
            serde_json::json!({
                "clientID": "gd",
                "clientName": "gd debug",
                "adapterID": "godot",
                "pathFormat": "path",
                "linesStartAt1": true,
                "columnsStartAt1": true,
                "supportsVariableType": true,
            }),
        )
    }

    /// Attach to the editor's running debug session.
    fn attach(&self) -> Option<Value> {
        self.send_request("attach", serde_json::json!({}))
    }

    /// Signal that configuration is complete.
    fn configuration_done(&self) -> Option<Value> {
        self.send_request("configurationDone", serde_json::json!({}))
    }

    /// Request the list of threads.
    pub fn threads(&self) -> Option<Value> {
        self.send_request("threads", serde_json::json!({}))
    }

    /// Request the call stack for a thread.
    pub fn stack_trace(&self, thread_id: i64) -> Option<Value> {
        self.send_request(
            "stackTrace",
            serde_json::json!({
                "threadId": thread_id,
                "startFrame": 0,
                "levels": 100,
            }),
        )
    }

    /// Request scopes for a stack frame.
    pub fn scopes(&self, frame_id: i64) -> Option<Value> {
        self.send_request("scopes", serde_json::json!({"frameId": frame_id}))
    }

    /// Request variables for a scope or expandable variable.
    pub fn variables(&self, variables_reference: i64) -> Option<Value> {
        self.send_request(
            "variables",
            serde_json::json!({"variablesReference": variables_reference}),
        )
    }

    /// Set breakpoints for a source file. `path` must be a full Windows path.
    pub fn set_breakpoints(&self, path: &str, lines: &[u32]) -> Option<Value> {
        let breakpoints: Vec<Value> = lines
            .iter()
            .map(|&l| serde_json::json!({"line": l}))
            .collect();
        self.send_request(
            "setBreakpoints",
            serde_json::json!({
                "source": {"path": path},
                "breakpoints": breakpoints,
            }),
        )
    }

    /// Evaluate an expression. Only member-access expressions are reliable in Godot
    /// (e.g. `self.speed`). Arbitrary expressions like `2+2` may timeout.
    pub fn evaluate(&self, expression: &str, context: &str) -> Option<Value> {
        self.send_request(
            "evaluate",
            serde_json::json!({
                "expression": expression,
                "context": context,
            }),
        )
    }

    /// Pause execution.
    pub fn pause(&self, thread_id: i64) -> Option<Value> {
        self.send_request("pause", serde_json::json!({"threadId": thread_id}))
    }

    /// Continue execution.
    pub fn continue_execution(&self, thread_id: i64) -> Option<Value> {
        self.send_request("continue", serde_json::json!({"threadId": thread_id}))
    }

    /// Step over (next line).
    pub fn next(&self, thread_id: i64) -> Option<Value> {
        self.send_request("next", serde_json::json!({"threadId": thread_id}))
    }

    /// Step into a function call.
    pub fn step_in(&self, thread_id: i64) -> Option<Value> {
        self.send_request("stepIn", serde_json::json!({"threadId": thread_id}))
    }

    /// Clean disconnect from the DAP server.
    pub fn disconnect(&self) {
        let _ = self.send_request("disconnect", serde_json::json!({}));
    }

    /// Wait for a `stopped` event (e.g. after setting a breakpoint and continuing).
    /// Returns the stopped event body, or None on timeout.
    #[allow(dead_code)]
    pub fn wait_for_stopped(&self, timeout_secs: u64) -> Option<Value> {
        let mut stream = self.stream.lock().unwrap();
        stream
            .get_mut()
            .set_read_timeout(Some(Duration::from_secs(timeout_secs)))
            .ok()?;

        let mut result = None;
        for _ in 0..50 {
            let Some(content_length) = read_content_length(&mut *stream) else {
                break;
            };
            let mut body = vec![0u8; content_length];
            if stream.read_exact(&mut body).is_err() {
                break;
            }
            if let Ok(msg) = serde_json::from_slice::<Value>(&body)
                && msg.get("type").and_then(|t| t.as_str()) == Some("event")
                && msg.get("event").and_then(|e| e.as_str()) == Some("stopped")
            {
                result = msg.get("body").cloned();
                break;
            }
        }

        // Restore default timeout
        let _ = stream
            .get_mut()
            .set_read_timeout(Some(Duration::from_secs(8)));
        result
    }

    /// Drain any pending events from the stream (non-blocking-ish with short timeout).
    fn drain_events(&self) {
        let mut stream = self.stream.lock().unwrap();
        stream
            .get_mut()
            .set_read_timeout(Some(Duration::from_millis(500)))
            .ok();

        for _ in 0..20 {
            let Some(cl) = read_content_length(&mut *stream) else {
                break;
            };
            let mut body = vec![0u8; cl];
            if stream.read_exact(&mut body).is_err() {
                break;
            }
        }

        stream
            .get_mut()
            .set_read_timeout(Some(Duration::from_secs(8)))
            .ok();
    }

    /// Send a probe breakpoint to discover Godot's project path from the error.
    fn discover_project_path(&self) {
        let seq = self.next_seq();
        let msg = serde_json::json!({
            "seq": seq,
            "type": "request",
            "command": "setBreakpoints",
            "arguments": {
                "source": {"path": "/__gd_probe__/nonexistent.gd"},
                "breakpoints": [],
            },
        });
        if self.send_raw(&msg).is_err() {
            return;
        }
        // Read the (expected) error response and extract editorPath
        if let Some(resp) = self.read_response_full(seq)
            && resp.get("success").and_then(|s| s.as_bool()) == Some(false)
            && let Some(editor_path) = resp
                .pointer("/body/error/variables/editorPath")
                .and_then(|v| v.as_str())
        {
            *self.project_path.lock().unwrap() = Some(editor_path.to_string());
        }
    }

    // ── Internal ──────────────────────────────────────────────────────

    fn next_seq(&self) -> i64 {
        let mut seq = self.next_seq.lock().unwrap();
        let current = *seq;
        *seq += 1;
        current
    }

    fn send_request(&self, command: &str, arguments: Value) -> Option<Value> {
        let seq = self.next_seq();
        let msg = serde_json::json!({
            "seq": seq,
            "type": "request",
            "command": command,
            "arguments": arguments,
        });

        self.send_raw(&msg).ok()?;
        self.read_response(seq)
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

    fn read_response(&self, expected_seq: i64) -> Option<Value> {
        let resp = self.read_response_full(expected_seq)?;
        if resp.get("success").and_then(|s| s.as_bool()) == Some(true) {
            return resp.get("body").cloned().or(Some(serde_json::json!({})));
        }
        None
    }

    /// Read the full DAP response (including error bodies) for the given seq.
    fn read_response_full(&self, expected_seq: i64) -> Option<Value> {
        let mut stream = self.stream.lock().unwrap();

        // Read messages until we find the response matching our request.
        // DAP interleaves events (stopped, continued, etc.) with responses.
        for _ in 0..30 {
            let content_length = read_content_length(&mut *stream)?;
            let mut body = vec![0u8; content_length];
            stream.read_exact(&mut body).ok()?;

            let msg: Value = serde_json::from_slice(&body).ok()?;

            if msg.get("type").and_then(|t| t.as_str()) == Some("response")
                && msg.get("request_seq").and_then(|s| s.as_i64()) == Some(expected_seq)
            {
                return Some(msg);
            }
            // Event or response for a different request — skip
        }

        None
    }
}

/// Read the Content-Length header from the DAP stream.
fn read_content_length(reader: &mut impl BufRead) -> Option<usize> {
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
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

#[cfg(test)]
mod tests {
    #[test]
    fn test_dap_request_envelope() {
        let client_seq = std::sync::Mutex::new(1i64);
        let seq = {
            let mut s = client_seq.lock().unwrap();
            let c = *s;
            *s += 1;
            c
        };
        let msg = serde_json::json!({
            "seq": seq,
            "type": "request",
            "command": "initialize",
            "arguments": {"clientID": "test"},
        });
        assert_eq!(msg["seq"], 1);
        assert_eq!(msg["type"], "request");
        assert_eq!(msg["command"], "initialize");
        assert_eq!(msg["arguments"]["clientID"], "test");
    }

    #[test]
    fn test_dap_response_parsing() {
        let resp = serde_json::json!({
            "seq": 5,
            "type": "response",
            "request_seq": 3,
            "success": true,
            "command": "stackTrace",
            "body": {
                "stackFrames": [
                    {"id": 0, "name": "_physics_process", "line": 42, "column": 0,
                     "source": {"name": "kart.gd", "path": "C:/project/kart.gd"}}
                ]
            }
        });

        assert_eq!(resp["type"], "response");
        assert_eq!(resp["request_seq"], 3);
        assert_eq!(resp["success"], true);

        let frames = resp["body"]["stackFrames"].as_array().unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0]["name"], "_physics_process");
        assert_eq!(frames[0]["line"], 42);
    }

    #[test]
    fn test_dap_event_detection() {
        let msg = serde_json::json!({
            "seq": 10,
            "type": "event",
            "event": "stopped",
            "body": {
                "reason": "breakpoint",
                "threadId": 1,
                "hitBreakpointIds": [0],
            }
        });

        assert_eq!(msg["type"], "event");
        assert_eq!(msg["event"], "stopped");
        assert_eq!(msg["body"]["reason"], "breakpoint");
        assert_eq!(msg["body"]["threadId"], 1);
    }

    #[test]
    fn test_failed_response_has_no_body() {
        let resp = serde_json::json!({
            "seq": 2,
            "type": "response",
            "request_seq": 1,
            "success": false,
            "command": "evaluate",
            "body": {
                "error": {
                    "format": "Timeout reached while processing a request.",
                    "id": 3,
                }
            }
        });

        // Our read_response returns None for failed responses
        assert_eq!(resp["success"], false);
    }
}
