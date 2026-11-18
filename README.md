# Floppy Workbench

**English** · [Türkçe](README.tr.md)

A cross-platform desktop app and CLI for managing **USB floppy‑disk emulators** of the
**SFR1M44‑U100 / Alfais 4408** family (sold under many names). It formats a plain USB stick
into the raw layout these devices expect, and lets you browse, edit, and manage the stored
floppies through a file‑explorer UI.

> Those industrial emulators plug into a machine's 34‑pin floppy connector and read a USB
> stick that has been split into up to 100 fixed‑size **floppy "banks"**, each a full 1.44 MB
> FAT12 floppy image. Floppy Workbench creates and manages that layout.

Built with **Rust** (core + CLI) and **Tauri 2** (GUI). The FAT12 formatter is validated
against the independent [`fatfs`](https://crates.io/crates/fatfs) crate.

---

## Features

### Format
- Split a USB stick into **N** evenly‑sized floppy banks (1.44 MB / 1.2 MB / 720 KB, FAT12).
- Write directly to the device, or build a raw `usb.img` to flash with Rufus / `dd`.
- Per‑block quick‑format and volume‑label editing.

### Contents — a real file explorer
- Two‑pane layout: the blocks on the left, the selected floppy's files on the right.
- **Navigate folders** with a breadcrumb; enter / go up.
- **Add files** (drag‑and‑drop or a native file dialog), **New folder**.
- **Cut / Copy / Paste** across folders and across blocks (recursive for folders).
- **Rename**, **Delete** (recursive), **Export** to your PC (native "Save as…" dialog).
- **Preview** text and image files in‑app (double‑click).
- A **details panel** for the selected item.

### Advanced
- Build a multi‑bank USB image file, write an existing image, or format a single floppy image.

### CLI (`floppyctl`)
```sh
floppyctl format   --size 1.44 --label MYDISK --out disk.img [--add FILE]
floppyctl info     disk.img
floppyctl build-usb --out usb.img --banks 100 --bank 0:1.44:BOOTDISK --add-to 0:readme.txt
floppyctl list-usb
floppyctl read-bank --drive 3 --index 0        # or --image usb.img --index 0
floppyctl write-usb --image usb.img --index <IDX> --yes   # destructive
```

---

## Device layout (SFR1M44‑U100)

Reverse‑engineered and reproduced by this tool:

- The USB is a flat sequence of **1.5 MiB banks**; bank *N* starts at offset `N × 0x180000`.
- The first **1,474,560 bytes** of each bank is a standard 1.44 MB FAT12 floppy; the rest is
  zero padding.
- There is **no partition table** — the device selects a bank by index (front‑panel display).
- U100 = 100 banks; U100K = 1000 banks. Windows mounts only bank 000 (offset 0).

See [`TECHNICAL_REPORT.md`](TECHNICAL_REPORT.md) for the full technical report ([Turkish](TEKNIK_RAPOR.md)).

---

## Requirements

- **Windows** (the GUI and direct device I/O are Windows‑only for now).
- **Administrator** privileges to read/write the raw USB device.
- **WebView2** runtime (preinstalled on Windows 11).
- To build: a recent **Rust** toolchain and **Node** (only the Tauri CLI prerequisites; the
  frontend itself is dependency‑free vanilla HTML).

## Build & run

```sh
# core tests
cargo test

# CLI
cargo run -p floppyctl -- info disk.img

# GUI (run the built exe as Administrator for device access)
cargo build --release -p floppy-gui
# → target/release/floppy-gui.exe   (single portable file, ~6 MB)
```

---

## Project structure

```
crates/
├─ fat12/      FAT12 image formatter (geometry, boot/FAT/root, bootable, label, file ops)
├─ floppyctl/  CLI + core: SFR1M44 bank layout, raw device I/O, path‑based fs operations
└─ floppy-gui/ Tauri 2 desktop app (frameless, npm‑free vanilla frontend)
```

## Safety

Writing to a physical drive is **destructive and irreversible**. Floppy Workbench only lists
**removable** drives, re‑verifies the target device number before every write, and asks for
confirmation — the system disk can never be targeted by accident. Even so, **test on a
spare USB stick first**, and double‑check the selected drive.

## Status

The FAT12 core and device layout are covered by tests and validated against an independent
FAT implementation. Removable‑drive detection has been verified against real hardware;
**the raw write/read path has not yet been verified end‑to‑end on a physical device** — treat
the destructive operations as experimental until you confirm them with your own emulator.

## License

MIT.

## Acknowledgements

The device layout was informed by community reverse‑engineering of the SFR1M44‑U100K, and the
FAT12 output is cross‑checked with the `fatfs` crate. Not affiliated with WCH, ipcas, Gotek,
or Alfais.
