//! Tests for reading a bank's contents (file listing) and slicing a bank out
//! of a full raw USB image.

use fat12::FloppyFormat;
use floppyctl::sfr::{bank_offset, build_usb_image, extract_bank, BankSpec, FLOPPY_IMAGE_BYTES};
use floppyctl::{build_image, list_fat_files};

#[test]
fn lists_files_in_a_bank_image() {
    let files = vec![
        ("HELLO.TXT".to_string(), b"hi".to_vec()),
        ("DATA.BIN".to_string(), vec![0u8; 1000]),
    ];
    let img = build_image(FloppyFormat::F1_44M, "DISK", 0, &files).unwrap();

    let listed = list_fat_files(&img).unwrap();
    let names: Vec<_> = listed.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"HELLO.TXT"));
    assert!(names.contains(&"DATA.BIN"));

    let data = listed.iter().find(|f| f.name == "DATA.BIN").unwrap();
    assert_eq!(data.size, 1000);
    assert!(!data.is_dir);
}

#[test]
fn boot_label_reads_label_from_sector() {
    let img = build_image(FloppyFormat::F1_44M, "MYVOL", 0, &[]).unwrap();
    assert_eq!(floppyctl::boot_label(&img[..512]), Some("MYVOL".to_string()));
}

#[test]
fn boot_label_none_for_blank_sector() {
    assert_eq!(floppyctl::boot_label(&[0u8; 512]), None);
}

#[test]
fn empty_bank_lists_no_files() {
    let img = build_image(FloppyFormat::F1_44M, "BLANK", 0, &[]).unwrap();
    assert!(list_fat_files(&img).unwrap().is_empty());
}

#[test]
fn extract_bank_returns_the_right_slice() {
    let banks = vec![BankSpec {
        index: 2,
        format: FloppyFormat::F1_44M,
        label: "THIRD".into(),
        files: vec![("A.TXT".into(), b"aaa".to_vec())],
    }];
    let raw = build_usb_image(5, &banks).unwrap();

    let bank2 = extract_bank(&raw, 2).unwrap();
    assert_eq!(bank2.len(), FLOPPY_IMAGE_BYTES);
    assert_eq!(bank2, &raw[bank_offset(2)..bank_offset(2) + FLOPPY_IMAGE_BYTES]);

    // And its file is listable.
    let files = list_fat_files(bank2).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].name, "A.TXT");
}

#[test]
fn extract_bank_out_of_range_errors() {
    let raw = build_usb_image(3, &[]).unwrap();
    assert!(extract_bank(&raw, 3).is_err());
}
