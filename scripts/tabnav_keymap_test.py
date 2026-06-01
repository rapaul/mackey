#!/usr/bin/env python3
"""Tab-navigation keymap test (runs inside the VM, as root).

Inject Super+Shift+[ and Super+Shift+] on a source keyboard and assert the
daemon's virtual output emits Ctrl+PageUp / Ctrl+PageDown (the global keymap's
prev/next-tab bindings) with no Super and no leaked Shift.

Prints 'RESULT: PASS' / 'RESULT: FAIL' and exits non-zero on failure.
"""
import select
import sys
import time

from evdev import InputDevice, UInput, ecodes, list_devices

OUTPUT_NAME = "mackey virtual keyboard"
SOURCE_NAME = "mackey-test-source"

# (in_key, expected output events). Super(Meta)+Shift+[ -> Ctrl down, PageUp
# down/up, Ctrl up; likewise ] -> Ctrl+PageDown.
CASES = [
    (
        ecodes.KEY_LEFTBRACE,
        [
            (ecodes.KEY_LEFTCTRL, 1),
            (ecodes.KEY_PAGEUP, 1),
            (ecodes.KEY_PAGEUP, 0),
            (ecodes.KEY_LEFTCTRL, 0),
        ],
    ),
    (
        ecodes.KEY_RIGHTBRACE,
        [
            (ecodes.KEY_LEFTCTRL, 1),
            (ecodes.KEY_PAGEDOWN, 1),
            (ecodes.KEY_PAGEDOWN, 0),
            (ecodes.KEY_LEFTCTRL, 0),
        ],
    ),
]


def drain(output: InputDevice) -> None:
    try:
        while output.read_one() is not None:
            pass
    except BlockingIOError:
        pass


def main() -> int:
    # Full keyboard range (incl. LEFTMETA) so udev tags it ID_INPUT_KEYBOARD.
    source = UInput({ecodes.EV_KEY: list(range(1, 128))}, name=SOURCE_NAME)
    time.sleep(2.5)  # let mackeyd grab it

    out_path = next((p for p in list_devices() if InputDevice(p).name == OUTPUT_NAME), None)
    if out_path is None:
        print("RESULT: FAIL (no output device found)")
        return 1
    output = InputDevice(out_path)

    def ev(code: int, value: int) -> None:
        source.write(ecodes.EV_KEY, code, value)
        source.syn()

    super_codes = {ecodes.KEY_LEFTMETA, ecodes.KEY_RIGHTMETA}
    shift_codes = {ecodes.KEY_LEFTSHIFT, ecodes.KEY_RIGHTSHIFT}
    ok = True
    for in_key, expected in CASES:
        drain(output)
        time.sleep(0.2)
        # Inject Super+Shift+<key> as a physical keyboard would.
        ev(ecodes.KEY_LEFTMETA, 1)
        ev(ecodes.KEY_LEFTSHIFT, 1)
        ev(in_key, 1)
        ev(in_key, 0)
        ev(ecodes.KEY_LEFTSHIFT, 0)
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

        leaked = any(c in super_codes or c in shift_codes for c, _ in got)
        case_ok = got == expected and not leaked
        print(f"in_key={in_key} expected={expected} got={got} leaked_mod={leaked}")
        ok = ok and case_ok

    if ok:
        print("RESULT: PASS")
        return 0
    print("RESULT: FAIL")
    return 1


if __name__ == "__main__":
    sys.exit(main())
