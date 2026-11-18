//! Numbered block labels used when formatting a whole USB (FLOPPY0, FLOPPY1, …).

use floppyctl::numbered_label;
use floppyctl::sfr::{build_usb_image, extract_bank, BankSpec};

#[test]
fn prefix_plus_number_uppercased() {
    assert_eq!(numbered_label("Floppy", 0), "FLOPPY0");
    assert_eq!(numbered_label("floppy", 1), "FLOPPY1");
    assert_eq!(numbered_label("FLoppy", 99), "FLOPPY99");
}

#[test]
fn empty_prefix_is_no_name() {
    assert_eq!(numbered_label("", 5), "NO NAME");
    assert_eq!(numbered_label("   ", 5), "NO NAME");
}

#[test]
fn long_prefix_is_shortened_but_number_kept() {
    assert_eq!(numbered_label("VERYLONGNAME", 7), "VERYLONGNA7");
    assert_eq!(numbered_label("VERYLONGNAME", 999), "VERYLONG999");
    assert!(numbered_label("VERYLONGNAME", 999).len() <= 11);
    // Trailing space left by truncation is dropped.
    assert_eq!(numbered_label("DISK A BCDEF", 12345), "DISK A12345");
}

#[test]
fn forbidden_characters_replaced() {
    assert_eq!(numbered_label("a.b/c", 1), "A_B_C1");
    assert_eq!(numbered_label("Disket", 3), "DISKET3");
    assert_eq!(numbered_label("Çalışma", 2), "_AL__MA2");
}

#[test]
fn labels_written_into_banks() {
    let fmt = floppyctl::parse_format("1.44").unwrap();
    let specs: Vec<BankSpec> = (0..3)
        .map(|i| BankSpec {
            index: i,
            format: fmt,
            label: numbered_label("Floppy", i),
            files: vec![],
        })
        .collect();
    let img = build_usb_image(3, &specs).unwrap();
    for i in 0..3 {
        let bank = extract_bank(&img, i).unwrap();
        let info = floppyctl::inspect(bank).unwrap();
        assert_eq!(info.label, format!("FLOPPY{i}"));
    }
}
