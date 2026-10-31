//! Per-target selection of compile-fail fixtures.
//!
//! rustc's diagnostics for one fixture differ between targets: on
//! aarch64-apple-darwin the Send/Sync failures carry objc2's main-thread-only
//! help notes and list more "required by" rows than on Linux. A fixture whose
//! expected text differs gets an *overlay*: `<overlay dir>/<name>.rs` holding
//! only `include!("../<name>.rs");` next to its own `.stderr`. The fixture body
//! stays single-sourced; only the expected diagnostic is per target.
//!
//! Overlays are compiled only on their own target (the macOS compile-contracts
//! CI job); the Linux job never sees them, so a Linux bless cannot rewrite a
//! macOS golden and vice versa.
//!
//! This file is `#[path]`-included by every `*_contracts` runner.

use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixtureTarget {
    Common,
    Arm64MacOs,
}

impl FixtureTarget {
    pub fn current() -> Self {
        if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
            Self::Arm64MacOs
        } else {
            Self::Common
        }
    }

    /// Every `*.rs` fixture directly in `dir` (sorted, subdirectories excluded),
    /// each replaced by its overlay under `overlay_dir` when this target has
    /// one. Panics when the directory is unreadable or empty: an empty runner
    /// would pass vacuously.
    pub fn fixtures(self, dir: &str, overlay_dir: &str) -> Vec<PathBuf> {
        let mut common: Vec<PathBuf> = std::fs::read_dir(dir)
            .unwrap_or_else(|error| panic!("compile-contract fixture directory {dir}: {error}"))
            .map(|entry| entry.expect("compile-contract fixture entry").path())
            .filter(|path| {
                path.is_file() && path.extension().is_some_and(|extension| extension == "rs")
            })
            .collect();
        common.sort();
        assert!(!common.is_empty(), "no compile-contract fixtures in {dir}");
        common
            .into_iter()
            .map(|path| self.overlay_or_common(overlay_dir, path))
            .collect()
    }

    /// `common` itself, or this target's overlay of it when one exists.
    fn overlay_or_common(self, overlay_dir: &str, common: PathBuf) -> PathBuf {
        match self {
            Self::Common => common,
            Self::Arm64MacOs => {
                let overlay =
                    Path::new(overlay_dir).join(common.file_name().expect("fixture filename"));
                if overlay.is_file() { overlay } else { common }
            }
        }
    }
}
