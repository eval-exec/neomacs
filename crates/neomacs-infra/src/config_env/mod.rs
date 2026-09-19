//! The contract every configuration fixture offers to a test session.
//!
//! Concrete environments ([`crate::config_env::DoomEnvironment`],
//! [`crate::config_env::SpacemacsEnvironment`]) own their layout policy;
//! this trait is only the *session contract* — the surface generic
//! harness code needs to mount any of them.  Probe files (which document
//! a consumer opens, what to wait for) stay consumer knowledge.

pub mod common;
pub mod doom;
pub mod spacemacs;

pub use doom::{DoomEnvironment, DoomSource};
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
