//! A host package must never silently become a cross-target capability.

#[path = "../../../../build_support/native_library_probe.rs"]
mod native_library_probe;

use native_library_probe::{
    CrossTargetPkgConfig, NativeLibraryProbe, cross_target_pkg_config, native_library_probe,
};

#[test]
fn native_build_may_probe_the_current_hosts_package_database() {
    assert_eq!(
        native_library_probe(
            "x86_64-unknown-linux-gnu",
            "x86_64-unknown-linux-gnu",
            CrossTargetPkgConfig::Unconfigured,
        ),
        NativeLibraryProbe::CurrentHost,
    );
}

#[test]
fn android_cross_build_cannot_inherit_linux_host_libraries() {
    assert_eq!(
        native_library_probe(
            "x86_64-unknown-linux-gnu",
            "aarch64-linux-android",
            CrossTargetPkgConfig::Unconfigured,
        ),
        NativeLibraryProbe::DisabledForCrossCompilation,
    );
}

#[test]
fn cross_architecture_build_cannot_inherit_same_os_host_libraries() {
    assert_eq!(
        native_library_probe(
            "x86_64-unknown-linux-gnu",
            "aarch64-unknown-linux-gnu",
            CrossTargetPkgConfig::Unconfigured,
        ),
        NativeLibraryProbe::DisabledForCrossCompilation,
    );
}

#[test]
fn explicitly_configured_cross_target_may_probe_its_own_package_database() {
    assert_eq!(
        native_library_probe(
            "x86_64-unknown-linux-gnu",
            "aarch64-linux-android",
            CrossTargetPkgConfig::Explicit,
        ),
        NativeLibraryProbe::ExplicitCrossTarget,
    );
}

#[test]
fn target_qualified_pkg_config_is_explicit_but_global_host_config_is_not() {
    let target = "aarch64-linux-android";
    assert_eq!(
        cross_target_pkg_config(target, |name| name == "PKG_CONFIG_aarch64_linux_android"),
        CrossTargetPkgConfig::Explicit,
    );
    assert_eq!(
        cross_target_pkg_config(target, |name| name == "PKG_CONFIG"
            || name == "PKG_CONFIG_PATH"),
        CrossTargetPkgConfig::Unconfigured,
    );
}
