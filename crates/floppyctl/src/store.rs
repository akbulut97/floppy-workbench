//! Where the banks live: a physical USB stick, or a raw USB image file on disk.
//!
//! Both implement [`BankStore`], so block operations (and the CLI) work the
//! same against a real device and an `usb.img` backup.

use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::device::{self, UsbDrive};
use crate::sfr::{bank_offset, BANK_STRIDE, FLOPPY_IMAGE_BYTES};

/// Number of banks whose floppy image fits completely in `len` bytes.
pub fn banks_in(len: u64) -> u32 {
    if len < FLOPPY_IMAGE_BYTES as u64 {
        0
    } else {
        ((len - FLOPPY_IMAGE_BYTES as u64) / BANK_STRIDE as u64 + 1) as u32
    }
}

/// Bank-level access to an SFR1M44 layout.
pub trait BankStore {
    /// How many banks the medium can hold.
    fn bank_count(&self) -> u32;
    /// Read the 1.44 MB floppy image of bank `index`.
    fn read_bank(&self, index: u32) -> Result<Vec<u8>>;
    /// Overwrite bank `index` with a 1.44 MB floppy image (verified). DESTRUCTIVE.
    fn write_bank(&self, index: u32, image: &[u8]) -> Result<()>;

    /// Read the first `count` banks as one raw buffer (bank N at `N * BANK_STRIDE`).
    fn read_banks(&self, count: u32, on_progress: &dyn Fn(u64, u64)) -> Result<Vec<u8>> {
        let count = count.min(self.bank_count());
        let mut raw = vec![0u8; count as usize * BANK_STRIDE];
        for i in 0..count {
            let img = self.read_bank(i)?;
            let off = bank_offset(i);
            raw[off..off + img.len()].copy_from_slice(&img);
            on_progress(i as u64 + 1, count as u64);
        }
        Ok(raw)
    }

    fn check_index(&self, index: u32) -> Result<()> {
        if index >= self.bank_count() {
            bail!(
                "block {index} is beyond the end ({} blocks available)",
                self.bank_count()
            );
        }
        Ok(())
    }
}

/// A physical removable drive.
pub struct DriveStore(pub UsbDrive);

impl BankStore for DriveStore {
    fn bank_count(&self) -> u32 {
        banks_in(self.0.size_bytes)
    }
    fn read_bank(&self, index: u32) -> Result<Vec<u8>> {
        self.check_index(index)?;
        device::read_bank(&self.0, index)
    }
    fn write_bank(&self, index: u32, image: &[u8]) -> Result<()> {
        self.check_index(index)?;
        device::write_bank(&self.0, index, image)
    }
    fn read_banks(&self, count: u32, on_progress: &dyn Fn(u64, u64)) -> Result<Vec<u8>> {
        // One big sequential read is much faster on a real stick.
        let count = count.min(self.bank_count());
        device::read_range(&self.0, count as usize * BANK_STRIDE, on_progress)
    }
}

/// A raw USB image file (e.g. made by `build-usb` or a backup). Writes modify
/// the file in place; its size never changes.
pub struct ImageFileStore {
    path: PathBuf,
    banks: u32,
}

impl ImageFileStore {
    pub fn open(path: &Path) -> Result<Self> {
        let len = std::fs::metadata(path)
            .with_context(|| format!("reading {}", path.display()))?
            .len();
        let banks = banks_in(len);
        if banks == 0 {
            bail!("{} is too small to hold a single block", path.display());
        }
        Ok(Self {
            path: path.to_path_buf(),
            banks,
        })
    }
}

impl BankStore for ImageFileStore {
    fn bank_count(&self) -> u32 {
        self.banks
    }
    fn read_bank(&self, index: u32) -> Result<Vec<u8>> {
        self.check_index(index)?;
        let mut f = std::fs::File::open(&self.path)?;
        f.seek(SeekFrom::Start(bank_offset(index) as u64))?;
        let mut buf = vec![0u8; FLOPPY_IMAGE_BYTES];
        f.read_exact(&mut buf)?;
        Ok(buf)
    }
    fn write_bank(&self, index: u32, image: &[u8]) -> Result<()> {
        self.check_index(index)?;
        if image.len() != FLOPPY_IMAGE_BYTES {
            bail!(
                "bank image must be exactly {FLOPPY_IMAGE_BYTES} bytes, got {}",
                image.len()
            );
        }
        let mut f = OpenOptions::new().write(true).open(&self.path)?;
        f.seek(SeekFrom::Start(bank_offset(index) as u64))?;
        f.write_all(image)?;
        f.sync_all()?;
        drop(f);
        if self.read_bank(index)? != image {
            bail!("verification failed for block {index}");
        }
        Ok(())
    }
}

/// Open either a physical drive (by index, from `list-usb`) or an image file.
pub fn open_store(drive: Option<u32>, image: Option<&Path>) -> Result<Box<dyn BankStore>> {
    match (drive, image) {
        (Some(_), Some(_)) => bail!("use either --drive or --image, not both"),
        (None, Some(path)) => Ok(Box::new(ImageFileStore::open(path)?)),
        (Some(idx), None) => {
            let d = device::list_usb_drives()?
                .into_iter()
                .find(|d| d.index == idx)
                .with_context(|| format!("no removable drive with index {idx}"))?;
            Ok(Box::new(DriveStore(d)))
        }
        (None, None) => bail!("provide either --drive <IDX> or --image <FILE>"),
    }
}
