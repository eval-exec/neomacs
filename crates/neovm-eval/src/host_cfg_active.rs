//! Host-activity predicate for the shared host-cfg vocabulary.
//!
//! [`HostCfgClass`] is defined in `host_cfg.rs` because the source-only
//! generators in `build_support` include that file verbatim, and `cargo
//! xtask` compiles them without the workspace build-script probe or any
//! features. The vocabulary must therefore stay free of `cfg!` probes. The
//! predicate belongs here, where every probing crate's build script has
//! emitted the same cfgs (see `build_support/host_cfg.rs`): the plan walker,
//! the T6 trace comparison and the H3b binder test all call it.

use crate::host_cfg::HostCfgClass;

impl HostCfgClass {
    /// Whether this build registers rows carrying this class.
    ///
    /// `dbus` and `lcms2` are decided by the shared build-script probe in
    /// every probing crate; the rest are plain target/feature facts.
    pub const fn is_active(self) -> bool {
        match self {
            HostCfgClass::Always => true,
            HostCfgClass::Macos => cfg!(target_os = "macos"),
            HostCfgClass::Windows => cfg!(windows),
            HostCfgClass::Linux => cfg!(target_os = "linux"),
            HostCfgClass::Dbus => cfg!(neomacs_have_dbus),
            HostCfgClass::Lcms2 => cfg!(neomacs_have_lcms2),
            HostCfgClass::Sqlite => cfg!(feature = "sqlite"),
            HostCfgClass::VmProfile => cfg!(feature = "vm-profile"),
            HostCfgClass::NotMacos => !cfg!(target_os = "macos"),
            HostCfgClass::X11Only => !cfg!(windows) && !cfg!(target_os = "macos"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::host_cfg::HostCfgClass;

    #[test]
    fn always_is_active() {
        assert!(HostCfgClass::Always.is_active());
    }
}
