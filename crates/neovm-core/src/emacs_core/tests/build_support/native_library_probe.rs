//! A host package must never silently become a cross-target capability.

#[path = "../../../../build_support/native_library_probe.rs"]
mod native_library_probe;

use native_library_probe::{NativeLibraryProbe, native_library_probe};

#[test]
fn native_build_may_probe_the_current_hosts_package_database() {
    assert_eq!(
        native_library_probe("x86_64-unknown-linux-gnu", "x86_64-unknown-linux-gnu"),
        NativeLibraryProbe::CurrentHost,
    );
}

#[test]
fn android_cross_build_cannot_inherit_linux_host_libraries() {
    assert_eq!(
        native_library_probe("x86_64-unknown-linux-gnu", "aarch64-linux-android"),
        NativeLibraryProbe::DisabledForCrossCompilation,
    );
}

#[test]
fn cross_architecture_build_cannot_inherit_same_os_host_libraries() {
    assert_eq!(
        native_library_probe("x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"),
        NativeLibraryProbe::DisabledForCrossCompilation,
    );
}
