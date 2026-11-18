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
    use windows::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
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
    nix::is_root()
}

/// Index of the first byte where `a` and `b` differ (a length difference counts
/// at the shorter length), or `None` if they are identical.
pub fn first_mismatch(a: &[u8], b: &[u8]) -> Option<usize> {
    a.iter()
        .zip(b)
        .position(|(x, y)| x != y)
        .or_else(|| (a.len() != b.len()).then(|| a.len().min(b.len())))
}

/// Read back `expected.len()` bytes at `offset` and fail if they differ.
pub fn verify_region(
    drive: &UsbDrive,
    offset: u64,
    expected: &[u8],
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    let got = read_region(drive, offset, expected.len(), on_progress)?;
    if let Some(pos) = first_mismatch(&got, expected) {
        bail!("verification failed at byte {}", offset + pos as u64);
    }
    Ok(())
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
    let offset = bank_offset(index) as u64;
    write_at(drive, offset, image, |_, _| {})?;
    // Read the bank back and compare, so a bad write never goes unnoticed.
    verify_region(drive, offset, image, |_, _| {})
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
    nix::list_usb_drives()
}

/// Read the 1.44MB FAT12 image of bank `index` directly from the drive.
pub fn read_bank(drive: &UsbDrive, index: u32) -> Result<Vec<u8>> {
    read_region(drive, bank_offset(index) as u64, FLOPPY_IMAGE_BYTES, |_, _| {})
}

/// Read `len` bytes from the start of the drive (for backing up the USB).
pub fn read_range(drive: &UsbDrive, len: usize, on_progress: impl Fn(u64, u64)) -> Result<Vec<u8>> {
    read_region(drive, 0, len, on_progress)
}

/// Read `len` bytes starting at `offset` (sector-aligned) from the drive.
#[cfg(windows)]
pub fn read_region(
    drive: &UsbDrive,
    offset: u64,
    len: usize,
    on_progress: impl Fn(u64, u64),
) -> Result<Vec<u8>> {
    use windows::Win32::Foundation::{CloseHandle, GENERIC_READ};
    use windows::Win32::Storage::FileSystem::{ReadFile, SetFilePointerEx, FILE_BEGIN};

    unsafe {
        let handle = win::open_physical(&drive.path, GENERIC_READ.0)?;
        if let Err(e) = SetFilePointerEx(handle, offset as i64, None, FILE_BEGIN) {
            let _ = CloseHandle(handle);
            return Err(e.into());
        }
        let mut buf = vec![0u8; len];
        let mut pos = 0usize;
        while pos < len {
            let want = (len - pos).min(1024 * 1024);
            let mut read = 0u32;
            if ReadFile(handle, Some(&mut buf[pos..pos + want]), Some(&mut read), None).is_err()
                || read == 0
            {
                let _ = CloseHandle(handle);
                bail!("read failed at offset {}", offset + pos as u64);
            }
            pos += read as usize;
            on_progress(pos as u64, len as u64);
        }
        let _ = CloseHandle(handle);
        Ok(buf)
    }
}

/// Read `len` bytes starting at `offset` (sector-aligned) from the drive.
#[cfg(not(windows))]
pub fn read_region(
    drive: &UsbDrive,
    offset: u64,
    len: usize,
    on_progress: impl Fn(u64, u64),
) -> Result<Vec<u8>> {
    nix::read_at(&drive.path, offset, len, on_progress)
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
pub fn read_bank_headers(drive: &UsbDrive, count: u32) -> Result<Vec<Vec<u8>>> {
    let mut out = Vec::new();
    for i in 0..count {
        match nix::read_at(&drive.path, bank_offset(i) as u64, 512, |_, _| {}) {
            Ok(h) => out.push(h),
            Err(_) => break, // end of device
        }
    }
    Ok(out)
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
            bail!(
                "safety check failed: opened device is not drive {}",
                drive.index
            );
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
fn write_at(
    drive: &UsbDrive,
    offset: u64,
    data: &[u8],
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    nix::write_at(drive, offset, data, on_progress)
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

// ----------------------------------------------------------------------------
// Pure helpers used by the Linux/macOS backend (kept platform-independent so
// they are unit-tested everywhere).
// ----------------------------------------------------------------------------

/// True if `dev` is `disk` itself or one of its partitions
/// (`/dev/sdb` → `/dev/sdb1`, `/dev/mmcblk0` → `/dev/mmcblk0p1`).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn is_partition_of(dev: &str, disk: &str) -> bool {
    if dev == disk {
        return true;
    }
    match dev.strip_prefix(disk) {
        Some(rest) => {
            let rest = rest.strip_prefix('p').unwrap_or(rest);
            !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit())
        }
        None => false,
    }
}

/// Byte size from `diskutil info` output ("Disk Size: 15.4 GB (15376318464 Bytes) …").
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn parse_diskutil_bytes(info: &str) -> u64 {
    info.lines()
        .map(str::trim)
        .find(|l| l.starts_with("Disk Size") || l.starts_with("Total Size"))
        .and_then(|l| l.split('(').nth(1))
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

// ----------------------------------------------------------------------------
// Linux / macOS implementation
// ----------------------------------------------------------------------------

#[cfg(not(windows))]
mod nix {
    use std::fs::{File, OpenOptions};
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::process::Command;

    use anyhow::{bail, Context, Result};

    use super::UsbDrive;

    pub fn is_root() -> bool {
        Command::new("id")
            .arg("-u")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
            .unwrap_or(false)
    }

    pub fn read_at(
        path: &str,
        offset: u64,
        len: usize,
        on_progress: impl Fn(u64, u64),
    ) -> Result<Vec<u8>> {
        let mut f = File::open(path).with_context(|| format!("opening {path} (run as root?)"))?;
        f.seek(SeekFrom::Start(offset))?;
        let mut buf = vec![0u8; len];
        let mut pos = 0usize;
        while pos < len {
            let want = (len - pos).min(1024 * 1024);
            let n = f.read(&mut buf[pos..pos + want])?;
            if n == 0 {
                bail!("short read: {pos} of {len} bytes");
            }
            pos += n;
            on_progress(pos as u64, len as u64);
        }
        Ok(buf)
    }

    pub fn write_at(
        drive: &UsbDrive,
        offset: u64,
        data: &[u8],
        on_progress: impl Fn(u64, u64),
    ) -> Result<()> {
        if !data.len().is_multiple_of(512) {
            bail!("image size {} is not a multiple of 512", data.len());
        }
        // Re-verify right before writing that the path is still a removable drive.
        if !list_usb_drives()?.iter().any(|d| d.path == drive.path) {
            bail!(
                "safety check failed: {} is no longer a removable drive",
                drive.path
            );
        }
        unmount(drive)?;
        let mut f = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&drive.path)
            .with_context(|| format!("opening {} for writing (run as root?)", drive.path))?;
        f.seek(SeekFrom::Start(offset))?;
        let total = data.len() as u64;
        let mut done = 0u64;
        for chunk in data.chunks(1024 * 1024) {
            f.write_all(chunk)?;
            done += chunk.len() as u64;
            on_progress(done, total);
        }
        f.sync_all()?;
        Ok(())
    }

    /// Linux: removable / USB block devices from sysfs.
    #[cfg(target_os = "linux")]
    pub fn list_usb_drives() -> Result<Vec<UsbDrive>> {
        let mut names: Vec<String> = std::fs::read_dir("/sys/block")?
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        let mut out = Vec::new();
        for (i, name) in names.iter().enumerate() {
            let base = format!("/sys/block/{name}");
            let read = |p: &str| {
                std::fs::read_to_string(format!("{base}/{p}"))
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            };
            let removable = read("removable") == "1";
            let usb = std::fs::canonicalize(format!("{base}/device"))
                .map(|p| p.to_string_lossy().contains("/usb"))
                .unwrap_or(false);
            let sectors: u64 = read("size").parse().unwrap_or(0); // always 512-byte units
            if !(removable || usb) || sectors == 0 {
                continue;
            }
            let model = format!("{} {}", read("device/vendor"), read("device/model"))
                .trim()
                .to_string();
            out.push(UsbDrive {
                index: i as u32,
                path: format!("/dev/{name}"),
                model,
                size_bytes: sectors * 512,
                removable: true,
            });
        }
        Ok(out)
    }

    #[cfg(target_os = "linux")]
    fn unmount(drive: &UsbDrive) -> Result<()> {
        let mounts = std::fs::read_to_string("/proc/mounts").unwrap_or_default();
        for dev in mounts.lines().filter_map(|l| l.split_whitespace().next()) {
            if super::is_partition_of(dev, &drive.path) {
                let ok = Command::new("umount").arg(dev).status()?.success();
                if !ok {
                    bail!("could not unmount {dev}");
                }
            }
        }
        Ok(())
    }

    /// macOS: external physical disks from `diskutil`; raw I/O via /dev/rdiskN.
    #[cfg(target_os = "macos")]
    pub fn list_usb_drives() -> Result<Vec<UsbDrive>> {
        let out = Command::new("diskutil")
            .args(["list", "external", "physical"])
            .output()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let mut drives = Vec::new();
        for line in text.lines() {
            let Some(dev) = line.split_whitespace().next() else {
                continue;
            };
            let Some(n) = dev
                .strip_prefix("/dev/disk")
                .and_then(|s| s.parse::<u32>().ok())
            else {
                continue;
            };
            let info = Command::new("diskutil").args(["info", dev]).output()?;
            let info = String::from_utf8_lossy(&info.stdout);
            let model = info
                .lines()
                .map(str::trim)
                .find_map(|l| l.strip_prefix("Device / Media Name:"))
                .unwrap_or("")
                .trim()
                .to_string();
            drives.push(UsbDrive {
                index: n,
                path: format!("/dev/rdisk{n}"),
                model,
                size_bytes: super::parse_diskutil_bytes(&info),
                removable: true,
            });
        }
        Ok(drives)
    }

    #[cfg(target_os = "macos")]
    fn unmount(drive: &UsbDrive) -> Result<()> {
        let disk = drive.path.replace("/dev/rdisk", "/dev/disk");
        let ok = Command::new("diskutil")
            .args(["unmountDisk", &disk])
            .status()?
            .success();
        if !ok {
            bail!("could not unmount {disk}");
        }
        Ok(())
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub fn list_usb_drives() -> Result<Vec<UsbDrive>> {
        bail!("drive enumeration is not implemented on this OS")
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    fn unmount(_drive: &UsbDrive) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{is_partition_of, parse_diskutil_bytes};

    #[test]
    fn partition_matching() {
        assert!(is_partition_of("/dev/sdb", "/dev/sdb"));
        assert!(is_partition_of("/dev/sdb1", "/dev/sdb"));
        assert!(is_partition_of("/dev/mmcblk0p2", "/dev/mmcblk0"));
        assert!(!is_partition_of("/dev/sdba", "/dev/sdb"));
        assert!(!is_partition_of("/dev/sdc1", "/dev/sdb"));
        assert!(!is_partition_of("/dev/sdbp", "/dev/sdb"));
    }

    #[test]
    fn diskutil_size() {
        let info = "   Device / Media Name:      SanDisk Cruzer\n   Disk Size:                 15.4 GB (15376318464 Bytes) (exactly 30031872 512-Byte-Units)\n";
        assert_eq!(parse_diskutil_bytes(info), 15_376_318_464);
        assert_eq!(parse_diskutil_bytes("nothing"), 0);
    }
}
