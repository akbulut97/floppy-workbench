//! End-to-end validation: our formatter output must be mountable and usable
//! by an *independent* FAT implementation (the `fatfs` crate).

use std::io::{Cursor, Read, Write};

use fat12::{format_image, FloppyFormat};
use fatfs::{FileSystem, FsOptions};

fn mountable(format: FloppyFormat) {
    let img = format_image(format, "FLOPPY", 0xCAFEBABE);
    let cursor = Cursor::new(img);
    let fs = FileSystem::new(cursor, FsOptions::new())
        .unwrap_or_else(|e| panic!("mount failed for {format:?}: {e:?}"));
    assert_eq!(
        fs.fat_type(),
        fatfs::FatType::Fat12,
        "{format:?} must be FAT12"
    );
}

#[test]
fn mountable_720k() {
    mountable(FloppyFormat::F720K);
}

#[test]
fn mountable_1_2m() {
    mountable(FloppyFormat::F1_2M);
}

#[test]
fn mountable_1_44m() {
    mountable(FloppyFormat::F1_44M);
}

#[test]
fn write_then_read_file_round_trips() {
    let img = format_image(FloppyFormat::F1_44M, "FLOPPY", 0xCAFEBABE);
    let cursor = Cursor::new(img);
    let fs = FileSystem::new(cursor, FsOptions::new()).expect("mount");

    let payload = b"hello from a freshly formatted floppy";
    {
        let root = fs.root_dir();
        let mut f = root.create_file("HELLO.TXT").expect("create");
        f.write_all(payload).expect("write");
    }

    let root = fs.root_dir();
    let mut f = root.open_file("HELLO.TXT").expect("open");
    let mut back = Vec::new();
    f.read_to_end(&mut back).expect("read");
    assert_eq!(back, payload);
}

#[test]
fn volume_label_is_readable() {
    let img = format_image(FloppyFormat::F1_44M, "MYDISK", 0x11223344);
    let cursor = Cursor::new(img);
    let fs = FileSystem::new(cursor, FsOptions::new()).expect("mount");
    assert_eq!(fs.volume_label().trim_end(), "MYDISK");
}
