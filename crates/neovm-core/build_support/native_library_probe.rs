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
    /// `HOST != TARGET`; an unqualified host probe cannot describe the target.
    DisabledForCrossCompilation,
}

pub fn native_library_probe(host: &str, target: &str) -> NativeLibraryProbe {
    if host == target {
        NativeLibraryProbe::CurrentHost
    } else {
        NativeLibraryProbe::DisabledForCrossCompilation
    }
}
