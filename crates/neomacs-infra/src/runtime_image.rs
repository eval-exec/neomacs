//! The editor's bootstrap runtime image, provisioned by the suites that need it.
//!
//! `tests/daemon_lifecycle.rs` runs the real `neomacs` daemon, and the daemon
//! boots from the runtime image beside its own binary
//! (`RuntimeImageRole::Bootstrap`, resolved by the editor's own loader).  No
//! `cargo build` produces that image: it is dumped by a byte copy of the
//! editor under its temacs role name, bootstrapping `loadup` from Lisp
//! sources.  That makes the image exactly the kind of thing this crate owns —
//! environment a test *mounts*, not assertion logic — and provisioning it
//! here is what lets `cargo nextest run` be sufficient on a fresh checkout,
//! instead of a bespoke build-and-test command whose absence surfaced as
//! "daemon never became ready" sixty seconds later.
//!
//! Provisioning is materialize-once, like the package caches: every test
//! process shares the same image, guarded by the same advisory-lock idiom,
//! and the image is regenerated only when it is missing or **older than the
//! editor binary that will load it**.  The staleness rule is the point of the
//! module.  The loader's dump fingerprint is a constant slot in dev builds,
//! so a stale image is not rejected — it is loaded silently, and the suite
//! would assert this week's behavior against last week's code.
//!
//! Names stay with the caller.  `daemon_lifecycle.rs` resolves them from the
//! editor's own loader API (`RuntimeImageRole::image_file_name`,
//! `TEMACS_ROLE_BINARY_NAME`,
//! `fingerprinted_runtime_image_path_for_executable`), so this module never
//! restates a layout that the code under test already defines.
//!
//! # Both names, deliberately
//!
//! GNU skips the fingerprinted twin of a bootstrap dump — `lisp/loadup.el`
//! guards the `add-name-to-file` with `;; Don't bother adding another name if
//! we're just building bootstrap-emacs` (GNU :640-645, and only `pdump` links
//! a `.pdmp` twin at :662).  This port does **not** skip it: neomacs
//! `lisp/loadup.el:627-638` links `bootstrap-neomacs-<fingerprint>.pdump` for
//! `pbootstrap` too, because one binary plays all three roles — the daemon
//! runs as `neomacs` (final mode), falls back to the bootstrap role, and
//! finds the image only at the loader's second rung, the fingerprinted twin.
//! Provisioning therefore requires *both* names, and the [GNU :640-645]
//! divergence is load-bearing: aligning `loadup.el` with GNU here would
//! silently break every daemon-lifecycle test by making the daemon
//! source-bootstrap instead (measured: ~4 minutes, then failure).

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use wait_timeout::ChildExt;

/// GNU's `loadup` dump protocol, exactly as `crates/xtask` drives it.
const BOOTSTRAP_ARGS: [&str; 4] = ["--batch", "-l", "loadup", "--temacs=pbootstrap"];

/// The smoke evaluation that proves the dumped image loads and evaluates.
const SMOKE_EVAL: &str = "(unless (= (+ 20 22) 42) (kill-emacs 1))";

/// Deadline for a cold `loadup` bootstrap.
///
/// This is not a budget — a contended machine has taken minutes — it is the
/// point at which a hang becomes a diagnosis instead of an indefinite wait.
const BOOTSTRAP_DEADLINE: Duration = Duration::from_secs(30 * 60);

/// Deadline for loading the finished image and evaluating one form.
const SMOKE_DEADLINE: Duration = Duration::from_secs(120);

/// How many lines of a failed child's output an error message carries.
const OUTPUT_TAIL_LINES: usize = 40;

/// Everything a bootstrap-image provision needs, already resolved.
///
/// The three name fields exist because only the editor knows them; a caller
/// that hardcodes any of them has created a second producer of the layout.
#[derive(Clone, Debug)]
pub struct BootstrapImagePlan {
    /// The editor binary the suite will run (`CARGO_BIN_EXE_neomacs`).  Both
    /// images are written beside it: the loader searches the executable's
    /// directory and `PATH_EXEC`, and for a development build those are the
    /// same place.
    pub editor: PathBuf,
    /// The runtime root the bootstrap runs against — the tree holding
    /// `lisp/loadup.el`.  Use [`crate::workspace_root`] so a nextest archive
    /// run resolves the machine it runs on, not the machine that built it.
    pub runtime_root: PathBuf,
    /// Program name of the raw (temacs) role copy to create beside the
    /// editor; the editor selects its bootstrap mode by program name.
    pub role_binary_name: String,
    /// Canonical image file name the bootstrap writes beside the editor.
    pub canonical_image_name: String,
    /// The file the loader will actually pick beside the editor (the
    /// fingerprinted twin of the canonical name).
    pub loader_image: PathBuf,
}

/// What a provision did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootstrapImageOutcome {
    /// A fresh image was already in place; nothing was executed.
    Reused,
    /// The image was missing or older than the editor, and was rebuilt.
    Rebuilt,
}

/// A provisioned image, with both names it answers to.
#[derive(Clone, Debug)]
pub struct ProvisionedBootstrapImage {
    /// The canonical name the bootstrap writes.
    pub canonical: PathBuf,
    /// The fingerprinted name the loader searches for.
    pub loader: PathBuf,
    /// Whether this call rebuilt it or found it fresh.
    pub outcome: BootstrapImageOutcome,
}

/// Ensure a fresh bootstrap image exists beside `plan.editor`.
///
/// On a fresh image this is three metadata calls.  On a missing or stale one
/// it serializes with every other process provisioning the same image,
/// re-checks, then: copies the editor to its temacs role name, dumps the
/// image from Lisp sources with an isolated HOME/XDG environment, and loads
/// the result back to prove it works before anyone depends on it.
pub fn provision_bootstrap_image(
    plan: &BootstrapImagePlan,
) -> Result<ProvisionedBootstrapImage, String> {
    let bin_dir = plan.editor.parent().ok_or_else(|| {
        format!(
            "editor binary {} has no directory to put the runtime image beside",
            plan.editor.display()
        )
    })?;
    let canonical = bin_dir.join(&plan.canonical_image_name);
    if let Some(image) = reuse_if_fresh(plan, &canonical) {
        return Ok(image);
    }

    // Only regeneration needs a bootable tree; a fresh image is reused even if
    // the checkout has since been rearranged.
    let loadup = plan.runtime_root.join("lisp").join("loadup.el");
    if !loadup.is_file() {
        return Err(format!(
            "runtime root {} does not hold lisp/loadup.el, so a runtime image \
             cannot be bootstrapped; point the plan at the workspace checkout \
             (workspace_root())",
            plan.runtime_root.display()
        ));
    }

    let _lock = lock_beside(&canonical)?;
    if let Some(image) = reuse_if_fresh(plan, &canonical) {
        return Ok(image);
    }
    provision_locked(plan, &canonical, bin_dir)
}

/// The fast path: both names present, non-empty, and not older than the
/// editor.  Both are required — the bootstrap creates them together, so one
/// without the other means someone removed half of a pair.
fn reuse_if_fresh(
    plan: &BootstrapImagePlan,
    canonical: &Path,
) -> Option<ProvisionedBootstrapImage> {
    let fresh = freshness(canonical, &plan.editor) == Freshness::Fresh
        && freshness(&plan.loader_image, &plan.editor) == Freshness::Fresh;
    fresh.then(|| ProvisionedBootstrapImage {
        canonical: canonical.to_path_buf(),
        loader: plan.loader_image.clone(),
        outcome: BootstrapImageOutcome::Reused,
    })
}

fn provision_locked(
    plan: &BootstrapImagePlan,
    canonical: &Path,
    bin_dir: &Path,
) -> Result<ProvisionedBootstrapImage, String> {
    let role_binary = bin_dir.join(&plan.role_binary_name);
    ensure_role_binary(&plan.editor, &role_binary)?;

    let scratch = tempfile::Builder::new()
        .prefix("neomacs-bootstrap-image-")
        .tempdir()
        .map_err(|error| format!("failed to create bootstrap scratch directory: {error}"))?;
    let environment = isolated_environment(scratch.path(), &plan.runtime_root)?;

    let bootstrap_output = scratch.path().join("bootstrap.out");
    let mut bootstrap = Command::new(&role_binary);
    bootstrap
        .args(BOOTSTRAP_ARGS)
        .current_dir(&plan.runtime_root);
    apply_environment(&mut bootstrap, &environment);
    run_bounded(
        &mut bootstrap,
        &bootstrap_output,
        BOOTSTRAP_DEADLINE,
        &format!(
            "bootstrapping the runtime image via {} did not complete",
            role_binary.display()
        ),
    )
    .map_err(|error| {
        format!(
            "{error}\nIf the checkout has never been built, the generated Lisp \
             inputs the dump loads from source (lisp/international/cp51932.el, \
             eucjp-ms.el, …) exist yet only after a build has produced them."
        )
    })?;

    for (path, what) in [
        (canonical, "canonical runtime image"),
        (plan.loader_image.as_path(), "loader runtime image"),
    ] {
        if !is_nonempty_file(path) {
            return Err(format!(
                "the bootstrap reported success but {what} {} was not written",
                path.display()
            ));
        }
    }

    let smoke_output = scratch.path().join("smoke.out");
    let mut smoke = Command::new(&plan.editor);
    smoke
        .args(["--batch", "-Q", "--dump-file"])
        .arg(canonical)
        .args(["--eval", SMOKE_EVAL])
        .current_dir(&plan.runtime_root);
    apply_environment(&mut smoke, &environment);
    if let Err(error) = run_bounded(
        &mut smoke,
        &smoke_output,
        SMOKE_DEADLINE,
        &format!(
            "the freshly dumped runtime image {} did not pass its smoke evaluation",
            canonical.display()
        ),
    ) {
        // A present-but-broken image would satisfy the freshness rule forever;
        // remove it so the next run starts over instead of reusing it.
        let _ = fs::remove_file(canonical);
        let _ = fs::remove_file(&plan.loader_image);
        return Err(format!(
            "{error}\nThe image was removed so the next run regenerates it."
        ));
    }

    Ok(ProvisionedBootstrapImage {
        canonical: canonical.to_path_buf(),
        loader: plan.loader_image.clone(),
        outcome: BootstrapImageOutcome::Rebuilt,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Freshness {
    Fresh,
    Missing,
    Stale,
}

/// Whether `image` exists, is non-empty, and is not older than `editor`.
///
/// Unreadable clocks answer `Stale`: regenerating an image that was already
/// current costs one bootstrap, while reusing a stale one silently tests the
/// wrong code.
fn freshness(image: &Path, editor: &Path) -> Freshness {
    let Ok(metadata) = fs::metadata(image) else {
        return Freshness::Missing;
    };
    if metadata.len() == 0 {
        return Freshness::Missing;
    }
    let times = (
        metadata.modified(),
        fs::metadata(editor).and_then(|meta| meta.modified()),
    );
    match times {
        (Ok(image_time), Ok(editor_time)) if image_time >= editor_time => Freshness::Fresh,
        _ => Freshness::Stale,
    }
}

fn is_nonempty_file(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
}

/// The raw-role copy of the editor, refreshed to match it.
///
/// The bootstrap runs as this copy because the editor selects its runtime
/// role by program name (a byte-identical copy named `neomacs-temacs` dumps
/// from source; the same copy named `neomacs` would demand the final image
/// this module is trying to avoid needing).
fn ensure_role_binary(editor: &Path, role_binary: &Path) -> Result<(), String> {
    if freshness(role_binary, editor) == Freshness::Fresh {
        return Ok(());
    }
    match fs::remove_file(role_binary) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "failed to remove the stale role copy {}: {error}",
                role_binary.display()
            ));
        }
    }
    fs::copy(editor, role_binary).map_err(|error| {
        format!(
            "failed to copy {} to its role name {}: {error}",
            editor.display(),
            role_binary.display()
        )
    })?;
    let permissions = fs::metadata(editor)
        .map_err(|error| format!("failed to read {} metadata: {error}", editor.display()))?
        .permissions();
    fs::set_permissions(role_binary, permissions).map_err(|error| {
        format!(
            "failed to make the role copy {} executable: {error}",
            role_binary.display()
        )
    })?;
    Ok(())
}

/// A writable, disposable HOME/XDG skeleton for the bootstrap.
///
/// The bootstrap must not read a developer's init files or write into their
/// real cache; every location the editor derives from the environment points
/// into the scratch directory, and the runtime root is named explicitly so
/// the dump is built against the checkout the suite runs from.
fn isolated_environment(
    scratch: &Path,
    runtime_root: &Path,
) -> Result<Vec<(OsString, OsString)>, String> {
    let mut environment = Vec::new();
    for key in [
        "HOME",
        "XDG_RUNTIME_DIR",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
        "TMPDIR",
    ] {
        let directory = scratch.join(key.to_ascii_lowercase());
        fs::create_dir_all(&directory)
            .map_err(|error| format!("failed to create {}: {error}", directory.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
                .map_err(|error| format!("failed to secure {}: {error}", directory.display()))?;
        }
        environment.push((OsString::from(key), directory.into_os_string()));
    }
    environment.push((
        OsString::from("NEOMACS_RUNTIME_ROOT"),
        runtime_root.as_os_str().to_owned(),
    ));
    environment.push((
        OsString::from("NEOMACS_LOG_FILE"),
        scratch.join("bootstrap.log").into_os_string(),
    ));
    Ok(environment)
}

/// Apply the isolated environment and keep display/server discovery out of
/// the bootstrap: an image must not depend on whichever X server or daemon
/// endpoint happens to exist on the machine that dumped it.
fn apply_environment(command: &mut Command, environment: &[(OsString, OsString)]) {
    command.envs(environment.iter().map(|(key, value)| (key, value)));
    for key in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "EMACS_SERVER_FILE",
        "EMACS_SOCKET_NAME",
    ] {
        command.env_remove(key);
    }
}

/// Run a child to completion under a deadline, capturing its output to a file.
///
/// On failure the error carries the tail of that file, because the editor's
/// diagnostics for a broken tree are in its output, not in the exit status.
fn run_bounded(
    command: &mut Command,
    output_path: &Path,
    deadline: Duration,
    what: &str,
) -> Result<(), String> {
    let output = fs::File::create(output_path)
        .map_err(|error| format!("{what}: cannot create {}: {error}", output_path.display()))?;
    let errors = output
        .try_clone()
        .map_err(|error| format!("{what}: cannot duplicate output handle: {error}"))?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::from(output))
        .stderr(Stdio::from(errors))
        .spawn()
        .map_err(|error| format!("{what}: failed to launch: {error}"))?;
    match child.wait_timeout(deadline) {
        Ok(Some(status)) if status.success() => Ok(()),
        Ok(Some(status)) => Err(format!(
            "{what}: exited {status}{}",
            output_tail(output_path)
        )),
        Ok(None) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(format!(
                "{what}: still running after {}s and was killed{}",
                deadline.as_secs(),
                output_tail(output_path)
            ))
        }
        Err(error) => Err(format!("{what}: waiting failed: {error}")),
    }
}

fn output_tail(path: &Path) -> String {
    let Ok(bytes) = fs::read(path) else {
        return String::new();
    };
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(OUTPUT_TAIL_LINES);
    if lines.is_empty() {
        return String::new();
    }
    format!("\n--- output tail ---\n{}", lines[start..].join("\n"))
}

/// An exclusive advisory lock beside the image under construction.
///
/// The lock file is separate from the image because the bootstrap deletes
/// and rewrites the image itself; the lock must outlive that.
fn lock_beside(image: &Path) -> Result<fs::File, String> {
    let mut lock_name = image
        .file_name()
        .map(OsString::from)
        .unwrap_or_else(|| OsString::from("bootstrap-image"));
    lock_name.push(".lock");
    let lock_path = image.with_file_name(lock_name);
    let lock = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|error| {
            format!(
                "failed to open the runtime-image lock {}: {error}",
                lock_path.display()
            )
        })?;
    fs4::FileExt::lock(&lock).map_err(|error| {
        format!(
            "failed to lock runtime-image provisioning at {}: {error}",
            lock_path.display()
        )
    })?;
    Ok(lock)
}

/// Whether `image` is missing, empty, or older than `editor`.
///
/// The freshness rule this module provisions by, exposed for callers that
/// must *diagnose* rather than regenerate: a runtime image with a role this
/// module does not own (the final image, whose producer is a full build) can
/// still be stale, and a stale image is loaded silently rather than rejected.
pub fn image_is_older_than(image: &Path, editor: &Path) -> bool {
    freshness(image, editor) != Freshness::Fresh
}

#[cfg(test)]
#[path = "runtime_image/tests/mod.rs"]
mod tests;
