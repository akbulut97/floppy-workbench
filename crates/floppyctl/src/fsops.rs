//! Explorer-style filesystem operations on a FAT12 image.
//!
//! Read-only operations ([`list_dir`], [`read_file_at`]) take `&[u8]`; mutating
//! operations take `&mut [u8]` and leave a valid image behind. Paths use `/` and
//! are relative to the image root (`""` is the root directory).

use std::io::{Cursor, Read, Write};

use anyhow::Result;
use fatfs::{FileSystem, FsOptions};

use crate::BankFile;

/// Join a directory path and a name into a root-relative path.
fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// List the entries in directory `path` (`""` = root).
pub fn list_dir(img: &[u8], path: &str) -> Result<Vec<BankFile>> {
    let fs = FileSystem::new(Cursor::new(img.to_vec()), FsOptions::new())?;
    let root = fs.root_dir();
    let mut out = Vec::new();
    let listing = if path.is_empty() {
        root.iter()
    } else {
        root.open_dir(path)?.iter()
    };
    for entry in listing {
        let e = entry?;
        let name = e.file_name();
        if name == "." || name == ".." {
            continue;
        }
        out.push(BankFile {
            name,
            size: e.len(),
            is_dir: e.is_dir(),
        });
    }
    Ok(out)
}

/// Find every file or folder (in any folder) whose name contains `query`,
/// case-insensitively. Returns `(full root-relative path, entry)` pairs.
pub fn find_entries(img: &[u8], query: &str) -> Result<Vec<(String, BankFile)>> {
    let needle = query.to_lowercase();
    let mut hits = Vec::new();
    let mut stack = vec![String::new()];
    while let Some(dir) = stack.pop() {
        for e in list_dir(img, &dir)? {
            let full = join(&dir, &e.name);
            if e.is_dir {
                stack.push(full.clone());
            }
            if e.name.to_lowercase().contains(&needle) {
                hits.push((full, e));
            }
        }
    }
    Ok(hits)
}

/// Read the contents of the file at `path`.
pub fn read_file_at(img: &[u8], path: &str) -> Result<Vec<u8>> {
    let fs = FileSystem::new(Cursor::new(img.to_vec()), FsOptions::new())?;
    let mut file = fs.root_dir().open_file(path)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    Ok(buf)
}

/// Create a directory at `path` (its parent must already exist).
pub fn make_dir(img: &mut [u8], path: &str) -> Result<()> {
    let fs = FileSystem::new(Cursor::new(&mut *img), FsOptions::new())?;
    fs.root_dir().create_dir(path)?;
    fs.unmount()?;
    Ok(())
}

/// Create (or overwrite) a file `name` inside directory `dir` with `data`.
pub fn add_file_at(img: &mut [u8], dir: &str, name: &str, data: &[u8]) -> Result<()> {
    let fs = FileSystem::new(Cursor::new(&mut *img), FsOptions::new())?;
    {
        let mut f = fs.root_dir().create_file(&join(dir, name))?;
        f.truncate()?;
        f.write_all(data)?;
        f.flush()?;
    }
    fs.unmount()?;
    Ok(())
}

/// Delete a file, or a directory and everything inside it, at `path`.
pub fn remove_path(img: &mut [u8], path: &str) -> Result<()> {
    let fs = FileSystem::new(Cursor::new(&mut *img), FsOptions::new())?;
    {
        let root = fs.root_dir();

        // Gather every descendant path (depth-first), then remove deepest first
        // so directories are empty by the time they are removed.
        let mut descendants: Vec<String> = Vec::new();
        let mut stack = vec![path.to_string()];
        while let Some(p) = stack.pop() {
            if let Ok(dir) = root.open_dir(&p) {
                for entry in dir.iter() {
                    let e = entry?;
                    let name = e.file_name();
                    if name == "." || name == ".." {
                        continue;
                    }
                    let child = format!("{p}/{name}");
                    descendants.push(child.clone());
                    stack.push(child);
                }
            }
        }
        descendants.sort_by_key(|s| std::cmp::Reverse(s.matches('/').count()));
        for p in &descendants {
            root.remove(p)?;
        }
        root.remove(path)?;
    }
    fs.unmount()?;
    Ok(())
}

/// One node of a copied subtree (relative to the subtree's own base).
#[derive(Debug, Clone)]
pub struct TreeNode {
    /// Path relative to the subtree base (`""` is the base entry itself).
    pub rel: String,
    pub is_dir: bool,
    pub data: Vec<u8>,
}

/// Read the entry at `path` (a file or a whole directory tree) into memory.
/// Parents come before children, so the result can be replayed with [`write_tree`].
pub fn read_tree(img: &[u8], path: &str) -> Result<Vec<TreeNode>> {
    let fs = FileSystem::new(Cursor::new(img.to_vec()), FsOptions::new())?;
    let root = fs.root_dir();
    let mut nodes = Vec::new();

    if root.open_dir(path).is_err() {
        // It's a file.
        let mut f = root.open_file(path)?;
        let mut data = Vec::new();
        f.read_to_end(&mut data)?;
        nodes.push(TreeNode { rel: String::new(), is_dir: false, data });
        return Ok(nodes);
    }

    // Directory: breadth-first so parents precede children.
    nodes.push(TreeNode { rel: String::new(), is_dir: true, data: Vec::new() });
    let mut queue = vec![(path.to_string(), String::new())];
    while let Some((dir_path, rel_prefix)) = queue.pop() {
        for entry in root.open_dir(&dir_path)?.iter() {
            let e = entry?;
            let name = e.file_name();
            if name == "." || name == ".." {
                continue;
            }
            let child_path = join(&dir_path, &name);
            let child_rel = join(&rel_prefix, &name);
            if e.is_dir() {
                nodes.push(TreeNode { rel: child_rel.clone(), is_dir: true, data: Vec::new() });
                queue.push((child_path, child_rel));
            } else {
                let mut f = root.open_file(&child_path)?;
                let mut data = Vec::new();
                f.read_to_end(&mut data)?;
                nodes.push(TreeNode { rel: child_rel, is_dir: false, data });
            }
        }
    }
    Ok(nodes)
}

/// Read a file or folder from the host filesystem into a subtree for [`write_tree`].
/// Names are upper-cased (DOS convention, matching single-file adds). Parents come
/// before children.
pub fn read_host_tree(path: &std::path::Path) -> Result<Vec<TreeNode>> {
    let meta = std::fs::metadata(path)?;
    if !meta.is_dir() {
        let data = std::fs::read(path)?;
        return Ok(vec![TreeNode { rel: String::new(), is_dir: false, data }]);
    }
    let mut nodes = vec![TreeNode { rel: String::new(), is_dir: true, data: Vec::new() }];
    let mut queue = vec![(path.to_path_buf(), String::new())];
    while let Some((dir, rel_prefix)) = queue.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir)?.collect::<std::io::Result<_>>()?;
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let name = e
                .file_name()
                .into_string()
                .map_err(|n| anyhow::anyhow!("invalid file name: {n:?}"))?
                .to_uppercase();
            let rel = join(&rel_prefix, &name);
            if e.file_type()?.is_dir() {
                nodes.push(TreeNode { rel: rel.clone(), is_dir: true, data: Vec::new() });
                queue.push((e.path(), rel));
            } else {
                let data = std::fs::read(e.path())?;
                nodes.push(TreeNode { rel, is_dir: false, data });
            }
        }
    }
    Ok(nodes)
}

/// Write a subtree (from [`read_tree`]) under `dst_base`.
pub fn write_tree(img: &mut [u8], dst_base: &str, nodes: &[TreeNode]) -> Result<()> {
    let fs = FileSystem::new(Cursor::new(&mut *img), FsOptions::new())?;
    {
        let root = fs.root_dir();
        for node in nodes {
            let full = if node.rel.is_empty() {
                dst_base.to_string()
            } else {
                join(dst_base, &node.rel)
            };
            if node.is_dir {
                root.create_dir(&full)?;
            } else {
                let mut f = root.create_file(&full)?;
                f.truncate()?;
                f.write_all(&node.data)?;
                f.flush()?;
            }
        }
    }
    fs.unmount()?;
    Ok(())
}

/// Rename or move the entry at `from` to `to` (both root-relative).
pub fn rename_path(img: &mut [u8], from: &str, to: &str) -> Result<()> {
    let fs = FileSystem::new(Cursor::new(&mut *img), FsOptions::new())?;
    {
        let root = fs.root_dir();
        let (parent, name) = match to.rsplit_once('/') {
            Some((p, n)) => (p, n),
            None => ("", to),
        };
        if parent.is_empty() {
            root.rename(from, &root, name)?;
        } else {
            let dst = root.open_dir(parent)?;
            root.rename(from, &dst, name)?;
        }
    }
    fs.unmount()?;
    Ok(())
}
