//! Block integrity check: blank / ok / warning / corrupt.

use fat12::FloppyFormat;
use floppyctl::health::{check_image, Health};

fn fresh() -> Vec<u8> {
    floppyctl::build_image(
        FloppyFormat::F1_44M,
        "HEALTHY",
        7,
        &[("A.TXT".into(), b"hello".to_vec())],
    )
    .unwrap()
}

/// Byte offset of FAT copy `n` (0 or 1) in a 1.44M image: 1 reserved sector, 9 sectors per FAT.
fn fat_offset(n: usize) -> usize {
    512 + n * 9 * 512
}

#[test]
fn all_zero_bank_is_blank() {
    let r = check_image(&vec![0u8; 1_474_560]);
    assert_eq!(r.status, Health::Blank);
    assert!(r.issues.is_empty());
}

#[test]
fn freshly_formatted_bank_is_ok_with_usage() {
    let r = check_image(&fresh());
    assert_eq!(r.status, Health::Ok, "issues: {:?}", r.issues);
    assert_eq!(r.label, "HEALTHY");
    assert_eq!(r.files, 1);
    assert!(r.used_bytes > 0 && r.total_bytes > r.used_bytes);
}

#[test]
fn folders_and_nested_files_are_counted() {
    let mut img = fresh();
    floppyctl::fsops::make_dir(&mut img, "DIR").unwrap();
    floppyctl::fsops::add_file_at(&mut img, "DIR", "B.BIN", &[1, 2, 3]).unwrap();
    let r = check_image(&img);
    assert_eq!(r.status, Health::Ok, "issues: {:?}", r.issues);
    assert_eq!(r.files, 2);
}

#[test]
fn differing_fat_copies_are_a_warning() {
    let mut img = fresh();
    // Mark a far-away (unused) cluster as end-of-chain only in the second FAT.
    let off = fat_offset(1) + 600;
    img[off] = 0xFF;
    img[off + 1] = 0xFF;
    let r = check_image(&img);
    assert_eq!(r.status, Health::Warning, "issues: {:?}", r.issues);
    assert!(r.issues.iter().any(|i| i.contains("FAT copies differ")));
}

#[test]
fn garbage_without_boot_signature_is_corrupt() {
    let mut img = vec![0u8; 1_474_560];
    img[100] = 0x42;
    let r = check_image(&img);
    assert_eq!(r.status, Health::Corrupt);
    assert!(!r.issues.is_empty());
}

#[test]
fn invalid_bpb_is_corrupt() {
    let mut img = fresh();
    img[11] = 0x00; // bytes per sector = 0x0200 -> 0x0200 with low byte 0 is still 512…
    img[12] = 0x03; // …so set high byte too: 0x0300 = 768, not a valid sector size
    let r = check_image(&img);
    assert_eq!(r.status, Health::Corrupt, "issues: {:?}", r.issues);
}

#[test]
fn out_of_range_cluster_pointer_is_corrupt() {
    let mut img = fresh();
    // A.TXT occupies cluster 2 (end of chain). Point it at cluster 0xFF0 (beyond the disk)
    // in both FAT copies. Cluster 2's entry is at byte 3 (low byte) and the low nibble of byte 4.
    for n in 0..2 {
        let off = fat_offset(n);
        img[off + 3] = 0xF0;
        img[off + 4] = (img[off + 4] & 0xF0) | 0x0F;
    }
    let r = check_image(&img);
    assert_eq!(r.status, Health::Corrupt, "issues: {:?}", r.issues);
}

#[test]
fn freedos_boot_disk_and_other_sizes_are_ok() {
    let boot = floppyctl::freedos::make_bootable_image("FREEDOS", 1).unwrap();
    let r = check_image(&boot);
    assert_eq!(r.status, Health::Ok, "issues: {:?}", r.issues);
    assert!(r.files >= 2);
    for f in [FloppyFormat::F720K, FloppyFormat::F1_2M] {
        let mut img = floppyctl::build_image(f, "SMALL", 1, &[]).unwrap();
        img.resize(1_474_560, 0); // as stored in a bank
        let r = check_image(&img);
        assert_eq!(r.status, Health::Ok, "{f:?}: {:?}", r.issues);
    }
}
