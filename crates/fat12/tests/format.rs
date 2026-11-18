//! Structural tests for the formatted image (boot sector, BPB, FAT).
//! Byte offsets: TEKNIK_RAPOR.md §4.3.

use fat12::{format_image, FloppyFormat, Geometry};

fn le16(buf: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([buf[off], buf[off + 1]])
}

#[test]
fn image_has_correct_length() {
    let img = format_image(FloppyFormat::F1_44M, "TEST", 0x12345678);
    assert_eq!(
        img.len(),
        Geometry::for_format(FloppyFormat::F1_44M).image_size()
    );
}

#[test]
fn boot_sector_signature_present() {
    let img = format_image(FloppyFormat::F1_44M, "TEST", 0x12345678);
    assert_eq!(img[510], 0x55, "boot signature byte 0");
    assert_eq!(img[511], 0xAA, "boot signature byte 1");
}

#[test]
fn bpb_fields_match_geometry() {
    let img = format_image(FloppyFormat::F1_44M, "TEST", 0x12345678);
    let g = Geometry::for_format(FloppyFormat::F1_44M);
    assert_eq!(le16(&img, 0x0B), g.bytes_per_sector, "bytes/sector");
    assert_eq!(img[0x0D], g.sectors_per_cluster, "sectors/cluster");
    assert_eq!(le16(&img, 0x0E), g.reserved_sectors, "reserved sectors");
    assert_eq!(img[0x10], g.num_fats, "num FATs");
    assert_eq!(le16(&img, 0x11), g.root_entries, "root entries");
    assert_eq!(le16(&img, 0x13), g.total_sectors, "total sectors (16-bit)");
    assert_eq!(img[0x15], g.media_descriptor, "media descriptor");
    assert_eq!(le16(&img, 0x16), g.sectors_per_fat, "sectors/FAT");
    assert_eq!(le16(&img, 0x18), g.sectors_per_track, "sectors/track");
    assert_eq!(le16(&img, 0x1A), g.num_heads, "num heads");
}

#[test]
fn fs_type_and_boot_sig_byte() {
    let img = format_image(FloppyFormat::F1_44M, "TEST", 0x12345678);
    assert_eq!(img[0x26], 0x29, "extended boot signature");
    assert_eq!(&img[0x36..0x3E], b"FAT12   ", "fs type string");
}

#[test]
fn fat_is_initialized() {
    let img = format_image(FloppyFormat::F1_44M, "TEST", 0x12345678);
    let g = Geometry::for_format(FloppyFormat::F1_44M);
    let fat_start = g.reserved_sectors as usize * g.bytes_per_sector as usize;
    // FAT[0] = media descriptor + 0xFF 0xFF ; FAT[1] = EOC (0xFFF).
    assert_eq!(
        img[fat_start], g.media_descriptor,
        "FAT[0] low byte = media"
    );
    assert_eq!(img[fat_start + 1], 0xFF);
    assert_eq!(img[fat_start + 2], 0xFF);
}

#[test]
fn root_dir_has_volume_label_entry() {
    let img = format_image(FloppyFormat::F1_44M, "DISKLABEL", 0x12345678);
    let g = Geometry::for_format(FloppyFormat::F1_44M);
    let bps = g.bytes_per_sector as usize;
    let root =
        (g.reserved_sectors as usize + g.num_fats as usize * g.sectors_per_fat as usize) * bps;
    // First root entry: 11-byte name (label, space-padded) + attr 0x08 (VOLUME_ID).
    assert_eq!(&img[root..root + 11], b"DISKLABEL  ", "label name field");
    assert_eq!(img[root + 11], 0x08, "ATTR_VOLUME_ID");
}

#[test]
fn both_fat_copies_identical() {
    let img = format_image(FloppyFormat::F1_44M, "TEST", 0x12345678);
    let g = Geometry::for_format(FloppyFormat::F1_44M);
    let bps = g.bytes_per_sector as usize;
    let fat_bytes = g.sectors_per_fat as usize * bps;
    let fat1 = g.reserved_sectors as usize * bps;
    let fat2 = fat1 + fat_bytes;
    assert_eq!(&img[fat1..fat1 + fat_bytes], &img[fat2..fat2 + fat_bytes]);
}
