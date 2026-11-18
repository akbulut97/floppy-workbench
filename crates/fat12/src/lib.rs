//! `fat12` — FAT12 floppy disk image formatter.
//!
//! Core, platform-independent library for creating valid FAT12 floppy images
//! (720K / 1.2M / 1.44M) as used by USB floppy emulators.
//!
//! Geometry source of truth: `TEKNIK_RAPOR.md` §4.1.

mod format;
pub use format::{
    add_file, format_image, format_image_with, make_bootable, remove_file, set_label, FormatOptions,
};

/// Errors produced by the `fat12` library.
#[derive(Debug)]
pub enum Fat12Error {
    /// Boot code does not fit in the boot sector (max is `bytes_per_sector - 0x40`).
    BootCodeTooLarge { max: usize, got: usize },
    /// An underlying filesystem/IO error.
    Io(std::io::Error),
}

impl std::fmt::Display for Fat12Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Fat12Error::BootCodeTooLarge { max, got } => {
                write!(f, "boot code too large: {got} bytes, max {max}")
            }
            Fat12Error::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

impl std::error::Error for Fat12Error {}

impl From<std::io::Error> for Fat12Error {
    fn from(e: std::io::Error) -> Self {
        Fat12Error::Io(e)
    }
}

/// A supported floppy disk format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloppyFormat {
    /// 3.5" DD, 720 KB.
    F720K,
    /// 5.25" HD, 1.2 MB.
    F1_2M,
    /// 3.5" HD, 1.44 MB.
    F1_44M,
}

/// Physical/logical geometry and BIOS Parameter Block (BPB) values for a format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub reserved_sectors: u16,
    pub num_fats: u8,
    pub root_entries: u16,
    pub total_sectors: u16,
    pub media_descriptor: u8,
    pub sectors_per_fat: u16,
    pub sectors_per_track: u16,
    pub num_heads: u16,
}

impl Geometry {
    /// Returns the standard geometry for the given floppy format.
    pub fn for_format(format: FloppyFormat) -> Self {
        match format {
            FloppyFormat::F720K => Geometry {
                bytes_per_sector: 512,
                sectors_per_cluster: 2,
                reserved_sectors: 1,
                num_fats: 2,
                root_entries: 112,
                total_sectors: 1440,
                media_descriptor: 0xF9,
                sectors_per_fat: 3,
                sectors_per_track: 9,
                num_heads: 2,
            },
            FloppyFormat::F1_2M => Geometry {
                bytes_per_sector: 512,
                sectors_per_cluster: 1,
                reserved_sectors: 1,
                num_fats: 2,
                root_entries: 224,
                total_sectors: 2400,
                media_descriptor: 0xF9,
                sectors_per_fat: 7,
                sectors_per_track: 15,
                num_heads: 2,
            },
            FloppyFormat::F1_44M => Geometry {
                bytes_per_sector: 512,
                sectors_per_cluster: 1,
                reserved_sectors: 1,
                num_fats: 2,
                root_entries: 224,
                total_sectors: 2880,
                media_descriptor: 0xF0,
                sectors_per_fat: 9,
                sectors_per_track: 18,
                num_heads: 2,
            },
        }
    }

    /// Total image size in bytes (`total_sectors * bytes_per_sector`).
    pub fn image_size(&self) -> usize {
        self.total_sectors as usize * self.bytes_per_sector as usize
    }
}
