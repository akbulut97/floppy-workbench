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

#[test]
fn add_host_folder_recursively() {
    use floppyctl::fsops::read_host_tree;
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("Proj");
    std::fs::create_dir_all(base.join("sub").join("deep")).unwrap();
    std::fs::write(base.join("readme.txt"), b"hello").unwrap();
    std::fs::write(base.join("sub").join("a.bin"), vec![7u8; 3000]).unwrap();
    std::fs::write(base.join("sub").join("deep").join("z.txt"), b"zz").unwrap();

    let mut img = blank();
    let tree = read_host_tree(&base).unwrap();
    write_tree(&mut img, "PROJ", &tree).unwrap();

    assert_eq!(names(&img, ""), vec!["PROJ"]);
    assert_eq!(names(&img, "PROJ"), vec!["README.TXT", "SUB"]);
    assert_eq!(names(&img, "PROJ/SUB"), vec!["A.BIN", "DEEP"]);
    assert_eq!(read_file_at(&img, "PROJ/README.TXT").unwrap(), b"hello");
    assert_eq!(read_file_at(&img, "PROJ/SUB/A.BIN").unwrap(), vec![7u8; 3000]);
    assert_eq!(read_file_at(&img, "PROJ/SUB/DEEP/Z.TXT").unwrap(), b"zz");

    // Adding the same folder again merges/overwrites instead of failing.
    std::fs::write(base.join("readme.txt"), b"v2").unwrap();
    let tree = read_host_tree(&base).unwrap();
    write_tree(&mut img, "PROJ", &tree).unwrap();
    assert_eq!(read_file_at(&img, "PROJ/README.TXT").unwrap(), b"v2");
    assert_eq!(names(&img, "PROJ"), vec!["README.TXT", "SUB"]);
}

#[test]
fn read_host_tree_single_file() {
    use floppyctl::fsops::read_host_tree;
    let tmp = tempfile::tempdir().unwrap();
    let f = tmp.path().join("x.txt");
    std::fs::write(&f, b"abc").unwrap();
    let tree = read_host_tree(&f).unwrap();
    assert_eq!(tree.len(), 1);
    assert!(!tree[0].is_dir);
    assert_eq!(tree[0].data, b"abc");
}

#[test]
fn find_entries_searches_all_folders_case_insensitively() {
    use floppyctl::fsops::find_entries;
    let mut img = blank();
    make_dir(&mut img, "PROGS").unwrap();
    make_dir(&mut img, "PROGS/OLD").unwrap();
    add_file_at(&mut img, "", "README.TXT", b"r").unwrap();
    add_file_at(&mut img, "PROGS", "PART1.NC", b"1").unwrap();
    add_file_at(&mut img, "PROGS/OLD", "PART2.NC", b"22").unwrap();

    let mut hits: Vec<String> = find_entries(&img, "part").unwrap().into_iter().map(|(p, _)| p).collect();
    hits.sort();
    assert_eq!(hits, vec!["PROGS/OLD/PART2.NC", "PROGS/PART1.NC"]);

    let old = find_entries(&img, "old").unwrap();
    assert_eq!(old.len(), 1);
    assert!(old[0].1.is_dir);
    assert!(find_entries(&img, "nothing").unwrap().is_empty());
}
