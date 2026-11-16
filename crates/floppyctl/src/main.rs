//! `floppyctl` — create and inspect FAT12 floppy images.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use floppyctl::blockset::BlockSet;
use floppyctl::sfr::{build_usb_image, BANK_STRIDE};
use floppyctl::{
    attach_file, build_image, inspect, parse_add_spec, parse_bank_spec, parse_format, parse_naming,
};

#[derive(Parser)]
#[command(name = "floppyctl", about = "Create and inspect FAT12 floppy images")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a formatted FAT12 image file.
    Format {
        /// Floppy size: 720, 1.2 or 1.44.
        #[arg(long, default_value = "1.44")]
        size: String,
        /// Volume label (max 11 chars).
        #[arg(long, default_value = "NO NAME")]
        label: String,
        /// Volume serial number (decimal).
        #[arg(long, default_value_t = 0)]
        serial: u32,
        /// Files to copy into the image (repeatable).
        #[arg(long = "add", value_name = "FILE")]
        add: Vec<PathBuf>,
        /// Output image path.
        #[arg(long, short)]
        out: PathBuf,
    },
    /// Show label, capacity and usage of an existing image.
    Info {
        /// Image file to inspect.
        image: PathBuf,
    },
    /// Add/overwrite a block in a floppy set (a directory of .img files).
    SetAdd {
        /// Set directory (USB drive root).
        #[arg(long)]
        dir: PathBuf,
        /// Block index.
        #[arg(long)]
        index: u32,
        /// Floppy size: 720, 1.2 or 1.44.
        #[arg(long, default_value = "1.44")]
        size: String,
        /// Volume label.
        #[arg(long, default_value = "NO NAME")]
        label: String,
        /// Naming scheme: gotek or ipcas.
        #[arg(long, default_value = "gotek")]
        naming: String,
    },
    /// List all blocks in a floppy set.
    SetList {
        /// Set directory (USB drive root).
        #[arg(long)]
        dir: PathBuf,
        /// Naming scheme: gotek or ipcas.
        #[arg(long, default_value = "gotek")]
        naming: String,
    },
    /// Build a raw SFR1M44 USB image (1.5 MiB banks) to flash onto the device's USB stick.
    BuildUsb {
        /// Output raw image path.
        #[arg(long, short)]
        out: PathBuf,
        /// Total number of banks (100 for U100, 1000 for U100K).
        #[arg(long, default_value_t = 100)]
        banks: u32,
        /// Bank spec "index:size:label", repeatable (e.g. --bank 0:1.44:BOOTDISK).
        #[arg(long = "bank", value_name = "SPEC")]
        bank: Vec<String>,
        /// Put a file into a bank: "index:path", repeatable (e.g. --add-to 0:readme.txt).
        #[arg(long = "add-to", value_name = "INDEX:PATH")]
        add_to: Vec<String>,
    },
    /// Show the files inside a bank, read from a device or an image file.
    ReadBank {
        /// Bank index to read.
        #[arg(long)]
        index: u32,
        /// Read live from this physical drive index (from `list-usb`).
        #[arg(long)]
        drive: Option<u32>,
        /// Or read from a raw USB image file instead.
        #[arg(long)]
        image: Option<PathBuf>,
    },
    /// List removable/USB drives detected on this machine.
    ListUsb,
    /// Write a raw image file directly to a removable USB drive. DESTRUCTIVE.
    WriteUsb {
        /// Raw image file to write.
        #[arg(long)]
        image: PathBuf,
        /// Target physical drive index (from `list-usb`).
        #[arg(long)]
        index: u32,
        /// Required confirmation; without it the command refuses to write.
        #[arg(long)]
        yes: bool,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Format {
            size,
            label,
            serial,
            add,
            out,
        } => {
            let format = parse_format(&size)?;
            let mut files = Vec::new();
            for path in &add {
                let data = std::fs::read(path)
                    .with_context(|| format!("reading {}", path.display()))?;
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .context("invalid file name")?
                    .to_uppercase();
                files.push((name, data));
            }
            let img = build_image(format, &label, serial, &files)?;
            std::fs::write(&out, &img)
                .with_context(|| format!("writing {}", out.display()))?;
            println!(
                "Wrote {} ({} bytes, {}, label '{}', {} file(s))",
                out.display(),
                img.len(),
                size,
                label,
                files.len()
            );
        }
        Command::Info { image } => {
            let data = std::fs::read(&image)
                .with_context(|| format!("reading {}", image.display()))?;
            let info = inspect(&data)?;
            println!("Image : {}", image.display());
            println!("Label : {}", info.label);
            println!("Type  : {}", info.fat_type);
            println!(
                "Total : {} bytes ({:.1} KB)",
                info.total_bytes,
                info.total_bytes as f64 / 1024.0
            );
            println!(
                "Used  : {} bytes ({:.1} KB)",
                info.used_bytes,
                info.used_bytes as f64 / 1024.0
            );
            println!(
                "Free  : {} bytes ({:.1} KB)",
                info.free_bytes,
                info.free_bytes as f64 / 1024.0
            );
        }
        Command::SetAdd {
            dir,
            index,
            size,
            label,
            naming,
        } => {
            let format = parse_format(&size)?;
            let set = BlockSet::new(&dir, parse_naming(&naming)?);
            let path = set.create_block(index, format, &label, 0)?;
            println!("Created block {index}: {}", path.display());
        }
        Command::SetList { dir, naming } => {
            let set = BlockSet::new(&dir, parse_naming(&naming)?);
            let blocks = set.list()?;
            if blocks.is_empty() {
                println!("(no blocks in {})", dir.display());
            } else {
                println!("{:<5} {:<12} {:<7} {:>10} {:>10}", "IDX", "LABEL", "TYPE", "USED", "FREE");
                for b in blocks {
                    println!(
                        "{:<5} {:<12} {:<7} {:>10} {:>10}",
                        b.index, b.info.label, b.info.fat_type, b.info.used_bytes, b.info.free_bytes
                    );
                }
            }
        }
        Command::BuildUsb {
            out,
            banks,
            bank,
            add_to,
        } => {
            let mut specs = bank
                .iter()
                .map(|s| parse_bank_spec(s))
                .collect::<Result<Vec<_>>>()?;
            for spec in &add_to {
                let (index, path) = parse_add_spec(spec)?;
                let path = PathBuf::from(path);
                let data = std::fs::read(&path)
                    .with_context(|| format!("reading {}", path.display()))?;
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .context("invalid file name")?
                    .to_uppercase();
                attach_file(&mut specs, index, name, data)?;
            }
            let img = build_usb_image(banks, &specs)?;
            std::fs::write(&out, &img)
                .with_context(|| format!("writing {}", out.display()))?;
            println!(
                "Wrote {} ({} banks x {} KiB = {:.1} MiB, {} formatted)",
                out.display(),
                banks,
                BANK_STRIDE / 1024,
                img.len() as f64 / (1024.0 * 1024.0),
                specs.len()
            );
            println!("Flash this image to the USB stick with Rufus / Win32DiskImager / dd, then insert it into the SFR1M44 device.");
        }
        Command::ReadBank {
            index,
            drive,
            image,
        } => {
            let bank: Vec<u8> = if let Some(path) = image {
                let raw = std::fs::read(&path)
                    .with_context(|| format!("reading {}", path.display()))?;
                floppyctl::sfr::extract_bank(&raw, index)?.to_vec()
            } else if let Some(drive_index) = drive {
                let d = floppyctl::device::list_usb_drives()?
                    .into_iter()
                    .find(|d| d.index == drive_index)
                    .with_context(|| format!("no removable drive with index {drive_index}"))?;
                floppyctl::device::read_bank(&d, index)?
            } else {
                anyhow::bail!("provide either --drive <IDX> or --image <FILE>");
            };
            let info = inspect(&bank)?;
            println!(
                "Bank {index}: label '{}', {} used / {} total",
                info.label, info.used_bytes, info.total_bytes
            );
            let files = floppyctl::list_fat_files(&bank)?;
            if files.is_empty() {
                println!("  (empty)");
            } else {
                for f in files {
                    let kind = if f.is_dir { "<DIR>" } else { "     " };
                    println!("  {kind} {:>10}  {}", f.size, f.name);
                }
            }
        }
        Command::ListUsb => {
            let drives = floppyctl::device::list_usb_drives()?;
            if drives.is_empty() {
                println!("(no removable/USB drives detected)");
            } else {
                println!("{:<5} {:<24} {:>12} PATH", "IDX", "MODEL", "SIZE(MB)");
                for d in drives {
                    println!(
                        "{:<5} {:<24} {:>12} {}",
                        d.index,
                        d.model,
                        d.size_bytes / (1024 * 1024),
                        d.path
                    );
                }
            }
        }
        Command::WriteUsb { image, index, yes } => {
            let data = std::fs::read(&image)
                .with_context(|| format!("reading {}", image.display()))?;
            let drives = floppyctl::device::list_usb_drives()?;
            let drive = drives
                .into_iter()
                .find(|d| d.index == index)
                .with_context(|| format!("no removable drive with index {index}"))?;
            println!(
                "Writing {} bytes to drive {} ({}, {} MB)...",
                data.len(),
                drive.index,
                drive.model,
                drive.size_bytes / (1024 * 1024)
            );
            floppyctl::device::write_image_to_drive(&drive, &data, yes)?;
            println!("Done. Safely eject the drive before removing it.");
        }
    }
    Ok(())
}
