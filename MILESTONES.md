# mackey — Milestone plan

Walking-skeleton / vertical-slice breakdown of the PRD. Each milestone is **independently shippable**: it builds a `.deb` and `.rpm`, installs cleanly on a clean Fedora 40 + Ubuntu 24.04 VM, and adds exactly one observable capability on top of the previous milestone. Every milestone defines both:

- **Automated tests** — runs in CI on every push (`cargo test`, integration harness, package linters).
- **VM verification** — a scripted scenario that installs the package on a clean VM, exercises the new capability, and asserts a concrete observable.

The "skeleton" that walks end-to-end through every milestone is the loop: **source → cargo build → `.deb`/`.rpm` → install on VM → systemd starts daemon → assertion passes**. M0–M2 build that loop with a do-nothing binary. M3 onward thickens it.

Two cross-cutting invariants are checked at every milestone from M3 onward:

- `id -nG tester` is byte-identical before and after install (the login user is never added to a group).
- The post-install script's only side effects are: create `mackey` system user/group, drop files under `/usr/`. It never enables a service, never touches GNOME settings.

---

## M0 — Toolchain & workspace skeleton

**What ships:** A cargo workspace with three crates (`mackeyd`, `mackey-gui`, `mackey-core`) and a `packaging/` directory. `mackeyd` is a binary that logs `mackeyd v0.0.0 starting` to stderr and exits 0. `rustfmt.toml`, `clippy.toml`, and a `rust-toolchain.toml` pinning a stable channel are in place.

**Automated tests**
- `cargo build --workspace` succeeds.
- `cargo clippy --workspace --all-targets -- -D warnings` is clean.
- `cargo test --workspace` runs (zero tests is acceptable).
- CI runs all three on every push.

**VM verification:** none yet — nothing to install. This is the "tools are installed and the repo compiles" gate.

---

## M1 — Hello-world `.deb` and `.rpm`

**What ships:** `cargo-deb` and `cargo-generate-rpm` metadata in `Cargo.toml` that package `mackeyd` to `/usr/bin/mackeyd` and nothing else. CI builds both on every push and uploads them as workflow artifacts.

**Automated tests**
- `cargo deb` and `cargo generate-rpm` produce artifacts in CI.
- `dpkg-deb -I` shows correct name, version, maintainer, depends.
- `rpm -qpi` shows the same metadata for the RPM.
- `lintian` (deb) and `rpmlint` (rpm) run; any errors fail CI (warnings allowed for now, tightened in M11).

**VM verification:** deferred to M2 once a VM harness exists. At this point we can `dpkg -i` / `rpm -i` on the dev box as a sanity check.

---

## M2 — VM test harness

**What ships:** Reproducible ephemeral VM provisioning for Fedora 40 and Ubuntu 24.04, both running GNOME Wayland, with a non-sudo `tester` user. Suggested stack: cloud-init images + `libvirt`/`qemu` driven by a thin shell or Python wrapper; alternative is `vagrant` with the `libvirt` provider. A `vmtest` CLI in the repo:

```
vmtest <fedora40|ubuntu24> install <pkg>     # copies and installs
vmtest <fedora40|ubuntu24> run "<cmd>"        # runs as tester
vmtest <fedora40|ubuntu24> journal mackey     # dumps daemon log
vmtest <fedora40|ubuntu24> snapshot reset     # rolls back to clean
```

**Automated tests**
- CI spins up both VMs from cached base images, installs the M1 package, runs `mackeyd` once, asserts exit 0 and that `mackeyd v0.0.0 starting` appears in the captured output. Snapshot-resets between distros.
- `vmtest` exits non-zero on any step failure (it's the thing every later milestone depends on).

**VM verification:** is the harness itself. The clean-install of the hello-world package on both distros is the deliverable.

---

## M3 — Systemd service running as the `mackey` system user

**What ships:** Post-install scriptlet creates the `mackey` system user/group (`useradd --system --no-create-home --shell /usr/sbin/nologin`). Package installs `/usr/lib/systemd/system/mackey.service` with `User=mackey`, `Group=mackey`, `ExecStart=/usr/bin/mackeyd`. The daemon's body is still a sleep loop that emits a heartbeat log line every 30s. The service is **not** enabled by the package.

**Automated tests**
- mackey-core unit test for the (still trivial) main loop's shutdown signal handling.
- Package linter: post-install scriptlet is idempotent (running it twice does not error).

**VM verification**
- Install package → `id mackey` shows the new user, `getent group mackey` shows the group, `systemctl is-enabled mackey.service` reports `disabled`.
- `systemctl enable --now mackey.service` → `systemctl is-active` returns `active`; `ps -o user= -p "$(pidof mackeyd)"` returns `mackey`.
- `id -nG tester` identical before/after install.
- `systemctl stop mackey.service` exits cleanly within 2s (the daemon's signal handling actually works).

---

## M4 — udev rule + `/dev/uinput` access (still no remapping)

**What ships:** Package installs `/usr/lib/udev/rules.d/90-mackey.rules` that ACLs `/dev/uinput` and keyboard-tagged `/dev/input/event*` to `GROUP="mackey", MODE="0660"`. Post-install runs `udevadm control --reload && udevadm trigger`. The daemon opens `/dev/uinput` at startup, logs success, then continues its sleep loop.

**Automated tests**
- mackey-core unit tests for any device-path classification helpers.
- A `udev-test` integration test (run in CI as root in a privileged container) installs the rule, fakes a uinput node, and asserts `getfacl` output.

**VM verification**
- After install, `getfacl /dev/uinput` shows `group:mackey:rw-`.
- Journal shows `opened /dev/uinput` from the daemon on startup.
- Plug a virtual USB keyboard via `qemu` device-add → `getfacl /dev/input/event<N>` shows `group:mackey:rw-` within 2s.
- `id -nG tester` still unchanged.

---

## M5 — Identity passthrough remapper (the first real walking skeleton)

**What ships:** Daemon now:
- Enumerates `/dev/input/event*` keyboard nodes, opens each with the `evdev` crate, **grabs exclusively**.
- Watches `/dev/input/` via `inotify` for new keyboards; opens + grabs them as they appear; releases them on removal.
- Creates a single uinput virtual keyboard and forwards every event from every grabbed device to it, unmodified.

No keymap engine yet — this is identity passthrough. This is the milestone where mackeyd is a "real" program for the first time; from here on every milestone is a small change to either the engine or the surrounding plumbing.

**Automated tests**
- `mackey-core` unit tests for the event-stream wiring (use `evdev`'s in-memory fixtures, no kernel needed).
- Integration test (`vmtest`): inject events into a virtual `uinput` source keyboard, read them off the daemon's output uinput device, assert equality.

**VM verification**
- After `systemctl start`, typing in a terminal still produces the typed characters (proof that grab+passthrough works without breaking input).
- `vmtest fedora40 run "evemu-record /dev/input/by-id/mackey-virtual-keyboard"` captures events generated by typing on the source.
- Hotplug: add a USB keyboard device mid-session → journal logs `grabbed /dev/input/eventN`; typing on it still reaches userspace.

---

## M6 — Built-in global keymap

**What ships:** `mackey-core` gains a small keymap engine and the hardcoded **global fallback** keymap (Super+C/V/X/A/Z/S/F/N/O/W/Q/T → Ctrl+equivalent). `mackeyd` plugs the engine between input and uinput output. No D-Bus, no focus tracking — every keystroke goes through the global keymap.

**Automated tests**
- Table-driven unit tests in `mackey-core`: feed `(modifiers, key)` tuples, assert the emitted output sequence. Cover every binding plus a handful of explicit "pass-through unchanged" cases.
- Property test: random key sequences with no Super modifier are passed through identically.
- Integration test (`vmtest`): inject Super+C on the source keyboard, assert Ctrl+C is read off the output uinput device.

**VM verification**
- In Firefox: select text in the URL bar, press Super+C, paste with Ctrl+V into Ghostty — the text appears.
- In Nautilus: Super+T opens a new tab.
- Stop the service → Super+C no longer copies, confirming the daemon is what made the difference.

---

## M7 — D-Bus focus tracker (no GNOME extension yet) + UID enforcement

**What ships:** Daemon owns the system-bus name `app.mackey.FocusTracker` (via `zbus`) and exposes `UpdateFocus(app_id: s, window_title: s)`. Package installs `/usr/share/dbus-1/system.d/app.mackey.conf` permitting any logged-in user to call that one method. On every call the daemon:

1. Looks up `sender_uid` via `org.freedesktop.DBus.GetConnectionUnixUser`.
2. Reads the active local-seat user from logind.
3. Rejects the call if those don't match (logs `rejected UpdateFocus from uid=N`).
4. Otherwise updates a `current_app_id: Option<String>` field that the keymap engine reads.

There is still only the global keymap, so an accepted call has no observable effect on remapping yet — it just sets the field.

**Automated tests**
- `mackey-core` IPC type round-trip tests.
- `zbus` server unit test using `zbus::Connection::session()` in a test fixture (sender UID mocking).
- Integration test (`vmtest`): `dbus-send --system --print-reply --dest=app.mackey.FocusTracker /app/mackey/FocusTracker app.mackey.FocusTracker.UpdateFocus string:firefox.desktop string:"Test"` from the active `tester` user succeeds; journal logs `accepted UpdateFocus app_id=firefox.desktop`.
- Same call from an SSH session for a different user is rejected; journal logs the rejection.

**VM verification**
- `busctl --system list | grep mackey` shows the well-known name.
- D-Bus policy permits `tester` to call only `UpdateFocus` and no other method on the bus name.
- UID rejection scenario above is reproducible interactively.

---

## M8 — GNOME Shell extension + per-app keymaps

**What ships:**
- `mackey-core` gains the three additional built-in keymaps: Files (`org.gnome.Nautilus.desktop`), Firefox (`firefox.desktop`), Ghostty (`com.mitchellh.ghostty.desktop`).
- Daemon switches the active keymap on `UpdateFocus`, with a **5s stale fallback**: if no `UpdateFocus` arrives for >5s the active keymap reverts to the global one and a warning is logged.
- Package ships `/usr/share/mackey/gnome-extension/mackey@mackey.app.shell-extension.zip`. The extension subscribes to `global.display::focus-window` and calls `UpdateFocus` over the system bus.

**Automated tests**
- Keymap selection unit tests in `mackey-core`: given a sequence of `UpdateFocus` events (with timestamps), assert which keymap is active at each tick, including the 5s stale-fallback timeout.
- Extension lint via `gnome-extensions pack --schemas` and `eslint`.

**VM verification**
- After `gnome-extensions install … && gnome-extensions enable …`, switching focus between Firefox and Ghostty produces journal entries `keymap → firefox.desktop` / `keymap → com.mitchellh.ghostty.desktop`.
- Pressing Super+T in Ghostty does the Ghostty-specific action; same key in Firefox does the Firefox one. (Concrete shortcut differences picked per the built-in keymap tables.)
- `gnome-extensions disable mackey@mackey.app` → within 10s the journal logs the fallback, and per-app behavior reverts to the global keymap.
- `id -nG tester` unchanged.

---

## M9 — GUI wizard view

**What ships:** `mackey-gui` becomes a real GTK4 app (single window, no tray, no background process). Package installs `/usr/bin/mackey` and `/usr/share/applications/mackey.desktop`. On launch the app:

- Polls `systemctl is-active mackey.service` and `gnome-extensions list --enabled` every 5s.
- If either is incomplete, shows the wizard view with copyable commands and one-line explanations.
- If both are satisfied, shows a placeholder "set up" screen (real status view lands in M10).

Wizard never executes commands on the user's behalf — only displays them.

**Automated tests**
- Unit tests on the state-detection layer in `mackey-gui` (with mocked command runners).
- A scripted-UI test using `dogtail` or `gtk4`'s testing helpers asserting that flipping a mocked state changes the rendered widgets within 5s.

**VM verification**
- Cold VM with service disabled and extension uninstalled → launch `mackey` from the application menu → wizard shows two pending items.
- Run the shown `sudo systemctl enable --now mackey.service` in a terminal → within 5s the first item flips to "done".
- Run the two `gnome-extensions` commands → within 5s the second item flips to "done".
- The wizard transitions to the placeholder satisfied screen.

---

## M10 — GUI status view + polkit-scoped Pause/Resume

**What ships:**
- Status view replaces the M9 placeholder: shows daemon state (`active` / `paused` / `failed`) and a single Pause/Resume toggle.
- Package installs `/usr/share/polkit-1/actions/app.mackey.policy` allowing the active local-seat user to `start` and `stop` (and only those) `mackey.service` without authentication.
- Toggle calls `systemctl start` / `stop` via the polkit-mediated D-Bus API.
- On relaunch when both wizard items are satisfied, the app boots straight to the status view; if either has regressed, the wizard returns for just that item.

**Automated tests**
- Polkit policy XML schema-validated in CI.
- Integration test (`vmtest`): from the `tester` session, call the same D-Bus method the GUI calls; assert no `sudo`/polkit auth dialog is needed (run with `PKEXEC_DISABLE_AGENT=1` and assert success).
- An auth-required negative test: from an SSH session for a different user, the same call fails with `not-authorized`.

**VM verification**
- Launch `mackey` post-wizard → status view shows `active`.
- Click Pause → daemon stops within 1s, Super+C no longer copies, **no auth prompt appears**.
- Click Resume → daemon restarts within 1s, Super+C copies again, no auth prompt.
- Disable extension externally → re-launch `mackey` → wizard returns just for the extension item.

---

## M11 — Systemd unit hardening

**What ships:** All hardening directives from the PRD (`NoNewPrivileges`, `CapabilityBoundingSet=`, `ProtectSystem=strict`, `ProtectHome=read-only`, `PrivateNetwork=yes`, `RestrictAddressFamilies=AF_UNIX`, `SystemCallFilter=@system-service`, `MemoryDenyWriteExecute=yes`, the `Protect*` suite) are applied to `mackey.service`. The daemon is audited to ensure it runs cleanly under every restriction (e.g. uses `AF_UNIX` only, never `mmap(PROT_WRITE | PROT_EXEC)`).

**Automated tests**
- CI runs `systemd-analyze security mackey.service` against the installed unit (in a Fedora container with the package installed); fails if the numeric score regresses below a tracked baseline stored in the repo.
- `lintian` and `rpmlint` warnings from M1 are now promoted to errors.

**VM verification**
- All M5–M10 scenarios still pass with the hardened unit.
- `systemd-analyze security mackey.service` in the VM matches the CI baseline (deterministic, given pinned distro images).
- 10-minute typing session (synthetic event injection via `vmtest`) produces zero journal errors at warn+ level.

---

## M12 — Tagged-release pipeline and full PRD verification

**What ships:** GitHub Actions workflow that on a tag push:

1. Builds `.deb` and `.rpm`.
2. Spins up Fedora 40 and Ubuntu 24.04 VMs from clean snapshots.
3. Runs the full PRD §Verification 1–6 checklist as a single `vmtest` scenario per distro.
4. On success, attaches both packages to a GitHub Release for the tag; on failure, the release is not created.

**Automated tests**
- The PRD §Verification checklist is encoded as a single integration scenario (`vmtest <distro> verify-prd`):
  1. Clean install of the tagged package on a fresh snapshot succeeds and post-install does not enable the service.
  2. Wizard scripted click-through: each item flips to "done" within 5s of running its shown command.
  3. Super+C in Firefox copies; Super+T in Nautilus opens a tab. `id -nG tester` identical before and after install.
  4. Disabling the GNOME extension causes a warning + global-keymap fallback within 10s; re-enabling restores per-app behavior.
  5. GUI Pause/Resume works without an auth prompt for the active seat user.
  6. `systemctl status mackey.service` is active under `mackey`; `ps -o user,supgrp -p $(pidof mackeyd)` shows `mackey` as the only supplementary group; 10-minute typing session is journal-clean.

**VM verification:** is the scenario above. Green CI on the tag = release is publishable.

---

## Notes on slicing choices

- **Why M0–M2 first.** The user's example correctly identified that a runnable hello-world `.rpm`/`.deb` plus an automated VM install loop is the *real* skeleton — every later milestone is a small diff on top of it.
- **Why the daemon thickens before the GUI (M3–M8 then M9–M10).** The daemon is the only thing whose security model is hard to get right; the GUI is a thin client of the daemon's D-Bus + polkit surface. Bringing the daemon all the way to "focus-aware remapping works" before building any GUI means the GUI is built against an already-verified backend, and the GUI milestones can focus on UX rather than discovering daemon bugs.
- **Why hardening is M11 and not M3.** Hardening directives are easy to add but easy to break silently — every directive constrains some syscall or capability that a later feature might want. Adding them last with a `systemd-analyze` regression gate means the score only ratchets in one direction from here on.
- **What's deliberately not a milestone.** Anything in the PRD's *Non-goals*: X11, KDE/Sway/Hyprland, Option-key special characters, multi-tap, per-device profiles, migration tools, user-editable config. v0.2 (user config + editor) is its own milestone plan, not part of MVP.
