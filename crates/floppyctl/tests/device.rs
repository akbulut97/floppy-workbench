//! Tests for USB device targeting safety checks (the pure validation logic).

use floppyctl::device::{check_write_target, UsbDrive};

fn drive(removable: bool, size: u64) -> UsbDrive {
    UsbDrive {
        index: 3,
        path: "\\\\.\\PhysicalDrive3".into(),
        model: "Generic USB".into(),
        size_bytes: size,
        removable,
    }
}

#[test]
fn accepts_removable_drive_that_fits() {
    assert!(check_write_target(&drive(true, 8_000_000_000), 150 * 1024 * 1024).is_ok());
}

#[test]
fn rejects_fixed_disk() {
    // Never allow writing to a non-removable (system) disk.
    let err = check_write_target(&drive(false, 8_000_000_000), 1024).unwrap_err();
    assert!(err.to_string().to_lowercase().contains("removable"));
}

#[test]
fn rejects_image_larger_than_drive() {
    let err = check_write_target(&drive(true, 1_000_000), 2_000_000).unwrap_err();
    assert!(err.to_string().to_lowercase().contains("larger") || err.to_string().contains("exceed"));
}

#[test]
fn rejects_empty_image() {
    assert!(check_write_target(&drive(true, 8_000_000_000), 0).is_err());
}
