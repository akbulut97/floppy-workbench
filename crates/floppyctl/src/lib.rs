//! Core logic for the `floppyctl` CLI, kept separate from argument parsing and
//! file IO so it can be unit-tested.

pub mod blockset;
pub mod device;
pub mod fsops;
pub mod sfr;

use std::io::Cursor;

use anyhow::{bail, Result};
use fat12::{add_file, format_image, FloppyFormat};
use fatfs::{FileSystem, FsOptions};
use serde::{Deserialize, Serialize};

/// One directory entry inside a bank's FAT12 filesystem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankFile {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
}

/// Parse the volume label from a FAT12 boot sector (first 512 bytes), returning
/// `None` if the sector is not a valid FAT12 boot sector (e.g. a blank bank).
pub fn boot_label(sector: &[u8]) -> Option<String> {
    if sector.len() < 512 {
        return None;
    }
    if sector[510] != 0x55 || sector[511] != 0xAA {
        return None;
    }
    if &sector[0x36..0x3E] != b"FAT12   " {
        return None;
    }
    Some(
        String::from_utf8_lossy(&sector[0x2B..0x36])
            .trim_end()
            .to_string(),
    )
}

/// List the files/directories in the root of a FAT12 image.
pub fn list_fat_files(img: &[u8]) -> Result<Vec<BankFile>> {
    let fs = FileSystem::new(Cursor::new(img.to_vec()), FsOptions::new())?;
    let mut out = Vec::new();
    for entry in fs.root_dir().iter() {
        let e = entry?;
        let name = e.file_name();
        if name == "." || name == ".." {
            continue;
        }
        out.push(BankFile {
            name,
            size: e.len(),
            is_dir: e.is_dir(),
        });
    }
    Ok(out)
}

/// Summary information about a formatted floppy image (for listing/inspection).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageInfo {
    pub label: String,
    pub fat_type: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
}

/// Read capacity/usage/label from a formatted image.
pub fn inspect(img: &[u8]) -> Result<ImageInfo> {
    // fatfs requires a writable backing even for read-only inspection.
    let fs = FileSystem::new(Cursor::new(img.to_vec()), FsOptions::new())?;
    let stats = fs.stats()?;
    let cluster = stats.cluster_size() as u64;
    let total = stats.total_clusters() as u64 * cluster;
    let free = stats.free_clusters() as u64 * cluster;
    Ok(ImageInfo {
        label: fs.volume_label().trim_end().to_string(),
        fat_type: format!("{:?}", fs.fat_type()).to_uppercase(),
        total_bytes: total,
        used_bytes: total - free,
        free_bytes: free,
    })
}

/// Parse a human size string (`"720"`, `"1.2"`, `"1.44"`) into a [`FloppyFormat`].
pub fn parse_format(s: &str) -> Result<FloppyFormat> {
    Ok(match s.trim() {
        "720" | "720k" | "720K" => FloppyFormat::F720K,
        "1.2" | "1.2m" | "1.2M" => FloppyFormat::F1_2M,
        "1.44" | "1.44m" | "1.44M" => FloppyFormat::F1_44M,
        other => bail!("unknown floppy size '{other}' (expected 720, 1.2 or 1.44)"),
    })
}

/// Parse a naming-scheme string (`"gotek"` or `"ipcas"`).
pub fn parse_naming(s: &str) -> Result<blockset::Naming> {
    Ok(match s.trim().to_lowercase().as_str() {
        "gotek" | "hxc" | "flashfloppy" => blockset::Naming::Gotek,
        "ipcas" | "wch" => blockset::Naming::Ipcas,
        other => bail!("unknown naming scheme '{other}' (expected gotek or ipcas)"),
    })
}

/// Parse a bank spec string `"<index>:<size>:<label>"` (e.g. `"3:1.44:MYDISK"`).
pub fn parse_bank_spec(s: &str) -> Result<sfr::BankSpec> {
    let mut parts = s.splitn(3, ':');
    let index = match parts.next() {
        Some(i) => i
            .trim()
            .parse::<u32>()
            .map_err(|_| anyhow::anyhow!("invalid bank index in '{s}'"))?,
        None => bail!("empty bank spec"),
    };
    let size = parts.next().unwrap_or("1.44");
    let format = parse_format(size)?;
    let label = parts.next().unwrap_or("NO NAME").to_string();
    Ok(sfr::BankSpec {
        index,
        format,
        label,
        files: Vec::new(),
    })
}

/// Parse an `--add-to` spec `"<index>:<path>"`. Splits on the *first* colon so
/// Windows paths (e.g. `C:\dir\file`) are preserved.
pub fn parse_add_spec(s: &str) -> Result<(u32, String)> {
    let (idx, path) = s
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("expected '<index>:<path>', got '{s}'"))?;
    let index = idx
        .trim()
        .parse::<u32>()
        .map_err(|_| anyhow::anyhow!("invalid bank index in '{s}'"))?;
    if path.is_empty() {
        bail!("empty path in '{s}'");
    }
    Ok((index, path.to_string()))
}

/// Attach a file to the bank with `index` among `specs`.
pub fn attach_file(
    specs: &mut [sfr::BankSpec],
    index: u32,
    name: String,
    data: Vec<u8>,
) -> Result<()> {
    let spec = specs
        .iter_mut()
        .find(|s| s.index == index)
        .ok_or_else(|| anyhow::anyhow!("no bank with index {index} to attach file to"))?;
    spec.files.push((name, data));
    Ok(())
}

/// Build a formatted FAT12 image and populate it with `files`.
pub fn build_image(
    format: FloppyFormat,
    label: &str,
    serial: u32,
    files: &[(String, Vec<u8>)],
) -> Result<Vec<u8>> {
    let mut img = format_image(format, label, serial);
    for (name, data) in files {
        add_file(&mut img, name, data)?;
    }
    Ok(img)
}
