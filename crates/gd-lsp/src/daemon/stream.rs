//! Cross-platform Unix Domain Socket re-exports.
//!
//! On Unix we use the standard library types directly.
//! On Windows we use `uds_windows` which provides the same API
//! over the AF_UNIX support added in Windows 10 build 17063.

#[cfg(unix)]
pub use std::os::unix::net::{UnixListener, UnixStream};

#[cfg(windows)]
pub use uds_windows::{UnixListener, UnixStream};
