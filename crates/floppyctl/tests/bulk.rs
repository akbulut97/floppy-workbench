//! Bulk .img helpers and the bank store (drive-or-image-file) abstraction.

use std::path::PathBuf;

use fat12::FloppyFormat;
use floppyctl::bulk::{bank_file_name, natural_cmp, plan_import, sort_natural};
use floppyctl::sfr::{BANK_STRIDE, FLOPPY_IMAGE_BYTES};
use floppyctl::store::{BankStore, ImageFileStore};

#[test]
fn bank_file_names_are_zero_padded() {
    assert_eq!(bank_file_name(0), "BLOCK000.img");
    assert_eq!(bank_file_name(42), "BLOCK042.img");
    assert_eq!(bank_file_name(999), "BLOCK999.img");
    assert_eq!(bank_file_name(1000), "BLOCK1000.img");
}

#[test]
fn natural_order_compares_numbers_by_value() {
    use std::cmp::Ordering::*;
    assert_eq!(natural_cmp("disk2.img", "disk10.img"), Less);
    assert_eq!(natural_cmp("DISK10.img", "disk9.img"), Greater);
    assert_eq!(natural_cmp("a.img", "B.img"), Less);
    assert_eq!(natural_cmp("x1", "x1"), Equal);
}

#[test]
fn sort_natural_uses_file_names_only() {
    let mut v = vec![
        PathBuf::from("/z/disk10.img"),
        PathBuf::from("/a/disk2.img"),
        PathBuf::from("/m/disk1.img"),
    ];
    sort_natural(&mut v);
    let names: Vec<_> = v
        .iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap())
        .collect();
    assert_eq!(names, ["disk1.img", "disk2.img", "disk10.img"]);
}

#[test]
fn plan_import_targets_consecutive_blocks() {
    let plan = plan_import(5, 3, 100).unwrap();
    assert_eq!(plan, vec![5, 6, 7]);
}

#[test]
fn plan_import_refuses_to_run_past_the_last_block() {
    assert!(plan_import(98, 3, 100).is_err());
    assert!(plan_import(0, 0, 100).is_err());
}

fn usb_file(banks: u32) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("usb.img");
    std::fs::write(&path, vec![0u8; banks as usize * BANK_STRIDE]).unwrap();
    (dir, path)
}

#[test]
fn image_store_reads_and_writes_banks_in_place() {
    let (_d, path) = usb_file(4);
    let store = ImageFileStore::open(&path).unwrap();
    assert_eq!(store.bank_count(), 4);

    let img = floppyctl::build_image(FloppyFormat::F1_44M, "STORE", 1, &[]).unwrap();
    store.write_bank(2, &img).unwrap();
    assert_eq!(store.read_bank(2).unwrap(), img);
    assert_eq!(store.read_bank(1).unwrap(), vec![0u8; FLOPPY_IMAGE_BYTES]);

    // The file size is unchanged and the padding after the floppy stays zero.
    let raw = std::fs::read(&path).unwrap();
    assert_eq!(raw.len(), 4 * BANK_STRIDE);
    assert!(raw[2 * BANK_STRIDE + FLOPPY_IMAGE_BYTES..3 * BANK_STRIDE]
        .iter()
        .all(|&b| b == 0));
}

#[test]
fn image_store_rejects_out_of_range_banks() {
    let (_d, path) = usb_file(2);
    let store = ImageFileStore::open(&path).unwrap();
    assert!(store.read_bank(2).is_err());
    assert!(store.write_bank(5, &[0u8; FLOPPY_IMAGE_BYTES]).is_err());
}

// ---- Block operations against an image-file store ----

use floppyctl::bulk::{check_banks, clone_bank, export_banks, import_images, search_banks};
use floppyctl::health::Health;

fn floppy(label: &str, files: &[(&str, &[u8])]) -> Vec<u8> {
    let files: Vec<(String, Vec<u8>)> = files
        .iter()
        .map(|(n, d)| (n.to_string(), d.to_vec()))
        .collect();
    floppyctl::build_image(FloppyFormat::F1_44M, label, 1, &files).unwrap()
}

#[test]
fn export_then_import_round_trips_blocks() {
    let (dir, path) = usb_file(6);
    let store = ImageFileStore::open(&path).unwrap();
    store.write_bank(0, &floppy("ZERO", &[])).unwrap();
    store
        .write_bank(1, &floppy("ONE", &[("X.TXT", b"x")]))
        .unwrap();

    let out = dir.path().join("out");
    let files = export_banks(&store, &[0, 1], &out, &|_, _| {}).unwrap();
    let names: Vec<_> = files
        .iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
        .collect();
    assert_eq!(names, ["BLOCK000.img", "BLOCK001.img"]);

    // Import them (given in the "wrong" order) into blocks 3 and 4.
    let mut paths = vec![files[1].clone(), files[0].clone()];
    let written = import_images(&store, 3, &mut paths, &|_, _| {}).unwrap();
    assert_eq!(written, vec![3, 4]);
    assert_eq!(store.read_bank(3).unwrap(), store.read_bank(0).unwrap());
    assert_eq!(store.read_bank(4).unwrap(), store.read_bank(1).unwrap());
}

#[test]
fn import_checks_every_image_before_writing_anything() {
    let (dir, path) = usb_file(4);
    let store = ImageFileStore::open(&path).unwrap();
    let good = dir.path().join("a1.img");
    let bad = dir.path().join("a2.img");
    std::fs::write(&good, floppy("GOOD", &[])).unwrap();
    std::fs::write(&bad, b"not a floppy").unwrap();
    let err = import_images(&store, 0, &mut [good, bad], &|_, _| {}).unwrap_err();
    assert!(err.to_string().contains("a2.img"), "{err}");
    // Block 0 must be untouched.
    assert_eq!(store.read_bank(0).unwrap(), vec![0u8; FLOPPY_IMAGE_BYTES]);
}

#[test]
fn import_refuses_when_images_do_not_fit() {
    let (dir, path) = usb_file(2);
    let store = ImageFileStore::open(&path).unwrap();
    let mut paths = Vec::new();
    for i in 0..2 {
        let p = dir.path().join(format!("d{i}.img"));
        std::fs::write(&p, floppy("D", &[])).unwrap();
        paths.push(p);
    }
    assert!(import_images(&store, 1, &mut paths, &|_, _| {}).is_err());
}

#[test]
fn clone_copies_a_block_and_refuses_same_block() {
    let (_d, path) = usb_file(3);
    let store = ImageFileStore::open(&path).unwrap();
    store.write_bank(0, &floppy("SRC", &[("A", b"a")])).unwrap();
    clone_bank(&store, 0, 2).unwrap();
    assert_eq!(store.read_bank(2).unwrap(), store.read_bank(0).unwrap());
    assert!(clone_bank(&store, 1, 1).is_err());
}

#[test]
fn search_and_check_scan_every_block() {
    let (_d, path) = usb_file(4);
    let store = ImageFileStore::open(&path).unwrap();
    store
        .write_bank(0, &floppy("A", &[("REPORT.TXT", b"r")]))
        .unwrap();
    store
        .write_bank(2, &floppy("B", &[("OLDREPORT.DOC", b"o"), ("X.BIN", b"x")]))
        .unwrap();
    let mut corrupt = floppy("C", &[]);
    corrupt[510] = 0; // break the boot signature
    store.write_bank(3, &corrupt).unwrap();

    let hits = search_banks(&store, 4, "report", &|_, _| {}).unwrap();
    let found: Vec<_> = hits.iter().map(|h| (h.block, h.path.as_str())).collect();
    assert_eq!(found, [(0, "REPORT.TXT"), (2, "OLDREPORT.DOC")]);

    let health = check_banks(&store, &[0, 1, 2, 3], &|_, _| {}).unwrap();
    let st: Vec<_> = health.iter().map(|(i, r)| (*i, r.status)).collect();
    assert_eq!(
        st,
        [
            (0, Health::Ok),
            (1, Health::Blank),
            (2, Health::Ok),
            (3, Health::Corrupt)
        ]
    );
}
