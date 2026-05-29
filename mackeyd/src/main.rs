//! The mackey daemon.
//!
//! At M4 the body is still a heartbeat loop, but it now opens /dev/uinput at
//! startup (granted by the packaged udev rule) and holds the handle. Later
//! milestones turn that handle into the virtual output keyboard.

use std::fs::{File, OpenOptions};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use mackey_core::{heartbeat_loop, Wait};

const HEARTBEAT: Duration = Duration::from_secs(30);

fn open_uinput() -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(mackey_core::UINPUT_PATH)
}

fn main() {
    // Running under systemd (Type=simple), stderr is captured by the journal.
    eprintln!("mackeyd v{} starting", mackey_core::VERSION);

    // Hold the uinput handle open for the daemon's lifetime. On failure we log
    // and keep running so the unit stays up for inspection.
    let _uinput = match open_uinput() {
        Ok(f) => {
            eprintln!("opened {}", mackey_core::UINPUT_PATH);
            Some(f)
        }
        Err(e) => {
            eprintln!("failed to open {}: {e}", mackey_core::UINPUT_PATH);
            None
        }
    };

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
