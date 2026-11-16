//! Geometry facts for the three supported floppy formats.
//! Source of truth: TEKNIK_RAPOR.md §4.1.

use fat12::{FloppyFormat, Geometry};

#[test]
fn image_size_720k() {
    let g = Geometry::for_format(FloppyFormat::F720K);
    assert_eq!(g.image_size(), 737_280);
}

#[test]
fn image_size_1_2m() {
    let g = Geometry::for_format(FloppyFormat::F1_2M);
    assert_eq!(g.image_size(), 1_228_800);
}

#[test]
fn image_size_1_44m() {
    let g = Geometry::for_format(FloppyFormat::F1_44M);
    assert_eq!(g.image_size(), 1_474_560);
}
