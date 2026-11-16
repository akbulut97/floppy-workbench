//! Tests for the file-based block manager (a "floppy set" = a directory of
//! .img files on a USB drive, Gotek/FlashFloppy style).

use fat12::FloppyFormat;
use floppyctl::blockset::{BlockSet, Naming};

#[test]
fn created_block_file_follows_naming() {
    let dir = tempfile::tempdir().unwrap();
    let set = BlockSet::new(dir.path(), Naming::Gotek);
    let path = set.create_block(0, FloppyFormat::F1_44M, "DISK0", 0).unwrap();
    assert_eq!(path.file_name().unwrap(), "DSKA0000.IMG");
    assert!(path.exists());
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 1_474_560);
}

#[test]
fn ipcas_naming_scheme() {
    let dir = tempfile::tempdir().unwrap();
    let set = BlockSet::new(dir.path(), Naming::Ipcas);
    let path = set.create_block(7, FloppyFormat::F720K, "X", 0).unwrap();
    assert_eq!(path.file_name().unwrap(), "FLOPPY007.IMG");
}

#[test]
fn list_returns_blocks_sorted_with_info() {
    let dir = tempfile::tempdir().unwrap();
    let set = BlockSet::new(dir.path(), Naming::Gotek);
    set.create_block(2, FloppyFormat::F1_44M, "THIRD", 0).unwrap();
    set.create_block(0, FloppyFormat::F1_44M, "FIRST", 0).unwrap();
    set.create_block(1, FloppyFormat::F720K, "SECOND", 0).unwrap();

    let blocks = set.list().unwrap();
    assert_eq!(blocks.len(), 3);
    assert_eq!(blocks[0].index, 0);
    assert_eq!(blocks[0].info.label, "FIRST");
    assert_eq!(blocks[1].index, 1);
    assert_eq!(blocks[1].info.label, "SECOND");
    assert_eq!(blocks[2].index, 2);
    assert_eq!(blocks[2].info.label, "THIRD");
}

#[test]
fn empty_set_lists_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let set = BlockSet::new(dir.path(), Naming::Gotek);
    assert!(set.list().unwrap().is_empty());
}
