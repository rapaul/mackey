//! The mackey daemon.
//!
//! At M0 this is a placeholder binary: it announces itself on stderr and exits
//! cleanly. Later milestones grow it into the evdev/uinput remapper.

fn main() {
    eprintln!("mackeyd v{} starting", mackey_core::VERSION);
}
