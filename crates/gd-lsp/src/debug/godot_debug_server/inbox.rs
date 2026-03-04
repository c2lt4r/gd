use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::debug::variant::GodotVariant;

const INBOX_MAX: usize = 1000;

pub(super) struct Inbox {
    messages: Mutex<Vec<Vec<GodotVariant>>>,
    notify: Condvar,
}

impl Inbox {
    pub(super) fn new() -> Self {
        Self {
            messages: Mutex::new(Vec::new()),
            notify: Condvar::new(),
        }
    }

    pub(super) fn push(&self, msg: Vec<GodotVariant>) {
        let mut msgs = self.messages.lock().unwrap();
        if msgs.len() >= INBOX_MAX {
            msgs.remove(0);
        }
        msgs.push(msg);
        self.notify.notify_all();
    }

    /// Wait for a message whose first element is a String matching `prefix`.
    /// Removes and returns it. Returns None on timeout.
    pub(super) fn wait_for(&self, prefix: &str, timeout: Duration) -> Option<Vec<GodotVariant>> {
        self.wait_for_any(&[prefix], timeout)
    }

    /// Wait for a message matching any of the given prefixes.
    /// Removes and returns the first match. Returns None on timeout.
    pub(super) fn wait_for_any(
        &self,
        prefixes: &[&str],
        timeout: Duration,
    ) -> Option<Vec<GodotVariant>> {
        let deadline = Instant::now() + timeout;
        let mut msgs = self.messages.lock().unwrap();
        loop {
            if let Some(idx) = msgs
                .iter()
                .position(|m| prefixes.iter().any(|p| msg_matches(m, p)))
            {
                return Some(msgs.remove(idx));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            let (guard, result) = self.notify.wait_timeout(msgs, remaining).unwrap();
            msgs = guard;
            if result.timed_out() {
                if let Some(idx) = msgs
                    .iter()
                    .position(|m| prefixes.iter().any(|p| msg_matches(m, p)))
                {
                    return Some(msgs.remove(idx));
                }
                return None;
            }
        }
    }
}

pub(super) fn msg_matches(msg: &[GodotVariant], prefix: &str) -> bool {
    matches!(msg.first(), Some(GodotVariant::String(s)) if s == prefix)
}
