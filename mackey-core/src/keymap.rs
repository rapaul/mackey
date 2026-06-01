//! The built-in keymap engine.
//!
//! While a Super (Meta) key is held, macOS-style shortcuts are rewritten to
//! their Linux equivalents. The exact rewrite depends on the **active keymap**,
//! which the daemon selects from the focused app (M8):
//!
//! - **Global fallback** — Super+{C,V,X,A,Z,S,F,N,O,W,Q,T} -> Ctrl+{same}.
//! - **Files** (`org.gnome.Nautilus.desktop`) — global, plus Super+Up -> Alt+Up.
//! - **Firefox** (`firefox.desktop`) — global, plus Super+Left/Right ->
//!   Alt+Left/Right (back/forward).
//! - **Ghostty** (`com.mitchellh.ghostty.desktop`) and **GNOME Terminal**
//!   (`org.gnome.Terminal.desktop`) — terminal convention: Super+{C,V,X,T,W,N}
//!   -> Ctrl+Shift+{same}; the rest map to Ctrl+{same}.
//!
//! In every keymap, anything not in the active table passes through untouched:
//! a key with Super *not* held, a lone Super tap (emitted on release so GNOME's
//! Activities still opens), and Super + any *unmapped* key (the real Super is
//! emitted so e.g. Super+L still reaches the desktop).
//!
//! The engine is a pure state machine over `(code, value)` key events plus an
//! active [`Keymap`], so it can be exhaustively unit-tested without a kernel.

use evdev::KeyCode;

const LEFTMETA: u16 = KeyCode::KEY_LEFTMETA.code();
const RIGHTMETA: u16 = KeyCode::KEY_RIGHTMETA.code();
const LEFTCTRL: u16 = KeyCode::KEY_LEFTCTRL.code();
const LEFTSHIFT: u16 = KeyCode::KEY_LEFTSHIFT.code();
const LEFTALT: u16 = KeyCode::KEY_LEFTALT.code();

// Input key codes used by the built-in tables.
const KEY_C: u16 = KeyCode::KEY_C.code();
const KEY_V: u16 = KeyCode::KEY_V.code();
const KEY_X: u16 = KeyCode::KEY_X.code();
const KEY_A: u16 = KeyCode::KEY_A.code();
const KEY_Z: u16 = KeyCode::KEY_Z.code();
const KEY_S: u16 = KeyCode::KEY_S.code();
const KEY_F: u16 = KeyCode::KEY_F.code();
const KEY_N: u16 = KeyCode::KEY_N.code();
const KEY_O: u16 = KeyCode::KEY_O.code();
const KEY_W: u16 = KeyCode::KEY_W.code();
const KEY_Q: u16 = KeyCode::KEY_Q.code();
const KEY_T: u16 = KeyCode::KEY_T.code();
const KEY_LEFT: u16 = KeyCode::KEY_LEFT.code();
const KEY_RIGHT: u16 = KeyCode::KEY_RIGHT.code();
const KEY_UP: u16 = KeyCode::KEY_UP.code();

// Modifier sets a binding can request, held while the mapped key is emitted.
const CTRL: &[u16] = &[LEFTCTRL];
const CTRL_SHIFT: &[u16] = &[LEFTCTRL, LEFTSHIFT];
const ALT: &[u16] = &[LEFTALT];

// evdev key event values.
const RELEASE: i32 = 0;
const PRESS: i32 = 1;

/// A built-in keymap: while Super is held, each listed input key is emitted with
/// the given modifier set instead of Super. Keys absent from `entries` pass the
/// real Super through.
pub struct Keymap {
    /// Stable identifier, logged on switch (`"global"` or a `.desktop` id).
    pub id: &'static str,
    entries: &'static [(u16, &'static [u16])],
}

impl Keymap {
    fn mods_for(&self, code: u16) -> Option<&'static [u16]> {
        self.entries
            .iter()
            .find_map(|&(c, mods)| (c == code).then_some(mods))
    }
}

static GLOBAL: Keymap = Keymap {
    id: "global",
    entries: &[
        (KEY_C, CTRL),
        (KEY_V, CTRL),
        (KEY_X, CTRL),
        (KEY_A, CTRL),
        (KEY_Z, CTRL),
        (KEY_S, CTRL),
        (KEY_F, CTRL),
        (KEY_N, CTRL),
        (KEY_O, CTRL),
        (KEY_W, CTRL),
        (KEY_Q, CTRL),
        (KEY_T, CTRL),
    ],
};

static FILES: Keymap = Keymap {
    id: "org.gnome.Nautilus.desktop",
    entries: &[
        (KEY_C, CTRL),
        (KEY_V, CTRL),
        (KEY_X, CTRL),
        (KEY_A, CTRL),
        (KEY_Z, CTRL),
        (KEY_S, CTRL),
        (KEY_F, CTRL),
        (KEY_N, CTRL),
        (KEY_O, CTRL),
        (KEY_W, CTRL),
        (KEY_Q, CTRL),
        (KEY_T, CTRL),
        // Files-specific: go to the parent directory.
        (KEY_UP, ALT),
    ],
};

static FIREFOX: Keymap = Keymap {
    id: "firefox.desktop",
    entries: &[
        (KEY_C, CTRL),
        (KEY_V, CTRL),
        (KEY_X, CTRL),
        (KEY_A, CTRL),
        (KEY_Z, CTRL),
        (KEY_S, CTRL),
        (KEY_F, CTRL),
        (KEY_N, CTRL),
        (KEY_O, CTRL),
        (KEY_W, CTRL),
        (KEY_Q, CTRL),
        (KEY_T, CTRL),
        // Firefox-specific: history back / forward.
        (KEY_LEFT, ALT),
        (KEY_RIGHT, ALT),
    ],
};

// Shared by every terminal keymap. Terminal copy/paste/cut and tab/window/split
// need Shift, since plain Ctrl+C is SIGINT in a terminal; the remaining global
// letters keep plain Ctrl.
const TERMINAL_ENTRIES: &[(u16, &[u16])] = &[
    (KEY_C, CTRL_SHIFT),
    (KEY_V, CTRL_SHIFT),
    (KEY_X, CTRL_SHIFT),
    (KEY_T, CTRL_SHIFT),
    (KEY_W, CTRL_SHIFT),
    (KEY_N, CTRL_SHIFT),
    (KEY_A, CTRL),
    (KEY_Z, CTRL),
    (KEY_S, CTRL),
    (KEY_F, CTRL),
    (KEY_O, CTRL),
    (KEY_Q, CTRL),
];

static GHOSTTY: Keymap = Keymap {
    id: "com.mitchellh.ghostty.desktop",
    entries: TERMINAL_ENTRIES,
};

static GNOME_TERMINAL: Keymap = Keymap {
    id: "org.gnome.Terminal.desktop",
    entries: TERMINAL_ENTRIES,
};

/// Resolve a focused app id to its built-in keymap, falling back to global.
fn keymap_for(app_id: Option<&str>) -> &'static Keymap {
    match app_id {
        Some("org.gnome.Nautilus.desktop") => &FILES,
        Some("firefox.desktop") => &FIREFOX,
        Some("com.mitchellh.ghostty.desktop") => &GHOSTTY,
        Some("org.gnome.Terminal.desktop") => &GNOME_TERMINAL,
        _ => &GLOBAL,
    }
}

fn is_meta(code: u16) -> bool {
    code == LEFTMETA || code == RIGHTMETA
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

/// The keymap state machine. One instance drives all grabbed keyboards; the
/// active [`Keymap`] is supplied per event so the daemon can switch context.
#[derive(Debug)]
pub struct KeymapEngine {
    super_down: bool,
    /// Which Meta keycode is currently held (to release the right one).
    meta_code: u16,
    /// Synthetic modifiers we've pressed for the current Super-hold, in press
    /// order; released (in reverse) when Super is released.
    held_mods: Vec<u16>,
    /// We passed the real Super through for an unmapped combo and owe its release.
    emitted_super: bool,
}

impl Default for KeymapEngine {
    fn default() -> Self {
        Self {
            super_down: false,
            meta_code: LEFTMETA,
            held_mods: Vec::new(),
            emitted_super: false,
        }
    }
}

impl KeymapEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Translate one EV_KEY event into the events to emit, under `keymap`.
    pub fn process(&mut self, code: u16, value: i32, keymap: &Keymap) -> Vec<KeyEvent> {
        if is_meta(code) {
            return self.process_meta(code, value);
        }
        if !self.super_down {
            return vec![KeyEvent::new(code, value)];
        }
        match keymap.mods_for(code) {
            Some(mods) => self.emit_mapped(code, value, mods),
            None => self.emit_unmapped(code, value),
        }
    }

    /// Super+mapped -> <mods>+key. Reconcile the held modifier set to `mods` on
    /// press (so a second key wanting different mods is correct), then the key.
    fn emit_mapped(&mut self, code: u16, value: i32, mods: &[u16]) -> Vec<KeyEvent> {
        let mut out = Vec::new();
        if value == PRESS {
            let mut i = 0;
            while i < self.held_mods.len() {
                let held = self.held_mods[i];
                if mods.contains(&held) {
                    i += 1;
                } else {
                    out.push(KeyEvent::new(held, RELEASE));
                    self.held_mods.remove(i);
                }
            }
            for &m in mods {
                if !self.held_mods.contains(&m) {
                    out.push(KeyEvent::new(m, PRESS));
                    self.held_mods.push(m);
                }
            }
        }
        out.push(KeyEvent::new(code, value));
        out
    }

    /// Super+unmapped -> pass the real Super through so the combo still works.
    fn emit_unmapped(&mut self, code: u16, value: i32) -> Vec<KeyEvent> {
        let mut out = Vec::new();
        if value == PRESS && !self.emitted_super {
            out.push(KeyEvent::new(self.meta_code, PRESS));
            self.emitted_super = true;
        }
        out.push(KeyEvent::new(code, value));
        out
    }

    fn process_meta(&mut self, code: u16, value: i32) -> Vec<KeyEvent> {
        match value {
            PRESS => {
                // Defer the Super press; we decide what to emit once a key follows.
                self.super_down = true;
                self.meta_code = code;
                self.held_mods.clear();
                self.emitted_super = false;
                Vec::new()
            }
            RELEASE => {
                let mut out = Vec::new();
                if !self.held_mods.is_empty() {
                    for &m in self.held_mods.iter().rev() {
                        out.push(KeyEvent::new(m, RELEASE));
                    }
                    self.held_mods.clear();
                } else if self.emitted_super {
                    out.push(KeyEvent::new(self.meta_code, RELEASE));
                } else {
                    // Nothing followed: emit the deferred tap so Super-alone works.
                    out.push(KeyEvent::new(self.meta_code, PRESS));
                    out.push(KeyEvent::new(self.meta_code, RELEASE));
                }
                self.super_down = false;
                self.emitted_super = false;
                out
            }
            // Ignore Meta autorepeat while held.
            _ => Vec::new(),
        }
    }
}

/// How long an `UpdateFocus` stays authoritative. After this, the engine falls
/// back to the global keymap (the focus signal is assumed lost).
pub const STALE_MS: u64 = 5000;

/// Tracks the focused app and when it was last reported, and selects the active
/// keymap from it. Pure over an injected monotonic clock (`now_ms`) so the
/// stale-fallback timing is unit-testable.
#[derive(Debug, Default)]
pub struct FocusState {
    app_id: Option<String>,
    updated_at_ms: u64,
}

impl FocusState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a fresh focus report.
    pub fn update(&mut self, app_id: String, now_ms: u64) {
        self.app_id = Some(app_id);
        self.updated_at_ms = now_ms;
    }

    /// The app id last reported (regardless of staleness).
    pub fn app_id(&self) -> Option<&str> {
        self.app_id.as_deref()
    }

    /// Whether the last report is older than the stale window.
    pub fn is_stale(&self, now_ms: u64) -> bool {
        self.app_id.is_some() && now_ms.saturating_sub(self.updated_at_ms) > STALE_MS
    }

    /// The keymap active at `now_ms`: the focused app's keymap, or the global
    /// fallback if nothing has been reported or the last report is stale.
    pub fn active_keymap(&self, now_ms: u64) -> &'static Keymap {
        if self.is_stale(now_ms) {
            &GLOBAL
        } else {
            keymap_for(self.app_id.as_deref())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive(engine: &mut KeymapEngine, keymap: &Keymap, events: &[(u16, i32)]) -> Vec<KeyEvent> {
        events
            .iter()
            .flat_map(|&(c, v)| engine.process(c, v, keymap))
            .collect()
    }

    const KEY_L: u16 = KeyCode::KEY_L.code(); // not in any mapped set

    #[test]
    fn super_plus_mapped_key_becomes_ctrl() {
        let mut e = KeymapEngine::new();
        let out = drive(
            &mut e,
            &GLOBAL,
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
    fn every_global_letter_is_rewritten_to_ctrl() {
        for &(k, _) in GLOBAL.entries {
            let mut e = KeymapEngine::new();
            let out = drive(
                &mut e,
                &GLOBAL,
                &[(LEFTMETA, 1), (k, 1), (k, 0), (LEFTMETA, 0)],
            );
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
        let out = drive(&mut e, &GLOBAL, &[(LEFTMETA, 1), (LEFTMETA, 0)]);
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
            &GLOBAL,
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
            &GLOBAL,
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
                let out = e.process(code, value, &GLOBAL);
                assert_eq!(out, vec![KeyEvent::new(code, value)]);
            }
        }
    }

    #[test]
    fn ghostty_copy_gets_ctrl_shift() {
        let mut e = KeymapEngine::new();
        let out = drive(
            &mut e,
            &GHOSTTY,
            &[(LEFTMETA, 1), (KEY_C, 1), (KEY_C, 0), (LEFTMETA, 0)],
        );
        assert_eq!(
            out,
            vec![
                KeyEvent::new(LEFTCTRL, 1),
                KeyEvent::new(LEFTSHIFT, 1),
                KeyEvent::new(KEY_C, 1),
                KeyEvent::new(KEY_C, 0),
                KeyEvent::new(LEFTSHIFT, 0),
                KeyEvent::new(LEFTCTRL, 0),
            ]
        );
    }

    #[test]
    fn ghostty_non_shifted_letter_stays_plain_ctrl() {
        let mut e = KeymapEngine::new();
        let out = drive(
            &mut e,
            &GHOSTTY,
            &[(LEFTMETA, 1), (KEY_A, 1), (KEY_A, 0), (LEFTMETA, 0)],
        );
        assert_eq!(
            out,
            vec![
                KeyEvent::new(LEFTCTRL, 1),
                KeyEvent::new(KEY_A, 1),
                KeyEvent::new(KEY_A, 0),
                KeyEvent::new(LEFTCTRL, 0),
            ]
        );
    }

    #[test]
    fn gnome_terminal_copy_gets_ctrl_shift() {
        let mut e = KeymapEngine::new();
        let out = drive(
            &mut e,
            &GNOME_TERMINAL,
            &[(LEFTMETA, 1), (KEY_C, 1), (KEY_C, 0), (LEFTMETA, 0)],
        );
        assert_eq!(
            out,
            vec![
                KeyEvent::new(LEFTCTRL, 1),
                KeyEvent::new(LEFTSHIFT, 1),
                KeyEvent::new(KEY_C, 1),
                KeyEvent::new(KEY_C, 0),
                KeyEvent::new(LEFTSHIFT, 0),
                KeyEvent::new(LEFTCTRL, 0),
            ]
        );
    }

    /// Both terminals share the same convention table.
    #[test]
    fn gnome_terminal_matches_ghostty_convention() {
        assert_eq!(GNOME_TERMINAL.entries, GHOSTTY.entries);
    }

    #[test]
    fn firefox_arrows_get_alt() {
        let mut e = KeymapEngine::new();
        let out = drive(
            &mut e,
            &FIREFOX,
            &[(LEFTMETA, 1), (KEY_LEFT, 1), (KEY_LEFT, 0), (LEFTMETA, 0)],
        );
        assert_eq!(
            out,
            vec![
                KeyEvent::new(LEFTALT, 1),
                KeyEvent::new(KEY_LEFT, 1),
                KeyEvent::new(KEY_LEFT, 0),
                KeyEvent::new(LEFTALT, 0),
            ]
        );
    }

    /// Super+T differs by app: Ctrl+Shift+T in Ghostty, plain Ctrl+T in Firefox.
    #[test]
    fn same_key_differs_by_active_keymap() {
        let mut g = KeymapEngine::new();
        let ghostty = drive(
            &mut g,
            &GHOSTTY,
            &[(LEFTMETA, 1), (KEY_T, 1), (KEY_T, 0), (LEFTMETA, 0)],
        );
        let mut f = KeymapEngine::new();
        let firefox = drive(
            &mut f,
            &FIREFOX,
            &[(LEFTMETA, 1), (KEY_T, 1), (KEY_T, 0), (LEFTMETA, 0)],
        );
        assert!(ghostty.contains(&KeyEvent::new(LEFTSHIFT, 1)));
        assert!(!firefox.contains(&KeyEvent::new(LEFTSHIFT, 1)));
    }

    #[test]
    fn keymap_for_resolves_known_apps_else_global() {
        assert_eq!(keymap_for(Some("firefox.desktop")).id, "firefox.desktop");
        assert_eq!(
            keymap_for(Some("org.gnome.Nautilus.desktop")).id,
            "org.gnome.Nautilus.desktop"
        );
        assert_eq!(
            keymap_for(Some("com.mitchellh.ghostty.desktop")).id,
            "com.mitchellh.ghostty.desktop"
        );
        assert_eq!(
            keymap_for(Some("org.gnome.Terminal.desktop")).id,
            "org.gnome.Terminal.desktop"
        );
        assert_eq!(keymap_for(Some("unknown.desktop")).id, "global");
        assert_eq!(keymap_for(None).id, "global");
    }

    #[test]
    fn focus_state_selects_app_keymap_then_falls_back_when_stale() {
        let mut s = FocusState::new();
        // Nothing reported yet -> global.
        assert_eq!(s.active_keymap(0).id, "global");

        // Report Firefox at t=1000 -> Firefox keymap holds within the window.
        s.update("firefox.desktop".to_string(), 1_000);
        assert_eq!(s.active_keymap(1_000).id, "firefox.desktop");
        assert_eq!(s.active_keymap(1_000 + STALE_MS).id, "firefox.desktop");
        assert!(!s.is_stale(1_000 + STALE_MS));

        // One ms past the window -> stale -> global.
        assert!(s.is_stale(1_000 + STALE_MS + 1));
        assert_eq!(s.active_keymap(1_000 + STALE_MS + 1).id, "global");

        // A fresh report revives the app keymap.
        s.update("com.mitchellh.ghostty.desktop".to_string(), 20_000);
        assert_eq!(s.active_keymap(20_100).id, "com.mitchellh.ghostty.desktop");
    }
}
