#!/usr/bin/env python3
"""M5 integration test (runs inside the VM, as root).

Create a virtual *source* keyboard (a stand-in for a physical one), let mackeyd
grab it, inject a key sequence, and read it back off mackeyd's virtual *output*
keyboard. Passthrough is identity, so output must equal input.

Prints 'RESULT: PASS' / 'RESULT: FAIL' and exits non-zero on failure.
"""
import select
import sys
import time

from evdev import InputDevice, UInput, ecodes, list_devices

OUTPUT_NAME = "mackey virtual keyboard"
SOURCE_NAME = "mackey-test-source"
SEQUENCE = [ecodes.KEY_H, ecodes.KEY_I]


def main() -> int:
    # Declare a full keyboard key range (ESC..F-keys). udev's input_id only sets
    # ID_INPUT_KEYBOARD=1 — which our udev rule keys on to grant the mackey group
    # access — when the standard keys ESC..D are all present. A real keyboard has
    # them; a partial set would be tagged ID_INPUT_KEY only and never grabbed.
    keys = list(range(ecodes.KEY_ESC, ecodes.KEY_F12 + 1))  # covers A/Z/SPACE and H/I
    source = UInput({ecodes.EV_KEY: keys}, name=SOURCE_NAME)

    # Give mackeyd's inotify watcher time to open + grab the new source.
    time.sleep(2.5)

    out_path = next((p for p in list_devices() if InputDevice(p).name == OUTPUT_NAME), None)
    if out_path is None:
        print("RESULT: FAIL (no output device found)")
        return 1
    output = InputDevice(out_path)

    # Drain anything pending, then inject.
    try:
        while output.read_one() is not None:
            pass
    except BlockingIOError:
        pass

    def tap(code: int) -> None:
        source.write(ecodes.EV_KEY, code, 1)
        source.syn()
        source.write(ecodes.EV_KEY, code, 0)
        source.syn()

    time.sleep(0.2)
    for code in SEQUENCE:
        tap(code)

    got: list[int] = []
    deadline = time.time() + 2.0
    while time.time() < deadline and len(got) < len(SEQUENCE):
        r, _, _ = select.select([output.fd], [], [], 0.2)
        if not r:
            continue
        for event in output.read():
            if event.type == ecodes.EV_KEY and event.value == 1:
                got.append(event.code)

    print(f"injected={SEQUENCE} got={got}")
    if got == SEQUENCE:
        print("RESULT: PASS")
        return 0
    print("RESULT: FAIL")
    return 1


if __name__ == "__main__":
    sys.exit(main())
