//! Shared types and the keymap engine for mackey.
//!
//! This crate is the pure, heavily-tested core: built-in keymap tables, IPC
//! message types, and the table-driven keymap engine. It has no I/O and no
//! kernel dependencies so it can be unit-tested in isolation.

/// The mackey version string, sourced from the crate version at build time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The uinput character device the daemon writes synthetic events to. Access is
/// granted to the `mackey` group via the packaged udev rule (see M4).
pub const UINPUT_PATH: &str = "/dev/uinput";

/// What a single heartbeat-loop wait step observed.
#[derive(Debug, PartialEq, Eq)]
pub enum Wait {
    /// The heartbeat interval elapsed; the loop should emit a beat.
    Tick,
    /// Shutdown was requested; the loop should stop.
    Shutdown,
}

/// Drive the daemon's heartbeat loop: call `wait` repeatedly, emitting a beat on
/// each [`Wait::Tick`] and returning as soon as it observes [`Wait::Shutdown`].
///
/// All timing and signal handling lives in the caller's `wait` closure, which
/// keeps this loop free of timers and OS signals and therefore unit-testable: a
/// test supplies a `wait` returning a fixed sequence and asserts the beats.
pub fn heartbeat_loop<W, B>(mut wait: W, mut beat: B)
where
    W: FnMut() -> Wait,
    B: FnMut(),
{
    while let Wait::Tick = wait() {
        beat();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_populated() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn beats_each_tick_then_stops_on_shutdown() {
        let mut ticks_left = 3;
        let mut beats = 0;
        heartbeat_loop(
            || {
                if ticks_left > 0 {
                    ticks_left -= 1;
                    Wait::Tick
                } else {
                    Wait::Shutdown
                }
            },
            || beats += 1,
        );
        assert_eq!(beats, 3);
    }

    #[test]
    fn immediate_shutdown_emits_no_beats() {
        let mut beats = 0;
        heartbeat_loop(|| Wait::Shutdown, || beats += 1);
        assert_eq!(beats, 0);
    }
}
