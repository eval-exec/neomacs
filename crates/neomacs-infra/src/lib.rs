//! Shared editor-configuration fixtures for the neomacs test suites.
//!
//! See the crate comment in `Cargo.toml` for the model.  The short version:
//! [`DoomEnvironment`] is a GNU-bootstrapped, sealed, read-only Doom Emacs
//! fixture that any suite (TUI today, GUI next) mounts per session with
//! [`display`] supplying the deterministic, isolated display sessions
//! (loopback-TCP Xvfb, weston-headless) those suites run their scenarios on.
//! [`DoomEnvironment::session_env`] and [`DoomEnvironment::session_args`],
//! never writing into it.

pub mod display;

use std::fs;
use std::os::unix::fs::PermissionsExt;
/// The workspace root baked in at compile time.
///
/// This is the **build** machine's path — for binaries shipped through
/// `cargo nextest archive`, wrong on every other runner.  Only
/// [`workspace_root`] should read it; call sites never choose it directly.
pub fn cargo_workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_WORKSPACE_DIR"))
}

/// The workspace root nextest exports at runtime: the live workspace on
/// the machine *running* the test, already adjusted by
/// `--workspace-remap`.  `None` outside nextest (`cargo test`, plain
/// `cargo run`).
pub fn nextest_workspace_root() -> Option<PathBuf> {
    std::env::var_os("NEXTEST_WORKSPACE_ROOT").map(PathBuf::from)
}

/// The workspace root of the machine *running* the test: nextest's
/// runtime value when present, the compile-time constant otherwise.
///
/// One archive job landing on a runner pool with a different home
/// (`/home/ubuntu` vs `/home/runner`) turned every downstream artifact
/// write into EACCES and wiped out a whole CI run — which is why this
/// fallback order lives here, once, instead of at each call site.
pub fn workspace_root() -> PathBuf {
    nextest_workspace_root().unwrap_or_else(cargo_workspace_root)
}
use std::path::{Path, PathBuf};
use std::process::Command;

/// Environment variable naming an operator's Doom checkout to copy instead
/// of cloning the pinned spec.  The operator's tree is never sealed or
/// written: materialization copies it into the cache first.
pub const DOOM_ROOT_OVERRIDE: &str = "NEOMACS_INFRA_DOOM_ROOT";

/// Overrides the fixture cache location (defaults to
/// `<workspace>/target/neomacs-infra`).
pub const INFRA_CACHE_OVERRIDE: &str = "NEOMACS_INFRA_CACHE";

/// The pinned provenance of the Doom fixture, from `doom-spec.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoomSpec {
    pub repo: String,
    pub revision: String,
}

impl DoomSpec {
    fn load() -> Result<Self, String> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("doom-spec.toml");
        let text = fs::read_to_string(&path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        let mut repo = None;
        let mut revision = None;
        for line in text.lines() {
            let line = line.trim();
            if let Some(value) = line.strip_prefix("repo = ") {
                repo = Some(value.trim_matches('"').to_owned());
            } else if let Some(value) = line.strip_prefix("revision = ") {
                revision = Some(value.trim_matches('"').to_owned());
            }
        }
        Ok(Self {
            repo: repo.ok_or("doom-spec.toml: repo is missing")?,
            revision: revision.ok_or("doom-spec.toml: revision is missing")?,
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
    root: PathBuf,
}

impl DoomEnvironment {
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

    /// The sealed fixture directory for the pinned revision, if it has been
    /// materialized.  `None` makes callers skip fast rather than build.
    pub fn open() -> Option<Self> {
        let revision = DoomSpec::load().ok()?.revision;
        let root = Self::cache_root().join(format!("doom-{}", &revision[..12.min(revision.len())]));
        let tree = root.join("tree");
        let sealed = fs::metadata(&root)
            .map(|meta| meta.permissions().mode() & 0o222 == 0)
            .unwrap_or(false);
        (root.join("MANIFEST").is_file() && tree.is_dir() && sealed).then_some(Self { root })
    }

    /// Bootstrap the fixture with GNU Emacs and seal it.  Safe to re-run:
    /// an already-sealed fixture for the pinned revision is reused.
    pub fn materialize(source: DoomSource) -> Result<Self, String> {
        let spec = DoomSpec::load()?;
        let root = Self::cache_root().join(format!(
            "doom-{}",
            &spec.revision[..12.min(spec.revision.len())]
        ));
        if let Some(existing) = Self::open_at(&root) {
            return Ok(existing);
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
            DoomSource::Pinned => {
                // Shallow fetch-by-SHA keeps the fixture reproducible
                // (exactly the pinned revision) without cloning history;
                // github.com serves allow-reachable-SHA fetches, which
                // doomemacs/core (bin/doom included) relies on here.
                for (program, args) in [
                    ("git", vec!["init".to_owned(), "--quiet".to_owned()]),
                    (
                        "git",
                        vec![
                            "remote".to_owned(),
                            "add".to_owned(),
                            "origin".to_owned(),
                            spec.repo.clone(),
                        ],
                    ),
                    (
                        "git",
                        vec![
                            "fetch".to_owned(),
                            "--depth".to_owned(),
                            "1".to_owned(),
                            "origin".to_owned(),
                            spec.revision.clone(),
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
                        .current_dir(&tree)
                        .status()
                        .map_err(|error| format!("spawn git {args:?}: {error}"))?;
                    if !status.success() {
                        return Err(format!("git {args:?} failed during clone"));
                    }
                }
            }
            DoomSource::Operator(ref path) => {
                copy_tree(path, &tree)?;
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
        fs::write(
            root.join("MANIFEST"),
            format!("name = doom\nsource = {source_note}\n"),
        )
        .map_err(|error| format!("write MANIFEST: {error}"))?;

        seal(&root)?;
        Self::open_at(&root).ok_or_else(|| "sealed fixture did not reopen".to_owned())
    }

    /// The read-only Doom tree to pass via `--init-directory`.
    pub fn tree(&self) -> PathBuf {
        self.root.join("tree")
    }

    /// The read-only bootstrap HOME shared by every session.
    pub fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    /// Seed one session's state directory.  `session_env` points the
    /// session's `DOOMLOCALDIR` at `<state>/.doom-local`; Doom reads its
    /// per-Emacs-version profile init and loaddefs from there, so a fresh
    /// directory reads as "never synced".  The fixture's small generated
    /// state (`.local/{cache,etc,state}`, a few MB) is copied in, while
    /// the multi-hundred-MB `straight/` package builds are shared by
    /// symlink: runtime only reads them, and a write attempt hits the
    /// seal and fails loudly.
    pub fn prepare_session_state(&self, session_state: &Path) -> Result<(), String> {
        let local = session_state.join(".doom-local");
        fs::create_dir_all(&local)
            .map_err(|error| format!("create {}: {error}", local.display()))?;
        for name in ["cache", "etc", "state"] {
            let source = self.tree().join(".local").join(name);
            if source.is_dir() {
                copy_tree(&source, &local.join(name))?;
            }
        }
        let straight = self.tree().join(".local").join("straight");
        let link = local.join("straight");
        if straight.is_dir() && !link.exists() {
            std::os::unix::fs::symlink(&straight, &link).map_err(|error| {
                format!(
                    "symlink {} -> {}: {error}",
                    link.display(),
                    straight.display()
                )
            })?;
        }
        Ok(())
    }

    /// Environment for one editor session mounting this fixture.
    /// `session_state` is a caller-owned per-session directory (typically a
    /// `TuiTempDirectory`) prepared by [`Self::prepare_session_state`];
    /// every writable Doom location lands inside it.
    pub fn session_env(
        &self,
        session_state: &Path,
    ) -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
        vec![
            ("HOME".into(), self.home().as_os_str().to_owned()),
            (
                "DOOMLOCALDIR".into(),
                session_state.join(".doom-local").into_os_string(),
            ),
        ]
    }

    /// Command arguments mounting this fixture's tree.
    pub fn session_args(&self) -> Vec<String> {
        vec![
            "--init-directory".to_owned(),
            self.tree().to_string_lossy().into_owned(),
        ]
    }

    fn open_at(root: &Path) -> Option<Self> {
        let sealed = fs::metadata(root)
            .map(|meta| meta.permissions().mode() & 0o222 == 0)
            .unwrap_or(false);
        (root.join("MANIFEST").is_file() && root.join("tree").is_dir() && sealed).then(|| Self {
            root: root.to_owned(),
        })
    }
}

/// Recursive copy preserving directory structure (permissions default to
/// the process umask; the seal fixes the final modes).
fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    let meta = fs::symlink_metadata(source)
        .map_err(|error| format!("stat {}: {error}", source.display()))?;
    if meta.is_symlink() {
        // straight symlinks per-Emacs-version build directories; replicate
        // the link itself rather than copying through it.
        let target = fs::read_link(source)
            .map_err(|error| format!("readlink {}: {error}", source.display()))?;
        std::os::unix::fs::symlink(&target, destination).map_err(|error| {
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

/// Recursively strip every write bit under `root`, sealing the fixture
/// against writes from the editors that mount it.  This is failure
/// isolation between tests, not a security boundary: a process running as
/// the owning user could chmod the bits back, but editors do not.
fn seal(root: &Path) -> Result<(), String> {
    fn walk(path: &Path) -> Result<(), String> {
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

/// `DoomEnvironment::open` for callers that want the resolution status.
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
