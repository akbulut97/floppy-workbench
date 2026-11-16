//! FAT12 image construction and file-level operations.

use std::io::{Cursor, Write};

use fatfs::{FileSystem, FsOptions};

use crate::{Fat12Error, FloppyFormat, Geometry};

/// Offset where boot code begins, immediately after the extended BPB.
const BOOT_CODE_OFFSET: usize = 0x3E;

/// Options controlling how an image is formatted.
#[derive(Debug, Clone, Copy, Default)]
pub struct FormatOptions<'a> {
    /// Volume label (truncated to 11 bytes).
    pub label: &'a str,
    /// Volume serial number.
    pub serial: u32,
    /// Optional boot-sector code (x86), placed at offset 0x3E. Anything beyond
    /// `bytes_per_sector - 0x40` is truncated; use [`make_bootable`] to get an
    /// error instead.
    pub boot_code: Option<&'a [u8]>,
}

/// Pad/truncate a volume label to the 11-byte, space-padded FAT field.
fn label_field(label: &str) -> [u8; 11] {
    let mut field = [b' '; 11];
    for (i, b) in label.bytes().take(11).enumerate() {
        field[i] = b;
    }
    field
}

/// Build a fully formatted, empty FAT12 image using the default options.
///
/// Convenience wrapper over [`format_image_with`].
pub fn format_image(format: FloppyFormat, label: &str, serial: u32) -> Vec<u8> {
    format_image_with(
        format,
        &FormatOptions {
            label,
            serial,
            boot_code: None,
        },
    )
}

/// Build a fully formatted, empty FAT12 image for `format`.
pub fn format_image_with(format: FloppyFormat, opts: &FormatOptions) -> Vec<u8> {
    let g = Geometry::for_format(format);
    let mut img = vec![0u8; g.image_size()];

    write_boot_sector(&mut img, &g, opts);
    write_fats(&mut img, &g);
    write_root_volume_label(&mut img, &g, opts.label);

    img
}

/// Add a file to an existing image at `path` (e.g. `"README.TXT"` or `"SUB/F.TXT"`).
///
/// Uses the `fatfs` driver for allocation; the image must already be formatted.
pub fn add_file(img: &mut [u8], path: &str, data: &[u8]) -> Result<(), Fat12Error> {
    let fs = FileSystem::new(Cursor::new(&mut *img), FsOptions::new())?;
    {
        let file = fs.root_dir();
        let mut f = file.create_file(path)?;
        f.truncate()?;
        f.write_all(data)?;
        f.flush()?;
    }
    fs.unmount()?;
    Ok(())
}

/// Change the volume label of an existing FAT12 image.
///
/// Updates the boot sector label field and the root-directory volume-id entry
/// (creating one in a free slot if none exists).
pub fn set_label(img: &mut [u8], label: &str) -> Result<(), Fat12Error> {
    if img.len() < 512 || img[510] != 0x55 || img[511] != 0xAA || &img[0x36..0x3E] != b"FAT12   " {
        return Err(Fat12Error::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "not a FAT12 image",
        )));
    }
    let field = label_field(label);

    // Boot sector label (offset 0x2B).
    img[0x2B..0x36].copy_from_slice(&field);

    // Locate the root directory from the BPB.
    let bps = u16::from_le_bytes([img[0x0B], img[0x0C]]) as usize;
    let reserved = u16::from_le_bytes([img[0x0E], img[0x0F]]) as usize;
    let num_fats = img[0x10] as usize;
    let spf = u16::from_le_bytes([img[0x16], img[0x17]]) as usize;
    let root_entries = u16::from_le_bytes([img[0x11], img[0x12]]) as usize;
    let root = (reserved + num_fats * spf) * bps;

    let mut free: Option<usize> = None;
    for i in 0..root_entries {
        let off = root + i * 32;
        let first = img[off];
        let attr = img[off + 11];
        if attr == 0x08 && first != 0x00 && first != 0xE5 {
            // Existing volume-id entry: update its name.
            img[off..off + 11].copy_from_slice(&field);
            return Ok(());
        }
        if free.is_none() && (first == 0x00 || first == 0xE5) {
            free = Some(off);
        }
    }
    if let Some(off) = free {
        img[off..off + 11].copy_from_slice(&field);
        img[off + 11] = 0x08; // ATTR_VOLUME_ID
    }
    Ok(())
}

/// Remove a file (or empty directory) at `path` from an existing image.
pub fn remove_file(img: &mut [u8], path: &str) -> Result<(), Fat12Error> {
    let fs = FileSystem::new(Cursor::new(&mut *img), FsOptions::new())?;
    fs.root_dir().remove(path)?;
    fs.unmount()?;
    Ok(())
}

/// Format a bootable FAT12 image: inject `boot_code` into the boot sector and
/// place `system_files` (e.g. FreeDOS `KERNEL.SYS` + `COMMAND.COM`) in the root.
pub fn make_bootable(
    format: FloppyFormat,
    label: &str,
    serial: u32,
    boot_code: &[u8],
    system_files: &[(&str, &[u8])],
) -> Result<Vec<u8>, Fat12Error> {
    let max = Geometry::for_format(format).bytes_per_sector as usize - BOOT_CODE_OFFSET - 2;
    if boot_code.len() > max {
        return Err(Fat12Error::BootCodeTooLarge {
            max,
            got: boot_code.len(),
        });
    }
    let opts = FormatOptions {
        label,
        serial,
        boot_code: Some(boot_code),
    };
    let mut img = format_image_with(format, &opts);
    for (name, data) in system_files {
        add_file(&mut img, name, data)?;
    }
    Ok(img)
}

fn write_boot_sector(img: &mut [u8], g: &Geometry, opts: &FormatOptions) {
    // 0x00: jump + NOP
    img[0x00..0x03].copy_from_slice(&[0xEB, 0x3C, 0x90]);
    // 0x03: OEM name (8 bytes)
    img[0x03..0x0B].copy_from_slice(b"MSDOS5.0");
    // --- BPB ---
    img[0x0B..0x0D].copy_from_slice(&g.bytes_per_sector.to_le_bytes());
    img[0x0D] = g.sectors_per_cluster;
    img[0x0E..0x10].copy_from_slice(&g.reserved_sectors.to_le_bytes());
    img[0x10] = g.num_fats;
    img[0x11..0x13].copy_from_slice(&g.root_entries.to_le_bytes());
    img[0x13..0x15].copy_from_slice(&g.total_sectors.to_le_bytes());
    img[0x15] = g.media_descriptor;
    img[0x16..0x18].copy_from_slice(&g.sectors_per_fat.to_le_bytes());
    img[0x18..0x1A].copy_from_slice(&g.sectors_per_track.to_le_bytes());
    img[0x1A..0x1C].copy_from_slice(&g.num_heads.to_le_bytes());
    img[0x1C..0x20].copy_from_slice(&0u32.to_le_bytes()); // hidden sectors
    img[0x20..0x24].copy_from_slice(&0u32.to_le_bytes()); // total sectors (32-bit)
    // --- Extended BPB (FAT12/16) ---
    img[0x24] = 0x00; // drive number (floppy)
    img[0x25] = 0x00; // reserved
    img[0x26] = 0x29; // extended boot signature
    img[0x27..0x2B].copy_from_slice(&opts.serial.to_le_bytes());
    img[0x2B..0x36].copy_from_slice(&label_field(opts.label));
    img[0x36..0x3E].copy_from_slice(b"FAT12   ");
    // 0x3E: boot code (optional)
    if let Some(code) = opts.boot_code {
        let max = g.bytes_per_sector as usize - BOOT_CODE_OFFSET - 2;
        let n = code.len().min(max);
        img[BOOT_CODE_OFFSET..BOOT_CODE_OFFSET + n].copy_from_slice(&code[..n]);
    }
    // 0x1FE: boot sector signature
    img[510] = 0x55;
    img[511] = 0xAA;
}

/// Write the volume label as the first root-directory entry (ATTR_VOLUME_ID).
fn write_root_volume_label(img: &mut [u8], g: &Geometry, label: &str) {
    let bps = g.bytes_per_sector as usize;
    let root = (g.reserved_sectors as usize + g.num_fats as usize * g.sectors_per_fat as usize) * bps;
    img[root..root + 11].copy_from_slice(&label_field(label));
    img[root + 11] = 0x08; // ATTR_VOLUME_ID
}

fn write_fats(img: &mut [u8], g: &Geometry) {
    let bps = g.bytes_per_sector as usize;
    let fat_bytes = g.sectors_per_fat as usize * bps;
    let first_fat = g.reserved_sectors as usize * bps;
    for i in 0..g.num_fats as usize {
        let base = first_fat + i * fat_bytes;
        // FAT[0] = media descriptor in low byte, rest 0xFF; FAT[1] = EOC (0xFFF).
        img[base] = g.media_descriptor;
        img[base + 1] = 0xFF;
        img[base + 2] = 0xFF;
    }
}
