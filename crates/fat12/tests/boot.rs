//! Bootable-disk infrastructure: boot-code injection + file placement.

use std::io::{Cursor, Read};

use fat12::{add_file, format_image_with, make_bootable, FloppyFormat, FormatOptions, Geometry};
use fatfs::{FileSystem, FsOptions};

#[test]
fn boot_code_is_injected_but_bpb_preserved() {
    let code = [0x90u8; 64]; // dummy boot stub (NOPs)
    let opts = FormatOptions {
        label: "BOOT",
        serial: 0xDEADBEEF,
        boot_code: Some(&code),
    };
    let img = format_image_with(FloppyFormat::F1_44M, &opts);
    let g = Geometry::for_format(FloppyFormat::F1_44M);

    // Boot code lands at 0x3E, right after the extended BPB.
    assert_eq!(&img[0x3E..0x3E + 64], &code, "boot code at 0x3E");
    // BPB untouched + boot signature intact.
    assert_eq!(
        u16::from_le_bytes([img[0x0B], img[0x0C]]),
        g.bytes_per_sector,
        "bytes/sector still intact"
    );
    assert_eq!(img[510], 0x55);
    assert_eq!(img[511], 0xAA);
}

#[test]
fn default_options_produce_no_boot_code() {
    let opts = FormatOptions::default();
    let img = format_image_with(FloppyFormat::F1_44M, &opts);
    // Without boot code, the area after the BPB stays zeroed.
    assert!(img[0x3E..0x3E + 64].iter().all(|&b| b == 0));
}

#[test]
fn add_file_then_read_back() {
    let mut img = format_image_with(FloppyFormat::F1_44M, &FormatOptions::default());
    add_file(&mut img, "README.TXT", b"on-disk content").expect("add_file");

    let fs = FileSystem::new(Cursor::new(img), FsOptions::new()).expect("mount");
    let mut f = fs.root_dir().open_file("README.TXT").expect("open");
    let mut back = Vec::new();
    f.read_to_end(&mut back).unwrap();
    assert_eq!(back, b"on-disk content");
}

#[test]
fn set_label_changes_the_volume_label() {
    let mut img = fat12::format_image(FloppyFormat::F1_44M, "OLD", 0);
    fat12::set_label(&mut img, "NEWLABEL").unwrap();

    // Boot sector label field.
    assert_eq!(&img[0x2B..0x36], b"NEWLABEL   ");
    // Readable by an independent reader.
    let fs = FileSystem::new(Cursor::new(img), FsOptions::new()).unwrap();
    assert_eq!(fs.volume_label().trim_end(), "NEWLABEL");
}

#[test]
fn remove_file_deletes_it() {
    let mut img = format_image_with(FloppyFormat::F1_44M, &FormatOptions::default());
    add_file(&mut img, "KEEP.TXT", b"keep").unwrap();
    add_file(&mut img, "GONE.TXT", b"bye").unwrap();

    fat12::remove_file(&mut img, "GONE.TXT").unwrap();

    let fs = FileSystem::new(Cursor::new(img), FsOptions::new()).unwrap();
    assert!(
        fs.root_dir().open_file("GONE.TXT").is_err(),
        "deleted file is gone"
    );
    assert!(
        fs.root_dir().open_file("KEEP.TXT").is_ok(),
        "other file remains"
    );
}

#[test]
fn make_bootable_places_system_files_and_boot_code() {
    // Stand-in for real FreeDOS binaries (KERNEL.SYS + COMMAND.COM).
    let boot_code = [0xEBu8, 0xFE]; // jmp $ — minimal valid stub
    let kernel = vec![0xAAu8; 2048];
    let command = vec![0xBBu8; 4096];

    let img = make_bootable(
        FloppyFormat::F1_44M,
        "DOSBOOT",
        0x1234,
        &boot_code,
        &[
            ("KERNEL.SYS", kernel.as_slice()),
            ("COMMAND.COM", command.as_slice()),
        ],
    )
    .expect("make_bootable");

    // Boot code present.
    assert_eq!(&img[0x3E..0x3E + 2], &boot_code);

    // Both system files mountable and byte-identical.
    let fs = FileSystem::new(Cursor::new(img), FsOptions::new()).expect("mount");
    for (name, expected) in [("KERNEL.SYS", &kernel), ("COMMAND.COM", &command)] {
        let mut f = fs
            .root_dir()
            .open_file(name)
            .unwrap_or_else(|_| panic!("open {name}"));
        let mut back = Vec::new();
        f.read_to_end(&mut back).unwrap();
        assert_eq!(&back, expected, "{name} contents");
    }
}
