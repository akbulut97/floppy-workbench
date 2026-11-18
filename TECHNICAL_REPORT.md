# USB Floppy Emulator Manager — Technical Analysis and Development Report

**English** · [Türkçe](TEKNIK_RAPOR.md)

**Reference application:** ipcas GmbH *USB Floppy Emulator V2 (FDEv2)* v1.40i
**Actual engine:** WCH / WinChipHead "SFD UFloppyManager II" (rebranded)
**Report date:** 2026-11-16
**Purpose:** Reference / specification for developing a similar USB floppy emulator management application

> Note: The findings about the "reference application" in this report are based on **static analysis** (file layout, ASCII/Unicode strings extracted from the binary, resource names, registry/file layout). The application was not run to verify behavior; items marked "needs verification" must be confirmed at runtime.

---

## 1. Purpose and Scope

The application manages the USB stick inside a **USB floppy emulator** (Gotek/HxC-class hardware or a WCH-based USB floppy device). Core functions:

- Split the USB stick into many "blocks", each **emulating an independent floppy**,
- Format these blocks as FAT12 floppies,
- List/display the blocks, transfer content (upload/download), write/save images, back up.

The target machine (e.g. an old CNC, textile machine, medical device, synthesizer) sees **one block at a time** through the emulator as a physical 3.5" floppy (A:).

---

## 2. Reference Application Analysis

### 2.1 Components

| File | Role |
|---|---|
| `USB_Floppy_Manager_v140i.exe` | GUI (PE32, i386, MFC-based, ~590 KB) |
| `QHSFDSvc.exe` | Windows service: `WCHSFDService` ("floppy disk block service") |
| `psvince.dll` | Installer/copy-protection helper (running-process check) |
| `FDEv2_*.pdf` | User manuals (DE/EN) |

### 2.2 Architecture

- **Two layers:** GUI ↔ core service.
- The service requires **Administrator** rights and **Windows 2000+** (`Un-support NT system, run above of Win2000`). The service is installed/uninstalled from within the application.
- **IPC (inter-process communication):** shared memory + synchronization objects:
  - `Global\WCHSFDMappingObject` (memory-mapped file — data),
  - `Global\WCHSFDRequestObject`, `Global\WCHSFDResponseObject` (request/response events).
- **Settings storage:** registry `Software\WinChipHead\ufdd`
  - `LastFormatType`, `LastFddFormatCout`, `QuickFormat`, `EnableBootDisk`, `BootDiskDOSVer`, `BackupPath`.
- **On-disk layout of the USB:** block directory pattern `%s\%c\FLOPPY%03d` → e.g. `DISK\F\FLOPPY000`
  - `%c` = drive letter the block is mounted on, `%03d` = block number (000..099).

### 2.3 Feature list extracted from the binary

**Format / partitioning**
- `Format USB Flash Drive` (Ctrl+F) — erases the whole USB and creates N blocks. Dialog: `format_dlg`.
- Block format — dialog: `format_fdd_dlg`, `Select floppy disk format`.
  - Single: `&Format...` / *Format current floppy disk block*
  - Quick: `&Quick format` / `QuickFormat`
  - Bulk: `Bulk &format...` / *Format several floppy disk blocks which are selected*
- Capacity types: **720K**, **1.2M**, **1.44M**
- File system: **FAT12 only** (`Error: Not FAT12 file system`; the image must be `< 1.5M`)
- Bootable DOS floppy: `EnableBootDisk`, `BootDiskDOSVer`, **DOS 6.22 / DOS 7.1**, writes `MSDOS.SYS`
- Block count range: **1–100** (`the number of floppy disk block (1 - 100)`)
- Warnings: `Alarm: Format will delete all the data!!!`, `Ensure to format?`

**View / listing**
- Views: Large icons / Small icons / List / **Details**
- Detail columns: `Serial number | Capability | Used | Renew time`
- `Enumerate floppy disk block: %d`, Refresh / Reread, device serial number display
- Capacity format: `%0.1f KB` / `%0.2f MB`
- Single/multi selection, "select all"

**Data operations**
- Write image / save block → image (`*.img`), bulk write/save
- Upload/Download block (PC folder ↔ block)
- Back up the whole USB (Ctrl+B → `\DISKBAK`, `BackupPath`)
- Options: open Explorer after opening, auto-clear the upload folder, auto-rename, overwrite the original

**Menu / shortcut map**
`File`, `USB Flash Drive`, `Option`, `Help` — Ctrl+F (format USB), Ctrl+B (backup), Ctrl+D (upload folder), Ctrl+Q (settings), Ctrl+A (select all), F1 (help).

---

## 3. Data Model

### 3.1 The "floppy disk block" (block / slot) concept

The USB stick is logically divided into **fixed-size blocks**. Each block = a complete floppy image (a raw sector array). Proposed model:

```
USB STICK
├─ Management/header area (optional: block table, metadata)
├─ BLOCK 0  → full floppy image (e.g. 1.44MB = 2880 × 512B)
├─ BLOCK 1  → full floppy image
├─ ...
└─ BLOCK N-1
```

Metadata suggested for each block:

| Field | Description |
|---|---|
| index | Block number (0..N-1) |
| geometry | 720K / 1.2M / 1.44M (sectors/track, tracks, heads) |
| serial | Volume serial number |
| capacity | Total capacity (bytes) |
| used | Used space (computed from the FAT) |
| label | Volume label |
| mtime | Last modified ("Renew time") |
| bootable | Whether it is DOS-bootable |

> **Hardware VERIFIED (2026-11-16): Alfais 4408 = SFR1M44-U100 family.**
> This is the industrial emulator managed by the ipcas/WCH "USB Floppy Manager v1.40i" software (**not** a Gotek/FlashFloppy that reads plain files). Exact raw USB layout:
>
> - The USB is divided into consecutive **1.5 MiB (0x180000-byte) "banks"**; there is no partition table.
> - Bank N offset = **N × 0x180000**.
> - The first **1,474,560 bytes** of each bank = a standard 1.44MB FAT12 floppy image; the remaining ~96 KB is zero padding.
> - U100 = 100 banks (the first 150 MiB are used), U100K = 1000 banks (1500 MiB).
> - Windows only sees bank 000 (offset 0) as a drive; the others live at raw offsets.
> - The active bank is chosen with the 3-digit display/buttons on the device's front panel.
>
> **Conclusion:** The file-based `BlockSet` model is INVALID for this device. The correct model is in `crates/floppyctl/src/sfr.rs`: build a raw image of the whole USB (banks at the correct offsets) and write it to the USB with Rufus/Win32DiskImager/dd. (Source: device reverse-engineering review — goughlui.com SFR1M44-U100K review.)

### 3.2 Access path options

1. **Raw sector access** (what the reference application does): raw read/write of the USB via `\\.\PhysicalDriveN`. Each block's offset = `index × block_size`. Requires admin + a kernel service.
2. **File-based** (e.g. Gotek/FlashFloppy): each floppy sits as a `.img`/`.hfe` **file** on a FAT32 USB stick. Much simpler, needs no admin. (Modern approach — recommended.)

Which one fits depends on the target emulator's firmware.

---

## 4. Floppy Format Technical Details (FAT12)

All the geometry and structure needed to write a formatter.

### 4.1 Floppy geometries

| Format | Size | Bytes/sec | Sec/track | Heads | Tracks | Total sec | Media | Sec/FAT | Root entries | Sec/cluster |
|---|---|---|---|---|---|---|---|---|---|---|
| 720K (3.5" DD) | 737,280 | 512 | 9 | 2 | 80 | 1440 | 0xF9 | 3 | 112 | 2 |
| 1.2M (5.25" HD) | 1,228,800 | 512 | 15 | 2 | 80 | 2400 | 0xF9 | 7 | 224 | 1 |
| 1.44M (3.5" HD) | 1,474,560 | 512 | 18 | 2 | 80 | 2880 | 0xF0 | 9 | 224 | 1 |

(For completeness: 2.88M → 36 sec/track, 5760 total, media 0xF0, 9 sec/FAT, 2 sec/cluster. Not present in the reference application.)

### 4.2 Logical floppy layout (1.44MB example)

```
Sector 0            : Boot sector (contains the BPB)
Sectors 1..9        : FAT #1   (9 sectors)
Sectors 10..18      : FAT #2   (9 sectors, copy)
Sectors 19..32      : Root directory (224 entries × 32B = 7168B = 14 sectors)
Sectors 33..2879    : Data area (starts at cluster 2)
```

### 4.3 Boot sector / BPB fields (offsets)

| Offset | Size | Field | 1.44M value |
|---|---|---|---|
| 0x00 | 3 | Jump | `EB 3C 90` |
| 0x03 | 8 | OEM name | e.g. `"MSDOS5.0"` |
| 0x0B | 2 | Bytes/sector | 512 |
| 0x0D | 1 | Sectors/cluster | 1 |
| 0x0E | 2 | Reserved sectors | 1 |
| 0x10 | 1 | Number of FATs | 2 |
| 0x11 | 2 | Root entries | 224 |
| 0x13 | 2 | Total sectors (16-bit) | 2880 |
| 0x15 | 1 | Media descriptor | 0xF0 |
| 0x16 | 2 | Sectors/FAT | 9 |
| 0x18 | 2 | Sectors/track | 18 |
| 0x1A | 2 | Number of heads | 2 |
| 0x1C | 4 | Hidden sectors | 0 |
| 0x20 | 4 | Total sectors (32-bit) | 0 |
| 0x24 | 1 | Drive number | 0x00 |
| 0x26 | 1 | Boot signature | 0x29 |
| 0x27 | 4 | Volume serial | random |
| 0x2B | 11 | Volume label | `"NO NAME    "` |
| 0x36 | 8 | FS type | `"FAT12   "` |
| 0x1FE | 2 | Signature | `0x55 0xAA` |

### 4.4 FAT12 initialization

- The first two FAT entries are reserved: FAT[0] = media byte + 0xF00 (e.g. `F0 FF FF`), FAT[1] = EOC.
- Each FAT12 entry is 12 bits → 2 entries are packed into 3 bytes.
- For an empty disk, the rest of the FAT is 0x00; the root directory and data area are 0x00.

### 4.5 Quick format vs full format

- **Quick format:** only the boot sector + FATs + root directory are cleared and written (the data area is untouched). Fast.
- **Full format:** all sectors are written with 0x00 (or 0xF6), then the structure is built. (Optional "verify".)

### 4.6 Bootable DOS floppy

- Real boot code is written to the boot sector (not just the BPB).
- System files in the root directory: `IO.SYS`, `MSDOS.SYS` (DOS 6.22), or a single `IO.SYS`/`COMMAND.COM` layout; for FreeDOS, `KERNEL.SYS` + `COMMAND.COM`.
- DOS 6.22 and DOS 7.1 (Win9x) boot code/BPB details differ → a template per version.
- **Legal note:** MS-DOS boot files are copyrighted; for a redistributable application, using **FreeDOS** is recommended.

---

## 5. Partition / Block Display

Suggested list-view columns (compatible with the reference):

```
# | Serial No | Capacity | Used | % Full | Label | Format | Last Modified | Boot
```

- Capacity/usage is computed by scanning the FAT: free cluster count × cluster size.
- View modes: Large icons / Small icons / List / Details.
- Refresh: re-enumerate the device + read each block's BPB and FAT summary.
- Distinguish empty/full/corrupt blocks with color/icons (UX improvement).

---

## 6. Functional Requirements (summary list)

Status as of the current Floppy Workbench release:

**Device**
- [x] Detect and select the USB floppy emulator device (removable drives only; re-scans automatically on plug/unplug)
- [x] Count/list blocks (1–100), read metadata (label, used/free space)
- [x] View modes + refresh: list and grid views (the grid shows usage), instead of the original 4 modes
- [x] Search file names across all blocks; integrity check of every block (extra)

**Format**
- [x] Split the USB into N blocks (full format)
- [x] Single-block format (720K / 1.2M / 1.44M, FAT12)
- [x] Quick format
- [x] Bulk (multi-select) format: Ctrl/Shift-click blocks, or format a range
- [x] Create bootable DOS floppies (FreeDOS)
- [x] Confirmation dialog for destructive operations
- [x] Remember the last format setting (plus device, tab and language)

**Data**
- [x] Upload files to a block / download files from a block (full file explorer: folders, cut/copy/paste, rename, preview)
- [x] Write a `.img` to a block / save a block as `.img`
- [x] Copy a block onto another block (extra)
- [x] Bulk write/save: load many `.img` files into consecutive blocks; save the selected blocks as `BLOCKnnn.img`
- [x] Back up / restore the whole USB (or its first N banks)
- [x] Read-back verification after every write (extra)

**System**
- [x] Admin elevation: the Windows release build embeds a `requireAdministrator` manifest; Linux/macOS run as root
- [x] Settings persistence (stored by the app, no registry)
- [x] Windows, Linux and macOS builds; installers via the release workflow (unsigned)

---

## 7. Technical Constraints and Risks

| Topic | Note |
|---|---|
| Raw disk access | Writing `\\.\PhysicalDriveN` needs admin; writing to the wrong drive risks **data loss** → device verification is mandatory |
| Hardware protocol | Block table / active-block selection is firmware-specific — needs reverse engineering or vendor documentation (needs verification) |
| FAT12 limit | Image < 1.5 MB; FAT12 only |
| DOS boot copyright | FreeDOS instead of MS-DOS |
| Operating system | Kernel service on Windows; the modern equivalent requires driver signing on Win10/11 |
| Concurrency | Write conflicts while the device is busy (target machine reading) |

---

## 8. Development Recommendations / Roadmap

**Technology decision (chosen): Rust + Tauri**
- **Core (`fat12` crate):** byte-exact FAT12/disk operations in Rust. The BPB is modeled one-to-one with `#[repr(C, packed)]` structs; independently verified with the `fatfs` crate (round-trip tests).
- **GUI:** Tauri (a single, small native binary; Windows/Linux/macOS). Core logic is separate from the GUI — it can also be used as a CLI.
- **Raw device access:** `\\.\PhysicalDrive` (Win) / `/dev/sd*` (Linux) / `/dev/disk*` (macOS), isolated with `#[cfg(target_os)]`.
- **Environment (verified on this machine):** rustc/cargo 1.99.1, Node 26.12 (for Tauri).

**Recommended order**
1. Determine the target hardware's access model (file-based or raw blocks). *Everything rests on this.*
2. Write an independent **FAT12 image library** (create/format/read/write, boot sector + FAT + root). With unit tests; use the Section 4 tables as the reference.
3. Verify the image on a real floppy drive/emulator (round-trip: create → read on the target machine).
4. Block management layer (listing + metadata).
5. GUI (list/detail views, format dialogs).
6. Data transfer (upload/download, image import/export), backup.
7. Bootable disks + bulk operations.

**First validation test (minimum)**
Produce an empty 1.44MB FAT12 image → mount it on Windows as a loop device/virtual floppy → confirm the format and file write/read. This proves the core of the formatter.

---

## 9. Appendices

- Extracted raw string files (in this session's scratchpad): `exe_strings.txt` (ASCII), `exe_unicode.txt` (UTF-16).
- Reference manuals: `FDEv2_20120711en.pdf`, `FDEv2 Readme.pdf` (in the application's install folder).
