//! USB device enumeration and (guarded) raw reading/writing.
//!
//! Writing a raw image to a physical drive is destructive. Every write goes
//! through [`check_write_target`], which refuses non-removable disks and images
//! that do not fit — so the system disk can never be targeted by accident.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::sfr::{bank_offset, FLOPPY_IMAGE_BYTES};

/// A removable (USB) drive the image can be written to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsbDrive {
    /// Physical drive index (e.g. 3 for `\\.\PhysicalDrive3`).
    pub index: u32,
    /// OS raw-device path.
    pub path: String,
    pub model: String,
    pub size_bytes: u64,
    pub removable: bool,
}

/// Safety gate for every raw write. Refuses fixed disks, oversized images and
/// empty images.
pub fn check_write_target(drive: &UsbDrive, image_len: u64) -> Result<()> {
    if !drive.removable {
        bail!(
            "refusing to write: drive {} ({}) is not removable",
            drive.index,
            drive.model
        );
    }
    if image_len == 0 {
        bail!("refusing to write an empty image");
    }
    if image_len > drive.size_bytes {
        bail!(
            "image ({} bytes) is larger than the drive ({} bytes)",
            image_len,
            drive.size_bytes
        );
    }
    Ok(())
}

/// True when this process is running elevated (as Administrator).
#[cfg(windows)]
pub fn is_elevated() -> bool {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut size = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut size,
        );
        let _ = CloseHandle(token);
        ok.is_ok() && elevation.TokenIsElevated != 0
    }
}

#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    false
}

/// Write `data` to the selected removable drive, starting at sector 0. **Destructive.**
///
/// `confirm` must be `true`; the caller is responsible for obtaining explicit
/// user confirmation first.
pub fn write_image_to_drive(
    drive: &UsbDrive,
    data: &[u8],
    confirm: bool,
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    if !confirm {
        bail!("write not confirmed");
    }
    check_write_target(drive, data.len() as u64)?;
    write_at(drive, 0, data, on_progress)
}

/// Overwrite a single bank (one 1.44MB floppy) in place on the device.
pub fn write_bank(drive: &UsbDrive, index: u32, image: &[u8]) -> Result<()> {
    if !drive.removable {
        bail!("refusing to write: drive {} is not removable", drive.index);
    }
    if image.len() != FLOPPY_IMAGE_BYTES {
        bail!(
            "bank image must be exactly {} bytes, got {}",
            FLOPPY_IMAGE_BYTES,
            image.len()
        );
    }
    write_at(drive, bank_offset(index) as u64, image, |_, _| {})
}

// ----------------------------------------------------------------------------
// Windows implementation
// ----------------------------------------------------------------------------

/// List removable/USB physical drives.
#[cfg(windows)]
pub fn list_usb_drives() -> Result<Vec<UsbDrive>> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    // CREATE_NO_WINDOW: don't flash a console window when calling PowerShell.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let script = "Get-CimInstance Win32_DiskDrive | ForEach-Object { \
        [pscustomobject]@{ index=[int]$_.Index; model=\"$($_.Model)\"; \
        size=[uint64]$_.Size; iface=\"$($_.InterfaceType)\"; media=\"$($_.MediaType)\" } } | \
        ConvertTo-Json -Compress";
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    if !output.status.success() {
        bail!(
            "failed to enumerate drives: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let text = text.trim();
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let value: serde_json::Value = serde_json::from_str(text)?;
    let items = match value {
        serde_json::Value::Array(a) => a,
        other => vec![other],
    };

    let mut drives = Vec::new();
    for item in items {
        let index = item.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let model = item
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let size = item.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
        let iface = item.get("iface").and_then(|v| v.as_str()).unwrap_or("");
        let media = item.get("media").and_then(|v| v.as_str()).unwrap_or("");
        let removable =
            media.to_lowercase().contains("removable") || iface.eq_ignore_ascii_case("USB");
        if removable {
            drives.push(UsbDrive {
                index,
                path: format!(r"\\.\PhysicalDrive{index}"),
                model,
                size_bytes: size,
                removable: true,
            });
        }
    }
    Ok(drives)
}

#[cfg(not(windows))]
pub fn list_usb_drives() -> Result<Vec<UsbDrive>> {
    bail!("drive enumeration is currently only implemented on Windows")
}

/// Read the 1.44MB FAT12 image of bank `index` directly from the drive.
#[cfg(windows)]
pub fn read_bank(drive: &UsbDrive, index: u32) -> Result<Vec<u8>> {
    use windows::Win32::Foundation::{CloseHandle, GENERIC_READ};
    use windows::Win32::Storage::FileSystem::{ReadFile, SetFilePointerEx, FILE_BEGIN};

    let offset = bank_offset(index) as i64;
    let len = FLOPPY_IMAGE_BYTES;
    unsafe {
        let handle = win::open_physical(&drive.path, GENERIC_READ.0)?;
        if let Err(e) = SetFilePointerEx(handle, offset, None, FILE_BEGIN) {
            let _ = CloseHandle(handle);
            return Err(e.into());
        }
        let mut buf = vec![0u8; len];
        let mut pos = 0usize;
        while pos < len {
            let mut read = 0u32;
            if let Err(e) = ReadFile(handle, Some(&mut buf[pos..]), Some(&mut read), None) {
                let _ = CloseHandle(handle);
                return Err(e.into());
            }
            if read == 0 {
                break;
            }
            pos += read as usize;
        }
        let _ = CloseHandle(handle);
        if pos != len {
            bail!("short read: {pos} of {len} bytes");
        }
        Ok(buf)
    }
}

#[cfg(not(windows))]
pub fn read_bank(_drive: &UsbDrive, _index: u32) -> Result<Vec<u8>> {
    bail!("raw device reading is currently only implemented on Windows")
}

/// Read `len` bytes from the start of the drive (for backing up the USB).
#[cfg(windows)]
pub fn read_range(drive: &UsbDrive, len: usize, on_progress: impl Fn(u64, u64)) -> Result<Vec<u8>> {
    use windows::Win32::Foundation::{CloseHandle, GENERIC_READ};
    use windows::Win32::Storage::FileSystem::ReadFile;

    unsafe {
        let handle = win::open_physical(&drive.path, GENERIC_READ.0)?;
        let mut buf = vec![0u8; len];
        let mut pos = 0usize;
        while pos < len {
            let want = (len - pos).min(1024 * 1024);
            let mut read = 0u32;
            if ReadFile(handle, Some(&mut buf[pos..pos + want]), Some(&mut read), None).is_err()
                || read == 0
            {
                let _ = CloseHandle(handle);
                bail!("read failed at offset {pos}");
            }
            pos += read as usize;
            on_progress(pos as u64, len as u64);
        }
        let _ = CloseHandle(handle);
        Ok(buf)
    }
}

#[cfg(not(windows))]
pub fn read_range(_drive: &UsbDrive, _len: usize, _on_progress: impl Fn(u64, u64)) -> Result<Vec<u8>> {
    bail!("raw device reading is currently only implemented on Windows")
}

/// Read the first 512 bytes (boot sector) of each of the first `count` banks,
/// opening the drive once. Used to label the block list cheaply. Scanning stops
/// early at the end of the device.
#[cfg(windows)]
pub fn read_bank_headers(drive: &UsbDrive, count: u32) -> Result<Vec<Vec<u8>>> {
    use windows::Win32::Foundation::{CloseHandle, GENERIC_READ};
    use windows::Win32::Storage::FileSystem::{ReadFile, SetFilePointerEx, FILE_BEGIN};

    unsafe {
        let handle = win::open_physical(&drive.path, GENERIC_READ.0)?;
        let mut out = Vec::with_capacity(count as usize);
        for i in 0..count {
            if SetFilePointerEx(handle, bank_offset(i) as i64, None, FILE_BEGIN).is_err() {
                break;
            }
            let mut buf = vec![0u8; 512];
            let mut read = 0u32;
            if ReadFile(handle, Some(&mut buf), Some(&mut read), None).is_err() || read < 512 {
                break;
            }
            out.push(buf);
        }
        let _ = CloseHandle(handle);
        Ok(out)
    }
}

#[cfg(not(windows))]
pub fn read_bank_headers(_drive: &UsbDrive, _count: u32) -> Result<Vec<Vec<u8>>> {
    bail!("raw device reading is currently only implemented on Windows")
}

/// Write `data` to `drive` starting at `offset`, locking the disk's volumes first.
#[cfg(windows)]
fn write_at(
    drive: &UsbDrive,
    offset: u64,
    data: &[u8],
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE};
    use windows::Win32::Storage::FileSystem::{SetFilePointerEx, WriteFile, FILE_BEGIN};

    if !data.len().is_multiple_of(512) {
        bail!("image size {} is not a multiple of 512", data.len());
    }
    let total = data.len() as u64;

    unsafe {
        let volumes = win::lock_disk_volumes(drive.index);
        let disk = win::open_physical(&drive.path, GENERIC_READ.0 | GENERIC_WRITE.0)?;

        if !win::device_number(disk)?.is_some_and(|n| n == drive.index) {
            let _ = CloseHandle(disk);
            win::close_all(volumes);
            bail!("safety check failed: opened device is not drive {}", drive.index);
        }

        let result = (|| {
            SetFilePointerEx(disk, offset as i64, None, FILE_BEGIN)?;
            let mut written_total = 0usize;
            for chunk in data.chunks(1024 * 1024) {
                let mut written = 0u32;
                WriteFile(disk, Some(chunk), Some(&mut written), None)?;
                written_total += written as usize;
                on_progress(written_total as u64, total);
            }
            if written_total != data.len() {
                bail!("short write: {written_total} of {} bytes", data.len());
            }
            Ok(())
        })();

        let _ = CloseHandle(disk);
        win::close_all(volumes);
        result
    }
}

#[cfg(not(windows))]
fn write_at(_drive: &UsbDrive, _offset: u64, _data: &[u8], _on_progress: impl Fn(u64, u64)) -> Result<()> {
    bail!("raw device writing is currently only implemented on Windows")
}

#[cfg(windows)]
mod win {
    use anyhow::Result;
    use windows::core::HSTRING;
    use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::Ioctl::{
        FSCTL_DISMOUNT_VOLUME, FSCTL_LOCK_VOLUME, IOCTL_STORAGE_GET_DEVICE_NUMBER,
        STORAGE_DEVICE_NUMBER,
    };
    use windows::Win32::System::IO::DeviceIoControl;

    /// Open `\\.\PhysicalDriveN` (or a `\\.\X:` volume) with the given access rights.
    pub unsafe fn open_physical(path: &str, access: u32) -> Result<HANDLE> {
        let wide = HSTRING::from(path);
        let handle = CreateFileW(
            &wide,
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )?;
        Ok(handle)
    }

    /// The physical device number backing an open handle, if available.
    pub unsafe fn device_number(handle: HANDLE) -> Result<Option<u32>> {
        let mut dev = STORAGE_DEVICE_NUMBER::default();
        let mut returned = 0u32;
        let ok = DeviceIoControl(
            handle,
            IOCTL_STORAGE_GET_DEVICE_NUMBER,
            None,
            0,
            Some(&mut dev as *mut _ as *mut _),
            std::mem::size_of::<STORAGE_DEVICE_NUMBER>() as u32,
            Some(&mut returned),
            None,
        );
        Ok(ok.ok().map(|_| dev.DeviceNumber))
    }

    /// Lock + dismount every mounted volume that lives on physical disk `index`.
    /// The returned handles must be kept open (and closed with [`close_all`]) to
    /// keep the volumes locked during the write.
    pub unsafe fn lock_disk_volumes(index: u32) -> Vec<HANDLE> {
        let mut handles = Vec::new();
        for letter in b'A'..=b'Z' {
            let path = format!(r"\\.\{}:", letter as char);
            let vol = match open_physical(&path, GENERIC_READ.0 | GENERIC_WRITE.0) {
                Ok(h) => h,
                Err(_) => continue,
            };
            if device_number(vol).ok().flatten() == Some(index) {
                let _ = DeviceIoControl(vol, FSCTL_LOCK_VOLUME, None, 0, None, 0, None, None);
                let _ = DeviceIoControl(vol, FSCTL_DISMOUNT_VOLUME, None, 0, None, 0, None, None);
                handles.push(vol);
            } else {
                let _ = CloseHandle(vol);
            }
        }
        handles
    }

    pub unsafe fn close_all(handles: Vec<HANDLE>) {
        for h in handles {
            let _ = CloseHandle(h);
        }
    }
}
