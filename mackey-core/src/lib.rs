//! Shared types and the keymap engine for mackey.
//!
//! This crate is the pure, heavily-tested core. At M5 it holds the device
//! classification used to decide which input devices the daemon grabs; the
//! keymap engine lands in M6.

use evdev::{AttributeSetRef, KeyCode};

mod keymap;
pub use keymap::{KeyEvent, KeymapEngine};

/// The mackey version string, sourced from the crate version at build time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The uinput character device the daemon writes synthetic events to. Access is
/// granted to the `mackey` group via the packaged udev rule (see M4).
pub const UINPUT_PATH: &str = "/dev/uinput";

/// The name of the daemon's virtual output keyboard. The daemon must never grab
/// this device back (that would be a feedback loop), so callers identify it by
/// this name.
pub const VIRTUAL_KEYBOARD_NAME: &str = "mackey virtual keyboard";

/// The system-bus name and object path the daemon's focus tracker owns.
pub const FOCUS_TRACKER_NAME: &str = "app.mackey.FocusTracker";
pub const FOCUS_TRACKER_PATH: &str = "/app/mackey/FocusTracker";

/// The payload of an `UpdateFocus` call: the focused window's desktop app id and
/// its title. Carried over D-Bus as `(ss)`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, zvariant::Type)]
pub struct FocusUpdate {
    pub app_id: String,
    pub window_title: String,
}

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
    fn focus_update_round_trips_over_dbus_encoding() {
        use zvariant::{serialized::Context, to_bytes, Endian, Type};
        // The D-Bus signature must be a struct of two strings.
        assert_eq!(FocusUpdate::SIGNATURE.to_string(), "(ss)");

        let original = FocusUpdate {
            app_id: "firefox.desktop".to_string(),
            window_title: "Mozilla Firefox".to_string(),
        };
        let ctxt = Context::new_dbus(Endian::Little, 0);
        let encoded = to_bytes(ctxt, &original).unwrap();
        let (decoded, _): (FocusUpdate, usize) = encoded.deserialize().unwrap();
        assert_eq!(original, decoded);
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
