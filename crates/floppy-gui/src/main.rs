// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use floppyctl::blockset::BlockSet;
use floppyctl::device::{self, UsbDrive};
use floppyctl::sfr::{build_usb_image, BankSpec, BANK_STRIDE};
use floppyctl::{build_image, parse_format, parse_naming, BankFile};
use serde::{Deserialize, Serialize};
use tauri::Emitter;

/// Emit a progress event the frontend's progress bar listens to.
fn emit_progress(window: &tauri::Window, phase: &str, done: u64, total: u64) {
    let _ = window.emit(
        "fw://progress",
        serde_json::json!({ "phase": phase, "done": done, "total": total }),
    );
}

/// A block as presented to the UI.
#[derive(Serialize)]
struct BlockDto {
    index: u32,
    path: String,
    label: String,
    fat_type: String,
    total_bytes: u64,
    used_bytes: u64,
    free_bytes: u64,
}

/// List all blocks in a floppy set directory.
#[tauri::command]
fn list_blocks(dir: String, naming: String) -> Result<Vec<BlockDto>, String> {
    let scheme = parse_naming(&naming).map_err(|e| e.to_string())?;
    let set = BlockSet::new(&dir, scheme);
    let blocks = set.list().map_err(|e| e.to_string())?;
    Ok(blocks
        .into_iter()
        .map(|b| BlockDto {
            index: b.index,
            path: b.path.display().to_string(),
            label: b.info.label,
            fat_type: b.info.fat_type,
            total_bytes: b.info.total_bytes,
            used_bytes: b.info.used_bytes,
            free_bytes: b.info.free_bytes,
        })
        .collect())
}

/// Create (or overwrite) a formatted block in a set.
#[tauri::command]
fn create_block(
    dir: String,
    index: u32,
    size: String,
    label: String,
    naming: String,
) -> Result<String, String> {
    let format = parse_format(&size).map_err(|e| e.to_string())?;
    let scheme = parse_naming(&naming).map_err(|e| e.to_string())?;
    let set = BlockSet::new(&dir, scheme);
    let path = set
        .create_block(index, format, &label, 0)
        .map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

/// Format a standalone .img file (not part of a set).
#[tauri::command]
fn format_image_file(path: String, size: String, label: String) -> Result<u64, String> {
    let format = parse_format(&size).map_err(|e| e.to_string())?;
    let img = build_image(format, &label, 0, &[]).map_err(|e| e.to_string())?;
    std::fs::write(&path, &img).map_err(|e| e.to_string())?;
    Ok(img.len() as u64)
}

/// One bank as described by the UI.
#[derive(Deserialize)]
struct BankInput {
    index: u32,
    size: String,
    label: String,
}

/// Build a raw SFR1M44 USB image file from a list of banks.
#[tauri::command]
fn build_sfr_image(out: String, banks: u32, bank_specs: Vec<BankInput>) -> Result<String, String> {
    let specs = bank_specs
        .into_iter()
        .map(|b| {
            Ok(BankSpec {
                index: b.index,
                format: parse_format(&b.size).map_err(|e| e.to_string())?,
                label: b.label,
                files: vec![],
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let img = build_usb_image(banks, &specs).map_err(|e| e.to_string())?;
    std::fs::write(&out, &img).map_err(|e| e.to_string())?;
    Ok(format!(
        "{} \u{2014} {} banks, {:.1} MiB ({} formatted)",
        out,
        banks,
        img.len() as f64 / (1024.0 * 1024.0),
        specs_len_note(&img)
    ))
}

/// Number of non-empty banks (for the status line).
fn specs_len_note(img: &[u8]) -> usize {
    img.chunks(BANK_STRIDE)
        .filter(|b| b.iter().any(|&x| x != 0))
        .count()
}

/// List removable/USB drives for the write-target dropdown.
#[tauri::command]
fn list_drives() -> Result<Vec<UsbDrive>, String> {
    device::list_usb_drives().map_err(|e| e.to_string())
}

/// "Begin to format": format the whole selected USB into `banks` identical
/// blank FAT12 floppies of `size`, then write it to the device. DESTRUCTIVE.
/// Each block is labelled `label_prefix` + (`label_start` + block index), e.g.
/// FLOPPY0, FLOPPY1, …; an empty prefix leaves them as `NO NAME`.
/// The frontend must confirm with the user before calling this.
#[tauri::command]
fn format_usb_device(
    window: tauri::Window,
    index: u32,
    size: String,
    banks: u32,
    label_prefix: Option<String>,
    label_start: Option<u32>,
    verify: Option<bool>,
) -> Result<String, String> {
    let format = parse_format(&size).map_err(|e| e.to_string())?;
    let prefix = label_prefix.unwrap_or_default();
    let start = label_start.unwrap_or(0);
    let specs: Vec<BankSpec> = (0..banks)
        .map(|i| BankSpec {
            index: i,
            format,
            label: floppyctl::numbered_label(&prefix, start.saturating_add(i)),
            files: vec![],
        })
        .collect();
    let img = build_usb_image(banks, &specs).map_err(|e| e.to_string())?;

    let drive = device::list_usb_drives()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|d| d.index == index)
        .ok_or_else(|| format!("no removable drive with index {index}"))?;
    device::write_image_to_drive(&drive, &img, true, |done, total| {
        emit_progress(&window, "write", done, total);
    })
    .map_err(|e| e.to_string())?;
    if verify.unwrap_or(true) {
        verify_written(&window, &drive, &img)?;
    }
    emit_progress(&window, "done", 0, 0);
    Ok(format!(
        "Formatted {} into {} x {} floppies ({:.0} MiB written)",
        drive.model,
        banks,
        size,
        img.len() as f64 / (1024.0 * 1024.0)
    ))
}

/// Write a raw image file to the selected removable drive. DESTRUCTIVE.
/// The frontend must confirm with the user before calling this.
#[tauri::command]
fn write_usb(
    window: tauri::Window,
    image_path: String,
    index: u32,
    verify: Option<bool>,
) -> Result<String, String> {
    let data = std::fs::read(&image_path).map_err(|e| e.to_string())?;
    let drive = device::list_usb_drives()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|d| d.index == index)
        .ok_or_else(|| format!("no removable drive with index {index}"))?;
    device::write_image_to_drive(&drive, &data, true, |done, total| {
        emit_progress(&window, "write", done, total);
    })
    .map_err(|e| e.to_string())?;
    if verify.unwrap_or(true) {
        verify_written(&window, &drive, &data)?;
    }
    emit_progress(&window, "done", 0, 0);
    Ok(format!(
        "Wrote {} bytes to {} ({})",
        data.len(),
        drive.path,
        drive.model
    ))
}

/// Find a removable drive by its physical index.
fn find_drive(drive_index: u32) -> Result<UsbDrive, String> {
    device::list_usb_drives()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|d| d.index == drive_index)
        .ok_or_else(|| format!("no removable drive with index {drive_index}"))
}

/// Join a directory path and a child name into a root-relative path.
fn join_path(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// A directory listing inside a bank, plus the bank's own meta.
#[derive(Serialize)]
struct DirView {
    index: u32,
    path: String,
    formatted: bool,
    label: String,
    used_bytes: u64,
    total_bytes: u64,
    entries: Vec<BankFile>,
}

fn dir_view(index: u32, path: &str, img: &[u8]) -> DirView {
    match floppyctl::inspect(img) {
        Ok(info) => DirView {
            index,
            path: path.to_string(),
            formatted: true,
            label: info.label,
            used_bytes: info.used_bytes,
            total_bytes: info.total_bytes,
            entries: floppyctl::fsops::list_dir(img, path).unwrap_or_default(),
        },
        Err(_) => DirView {
            index,
            path: path.to_string(),
            formatted: false,
            label: String::new(),
            used_bytes: 0,
            total_bytes: 0,
            entries: vec![],
        },
    }
}

/// List a directory inside a bank (read-only). Blank banks report `formatted: false`.
#[tauri::command]
fn browse_bank(drive_index: u32, index: u32, path: String) -> Result<DirView, String> {
    let drive = find_drive(drive_index)?;
    let img = device::read_bank(&drive, index).map_err(|e| e.to_string())?;
    Ok(dir_view(index, &path, &img))
}

/// A block as shown in the Explorer-style block list.
#[derive(Serialize)]
struct BlockLabel {
    index: u32,
    formatted: bool,
    label: String,
}

/// Cheaply label the first `count` blocks of a drive (reads one sector each).
#[tauri::command]
fn scan_blocks(drive_index: u32, count: u32) -> Result<Vec<BlockLabel>, String> {
    let drive = device::list_usb_drives()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|d| d.index == drive_index)
        .ok_or_else(|| format!("no removable drive with index {drive_index}"))?;
    let headers = device::read_bank_headers(&drive, count).map_err(|e| e.to_string())?;
    Ok(headers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let label = floppyctl::boot_label(h);
            BlockLabel {
                index: i as u32,
                formatted: label.is_some(),
                label: label.unwrap_or_default(),
            }
        })
        .collect())
}

/// Read a bank, run `op` on the image, write it back, and return the listing of `path`.
fn mutate_bank(
    drive_index: u32,
    index: u32,
    path: &str,
    op: impl FnOnce(&mut Vec<u8>) -> Result<(), String>,
) -> Result<DirView, String> {
    let drive = find_drive(drive_index)?;
    let mut img = device::read_bank(&drive, index).map_err(|e| e.to_string())?;
    if floppyctl::inspect(&img).is_err() {
        return Err(format!("Block {index} isn't formatted — format the USB first."));
    }
    op(&mut img)?;
    device::write_bank(&drive, index, &img).map_err(|e| e.to_string())?;
    Ok(dir_view(index, path, &img))
}

/// Create a folder named `name` inside `path`.
#[tauri::command]
fn mkdir_in_bank(
    drive_index: u32,
    index: u32,
    path: String,
    name: String,
) -> Result<DirView, String> {
    mutate_bank(drive_index, index, &path, |img| {
        floppyctl::fsops::make_dir(img, &join_path(&path, &name)).map_err(|e| e.to_string())
    })
}

/// Add dropped/picked files and folders (recursively) into directory `path`.
#[tauri::command]
fn add_files_in_dir(
    drive_index: u32,
    index: u32,
    path: String,
    files: Vec<String>,
) -> Result<DirView, String> {
    mutate_bank(drive_index, index, &path, |img| {
        for p in &files {
            let name = std::path::Path::new(p)
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| format!("invalid file name: {p}"))?
                .to_uppercase();
            let tree = floppyctl::fsops::read_host_tree(std::path::Path::new(p))
                .map_err(|e| format!("{p}: {e}"))?;
            floppyctl::fsops::write_tree(img, &join_path(&path, &name), &tree)
                .map_err(|e| format!("{name}: {e} (floppy full?)"))?;
        }
        Ok(())
    })
}

/// Files/folders copied in Windows Explorer, plus the clipboard sequence number.
#[tauri::command]
fn clipboard_files() -> floppyctl::clipboard::ClipboardFiles {
    floppyctl::clipboard::clipboard_files()
}

/// Delete entries (files or folders, recursively) named `names` inside `path`.
#[tauri::command]
fn delete_in_dir(
    drive_index: u32,
    index: u32,
    path: String,
    names: Vec<String>,
) -> Result<DirView, String> {
    mutate_bank(drive_index, index, &path, |img| {
        for name in &names {
            floppyctl::fsops::remove_path(img, &join_path(&path, name))
                .map_err(|e| format!("{name}: {e}"))?;
        }
        Ok(())
    })
}

/// Rename entry `from` to `to` within `path`.
#[tauri::command]
fn rename_in_dir(
    drive_index: u32,
    index: u32,
    path: String,
    from: String,
    to: String,
) -> Result<DirView, String> {
    mutate_bank(drive_index, index, &path, |img| {
        floppyctl::fsops::rename_path(img, &join_path(&path, &from), &join_path(&path, &to))
            .map_err(|e| e.to_string())
    })
}

/// Save a file from a bank to the PC. With an empty `dest`, saves into Downloads.
#[tauri::command]
fn export_file(
    drive_index: u32,
    index: u32,
    path: String,
    name: String,
    dest: String,
) -> Result<String, String> {
    let drive = find_drive(drive_index)?;
    let img = device::read_bank(&drive, index).map_err(|e| e.to_string())?;
    let data = floppyctl::fsops::read_file_at(&img, &join_path(&path, &name))
        .map_err(|e| e.to_string())?;
    let dest = if dest.trim().is_empty() {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .map_err(|_| "home directory not found".to_string())?;
        std::path::Path::new(&home)
            .join("Downloads")
            .join(&name)
            .to_string_lossy()
            .into_owned()
    } else {
        dest
    };
    std::fs::write(&dest, &data).map_err(|e| format!("{dest}: {e}"))?;
    Ok(dest)
}

/// Copy or move (`cut`) entries from one block/folder to another.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn transfer_entries(
    drive_index: u32,
    src_index: u32,
    src_path: String,
    names: Vec<String>,
    dst_index: u32,
    dst_path: String,
    cut: bool,
) -> Result<DirView, String> {
    use floppyctl::fsops::{read_tree, remove_path, write_tree};
    let drive = find_drive(drive_index)?;

    if src_index == dst_index {
        let mut img = device::read_bank(&drive, src_index).map_err(|e| e.to_string())?;
        if floppyctl::inspect(&img).is_err() {
            return Err("block isn't formatted".into());
        }
        for name in &names {
            let src_full = join_path(&src_path, name);
            let dst_full = join_path(&dst_path, name);
            let tree = read_tree(&img, &src_full).map_err(|e| format!("{name}: {e}"))?;
            write_tree(&mut img, &dst_full, &tree).map_err(|e| format!("{name}: {e}"))?;
            if cut {
                remove_path(&mut img, &src_full).map_err(|e| format!("{name}: {e}"))?;
            }
        }
        device::write_bank(&drive, src_index, &img).map_err(|e| e.to_string())?;
        Ok(dir_view(dst_index, &dst_path, &img))
    } else {
        let mut src_img = device::read_bank(&drive, src_index).map_err(|e| e.to_string())?;
        let mut dst_img = device::read_bank(&drive, dst_index).map_err(|e| e.to_string())?;
        if floppyctl::inspect(&dst_img).is_err() {
            return Err("destination block isn't formatted".into());
        }
        for name in &names {
            let tree = read_tree(&src_img, &join_path(&src_path, name))
                .map_err(|e| format!("{name}: {e}"))?;
            write_tree(&mut dst_img, &join_path(&dst_path, name), &tree)
                .map_err(|e| format!("{name}: {e}"))?;
        }
        device::write_bank(&drive, dst_index, &dst_img).map_err(|e| e.to_string())?;
        if cut {
            for name in &names {
                remove_path(&mut src_img, &join_path(&src_path, name))
                    .map_err(|e| format!("{name}: {e}"))?;
            }
            device::write_bank(&drive, src_index, &src_img).map_err(|e| e.to_string())?;
        }
        Ok(dir_view(dst_index, &dst_path, &dst_img))
    }
}

/// A file's contents prepared for in-app preview.
#[derive(Serialize)]
struct Preview {
    kind: String, // "text" | "image" | "binary"
    text: String,
    data_url: String,
    size: u64,
}

/// Read a file from a bank and prepare it for preview (text inline, images as data URL).
#[tauri::command]
fn preview_file(
    drive_index: u32,
    index: u32,
    path: String,
    name: String,
) -> Result<Preview, String> {
    use base64::Engine;
    let drive = find_drive(drive_index)?;
    let img = device::read_bank(&drive, index).map_err(|e| e.to_string())?;
    let data = floppyctl::fsops::read_file_at(&img, &join_path(&path, &name))
        .map_err(|e| e.to_string())?;
    let size = data.len() as u64;

    let lower = name.to_lowercase();
    let image_mime = [
        (".png", "image/png"),
        (".jpg", "image/jpeg"),
        (".jpeg", "image/jpeg"),
        (".gif", "image/gif"),
        (".bmp", "image/bmp"),
        (".ico", "image/x-icon"),
        (".webp", "image/webp"),
    ]
    .iter()
    .find(|(ext, _)| lower.ends_with(ext))
    .map(|(_, m)| *m);

    if let Some(mime) = image_mime {
        let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
        return Ok(Preview {
            kind: "image".into(),
            text: String::new(),
            data_url: format!("data:{mime};base64,{b64}"),
            size,
        });
    }

    let head = &data[..data.len().min(8192)];
    if !head.contains(&0u8) {
        let end = data.len().min(256 * 1024);
        return Ok(Preview {
            kind: "text".into(),
            text: String::from_utf8_lossy(&data[..end]).to_string(),
            data_url: String::new(),
            size,
        });
    }
    Ok(Preview {
        kind: "binary".into(),
        text: String::new(),
        data_url: String::new(),
        size,
    })
}

/// Change a block's volume label. Returns the new (truncated) label.
#[tauri::command]
fn set_block_label(drive_index: u32, index: u32, label: String) -> Result<String, String> {
    let drive = find_drive(drive_index)?;
    let mut img = device::read_bank(&drive, index).map_err(|e| e.to_string())?;
    if floppyctl::inspect(&img).is_err() {
        return Err(format!("Block {index} isn't formatted."));
    }
    fat12::set_label(&mut img, &label).map_err(|e| e.to_string())?;
    device::write_bank(&drive, index, &img).map_err(|e| e.to_string())?;
    Ok(label.chars().take(11).collect())
}

/// Quick-format a single block as a blank 1.44M floppy with `label`. DESTRUCTIVE.
#[tauri::command]
fn format_one_block(drive_index: u32, index: u32, label: String) -> Result<(), String> {
    let drive = find_drive(drive_index)?;
    let img = build_image(fat12::FloppyFormat::F1_44M, &label, 0, &[]).map_err(|e| e.to_string())?;
    device::write_bank(&drive, index, &img).map_err(|e| e.to_string())
}

/// Save one block as a 1.44 MB floppy image file.
#[tauri::command]
fn export_bank(drive_index: u32, index: u32, dest: String) -> Result<String, String> {
    let drive = find_drive(drive_index)?;
    let img = device::read_bank(&drive, index).map_err(|e| e.to_string())?;
    std::fs::write(&dest, &img).map_err(|e| format!("{dest}: {e}"))?;
    Ok(dest)
}

/// Write a floppy image file into one block (raw copy, any floppy format the
/// target machine uses). The write is read back and verified. DESTRUCTIVE.
#[tauri::command]
fn import_bank(drive_index: u32, index: u32, path: String) -> Result<(), String> {
    let drive = find_drive(drive_index)?;
    let data = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
    let img = floppyctl::sfr::normalize_bank_image(&data).map_err(|e| e.to_string())?;
    device::write_bank(&drive, index, img).map_err(|e| e.to_string())
}

/// Copy block `src` onto block `dst` byte for byte (verified). DESTRUCTIVE for `dst`.
#[tauri::command]
fn clone_bank(drive_index: u32, src: u32, dst: u32) -> Result<(), String> {
    if src == dst {
        return Err("source and target block are the same".into());
    }
    let drive = find_drive(drive_index)?;
    let img = device::read_bank(&drive, src).map_err(|e| e.to_string())?;
    device::write_bank(&drive, dst, &img).map_err(|e| e.to_string())
}

/// Read a whole-USB write back and compare it, reporting "verify" progress.
fn verify_written(window: &tauri::Window, drive: &UsbDrive, data: &[u8]) -> Result<(), String> {
    device::verify_region(drive, 0, data, |done, total| {
        emit_progress(window, "verify", done, total);
    })
    .map_err(|e| e.to_string())
}

/// Back up the first `banks` banks of the USB to a raw image file.
#[tauri::command]
fn backup_usb(
    window: tauri::Window,
    drive_index: u32,
    banks: u32,
    out_path: String,
) -> Result<String, String> {
    let drive = find_drive(drive_index)?;
    let len = banks as usize * BANK_STRIDE;
    if len as u64 > drive.size_bytes {
        return Err("requested size is larger than the drive".into());
    }
    let data = device::read_range(&drive, len, |done, total| {
        emit_progress(&window, "read", done, total);
    })
    .map_err(|e| e.to_string())?;
    emit_progress(&window, "done", 0, 0);
    std::fs::write(&out_path, &data).map_err(|e| format!("{out_path}: {e}"))?;
    Ok(format!(
        "{} ({:.0} MiB)",
        out_path,
        data.len() as f64 / (1024.0 * 1024.0)
    ))
}

/// Quick-format `count` blocks starting at `start`, all blank 1.44M with `label`.
/// DESTRUCTIVE. Reports progress per block.
#[tauri::command]
fn format_block_range(
    window: tauri::Window,
    drive_index: u32,
    start: u32,
    count: u32,
    label: String,
) -> Result<(), String> {
    let drive = find_drive(drive_index)?;
    let img = build_image(fat12::FloppyFormat::F1_44M, &label, 0, &[]).map_err(|e| e.to_string())?;
    for i in 0..count {
        device::write_bank(&drive, start + i, &img).map_err(|e| e.to_string())?;
        emit_progress(&window, "write", (i + 1) as u64, count as u64);
    }
    emit_progress(&window, "done", 0, 0);
    Ok(())
}

/// Make a single block a FreeDOS-bootable 1.44M floppy (boots to A:\>). DESTRUCTIVE.
#[tauri::command]
fn make_bank_bootable(drive_index: u32, index: u32, label: String) -> Result<(), String> {
    let drive = find_drive(drive_index)?;
    let img = floppyctl::freedos::make_bootable_image(&label, 0).map_err(|e| e.to_string())?;
    device::write_bank(&drive, index, &img).map_err(|e| e.to_string())
}

/// True when the app is running elevated (as Administrator). Used to warn that
/// Explorer→app drag-and-drop is blocked by Windows while elevated.
#[tauri::command]
fn is_elevated() -> bool {
    floppyctl::device::is_elevated()
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            list_blocks,
            create_block,
            format_image_file,
            build_sfr_image,
            list_drives,
            write_usb,
            format_usb_device,
            scan_blocks,
            browse_bank,
            mkdir_in_bank,
            add_files_in_dir,
            clipboard_files,
            delete_in_dir,
            rename_in_dir,
            export_file,
            transfer_entries,
            set_block_label,
            format_one_block,
            preview_file,
            is_elevated,
            make_bank_bootable,
            backup_usb,
            format_block_range,
            export_bank,
            import_bank,
            clone_bank
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
