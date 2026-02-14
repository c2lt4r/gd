#![allow(dead_code)]

mod commands;
pub(crate) mod inbox;
mod inspect;
mod live;
pub(crate) mod parsers;

#[cfg(test)]
mod tests;

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
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

pub struct GodotDebugServer {
    stream: Mutex<Option<TcpStream>>,
    listener: TcpListener,
    port: u16,
    inbox: Arc<Inbox>,
    /// Set to false when we want the reader thread to stop.
    running: Arc<Mutex<bool>>,
    /// Tracks whether the game is paused at a breakpoint (debug_enter/debug_exit).
    at_breakpoint: Arc<AtomicBool>,
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

    /// Check if connected.
    pub fn is_connected(&self) -> bool {
        self.stream.lock().unwrap().is_some()
    }

    /// Check if the game is currently paused at a breakpoint.
    /// This is tracked by the reader thread from debug_enter/debug_exit messages,
    /// so it returns instantly without any network round-trip.
    pub fn is_at_breakpoint(&self) -> bool {
        self.at_breakpoint.load(Ordering::Relaxed)
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

        std::thread::spawn(move || {
            reader_loop(stream, &inbox, &running, &at_breakpoint);
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

fn reader_loop(
    mut stream: TcpStream,
    inbox: &Inbox,
    running: &Mutex<bool>,
    at_breakpoint: &AtomicBool,
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

            // Track breakpoint state from debug_enter/debug_exit messages
            if let Some(GodotVariant::String(cmd)) = items.first() {
                match cmd.as_str() {
                    "debug_enter" => at_breakpoint.store(true, Ordering::Relaxed),
                    "debug_exit" => at_breakpoint.store(false, Ordering::Relaxed),
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
