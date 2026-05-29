//! Shared types and the keymap engine for mackey.
//!
//! This crate is the pure, heavily-tested core. At M5 it holds the device
//! classification used to decide which input devices the daemon grabs; the
//! keymap engine lands in M6.

use evdev::{AttributeSetRef, KeyCode};

/// The mackey version string, sourced from the crate version at build time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The uinput character device the daemon writes synthetic events to. Access is
/// granted to the `mackey` group via the packaged udev rule (see M4).
pub const UINPUT_PATH: &str = "/dev/uinput";

/// The name of the daemon's virtual output keyboard. The daemon must never grab
/// this device back (that would be a feedback loop), so callers identify it by
/// this name.
pub const VIRTUAL_KEYBOARD_NAME: &str = "mackey virtual keyboard";

/// Whether a device's set of supported keys makes it a keyboard worth grabbing.
///
/// The heuristic requires the core alphabetic keys plus space, which a real
/// keyboard always has and which mice, consumer-control surfaces, power
/// buttons, and the like do not. Kept here (away from any I/O) so it can be
/// unit-tested with in-memory `AttributeSet`s, no kernel required.
pub fn is_keyboard(keys: &AttributeSetRef<KeyCode>) -> bool {
    keys.contains(KeyCode::KEY_A)
        && keys.contains(KeyCode::KEY_Z)
        && keys.contains(KeyCode::KEY_SPACE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use evdev::{AttributeSet, KeyCode};

    #[test]
    fn version_is_populated() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn full_keyboard_is_recognized() {
        let mut keys = AttributeSet::<KeyCode>::new();
        for k in [
            KeyCode::KEY_A,
            KeyCode::KEY_Z,
            KeyCode::KEY_SPACE,
            KeyCode::KEY_ENTER,
        ] {
            keys.insert(k);
        }
        assert!(is_keyboard(&keys));
    }

    #[test]
    fn non_keyboard_devices_are_rejected() {
        // A consumer-control / volume surface: has keys, but not the core set.
        let mut keys = AttributeSet::<KeyCode>::new();
        keys.insert(KeyCode::KEY_VOLUMEUP);
        keys.insert(KeyCode::KEY_VOLUMEDOWN);
        assert!(!is_keyboard(&keys));

        // Missing just one required key is still not a keyboard.
        let mut almost = AttributeSet::<KeyCode>::new();
        almost.insert(KeyCode::KEY_A);
        almost.insert(KeyCode::KEY_Z);
        assert!(!is_keyboard(&almost));
    }
}
