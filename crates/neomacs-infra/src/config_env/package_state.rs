//! The installed-package identity of one sealed fixture.
//!
//! A fixture's provenance is two things: the pinned tree revision and the
//! package builds the GNU bootstrap produced.  The tree is pinned by
//! `<name>-spec.toml`; the packages came from live package archives and are
//! only reproducible as a *record*.  `PackageStateIdentity` is that record:
//! written into the sealed fixture as `PACKAGES`, hashable, and cross-checked
//! against the spec's pin, so a rebuild that silently pulled different
//! package versions fails at open time instead of diverging comparisons.
//!
//! The two supported layouts are the two distributions' shapes — package.el
//! `elpa/<emacs-version>/<name>-<version>/` (Spacemacs) and straight's
//! `repos/` builds (Doom).  The variant *is* the layout: no shape that was
//! never a real package state can be constructed.

use crate::inventory::sha256_hex;
use std::fs;
use std::path::Path;

/// One package build the bootstrap produced.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct InstalledPackage {
    pub name: String,
    pub version: String,
}

/// The package-state identity of one sealed fixture, in its native layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageStateIdentity {
    /// GNU package.el: `elpa/<emacs-version>/<kind>/<name>-<version>/`.
    Elpa {
        emacs_version: String,
        packages: Vec<InstalledPackage>,
    },
    /// Straight: `repos/<repo>/` builds anchored by the per-Emacs build
    /// cache (this fixture shape carries no versions lockfile).
    Straight {
        build_cache_sha256: String,
        repos: Vec<String>,
    },
}

/// SHA-256 of a record — the pin the spec stores.
pub fn digest(text: &str) -> String {
    sha256_hex(text.as_bytes())
}

/// Parse `<name>-<version>` from a package build directory name.
///
/// Package names themselves may contain hyphens and dots (`dash.el`,
/// `let-alist`), so the version is found by scanning from the right for the
/// first hyphen whose suffix is digits-and-dots.
fn parse_package_dir(name: &str) -> Option<InstalledPackage> {
    let cut = name.rfind('-').filter(|at| {
        name[*at + 1..]
            .chars()
            .all(|ch| ch.is_ascii_digit() || ch == '.')
            && name[*at + 1..].contains(|ch: char| ch.is_ascii_digit())
    })?;
    let (name, version) = (&name[..cut], &name[cut + 1..]);
    (!name.is_empty()).then(|| InstalledPackage {
        name: name.to_owned(),
        version: (*version).to_owned(),
    })
}

/// Sorted subdirectory names of `dir`.
fn read_sorted_dirs(dir: &Path) -> Result<Vec<std::path::PathBuf>, String> {
    let items = fs::read_dir(dir).map_err(|error| format!("read {}: {error}", dir.display()))?;
    let mut paths: Vec<_> = items
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            fs::symlink_metadata(path)
                .map(|meta| meta.is_dir())
                .unwrap_or(false)
        })
        .collect();
    paths.sort();
    Ok(paths)
}

/// Sorted entry paths of `dir` (files and directories).
fn read_sorted_entries(dir: &Path) -> Result<Vec<std::path::PathBuf>, String> {
    let items = fs::read_dir(dir).map_err(|error| format!("read {}: {error}", dir.display()))?;
    let mut paths: Vec<_> = items
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    paths.sort();
    Ok(paths)
}

impl PackageStateIdentity {
    /// Read the package state of a fixture root, detecting its layout.
    ///
    /// Spacemacs mounts through HOME (its tree *is* `.emacs.d`) and keeps
    /// package builds in `package-state/elpa/<emacs-version>/...`; Doom keeps
    /// straight builds inside the tree at `tree/.local/straight/`.  A root
    /// with neither is not a fixture we know how to describe.
    pub fn from_fixture(root: &Path) -> Result<Self, String> {
        let elpa = root.join("package-state/elpa");
        if elpa.is_dir() {
            return Self::from_elpa(&elpa);
        }
        let straight = root.join("tree/.local/straight");
        if straight.is_dir() {
            return Self::from_straight(&straight);
        }
        Err(format!(
            "{}: neither package-state/elpa nor tree/.local/straight present; \
             unknown package-state layout",
            root.display()
        ))
    }

    /// GNU package.el layout: one emacs-version directory whose descendants
    /// hold the `<name>-<version>` builds.
    fn from_elpa(elpa: &Path) -> Result<Self, String> {
        let versions = read_sorted_dirs(elpa)?;
        if versions.len() != 1 {
            return Err(format!(
                "{}: expected exactly one emacs version directory, found {}",
                elpa.display(),
                versions.len()
            ));
        }
        let emacs_version = versions
            .first()
            .and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .ok_or_else(|| format!("{}: unreadable emacs version dir", elpa.display()))?;
        let mut packages = Vec::new();
        let mut pending = versions;
        while let Some(dir) = pending.pop() {
            for path in read_sorted_entries(&dir)? {
                let meta = fs::symlink_metadata(&path)
                    .map_err(|error| format!("stat {}: {error}", path.display()))?;
                if meta.is_dir() {
                    if let Some(package) = parse_package_dir(
                        &path
                            .file_name()
                            .map(|name| name.to_string_lossy())
                            .unwrap_or_default(),
                    ) {
                        packages.push(package);
                    } else {
                        // Layout scaffolding (e.g. `develop`, `archives`):
                        // descend, its children may be package builds.
                        pending.push(path);
                    }
                }
            }
        }
        packages.sort();
        Ok(Self::Elpa {
            emacs_version,
            packages,
        })
    }

    /// Straight layout: the sorted repo list, anchored by the digest of the
    /// single `build-*-cache.el` the bootstrap generated.
    fn from_straight(straight: &Path) -> Result<Self, String> {
        let mut build_caches = Vec::new();
        for path in read_sorted_entries(straight)? {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned());
            if name
                .as_deref()
                .is_some_and(|name| name.starts_with("build-") && name.ends_with("-cache.el"))
                && fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_file())
            {
                build_caches.push(path);
            }
        }
        if build_caches.len() != 1 {
            return Err(format!(
                "{}: expected exactly one build-*-cache.el, found {}",
                straight.display(),
                build_caches.len()
            ));
        }
        let build_cache_sha256 = {
            let bytes = fs::read(&build_caches[0])
                .map_err(|error| format!("read {}: {error}", build_caches[0].display()))?;
            sha256_hex(&bytes)
        };
        let mut repos: Vec<String> = read_sorted_dirs(&straight.join("repos"))?
            .into_iter()
            .filter_map(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .collect();
        repos.sort();
        Ok(Self::Straight {
            build_cache_sha256,
            repos,
        })
    }

    /// Serialize to the fixture's `PACKAGES` record (stable text).
    pub fn to_record(&self) -> String {
        match self {
            Self::Elpa {
                emacs_version,
                packages,
            } => {
                let mut out = format!(
                    "schema = 1\nlayout = elpa\nemacs_version = {emacs_version}\npackages = {}\n",
                    packages.len()
                );
                for package in packages {
                    out.push_str(&format!("{} {}\n", package.name, package.version));
                }
                out
            }
            Self::Straight {
                build_cache_sha256,
                repos,
            } => {
                let mut out = format!(
                    "schema = 1\nlayout = straight\nbuild_cache = {build_cache_sha256}\nrepos = {}\n",
                    repos.len()
                );
                for repo in repos {
                    out.push_str(repo);
                    out.push('\n');
                }
                out
            }
        }
    }

    /// Parse back a `PACKAGES` record.
    pub fn parse_record(text: &str) -> Result<Self, String> {
        let mut lines = text.lines();
        let schema = lines.next().ok_or("PACKAGES record is empty")?;
        if schema != "schema = 1" {
            return Err(format!("PACKAGES record has unknown {schema:?}"));
        }
        let layout = lines
            .next()
            .and_then(|line| line.strip_prefix("layout = "))
            .ok_or("PACKAGES record missing layout")?;
        match layout {
            "elpa" => {
                let emacs_version = lines
                    .next()
                    .and_then(|line| line.strip_prefix("emacs_version = "))
                    .ok_or("PACKAGES record missing emacs_version")?
                    .to_owned();
                let count: usize = lines
                    .next()
                    .and_then(|line| line.strip_prefix("packages = "))
                    .and_then(|count| count.parse().ok())
                    .ok_or("PACKAGES record missing packages count")?;
                let mut packages = Vec::with_capacity(count);
                for line in lines {
                    if line.is_empty() {
                        continue;
                    }
                    let (name, version) = line
                        .split_once(' ')
                        .ok_or_else(|| format!("PACKAGES line {line:?} is not `name version`"))?;
                    packages.push(InstalledPackage {
                        name: name.to_owned(),
                        version: version.to_owned(),
                    });
                }
                if packages.len() != count {
                    return Err(format!(
                        "PACKAGES records {count} packages but lists {}",
                        packages.len()
                    ));
                }
                Ok(Self::Elpa {
                    emacs_version,
                    packages,
                })
            }
            "straight" => {
                let build_cache_sha256 = lines
                    .next()
                    .and_then(|line| line.strip_prefix("build_cache = "))
                    .ok_or("PACKAGES record missing build_cache digest")?
                    .to_owned();
                let count: usize = lines
                    .next()
                    .and_then(|line| line.strip_prefix("repos = "))
                    .and_then(|count| count.parse().ok())
                    .ok_or("PACKAGES record missing repos count")?;
                let mut repos = Vec::with_capacity(count);
                for line in lines {
                    if line.is_empty() {
                        continue;
                    }
                    repos.push(line.to_owned());
                }
                if repos.len() != count {
                    return Err(format!(
                        "PACKAGES records {count} repos but lists {}",
                        repos.len()
                    ));
                }
                Ok(Self::Straight {
                    build_cache_sha256,
                    repos,
                })
            }
            other => Err(format!(
                "PACKAGES record has unknown layout {other:?} (known: elpa, straight)"
            )),
        }
    }
}
