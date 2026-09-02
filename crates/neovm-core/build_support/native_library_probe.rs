//! Policy for build-time discovery of native target libraries.
//!
//! A build script runs on `HOST`, while its output is linked into `TARGET`.
//! Host `pkg-config` results are therefore valid only for a native build.  A
//! cross target must acquire optional libraries through an explicit target
//! package instead of inheriting whatever happens to be installed beside
//! Cargo.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeLibraryProbe {
    /// `HOST == TARGET`; querying the current process's pkg-config database is
    /// allowed.
    CurrentHost,
    /// `HOST != TARGET`, but the target owns an explicit pkg-config wrapper or
    /// sysroot. Querying that target database is allowed.
    ExplicitCrossTarget,
    /// `HOST != TARGET`; an unqualified host probe cannot describe the target.
    DisabledForCrossCompilation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CrossTargetPkgConfig {
    Unconfigured,
    Explicit,
}

pub fn target_pkg_config_env_names(target: &str) -> Vec<String> {
    let underscored = target.replace('-', "_");
    ["PKG_CONFIG", "PKG_CONFIG_SYSROOT_DIR"]
        .into_iter()
        .flat_map(|base| {
            [
                format!("{base}_{target}"),
                format!("{base}_{underscored}"),
                format!("TARGET_{base}"),
            ]
        })
        .collect()
}

pub fn cross_target_pkg_config(
    target: &str,
    mut is_set: impl FnMut(&str) -> bool,
) -> CrossTargetPkgConfig {
    if target_pkg_config_env_names(target)
        .iter()
        .any(|name| is_set(name))
    {
        CrossTargetPkgConfig::Explicit
    } else {
        CrossTargetPkgConfig::Unconfigured
    }
}

pub fn native_library_probe(
    host: &str,
    target: &str,
    cross_target: CrossTargetPkgConfig,
) -> NativeLibraryProbe {
    if host == target {
        NativeLibraryProbe::CurrentHost
    } else {
        match cross_target {
            CrossTargetPkgConfig::Explicit => NativeLibraryProbe::ExplicitCrossTarget,
            CrossTargetPkgConfig::Unconfigured => NativeLibraryProbe::DisabledForCrossCompilation,
        }
    }
}
