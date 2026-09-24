//! The Spacemacs fixture: a GNU-bootstrapped (first-run package
//! install), sealed Spacemacs checkout that sessions mount through HOME
//! — the distribution *is* `~/.emacs.d`.
//!
//! Spacemacs has no `DOOMLOCALDIR`-style redirect: its writable state
//! lives inside `.emacs.d` itself (`elpa/`, `.cache/`).  The session
//! model is therefore a **HOME overlay**: `<state>/.emacs.d` is a real
//! directory whose entries symlink into the sealed tree, except `elpa`
//! (shared read-only from the fixture's package state, like Doom's
//! `straight/`) and `.cache` (copied per session, a few MB).  A package
//! install attempt hits the seal and fails loudly, which is the honest
//! failure: fixtures are comparison witnesses, not sandboxes for growth.
//!
//! The user dotfile (`<state>/.spacemacs`) symlinks to the fixture's
//! deterministic dotfile — without one, Spacemacs runs its interactive
//! first-run wizard that no harness may answer.

use super::ConfigEnvironment;
use super::common::{self, Spec};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Environment variable naming an operator's Spacemacs checkout to copy
/// instead of cloning the pinned spec.
pub const SPACEMACS_ROOT_OVERRIDE: &str = "NEOMACS_INFRA_SPACEMACS_ROOT";

/// Where a materialization takes the Spacemacs tree from.
#[derive(Debug, Clone)]
pub enum SpacemacsSource {
    /// Clone the pinned `spacemacs-spec.toml` revision (network).
    Pinned,
    /// Copy an operator's checkout (fixture identity is still the pinned
    /// revision).
    Operator(PathBuf),
}

impl SpacemacsSource {
    pub fn resolve() -> Result<Self, String> {
        match std::env::var_os(SPACEMACS_ROOT_OVERRIDE) {
            Some(path) => Ok(Self::Operator(PathBuf::from(path))),
            None => Ok(Self::Pinned),
        }
    }
}

/// A materialized, sealed Spacemacs fixture.
#[derive(Debug, Clone)]
pub struct SpacemacsEnvironment {
    pub(super) root: PathBuf,
}

impl SpacemacsEnvironment {
    pub fn open() -> Option<Self> {
        let revision = Spec::load("spacemacs").ok()?.revision;
        let root = common::fixture_root("spacemacs", &revision);
        common::is_open_fixture(&root).then_some(Self { root })
    }

    pub fn materialize(source: SpacemacsSource) -> Result<Self, String> {
        let spec = Spec::load("spacemacs")?;
        let root = common::fixture_root("spacemacs", &spec.revision);
        if common::is_open_fixture(&root) {
            return Ok(Self { root });
        }
        if root.exists() {
            fs::remove_dir_all(&root)
                .map_err(|error| format!("clean {}: {error}", root.display()))?;
        }
        let tree = root.join("tree");
        let home = root.join("home");
        fs::create_dir_all(&tree).map_err(|error| format!("create {}: {error}", tree.display()))?;
        fs::create_dir_all(&home).map_err(|error| format!("create {}: {error}", home.display()))?;

        match source {
            SpacemacsSource::Pinned => common::shallow_fetch(&spec.repo, &spec.revision, &tree)?,
            SpacemacsSource::Operator(ref path) => common::copy_tree(path, &tree)?,
        }

        // The deterministic dotfile suppresses the first-run wizard; the
        // bootstrap overlay mirrors the session layout so the bootstrap
        // sees exactly what sessions see.
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/spacemacs-dotfile.el"),
            home.join(".spacemacs"),
        )
        .map_err(|error| format!("install fixture dotfile: {error}"))?;
        let bootstrap = root.join("bootstrap-home");
        overlay_emacs_d(&tree, &bootstrap, None)?;
        fs::copy(home.join(".spacemacs"), bootstrap.join(".spacemacs"))
            .map_err(|error| format!("install bootstrap dotfile: {error}"))?;

        // GNU Emacs performs the first-run package install — the fixture
        // is canonical, not reconstructed.  Batch keeps the bootstrap
        // scriptable; package.el installs the core layers' packages into
        // the overlay's writable elpa.
        let status = Command::new("emacs")
            .arg("--batch")
            .arg("-l")
            .arg(bootstrap.join(".emacs.d/init.el"))
            .env("HOME", &bootstrap)
            .env_remove("SPACEMACSDIR")
            .status()
            .map_err(|error| format!("spawn emacs bootstrap: {error}"))?;
        if !status.success() {
            return Err("spacemacs first-run bootstrap failed".to_owned());
        }

        // Extract the generated package state beside the tree: elpa is
        // the multi-hundred-MB share (symlinked per session), .cache the
        // small per-session copy.  The bootstrap home is then discarded
        // — sessions build their own overlay from the sealed pieces.
        let state = root.join("package-state");
        fs::create_dir_all(&state)
            .map_err(|error| format!("create {}: {error}", state.display()))?;
        for name in ["elpa", ".cache"] {
            let generated = bootstrap.join(".emacs.d").join(name);
            if generated.is_dir() {
                fs::rename(&generated, state.join(name))
                    .map_err(|error| format!("extract {name}: {error}"))?;
            }
        }
        fs::remove_dir_all(&bootstrap)
            .map_err(|error| format!("clean {}: {error}", bootstrap.display()))?;

        let source_note = match &source {
            SpacemacsSource::Pinned => format!("{} @ {}", spec.repo, spec.revision),
            SpacemacsSource::Operator(path) => {
                format!("operator copy of {} (@ {})", path.display(), spec.revision)
            }
        };
        common::manifest_and_seal(&root, "spacemacs", &source_note)?;
        common::is_open_fixture(&root)
            .then_some(Self { root })
            .ok_or_else(|| "sealed fixture did not reopen".to_owned())
    }

    /// The fixture's extracted generated package state (`elpa/`,
    /// `.cache/`) produced by the GNU bootstrap.
    fn package_state(&self) -> PathBuf {
        self.root.join("package-state")
    }
}

/// Build an overlay `.emacs.d`: a real directory whose entries symlink to
/// the sealed tree's entries, except the writable ones.  `link_elpa`
/// names a directory (inside the fixture) to link `elpa` at — `None`
/// leaves `elpa`/`.cache` as fresh empty dirs for the bootstrap to fill.
fn overlay_emacs_d(tree: &Path, home: &Path, link_elpa: Option<&Path>) -> Result<(), String> {
    let emacs_d = home.join(".emacs.d");
    fs::create_dir_all(&emacs_d)
        .map_err(|error| format!("create {}: {error}", emacs_d.display()))?;
    let entries =
        fs::read_dir(tree).map_err(|error| format!("read {}: {error}", tree.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("read {}: {error}", tree.display()))?;
        let name = entry.file_name();
        if name == ".git" {
            // The shallow-fetch metadata: pure weight in an .emacs.d
            // overlay, and its objects are large.
            continue;
        }
        let destination = emacs_d.join(&name);
        super::common::symlink(&entry.path(), &destination)
            .map_err(|error| format!("symlink {}: {error}", destination.display()))?;
    }
    for name in ["elpa", ".cache"] {
        let dir = emacs_d.join(name);
        if dir.exists() {
            continue;
        }
        if name == "elpa" {
            if let Some(elpa) = link_elpa {
                super::common::symlink(elpa, &dir)
                    .map_err(|error| format!("symlink {}: {error}", dir.display()))?;
                continue;
            }
        }
        fs::create_dir_all(&dir).map_err(|error| format!("create {}: {error}", dir.display()))?;
    }
    Ok(())
}

impl ConfigEnvironment for SpacemacsEnvironment {
    fn name(&self) -> &'static str {
        "spacemacs"
    }

    fn tree(&self) -> PathBuf {
        self.root.join("tree")
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    fn prepare_session_state(&self, session_state: &Path) -> Result<(), String> {
        overlay_emacs_d(
            &self.tree(),
            session_state,
            Some(&self.package_state().join("elpa")),
        )?;
        // The per-session .cache: small, writable, seeded from the
        // bootstrap's.
        let cache = self.package_state().join(".cache");
        if cache.is_dir() {
            common::copy_tree_writable(&cache, &session_state.join(".emacs.d/.cache"))?;
        }
        // The deterministic dotfile, read-only through the seal.
        let dotfile = session_state.join(".spacemacs");
        if !dotfile.exists() {
            super::common::symlink(&self.home().join(".spacemacs"), &dotfile)
                .map_err(|error| format!("symlink {}: {error}", dotfile.display()))?;
        }
        Ok(())
    }

    fn session_env(&self, session_state: &Path) -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
        // Spacemacs mounts through HOME alone: the distribution IS
        // ~/.emacs.d, and the session state directory is the session HOME.
        let mut env = vec![("HOME".into(), session_state.as_os_str().to_owned())];
        env.extend(super::session_xdg_env(session_state));
        env
    }

    fn session_args(&self) -> Vec<String> {
        Vec::new()
    }

    fn verify_deep(&self) -> Result<crate::config_env::inventory::Drift, String> {
        common::verify_sealed_fixture(&self.root)
    }
}

/// [`SpacemacsEnvironment::open`] for callers that want the resolution
/// status.
pub fn spacemacs_status() -> Result<String, String> {
    if SpacemacsEnvironment::open().is_some() {
        return Ok("materialized and sealed".to_owned());
    }
    Spec::load("spacemacs")?;
    match SpacemacsSource::resolve() {
        Ok(source) => Err(format!(
            "not materialized (would build from {})",
            match source {
                SpacemacsSource::Pinned => "the pinned spacemacs-spec.toml clone".to_owned(),
                SpacemacsSource::Operator(path) => {
                    format!("operator checkout {}", path.display())
                }
            }
        )),
        Err(error) => Err(error),
    }
}
