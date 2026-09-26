//! Shared test environment infrastructure for the neomacs test suites.
//!
//! The crate owns everything a test *mounts* that is not the code under
//! test:
//!
//! * [`config_env`] — GNU-bootstrapped, sealed, read-only editor
//!   configuration fixtures (Doom today, Spacemacs next) that suites
//!   mount per session;
//! * [`display`] — the deterministic, isolated display sessions
//!   (loopback-TCP Xvfb, weston-headless, sway) those suites run their
//!   scenarios on;
//! * [`workspace_root`] — the runtime-resolved workspace identity that
//!   archive-shipped binaries must use.
//!
//! The one test for whether something belongs here: is it environment a
//! test mounts, or assertion logic?  Assertion logic stays in the suite.

pub mod config_env;
pub mod display;
pub mod inventory;
pub mod packages;
pub mod tools;

pub use config_env::{
    ConfigEnvironment, DoomEnvironment, DoomSource, SpacemacsEnvironment, SpacemacsSource,
};

use std::path::PathBuf;

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

/// The invoking crate's root directory, resolved on the machine *running*
/// the code.
///
/// `env!("CARGO_MANIFEST_DIR")` is the build machine's absolute path, and an
/// archive-shipped test binary runs where that path does not exist (nextest
/// `--workspace-remap`).  The compile-time manifest directory is folded into
/// its workspace-relative path and joined onto [`workspace_root`], so a test
/// in one crate can read another crate's fixtures without depending on the
/// running process's own `CARGO_MANIFEST_DIR`.
#[macro_export]
macro_rules! crate_root {
    () => {{
        let compiled = ::std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        match compiled.strip_prefix($crate::cargo_workspace_root()) {
            Ok(relative) => $crate::workspace_root().join(relative),
            Err(_) => compiled.to_path_buf(),
        }
    }};
}
