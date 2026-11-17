# Bundled FreeDOS boot files

These files make a floppy bootable into a DOS prompt. They are **FreeDOS**
components, licensed under the **GNU GPL v2 (or later)** — not MIT like the rest
of this project. They are bundled here as data (mere aggregation); the tool that
writes them is a separate work.

| File | What it is | Source | Version |
|------|------------|--------|---------|
| `BOOT.BIN` | 512-byte FAT12 boot sector (loads `KERNEL.SYS`) | FreeDOS 1.3 Floppy Edition, `144m/x86BOOT.img` (first sector) | 1.3 (2022) |
| `KERNEL.SYS` | FreeDOS kernel | same `x86BOOT.img` (root) | kernel 2043 |
| `COMMAND.COM` | FreeCOM command interpreter (English) | FDOS/freecom release `com086`, `command.zip` → `BIN/COMMAND.COM` | FreeCOM 0.86 |

Downloads:
- https://www.ibiblio.org/pub/micro/pc-stuff/freedos/files/distributions/1.3/official/FD13-FloppyEdition.zip
- https://github.com/FDOS/freecom/releases/tag/com086

Source code (as required by the GPL) is available from the FreeDOS project:
- Kernel: https://github.com/FDOS/kernel
- FreeCOM: https://github.com/FDOS/freecom (`freecom-source.zip` on the release)

FreeDOS is a trademark of the FreeDOS project. This project is not affiliated
with FreeDOS.
