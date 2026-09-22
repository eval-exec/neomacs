//! The fixture inventory: a content manifest that proves a sealed fixture
//! is byte-identical to the day it was sealed.
//!
//! The seal (write-bit stripping) stops accidental writes from succeeding;
//! the inventory makes any mutation that *did* get through — an editor
//! escaping a redirect, a chmod by tooling, a hand edit during debugging —
//! detectable at the next verify instead of silently diverging every
//! comparison that mounts the fixture.

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

/// One file's recorded identity, relative to the fixture root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

/// The full content inventory of one sealed fixture.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Inventory {
    pub entries: Vec<Entry>,
}

impl Inventory {
    /// Walk `root`, hashing every regular file.  Symlinks are recorded by
    /// their target string (content is whatever the target holds and is
    /// covered when the target itself is visited, if it lives inside the
    /// fixture).
    pub fn build(root: &Path) -> Result<Self, String> {
        let mut entries = Vec::new();
        fn walk(root: &Path, dir: &Path, entries: &mut Vec<Entry>) -> Result<(), String> {
            let items =
                fs::read_dir(dir).map_err(|error| format!("read {}: {error}", dir.display()))?;
            let mut items: Vec<_> = items.filter_map(Result::ok).map(|e| e.path()).collect();
            items.sort();
            for path in items {
                let meta = fs::symlink_metadata(&path)
                    .map_err(|error| format!("stat {}: {error}", path.display()))?;
                let rel = path
                    .strip_prefix(root)
                    .map_err(|error| format!("relativize {}: {error}", path.display()))?
                    .to_string_lossy()
                    .into_owned();
                if meta.is_symlink() {
                    let target = fs::read_link(&path)
                        .map_err(|error| format!("readlink {}: {error}", path.display()))?;
                    entries.push(Entry {
                        path: rel,
                        size: 0,
                        sha256: format!("link:{}", target.to_string_lossy()),
                    });
                } else if meta.is_dir() {
                    walk(root, &path, entries)?;
                } else {
                    let bytes = fs::read(&path)
                        .map_err(|error| format!("read {}: {error}", path.display()))?;
                    let digest = Sha256::digest(&bytes);
                    let mut hex = String::with_capacity(digest.len() * 2);
                    for byte in digest {
                        hex.push_str(&format!("{byte:02x}"));
                    }
                    entries.push(Entry {
                        path: rel,
                        size: bytes.len() as u64,
                        sha256: hex,
                    });
                }
            }
            Ok(())
        }
        walk(root, root, &mut entries)?;
        Ok(Self { entries })
    }

    /// Serialize to stable JSON-lines (one entry per line).
    pub fn to_jsonl(&self) -> String {
        let mut out = String::new();
        for e in &self.entries {
            out.push_str(&format!(
                "{{\"path\":{:?},\"size\":{},\"sha256\":{:?}}}\n",
                e.path.replace('\\', "/"),
                e.size,
                e.sha256
            ));
        }
        out
    }

    /// Parse back a JSON-lines inventory.  Tolerant of the two fields in
    /// any order; unknown keys ignored.
    pub fn parse_jsonl(text: &str) -> Result<Self, String> {
        let mut entries = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let get = |key: &str| -> Option<String> {
                let marker = format!("\"{key}\":");
                let i = line.find(&marker)?;
                let rest = line[i + marker.len()..].trim_start();
                let rest = rest.strip_prefix('"')?;
                let end = rest.find('"')?;
                Some(rest[..end].to_owned())
            };
            let path = get("path").ok_or("inventory line missing path")?;
            let size: u64 = get_number(line, "size").ok_or("inventory line missing size")?;
            let sha256 = get("sha256").ok_or("inventory line missing sha256")?;
            entries.push(Entry { path, size, sha256 });
        }
        Ok(Self { entries })
    }
}

/// Read an unquoted numeric field from a JSON-lines entry.
fn get_number(line: &str, key: &str) -> Option<u64> {
    let marker = format!("\"{key}\":");
    let i = line.find(&marker)?;
    let rest = line[i + marker.len()..].trim_start();
    let end = rest.find(',')?;
    rest[..end].trim().parse().ok()
}

/// Differences between the inventory and the tree as it exists now.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Drift {
    pub missing: Vec<String>,
    pub modified: Vec<String>,
    pub added: Vec<String>,
}

impl Drift {
    pub fn is_clean(&self) -> bool {
        self.missing.is_empty() && self.modified.is_empty() && self.added.is_empty()
    }
}

/// Deep verification: re-walk the tree, re-hash every file, and compare
/// against the sealed inventory.  A clean report proves the fixture is
/// byte-identical to the day it was sealed.
pub fn verify_deep(root: &Path, inventory: &Inventory) -> Result<Drift, String> {
    // Current tree state, walked and hashed the same way.
    let mut current: Vec<(String, u64, String)> = Vec::new();
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, u64, String)>) -> Result<(), String> {
        let items =
            fs::read_dir(dir).map_err(|error| format!("read {}: {error}", dir.display()))?;
        let mut items: Vec<_> = items.filter_map(Result::ok).map(|e| e.path()).collect();
        items.sort();
        for path in items {
            let meta = fs::symlink_metadata(&path)
                .map_err(|error| format!("stat {}: {error}", path.display()))?;
            let rel = path
                .strip_prefix(root)
                .map_err(|error| format!("relativize {}: {error}", path.display()))?
                .to_string_lossy()
                .into_owned();
            if meta.is_symlink() {
                let target = fs::read_link(&path)
                    .map_err(|error| format!("readlink {}: {error}", path.display()))?;
                out.push((rel, 0, format!("link:{}", target.to_string_lossy())));
            } else if meta.is_dir() {
                walk(root, &path, out)?;
            } else {
                let bytes =
                    fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
                let digest = Sha256::digest(&bytes);
                let mut hex = String::with_capacity(digest.len() * 2);
                for byte in digest {
                    hex.push_str(&format!("{byte:02x}"));
                }
                out.push((rel, bytes.len() as u64, hex));
            }
        }
        Ok(())
    }
    walk(root, root, &mut current)?;

    let sealed: std::collections::HashMap<&str, (u64, &str)> = inventory
        .entries
        .iter()
        .map(|e| (e.path.as_str(), (e.size, e.sha256.as_str())))
        .collect();
    let now: std::collections::HashMap<&str, (u64, &str)> = current
        .iter()
        .map(|(p, s, h)| (p.as_str(), (*s, h.as_str())))
        .collect();

    let mut drift = Drift::default();
    for (path, (size, hash)) in &sealed {
        match now.get(path) {
            None => drift.missing.push((*path).to_owned()),
            Some((live_size, live_hash)) if live_size != size || live_hash != hash => {
                drift.modified.push((*path).to_owned())
            }
            Some(_) => {}
        }
    }
    for (path, (size, hash)) in &now {
        if !sealed.contains_key(path) {
            drift.added.push(format!("{path} ({size}B {hash})"));
        }
    }
    Ok(drift)
}
