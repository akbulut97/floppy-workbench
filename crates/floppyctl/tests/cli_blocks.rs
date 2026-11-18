//! End-to-end tests of the block subcommands, run against a raw USB image file.

use std::path::Path;
use std::process::{Command, Output};

fn floppyctl(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_floppyctl"))
        .args(args)
        .output()
        .expect("run floppyctl")
}

fn ok(args: &[&str]) -> String {
    let out = floppyctl(args);
    assert!(
        out.status.success(),
        "floppyctl {args:?} failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

#[test]
fn block_commands_work_on_an_image_file() {
    let dir = tempfile::tempdir().unwrap();
    let usb = dir.path().join("usb.img");
    let note = dir.path().join("notes.txt");
    std::fs::write(&note, "hello").unwrap();

    // Two formatted banks out of six; bank 0 gets a file.
    ok(&[
        "build-usb",
        "--out",
        s(&usb),
        "--banks",
        "6",
        "--bank",
        "0:1.44:FIRST",
        "--bank",
        "1:1.44:SECOND",
        "--add-to",
        &format!("0:{}", s(&note)),
    ]);

    // Destructive commands refuse without --yes.
    let refused = floppyctl(&["clone-bank", "--image", s(&usb), "--from", "0", "--to", "3"]);
    assert!(!refused.status.success());

    ok(&[
        "clone-bank",
        "--image",
        s(&usb),
        "--from",
        "0",
        "--to",
        "3",
        "--yes",
    ]);

    let found = ok(&["search", "--image", s(&usb), "--banks", "6", "notes"]);
    assert!(found.contains("000") && found.contains("003"), "{found}");
    assert!(found.contains("NOTES.TXT"), "{found}");

    let out_dir = dir.path().join("saved");
    ok(&[
        "export-banks",
        "--image",
        s(&usb),
        "--from",
        "0",
        "--count",
        "2",
        "--out-dir",
        s(&out_dir),
    ]);
    assert!(out_dir.join("BLOCK000.img").exists());
    assert!(out_dir.join("BLOCK001.img").exists());

    ok(&[
        "import-banks",
        "--image",
        s(&usb),
        "--start",
        "4",
        "--yes",
        s(&out_dir.join("BLOCK001.img")),
        s(&out_dir.join("BLOCK000.img")),
    ]);
    // Natural order: BLOCK000 -> 4, BLOCK001 -> 5.
    let b4 = ok(&["read-bank", "--image", s(&usb), "--index", "4"]);
    assert!(b4.contains("FIRST") && b4.contains("NOTES.TXT"), "{b4}");

    ok(&[
        "format-banks",
        "--image",
        s(&usb),
        "--from",
        "2",
        "--label",
        "boot",
        "--freedos",
        "--yes",
    ]);
    let b2 = ok(&["read-bank", "--image", s(&usb), "--index", "2"]);
    assert!(b2.contains("BOOT") && b2.contains("KERNEL.SYS"), "{b2}");

    let check = ok(&["check", "--image", s(&usb), "--count", "6"]);
    assert!(check.contains("6 checked: 6 ok"), "{check}");
}

#[test]
fn check_exits_with_2_when_a_block_is_corrupt() {
    let dir = tempfile::tempdir().unwrap();
    let usb = dir.path().join("usb.img");
    ok(&[
        "build-usb",
        "--out",
        s(&usb),
        "--banks",
        "2",
        "--bank",
        "0:1.44:A",
    ]);
    let mut raw = std::fs::read(&usb).unwrap();
    raw[510] = 0; // break bank 0's boot signature
    std::fs::write(&usb, raw).unwrap();

    let out = floppyctl(&["check", "--image", s(&usb), "--count", "2"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stdout).contains("CORRUPT"));
}
