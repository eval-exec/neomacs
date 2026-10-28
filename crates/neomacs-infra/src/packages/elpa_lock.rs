//! The ELPA lock manifest: pinned name/version/commit/checksum rows for the
//! vendored GNU ELPA release tarballs.
//!
//! Mirrors the MELPA lock TSV's philosophy in data shape and one stronger
//! rule in kind: the repository pins **provenance data and the exact release
//! bytes**.  A row says "this suite uses exactly this release of that archive
//! package, published from this upstream commit, whose release tarball has
//! this sha256"; the tarball itself is committed under
//! `crates/neomacs-infra/fixtures/gnu-elpa/` and installed from file (see
//! [`super::elpa_archive`]), so no test ever contacts the archive.  The seal
//! inventory catches any drift afterwards.
//!
//! The `commit` column is the archive revision the release was published
//! from (GNU ELPA records it in each package's `-pkg.el` descriptor); the
//! `sha256` column is the digest of the committed tarball.

use std::sync::OnceLock;

const LOCKED_ELPA_MANIFEST: &str = include_str!("elpa-package-lock.tsv");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LockedElpaSource {
    pub name: &'static str,
    pub version: &'static str,
    pub commit: &'static str,
    pub sha256: &'static str,
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
                header_seen = columns == vec!["package", "version", "commit", "sha256"];
                if !header_seen {
                    return Err(
                        "elpa lock manifest must start with the package/version/commit/sha256 \
                         header row"
                            .to_string(),
                    );
                }
                continue;
            }
            let [name, version, commit, sha256] = columns.as_slice() else {
                return Err(format!(
                    "elpa lock row {index} must have exactly package/version/commit/sha256 \
                     columns"
                ));
            };
            if name.is_empty() || version.is_empty() || commit.is_empty() || sha256.is_empty() {
                return Err(format!("elpa lock row {index} has an empty cell"));
            }
            if sha256.len() != 64
                || !sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(format!(
                    "elpa lock row {index} ({name} {version}) must carry a lowercase hex sha256, \
                     got `{sha256}`"
                ));
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
                sha256,
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

/// Every pinned GNU ELPA row, in manifest order.
pub fn locked_elpa_sources() -> Result<&'static [LockedElpaSource], String> {
    Ok(&elpa_catalog().as_ref().map_err(Clone::clone)?.rows)
}

/// Resolve a locked ELPA row, or `Err` naming the nearest problem.
pub fn locked_elpa_source(name: &str, version: &str) -> Result<LockedElpaSource, String> {
    let catalog = elpa_catalog().as_ref().map_err(Clone::clone)?;
    catalog
        .source(name, version)
        .ok_or_else(|| match catalog.source(name, "") {
            Some(row) => format!(
                "exact elpa package {name} {version} has no lock row, but {name} {} is pinned in \
                 crates/neomacs-infra/src/packages/elpa-package-lock.tsv",
                row.version
            ),
            None => format!(
                "elpa package {name} {version} has no vendored lock row in \
                 crates/neomacs-infra/src/packages/elpa-package-lock.tsv; GNU ELPA packages \
                 install from checksummed tarballs committed under \
                 crates/neomacs-infra/fixtures/gnu-elpa/, never from the network"
            ),
        })
}

#[cfg(test)]
#[path = "elpa_lock/tests/elpa_lock_test.rs"]
mod tests;
