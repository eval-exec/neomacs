//! The Doom Emacs fixture: a GNU-bootstrapped (`bin/doom sync`), sealed
//! Doom checkout that sessions mount via `--init-directory`, with
//! writable state redirected through Doom's native `DOOMLOCALDIR`.

use super::ConfigEnvironment;
use super::common::{self, Spec};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Environment variable naming an operator's Doom checkout to copy instead
/// of cloning the pinned spec.  The operator's tree is never sealed or
/// written: materialization copies it into the cache first.
pub const DOOM_ROOT_OVERRIDE: &str = "NEOMACS_INFRA_DOOM_ROOT";

/// The pinned provenance of the Doom fixture, from `doom-spec.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoomSpec {
    pub repo: String,
    pub revision: String,
}

impl DoomSpec {
    fn load() -> Result<Self, String> {
        let spec = Spec::load("doom")?;
        Ok(Self {
            repo: spec.repo,
            revision: spec.revision,
        })
    }
}

/// Where a materialization takes the Doom tree from.
#[derive(Debug, Clone)]
pub enum DoomSource {
    /// Clone the pinned `doom-spec.toml` revision (network).
    Pinned,
    /// Copy an operator's checkout (its provenance is recorded, but the
    /// fixture identity is still the pinned revision).
    Operator(PathBuf),
}

impl DoomSource {
    /// The source a bare `materialize` uses: the operator override when
    /// set, else the pinned spec.
    pub fn resolve() -> Result<Self, String> {
        match std::env::var_os(DOOM_ROOT_OVERRIDE) {
            Some(path) => Ok(Self::Operator(PathBuf::from(path))),
            None => Ok(Self::Pinned),
        }
    }
}

/// A materialized, sealed Doom fixture.  Cheap to reopen: [`Self::open`]
/// only checks the manifest and the seal.
#[derive(Debug, Clone)]
pub struct DoomEnvironment {
    pub(super) root: PathBuf,
}

impl DoomEnvironment {
    /// The sealed fixture for the pinned revision, if materialized.
    /// `None` makes callers skip fast rather than build.
    pub fn open() -> Option<Self> {
        let revision = DoomSpec::load().ok()?.revision;
        let root = common::fixture_root("doom", &revision);
        common::is_open_fixture(&root).then_some(Self { root })
    }

    /// Bootstrap the fixture with GNU Emacs and seal it.  Safe to re-run:
    /// an already-sealed fixture for the pinned revision is reused.
    pub fn materialize(source: DoomSource) -> Result<Self, String> {
        let spec = DoomSpec::load()?;
        let root = common::fixture_root("doom", &spec.revision);
        if common::is_open_fixture(&root) {
            return Ok(Self { root });
        }
        if root.exists() {
            // An unsealed leftover of a failed materialization.
            fs::remove_dir_all(&root)
                .map_err(|error| format!("clean {}: {error}", root.display()))?;
        }
        let tree = root.join("tree");
        let home = root.join("home");
        fs::create_dir_all(&tree).map_err(|error| format!("create {}: {error}", tree.display()))?;
        fs::create_dir_all(&home).map_err(|error| format!("create {}: {error}", home.display()))?;

        match source {
            DoomSource::Pinned => common::shallow_fetch(&spec.repo, &spec.revision, &tree)?,
            DoomSource::Operator(ref path) => {
                common::copy_tree(path, &tree)?;
            }
        }

        // Files copied from an operator checkout bake the operator's
        // absolute paths and system fingerprint into the generated
        // profile/envvar state, which then dangles or trips "system has
        // changed" prompts once the tree lives here.  Drop every
        // generated child of .local — keeping only the straight/ package
        // builds — so the sync below regenerates them at the fixture's
        // own location.
        let local = tree.join(".local");
        fs::create_dir_all(&local)
            .map_err(|error| format!("create {}: {error}", local.display()))?;
        let entries =
            fs::read_dir(&local).map_err(|error| format!("read {}: {error}", local.display()))?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("read {}: {error}", local.display()))?;
            if entry.file_name() == "straight" {
                continue;
            }
            let path = entry.path();
            fs::remove_dir_all(&path)
                .map_err(|error| format!("remove {}: {error}", path.display()))?;
        }

        // GNU Emacs generates every derived file — autoloads, package
        // builds, caches — so the fixture is canonical, not GNU-shaped
        // guesswork.  HOME and DOOMLOCALDIR keep the bootstrap inside the
        // fixture.  A full `doom sync` regenerates the wiped profile; the
        // one prompt it can raise ("system has changed … rebuild?") is
        // answered by piped stdin, and answering yes rebuilds the
        // packages against this fixture's Emacs — the canonicalization
        // this step exists for.
        let local_dir = tree.join(".local");
        let mut child = Command::new("sh")
            .arg(tree.join("bin/doom"))
            .args(["sync"])
            .env("EMACSDIR", &tree)
            .env("HOME", &home)
            .env("DOOMLOCALDIR", &local_dir)
            .env_remove("DOOMDIR")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .map_err(|error| format!("spawn bin/doom sync: {error}"))?;
        use std::io::Write as _;
        if let Some(stdin) = child.stdin.as_mut() {
            writeln!(stdin, "y").map_err(|error| format!("answer bin/doom prompt: {error}"))?;
        }
        let status = child
            .wait()
            .map_err(|error| format!("wait bin/doom sync: {error}"))?;
        if !status.success() {
            return Err("bin/doom sync failed during materialization".to_owned());
        }

        let source_note = match &source {
            DoomSource::Pinned => format!("{} @ {}", spec.repo, spec.revision),
            DoomSource::Operator(path) => {
                format!("operator copy of {} (@ {})", path.display(), spec.revision)
            }
        };
        common::manifest_and_seal(&root, "doom", &source_note)?;
        common::is_open_fixture(&root)
            .then_some(Self { root })
            .ok_or_else(|| "sealed fixture did not reopen".to_owned())
    }
}

impl ConfigEnvironment for DoomEnvironment {
    fn name(&self) -> &'static str {
        "doom"
    }

    fn tree(&self) -> PathBuf {
        self.root.join("tree")
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    /// Seed one session's state directory.  [`Self::session_env`] points
    /// the session's `DOOMLOCALDIR` at `<state>/.doom-local`; Doom reads
    /// its per-Emacs-version profile init and loaddefs from there, so a
    /// fresh directory reads as "never synced".  The fixture's small
    /// generated state (`.local/{cache,etc,state}`, a few MB) is copied
    /// in, while the multi-hundred-MB `straight/` package builds are
    /// shared by symlink.
    fn prepare_session_state(&self, session_state: &Path) -> Result<(), String> {
        let local = session_state.join(".doom-local");
        fs::create_dir_all(&local)
            .map_err(|error| format!("create {}: {error}", local.display()))?;
        for name in ["cache", "etc", "state"] {
            let source = self.tree().join(".local").join(name);
            if source.is_dir() {
                common::copy_tree_writable(&source, &local.join(name))?;
            }
        }
        let straight = self.tree().join(".local").join("straight");
        let link = local.join("straight");
        if straight.is_dir() && !link.exists() {
            super::common::symlink(&straight, &link)
                .map_err(|error| format!("symlink {}: {error}", link.display()))?;
        }
        Ok(())
    }

    fn session_env(&self, session_state: &Path) -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
        let mut env = vec![
            ("HOME".into(), self.home().as_os_str().to_owned()),
            (
                "DOOMLOCALDIR".into(),
                session_state.join(".doom-local").into_os_string(),
            ),
        ];
        env.extend(super::session_xdg_env(session_state));
        env
    }

    fn session_args(&self) -> Vec<String> {
        vec![
            "--init-directory".to_owned(),
            self.tree().to_string_lossy().into_owned(),
        ]
    }

    fn verify_deep(&self) -> Result<crate::config_env::inventory::Drift, String> {
        let inventory = crate::config_env::inventory::Inventory::build(&self.root)?;
        crate::config_env::inventory::verify_deep(&self.root, &inventory)
    }
}

/// [`DoomEnvironment::open`] for callers that want the resolution status.
pub fn doom_status() -> Result<String, String> {
    if DoomEnvironment::open().is_some() {
        return Ok("materialized and sealed".to_owned());
    }
    DoomSpec::load()?;
    match DoomSource::resolve() {
        Ok(source) => Err(format!(
            "not materialized (would build from {})",
            match source {
                DoomSource::Pinned => "the pinned doom-spec.toml clone".to_owned(),
                DoomSource::Operator(path) => format!("operator checkout {}", path.display()),
            }
        )),
        Err(error) => Err(error),
    }
}
