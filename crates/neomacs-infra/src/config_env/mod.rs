//! The contract every configuration fixture offers to a test session.
//!
//! Concrete environments ([`crate::config_env::DoomEnvironment`],
//! [`crate::config_env::SpacemacsEnvironment`]) own their layout policy;
//! this trait is only the *session contract* — the surface generic
//! harness code needs to mount any of them.  Probe files (which document
//! a consumer opens, what to wait for) stay consumer knowledge.

pub mod common;
pub mod doom;
pub mod inventory;
pub mod spacemacs;

pub use doom::{DoomEnvironment, DoomSource};
pub use inventory::{Drift, Inventory};
pub use spacemacs::{SpacemacsEnvironment, SpacemacsSource};

use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub trait ConfigEnvironment {
    /// The fixture's short name, as addressed by the CLI
    /// (`infra materialize <name>`) and the cache directory stem.
    fn name(&self) -> &'static str;

    /// The sealed distribution checkout.
    fn tree(&self) -> PathBuf;

    /// The sealed bootstrap HOME shared by every session.
    fn home(&self) -> PathBuf;

    /// Environment for one editor session mounting this fixture.
    /// `session_state` is a caller-owned per-session directory prepared
    /// by [`Self::prepare_session_state`]; every writable location the
    /// distribution uses lands inside it.
    fn session_env(&self, session_state: &Path) -> Vec<(OsString, OsString)>;

    /// Command arguments mounting this fixture's tree (empty when the
    /// environment mounts through HOME alone).
    fn session_args(&self) -> Vec<String>;

    /// Seed one session's state directory: copy the small generated
    /// state, share the multi-hundred-MB package builds read-only by
    /// symlink, so a write attempt hits the seal and fails loudly.
    fn prepare_session_state(&self, session_state: &Path) -> Result<(), String>;

    /// Deep verification: re-walk the sealed fixture, re-hash every file,
    /// and compare against the sealed inventory.  Ok(drift) with a clean
    /// drift proves the fixture is byte-identical to the day it was
    /// sealed; Err means the fixture state itself is unreadable.
    fn verify_deep(&self) -> Result<crate::config_env::inventory::Drift, String>;
}

/// XDG directories pinned inside the session state, so GTK3, fontconfig,
/// and GLib never read the operator's real `~/.config` — both a pollution
/// guard and a reproducibility requirement: ambient XDG content differs
/// between machines and silently diverges paired GUI comparisons.
pub fn session_xdg_env(session_state: &Path) -> Vec<(OsString, OsString)> {
    [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_DATA_HOME", "share"),
    ]
    .into_iter()
    .map(|(key, dir)| (key.into(), session_state.join(dir).into_os_string()))
    .chain([
        // GTK's accessibility bridge spawns a session bus under the
        // redirected XDG and blocks the whole GUI startup in do_wait --
        // an editor comparison never exercises it, so turn it off at the
        // bridge instead of shipping a private D-Bus.
        ("NO_AT_BRIDGE".into(), "1".into()),
        ("GTK_MODULES".into(), "".into()),
    ])
    .collect()
}

/// Every environment the CLI and generic harness know about.
pub const NAMES: &[&str] = &["doom", "spacemacs"];

/// Open the materialized fixture by CLI name, if it exists.
pub fn open_by_name(name: &str) -> Option<Box<dyn ConfigEnvironment>> {
    match name {
        "doom" => doom::DoomEnvironment::open().map(|env| Box::new(env) as _),
        "spacemacs" => spacemacs::SpacemacsEnvironment::open().map(|env| Box::new(env) as _),
        _ => None,
    }
}
