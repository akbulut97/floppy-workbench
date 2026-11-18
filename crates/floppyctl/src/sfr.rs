//! SFR1M44-U100 / U100K raw USB image layout.
//!
//! These industrial USB floppy emulators (sold under many names, e.g. "Gotek
//! SFR1M44", "Alfais 4408") do **not** read plain files from a FAT USB stick.
//! They store raw 1.44MB FAT12 floppy images in fixed-size "banks":
//!
//! - Each bank occupies [`BANK_STRIDE`] (1.5 MiB) and starts at `N * BANK_STRIDE`.
//! - The first [`FLOPPY_IMAGE_BYTES`] of a bank is a standard 1.44MB FAT12 image;
//!   the remainder is zero padding.
//! - There is no partition table; the device selects a bank by its index.
//!
//! This module builds the full raw image, which can then be written to the USB
//! device with an imaging tool (Rufus / Win32DiskImager / `dd`).

use anyhow::{bail, Result};
use fat12::FloppyFormat;

use crate::build_image;

/// Bytes reserved per bank (1.5 MiB).
pub const BANK_STRIDE: usize = 0x180000;

/// Size of the FAT12 floppy image stored at the start of each bank (1.44 MB).
pub const FLOPPY_IMAGE_BYTES: usize = 1_474_560;

/// Byte offset of bank `index` within the raw USB image.
pub fn bank_offset(index: u32) -> usize {
    index as usize * BANK_STRIDE
}

/// Check an image file before writing it into one bank. Accepts a 1.44 MB floppy
/// image, or a raw 1.5 MiB bank dump (its padding is dropped).
pub fn normalize_bank_image(data: &[u8]) -> Result<&[u8]> {
    match data.len() {
        FLOPPY_IMAGE_BYTES => Ok(data),
        // A raw dump of a whole bank: keep the floppy, drop the zero padding.
        BANK_STRIDE => Ok(&data[..FLOPPY_IMAGE_BYTES]),
        n => bail!(
            "image is {n} bytes; a block needs a 1.44 MB floppy image ({FLOPPY_IMAGE_BYTES} bytes)"
        ),
    }
}

/// Borrow the FAT12 image of bank `index` from a full raw USB image.
pub fn extract_bank(raw: &[u8], index: u32) -> Result<&[u8]> {
    let off = bank_offset(index);
    let end = off + FLOPPY_IMAGE_BYTES;
    if end > raw.len() {
        bail!(
            "bank {index} is beyond the image ({} banks total)",
            raw.len() / BANK_STRIDE
        );
    }
    Ok(&raw[off..end])
}

/// One bank to place in the image.
#[derive(Debug, Clone)]
pub struct BankSpec {
    pub index: u32,
    pub format: FloppyFormat,
    pub label: String,
    pub files: Vec<(String, Vec<u8>)>,
}

/// Build a full raw USB image of `total_banks` banks, filling in `banks`.
///
/// Unspecified banks are left zeroed (the device's self-format also leaves blank
/// banks). Returns an error if any bank index is `>= total_banks`.
pub fn build_usb_image(total_banks: u32, banks: &[BankSpec]) -> Result<Vec<u8>> {
    let mut raw = vec![0u8; total_banks as usize * BANK_STRIDE];
    for spec in banks {
        if spec.index >= total_banks {
            bail!(
                "bank index {} is out of range (total banks: {})",
                spec.index,
                total_banks
            );
        }
        let img = build_image(spec.format, &spec.label, 0, &spec.files)?;
        if img.len() > BANK_STRIDE {
            bail!(
                "bank {} image ({} bytes) exceeds bank stride ({} bytes)",
                spec.index,
                img.len(),
                BANK_STRIDE
            );
        }
        let off = bank_offset(spec.index);
        raw[off..off + img.len()].copy_from_slice(&img);
    }
    Ok(raw)
}
