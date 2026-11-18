//! Tests for the CLI's core logic (parsing + image building), independent of
//! argument plumbing and the filesystem.

use std::io::{Cursor, Read};

use fat12::FloppyFormat;
use fatfs::{FileSystem, FsOptions};

#[test]
fn parse_format_accepts_known_sizes() {
    assert_eq!(floppyctl::parse_format("720").unwrap(), FloppyFormat::F720K);
    assert_eq!(floppyctl::parse_format("1.2").unwrap(), FloppyFormat::F1_2M);
    assert_eq!(
        floppyctl::parse_format("1.44").unwrap(),
        FloppyFormat::F1_44M
    );
}

#[test]
fn parse_naming_known_and_unknown() {
    use floppyctl::blockset::Naming;
    assert_eq!(floppyctl::parse_naming("gotek").unwrap(), Naming::Gotek);
    assert_eq!(floppyctl::parse_naming("ipcas").unwrap(), Naming::Ipcas);
    assert!(floppyctl::parse_naming("weird").is_err());
}

#[test]
fn parse_format_rejects_unknown() {
    assert!(floppyctl::parse_format("2.88").is_err());
    assert!(floppyctl::parse_format("nonsense").is_err());
}

#[test]
fn build_image_is_mountable_and_labeled() {
    let img = floppyctl::build_image(FloppyFormat::F1_44M, "CLIDISK", 0x1234, &[]).unwrap();
    let fs = FileSystem::new(Cursor::new(img), FsOptions::new()).unwrap();
    assert_eq!(fs.fat_type(), fatfs::FatType::Fat12);
    assert_eq!(fs.volume_label().trim_end(), "CLIDISK");
}

#[test]
fn inspect_reports_label_and_space() {
    let img = floppyctl::build_image(FloppyFormat::F1_44M, "INFODISK", 7, &[]).unwrap();
    let info = floppyctl::inspect(&img).unwrap();
    assert_eq!(info.label, "INFODISK");
    assert_eq!(info.fat_type, "FAT12");
    assert_eq!(info.total_bytes, info.used_bytes + info.free_bytes);
    assert!(
        info.free_bytes > 1_000_000,
        "a blank 1.44M disk is mostly free"
    );
}

#[test]
fn inspect_used_grows_after_adding_file() {
    let empty =
        floppyctl::inspect(&floppyctl::build_image(FloppyFormat::F1_44M, "D", 1, &[]).unwrap())
            .unwrap();
    let filled = floppyctl::inspect(
        &floppyctl::build_image(
            FloppyFormat::F1_44M,
            "D",
            1,
            &[("BIG.BIN".to_string(), vec![0u8; 50_000])],
        )
        .unwrap(),
    )
    .unwrap();
    assert!(filled.used_bytes > empty.used_bytes);
}

#[test]
fn build_image_includes_added_files() {
    let files = vec![("HELLO.TXT".to_string(), b"hi there".to_vec())];
    let img = floppyctl::build_image(FloppyFormat::F720K, "DISK", 1, &files).unwrap();
    let fs = FileSystem::new(Cursor::new(img), FsOptions::new()).unwrap();
    let mut f = fs.root_dir().open_file("HELLO.TXT").unwrap();
    let mut back = Vec::new();
    f.read_to_end(&mut back).unwrap();
    assert_eq!(back, b"hi there");
}
