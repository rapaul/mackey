# mackey

macOS-style keyboard shortcuts on Linux. Press <kbd>⌘</kbd><kbd>C</kbd> to copy,
<kbd>⌘</kbd><kbd>T</kbd> for a new tab — mackey rewrites the <kbd>⌘</kbd> (Super)
shortcuts your fingers already know into the Linux equivalent for the app you're
focused on.

**Supported platform: GNOME on Wayland only.** X11, KDE, and Sway are out of scope.

## Installation

mackey ships as a `.deb` / `.rpm`. There is no hosted CI — build the package
locally, install it, then run the wizard.

```sh
# 1. Build the .deb and .rpm
./scripts/build-packages.sh

# 2. Install the package for your distro
sudo apt install ./target/debian/mackeyd_*.deb        # Debian / Ubuntu
sudo dnf install ./target/generate-rpm/mackeyd-*.rpm  # Fedora

# 3. Run the setup wizard
mackey
```

The wizard starts the daemon and enables the GNOME extension. **Enabling the
extension requires logging out and back in** (a Wayland session can't load a
freshly installed shell extension live) — the wizard will tell you when. Log out,
log back in, and re-run `mackey` to finish.

## Shortcuts

While <kbd>⌘</kbd> (the Super/Meta key) is held, mackey remaps to the right Linux
shortcut for the focused app. Anything not in the table passes through untouched —
a lone <kbd>⌘</kbd> tap still opens Activities.

### Common shortcuts (everywhere)

The global fallback for any app without a dedicated keymap:

| Press | Does |
| --- | --- |
| <kbd>⌘</kbd><kbd>C</kbd> | Copy |
| <kbd>⌘</kbd><kbd>V</kbd> | Paste |
| <kbd>⌘</kbd><kbd>X</kbd> | Cut |
| <kbd>⌘</kbd><kbd>A</kbd> | Select all |
| <kbd>⌘</kbd><kbd>Z</kbd> | Undo |
| <kbd>⌘</kbd><kbd>S</kbd> | Save |
| <kbd>⌘</kbd><kbd>F</kbd> | Find |
| <kbd>⌘</kbd><kbd>N</kbd> | New |
| <kbd>⌘</kbd><kbd>O</kbd> | Open |
| <kbd>⌘</kbd><kbd>T</kbd> | New tab |
| <kbd>⌘</kbd><kbd>W</kbd> | Close |
| <kbd>⌘</kbd><kbd>Q</kbd> | Quit |

### Firefox

All of the common shortcuts above, plus:

| Press | Does |
| --- | --- |
| <kbd>⌘</kbd><kbd>R</kbd> | Reload |
| <kbd>⌘</kbd><kbd>⇧</kbd><kbd>P</kbd> | Private window |

### Ghostty

| Press | Does |
| --- | --- |
| <kbd>⌘</kbd><kbd>C</kbd> | Copy |
| <kbd>⌘</kbd><kbd>V</kbd> | Paste |
| <kbd>⌘</kbd><kbd>A</kbd> | Select all |
| <kbd>⌘</kbd><kbd>F</kbd> | Find |
| <kbd>⌘</kbd><kbd>T</kbd> | New tab |
| <kbd>⌘</kbd><kbd>W</kbd> | Close tab |
| <kbd>⌘</kbd><kbd>N</kbd> | New window |
| <kbd>⌘</kbd><kbd>Q</kbd> | Quit |
| <kbd>⌘</kbd><kbd>1</kbd>…<kbd>9</kbd> | Switch to tab 1–9 |
| <kbd>⌘</kbd><kbd>D</kbd> | Split right |
| <kbd>⌘</kbd><kbd>⇧</kbd><kbd>D</kbd> | Split down |
| <kbd>⌘</kbd><kbd>[</kbd> / <kbd>⌘</kbd><kbd>]</kbd> | Go to previous / next split |
| <kbd>⌘</kbd><kbd>⌥</kbd><kbd>←</kbd><kbd>↑</kbd><kbd>↓</kbd><kbd>→</kbd> | Go to split by direction |
| <kbd>⌘</kbd><kbd>⌃</kbd><kbd>←</kbd><kbd>↑</kbd><kbd>↓</kbd><kbd>→</kbd> | Resize split |
| <kbd>⌘</kbd><kbd>⇧</kbd><kbd>↵</kbd> | Zoom / unzoom split |
| <kbd>⌘</kbd><kbd>⇧</kbd><kbd>P</kbd> | Command palette |
| <kbd>⌘</kbd><kbd>=</kbd> / <kbd>⌘</kbd><kbd>-</kbd> / <kbd>⌘</kbd><kbd>0</kbd> | Font bigger / smaller / reset |
| <kbd>⌘</kbd><kbd>↵</kbd> | Toggle fullscreen |
| <kbd>⌘</kbd><kbd>,</kbd> | Open config |

### Files (Nautilus)

All of the common shortcuts above, plus:

| Press | Does |
| --- | --- |
| <kbd>⌘</kbd><kbd>↑</kbd> | Go to parent folder |

## License

[MIT](LICENSE)
