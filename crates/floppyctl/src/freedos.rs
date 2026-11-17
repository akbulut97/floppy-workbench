//! Build a FreeDOS-bootable 1.44MB floppy image from bundled FreeDOS files.
//!
//! The embedded files are GPL FreeDOS components — see `assets/freedos/README.md`.

use fat12::{make_bootable, Fat12Error, FloppyFormat};

/// FreeDOS FAT12 boot sector (512 bytes); the boot code is at `0x3E..0x1FE`.
const BOOT_SECTOR: &[u8] = include_bytes!("../assets/freedos/BOOT.BIN");
const KERNEL: &[u8] = include_bytes!("../assets/freedos/KERNEL.SYS");
const COMMAND: &[u8] = include_bytes!("../assets/freedos/COMMAND.COM");

/// Build a 1.44MB floppy image that boots FreeDOS to a `A:\>` prompt.
///
/// Our standard 1.44MB BPB is kept; only the FreeDOS boot *code* and the system
/// files (`KERNEL.SYS`, `COMMAND.COM`) are added — exactly what the DOS `SYS`
/// command does.
pub fn make_bootable_image(label: &str, serial: u32) -> Result<Vec<u8>, Fat12Error> {
    let boot_code = &BOOT_SECTOR[0x3E..0x1FE];
    make_bootable(
        FloppyFormat::F1_44M,
        label,
        serial,
        boot_code,
        &[("KERNEL.SYS", KERNEL), ("COMMAND.COM", COMMAND)],
    )
}
