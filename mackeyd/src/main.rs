//! The mackey daemon.
//!
//! At M3 the body is still a heartbeat loop: it announces itself, emits a beat
//! every 30s, and shuts down cleanly on SIGTERM/SIGINT (so `systemctl stop`
//! returns promptly). Later milestones replace the beat with the real
//! evdev/uinput remapper.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use mackey_core::{heartbeat_loop, Wait};

const HEARTBEAT: Duration = Duration::from_secs(30);

fn main() {
    // Running under systemd (Type=simple), stderr is captured by the journal.
    eprintln!("mackeyd v{} starting", mackey_core::VERSION);

    // A background thread turns the first SIGTERM/SIGINT into a channel message,
    // which wakes the loop immediately instead of waiting out the interval.
    let (tx, rx) = mpsc::channel::<()>();
    let mut signals = signal_hook::iterator::Signals::new([
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGINT,
    ])
    .expect("register SIGTERM/SIGINT handler");
    thread::spawn(move || {
        if signals.forever().next().is_some() {
            let _ = tx.send(());
        }
    });

    heartbeat_loop(
        || match rx.recv_timeout(HEARTBEAT) {
            Err(RecvTimeoutError::Timeout) => Wait::Tick,
            // A signal arrived, or the sender vanished: either way, stop.
            Ok(()) | Err(RecvTimeoutError::Disconnected) => Wait::Shutdown,
        },
        || eprintln!("mackeyd heartbeat"),
    );

    eprintln!("mackeyd shutting down");
}
