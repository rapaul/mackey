# PRD: mackey — a Rust port of Toshy

## Context

Linux users coming from macOS lose their muscle memory the moment they switch desktops: Ctrl vs Cmd, different shortcut combinations in every app, and Option-key special characters that just don't exist. [Toshy](https://github.com/RedBearAK/toshy) solves this on Linux today, but it's a Python application built on top of `xwaykeyz`/`keyszer` — a stack that ships a Python virtualenv, runs an installer script that touches the system in many places, and is comparatively heavy to package and distribute cleanly.

**mackey** is a Rust rewrite of Toshy with a deliberately narrow MVP: ship a single statically-linked daemon plus a small GTK4 configuration app, distributed as `.rpm` and `.deb` packages whose post-install does as little as possible. On first launch the GUI walks the user through the one-time system setup (systemd user service, udev rule, GNOME Shell extension) and then gets out of the way.

The goal is *not* feature parity with Toshy on day one. The goal is a clean foundation that nails the most common case — a MacBook-layout keyboard on Fedora or Ubuntu running GNOME on Wayland — and is easy to extend.

## Goals

- A user on Fedora or Ubuntu (GNOME/Wayland) can `dnf install mackey` / `apt install mackey`, launch the app, click through a short wizard, and have macOS-style shortcuts working within a couple of minutes.
- The daemon is a single Rust binary with no Python runtime, no virtualenv, no per-install compilation.
- All dependencies (systemd unit, udev rule, GNOME Shell extension, GTK4 app) ship inside the package.
- Configuration lives in human-editable TOML; the GUI is a convenience, not a requirement.

## Non-goals (MVP)

- X11 support, KDE / Sway / Hyprland / COSMIC support — GNOME Wayland only.
- The 600+ Option-key special characters from Toshy.
- Multi-tap, hold-to-modify, and other advanced layering features.
- Per-keyboard-device profiles or runtime device hotswap intelligence beyond "it works when you plug one in".
- Windows / macOS support.
- Migration tooling for existing Toshy / xremap / kanata configurations.

## Target user

A developer or power user on Fedora 40+ / Ubuntu 24.04+ running GNOME on Wayland with a MacBook-style keyboard (built-in laptop keyboard or external Apple/Keychron). They are comfortable installing packages from a `.rpm`/`.deb` file but should not need to touch the terminal after that.

## User experience

### Install
```
sudo dnf install ./mackey-0.1.0.x86_64.rpm        # or
sudo apt install ./mackey_0.1.0_amd64.deb
```
The package places:
- `/usr/bin/mackeyd` — the remapper daemon
- `/usr/bin/mackey` — the GTK4 configuration app
- `/usr/lib/systemd/system/mackey.service` — system service (not enabled yet); runs `mackeyd` as the dedicated `mackey` user with `Group=mackey` and no other supplementary groups
- `/usr/lib/udev/rules.d/90-mackey.rules` — sets `GROUP="mackey", MODE="0660"` on `/dev/uinput` and `/dev/input/event*`. Effective immediately for newly attached devices and after `udevadm trigger` for existing ones; full effect on next boot.
- `/usr/share/dbus-1/system.d/app.mackey.conf` — D-Bus system-bus policy permitting any logged-in user to call `app.mackey.FocusTracker.UpdateFocus` on the daemon
- `/usr/share/polkit-1/actions/app.mackey.policy` — polkit action allowing the active local-seat user to `start` / `stop` / `reload` `mackey.service` without a `sudo` prompt
- `/usr/share/mackey/gnome-extension/` — the bundled GNOME Shell extension source
- `/usr/share/applications/mackey.desktop` — launcher entry
- Default config template at `/usr/share/mackey/default-config.toml`

The post-install script creates the `mackey` system user and group (which own the daemon process and gate `/dev/input` access via the udev rule) and installs the files above. It does **not** enable the service, add any login user to any group, or touch GNOME settings — that's the wizard's job, with consent. **The invoking user's group memberships are never modified by mackey at any point.**

### First run
Launching `mackey` from the application menu shows a wizard:

1. **Welcome** — one-paragraph explanation of what mackey does.
2. **System setup** — two checklist items. mackey never runs these commands on the user's behalf; each item shows the exact command(s) the user should copy and run in a terminal, alongside a one-line explanation of what it does. While the wizard is open, the GUI polls for completion every 5 seconds and live-updates each item's status (pending → done) so the user gets immediate confirmation that what they ran worked.
   - **Enable the systemd system service.** Detected via `systemctl is-active mackey.service`. Command shown:
     ```
     sudo systemctl enable --now mackey.service
     ```
     This step is privileged because `mackeyd` runs as a system service under a dedicated `mackey` user — that isolation is what lets us grant the daemon `/dev/input` access via a udev-tagged group without ever adding the login user to `input` or any other privileged group.
   - **Install the GNOME Shell extension.** Detected by polling `gnome-extensions list --enabled` for `mackey@mackey.app`. Because GNOME Wayland exposes no public window-focus API, **mackey refuses to apply app-specific keymaps until the extension is detected**. Command shown:
     ```
     gnome-extensions install /usr/share/mackey/gnome-extension/mackey@mackey.app.shell-extension.zip
     gnome-extensions enable mackey@mackey.app
     ```
3. **Keyboard layout** — informs the user that mackey assumes a MacBook layout (Cmd in the position of the left Super/Win key) and links to the config file for advanced overrides. No prompt — assumed.
4. **Done** — the user closes the window. The daemon keeps running under systemd; mackey has no background GUI process, no tray icon. If the user wants to revisit configuration, they relaunch `mackey` from the application menu (or run `mackey` in a terminal). On relaunch, if both system items are still satisfied, the wizard is skipped and the user lands directly on a status view (see GUI below); if either item has regressed, the wizard reappears for just that item.

### Daily use
- Daemon runs as a systemd **system** service under the dedicated `mackey` user, captures all keyboards via `evdev`, and emits remapped events via `uinput`. It is the only mackey process running between configuration sessions.
- The GNOME Shell extension pushes focus changes to the daemon over the system D-Bus (`app.mackey.FocusTracker.UpdateFocus`); the daemon switches keymap context accordingly.
- There is no tray icon, no background GUI, and no menu-bar presence. To interact with mackey after install the user runs `mackey`.

## Functional requirements

### Daemon (`mackeyd`)
- Runs as a **systemd system service** under the dedicated `mackey` system user, with `Group=mackey` and no other supplementary groups. The udev rule ACLs `/dev/uinput` and `/dev/input/event*` to that group, so the daemon has exactly the access it needs and nothing else. No login user's groups are modified.
- Identifies the active local-seat user via logind and reads that user's config at `~/.config/mackey/config.toml` (world-readable 0644 by default; falls back to `/usr/share/mackey/default-config.toml` if absent). MVP assumes a single active seat.
- Watches `/dev/input/event*` for new keyboards via `inotify`; opens them with the `evdev` crate; grabs them exclusively.
- Maintains one `uinput` virtual keyboard via `evdev`'s uinput support; writes remapped events there.
- Owns the well-known **system-bus** name `app.mackey.FocusTracker`. The GNOME Shell extension calls `UpdateFocus(app_id, window_title)` on it whenever focus changes; the packaged D-Bus policy permits any logged-in user to make that call. If no `UpdateFocus` arrives for >5s the daemon falls back to the global keymap and logs a warning.
- `systemctl reload mackey.service` (SIGHUP) reloads the active user's config without restart.
- Logs to the systemd journal via `tracing` + `tracing-journald`.

### Config (TOML)
Minimum schema for MVP:
```toml
[global]
# Swap Ctrl behavior to Cmd-on-Super for most app shortcuts
swap_cmd_ctrl = true

[[keymap]]
name = "Default"
# Applies when no app-specific keymap matches
[keymap.bindings]
"Super+C" = "Ctrl+C"
"Super+V" = "Ctrl+V"
# ...

[[keymap]]
name = "Firefox"
match_app_id = "firefox.desktop"
[keymap.bindings]
"Super+T" = "Ctrl+T"
"Super+W" = "Ctrl+W"
```
Default config ships with bindings for: Files (Nautilus), Firefox, Ghostty, and a global fallback.

### GUI (`mackey`)
- GTK4 via `gtk4-rs`. Single foreground window; exits when the user closes it. No tray, no background process.
- Two views, picked at launch based on system state:
  - **Wizard view** — shown when either system-setup item is incomplete. Behaviour as described above (commands shown, 5s polling).
  - **Status view** — shown when everything is set up. Displays daemon status (active / paused / failed), the config file path with an "Open in editor" button (`xdg-open ~/.config/mackey/config.toml`), a "Reload config" button (`systemctl reload mackey.service`), and a "Pause / Resume" toggle (`systemctl stop` / `start mackey.service`). All three system-service calls go through the packaged polkit policy, so they don't prompt for `sudo` when invoked by the active seat user.
- No in-app keymap editor in MVP — editing happens in the user's text editor.

### GNOME Shell extension
- Minimal JS extension shipped as a `.shell-extension.zip` inside the package.
- On every focus change calls `app.mackey.FocusTracker.UpdateFocus(app_id, window_title)` on the **system D-Bus** (not the session bus — the daemon runs as the `mackey` system user, so the bridge has to cross that boundary). The packaged `/usr/share/dbus-1/system.d/app.mackey.conf` policy whitelists this one method for any logged-in user.
- No UI, no preferences — pure D-Bus bridge.

### Packaging
- `cargo-deb` for `.deb`, `cargo-generate-rpm` for `.rpm`. Both consume metadata blocks in `Cargo.toml`.
- Package declares runtime deps: `libgtk-4-1` (deb) / `gtk4` (rpm), `gnome-shell` (suggested, not required), `systemd`.
- CI workflow (GitHub Actions) builds both packages on tagged releases.

## Security model

mackey is designed so that gaining macOS-style shortcuts adds **zero standing privilege** to the user's account. The model rests on four packaged artifacts working together:

1. **Dedicated unprivileged system user.** `mackeyd` runs as the `mackey` system user (no shell, no home directory) — never as `root`, never as a logged-in user. A daemon compromise yields exactly evdev/uinput access and nothing else.
2. **Narrow device access via udev + group.** The packaged udev rule sets `GROUP="mackey", MODE="0660"` on `/dev/uinput` and `/dev/input/event*`; the systemd unit pins the daemon to that group with `Group=mackey`. The login user is **never** added to `input` or any other privileged group — `id -nG` is identical before and after install. The only root-touching action the user ever performs is the one-time `sudo systemctl enable --now mackey.service` in the first-run wizard.
3. **Polkit-scoped service control.** `app.mackey.policy` lets the active local-seat user run exactly `start`, `stop`, and `reload` on `mackey.service` — and nothing else — without a `sudo` prompt. That's what the GUI's Pause/Resume/Reload buttons call; arbitrary `systemctl` operations still require `sudo`.
4. **D-Bus policy whitelists one method.** `/usr/share/dbus-1/system.d/app.mackey.conf` lets any logged-in user call exactly `app.mackey.FocusTracker.UpdateFocus(app_id, window_title)` on the system bus. No other surface of the daemon is reachable from a user session.

**Threats explicitly out of scope.** mackey does not try to defend against a local user who already controls their own session — that user can already capture their own keystrokes and run their own `uinput` device. A malicious or buggy GNOME Shell extension can lie about which app is focused (causing the wrong keymap to apply) but cannot inject keystrokes, escalate privilege, or otherwise widen the attack surface beyond what an in-session attacker already has. mackey's job is to be a tool with minimal blast radius, not a sandbox around the user.

## Technical architecture (high-level)

- **Workspace layout:** `mackeyd/` (daemon), `mackey-gui/` (GTK4 app), `mackey-core/` (shared types: config schema, IPC messages, keymap engine), `gnome-extension/` (JS, not part of cargo workspace), `packaging/` (cargo-deb + cargo-generate-rpm metadata, systemd unit, udev rule).
- **Key crates:** `evdev` (input + uinput), `tokio` (async runtime + signal handling), `zbus` (D-Bus client for the GNOME extension), `serde` + `toml` (config), `tracing` + `tracing-journald` (logging), `gtk4` + `libadwaita` (GUI), `notify` or raw `inotify` (device hotplug).
- **Reference projects** to study but not depend on: [`xremap`](https://github.com/xremap/xremap) (simple evdev daemon), [`kanata`](https://github.com/jtroo/kanata) (modular layering engine).

## Verification

The MVP is shippable when, on a clean Fedora 40 and a clean Ubuntu 24.04 VM both running GNOME Wayland:

1. The `.rpm` / `.deb` installs without errors and does not modify the running system.
2. Launching `mackey` shows the wizard with two checklist items. Each item shows a copyable command and an explanation. The wizard polls every 5 seconds and a checklist item flips from "pending" to "done" within 5 seconds of the user running its command.
3. After wizard completion, Super+C/Super+V/Super+T behave as documented in Firefox and Nautilus — **without a logout and without any change to the user's group membership** (verify with `id -nG` before and after install).
4. Disabling the GNOME extension causes the daemon to log a warning and fall back to the global keymap within 10 seconds; re-enabling restores app-specific behavior.
5. Editing `~/.config/mackey/config.toml` and clicking "Reload config" in the status view applies changes without restarting the service.
6. `systemctl status mackey.service` reports active and running as the `mackey` user; `ps -o user,supgrp -p $(pidof mackeyd)` shows `mackey` as the only supplementary group; journal logs are clean of errors during a 10-minute typing session.

