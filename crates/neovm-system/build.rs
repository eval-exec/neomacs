//! Build-script detection for the host-system crate.
//!
//! The D-Bus transport moved here with `dbusbind` (stage 6), so the
//! `neomacs_have_dbus` cfg must be decided by the crate that compiles the
//! transport.  Keeping the probe in `neovm-core` alone left the moved
//! `dbusbind` code compiled as the `--without-dbus` configuration even when
//! the library is present.

/// Whether this build has a libdbus transport for `dbusbind`.
///
/// GNU's `configure.ac:3921-3942` sets `HAVE_DBUS` when `dbus-1 >= 1.0`
/// links.  The `dbus` crate is a Unix dependency, but not a macOS one:
/// `Cargo.toml` keeps the rule and says why.  Windows stays the
/// `--without-dbus` configuration until a socket watch arm exists.
fn detect_dbus() {
    println!("cargo:rustc-check-cfg=cfg(neomacs_have_dbus)");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");
    println!("cargo:rerun-if-env-changed=NEOMACS_DISABLE_DBUS");

    // Target, not host: Cargo sets `CARGO_CFG_UNIX` for the crate being built.
    if std::env::var_os("CARGO_CFG_UNIX").is_none() {
        return;
    }
    // macOS is excluded to match the dependency rule in `Cargo.toml`: the
    // `dbus` crate is not in that target's graph, so this cfg would compile
    // `dbusbind` against a crate that is not there.  GNU reaches the same
    // place on a stock macOS -- no `dbus-1.pc`, so `HAVE_DBUS=no`.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
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

fn main() {
    detect_dbus();
}
