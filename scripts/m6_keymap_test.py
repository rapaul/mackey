#!/usr/bin/env python3
"""M6 integration test (runs inside the VM, as root).

Inject Super+C on a source keyboard and assert the daemon's virtual output
emits Ctrl+C (and no Super), i.e. the global keymap rewrote it.

Prints 'RESULT: PASS' / 'RESULT: FAIL' and exits non-zero on failure.
"""
import select
import sys
import time

from evdev import InputDevice, UInput, ecodes, list_devices

OUTPUT_NAME = "mackey virtual keyboard"
SOURCE_NAME = "mackey-test-source"

# Super(Meta) + C  ->  expected  Ctrl down, C down, C up, Ctrl up.
EXPECTED = [
    (ecodes.KEY_LEFTCTRL, 1),
    (ecodes.KEY_C, 1),
    (ecodes.KEY_C, 0),
    (ecodes.KEY_LEFTCTRL, 0),
]


def main() -> int:
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

    # Inject Super+C as a physical keyboard would: Meta down, C down, C up, Meta up.
    def ev(code: int, value: int) -> None:
        source.write(ecodes.EV_KEY, code, value)
        source.syn()

    time.sleep(0.2)
    ev(ecodes.KEY_LEFTMETA, 1)
    ev(ecodes.KEY_C, 1)
    ev(ecodes.KEY_C, 0)
    ev(ecodes.KEY_LEFTMETA, 0)

    got: list[tuple[int, int]] = []
    deadline = time.time() + 2.0
    while time.time() < deadline and len(got) < len(EXPECTED):
        r, _, _ = select.select([output.fd], [], [], 0.2)
        if not r:
            continue
        for event in output.read():
            if event.type == ecodes.EV_KEY:
                got.append((event.code, event.value))

    print(f"expected={EXPECTED} got={got}")
    super_codes = {ecodes.KEY_LEFTMETA, ecodes.KEY_RIGHTMETA}
    if got == EXPECTED and not any(c in super_codes for c, _ in got):
        print("RESULT: PASS")
        return 0
    print("RESULT: FAIL")
    return 1


if __name__ == "__main__":
    sys.exit(main())
