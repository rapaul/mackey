//! The mackey GUI: a GTK4 wizard/status window.
//!
//! M9 (in progress): the setup-state detection layer is in place and unit
//! tested. The GTK4 wizard window is built on top of it next, once the GTK
//! development libraries are available. For now the binary prints the detected
//! setup state so the layer is exercised end to end.

mod setup;

use setup::{detect, SystemRunner};

fn main() {
    let state = detect(&SystemRunner);
    let phase = if state.is_complete() {
        "set up"
    } else {
        "wizard"
    };
    println!("mackey GUI v{} — {phase} ({state:?})", mackey_core::VERSION);
}
