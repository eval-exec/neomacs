//! The tools lock manifest: pinned external tool identities.
//!
//! Rows pin NAME + VERSION (+ per-strategy resolution data).  This is
//! provenance data only — the repository never vendors tool binaries; the
//! resolution strategies (PATH probe, nix build, release tarball) fetch or
//! locate the pinned build.

use std::sync::OnceLock;

const TOOLS_LOCK_MANIFEST: &str = include_str!("tools-lock.tsv");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolLockEntry {
    pub name: &'static str,
    pub version: &'static str,
    /// Expected sha256 of the resolved binary.  `None` for `System`-source
    /// rows where the host package manager owns the bytes and only the
    /// version is pinned.
    pub sha256: Option<&'static str>,
}

#[derive(Debug)]
struct ToolLockCatalog {
    rows: Vec<ToolLockEntry>,
}

impl ToolLockCatalog {
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
                header_seen = columns == vec!["name", "version", "sha256"];
                if !header_seen {
                    return Err("tools lock manifest must start with the header row".to_string());
                }
                continue;
            }
            let [name, version, sha256] = columns.as_slice() else {
                return Err(format!(
                    "tools lock row {index} must have exactly name/version/sha256 columns"
                ));
            };
            if name.is_empty() || version.is_empty() {
                return Err(format!("tools lock row {index} has an empty cell"));
            }
            if let Some(previous) = previous {
                if *name < previous {
                    return Err(format!(
                        "tools lock rows must be sorted by name: {name} after {previous}"
                    ));
                }
            }
            previous = Some(name);
            let sha256 = (!sha256.is_empty()).then_some(*sha256);
            rows.push(ToolLockEntry {
                name,
                version,
                sha256,
            });
        }
        if !header_seen {
            return Err("tools lock manifest is empty".to_string());
        }
        Ok(Self { rows })
    }

    fn entry(&self, name: &str) -> Option<ToolLockEntry> {
        self.rows.iter().copied().find(|row| row.name == name)
    }
}

fn tools_catalog() -> &'static Result<ToolLockCatalog, String> {
    static CATALOG: OnceLock<Result<ToolLockCatalog, String>> = OnceLock::new();
    CATALOG.get_or_init(|| ToolLockCatalog::parse(TOOLS_LOCK_MANIFEST))
}

/// Resolve a tool's lock row, or `Err` naming the nearest problem.
pub fn tool_lock_entry(name: &str) -> Result<ToolLockEntry, String> {
    let catalog = tools_catalog().as_ref().map_err(Clone::clone)?;
    catalog
        .entry(name)
        .ok_or_else(|| format!("tool `{name}` has no lock row"))
}

#[cfg(test)]
mod tests {
    use super::ToolLockCatalog;

    #[test]
    fn tools_lock_rejects_unsorted_rows() {
        let error = ToolLockCatalog::parse(
            "name\tversion\tsha256\n\
             zed\t1.0\t\n\
             abi\t2.0\t\n",
        )
        .expect_err("unsorted rows must be rejected");
        assert!(error.contains("sorted by name"), "{error}");
    }

    #[test]
    fn tools_lock_rejects_missing_header_and_tolerates_empty_sha() {
        let error =
            ToolLockCatalog::parse("git\t2.51.2\n").expect_err("missing header must be rejected");
        assert!(error.contains("header row"), "{error}");

        let catalog = ToolLockCatalog::parse("name\tversion\tsha256\ngit\t2.51.2\t\n")
            .expect("empty sha256 cell is a version-only pin");
        assert!(catalog.entry("git").is_some());
    }
}
