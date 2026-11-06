//! One host-cfg classification for the canonical registration plan, the H3b
//! bootstrap snapshot and the T6 order gate.
//!
//! A registration row's `cfg` label names the build condition under which the
//! name is registered. The plan walker, the T6 trace comparison and the H3b
//! binder test must all decide activity the same way, so they all call
//! [`HostCfgClass::is_active`]. The underlying cfgs come from the workspace's
//! single build-script probe (`build_support/host_cfg.rs`), so this predicate
//! and the four probing crates' compile-time views agree.

/// Every host condition a registration row can be gated on.
///
/// The enum is exhaustive on purpose: a new label must be added here (and to
/// [`HostCfgClass::is_active`]) before a plan row may use it, so a row can
/// never silently fall into a default activity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HostCfgClass {
    Always,
    Macos,
    Windows,
    Linux,
    Dbus,
    Lcms2,
    Sqlite,
    VmProfile,
}

impl HostCfgClass {
    /// Every class, in the label vocabulary's declaration order.
    pub const ALL: [Self; 8] = [
        Self::Always,
        Self::Macos,
        Self::Windows,
        Self::Linux,
        Self::Dbus,
        Self::Lcms2,
        Self::Sqlite,
        Self::VmProfile,
    ];

    /// The plan/snapshot label for this class.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::Macos => "macos",
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::Dbus => "dbus",
            Self::Lcms2 => "lcms2",
            Self::Sqlite => "sqlite",
            Self::VmProfile => "vm-profile",
        }
    }

    /// Parse a plan/snapshot label. Unknown labels are an error at the call
    /// site, never an implicit inactive row.
    pub fn from_label(label: &str) -> Option<Self> {
        Some(match label {
            "always" => Self::Always,
            "macos" => Self::Macos,
            "windows" => Self::Windows,
            "linux" => Self::Linux,
            "dbus" => Self::Dbus,
            "lcms2" => Self::Lcms2,
            "sqlite" => Self::Sqlite,
            "vm-profile" => Self::VmProfile,
            _ => return None,
        })
    }

    /// Whether this build registers rows carrying this class.
    ///
    /// `dbus` and `lcms2` are decided by the shared build-script probe in
    /// every probing crate; the rest are plain target/feature facts.
    pub const fn is_active(self) -> bool {
        match self {
            Self::Always => true,
            Self::Macos => cfg!(target_os = "macos"),
            Self::Windows => cfg!(windows),
            Self::Linux => cfg!(target_os = "linux"),
            Self::Dbus => cfg!(neomacs_have_dbus),
            Self::Lcms2 => cfg!(neomacs_have_lcms2),
            Self::Sqlite => cfg!(feature = "sqlite"),
            Self::VmProfile => cfg!(feature = "vm-profile"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::HostCfgClass;

    #[test]
    fn every_class_round_trips_through_its_label() {
        for class in HostCfgClass::ALL {
            assert_eq!(HostCfgClass::from_label(class.label()), Some(class));
        }
        assert_eq!(HostCfgClass::from_label("lcms"), None);
        assert!(HostCfgClass::Always.is_active());
    }
}
