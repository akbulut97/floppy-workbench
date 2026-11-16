//! Explorer-style filesystem operations on a FAT12 image: navigate, create
//! folders, add/delete/rename/read at arbitrary paths.

use fat12::FloppyFormat;
use floppyctl::build_image;
use floppyctl::fsops::{
    add_file_at, list_dir, make_dir, read_file_at, read_tree, remove_path, rename_path, write_tree,
};

fn blank() -> Vec<u8> {
    build_image(FloppyFormat::F1_44M, "DISK", 0, &[]).unwrap()
}

fn names(img: &[u8], path: &str) -> Vec<String> {
    let mut v: Vec<String> = list_dir(img, path).unwrap().into_iter().map(|e| e.name).collect();
    v.sort();
    v
}

#[test]
fn make_dir_and_list_root() {
    let mut img = blank();
    make_dir(&mut img, "DOCS").unwrap();
    assert_eq!(names(&img, ""), vec!["DOCS"]);
    let docs = &list_dir(&img, "").unwrap()[0];
    assert!(docs.is_dir);
}

#[test]
fn add_file_into_subdirectory() {
    let mut img = blank();
    make_dir(&mut img, "DOCS").unwrap();
    add_file_at(&mut img, "DOCS", "A.TXT", b"hello").unwrap();
    assert_eq!(names(&img, "DOCS"), vec!["A.TXT"]);
    assert_eq!(read_file_at(&img, "DOCS/A.TXT").unwrap(), b"hello");
}

#[test]
fn nested_directories() {
    let mut img = blank();
    make_dir(&mut img, "A").unwrap();
    make_dir(&mut img, "A/B").unwrap();
    add_file_at(&mut img, "A/B", "DEEP.TXT", b"deep").unwrap();
    assert_eq!(names(&img, "A/B"), vec!["DEEP.TXT"]);
}

#[test]
fn remove_single_file() {
    let mut img = blank();
    add_file_at(&mut img, "", "A.TXT", b"x").unwrap();
    remove_path(&mut img, "A.TXT").unwrap();
    assert!(names(&img, "").is_empty());
}

#[test]
fn remove_directory_recursively() {
    let mut img = blank();
    make_dir(&mut img, "A").unwrap();
    make_dir(&mut img, "A/B").unwrap();
    add_file_at(&mut img, "A", "F1.TXT", b"1").unwrap();
    add_file_at(&mut img, "A/B", "F2.TXT", b"2").unwrap();

    remove_path(&mut img, "A").unwrap();
    assert!(names(&img, "").is_empty(), "the whole tree is gone");
}

#[test]
fn rename_a_file() {
    let mut img = blank();
    add_file_at(&mut img, "", "OLD.TXT", b"data").unwrap();
    rename_path(&mut img, "OLD.TXT", "NEW.TXT").unwrap();
    assert_eq!(names(&img, ""), vec!["NEW.TXT"]);
    assert_eq!(read_file_at(&img, "NEW.TXT").unwrap(), b"data");
}

#[test]
fn copy_a_file_via_tree() {
    let mut img = blank();
    add_file_at(&mut img, "", "A.TXT", b"payload").unwrap();
    let tree = read_tree(&img, "A.TXT").unwrap();
    write_tree(&mut img, "COPY.TXT", &tree).unwrap();
    assert_eq!(read_file_at(&img, "COPY.TXT").unwrap(), b"payload");
    assert_eq!(read_file_at(&img, "A.TXT").unwrap(), b"payload");
}

#[test]
fn copy_a_folder_tree() {
    let mut img = blank();
    make_dir(&mut img, "SRC").unwrap();
    make_dir(&mut img, "SRC/SUB").unwrap();
    add_file_at(&mut img, "SRC", "F1.TXT", b"1").unwrap();
    add_file_at(&mut img, "SRC/SUB", "F2.TXT", b"2").unwrap();

    let tree = read_tree(&img, "SRC").unwrap();
    write_tree(&mut img, "DST", &tree).unwrap();

    assert_eq!(read_file_at(&img, "DST/F1.TXT").unwrap(), b"1");
    assert_eq!(read_file_at(&img, "DST/SUB/F2.TXT").unwrap(), b"2");
    // source untouched
    assert_eq!(read_file_at(&img, "SRC/F1.TXT").unwrap(), b"1");
}

#[test]
fn rename_into_another_folder() {
    let mut img = blank();
    make_dir(&mut img, "DEST").unwrap();
    add_file_at(&mut img, "", "F.TXT", b"q").unwrap();
    rename_path(&mut img, "F.TXT", "DEST/F.TXT").unwrap();
    assert_eq!(names(&img, ""), vec!["DEST"]);
    assert_eq!(names(&img, "DEST"), vec!["F.TXT"]);
}
