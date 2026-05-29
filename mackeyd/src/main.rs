//! The mackey daemon.
//!
//! M5 — the first real walking skeleton. mackeyd creates one virtual output
//! keyboard, grabs every physical keyboard exclusively (existing ones at
//! startup, new ones via inotify), and forwards their events to the virtual
//! keyboard unmodified. No keymap engine yet; that arrives in M6.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, Device, InputEvent, KeyCode};
use inotify::{Inotify, WatchMask};
use mackey_core::{is_keyboard, VIRTUAL_KEYBOARD_NAME};

const INPUT_DIR: &str = "/dev/input";

/// Shared handle to the single virtual output keyboard.
type Output = Arc<Mutex<VirtualDevice>>;

/// Build the virtual output keyboard, advertising the full key range so any
/// forwarded key is accepted by the kernel.
fn build_virtual_keyboard() -> std::io::Result<VirtualDevice> {
    let mut keys = AttributeSet::<KeyCode>::new();
    for code in 1u16..0x300 {
        keys.insert(KeyCode::new(code));
    }
    VirtualDevice::builder()?
        .name(VIRTUAL_KEYBOARD_NAME)
        .with_keys(&keys)?
        .build()
}

/// A keyboard we should grab — i.e. a real keyboard and not our own output.
fn is_grabbable(dev: &Device) -> bool {
    if dev.name() == Some(VIRTUAL_KEYBOARD_NAME) {
        return false;
    }
    dev.supported_keys().map(is_keyboard).unwrap_or(false)
}

/// Grab one device and spawn a thread forwarding its events to the output.
fn grab_and_forward(path: PathBuf, mut dev: Device, out: Output) {
    if dev.grab().is_err() {
        return;
    }
    thread::spawn(move || {
        eprintln!("grabbed {}", path.display());
        loop {
            let events: Vec<InputEvent> = match dev.fetch_events() {
                Ok(evts) => evts.collect(),
                Err(e) => {
                    // ENODEV on unplug, or any read error: drop the device,
                    // which ungrabs it as the fd closes.
                    eprintln!("released {} ({e})", path.display());
                    return;
                }
            };
            if let Ok(mut out) = out.lock() {
                let _ = out.emit(&events);
            }
        }
    });
}

/// Grab every keyboard already present at startup.
fn grab_existing(out: &Output) {
    for (path, dev) in evdev::enumerate() {
        if is_grabbable(&dev) {
            grab_and_forward(path, dev, Arc::clone(out));
        }
    }
}

/// Open + grab a freshly-appeared node, retrying briefly so udev has time to
/// apply the mackey-group ACL before we (running as mackey) open it.
fn grab_new_node(path: &Path, out: &Output) {
    for attempt in 0..10 {
        thread::sleep(Duration::from_millis(100));
        match Device::open(path) {
            Ok(dev) => {
                if is_grabbable(&dev) {
                    grab_and_forward(path.to_path_buf(), dev, Arc::clone(out));
                }
                return;
            }
            // Permissions not applied yet: keep retrying for ~1s.
            Err(_) if attempt < 9 => continue,
            Err(_) => return,
        }
    }
}

/// Watch /dev/input for new keyboards and grab them as they appear.
fn hotplug_loop(out: Output) {
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
                grab_new_node(&PathBuf::from(INPUT_DIR).join(&*name), &out);
            }
        }
    }
}

fn main() {
    eprintln!("mackeyd v{} starting", mackey_core::VERSION);

    let out: Output = match build_virtual_keyboard() {
        Ok(dev) => {
            eprintln!("created virtual keyboard \"{VIRTUAL_KEYBOARD_NAME}\"");
            Arc::new(Mutex::new(dev))
        }
        Err(e) => {
            eprintln!("fatal: cannot create virtual keyboard: {e}");
            std::process::exit(1);
        }
    };

    grab_existing(&out);

    let hotplug_out = Arc::clone(&out);
    thread::spawn(move || hotplug_loop(hotplug_out));

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
