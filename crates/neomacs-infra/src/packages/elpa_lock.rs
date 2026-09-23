//! The ELPA lock manifest: pinned name/version/commit rows for archive
//! packages.
//!
//! Mirrors the MELPA lock TSV's philosophy: the repository pins **provenance
//! data only** — no package source is vendored.  A row says "this suite uses
//! exactly this release of that archive package"; the install itself is
//! driven by the editor through the archive (version-checked by GNU's own
//! `package-install` path, see [`super::elpa_archive`]), and the seal
//! inventory catches any drift afterwards.
//!
//! The `commit` column is the archive revision the release was published
//! from (GNU ELPA records it in each package's `-pkg.el` descriptor), kept
//! for provenance and diagnostics.

use std::sync::OnceLock;

const LOCKED_ELPA_MANIFEST: &str = include_str!("elpa-package-lock.tsv");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LockedElpaSource {
    pub name: &'static str,
    pub version: &'static str,
    pub commit: &'static str,
}

#[derive(Debug)]
struct LockedElpaCatalog {
    rows: Vec<LockedElpaSource>,
}

impl LockedElpaCatalog {
    fn parse(manifest: &'static str) -> Result<Self, String> {
        let mut rows = Vec::new();
        let mut header_seen = false;
        let mut previous: Option<&str> = None;
        for (index, line) in manifest.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let columns: Vec<&str> = line.split('\t').collect();
            if !header_seen {
                header_seen = columns == vec!["package", "version", "commit"];
                if !header_seen {
                    return Err("elpa lock manifest must start with the header row".to_string());
                }
                continue;
            }
            let [name, version, commit] = columns.as_slice() else {
                return Err(format!(
                    "elpa lock row {index} must have exactly package/version/commit columns"
                ));
            };
            if name.is_empty() || version.is_empty() || commit.is_empty() {
                return Err(format!("elpa lock row {index} has an empty cell"));
            }
            if let Some(previous) = previous {
                if *name < previous {
                    return Err(format!(
                        "elpa lock rows must be sorted by package name: {name} after {previous}"
                    ));
                }
            }
            previous = Some(name);
            rows.push(LockedElpaSource {
                name,
                version,
                commit,
            });
        }
        if !header_seen {
            return Err("elpa lock manifest is empty".to_string());
        }
        Ok(Self { rows })
    }

    fn source(&self, name: &str, version: &str) -> Option<LockedElpaSource> {
        self.rows
            .iter()
            .copied()
            .find(|row| row.name == name && row.version == version)
    }
}

fn elpa_catalog() -> &'static Result<LockedElpaCatalog, String> {
    static CATALOG: OnceLock<Result<LockedElpaCatalog, String>> = OnceLock::new();
    CATALOG.get_or_init(|| LockedElpaCatalog::parse(LOCKED_ELPA_MANIFEST))
}

/// Resolve a locked ELPA row, or `Err` naming the nearest problem.
pub fn locked_elpa_source(name: &str, version: &str) -> Result<LockedElpaSource, String> {
    let catalog = elpa_catalog().as_ref().map_err(Clone::clone)?;
    catalog
        .source(name, version)
        .ok_or_else(|| match catalog.source(name, "") {
            Some(row) => format!(
                "exact elpa package {name} {version} has no lock row, but {name} {} is pinned",
                row.version
            ),
            None => format!("exact elpa package {name} {version} has no lock row"),
        })
}

#[cfg(test)]
#[path = "elpa_lock/tests/elpa_lock_test.rs"]
mod tests;
