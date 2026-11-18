//! Integrity check for a single floppy bank: is it blank, healthy, suspicious
//! (warning) or corrupt?
//!
//! The check validates the boot sector / BPB, scans every FAT12 entry for
//! impossible cluster pointers, compares the FAT copies, and finally mounts the
//! filesystem and reads every file to make sure each one is readable and has
//! the length its directory entry claims.

use std::io::{Cursor, Read};

use fatfs::{FileSystem, FsOptions};
use serde::Serialize;

/// Overall verdict for a bank, worst wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Health {
    /// Never formatted (all zero / all 0xFF).
    Blank,
    /// Mounts, every file is readable, no inconsistencies.
    Ok,
    /// Usable, but something is off (e.g. the FAT copies differ, bad clusters).
    Warning,
    /// Not a usable FAT12 floppy, or files cannot be read back.
    Corrupt,
}

/// The result of [`check_image`].
#[derive(Debug, Clone, Serialize)]
pub struct HealthReport {
    pub status: Health,
    pub label: String,
    pub used_bytes: u64,
    pub total_bytes: u64,
    /// Number of files (not folders), in all folders.
    pub files: u32,
    /// Human-readable findings (empty when everything is fine).
    pub issues: Vec<String>,
}

impl HealthReport {
    fn new(status: Health) -> Self {
        Self {
            status,
            label: String::new(),
            used_bytes: 0,
            total_bytes: 0,
            files: 0,
            issues: Vec::new(),
        }
    }

    fn flag(&mut self, level: Health, issue: impl Into<String>) {
        self.status = self.status.max(level);
        self.issues.push(issue.into());
    }
}

/// The BPB fields the check needs.
struct Bpb {
    bytes_per_sector: usize,
    sectors_per_cluster: usize,
    reserved: usize,
    fats: usize,
    root_entries: usize,
    total_sectors: usize,
    media: u8,
    sectors_per_fat: usize,
}

fn u16_at(b: &[u8], off: usize) -> usize {
    u16::from_le_bytes([b[off], b[off + 1]]) as usize
}

fn parse_bpb(img: &[u8]) -> Result<Bpb, String> {
    if img.len() < 512 {
        return Err("image is shorter than one sector".into());
    }
    if img[510] != 0x55 || img[511] != 0xAA {
        return Err("no boot sector signature (55 AA)".into());
    }
    let total16 = u16_at(img, 19);
    let total32 = u32::from_le_bytes([img[32], img[33], img[34], img[35]]) as usize;
    let bpb = Bpb {
        bytes_per_sector: u16_at(img, 11),
        sectors_per_cluster: img[13] as usize,
        reserved: u16_at(img, 14),
        fats: img[16] as usize,
        root_entries: u16_at(img, 17),
        total_sectors: if total16 != 0 { total16 } else { total32 },
        media: img[21],
        sectors_per_fat: u16_at(img, 22),
    };
    if ![512, 1024, 2048, 4096].contains(&bpb.bytes_per_sector) {
        return Err(format!("invalid bytes-per-sector {}", bpb.bytes_per_sector));
    }
    if bpb.sectors_per_cluster == 0 || !bpb.sectors_per_cluster.is_power_of_two() {
        return Err(format!(
            "invalid sectors-per-cluster {}",
            bpb.sectors_per_cluster
        ));
    }
    if bpb.reserved == 0 || !(1..=2).contains(&bpb.fats) || bpb.sectors_per_fat == 0 {
        return Err("invalid reserved/FAT layout in the boot sector".into());
    }
    if bpb.root_entries == 0 || bpb.total_sectors == 0 {
        return Err("invalid root-directory or total-sector count".into());
    }
    if bpb.total_sectors * bpb.bytes_per_sector > img.len() {
        return Err(format!(
            "boot sector claims {} bytes but the bank holds {}",
            bpb.total_sectors * bpb.bytes_per_sector,
            img.len()
        ));
    }
    Ok(bpb)
}

/// Read FAT12 entry `n` from a FAT table.
fn fat12_entry(fat: &[u8], n: usize) -> u16 {
    let off = n + n / 2;
    let pair = u16::from_le_bytes([fat[off], fat[off + 1]]);
    if n.is_multiple_of(2) {
        pair & 0x0FFF
    } else {
        pair >> 4
    }
}

/// Check one floppy image (a bank's first 1,474,560 bytes, or any FAT12 image).
pub fn check_image(img: &[u8]) -> HealthReport {
    if img.iter().all(|&b| b == 0) || img.iter().all(|&b| b == 0xFF) {
        return HealthReport::new(Health::Blank);
    }
    let mut r = HealthReport::new(Health::Ok);

    let bpb = match parse_bpb(img) {
        Ok(b) => b,
        Err(e) => {
            r.flag(Health::Corrupt, e);
            return r;
        }
    };

    // ---- FAT table scan ----
    let sector = bpb.bytes_per_sector;
    let root_sectors = (bpb.root_entries * 32).div_ceil(sector);
    let data_start = bpb.reserved + bpb.fats * bpb.sectors_per_fat + root_sectors;
    let clusters = bpb.total_sectors.saturating_sub(data_start) / bpb.sectors_per_cluster;
    if clusters == 0 || clusters >= 4085 {
        r.flag(
            Health::Corrupt,
            format!("not a FAT12 layout ({clusters} data clusters)"),
        );
        return r;
    }
    let fat_len = bpb.sectors_per_fat * sector;
    let fat_at = |n: usize| &img[(bpb.reserved + n * bpb.sectors_per_fat) * sector..][..fat_len];
    let fat = fat_at(0);
    if (clusters + 2) * 3 / 2 + 1 > fat_len {
        r.flag(Health::Corrupt, "FAT is too small for the disk size");
        return r;
    }
    if fat[0] != bpb.media {
        r.flag(
            Health::Warning,
            format!(
                "FAT media byte {:#04x} does not match the boot sector ({:#04x})",
                fat[0], bpb.media
            ),
        );
    }
    let max_cluster = clusters as u16 + 1;
    let (mut invalid, mut bad) = (0u32, 0u32);
    for n in 2..clusters + 2 {
        match fat12_entry(fat, n) {
            0 => {}
            0xFF7 => bad += 1,
            v if v >= 0xFF8 => {}
            v if (2..=max_cluster).contains(&v) => {}
            _ => invalid += 1,
        }
    }
    if invalid > 0 {
        r.flag(
            Health::Corrupt,
            format!("FAT has {invalid} invalid cluster pointer(s)"),
        );
    }
    if bad > 0 {
        r.flag(Health::Warning, format!("{bad} cluster(s) marked bad"));
    }
    if bpb.fats == 2 && fat_at(0) != fat_at(1) {
        r.flag(Health::Warning, "FAT copies differ");
    }

    // ---- Mount and read every file ----
    let fs = match FileSystem::new(Cursor::new(img.to_vec()), FsOptions::new()) {
        Ok(fs) => fs,
        Err(e) => {
            r.flag(Health::Corrupt, format!("cannot mount: {e}"));
            return r;
        }
    };
    r.label = fs.volume_label().trim_end().to_string();
    if let Ok(stats) = fs.stats() {
        let cl = stats.cluster_size() as u64;
        r.total_bytes = stats.total_clusters() as u64 * cl;
        r.used_bytes = r.total_bytes - stats.free_clusters() as u64 * cl;
    }
    if invalid > 0 {
        // Following broken chains is pointless (and could loop); stop here.
        return r;
    }

    let root = fs.root_dir();
    let mut stack = vec![String::new()];
    let mut visited = 0usize;
    while let Some(dir_path) = stack.pop() {
        let dir = if dir_path.is_empty() {
            root.clone()
        } else {
            match root.open_dir(&dir_path) {
                Ok(d) => d,
                Err(e) => {
                    r.flag(Health::Corrupt, format!("folder {dir_path}: {e}"));
                    continue;
                }
            }
        };
        for entry in dir.iter() {
            visited += 1;
            if visited > 20_000 {
                r.flag(Health::Corrupt, "directory structure loops");
                return r;
            }
            let e = match entry {
                Ok(e) => e,
                Err(e) => {
                    r.flag(Health::Corrupt, format!("folder /{dir_path}: {e}"));
                    break;
                }
            };
            let name = e.file_name();
            if name == "." || name == ".." {
                continue;
            }
            let path = if dir_path.is_empty() {
                name
            } else {
                format!("{dir_path}/{name}")
            };
            if e.is_dir() {
                stack.push(path);
                continue;
            }
            r.files += 1;
            let mut buf = Vec::new();
            match e.to_file().read_to_end(&mut buf) {
                Ok(n) if n as u64 == e.len() => {}
                Ok(n) => r.flag(
                    Health::Corrupt,
                    format!("{path}: only {n} of {} bytes readable", e.len()),
                ),
                Err(err) => r.flag(Health::Corrupt, format!("{path}: unreadable ({err})")),
            }
        }
    }
    r
}
