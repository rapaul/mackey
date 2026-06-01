# vmtest — local VM integration harness

Reproducible, ephemeral VMs for mackey's integration tests. Runs **rootless**:
cloud-init images booted with `qemu` (KVM), user-mode networking + an SSH
port-forward. No libvirt, no host `sudo`. Replaces hosted CI — everything runs
on the dev box and in these local VMs.

## Requirements

- `/dev/kvm` accessible to your user (world-rw on the dev box).
- `qemu-system-x86_64`, `qemu-img`, `genisoimage`, `ssh`/`scp`, `wget`.
- ~6 GB free disk per distro (base image + golden + ephemeral overlay).

Run `vmtest/vmtest selftest` to check tooling without booting anything.

## Distros

| key                     | image                          | ssh port |
|-------------------------|--------------------------------|----------|
| `ubuntu` (`ubuntu24`)   | Ubuntu 24.04 server cloud      | 2224     |
| `fedora` (`fedora43`)   | Fedora 43 Cloud Base           | 2243     |

> The milestone plan names Fedora 40; per project decision we track the latest
> Fedora (43) since 40's cloud images are archived. Both VMs run **GNOME
> Wayland** (gdm autologin as `tester`).

## Image layers

```
images/<distro>.qcow2          pristine base cloud image (downloaded once)
run/<distro>/provisioned.qcow2 golden: base + GNOME + tester (built once)
run/<distro>/disk.qcow2        ephemeral overlay; tests run here
```

`snapshot reset` drops the ephemeral overlay and recreates it from the golden —
an O(1) rollback to a clean state between tests.

Data lives under `$VMTEST_DATA` (default `~/.local/share/mackey-vmtest`); point
it elsewhere to use a different disk.

## Commands

```
vmtest <distro> fetch              download the base image
vmtest <distro> up                 provision golden (once) + boot ephemeral
vmtest <distro> install <pkg>      install a .deb/.rpm
vmtest <distro> run "<cmd>"        run a command as tester (exit code passes through)
vmtest <distro> journal <unit>     dump a systemd unit's journal
vmtest <distro> snapshot reset     roll back to the golden image
vmtest <distro> down               power off
vmtest <distro> destroy            power off + delete per-distro state
vmtest <distro> status             running / ssh-reachable
vmtest selftest                    validate tooling + arg parsing (no VM)
```

First `up` for a distro provisions the golden image (installs GNOME via
cloud-init) and is slow (10–30 min). Subsequent boots are fast.

## M2 scenario

`scripts/vm-verify-m1.sh <distro>` installs the M1 package, runs `mackeyd` once,
asserts it prints `mackeyd v0.2.0 starting` and exits 0, then resets the VM.
This is the M2 deliverable: a clean install of the hello-world package on both
distros.
