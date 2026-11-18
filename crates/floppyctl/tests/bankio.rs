//! Per-block image import/export and write verification (pure logic).

use floppyctl::device::first_mismatch;
use floppyctl::sfr::{normalize_bank_image, BANK_STRIDE, FLOPPY_IMAGE_BYTES};

#[test]
fn first_mismatch_none_when_equal() {
    assert_eq!(first_mismatch(b"abc", b"abc"), None);
}

#[test]
fn first_mismatch_reports_position() {
    assert_eq!(first_mismatch(b"abcd", b"abXd"), Some(2));
}

#[test]
fn first_mismatch_counts_length_difference() {
    assert_eq!(first_mismatch(b"ab", b"abc"), Some(2));
}

#[test]
fn accepts_exact_floppy_image() {
    let d = vec![1u8; FLOPPY_IMAGE_BYTES];
    assert_eq!(normalize_bank_image(&d).unwrap().len(), FLOPPY_IMAGE_BYTES);
}

#[test]
fn accepts_full_bank_dump_and_trims_padding() {
    let mut d = vec![1u8; BANK_STRIDE];
    d[FLOPPY_IMAGE_BYTES] = 9; // padding area
    let n = normalize_bank_image(&d).unwrap();
    assert_eq!(n.len(), FLOPPY_IMAGE_BYTES);
    assert!(n.iter().all(|&b| b == 1));
}

#[test]
fn rejects_other_sizes() {
    let err = normalize_bank_image(&vec![0u8; 737_280]).unwrap_err();
    assert!(err.to_string().contains("1.44"));
}
