//! Tests for the SFR1M44-U100 raw USB image layout.
//!
//! Layout (from device reverse-engineering): the USB is a flat sequence of
//! 1.5 MiB "banks"; bank N lives at offset N * 0x180000, and the first
//! 1,474,560 bytes of each bank is a standard 1.44MB FAT12 floppy image.

use fat12::FloppyFormat;
use floppyctl::sfr::{bank_offset, build_usb_image, BankSpec, BANK_STRIDE, FLOPPY_IMAGE_BYTES};

#[test]
fn bank_offsets_follow_1_5_mib_stride() {
    assert_eq!(BANK_STRIDE, 0x180000);
    assert_eq!(FLOPPY_IMAGE_BYTES, 1_474_560);
    assert_eq!(bank_offset(0), 0);
    assert_eq!(bank_offset(1), 0x180000);
    assert_eq!(bank_offset(5), 5 * 0x180000);
}

#[test]
fn image_length_is_total_banks_times_stride() {
    let img = build_usb_image(10, &[]).unwrap();
    assert_eq!(img.len(), 10 * BANK_STRIDE);
}

#[test]
fn banks_land_at_correct_offsets_and_are_valid_fat12() {
    let banks = vec![
        BankSpec {
            index: 0,
            format: FloppyFormat::F1_44M,
            label: "FIRST".into(),
            files: vec![],
        },
        BankSpec {
            index: 2,
            format: FloppyFormat::F1_44M,
            label: "THIRD".into(),
            files: vec![("A.TXT".into(), b"aaa".to_vec())],
        },
    ];
    let img = build_usb_image(3, &banks).unwrap();
    assert_eq!(img.len(), 3 * BANK_STRIDE);

    // Bank 0 at offset 0.
    let b0 = &img[0..FLOPPY_IMAGE_BYTES];
    assert_eq!(floppyctl::inspect(b0).unwrap().label, "FIRST");

    // Bank 2 at offset 2*stride, with its file readable.
    let off2 = bank_offset(2);
    let b2 = &img[off2..off2 + FLOPPY_IMAGE_BYTES];
    assert_eq!(floppyctl::inspect(b2).unwrap().label, "THIRD");

    // Bank 1 (unused) region is all zero.
    let off1 = bank_offset(1);
    assert!(img[off1..off1 + BANK_STRIDE].iter().all(|&b| b == 0));
}

#[test]
fn parse_bank_spec_index_size_label() {
    let spec = floppyctl::parse_bank_spec("3:1.44:MYDISK").unwrap();
    assert_eq!(spec.index, 3);
    assert_eq!(spec.format, FloppyFormat::F1_44M);
    assert_eq!(spec.label, "MYDISK");
    assert!(spec.files.is_empty());
}

#[test]
fn parse_bank_spec_rejects_malformed() {
    assert!(floppyctl::parse_bank_spec("nope").is_err());
    assert!(floppyctl::parse_bank_spec("1:9.99:X").is_err());
}

#[test]
fn parse_add_spec_splits_on_first_colon() {
    // Windows paths contain colons, so only the first colon separates index.
    let (idx, path) = floppyctl::parse_add_spec("2:C:\\tmp\\file.txt").unwrap();
    assert_eq!(idx, 2);
    assert_eq!(path, "C:\\tmp\\file.txt");
}

#[test]
fn parse_add_spec_rejects_missing_path() {
    assert!(floppyctl::parse_add_spec("2").is_err());
    assert!(floppyctl::parse_add_spec("x:/p").is_err());
}

#[test]
fn attach_files_targets_matching_bank() {
    let mut specs = vec![
        BankSpec {
            index: 0,
            format: FloppyFormat::F1_44M,
            label: "A".into(),
            files: vec![],
        },
        BankSpec {
            index: 1,
            format: FloppyFormat::F1_44M,
            label: "B".into(),
            files: vec![],
        },
    ];
    floppyctl::attach_file(&mut specs, 1, "F.TXT".into(), b"data".to_vec()).unwrap();
    assert!(specs[0].files.is_empty());
    assert_eq!(
        specs[1].files,
        vec![("F.TXT".to_string(), b"data".to_vec())]
    );
}

#[test]
fn attach_files_errors_on_unknown_bank() {
    let mut specs = vec![BankSpec {
        index: 0,
        format: FloppyFormat::F1_44M,
        label: "A".into(),
        files: vec![],
    }];
    assert!(floppyctl::attach_file(&mut specs, 9, "F".into(), vec![]).is_err());
}

#[test]
fn rejects_bank_index_beyond_total() {
    let banks = vec![BankSpec {
        index: 5,
        format: FloppyFormat::F1_44M,
        label: "X".into(),
        files: vec![],
    }];
    assert!(build_usb_image(3, &banks).is_err());
}
