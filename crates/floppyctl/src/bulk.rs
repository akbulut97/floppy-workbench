//! Helpers for bulk block operations: file naming, natural ordering, and
//! planning which blocks a batch of images lands in.

use std::cmp::Ordering;
use std::path::PathBuf;

use anyhow::{bail, Result};

/// The file name used when saving block `index` as an image: `BLOCK007.img`.
pub fn bank_file_name(index: u32) -> String {
    format!("BLOCK{index:03}.img")
}

/// Compare two names "naturally": case-insensitive, with digit runs compared by
/// numeric value (`disk2` < `disk10`).
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    let (mut x, mut y) = (a.as_bytes(), b.as_bytes());
    loop {
        match (x.first(), y.first()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let nx = x.iter().take_while(|c| c.is_ascii_digit()).count();
                let ny = y.iter().take_while(|c| c.is_ascii_digit()).count();
                let (dx, dy) = (trim_zeros(&x[..nx]), trim_zeros(&y[..ny]));
                let ord = dx.len().cmp(&dy.len()).then_with(|| dx.cmp(dy));
                if ord != Ordering::Equal {
                    return ord;
                }
                x = &x[nx..];
                y = &y[ny..];
            }
            (Some(c), Some(d)) => {
                if c != d {
                    return c.cmp(d);
                }
                x = &x[1..];
                y = &y[1..];
            }
        }
    }
}

fn trim_zeros(digits: &[u8]) -> &[u8] {
    let n = digits.iter().take_while(|&&c| c == b'0').count();
    &digits[n.min(digits.len().saturating_sub(1))..]
}

/// Sort paths by their file name in natural order.
pub fn sort_natural(paths: &mut [PathBuf]) {
    let name = |p: &PathBuf| {
        p.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    paths.sort_by(|a, b| natural_cmp(&name(a), &name(b)));
}

/// The blocks that `count` images written from block `start` will occupy, or
/// an error if they don't fit in `total_banks`.
pub fn plan_import(start: u32, count: u32, total_banks: u32) -> Result<Vec<u32>> {
    if count == 0 {
        bail!("no images to write");
    }
    let end = start as u64 + count as u64;
    if end > total_banks as u64 {
        bail!(
            "{count} image(s) from block {start} would need blocks up to {}, but the USB only has {total_banks}",
            end - 1
        );
    }
    Ok((start..start + count).collect())
}

// ---------------------------------------------------------------------------
// Block operations over any [`BankStore`] (USB stick or image file)
// ---------------------------------------------------------------------------

use std::path::Path;

use anyhow::Context;
use serde::Serialize;

use crate::health::{check_image, HealthReport};
use crate::sfr::{extract_bank, normalize_bank_image};
use crate::store::BankStore;

/// Save the given blocks as `BLOCKnnn.img` files in `dir` (created if needed).
/// Returns the written paths in block order.
pub fn export_banks(
    store: &dyn BankStore,
    indices: &[u32],
    dir: &Path,
    on_progress: &dyn Fn(u64, u64),
) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let mut out = Vec::with_capacity(indices.len());
    for (n, &i) in indices.iter().enumerate() {
        let img = store.read_bank(i)?;
        let path = dir.join(bank_file_name(i));
        std::fs::write(&path, &img).with_context(|| format!("writing {}", path.display()))?;
        out.push(path);
        on_progress(n as u64 + 1, indices.len() as u64);
    }
    Ok(out)
}

/// Write image files into consecutive blocks starting at `start`. The files
/// are sorted in natural name order first (`paths` is sorted in place), and all
/// of them are read and checked before the first block is touched.
/// Returns the blocks written. DESTRUCTIVE.
pub fn import_images(
    store: &dyn BankStore,
    start: u32,
    paths: &mut [PathBuf],
    on_progress: &dyn Fn(u64, u64),
) -> Result<Vec<u32>> {
    sort_natural(paths);
    let targets = plan_import(start, paths.len() as u32, store.bank_count())?;
    let mut images = Vec::with_capacity(paths.len());
    for p in paths.iter() {
        let data = std::fs::read(p).with_context(|| format!("reading {}", p.display()))?;
        normalize_bank_image(&data).with_context(|| p.display().to_string())?;
        images.push(data);
    }
    for (n, (img, &block)) in images.iter().zip(&targets).enumerate() {
        store.write_bank(block, normalize_bank_image(img)?)?;
        on_progress(n as u64 + 1, targets.len() as u64);
    }
    Ok(targets)
}

/// Copy block `src` onto block `dst`. DESTRUCTIVE for `dst`.
pub fn clone_bank(store: &dyn BankStore, src: u32, dst: u32) -> Result<()> {
    if src == dst {
        bail!("source and target block are the same");
    }
    let img = store.read_bank(src)?;
    store.write_bank(dst, &img)
}

/// One search hit across blocks.
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub block: u32,
    pub path: String,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
}

/// Search file/folder names (case-insensitive substring) in the first `count` blocks.
pub fn search_banks(
    store: &dyn BankStore,
    count: u32,
    query: &str,
    on_progress: &dyn Fn(u64, u64),
) -> Result<Vec<SearchHit>> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(vec![]);
    }
    let raw = store.read_banks(count, on_progress)?;
    let mut hits = Vec::new();
    for block in 0..count {
        let Ok(img) = extract_bank(&raw, block) else {
            break;
        };
        if crate::inspect(img).is_err() {
            continue; // blank or non-FAT block
        }
        let mut found = crate::fsops::find_entries(img, query).unwrap_or_default();
        found.sort_by(|a, b| natural_cmp(&a.0, &b.0));
        for (path, e) in found {
            hits.push(SearchHit {
                block,
                path,
                name: e.name,
                size: e.size,
                is_dir: e.is_dir,
            });
        }
    }
    Ok(hits)
}

/// Run the integrity check on each of `indices`.
pub fn check_banks(
    store: &dyn BankStore,
    indices: &[u32],
    on_progress: &dyn Fn(u64, u64),
) -> Result<Vec<(u32, HealthReport)>> {
    let Some(&max) = indices.iter().max() else {
        return Ok(vec![]);
    };
    // Many blocks: one sequential read is far faster on a USB stick.
    let raw = if indices.len() > 8 {
        Some(store.read_banks(max + 1, on_progress)?)
    } else {
        None
    };
    let mut out = Vec::with_capacity(indices.len());
    for (n, &i) in indices.iter().enumerate() {
        let report = match &raw {
            Some(raw) => check_image(extract_bank(raw, i)?),
            None => {
                let img = store.read_bank(i)?;
                on_progress(n as u64 + 1, indices.len() as u64);
                check_image(&img)
            }
        };
        out.push((i, report));
    }
    Ok(out)
}
