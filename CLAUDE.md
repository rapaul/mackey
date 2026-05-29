# CLAUDE.md — mackey

macOS-style keyboard shortcuts on Linux (GNOME/Wayland). See `PRD.md` for the
product spec and `MILESTONES.md` for the build order. Read both before non-trivial
work.

## Workflow (required)

**Commit each logical step.** A logical step is one coherent change — a milestone
sub-task, a bug fix, a refactor. Don't batch unrelated changes into one commit, and
don't leave a working step uncommitted before starting the next.

**Run all tests before every commit.** Never commit on red.

```
cargo test --workspace
```

A commit is only allowed when the full gate passes:

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

If any of these fail, fix the cause before committing — don't `--no-verify`, don't
disable a lint, don't delete a failing test to make the gate pass.

Commit messages: imperative subject line, reference the milestone when relevant
(e.g. `M5: forward evdev events to uinput unmodified`).

## Test-first

Per `MILESTONES.md`, every milestone defines automated tests. Write the test (or the
failing assertion) first, then make it pass. The keymap engine in `mackey-core` is
table-driven and unit-testable with `evdev`'s in-memory fixtures — no kernel needed.
Integration tests that need a real `uinput`/systemd/D-Bus run through the `vmtest`
harness (M2 onward), not on the dev box.

## Architecture

Cargo workspace (the `gnome-extension/` JS and `packaging/` are outside the workspace):

- `mackey-core/` — shared types: built-in keymap tables, IPC message types, the
  keymap engine. The pure, heavily-tested core.
- `mackeyd/` — the daemon. systemd **system** service running as the dedicated
  `mackey` user. Grabs keyboards via `evdev`, emits via `uinput`, owns the
  `app.mackey.FocusTracker` system-bus name (`zbus`), logs via `tracing-journald`.
- `mackey-gui/` — GTK4 (`gtk4-rs` + `libadwaita`) wizard/status app. Single
  foreground window, no tray, no background process.
- `gnome-extension/` — minimal JS extension; D-Bus bridge that pushes focus changes
  to the daemon. No UI.
- `packaging/` — `cargo-deb` + `cargo-generate-rpm` metadata, systemd unit, udev
  rule, D-Bus policy, polkit policy.

## Conventions

- `rust-toolchain.toml` pins the stable channel — use it, don't `rustup override`.
- Keep changes surgical and the MVP narrow. Anything in the PRD's *Non-goals*
  (X11, KDE/Sway, Option-key specials, user-editable config) is out of scope — it
  belongs to v0.2, not this work.
- The security model is the hard part. Never widen the daemon's privilege: it runs as
  an unprivileged system user, gets `/dev/input` access only via the udev group ACL,
  and **never** modifies any login user's group membership. The packaging post-install
  must not enable the service or touch GNOME settings. Treat the PRD's *Security model*
  section as a contract.
- D-Bus calls into the daemon are UID-checked against the active local-seat user
  (logind). Don't bypass that check.
