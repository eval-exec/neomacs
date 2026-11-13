//! Single source of truth for the host capability cfgs shared by every crate
//! whose source branches on them.
//!
//! `neovm-core`, `neovm-system`, `neovm-value` and `neovm-eval` include this
//! file from their build scripts with
//!
//! ```ignore
//! #[path = "../../build_support/host_cfg.rs"]
//! mod host_cfg;
//! ```
//!
//! and call `host_cfg::configure_all()` once from `main`. Every one of them
//! therefore emits the same three `rustc-check-cfg` names and reaches the
//! same detection, so their compile-time views of one host agree. The
//! cross-crate HOST_CFG agreement guard compares those views at test time.
//!
//! The detection inputs are exactly the ones the split build scripts used:
//! `pkg-config` (through the `pkg-config` crate and `PKG_CONFIG*`),
//! `NEOMACS_DISABLE_DBUS`, `LCMS2_NO_PKG_CONFIG`, the `dbus-vendored` and
//! `webview` marker features, and Cargo's target cfg variables. The probes
//! are host-only: they never touch the network and never write to the tree.
//!
//! Two rustc-env values ride along from every including crate, because only
//! the crate that reads each one consumes it: the lcms2 candidate libraries
//! (`NEOMACS_LCMS2_LIBRARY_CANDIDATES`, read by neovm-eval's lcms module) and
//! the probed dbus version (`NEOMACS_DBUS_COMPILED_VERSION`, read by
//! neovm-system's dbusbind).

use std::path::PathBuf;

/// Emit every host-cfg check declaration and every detected cfg.
pub fn configure_all() {
    // The shared source lives outside the including crate, so declare it as
    // a build-script input explicitly; otherwise editing a probe would not
    // rerun the scripts of crates whose own sources did not change.
    if let Some(manifest_dir) = std::env::var_os("CARGO_MANIFEST_DIR") {
        let shared = PathBuf::from(manifest_dir).join("../../build_support/host_cfg.rs");
        println!("cargo:rerun-if-changed={}", shared.display());
    }
    println!("cargo:rustc-check-cfg=cfg(neomacs_have_dbus)");
    println!("cargo:rustc-check-cfg=cfg(neomacs_have_lcms2)");
    println!("cargo:rustc-check-cfg=cfg(neomacs_have_wkwebview)");
    detect_dbus();
    detect_lcms2();
    detect_wkwebview();
}

fn target_os() -> Option<String> {
    std::env::var("CARGO_CFG_TARGET_OS").ok()
}

/// Whether this build has a libdbus transport for `dbusbind`.
///
/// GNU's `configure.ac:3921-3942` sets `HAVE_DBUS` when `dbus-1 >= 1.0`
/// links. The `dbus` crate is a Unix dependency, but not a macOS one:
/// `Cargo.toml` keeps the rule and says why. Windows stays the
/// `--without-dbus` configuration until a socket watch arm exists.
/// `c_features` reads this cfg so `(featurep 'dbusbind)` cannot go true
/// without the library.
fn detect_dbus() {
    println!("cargo:rerun-if-env-changed=PKG_CONFIG");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");
    println!("cargo:rerun-if-env-changed=NEOMACS_DISABLE_DBUS");

    // Target, not host: Cargo sets `CARGO_CFG_UNIX` for the crate being built.
    if std::env::var_os("CARGO_CFG_UNIX").is_none() {
        return;
    }
    // macOS is excluded to match the dependency rule in `Cargo.toml`: the
    // `dbus` crate is not in that target's graph, so this cfg would compile
    // `dbusbind` against a crate that is not there. GNU reaches the same
    // place on a stock macOS -- no `dbus-1.pc`, so `HAVE_DBUS=no`.
    if target_os().as_deref() == Some("macos") {
        return;
    }
    if std::env::var_os("NEOMACS_DISABLE_DBUS").is_some() {
        return;
    }

    let vendored = std::env::var_os("CARGO_FEATURE_DBUS_VENDORED").is_some();
    let probed = pkg_config::Config::new()
        .atleast_version("1.0")
        .cargo_metadata(false)
        .probe("dbus-1");

    if vendored || probed.is_ok() {
        println!("cargo:rustc-cfg=neomacs_have_dbus");
    }
    if let Ok(library) = probed {
        if !library.version.is_empty() {
            println!(
                "cargo:rustc-env=NEOMACS_DBUS_COMPILED_VERSION={}",
                library.version
            );
        }
    }
}

fn detect_lcms2() {
    println!("cargo:rerun-if-env-changed=PKG_CONFIG");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");
    println!("cargo:rerun-if-env-changed=LCMS2_NO_PKG_CONFIG");

    if std::env::var_os("LCMS2_NO_PKG_CONFIG").is_some() {
        return;
    }

    let Ok(library) = pkg_config::Config::new()
        .cargo_metadata(false)
        .probe("lcms2")
    else {
        return;
    };

    println!("cargo:rustc-cfg=neomacs_have_lcms2");
    let candidates = lcms2_library_candidates(&library.link_paths);
    if !candidates.is_empty() {
        println!("cargo:rustc-env=NEOMACS_LCMS2_LIBRARY_CANDIDATES={candidates}");
    }
}

/// Names the lcms2 shared library can have beside a probed link path.
///
/// The target, not the build script's host, decides the names: a build script
/// runs on the host even when Cargo builds for a different target.
fn lcms2_library_candidates(paths: &[PathBuf]) -> String {
    let names: &[&str] = match target_os().as_deref() {
        Some("windows") => &["liblcms2-2.dll", "lcms2.dll"],
        Some("macos") => &["liblcms2.2.dylib", "liblcms2.dylib"],
        Some("linux") => &["liblcms2.so.2", "liblcms2.so"],
        _ if std::env::var_os("CARGO_CFG_UNIX").is_some() => &["liblcms2.so.2", "liblcms2.so"],
        _ => &["lcms2"],
    };

    let mut candidates = Vec::new();
    for path in paths {
        for name in names {
            let candidate = path.join(name);
            if candidate.exists() {
                candidates.push(candidate.display().to_string());
            }
        }
    }
    candidates.join(":")
}

/// Whether this build has a native inline web view.
///
/// The backend is `crates/neomacs-webview/src/platform/macos`, compiled only
/// under `neomacs-webview`'s `webview` feature, which `neomacs` forwards to
/// this crate's `webview` feature. There is no library to look for --
/// `WebKit.framework` ships with macOS -- so the probe is "the feature is on
/// and the target is macOS". Either half alone is wrong: the feature on
/// Linux selects the WPE path, whose `xwidget-internal` is still NotBuilt,
/// and macOS without the feature has no backend behind the symbol.
fn detect_wkwebview() {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_WEBVIEW");
    if target_os().as_deref() == Some("macos")
        && std::env::var_os("CARGO_FEATURE_WEBVIEW").is_some()
    {
        println!("cargo:rustc-cfg=neomacs_have_wkwebview");
    }
}
