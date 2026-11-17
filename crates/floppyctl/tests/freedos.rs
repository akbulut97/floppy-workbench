//! A FreeDOS-bootable image must mount, carry KERNEL.SYS + COMMAND.COM, and
//! have the FreeDOS boot code + signature in its boot sector.

use std::io::{Cursor, Read};

use fatfs::{FileSystem, FsOptions};
use floppyctl::freedos::make_bootable_image;

#[test]
fn bootable_image_has_kernel_command_and_boot_code() {
    let img = make_bootable_image("FREEDOS", 0x1234).unwrap();
    assert_eq!(img.len(), 1_474_560, "1.44MB");

    // Boot signature + non-empty boot code just after the BPB.
    assert_eq!(img[510], 0x55);
    assert_eq!(img[511], 0xAA);
    assert!(img[0x3E..0x1FE].iter().any(|&b| b != 0), "boot code present");

    // Files readable by an independent FAT reader.
    let fs = FileSystem::new(Cursor::new(img), FsOptions::new()).unwrap();
    assert_eq!(fs.volume_label().trim_end(), "FREEDOS");
    for (name, min) in [("KERNEL.SYS", 40_000u64), ("COMMAND.COM", 50_000)] {
        let mut f = fs.root_dir().open_file(name).unwrap_or_else(|_| panic!("open {name}"));
        let mut d = Vec::new();
        f.read_to_end(&mut d).unwrap();
        assert!(d.len() as u64 > min, "{name} is {} bytes", d.len());
    }
}
