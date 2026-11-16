//! File-based block manager: a "floppy set" is a directory (a USB drive root)
//! holding one `.img` file per emulated floppy, as used by Gotek/FlashFloppy
//! and similar file-based USB floppy emulators.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use fat12::FloppyFormat;

use crate::{build_image, inspect, ImageInfo};

/// Naming scheme for block image files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Naming {
    /// `DSKA0000.IMG` (Gotek / HxC / FlashFloppy indexed mode).
    Gotek,
    /// `FLOPPY000.IMG` (ipcas / WCH style).
    Ipcas,
}

impl Naming {
    fn prefix(&self) -> &'static str {
        match self {
            Naming::Gotek => "DSKA",
            Naming::Ipcas => "FLOPPY",
        }
    }

    fn width(&self) -> usize {
        match self {
            Naming::Gotek => 4,
            Naming::Ipcas => 3,
        }
    }

    /// File name for the given block index (e.g. `DSKA0003.IMG`).
    pub fn filename(&self, index: u32) -> String {
        format!(
            "{}{:0width$}.IMG",
            self.prefix(),
            index,
            width = self.width()
        )
    }

    /// Parse a block index out of a file name, if it matches this scheme.
    pub fn parse_index(&self, name: &str) -> Option<u32> {
        let upper = name.to_uppercase();
        let digits = upper.strip_prefix(self.prefix())?.strip_suffix(".IMG")?;
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        digits.parse().ok()
    }
}

/// One emulated floppy in a set.
#[derive(Debug, Clone)]
pub struct BlockEntry {
    pub index: u32,
    pub path: PathBuf,
    pub info: ImageInfo,
}

/// A directory of block image files.
pub struct BlockSet {
    root: PathBuf,
    naming: Naming,
}

impl BlockSet {
    pub fn new(root: impl AsRef<Path>, naming: Naming) -> Self {
        BlockSet {
            root: root.as_ref().to_path_buf(),
            naming,
        }
    }

    /// Create (or overwrite) block `index` as a freshly formatted image.
    pub fn create_block(
        &self,
        index: u32,
        format: FloppyFormat,
        label: &str,
        serial: u32,
    ) -> Result<PathBuf> {
        std::fs::create_dir_all(&self.root)
            .with_context(|| format!("creating {}", self.root.display()))?;
        let path = self.root.join(self.naming.filename(index));
        let img = build_image(format, label, serial, &[])?;
        std::fs::write(&path, &img).with_context(|| format!("writing {}", path.display()))?;
        Ok(path)
    }

    /// List all blocks in the set, sorted by index, with their info.
    pub fn list(&self) -> Result<Vec<BlockEntry>> {
        let mut entries = Vec::new();
        if !self.root.exists() {
            return Ok(entries);
        }
        for dir_entry in std::fs::read_dir(&self.root)
            .with_context(|| format!("reading {}", self.root.display()))?
        {
            let dir_entry = dir_entry?;
            let name = dir_entry.file_name();
            let name = name.to_string_lossy();
            if let Some(index) = self.naming.parse_index(&name) {
                let path = dir_entry.path();
                let data = std::fs::read(&path)?;
                let info = inspect(&data)
                    .with_context(|| format!("inspecting {}", path.display()))?;
                entries.push(BlockEntry { index, path, info });
            }
        }
        entries.sort_by_key(|e| e.index);
        Ok(entries)
    }
}
