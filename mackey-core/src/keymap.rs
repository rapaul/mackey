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
//! - **GNOME Terminal** (`org.gnome.Terminal.desktop`) — terminal convention:
//!   Super+{C,V,X,T,W,N} -> Ctrl+Shift+{same}; the rest map to Ctrl+{same}.
//! - **Ghostty** (`com.mitchellh.ghostty.desktop`) — Ghostty's own defaults (its
//!   macOS `super+` bindings rewritten to the Linux bindings): Super+{C,V,A,F,N,
//!   T,W,Q} -> Ctrl+Shift+{same}; Super+{,/=/-/0/Enter} -> Ctrl+{same};
//!   Super+{1..9} -> Alt+{same} (tab navigation); Super+Shift+{,/Enter/P} ->
//!   Ctrl+Shift+{same}; the new splits Super+D -> Ctrl+Shift+O and Super+Shift+D
//!   -> Ctrl+Shift+E; and split navigation Super+[ / ] -> Super+Ctrl+[ / ],
//!   Super+Alt+Arrow -> Ctrl+Alt+Arrow (goto), Super+Ctrl+Arrow ->
//!   Super+Ctrl+Shift+Arrow (resize).
//!
//! A binding may therefore differ from the global "same key" rule in two ways: it
//! can require extra **input modifiers** (Shift/Ctrl/Alt) as part of the trigger —
//! so one key carries several bindings (Cmd+D vs Cmd+Shift+D; Cmd+Alt+Up vs
//! Cmd+Ctrl+Up) — and it can emit a **different output key** than the input. Input
//! modifiers are matched on what's physically held, so press order is irrelevant
//! (Shift-then-Cmd resolves the same as Cmd-then-Shift). In every keymap, anything
//! not in the active table passes through untouched: a key with Super *not* held,
//! a lone Super tap (emitted on release so GNOME's Activities still opens), and
//! Super(+mods) + any *unmapped* key (the real Super, plus any held modifiers, is
//! emitted so e.g. Super+L still reaches the desktop).
//!
//! The engine is a pure state machine over `(code, value)` key events plus an
//! active [`Keymap`], so it can be exhaustively unit-tested without a kernel.

use evdev::KeyCode;

const LEFTMETA: u16 = KeyCode::KEY_LEFTMETA.code();
const RIGHTMETA: u16 = KeyCode::KEY_RIGHTMETA.code();
const LEFTCTRL: u16 = KeyCode::KEY_LEFTCTRL.code();
const RIGHTCTRL: u16 = KeyCode::KEY_RIGHTCTRL.code();
const LEFTSHIFT: u16 = KeyCode::KEY_LEFTSHIFT.code();
const RIGHTSHIFT: u16 = KeyCode::KEY_RIGHTSHIFT.code();
const LEFTALT: u16 = KeyCode::KEY_LEFTALT.code();
const RIGHTALT: u16 = KeyCode::KEY_RIGHTALT.code();

// Input key codes used by the built-in tables.
const KEY_C: u16 = KeyCode::KEY_C.code();
const KEY_V: u16 = KeyCode::KEY_V.code();
const KEY_X: u16 = KeyCode::KEY_X.code();
const KEY_D: u16 = KeyCode::KEY_D.code();
const KEY_E: u16 = KeyCode::KEY_E.code();
const KEY_A: u16 = KeyCode::KEY_A.code();
const KEY_Z: u16 = KeyCode::KEY_Z.code();
const KEY_S: u16 = KeyCode::KEY_S.code();
const KEY_F: u16 = KeyCode::KEY_F.code();
const KEY_N: u16 = KeyCode::KEY_N.code();
const KEY_O: u16 = KeyCode::KEY_O.code();
const KEY_W: u16 = KeyCode::KEY_W.code();
const KEY_Q: u16 = KeyCode::KEY_Q.code();
const KEY_T: u16 = KeyCode::KEY_T.code();
const KEY_P: u16 = KeyCode::KEY_P.code();
const KEY_LEFT: u16 = KeyCode::KEY_LEFT.code();
const KEY_RIGHT: u16 = KeyCode::KEY_RIGHT.code();
const KEY_UP: u16 = KeyCode::KEY_UP.code();
const KEY_DOWN: u16 = KeyCode::KEY_DOWN.code();
const KEY_LEFTBRACE: u16 = KeyCode::KEY_LEFTBRACE.code();
const KEY_RIGHTBRACE: u16 = KeyCode::KEY_RIGHTBRACE.code();

// Ghostty-specific keys (font size, config, fullscreen, tab navigation).
const KEY_COMMA: u16 = KeyCode::KEY_COMMA.code();
const KEY_MINUS: u16 = KeyCode::KEY_MINUS.code();
const KEY_EQUAL: u16 = KeyCode::KEY_EQUAL.code();
const KEY_ENTER: u16 = KeyCode::KEY_ENTER.code();
const KEY_0: u16 = KeyCode::KEY_0.code();
const KEY_1: u16 = KeyCode::KEY_1.code();
const KEY_2: u16 = KeyCode::KEY_2.code();
const KEY_3: u16 = KeyCode::KEY_3.code();
const KEY_4: u16 = KeyCode::KEY_4.code();
const KEY_5: u16 = KeyCode::KEY_5.code();
const KEY_6: u16 = KeyCode::KEY_6.code();
const KEY_7: u16 = KeyCode::KEY_7.code();
const KEY_8: u16 = KeyCode::KEY_8.code();
const KEY_9: u16 = KeyCode::KEY_9.code();

// Output modifier sets a binding can request, held while the mapped key is
// emitted. (Super-bearing sets are for Ghostty's split navigation, whose Linux
// bindings keep Super: e.g. goto_split is super+ctrl+[.)
const CTRL: &[u16] = &[LEFTCTRL];
const CTRL_SHIFT: &[u16] = &[LEFTCTRL, LEFTSHIFT];
const ALT: &[u16] = &[LEFTALT];
const CTRL_ALT: &[u16] = &[LEFTCTRL, LEFTALT];
const SUPER_CTRL: &[u16] = &[LEFTMETA, LEFTCTRL];
const SUPER_CTRL_SHIFT: &[u16] = &[LEFTMETA, LEFTCTRL, LEFTSHIFT];

// evdev key event values.
const RELEASE: i32 = 0;
const PRESS: i32 = 1;

/// The non-Super modifiers that can take part in a shortcut, tracked as a set so
/// a binding's trigger and the engine's live state can be compared directly.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
struct InputMods {
    shift: bool,
    ctrl: bool,
    alt: bool,
}

/// One modifier class. Left/right variants of a physical modifier collapse to the
/// same class; `idx` indexes the engine's per-class arrays.
#[derive(Clone, Copy)]
enum Mod {
    Shift,
    Ctrl,
    Alt,
}

impl Mod {
    fn idx(self) -> usize {
        match self {
            Mod::Shift => 0,
            Mod::Ctrl => 1,
            Mod::Alt => 2,
        }
    }
}

impl InputMods {
    fn get(self, m: Mod) -> bool {
        match m {
            Mod::Shift => self.shift,
            Mod::Ctrl => self.ctrl,
            Mod::Alt => self.alt,
        }
    }

    fn set(&mut self, m: Mod, v: bool) {
        match m {
            Mod::Shift => self.shift = v,
            Mod::Ctrl => self.ctrl = v,
            Mod::Alt => self.alt = v,
        }
    }

    fn any(self) -> bool {
        self.shift || self.ctrl || self.alt
    }
}

const NO_MODS: InputMods = InputMods {
    shift: false,
    ctrl: false,
    alt: false,
};
const SHIFT_IN: InputMods = InputMods {
    shift: true,
    ctrl: false,
    alt: false,
};
const CTRL_IN: InputMods = InputMods {
    shift: false,
    ctrl: true,
    alt: false,
};
const ALT_IN: InputMods = InputMods {
    shift: false,
    ctrl: false,
    alt: true,
};

/// Which modifier class a key code belongs to (collapsing left/right), if any.
fn mod_class(code: u16) -> Option<Mod> {
    if code == LEFTSHIFT || code == RIGHTSHIFT {
        Some(Mod::Shift)
    } else if code == LEFTCTRL || code == RIGHTCTRL {
        Some(Mod::Ctrl)
    } else if code == LEFTALT || code == RIGHTALT {
        Some(Mod::Alt)
    } else {
        None
    }
}

/// One rewrite rule. While Super is held, pressing `in_key` together with exactly
/// the modifiers in `in_mods` is emitted as `out_mods` + `out_key` instead. Both
/// halves are general: `out_key` may differ from `in_key` (Ghostty's Cmd+D split
/// -> Ctrl+Shift+O), and `in_mods` lets one input key carry several bindings
/// (Cmd+D vs Cmd+Shift+D; Cmd+Alt+Up vs Cmd+Ctrl+Up).
#[derive(Clone, Copy)]
struct Binding {
    in_key: u16,
    in_mods: InputMods,
    out_mods: &'static [u16],
    out_key: u16,
}

/// The common rule: same key, no extra input modifier (Super+X -> mods+X).
const fn key(in_key: u16, out_mods: &'static [u16]) -> Binding {
    Binding {
        in_key,
        in_mods: NO_MODS,
        out_mods,
        out_key: in_key,
    }
}

/// A built-in keymap: while Super is held, each listed binding is emitted with
/// its modifier set instead of Super. Keys absent from `bindings` pass the real
/// Super through.
pub struct Keymap {
    /// Stable identifier, logged on switch (`"global"` or a `.desktop` id).
    pub id: &'static str,
    bindings: &'static [Binding],
}

impl Keymap {
    /// The binding for `code` given exactly the input modifiers held, if any.
    fn binding_for(&self, code: u16, mods: InputMods) -> Option<Binding> {
        self.bindings
            .iter()
            .copied()
            .find(|b| b.in_key == code && b.in_mods == mods)
    }
}

static GLOBAL: Keymap = Keymap {
    id: "global",
    bindings: &[
        key(KEY_C, CTRL),
        key(KEY_V, CTRL),
        key(KEY_X, CTRL),
        key(KEY_A, CTRL),
        key(KEY_Z, CTRL),
        key(KEY_S, CTRL),
        key(KEY_F, CTRL),
        key(KEY_N, CTRL),
        key(KEY_O, CTRL),
        key(KEY_W, CTRL),
        key(KEY_Q, CTRL),
        key(KEY_T, CTRL),
    ],
};

static FILES: Keymap = Keymap {
    id: "org.gnome.Nautilus.desktop",
    bindings: &[
        key(KEY_C, CTRL),
        key(KEY_V, CTRL),
        key(KEY_X, CTRL),
        key(KEY_A, CTRL),
        key(KEY_Z, CTRL),
        key(KEY_S, CTRL),
        key(KEY_F, CTRL),
        key(KEY_N, CTRL),
        key(KEY_O, CTRL),
        key(KEY_W, CTRL),
        key(KEY_Q, CTRL),
        key(KEY_T, CTRL),
        // Files-specific: go to the parent directory.
        key(KEY_UP, ALT),
    ],
};

static FIREFOX: Keymap = Keymap {
    id: "firefox.desktop",
    bindings: &[
        key(KEY_C, CTRL),
        key(KEY_V, CTRL),
        key(KEY_X, CTRL),
        key(KEY_A, CTRL),
        key(KEY_Z, CTRL),
        key(KEY_S, CTRL),
        key(KEY_F, CTRL),
        key(KEY_N, CTRL),
        key(KEY_O, CTRL),
        key(KEY_W, CTRL),
        key(KEY_Q, CTRL),
        key(KEY_T, CTRL),
        // Firefox-specific: history back / forward.
        key(KEY_LEFT, ALT),
        key(KEY_RIGHT, ALT),
    ],
};

// Shared by every terminal keymap. Terminal copy/paste/cut and tab/window/split
// need Shift, since plain Ctrl+C is SIGINT in a terminal; the remaining global
// letters keep plain Ctrl.
const TERMINAL_ENTRIES: &[Binding] = &[
    key(KEY_C, CTRL_SHIFT),
    key(KEY_V, CTRL_SHIFT),
    key(KEY_X, CTRL_SHIFT),
    key(KEY_T, CTRL_SHIFT),
    key(KEY_W, CTRL_SHIFT),
    key(KEY_N, CTRL_SHIFT),
    key(KEY_A, CTRL),
    key(KEY_Z, CTRL),
    key(KEY_S, CTRL),
    key(KEY_F, CTRL),
    key(KEY_O, CTRL),
    key(KEY_Q, CTRL),
];

// Ghostty's own defaults, derived from `src/config/Config.zig` (the macOS
// `super+` bindings) cross-referenced with `ghostty +list-keybinds --default`
// (the Linux bindings). Each binding rewrites Ghostty's macOS shortcut to its
// Linux equivalent:
//
//   super+{c,v,a,f,n,t,w,q} -> ctrl+shift+{same}   (copy/paste/select-all/
//                                                    search/window/tab/quit)
//   super+{comma,=,-,0,enter} -> ctrl+{same}        (config/font-size/fullscreen)
//   super+{1..9} -> alt+{same}                       (goto_tab / last_tab)
//   super+shift+comma -> ctrl+shift+comma            (reload_config)
//   super+shift+enter -> ctrl+shift+enter            (toggle_split_zoom)
//   super+shift+p     -> ctrl+shift+p                (toggle_command_palette)
//   super+d -> ctrl+shift+o, super+shift+d -> ctrl+shift+e   (new_split right/down)
//   super+[ / super+] -> super+ctrl+[ / ]            (goto_split prev/next)
//   super+alt+arrow   -> ctrl+alt+arrow              (goto_split directional)
//   super+ctrl+arrow  -> super+ctrl+shift+arrow      (resize_split)
//
// Ghostty binds no super shortcut to S/Z/X/O on macOS, so those are absent here
// (they pass the real Super through rather than injecting Ctrl+S/Z into the
// shell). The macOS-only natural-text-editing binds (super+arrow, super+
// backspace) and alt+super+i (inspector) have no Linux default and are omitted.
const GHOSTTY_ENTRIES: &[Binding] = &[
    key(KEY_C, CTRL_SHIFT),
    key(KEY_V, CTRL_SHIFT),
    key(KEY_A, CTRL_SHIFT),
    key(KEY_F, CTRL_SHIFT),
    key(KEY_N, CTRL_SHIFT),
    key(KEY_T, CTRL_SHIFT),
    key(KEY_W, CTRL_SHIFT),
    key(KEY_Q, CTRL_SHIFT),
    key(KEY_COMMA, CTRL),
    key(KEY_EQUAL, CTRL),
    key(KEY_MINUS, CTRL),
    key(KEY_0, CTRL),
    key(KEY_ENTER, CTRL),
    key(KEY_1, ALT),
    key(KEY_2, ALT),
    key(KEY_3, ALT),
    key(KEY_4, ALT),
    key(KEY_5, ALT),
    key(KEY_6, ALT),
    key(KEY_7, ALT),
    key(KEY_8, ALT),
    key(KEY_9, ALT),
    // New split: Cmd+D opens a right split, Cmd+Shift+D a down split. Ghostty's
    // Linux defaults bind these to Ctrl+Shift+O / Ctrl+Shift+E (different keys).
    Binding {
        in_key: KEY_D,
        in_mods: NO_MODS,
        out_mods: CTRL_SHIFT,
        out_key: KEY_O,
    },
    Binding {
        in_key: KEY_D,
        in_mods: SHIFT_IN,
        out_mods: CTRL_SHIFT,
        out_key: KEY_E,
    },
    // Same-key Super+Shift bindings: reload_config, zoom split, command palette.
    Binding {
        in_key: KEY_COMMA,
        in_mods: SHIFT_IN,
        out_mods: CTRL_SHIFT,
        out_key: KEY_COMMA,
    },
    Binding {
        in_key: KEY_ENTER,
        in_mods: SHIFT_IN,
        out_mods: CTRL_SHIFT,
        out_key: KEY_ENTER,
    },
    Binding {
        in_key: KEY_P,
        in_mods: SHIFT_IN,
        out_mods: CTRL_SHIFT,
        out_key: KEY_P,
    },
    // goto_split: Cmd+[ / Cmd+] cycle splits (Linux super+ctrl+[ / ]);
    // Cmd+Alt+Arrow move directionally (Linux ctrl+alt+arrow).
    Binding {
        in_key: KEY_LEFTBRACE,
        in_mods: NO_MODS,
        out_mods: SUPER_CTRL,
        out_key: KEY_LEFTBRACE,
    },
    Binding {
        in_key: KEY_RIGHTBRACE,
        in_mods: NO_MODS,
        out_mods: SUPER_CTRL,
        out_key: KEY_RIGHTBRACE,
    },
    Binding {
        in_key: KEY_UP,
        in_mods: ALT_IN,
        out_mods: CTRL_ALT,
        out_key: KEY_UP,
    },
    Binding {
        in_key: KEY_DOWN,
        in_mods: ALT_IN,
        out_mods: CTRL_ALT,
        out_key: KEY_DOWN,
    },
    Binding {
        in_key: KEY_LEFT,
        in_mods: ALT_IN,
        out_mods: CTRL_ALT,
        out_key: KEY_LEFT,
    },
    Binding {
        in_key: KEY_RIGHT,
        in_mods: ALT_IN,
        out_mods: CTRL_ALT,
        out_key: KEY_RIGHT,
    },
    // resize_split: Cmd+Ctrl+Arrow (Linux super+ctrl+shift+arrow).
    Binding {
        in_key: KEY_UP,
        in_mods: CTRL_IN,
        out_mods: SUPER_CTRL_SHIFT,
        out_key: KEY_UP,
    },
    Binding {
        in_key: KEY_DOWN,
        in_mods: CTRL_IN,
        out_mods: SUPER_CTRL_SHIFT,
        out_key: KEY_DOWN,
    },
    Binding {
        in_key: KEY_LEFT,
        in_mods: CTRL_IN,
        out_mods: SUPER_CTRL_SHIFT,
        out_key: KEY_LEFT,
    },
    Binding {
        in_key: KEY_RIGHT,
        in_mods: CTRL_IN,
        out_mods: SUPER_CTRL_SHIFT,
        out_key: KEY_RIGHT,
    },
];

static GHOSTTY: Keymap = Keymap {
    id: "com.mitchellh.ghostty.desktop",
    bindings: GHOSTTY_ENTRIES,
};

static GNOME_TERMINAL: Keymap = Keymap {
    id: "org.gnome.Terminal.desktop",
    bindings: TERMINAL_ENTRIES,
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
    /// Which Shift/Ctrl/Alt modifiers are physically held, tracked at all times.
    /// Binding lookup keys on this set, so the press order of Super and the other
    /// modifiers doesn't matter (Shift-then-Cmd resolves the same as Cmd-then-Shift).
    phys: InputMods,
    /// The physical keycode last seen for each class (to emit/release the exact
    /// left/right variant the user pressed). Indexed by [`Mod::idx`].
    phys_code: [u16; 3],
    /// Modifiers pressed *during* a Super-hold whose press we deferred (did not
    /// emit). They drive binding lookup but stay invisible unless an unmapped
    /// combo forces them through. (Modifiers pressed before Super were emitted
    /// already, so they aren't suppressed — the binding's `out_mods` re-asserts
    /// them harmlessly.)
    suppressed: InputMods,
    /// Synthetic output modifiers we've pressed for the current Super-hold, in
    /// press order; released (in reverse) when Super is released.
    held_mods: Vec<u16>,
    /// We passed the real Super through for an unmapped combo and owe its release.
    emitted_super: bool,
    /// Remapped keys currently held, as `(in_key, out_key)`, so a key's release
    /// and autorepeat emit the same output key the press chose — even if a
    /// modifier was released first (which can change what a fresh lookup picks).
    active: Vec<(u16, u16)>,
}

impl Default for KeymapEngine {
    fn default() -> Self {
        Self {
            super_down: false,
            meta_code: LEFTMETA,
            phys: InputMods::default(),
            phys_code: [LEFTSHIFT, LEFTCTRL, LEFTALT],
            suppressed: InputMods::default(),
            held_mods: Vec::new(),
            emitted_super: false,
            active: Vec::new(),
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
        if let Some(m) = mod_class(code) {
            return self.process_mod(m, code, value);
        }
        if value == PRESS {
            if !self.super_down {
                return vec![KeyEvent::new(code, value)];
            }
            return match keymap.binding_for(code, self.phys) {
                Some(b) => self.emit_mapped(code, b),
                None => self.emit_unmapped(code),
            };
        }
        // Release / autorepeat: if this key was emitted as a remap, replay the
        // same output key; otherwise (passthrough or no-Super key) pass it on.
        match self.active_out(code, value == RELEASE) {
            Some(out_key) => vec![KeyEvent::new(out_key, value)],
            None => vec![KeyEvent::new(code, value)],
        }
    }

    /// Output key a remapped `code` is currently emitting, if any. When `remove`,
    /// the entry is dropped (a release ends the hold; autorepeat keeps it).
    fn active_out(&mut self, code: u16, remove: bool) -> Option<u16> {
        let idx = self.active.iter().position(|&(in_c, _)| in_c == code)?;
        let out_key = self.active[idx].1;
        if remove {
            self.active.remove(idx);
        }
        Some(out_key)
    }

    /// Super+mapped -> `out_mods`+`out_key`. Reconcile the held modifier set to
    /// `out_mods` (so a second key wanting different mods is correct), then emit
    /// the output key and record the remap for release/autorepeat.
    fn emit_mapped(&mut self, code: u16, b: Binding) -> Vec<KeyEvent> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.held_mods.len() {
            let held = self.held_mods[i];
            if b.out_mods.contains(&held) {
                i += 1;
            } else {
                out.push(KeyEvent::new(held, RELEASE));
                self.held_mods.remove(i);
            }
        }
        for &m in b.out_mods {
            if !self.held_mods.contains(&m) {
                out.push(KeyEvent::new(m, PRESS));
                self.held_mods.push(m);
            }
        }
        self.active.retain(|&(in_c, _)| in_c != code);
        self.active.push((code, b.out_key));
        out.push(KeyEvent::new(b.out_key, PRESS));
        out
    }

    /// Super(+mods)+unmapped -> pass the real Super, plus any modifiers whose
    /// press we deferred, through so the combo still reaches the desktop.
    fn emit_unmapped(&mut self, code: u16) -> Vec<KeyEvent> {
        let mut out = Vec::new();
        if !self.emitted_super {
            out.push(KeyEvent::new(self.meta_code, PRESS));
            self.emitted_super = true;
        }
        for m in [Mod::Shift, Mod::Ctrl, Mod::Alt] {
            if self.phys.get(m) && self.suppressed.get(m) {
                out.push(KeyEvent::new(self.phys_code[m.idx()], PRESS));
                self.suppressed.set(m, false);
            }
        }
        out.push(KeyEvent::new(code, PRESS));
        out
    }

    /// Shift/Ctrl/Alt handling. We always track the physical state. While Super is
    /// held, a fresh press is deferred (suppressed) so it shapes the binding
    /// lookup without leaking — unless an unmapped combo already forced the real
    /// Super through, in which case we pass the modifier too.
    fn process_mod(&mut self, m: Mod, code: u16, value: i32) -> Vec<KeyEvent> {
        match value {
            PRESS => {
                self.phys.set(m, true);
                self.phys_code[m.idx()] = code;
                if self.super_down {
                    if self.emitted_super {
                        return vec![KeyEvent::new(code, PRESS)];
                    }
                    self.suppressed.set(m, true);
                    return Vec::new();
                }
                vec![KeyEvent::new(code, PRESS)]
            }
            RELEASE => {
                self.phys.set(m, false);
                if self.suppressed.get(m) {
                    // The deferred press was never emitted: swallow the release.
                    self.suppressed.set(m, false);
                    return Vec::new();
                }
                vec![KeyEvent::new(code, RELEASE)]
            }
            // Autorepeat: swallow while the press is still deferred, else pass on.
            _ => {
                if self.super_down && self.suppressed.get(m) {
                    Vec::new()
                } else {
                    vec![KeyEvent::new(code, value)]
                }
            }
        }
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
                let had_mods = !self.held_mods.is_empty();
                for &m in self.held_mods.iter().rev() {
                    out.push(KeyEvent::new(m, RELEASE));
                }
                self.held_mods.clear();
                if self.emitted_super {
                    out.push(KeyEvent::new(self.meta_code, RELEASE));
                } else if !had_mods && !self.phys.any() {
                    // Nothing followed: emit the deferred tap so Super-alone works.
                    // (Suppressed when another modifier is held — Super+mod alone is
                    // a no-op, and that modifier's release is swallowed separately.)
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
        for b in GLOBAL.bindings {
            let k = b.in_key;
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

    /// Ghostty select-all is Cmd+A on macOS -> Ctrl+Shift+A on Linux (unlike the
    /// generic global table, where Super+A is plain Ctrl+A).
    #[test]
    fn ghostty_select_all_gets_ctrl_shift() {
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
                KeyEvent::new(LEFTSHIFT, 1),
                KeyEvent::new(KEY_A, 1),
                KeyEvent::new(KEY_A, 0),
                KeyEvent::new(LEFTSHIFT, 0),
                KeyEvent::new(LEFTCTRL, 0),
            ]
        );
    }

    /// Ghostty open-config (Cmd+, -> Ctrl+,) keeps plain Ctrl, no Shift.
    #[test]
    fn ghostty_open_config_gets_plain_ctrl() {
        let mut e = KeymapEngine::new();
        let out = drive(
            &mut e,
            &GHOSTTY,
            &[(LEFTMETA, 1), (KEY_COMMA, 1), (KEY_COMMA, 0), (LEFTMETA, 0)],
        );
        assert_eq!(
            out,
            vec![
                KeyEvent::new(LEFTCTRL, 1),
                KeyEvent::new(KEY_COMMA, 1),
                KeyEvent::new(KEY_COMMA, 0),
                KeyEvent::new(LEFTCTRL, 0),
            ]
        );
    }

    /// Ghostty goto-tab (Cmd+1 -> Alt+1 on Linux) rewrites Super to Alt.
    #[test]
    fn ghostty_goto_tab_gets_alt() {
        let mut e = KeymapEngine::new();
        let out = drive(
            &mut e,
            &GHOSTTY,
            &[(LEFTMETA, 1), (KEY_1, 1), (KEY_1, 0), (LEFTMETA, 0)],
        );
        assert_eq!(
            out,
            vec![
                KeyEvent::new(LEFTALT, 1),
                KeyEvent::new(KEY_1, 1),
                KeyEvent::new(KEY_1, 0),
                KeyEvent::new(LEFTALT, 0),
            ]
        );
    }

    /// Ghostty binds no super shortcut to S on macOS (Ctrl+S would freeze the
    /// terminal), so Super+S is unmapped and the real Super passes through.
    #[test]
    fn ghostty_super_s_is_unmapped_passthrough() {
        let mut e = KeymapEngine::new();
        let out = drive(
            &mut e,
            &GHOSTTY,
            &[(LEFTMETA, 1), (KEY_S, 1), (KEY_S, 0), (LEFTMETA, 0)],
        );
        assert_eq!(
            out,
            vec![
                KeyEvent::new(LEFTMETA, 1),
                KeyEvent::new(KEY_S, 1),
                KeyEvent::new(KEY_S, 0),
                KeyEvent::new(LEFTMETA, 0),
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

    /// Ghostty quit is Cmd+Q -> Ctrl+Shift+Q on Linux, whereas GNOME Terminal
    /// follows the generic terminal convention (plain Ctrl+Q). They no longer
    /// share one table.
    #[test]
    fn ghostty_and_gnome_terminal_differ_on_quit() {
        assert_eq!(
            GHOSTTY.binding_for(KEY_Q, NO_MODS).map(|b| b.out_mods),
            Some(CTRL_SHIFT)
        );
        assert_eq!(
            GNOME_TERMINAL
                .binding_for(KEY_Q, NO_MODS)
                .map(|b| b.out_mods),
            Some(CTRL)
        );
    }

    /// Ghostty's right split is Cmd+D -> Ctrl+Shift+O: the output key (O) differs
    /// from the input key (D).
    #[test]
    fn ghostty_split_right_remaps_d_to_o() {
        let mut e = KeymapEngine::new();
        let out = drive(
            &mut e,
            &GHOSTTY,
            &[(LEFTMETA, 1), (KEY_D, 1), (KEY_D, 0), (LEFTMETA, 0)],
        );
        assert_eq!(
            out,
            vec![
                KeyEvent::new(LEFTCTRL, 1),
                KeyEvent::new(LEFTSHIFT, 1),
                KeyEvent::new(KEY_O, 1),
                KeyEvent::new(KEY_O, 0),
                KeyEvent::new(LEFTSHIFT, 0),
                KeyEvent::new(LEFTCTRL, 0),
            ]
        );
    }

    /// Adding Shift to the same input key picks a different binding: Cmd+Shift+D
    /// is the down split -> Ctrl+Shift+E. The deferred physical Shift is consumed
    /// (never emitted); the synthetic Shift comes from the binding's out_mods.
    #[test]
    fn ghostty_split_down_uses_shifted_binding() {
        let mut e = KeymapEngine::new();
        let out = drive(
            &mut e,
            &GHOSTTY,
            &[
                (LEFTMETA, 1),
                (LEFTSHIFT, 1),
                (KEY_D, 1),
                (KEY_D, 0),
                (LEFTSHIFT, 0),
                (LEFTMETA, 0),
            ],
        );
        assert_eq!(
            out,
            vec![
                KeyEvent::new(LEFTCTRL, 1),
                KeyEvent::new(LEFTSHIFT, 1),
                KeyEvent::new(KEY_E, 1),
                KeyEvent::new(KEY_E, 0),
                KeyEvent::new(LEFTSHIFT, 0),
                KeyEvent::new(LEFTCTRL, 0),
            ]
        );
    }

    /// Releasing the remapped key emits the same output key the press chose, even
    /// when Shift is released before the key (a fresh lookup would pick the right
    /// split's O instead of E).
    #[test]
    fn ghostty_split_down_release_translates_to_chosen_key() {
        let mut e = KeymapEngine::new();
        let out = drive(
            &mut e,
            &GHOSTTY,
            &[
                (LEFTMETA, 1),
                (LEFTSHIFT, 1),
                (KEY_D, 1),
                (LEFTSHIFT, 0), // Shift released first
                (KEY_D, 0),
                (LEFTMETA, 0),
            ],
        );
        // The E press and its matching E release must both appear (never an O).
        assert!(out.contains(&KeyEvent::new(KEY_E, 1)));
        assert!(out.contains(&KeyEvent::new(KEY_E, 0)));
        assert!(!out.iter().any(|ev| ev.code == KEY_O));
    }

    /// Super+Shift+, (reload_config) is a same-key shifted binding -> Ctrl+Shift+,.
    #[test]
    fn ghostty_reload_config_uses_shifted_comma() {
        let mut e = KeymapEngine::new();
        let out = drive(
            &mut e,
            &GHOSTTY,
            &[
                (LEFTMETA, 1),
                (LEFTSHIFT, 1),
                (KEY_COMMA, 1),
                (KEY_COMMA, 0),
                (LEFTSHIFT, 0),
                (LEFTMETA, 0),
            ],
        );
        assert_eq!(
            out,
            vec![
                KeyEvent::new(LEFTCTRL, 1),
                KeyEvent::new(LEFTSHIFT, 1),
                KeyEvent::new(KEY_COMMA, 1),
                KeyEvent::new(KEY_COMMA, 0),
                KeyEvent::new(LEFTSHIFT, 0),
                KeyEvent::new(LEFTCTRL, 0),
            ]
        );
    }

    /// Super+Shift on a key with no shifted binding falls through to the unmapped
    /// path: the real Super+Shift+key passes through to the desktop.
    #[test]
    fn ghostty_super_shift_unmapped_passes_through() {
        let mut e = KeymapEngine::new();
        let out = drive(
            &mut e,
            &GHOSTTY,
            &[
                (LEFTMETA, 1),
                (LEFTSHIFT, 1),
                (KEY_L, 1),
                (KEY_L, 0),
                (LEFTSHIFT, 0),
                (LEFTMETA, 0),
            ],
        );
        assert_eq!(
            out,
            vec![
                KeyEvent::new(LEFTMETA, 1),
                KeyEvent::new(LEFTSHIFT, 1),
                KeyEvent::new(KEY_L, 1),
                KeyEvent::new(KEY_L, 0),
                KeyEvent::new(LEFTSHIFT, 0),
                KeyEvent::new(LEFTMETA, 0),
            ]
        );
    }

    /// Pressing Shift *before* Super still selects the shifted binding: Cmd+Shift+D
    /// is the down split regardless of modifier order. (The physical Shift leaks
    /// through, but the binding's Ctrl+Shift output re-asserts it, so E fires with
    /// Ctrl+Shift held — and never an O.)
    #[test]
    fn ghostty_shift_before_super_selects_down_split() {
        let mut e = KeymapEngine::new();
        let out = drive(
            &mut e,
            &GHOSTTY,
            &[
                (LEFTSHIFT, 1), // Shift first
                (LEFTMETA, 1),
                (KEY_D, 1),
                (KEY_D, 0),
                (LEFTMETA, 0),
                (LEFTSHIFT, 0),
            ],
        );
        assert!(out.contains(&KeyEvent::new(KEY_E, 1)));
        assert!(out.contains(&KeyEvent::new(KEY_E, 0)));
        assert!(out.contains(&KeyEvent::new(LEFTCTRL, 1)));
        assert!(!out.iter().any(|ev| ev.code == KEY_O));
    }

    /// goto_split prev: Cmd+[ -> Super+Ctrl+[ (output keeps Super; same key).
    #[test]
    fn ghostty_goto_split_bracket_keeps_super() {
        let mut e = KeymapEngine::new();
        let out = drive(
            &mut e,
            &GHOSTTY,
            &[
                (LEFTMETA, 1),
                (KEY_LEFTBRACE, 1),
                (KEY_LEFTBRACE, 0),
                (LEFTMETA, 0),
            ],
        );
        assert_eq!(
            out,
            vec![
                KeyEvent::new(LEFTMETA, 1),
                KeyEvent::new(LEFTCTRL, 1),
                KeyEvent::new(KEY_LEFTBRACE, 1),
                KeyEvent::new(KEY_LEFTBRACE, 0),
                KeyEvent::new(LEFTCTRL, 0),
                KeyEvent::new(LEFTMETA, 0),
            ]
        );
    }

    /// goto_split directional: Cmd+Alt+Up -> Ctrl+Alt+Up. Alt is an input modifier
    /// that selects the binding and is replaced by the output set.
    #[test]
    fn ghostty_goto_split_alt_arrow_becomes_ctrl_alt() {
        let mut e = KeymapEngine::new();
        let out = drive(
            &mut e,
            &GHOSTTY,
            &[
                (LEFTMETA, 1),
                (LEFTALT, 1),
                (KEY_UP, 1),
                (KEY_UP, 0),
                (LEFTALT, 0),
                (LEFTMETA, 0),
            ],
        );
        assert_eq!(
            out,
            vec![
                KeyEvent::new(LEFTCTRL, 1),
                KeyEvent::new(LEFTALT, 1),
                KeyEvent::new(KEY_UP, 1),
                KeyEvent::new(KEY_UP, 0),
                KeyEvent::new(LEFTALT, 0),
                KeyEvent::new(LEFTCTRL, 0),
            ]
        );
    }

    /// resize_split: Cmd+Ctrl+Up -> Super+Ctrl+Shift+Up. Ctrl is an input modifier;
    /// the output keeps Super+Ctrl and adds Shift.
    #[test]
    fn ghostty_resize_split_ctrl_arrow_adds_shift() {
        let mut e = KeymapEngine::new();
        let out = drive(
            &mut e,
            &GHOSTTY,
            &[
                (LEFTMETA, 1),
                (LEFTCTRL, 1),
                (KEY_UP, 1),
                (KEY_UP, 0),
                (LEFTCTRL, 0),
                (LEFTMETA, 0),
            ],
        );
        assert_eq!(
            out,
            vec![
                KeyEvent::new(LEFTMETA, 1),
                KeyEvent::new(LEFTCTRL, 1),
                KeyEvent::new(LEFTSHIFT, 1),
                KeyEvent::new(KEY_UP, 1),
                KeyEvent::new(KEY_UP, 0),
                KeyEvent::new(LEFTSHIFT, 0),
                KeyEvent::new(LEFTCTRL, 0),
                KeyEvent::new(LEFTMETA, 0),
            ]
        );
    }

    /// Up arrow carries two Ghostty bindings keyed on the input modifier: Alt ->
    /// goto_split (Ctrl+Alt), Ctrl -> resize_split (Super+Ctrl+Shift).
    #[test]
    fn ghostty_arrow_binding_depends_on_input_modifier() {
        assert_eq!(
            GHOSTTY.binding_for(KEY_UP, ALT_IN).map(|b| b.out_mods),
            Some(CTRL_ALT)
        );
        assert_eq!(
            GHOSTTY.binding_for(KEY_UP, CTRL_IN).map(|b| b.out_mods),
            Some(SUPER_CTRL_SHIFT)
        );
        assert!(GHOSTTY.binding_for(KEY_UP, NO_MODS).is_none());
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
