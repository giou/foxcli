//! Ctrl+C (SIGINT) handling for cancellable operations.
//!
//! For cancellable commands (e.g. `backup`) the client installs a SIGINT
//! handler that records a cancellation request instead of terminating. The
//! progress UI's timer thread observes the request, forwards it to the engine
//! over the out-of-band cancel FIFO, and updates its hint. A second Ctrl+C
//! force-exits as an escape hatch in case the engine never tears down.

use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};

/// Engine-side out-of-band cancel FIFO (mirrors `FOX_CANCEL_FILE` in
/// `orscmd/orscmd.h`). The recovery GUI watches it even while a command runs.
pub const CANCEL_FIFO: &str = "/system/bin/foxcancel";

static REQUESTED: AtomicBool = AtomicBool::new(false);

extern "C" fn handle_sigint(_sig: libc::c_int) {
    // Async-signal-safe: only an atomic store, or _exit on the second press.
    if REQUESTED.swap(true, Ordering::SeqCst) {
        // A request is already pending and nothing has handled it yet -- the
        // user is insisting, so exit immediately rather than wait on the engine.
        unsafe { libc::_exit(130) };
    }
}

/// Install the SIGINT handler. Call once, before a cancellable command runs.
pub fn install() {
    unsafe {
        libc::signal(libc::SIGINT, handle_sigint as *const () as libc::sighandler_t);
    }
}

/// Whether the user has requested cancellation.
pub fn requested() -> bool {
    REQUESTED.load(Ordering::SeqCst)
}

/// Forward a cancellation request to the engine. Best-effort and non-blocking:
/// the FIFO write end is opened `O_NONBLOCK` so a missing reader fails fast
/// (ENXIO) rather than blocking the caller.
pub fn send() -> bool {
    let Ok(path) = CString::new(CANCEL_FIFO) else {
        return false;
    };
    unsafe {
        let fd = libc::open(path.as_ptr(), libc::O_WRONLY | libc::O_NONBLOCK);
        if fd < 0 {
            return false;
        }
        let byte = b"c";
        let _ = libc::write(fd, byte.as_ptr() as *const libc::c_void, 1);
        libc::close(fd);
    }
    true
}
