//! The lifecycle every configuration fixture shares, written once.
//!
//! A fixture is staged under `target/infra/<name>-<rev12>/`, bootstrapped
//! by GNU Emacs (so every derived file is canonical, not GNU-shaped
//! guesswork), recorded in a `MANIFEST`, and then **sealed** read-only —
//! after which any write from an editor mounting it fails loudly instead
//! of silently dirtying it for every other session.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Whether `root` carries the seal: every write bit cleared.
///
/// The seal is a Unix permission-bits property, so a host without them
/// cannot hold a sealed fixture.  Answering `false` there keeps the
/// fixture *absent* rather than accepted-unsealed, which is what lets an
/// environment's `open` skip instead of running a suite against a
/// writable tree.
pub fn is_sealed(root: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(root)
            .map(|meta| meta.permissions().mode() & 0o222 == 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = root;
        false
    }
}

/// Link `target` at `link`.
///
/// Unix has one call; Windows splits it into file and directory forms and
/// wants a privilege a fixture cannot assume.  The refusal is unreachable
/// today -- `seal` stops a materialization before any copying starts --
/// but the crate still has to compile for the workspace's Windows
/// `cargo check`.
#[cfg(unix)]
pub fn symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(not(unix))]
pub fn symlink(_target: &Path, _link: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "symlinks are unavailable on this platform",
    ))
}

/// Overrides the fixture cache location (defaults to
/// `<workspace>/target/infra`).
pub const INFRA_CACHE_OVERRIDE: &str = "NEOMACS_INFRA_CACHE";

/// The pinned provenance of one fixture, from its `<name>-spec.toml`
/// beside this crate's manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    pub repo: String,
    pub revision: String,
    /// The SHA-256 of the sealed fixture's `PACKAGES` record, from the
    /// `packages = "<digest>"` line.  `None` means unpinned: the fixture
    /// exists, but no one has committed to its package identity yet.
    pub packages: Option<String>,
}

impl Spec {
    pub fn load(name: &str) -> Result<Self, String> {
        Self::load_from(Path::new(env!("CARGO_MANIFEST_DIR")), name)
    }

    /// [`Self::load`] anchored at an explicit spec directory — the seam the
    /// pin tests use; production resolves against this crate's manifest.
    pub fn load_from(manifest_dir: &Path, name: &str) -> Result<Self, String> {
        let path = manifest_dir.join(format!("{name}-spec.toml"));
        let text = fs::read_to_string(&path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        let mut repo = None;
        let mut revision = None;
        let mut packages = None;
        for line in text.lines() {
            let line = line.trim();
            if let Some(value) = line.strip_prefix("repo = ") {
                repo = Some(value.trim_matches('"').to_owned());
            } else if let Some(value) = line.strip_prefix("revision = ") {
                revision = Some(value.trim_matches('"').to_owned());
            } else if let Some(value) = line.strip_prefix("packages = ") {
                packages = Some(value.trim_matches('"').to_owned());
            }
        }
        Ok(Self {
            repo: repo.ok_or_else(|| format!("{name}-spec.toml: repo is missing"))?,
            revision: revision.ok_or_else(|| format!("{name}-spec.toml: revision is missing"))?,
            packages,
        })
    }

    /// Record the fixture's package-identity pin in the spec, with a log
    /// line for the re-baseline.
    ///
    /// Re-pinning is its own explicit ceremony — the fixture-check side
    /// refuses divergences, so this is the only legitimate way the pin
    /// changes, and it always leaves the log behind `parity-reference.toml`
    /// leaves its.
    pub fn pin_packages(
        manifest_dir: &Path,
        name: &str,
        digest: &str,
        note: &str,
    ) -> Result<(), String> {
        let path = manifest_dir.join(format!("{name}-spec.toml"));
        let text = fs::read_to_string(&path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        let pin_line = format!("packages = \"{digest}\"");
        // Rewrite the pin line in place if present, else append it; then
        // append the re-baseline log line.
        let mut lines: Vec<String> = text.lines().map(|line| line.to_owned()).collect();
        if let Some(existing) = lines
            .iter_mut()
            .find(|line| line.trim_start().starts_with("packages ="))
        {
            *existing = pin_line;
        } else {
            lines.push(pin_line);
        }
        lines.push(format!("#   {note}"));
        fs::write(&path, lines.join("\n") + "\n")
            .map_err(|error| format!("write {}: {error}", path.display()))
    }
}

/// The pin check a fixture must pass at open time: the fixture's sealed
/// `PACKAGES` digest must equal the spec's recorded pin.
///
/// An unpinned spec passes (nothing has committed to a package identity
/// yet — `infra status` surfaces it); a pinned divergence is an error that
/// names the ceremony, because silently mounting a re-built fixture is
/// exactly the divergence this exists to catch.
pub fn check_package_pin(
    pin: Option<&str>,
    name: &str,
    identity: &super::package_state::PackageStateIdentity,
) -> Result<(), String> {
    let Some(pinned) = pin else {
        return Ok(());
    };
    let fixture_digest = super::package_state::digest(&identity.to_record());
    if fixture_digest == *pinned {
        return Ok(());
    }
    Err(format!(
        "the fixture's package identity ({fixture_digest}) diverges from the \
         pinned identity ({pinned}); the spec was pinned against a different \
         package set\n\
         fix: re-materialize deliberately, then run \
         `cargo run -p xtask -- infra pin-packages {name}` to record it"
    ))
}

/// The fixture cache root: `$NEOMACS_INFRA_CACHE`, else the workspace's
/// `target/infra` (anchored at this crate's manifest, so every suite and
/// the xtask CLI resolve the same directory whatever their CWD).
pub fn cache_root() -> PathBuf {
    std::env::var_os(INFRA_CACHE_OVERRIDE).map_or_else(
        || {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(2)
                .unwrap_or(Path::new("."))
                .join("target")
                .join("infra")
        },
        PathBuf::from,
    )
}

/// The sealed fixture directory for one name and spec revision.
pub fn fixture_root(name: &str, revision: &str) -> PathBuf {
    cache_root().join(format!("{name}-{}", &revision[..12.min(revision.len())]))
}

/// A fixture root is openable when its MANIFEST exists, its tree exists,
/// and the seal (read-only root) is in place.
pub fn is_open_fixture(root: &Path) -> bool {
    root.join("MANIFEST").is_file() && root.join("tree").is_dir() && is_sealed(root)
}

/// Shallow fetch-by-SHA into `tree`: exactly the pinned revision without
/// cloning history.  github.com serves allow-reachable-SHA fetches, which
/// both doomemacs/core and syl20bnr/spacemacs rely on here.
pub fn shallow_fetch(repo: &str, revision: &str, tree: &Path) -> Result<(), String> {
    for (program, args) in [
        ("git", vec!["init".to_owned(), "--quiet".to_owned()]),
        (
            "git",
            vec![
                "remote".to_owned(),
                "add".to_owned(),
                "origin".to_owned(),
                repo.to_owned(),
            ],
        ),
        (
            "git",
            vec![
                "fetch".to_owned(),
                "--depth".to_owned(),
                "1".to_owned(),
                "origin".to_owned(),
                revision.to_owned(),
            ],
        ),
        (
            "git",
            vec![
                "checkout".to_owned(),
                "--quiet".to_owned(),
                "--detach".to_owned(),
                "FETCH_HEAD".to_owned(),
            ],
        ),
    ] {
        let status = Command::new(program)
            .args(&args)
            .current_dir(tree)
            .status()
            .map_err(|error| format!("spawn git {args:?}: {error}"))?;
        if !status.success() {
            return Err(format!("git {args:?} failed during clone"));
        }
    }
    Ok(())
}

/// Recursive copy preserving directory structure and symlinks (straight
/// and elpa symlink per-version build directories; replicate the link
/// itself rather than copying through it).  Permissions default to the
/// process umask; the seal fixes the final modes.
pub fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    let meta = fs::symlink_metadata(source)
        .map_err(|error| format!("stat {}: {error}", source.display()))?;
    if meta.is_symlink() {
        let target = fs::read_link(source)
            .map_err(|error| format!("readlink {}: {error}", source.display()))?;
        symlink(&target, destination).map_err(|error| {
            format!(
                "symlink {} -> {}: {error}",
                destination.display(),
                target.display()
            )
        })?;
        Ok(())
    } else if meta.is_dir() {
        fs::create_dir_all(destination)
            .map_err(|error| format!("create {}: {error}", destination.display()))?;
        let entries =
            fs::read_dir(source).map_err(|error| format!("read {}: {error}", source.display()))?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("read {}: {error}", source.display()))?;
            copy_tree(&entry.path(), &destination.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(source, destination)
            .map_err(|error| {
                format!(
                    "copy {} -> {}: {error}",
                    source.display(),
                    destination.display()
                )
            })
            .map(|_| ())
    }
}

/// Copy a tree for per-session use: identical to [`copy_tree`], then
/// restore owner write bits on everything copied.  The sources are
/// sealed (read-only), and `fs::copy` reproduces their permission bits —
/// a session that legitimately rewrites one of these files (Spacemacs
/// re-saves `.cache/spacemacs-buffer.el` on every boot) would otherwise
/// hit EACCES from its own seeded state.
#[cfg(unix)]
pub fn copy_tree_writable(source: &Path, destination: &Path) -> Result<(), String> {
    copy_tree(source, destination)?;
    fn unwall(path: &Path) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        let meta = fs::symlink_metadata(path)
            .map_err(|error| format!("stat {}: {error}", path.display()))?;
        if meta.is_dir() {
            let mut permissions = meta.permissions();
            permissions.set_mode(meta.permissions().mode() | 0o700);
            fs::set_permissions(path, permissions)
                .map_err(|error| format!("unseal {}: {error}", path.display()))?;
            let entries =
                fs::read_dir(path).map_err(|error| format!("read {}: {error}", path.display()))?;
            for entry in entries {
                unwall(
                    &entry
                        .map_err(|error| format!("read {}: {error}", path.display()))?
                        .path(),
                )?;
            }
        } else if meta.is_file() {
            let mut permissions = meta.permissions();
            permissions.set_mode(meta.permissions().mode() | 0o600);
            fs::set_permissions(path, permissions)
                .map_err(|error| format!("unseal {}: {error}", path.display()))?;
        }
        Ok(())
    }
    unwall(destination)
}

/// A no-op where there are no permission bits to restore; the seal
/// refusal already prevents reaching here on such hosts.
#[cfg(not(unix))]
pub fn copy_tree_writable(source: &Path, destination: &Path) -> Result<(), String> {
    copy_tree(source, destination)
}

/// Recursively strip every write bit under `root`, sealing the fixture
/// against writes from the editors that mount it.  This is failure
/// isolation between tests, not a security boundary: a process running as
/// the owning user could chmod the bits back, but editors do not.
#[cfg(unix)]
pub fn seal(root: &Path) -> Result<(), String> {
    fn walk(path: &Path) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        let meta = fs::symlink_metadata(path)
            .map_err(|error| format!("stat {}: {error}", path.display()))?;
        if meta.is_dir() {
            let mut permissions = meta.permissions();
            permissions.set_mode(meta.permissions().mode() & !0o222);
            fs::set_permissions(path, permissions)
                .map_err(|error| format!("seal {}: {error}", path.display()))?;
            let entries =
                fs::read_dir(path).map_err(|error| format!("read {}: {error}", path.display()))?;
            for entry in entries {
                walk(
                    &entry
                        .map_err(|error| format!("read {}: {error}", path.display()))?
                        .path(),
                )?;
            }
        } else if meta.is_file() {
            let mut permissions = meta.permissions();
            permissions.set_mode(meta.permissions().mode() & !0o222);
            fs::set_permissions(path, permissions)
                .map_err(|error| format!("seal {}: {error}", path.display()))?;
        }
        Ok(())
    }
    walk(root)
}

/// Write the MANIFEST, record the content inventory the fixture is verified
/// against, then seal — the tail every environment's materialize shares.
///
/// The identity chain is MANIFEST -> INVENTORY -> bytes: `INVENTORY` hashes
/// every fixture file, and `MANIFEST` records the digest of those hashes.
/// The two record files are excluded from their own content — a file cannot
/// contain a digest of itself, and the root of the chain is trusted the way
/// `parity-reference.toml` is — so verification rebuilds the current state
/// excluding `MANIFEST` and `INVENTORY` and compares those bytes only.
pub fn manifest_and_seal(root: &Path, name: &str, source_note: &str) -> Result<(), String> {
    // Hash the fixture content, excluding both record files.  Neither is
    // final yet, and their bytes are exactly what the two writes below fix.
    let inventory = super::inventory::Inventory::build_excluding(root, RECORD_FILES)?;
    let inventory_text = inventory.to_jsonl();
    let inventory_digest = super::inventory::sha256_hex(inventory_text.as_bytes());
    fs::write(root.join(INVENTORY_FILE), &inventory_text)
        .map_err(|error| format!("write INVENTORY: {error}"))?;
    fs::write(
        root.join("MANIFEST"),
        format!("name = {name}\nsource = {source_note}\ninventory = {inventory_digest}\n"),
    )
    .map_err(|error| format!("write MANIFEST: {error}"))?;
    seal(root)?;
    // Self-check: the sealed fixture must verify clean against the inventory
    // it was just recorded with.  A drift here means the bootstrap wrote
    // after the inventory was taken, or the seal missed a path -- fail
    // loudly now, not six hours later in a mysterious parity divergence.
    let drift = verify_sealed_fixture(root)?;
    if !drift.is_clean() {
        return Err(format!(
            "{name} fixture failed post-seal self-check: {} missing, {} modified, {} added",
            drift.missing.len(),
            drift.modified.len(),
            drift.added.len()
        ));
    }
    Ok(())
}

/// The file the fixture's sealed content inventory is recorded under.
pub const INVENTORY_FILE: &str = "INVENTORY";

/// The file the fixture's installed-package identity is recorded under.
pub const PACKAGES_FILE: &str = "PACKAGES";

/// Read the sealed `PACKAGES` record of a fixture, if it has one.
///
/// `Err` for a fixture that predates the record.
pub fn load_sealed_packages(
    root: &Path,
) -> Result<super::package_state::PackageStateIdentity, String> {
    let text = fs::read_to_string(root.join(PACKAGES_FILE)).map_err(|error| {
        format!(
            "{}: cannot read the sealed package record: {error}\n\
                 (a fixture without one does not record its package identity; \
                 re-materialize)",
            root.join(PACKAGES_FILE).display()
        )
    })?;
    super::package_state::PackageStateIdentity::parse_record(&text)
}

/// The two record files: the roots and subjects of the identity chain.
/// Both are excluded from content hashing — `INVENTORY` cannot contain its
/// own digest, and `MANIFEST` is written after the inventory walk — and
/// `MANIFEST` is trusted as the chain root, as `parity-reference.toml` is.
const RECORD_FILES: &[&str] = &["MANIFEST", INVENTORY_FILE];

/// Load the sealed content inventory of a fixture.
///
/// `Err` for a fixture that predates the inventory record — such a fixture
/// cannot prove byte-identity, and the honest answer is to re-materialize.
pub fn load_sealed_inventory(root: &Path) -> Result<super::inventory::Inventory, String> {
    let text = fs::read_to_string(root.join(INVENTORY_FILE)).map_err(|error| {
        format!(
            "{}: cannot read the sealed content inventory: {error}\n             (a fixture without one cannot prove byte-identity; re-materialize)",
            root.join(INVENTORY_FILE).display()
        )
    })?;
    let manifest = fs::read_to_string(root.join("MANIFEST"))
        .map_err(|error| format!("read MANIFEST: {error}"))?;
    let recorded_digest = manifest_field(&manifest, "inventory").ok_or_else(|| {
        "MANIFEST does not record an inventory digest; the fixture predates the \
         MANIFEST -> INVENTORY -> bytes chain and must be re-materialized"
    })?;
    if recorded_digest != super::inventory::sha256_hex(text.as_bytes()) {
        return Err(
            "the sealed content inventory does not match the digest MANIFEST \
             recorded for it; the fixture record was mutated after sealing"
                .to_owned(),
        );
    }
    super::inventory::Inventory::parse_jsonl(&text)
}

/// One `key = value` line from a MANIFEST, if present.
fn manifest_field(manifest: &str, key: &str) -> Option<String> {
    let line = manifest
        .lines()
        .find(|line| line.starts_with(key) && line[key.len()..].trim_start().starts_with('='))?;
    let value = line[line.find('=').unwrap() + 1..].trim();
    Some(value.to_owned())
}

/// Verify a fixture against the inventory it was sealed with.
///
/// Not "build an inventory and compare the tree against itself": the stored
/// record is the authority, so any post-seal mutation reports as drift.
pub fn verify_sealed_fixture(root: &Path) -> Result<super::inventory::Drift, String> {
    let inventory = load_sealed_inventory(root)?;
    super::inventory::verify_deep(root, &inventory, RECORD_FILES)
}

/// Build the content inventory of a sealed fixture root.
pub fn build_inventory(root: &Path) -> Result<super::inventory::Inventory, String> {
    super::inventory::Inventory::build(root)
}

/// Windows has no permission bits to clear, so the fixture cannot be
/// sealed -- and for the editors that mount it, being sealed is the
/// property that makes mounting safe.  Refusing keeps a host from
/// materializing a writable tree that `open` would then have to guess
/// about; `is_sealed` answers `false` for the same reason.  Unreachable
/// in the suites, which are Unix-only; the crate still has to compile
/// for the workspace's Windows `cargo check`.
#[cfg(not(unix))]
pub fn seal(_root: &Path) -> Result<(), String> {
    Err(
        "the fixture seal needs Unix permission bits; this platform \
         cannot materialize the fixture"
            .to_owned(),
    )
}
