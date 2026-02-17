#![allow(dead_code)]

mod commands;
pub(crate) mod inbox;
mod inspect;
mod live;
pub(crate) mod parsers;

#[cfg(test)]
mod tests;

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::debug::variant::{GodotVariant, decode_packet, encode_packet};

use inbox::Inbox;

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct StackFrameInfo {
    pub file: String,
    pub line: u32,
    pub function: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FrameVariables {
    pub locals: Vec<DebugVariable>,
    pub members: Vec<DebugVariable>,
    pub globals: Vec<DebugVariable>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DebugVariable {
    pub name: String,
    pub value: GodotVariant,
    pub var_type: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvalResult {
    pub name: String,
    pub value: GodotVariant,
    pub var_type: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SceneTree {
    pub nodes: Vec<SceneNode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SceneNode {
    pub name: String,
    pub class_name: String,
    pub object_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene_file_path: Option<String>,
    pub children: Vec<SceneNode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScreenshotResult {
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ObjectInfo {
    pub object_id: u64,
    pub class_name: String,
    pub properties: Vec<ObjectProperty>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ObjectProperty {
    pub name: String,
    pub value: GodotVariant,
    pub type_id: u32,
    pub hint: u32,
    pub hint_string: String,
    pub usage: u32,
}

// ---------------------------------------------------------------------------
// GodotDebugServer
// ---------------------------------------------------------------------------

type OnDisconnectCallback = Arc<Mutex<Option<Box<dyn Fn() + Send>>>>;

/// A captured output line from Godot's `print()` / `push_error()` / `push_warning()`.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct CapturedOutput {
    pub message: String,
    /// `"log"`, `"error"`, `"warning"`, or `"log_rich"`
    pub r#type: String,
}

/// An output line in the log ring buffer with a monotonic sequence number.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct LogEntry {
    /// Monotonic sequence number (for `--follow` cursoring).
    pub seq: u64,
    pub message: String,
    pub r#type: String,
}

/// Maximum number of log entries kept in the ring buffer.
const LOG_RING_CAPACITY: usize = 2000;

pub struct GodotDebugServer {
    stream: Mutex<Option<TcpStream>>,
    listener: TcpListener,
    port: u16,
    inbox: Arc<Inbox>,
    /// Set to false when we want the reader thread to stop.
    running: Arc<Mutex<bool>>,
    /// Tracks whether the game is paused at a breakpoint (debug_enter/debug_exit).
    at_breakpoint: Arc<AtomicBool>,
    /// Set to true when the reader thread exits (game disconnected).
    disconnected: Arc<AtomicBool>,
    /// Callback invoked when the game disconnects (reader thread exits).
    on_disconnect: OnDisconnectCallback,
    /// When true, the reader loop buffers "output" messages into `output_buffer` for eval capture.
    capturing_output: Arc<AtomicBool>,
    /// Buffered output lines captured during an eval.
    output_buffer: Arc<Mutex<Vec<CapturedOutput>>>,
    /// Always-on log ring buffer. All output/error messages are stored here.
    log_ring: Arc<Mutex<VecDeque<LogEntry>>>,
    /// Monotonic sequence counter for log entries.
    log_seq: Arc<AtomicU64>,
}

impl GodotDebugServer {
    /// Default port for the gd binary debug protocol.
    /// Godot uses 6005 (LSP) and 6006 (debugger), so we use 6008.
    pub const DEFAULT_PORT: u16 = 6008;

    /// Create a new server listening on the given port on all interfaces.
    /// Binds to 0.0.0.0 so the port is reachable from Windows when running in WSL2.
    /// Pass 0 to let the OS assign a random port (useful for tests).
    pub fn new(port: u16) -> Option<Self> {
        let listener = TcpListener::bind(format!("0.0.0.0:{port}")).ok()?;
        let port = listener.local_addr().ok()?.port();
        Some(Self {
            stream: Mutex::new(None),
            listener,
            port,
            inbox: Arc::new(Inbox::new()),
            running: Arc::new(Mutex::new(true)),
            at_breakpoint: Arc::new(AtomicBool::new(false)),
            disconnected: Arc::new(AtomicBool::new(false)),
            on_disconnect: Arc::new(Mutex::new(None)),
            capturing_output: Arc::new(AtomicBool::new(false)),
            output_buffer: Arc::new(Mutex::new(Vec::new())),
            log_ring: Arc::new(Mutex::new(VecDeque::with_capacity(LOG_RING_CAPACITY))),
            log_seq: Arc::new(AtomicU64::new(1)),
        })
    }

    /// Get the port we're listening on.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Accept a connection from the game (blocking, with timeout).
    /// Returns true if a connection was accepted.
    /// Checks `running` flag so it can be interrupted by dropping the server.
    pub fn accept(&self, timeout: Duration) -> bool {
        let _ = self.listener.set_nonblocking(true);
        let deadline = Instant::now() + timeout;
        loop {
            // Check if we've been shut down (e.g. server replaced)
            if !*self.running.lock().unwrap() {
                return false;
            }
            match self.listener.accept() {
                Ok((tcp_stream, _addr)) => {
                    let _ = tcp_stream.set_nonblocking(false);
                    // Clone for the reader thread
                    let Ok(reader_stream) = tcp_stream.try_clone() else {
                        return false;
                    };
                    *self.stream.lock().unwrap() = Some(tcp_stream);
                    self.disconnected.store(false, Ordering::Release);
                    self.spawn_reader(reader_stream);
                    return true;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return false;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(_) => return false,
            }
        }
    }

    /// Check if connected. Returns false if the reader thread has exited
    /// (game disconnected) even if the stream object hasn't been cleaned up yet.
    pub fn is_connected(&self) -> bool {
        self.stream.lock().unwrap().is_some() && !self.disconnected.load(Ordering::Acquire)
    }

    /// Register a callback to be invoked when the game disconnects.
    pub fn set_on_disconnect(&self, cb: impl Fn() + Send + 'static) {
        *self.on_disconnect.lock().unwrap() = Some(Box::new(cb));
    }

    /// Check if the game is currently paused at a breakpoint.
    /// This is tracked by the reader thread from debug_enter/debug_exit messages,
    /// so it returns instantly without any network round-trip.
    pub fn is_at_breakpoint(&self) -> bool {
        self.at_breakpoint.load(Ordering::Relaxed)
    }

    /// Start capturing output messages from Godot (print, push_error, etc.).
    /// Clears any previously buffered output.
    pub fn start_output_capture(&self) {
        self.output_buffer.lock().unwrap().clear();
        self.capturing_output.store(true, Ordering::Release);
    }

    /// Drain all buffered output messages captured so far.
    /// Does NOT stop capturing — output continues to be buffered until the next
    /// `start_output_capture()` call (which clears and restarts).
    pub fn drain_output(&self) -> Vec<CapturedOutput> {
        std::mem::take(&mut *self.output_buffer.lock().unwrap())
    }

    /// Query the log ring buffer. Returns entries matching the filter criteria.
    /// - `after_seq`: only return entries with seq > this value (for follow/polling)
    /// - `count`: max entries to return (0 = all matching)
    /// - `type_filter`: only return entries of this type (None = all)
    pub fn query_log(
        &self,
        after_seq: u64,
        count: usize,
        type_filter: Option<&str>,
    ) -> Vec<LogEntry> {
        let ring = self.log_ring.lock().unwrap();
        let iter = ring.iter().filter(|e| {
            e.seq > after_seq
                && type_filter.is_none_or(|f| {
                    if f == "errors" {
                        e.r#type == "error" || e.r#type == "warning"
                    } else {
                        e.r#type == f
                    }
                })
        });
        if count > 0 {
            // Return the LAST `count` entries (most recent)
            let entries: Vec<&LogEntry> = iter.collect();
            let start = entries.len().saturating_sub(count);
            entries[start..].iter().map(|e| (*e).clone()).collect()
        } else {
            iter.cloned().collect()
        }
    }

    /// Clear the log ring buffer.
    pub fn clear_log(&self) {
        self.log_ring.lock().unwrap().clear();
    }

    /// Send a command to the game.
    /// Wire format: Array([String(command), Int(thread_id), Array([args...])])
    /// Godot 4.2+ requires three elements: command name, thread_id, and a
    /// data Array wrapping all parameters. This matches the editor's format
    /// (see godot-vscode-plugin server_controller.ts send_command).
    pub fn send_command(&self, command: &str, args: &[GodotVariant]) -> bool {
        let items = vec![
            GodotVariant::String(command.to_string()),
            GodotVariant::Int(1), // thread_id (main thread)
            GodotVariant::Array(args.to_vec()),
        ];
        let packet = encode_packet(&items);
        eprintln!(
            "debug_server: send {command} ({} bytes) args={args:?}",
            packet.len()
        );

        let mut guard = self.stream.lock().unwrap();
        if let Some(ref mut stream) = *guard {
            match stream.write_all(&packet) {
                Ok(()) => {
                    let _ = stream.flush();
                    true
                }
                Err(e) => {
                    eprintln!("debug_server: write failed: {e}");
                    false
                }
            }
        } else {
            eprintln!("debug_server: no connection");
            false
        }
    }

    /// Wait for a specific response message (by command prefix), with timeout.
    pub fn wait_message(&self, prefix: &str, timeout: Duration) -> Option<Vec<GodotVariant>> {
        self.inbox.wait_for(prefix, timeout)
    }

    /// Wait for any of several response messages, returning the first match.
    pub fn wait_message_any(
        &self,
        prefixes: &[&str],
        timeout: Duration,
    ) -> Option<Vec<GodotVariant>> {
        self.inbox.wait_for_any(prefixes, timeout)
    }

    // ── Internal ──

    fn spawn_reader(&self, stream: TcpStream) {
        let inbox = Arc::clone(&self.inbox);
        let running = Arc::clone(&self.running);
        let at_breakpoint = Arc::clone(&self.at_breakpoint);
        let disconnected = Arc::clone(&self.disconnected);
        let on_disconnect = Arc::clone(&self.on_disconnect);
        let capturing_output = Arc::clone(&self.capturing_output);
        let output_buffer = Arc::clone(&self.output_buffer);
        let log_ring = Arc::clone(&self.log_ring);
        let log_seq = Arc::clone(&self.log_seq);

        std::thread::spawn(move || {
            reader_loop(
                stream,
                &inbox,
                &running,
                &at_breakpoint,
                &capturing_output,
                &output_buffer,
                &log_ring,
                &log_seq,
            );
            // Reader exited — game disconnected
            disconnected.store(true, Ordering::Release);
            if let Some(cb) = on_disconnect.lock().unwrap().as_ref() {
                cb();
            }
        });
    }
}

impl Drop for GodotDebugServer {
    fn drop(&mut self) {
        *self.running.lock().unwrap() = false;
    }
}

// ---------------------------------------------------------------------------
// Reader thread
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn reader_loop(
    mut stream: TcpStream,
    inbox: &Inbox,
    running: &Mutex<bool>,
    at_breakpoint: &AtomicBool,
    capturing_output: &AtomicBool,
    output_buffer: &Mutex<Vec<CapturedOutput>>,
    log_ring: &Mutex<VecDeque<LogEntry>>,
    log_seq: &AtomicU64,
) {
    // Set a short read timeout so we can check the `running` flag periodically
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));

    loop {
        if !*running.lock().unwrap() {
            break;
        }

        // Read 4-byte length header
        let mut len_buf = [0u8; 4];
        match stream.read_exact(&mut len_buf) {
            Ok(()) => {}
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(_) => break, // Disconnected or other error
        }
        let payload_len = u32::from_le_bytes(len_buf) as usize;

        // Sanity check: reject absurdly large messages (> 64MB)
        if payload_len > 64 * 1024 * 1024 {
            break;
        }

        // Read payload
        let mut payload = vec![0u8; payload_len];
        if stream.read_exact(&mut payload).is_err() {
            break;
        }

        // Build full packet (length + payload) for decode_packet
        let mut full = Vec::with_capacity(4 + payload_len);
        full.extend_from_slice(&len_buf);
        full.extend_from_slice(&payload);

        if let Some(items) = decode_packet(&full) {
            // Normalize from wire format [cmd, thread_id, Array(data)]
            // to flat [cmd, data_items...] for downstream parsing
            let items = normalize_message(items);

            // Track breakpoint state and capture output/error messages
            if let Some(GodotVariant::String(cmd)) = items.first() {
                match cmd.as_str() {
                    "debug_enter" => at_breakpoint.store(true, Ordering::Relaxed),
                    "debug_exit" => at_breakpoint.store(false, Ordering::Relaxed),
                    "output" => {
                        // Always push to the log ring buffer
                        push_output_to_log(&items, log_ring, log_seq);
                        // Also push to eval capture buffer if active
                        if capturing_output.load(Ordering::Acquire) {
                            extract_output_messages(&items, output_buffer);
                        }
                    }
                    "error" => {
                        push_error_to_log(&items, log_ring, log_seq);
                        if capturing_output.load(Ordering::Acquire) {
                            extract_error_message(&items, output_buffer);
                        }
                    }
                    _ => {}
                }
            }

            inbox.push(items);
        } else {
            // Diagnostic: try to find where decoding fails
            let mut diag_offset = 4; // skip length header
            if let Some(header) = crate::debug::variant::decode_variant(&full, &mut diag_offset) {
                eprintln!(
                    "debug_server: packet decoded as {header} but expected Array ({payload_len} bytes)"
                );
            } else {
                // Show first bytes for diagnosis
                let preview: Vec<String> =
                    full.iter().take(64).map(|b| format!("{b:02x}")).collect();
                eprintln!(
                    "debug_server: failed to decode packet ({payload_len} bytes), first bytes: {}",
                    preview.join(" ")
                );
            }
        }
    }
}

/// Normalize a Godot 4.2+ message from wire format [String(cmd), Int(thread_id), Array(data)]
/// into a flat list [String(cmd), data_items...] for downstream parsing.
fn normalize_message(items: Vec<GodotVariant>) -> Vec<GodotVariant> {
    // Expected format: [String(cmd), Int(thread_id), Array([args...])]
    if items.len() >= 3
        && let Some(GodotVariant::String(_)) = items.first()
        && let Some(GodotVariant::Int(_)) = items.get(1)
        && let Some(GodotVariant::Array(_)) = items.get(2)
    {
        let mut result = Vec::new();
        result.push(items[0].clone()); // command name
        // Flatten the inner data array
        if let GodotVariant::Array(data) = &items[2] {
            result.extend_from_slice(data);
        }
        return result;
    }
    // Fallback: return as-is (older protocol or unknown format)
    items
}

/// Parse a normalized "output" message and append to the capture buffer.
/// Godot format (after normalization): `["output", PackedStringArray(strings), PackedInt32Array(types)]`
/// Types: 0 = LOG, 1 = ERROR, 2 = LOG_RICH
fn extract_output_messages(items: &[GodotVariant], buffer: &Mutex<Vec<CapturedOutput>>) {
    // Godot sends PackedStringArray for the message strings
    let strings: &[String] = match items.get(1) {
        Some(GodotVariant::PackedStringArray(v)) => v,
        Some(GodotVariant::Array(v)) => {
            // Fallback: extract String variants from Array
            let mut buf = buffer.lock().unwrap();
            for (i, s) in v.iter().enumerate() {
                let GodotVariant::String(msg) = s else {
                    continue;
                };
                let type_name = type_from_array(items.get(2), i);
                buf.push(CapturedOutput {
                    message: msg.clone(),
                    r#type: type_name.to_string(),
                });
            }
            return;
        }
        _ => return,
    };

    // Godot sends PackedInt32Array for the message types
    let types: Option<&[i32]> = match items.get(2) {
        Some(GodotVariant::PackedInt32Array(v)) => Some(v),
        _ => None,
    };

    let mut buf = buffer.lock().unwrap();
    for (i, msg) in strings.iter().enumerate() {
        let type_name = types
            .and_then(|t| t.get(i))
            .map_or("log", |&v| match v {
                1 => "error",
                2 => "log_rich",
                _ => "log",
            });
        // Strip trailing null bytes that Godot sometimes appends
        let msg = msg.trim_end_matches('\0');
        if msg.is_empty() {
            continue;
        }
        buf.push(CapturedOutput {
            message: msg.to_string(),
            r#type: type_name.to_string(),
        });
    }
}

/// Helper: get type name from an Array variant (fallback path).
fn type_from_array(item: Option<&GodotVariant>, idx: usize) -> &'static str {
    match item {
        Some(GodotVariant::Array(t)) => t
            .get(idx)
            .and_then(|v| match v {
                GodotVariant::Int(0) => Some("log"),
                GodotVariant::Int(1) => Some("error"),
                GodotVariant::Int(2) => Some("log_rich"),
                _ => None,
            })
            .unwrap_or("log"),
        _ => "log",
    }
}

/// Parse a normalized "error" message and append to the capture buffer.
/// Godot wire format (after normalization):
///   `["error", Int(hr), Int(has_stack), Int(thread), Int(id),
///     String(file), String(func), Int(line),
///     String(rationale), String(error_descr), Bool(warning), ...]`
/// We capture `rationale` (index 8) or `error_descr` (index 9).
fn extract_error_message(items: &[GodotVariant], buffer: &Mutex<Vec<CapturedOutput>>) {
    // Try rationale first (e.g. "something bad"), then error_descr
    let msg = items
        .get(8)
        .and_then(|v| match v {
            GodotVariant::String(s) if !s.is_empty() => Some(s.as_str()),
            _ => None,
        })
        .or_else(|| {
            items.get(9).and_then(|v| match v {
                GodotVariant::String(s) if !s.is_empty() => Some(s.as_str()),
                _ => None,
            })
        });

    if let Some(msg) = msg {
        let is_warning = matches!(items.get(10), Some(GodotVariant::Bool(true)));
        buffer.lock().unwrap().push(CapturedOutput {
            message: msg.to_string(),
            r#type: if is_warning { "warning" } else { "error" }.to_string(),
        });
    }
}

// ---------------------------------------------------------------------------
// Log ring buffer helpers
// ---------------------------------------------------------------------------

/// Push output messages to the always-on log ring buffer.
fn push_output_to_log(
    items: &[GodotVariant],
    ring: &Mutex<VecDeque<LogEntry>>,
    seq: &AtomicU64,
) {
    let strings: &[String] = match items.get(1) {
        Some(GodotVariant::PackedStringArray(v)) => v,
        _ => return,
    };
    let types: Option<&[i32]> = match items.get(2) {
        Some(GodotVariant::PackedInt32Array(v)) => Some(v),
        _ => None,
    };

    let mut ring = ring.lock().unwrap();
    for (i, msg) in strings.iter().enumerate() {
        let msg = msg.trim_end_matches('\0');
        if msg.is_empty() {
            continue;
        }
        let type_name = types
            .and_then(|t| t.get(i))
            .map_or("log", |&v| match v {
                1 => "error",
                2 => "log_rich",
                _ => "log",
            });
        let id = seq.fetch_add(1, Ordering::Relaxed);
        if ring.len() >= LOG_RING_CAPACITY {
            ring.pop_front();
        }
        ring.push_back(LogEntry {
            seq: id,
            message: msg.to_string(),
            r#type: type_name.to_string(),
        });
    }
}

/// Push an error/warning message to the always-on log ring buffer.
fn push_error_to_log(
    items: &[GodotVariant],
    ring: &Mutex<VecDeque<LogEntry>>,
    seq: &AtomicU64,
) {
    let msg = items
        .get(8)
        .and_then(|v| match v {
            GodotVariant::String(s) if !s.is_empty() => Some(s.as_str()),
            _ => None,
        })
        .or_else(|| {
            items.get(9).and_then(|v| match v {
                GodotVariant::String(s) if !s.is_empty() => Some(s.as_str()),
                _ => None,
            })
        });

    if let Some(msg) = msg {
        let is_warning = matches!(items.get(10), Some(GodotVariant::Bool(true)));
        let id = seq.fetch_add(1, Ordering::Relaxed);
        let mut ring = ring.lock().unwrap();
        if ring.len() >= LOG_RING_CAPACITY {
            ring.pop_front();
        }
        ring.push_back(LogEntry {
            seq: id,
            message: msg.to_string(),
            r#type: if is_warning { "warning" } else { "error" }.to_string(),
        });
    }
}
