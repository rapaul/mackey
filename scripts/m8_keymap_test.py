#!/usr/bin/env python3
"""M8 integration test (runs inside the VM, as root).

Inject Super+T on a source keyboard and assert the daemon's virtual output
emits the sequence the *currently active* per-app keymap dictates. The caller
sets the focus first by calling UpdateFocus over D-Bus as the seat user, then
runs this with the matching app name.

Usage:  m8_keymap_test.py <ghostty|firefox>
Prints 'RESULT: PASS' / 'RESULT: FAIL' and exits non-zero on failure.
"""
import select
import sys
import time

from evdev import InputDevice, UInput, ecodes, list_devices

OUTPUT_NAME = "mackey virtual keyboard"
SOURCE_NAME = "mackey-test-source"

# Super+T under each app's keymap. Ghostty uses the terminal convention
# (Ctrl+Shift+T); Firefox uses plain Ctrl+T (the global mapping).
EXPECTED = {
    "ghostty": [
        (ecodes.KEY_LEFTCTRL, 1),
        (ecodes.KEY_LEFTSHIFT, 1),
        (ecodes.KEY_T, 1),
        (ecodes.KEY_T, 0),
        (ecodes.KEY_LEFTSHIFT, 0),
        (ecodes.KEY_LEFTCTRL, 0),
    ],
    "firefox": [
        (ecodes.KEY_LEFTCTRL, 1),
        (ecodes.KEY_T, 1),
        (ecodes.KEY_T, 0),
        (ecodes.KEY_LEFTCTRL, 0),
    ],
}


def main() -> int:
    if len(sys.argv) != 2 or sys.argv[1] not in EXPECTED:
        print("RESULT: FAIL (usage: m8_keymap_test.py <ghostty|firefox>)")
        return 2
    expected = EXPECTED[sys.argv[1]]

    # Full keyboard range (incl. LEFTMETA) so udev tags it ID_INPUT_KEYBOARD.
    source = UInput({ecodes.EV_KEY: list(range(1, 128))}, name=SOURCE_NAME)
    time.sleep(2.5)  # let mackeyd grab it

    out_path = next((p for p in list_devices() if InputDevice(p).name == OUTPUT_NAME), None)
    if out_path is None:
        print("RESULT: FAIL (no output device found)")
        return 1
    output = InputDevice(out_path)
    try:
        while output.read_one() is not None:
            pass
    except BlockingIOError:
        pass

    def ev(code: int, value: int) -> None:
        source.write(ecodes.EV_KEY, code, value)
        source.syn()

    time.sleep(0.2)
    ev(ecodes.KEY_LEFTMETA, 1)
    ev(ecodes.KEY_T, 1)
    ev(ecodes.KEY_T, 0)
    ev(ecodes.KEY_LEFTMETA, 0)

    got: list[tuple[int, int]] = []
    deadline = time.time() + 2.0
    while time.time() < deadline and len(got) < len(expected):
        r, _, _ = select.select([output.fd], [], [], 0.2)
        if not r:
            continue
        for event in output.read():
            if event.type == ecodes.EV_KEY:
                got.append((event.code, event.value))

    print(f"expected={expected} got={got}")
    super_codes = {ecodes.KEY_LEFTMETA, ecodes.KEY_RIGHTMETA}
    if got == expected and not any(c in super_codes for c, _ in got):
        print("RESULT: PASS")
        return 0
    print("RESULT: FAIL")
    return 1


if __name__ == "__main__":
    sys.exit(main())
