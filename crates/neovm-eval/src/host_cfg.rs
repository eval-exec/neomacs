//! One host-cfg classification for the canonical registration plan, the H3b
//! bootstrap snapshot and the T6 order gate.
//!
//! A registration row's `cfg` label names the build condition under which the
//! name is registered. The plan walker, the T6 trace comparison and the H3b
//! binder test must all decide activity the same way, so they all call
//! [`HostCfgClass::is_active`], defined in this crate's `host_cfg_active`
//! module where the build-script probe's cfgs are compiled in. The underlying
//! cfgs come from the workspace's single build-script probe
//! (`build_support/host_cfg.rs`), so the predicate and the four probing
//! crates' compile-time views agree.
//!
//! This file holds only the label VOCABULARY. The source-only generators in
//! `build_support` include it verbatim and are compiled by `cargo xtask`,
//! which has no build-script probe and no feature set: an `is_active` here
//! would compile there as an unknown-cfg warning that always answers false.
//! The activity predicate therefore lives in `host_cfg_active.rs`, which only
//! eval compiles.

/// Every host condition a registration row can be gated on.
///
/// The enum is exhaustive on purpose: a new label must be added here (and to
/// the activity predicate in `host_cfg_active`) before a plan row may use it,
/// so a row can never silently fall into a default activity.
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
    /// Every window-system surface except the macOS NS one.  GNU's W32 and X11
    /// builds both define these names (e.g. w32fns.c's x-change-window-property),
    /// while the NS object set does not; the NS build leaves them void.
    NotMacos,
    /// The X11 window system only.  GNU's W32 build has no DEFUN for these
    /// names (only the four w32fns.c ones above are w32 DEFUNs) and none of
    /// the Lisp files its windows-nt dump loads binds them, while the NS
    /// object set does not define them either; only an X11 build binds them.
    X11Only,
}

impl HostCfgClass {
    /// Every class, in the label vocabulary's declaration order.
    pub const ALL: [Self; 10] = [
        Self::Always,
        Self::Macos,
        Self::Windows,
        Self::Linux,
        Self::Dbus,
        Self::Lcms2,
        Self::Sqlite,
        Self::VmProfile,
        Self::NotMacos,
        Self::X11Only,
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
            Self::NotMacos => "not-macos",
            Self::X11Only => "x11-only",
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
            "not-macos" => Self::NotMacos,
            "x11-only" => Self::X11Only,
            _ => return None,
        })
    }

    /// Whether the label vocabulary is self-consistent: every class in
    /// [`Self::ALL`] parses back from its own [`Self::label`].
    ///
    /// The source-only generators call this before rendering, so a label that
    /// only one direction knows about fails the build instead of silently
    /// dropping a registration row.
    pub fn vocabulary_round_trips() -> bool {
        Self::ALL
            .iter()
            .all(|class| Self::from_label(class.label()) == Some(*class))
    }
}

#[cfg(test)]
mod tests {
    use super::HostCfgClass;

    #[test]
    fn every_class_round_trips_through_its_label() {
        assert!(HostCfgClass::vocabulary_round_trips());
        assert_eq!(HostCfgClass::from_label("lcms"), None);
    }
}
