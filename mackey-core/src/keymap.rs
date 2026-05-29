//! The built-in keymap engine.
//!
//! M6 ships only the **global fallback** keymap: while a Super (Meta) key is
//! held, the macOS-style shortcuts Super+{C,V,X,A,Z,S,F,N,O,W,Q,T} are rewritten
//! to Ctrl+{same}. Everything else is passed through untouched, including:
//!
//! - any key with Super *not* held,
//! - a lone Super tap (emitted on release so GNOME's Activities still opens),
//! - Super + any *unmapped* key (the real Super is emitted so e.g. Super+L still
//!   reaches the desktop).
//!
//! The engine is a pure state machine over `(code, value)` key events so it can
//! be exhaustively unit-tested without a kernel. The daemon feeds it EV_KEY
//! events and emits whatever it returns.

use evdev::KeyCode;

const LEFTMETA: u16 = KeyCode::KEY_LEFTMETA.code();
const RIGHTMETA: u16 = KeyCode::KEY_RIGHTMETA.code();
const LEFTCTRL: u16 = KeyCode::KEY_LEFTCTRL.code();

/// Keys that become Ctrl+<key> while Super is held (the global fallback keymap).
const MAPPED: [u16; 12] = [
    KeyCode::KEY_C.code(),
    KeyCode::KEY_V.code(),
    KeyCode::KEY_X.code(),
    KeyCode::KEY_A.code(),
    KeyCode::KEY_Z.code(),
    KeyCode::KEY_S.code(),
    KeyCode::KEY_F.code(),
    KeyCode::KEY_N.code(),
    KeyCode::KEY_O.code(),
    KeyCode::KEY_W.code(),
    KeyCode::KEY_Q.code(),
    KeyCode::KEY_T.code(),
];

// evdev key event values.
const RELEASE: i32 = 0;
const PRESS: i32 = 1;

fn is_meta(code: u16) -> bool {
    code == LEFTMETA || code == RIGHTMETA
}

fn is_mapped(code: u16) -> bool {
    MAPPED.contains(&code)
}

/// A single key event: an EV_KEY `code` with a `value` (0=release, 1=press,
/// 2=autorepeat).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub code: u16,
    pub value: i32,
}

impl KeyEvent {
    fn new(code: u16, value: i32) -> Self {
        Self { code, value }
    }
}

/// The keymap state machine. One instance drives all grabbed keyboards.
#[derive(Debug)]
pub struct KeymapEngine {
    super_down: bool,
    /// Which Meta keycode is currently held (to release the right one).
    meta_code: u16,
    /// We injected a synthetic Ctrl for a mapped combo and owe a Ctrl release.
    emitted_ctrl: bool,
    /// We passed the real Super through for an unmapped combo and owe its release.
    emitted_super: bool,
}

impl Default for KeymapEngine {
    fn default() -> Self {
        Self {
            super_down: false,
            meta_code: LEFTMETA,
            emitted_ctrl: false,
            emitted_super: false,
        }
    }
}

impl KeymapEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Translate one EV_KEY event into the events to emit.
    pub fn process(&mut self, code: u16, value: i32) -> Vec<KeyEvent> {
        if is_meta(code) {
            return self.process_meta(code, value);
        }
        if !self.super_down {
            return vec![KeyEvent::new(code, value)];
        }
        if is_mapped(code) {
            // Super+mapped -> Ctrl+mapped. Press Ctrl once, lazily.
            let mut out = Vec::new();
            if value == PRESS && !self.emitted_ctrl {
                out.push(KeyEvent::new(LEFTCTRL, PRESS));
                self.emitted_ctrl = true;
            }
            out.push(KeyEvent::new(code, value));
            out
        } else {
            // Super+unmapped -> pass the real Super through so the combo works.
            let mut out = Vec::new();
            if value == PRESS && !self.emitted_super {
                out.push(KeyEvent::new(self.meta_code, PRESS));
                self.emitted_super = true;
            }
            out.push(KeyEvent::new(code, value));
            out
        }
    }

    fn process_meta(&mut self, code: u16, value: i32) -> Vec<KeyEvent> {
        match value {
            PRESS => {
                // Defer the Super press; we decide Ctrl vs Super once a key follows.
                self.super_down = true;
                self.meta_code = code;
                self.emitted_ctrl = false;
                self.emitted_super = false;
                Vec::new()
            }
            RELEASE => {
                let mut out = Vec::new();
                if self.emitted_ctrl {
                    out.push(KeyEvent::new(LEFTCTRL, RELEASE));
                } else if self.emitted_super {
                    out.push(KeyEvent::new(self.meta_code, RELEASE));
                } else {
                    // Nothing followed: emit the deferred tap so Super-alone works.
                    out.push(KeyEvent::new(self.meta_code, PRESS));
                    out.push(KeyEvent::new(self.meta_code, RELEASE));
                }
                self.super_down = false;
                self.emitted_ctrl = false;
                self.emitted_super = false;
                out
            }
            // Ignore Meta autorepeat while held.
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive(engine: &mut KeymapEngine, events: &[(u16, i32)]) -> Vec<KeyEvent> {
        events
            .iter()
            .flat_map(|&(c, v)| engine.process(c, v))
            .collect()
    }

    const KEY_C: u16 = KeyCode::KEY_C.code();
    const KEY_L: u16 = KeyCode::KEY_L.code(); // not in the mapped set

    #[test]
    fn super_plus_mapped_key_becomes_ctrl() {
        let mut e = KeymapEngine::new();
        let out = drive(
            &mut e,
            &[(LEFTMETA, 1), (KEY_C, 1), (KEY_C, 0), (LEFTMETA, 0)],
        );
        assert_eq!(
            out,
            vec![
                KeyEvent::new(LEFTCTRL, 1),
                KeyEvent::new(KEY_C, 1),
                KeyEvent::new(KEY_C, 0),
                KeyEvent::new(LEFTCTRL, 0),
            ]
        );
    }

    #[test]
    fn every_mapped_key_is_rewritten_to_ctrl() {
        for &k in &MAPPED {
            let mut e = KeymapEngine::new();
            let out = drive(&mut e, &[(LEFTMETA, 1), (k, 1), (k, 0), (LEFTMETA, 0)]);
            assert_eq!(
                out,
                vec![
                    KeyEvent::new(LEFTCTRL, 1),
                    KeyEvent::new(k, 1),
                    KeyEvent::new(k, 0),
                    KeyEvent::new(LEFTCTRL, 0),
                ],
                "mapping for key code {k}"
            );
        }
    }

    #[test]
    fn lone_super_tap_passes_through_on_release() {
        let mut e = KeymapEngine::new();
        let out = drive(&mut e, &[(LEFTMETA, 1), (LEFTMETA, 0)]);
        assert_eq!(
            out,
            vec![KeyEvent::new(LEFTMETA, 1), KeyEvent::new(LEFTMETA, 0)]
        );
    }

    #[test]
    fn super_plus_unmapped_key_keeps_super() {
        let mut e = KeymapEngine::new();
        let out = drive(
            &mut e,
            &[(LEFTMETA, 1), (KEY_L, 1), (KEY_L, 0), (LEFTMETA, 0)],
        );
        assert_eq!(
            out,
            vec![
                KeyEvent::new(LEFTMETA, 1),
                KeyEvent::new(KEY_L, 1),
                KeyEvent::new(KEY_L, 0),
                KeyEvent::new(LEFTMETA, 0),
            ]
        );
    }

    #[test]
    fn right_super_is_also_recognized() {
        let mut e = KeymapEngine::new();
        let out = drive(
            &mut e,
            &[(RIGHTMETA, 1), (KEY_C, 1), (KEY_C, 0), (RIGHTMETA, 0)],
        );
        assert_eq!(out.first(), Some(&KeyEvent::new(LEFTCTRL, 1)));
        assert_eq!(out.last(), Some(&KeyEvent::new(LEFTCTRL, 0)));
    }

    /// Property: with Super never held, every key event is passed through
    /// unchanged, for all codes and all event values.
    #[test]
    fn no_super_is_identity_passthrough() {
        let mut e = KeymapEngine::new();
        for code in 1u16..256 {
            if is_meta(code) {
                continue;
            }
            for value in [1, 2, 0] {
                let out = e.process(code, value);
                assert_eq!(out, vec![KeyEvent::new(code, value)]);
            }
        }
    }
}
