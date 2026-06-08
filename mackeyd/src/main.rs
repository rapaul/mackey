//! The mackey daemon.
//!
//! M6 — the global keymap. mackeyd grabs every physical keyboard (existing ones
//! at startup, new ones via inotify) and forwards their events to a single
//! virtual output keyboard, running each key event through the built-in global
//! keymap engine (Super+{C,V,X,…} -> Ctrl+…). No focus tracking yet.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, BusType, Device, EventType, InputEvent, InputId, KeyCode};
use inotify::{Inotify, WatchMask};
use mackey_core::{is_keyboard, FocusState, Keymap, KeymapEngine, VIRTUAL_KEYBOARD_NAME};

mod focus;

const INPUT_DIR: &str = "/dev/input";

/// Shared handles the reader threads forward through: the keymap engine
/// (modifier state), the single virtual output keyboard, and the focus state
/// that selects the active keymap. `start` is the monotonic origin for the
/// focus timestamps.
#[derive(Clone)]
struct Forwarder {
    engine: Arc<Mutex<KeymapEngine>>,
    out: Arc<Mutex<VirtualDevice>>,
    focus: Arc<Mutex<FocusState>>,
    start: Instant,
}

/// Identity for the virtual output keyboard. Declared on the PS/2 (`i8042`) bus
/// so libinput's bundled `10-generic-keyboard.quirks` tags it
/// `AttrKeyboardIntegration=internal`. The daemon grabs the real keyboard, so
/// without this the synthetic keystrokes would come from a device libinput
/// treats as external — silently breaking disable-while-typing (libinput pairs
/// touchpads only with internal keyboards).
fn virtual_keyboard_id() -> InputId {
    InputId::new(BusType::BUS_I8042, 0x1, 0x1, 0x1)
}

/// Build the virtual output keyboard, advertising the full key range so any
/// emitted key (including the synthetic Ctrl) is accepted by the kernel.
fn build_virtual_keyboard() -> std::io::Result<VirtualDevice> {
    let mut keys = AttributeSet::<KeyCode>::new();
    for code in 1u16..0x300 {
        keys.insert(KeyCode::new(code));
    }
    VirtualDevice::builder()?
        .name(VIRTUAL_KEYBOARD_NAME)
        .input_id(virtual_keyboard_id())
        .with_keys(&keys)?
        .build()
}

/// Run a batch of input events through the keymap engine, translating EV_KEY
/// events and passing everything else (SYN, MSC, …) through unchanged.
fn translate(engine: &mut KeymapEngine, keymap: &Keymap, batch: &[InputEvent]) -> Vec<InputEvent> {
    let mut out = Vec::with_capacity(batch.len());
    for ev in batch {
        if ev.event_type() == EventType::KEY {
            for k in engine.process(ev.code(), ev.value(), keymap) {
                out.push(InputEvent::new(EventType::KEY.0, k.code, k.value));
            }
        } else {
            out.push(*ev);
        }
    }
    out
}

/// A keyboard we should grab — a real keyboard and not our own output.
fn is_grabbable(dev: &Device) -> bool {
    if dev.name() == Some(VIRTUAL_KEYBOARD_NAME) {
        return false;
    }
    dev.supported_keys().map(is_keyboard).unwrap_or(false)
}

/// Grab one device and spawn a thread forwarding its events through the engine.
fn grab_and_forward(path: PathBuf, mut dev: Device, fwd: Forwarder) {
    if dev.grab().is_err() {
        return;
    }
    thread::spawn(move || {
        eprintln!("grabbed {}", path.display());
        loop {
            let batch: Vec<InputEvent> = match dev.fetch_events() {
                Ok(evts) => evts.collect(),
                Err(e) => {
                    // ENODEV on unplug, or any read error: dropping the device
                    // closes the fd, which ungrabs it.
                    eprintln!("released {} ({e})", path.display());
                    return;
                }
            };
            // Pick the keymap for the current focus, then translate the batch.
            let now_ms = fwd.start.elapsed().as_millis() as u64;
            let keymap = match fwd.focus.lock() {
                Ok(focus) => focus.active_keymap(now_ms),
                Err(_) => continue,
            };
            let translated = match fwd.engine.lock() {
                Ok(mut engine) => translate(&mut engine, keymap, &batch),
                Err(_) => continue,
            };
            if let Ok(mut out) = fwd.out.lock() {
                let _ = out.emit(&translated);
            }
        }
    });
}

/// Grab every keyboard already present at startup.
fn grab_existing(fwd: &Forwarder) {
    for (path, dev) in evdev::enumerate() {
        if is_grabbable(&dev) {
            grab_and_forward(path, dev, fwd.clone());
        }
    }
}

/// Open + grab a freshly-appeared node, retrying briefly so udev has time to
/// apply the mackey-group ACL before we (running as mackey) open it.
fn grab_new_node(path: &Path, fwd: &Forwarder) {
    for attempt in 0..10 {
        thread::sleep(Duration::from_millis(100));
        match Device::open(path) {
            Ok(dev) => {
                if is_grabbable(&dev) {
                    grab_and_forward(path.to_path_buf(), dev, fwd.clone());
                }
                return;
            }
            Err(_) if attempt < 9 => continue,
            Err(_) => return,
        }
    }
}

/// Watch /dev/input for new keyboards and grab them as they appear.
fn hotplug_loop(fwd: Forwarder) {
    let mut inotify = match Inotify::init() {
        Ok(i) => i,
        Err(e) => {
            eprintln!("hotplug disabled: inotify init failed: {e}");
            return;
        }
    };
    if let Err(e) = inotify.watches().add(INPUT_DIR, WatchMask::CREATE) {
        eprintln!("hotplug disabled: cannot watch {INPUT_DIR}: {e}");
        return;
    }
    let mut buf = [0u8; 4096];
    loop {
        let events = match inotify.read_events_blocking(&mut buf) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("hotplug watch ended: {e}");
                return;
            }
        };
        for event in events {
            let Some(name) = event.name else { continue };
            let name = name.to_string_lossy();
            if name.starts_with("event") {
                grab_new_node(&PathBuf::from(INPUT_DIR).join(&*name), &fwd);
            }
        }
    }
}

/// Emit a warning the moment focus reports go stale (>5s without an
/// `UpdateFocus`), once per stale period. The keymap reversion itself is handled
/// by `FocusState::active_keymap`; this only surfaces it in the journal.
fn stale_watchdog(focus: Arc<Mutex<FocusState>>, start: Instant) {
    let mut warned = false;
    loop {
        thread::sleep(Duration::from_secs(1));
        let now_ms = start.elapsed().as_millis() as u64;
        let stale = match focus.lock() {
            Ok(focus) => focus.is_stale(now_ms),
            Err(_) => continue,
        };
        if stale && !warned {
            eprintln!("warning: no UpdateFocus in >5s, falling back to global keymap");
            warned = true;
        } else if !stale {
            warned = false;
        }
    }
}

fn main() {
    eprintln!("mackeyd v{} starting", mackey_core::VERSION);

    let out = match build_virtual_keyboard() {
        Ok(dev) => {
            eprintln!("created virtual keyboard \"{VIRTUAL_KEYBOARD_NAME}\"");
            Arc::new(Mutex::new(dev))
        }
        Err(e) => {
            eprintln!("fatal: cannot create virtual keyboard: {e}");
            std::process::exit(1);
        }
    };
    // Monotonic origin and shared focus state for keymap selection.
    let start = Instant::now();
    let focus_state: Arc<Mutex<FocusState>> = Arc::new(Mutex::new(FocusState::new()));

    let fwd = Forwarder {
        engine: Arc::new(Mutex::new(KeymapEngine::new())),
        out,
        focus: focus_state.clone(),
        start,
    };

    grab_existing(&fwd);

    let hotplug_fwd = fwd.clone();
    thread::spawn(move || hotplug_loop(hotplug_fwd));

    // The focus tracker updates the shared focus state on each accepted call.
    let focus_serve = focus_state.clone();
    thread::spawn(move || focus::serve(focus_serve, start));

    // Warn (once) when focus reports go stale and the keymap reverts to global.
    let focus_watch = focus_state.clone();
    thread::spawn(move || stale_watchdog(focus_watch, start));

    // Block until asked to stop. Process exit closes every grabbed fd, which
    // ungrabs the physical keyboards — so input is never left frozen.
    let mut signals = signal_hook::iterator::Signals::new([
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGINT,
    ])
    .expect("register SIGTERM/SIGINT handler");
    signals.forever().next();

    eprintln!("mackeyd shutting down");
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The virtual keyboard must advertise the i8042 (PS/2) bus: that is what
    /// makes libinput tag it an internal keyboard (via its bundled
    /// `10-generic-keyboard.quirks`) so disable-while-typing keeps working while
    /// the daemon holds the real keyboard grabbed.
    #[test]
    fn virtual_keyboard_is_on_the_i8042_bus() {
        assert_eq!(virtual_keyboard_id().bus_type(), BusType::BUS_I8042);
    }
}
