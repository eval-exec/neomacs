//! Single-variable opt configuration, defaulting to qualified primitive-loop SSA.
//!
//! Threading: the process publishes one immutable scalar enum through OnceLock.
//! Presets only supply compile-time defaults, with no Lisp handles, feedback,
//! observation, native-entry checks or runtime state.

use super::knobs::{OptAdmit, OptEarlyMode, OptMode, OptPasses, OptProfitMode};

/// One opt configuration preset. Threading: immutable process configuration;
/// tests override only this scalar for compilers on their own test thread.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Profile {
    #[default]
    Off,
    Lists20,
    Lists32,
    Lists48,
    Lists48Osr,
    Lists64,
}

/// Defaults for absent individual knobs. Threading: compiler-owned scalar copy,
/// never published into a leaf, source, cache or worker payload.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Defaults {
    pub(super) mode: OptMode,
    pub(super) admit: OptAdmit,
    pub(super) passes: OptPasses,
    pub(super) profit: OptProfitMode,
    pub(super) early: OptEarlyMode,
    pub(super) max_ops: usize,
    pub(super) fast: bool,
    pub(super) require_osr: bool,
}

impl Profile {
    /// Resolve process configuration without mutating the environment.
    /// Threading: borrowed scalar input; the caller publishes the result once.
    #[doc(hidden)]
    pub fn from_env(value: Result<&str, &std::env::VarError>) -> Self {
        resolve(value, || Self::Lists48Osr, Self::parse)
    }

    #[doc(hidden)]
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some("lists20") => Self::Lists20,
            Some("lists32") => Self::Lists32,
            Some("lists48") => Self::Lists48,
            Some("lists48-osr") => Self::Lists48Osr,
            Some("lists64") => Self::Lists64,
            _ => Self::Off,
        }
    }

    #[doc(hidden)]
    pub fn defaults(self) -> Defaults {
        let max_ops = match self {
            Self::Off => return Defaults::default(),
            Self::Lists20 => 20,
            Self::Lists32 => 32,
            Self::Lists48 | Self::Lists48Osr => 48,
            Self::Lists64 => 64,
        };
        Defaults {
            mode: OptMode::Opt,
            admit: OptAdmit::ALL,
            passes: OptPasses {
                fold: true,
                bool_rep: true,
                reps: true,
                reads: true,
                ..OptPasses::default()
            },
            profit: if self == Self::Lists48Osr {
                OptProfitMode::PrimitiveLoops
            } else {
                OptProfitMode::Lists
            },
            early: OptEarlyMode::Hot,
            max_ops,
            fast: true,
            require_osr: self == Self::Lists48Osr,
        }
    }
}

/// Preserve the old parser for every explicitly present value, including an
/// empty/invalid string or non-Unicode environment entry. Only absence accepts
/// the preset default. Threading: borrowed compiler input, no environment writes.
pub(super) fn resolve<T>(
    explicit: Result<&str, &std::env::VarError>,
    absent: impl FnOnce() -> T,
    parse: impl FnOnce(Option<&str>) -> T,
) -> T {
    match explicit {
        Ok(value) => parse(Some(value)),
        Err(std::env::VarError::NotPresent) => absent(),
        Err(std::env::VarError::NotUnicode(_)) => parse(None),
    }
}

#[cold]
#[inline(never)]
pub(super) fn selected() -> Defaults {
    static PROFILE: std::sync::OnceLock<Profile> = std::sync::OnceLock::new();
    PROFILE
        .get_or_init(|| Profile::from_env(std::env::var("NEOVM_JIT_OPT_PROFILE").as_deref()))
        .defaults()
}

#[cfg(any(test, feature = "test-support"))]
std::thread_local! {
    /// Test-thread scalar compiler configuration only; never Lisp/mutator state.
    static TEST_PROFILE: std::cell::Cell<Option<Profile>> = const { std::cell::Cell::new(None) };
}

/// Effective preset defaults for test compilers, below existing individual test
/// overrides and above process configuration. No process environment is mutated.
#[cfg(any(test, feature = "test-support"))]
pub(super) fn test_defaults() -> Option<Defaults> {
    TEST_PROFILE.with(|value| value.get().map(Profile::defaults))
}

/// Test-owned scalar selection. Threading: restores the exact previous override
/// on this compiler thread; it neither reads nor retains any Lisp state.
#[cfg(any(test, feature = "test-support"))]
#[must_use = "the profile override ends when the returned guard is dropped"]
pub(super) fn scope_for_test(profile: Profile) -> impl Drop {
    /// Threading: scalar override owned and restored on this test's compiler thread.
    #[derive(Debug)]
    #[must_use = "dropping the guard restores the previous test profile"]
    struct Scope {
        previous: Option<Profile>,
        _thread: std::marker::PhantomData<*const ()>,
    }
    static_assertions::assert_not_impl_any!(Scope: Send, Sync);
    impl Drop for Scope {
        fn drop(&mut self) {
            TEST_PROFILE.with(|value| value.set(self.previous));
        }
    }
    Scope {
        previous: TEST_PROFILE.with(|value| value.replace(Some(profile))),
        _thread: std::marker::PhantomData,
    }
}

/// The lists48-osr startup stack from the Opt profit tests.
///
/// `Settings::enter` publishes Opt+ALL, then the test clears those individual
/// overrides so this profile supplies them, while inline2, tier-2, deopt and
/// the background mode stay on the surrounding stack. Drop restores that
/// stack and clears the JIT cache, matching `CacheScope`.
///
/// Threading: scalar compiler TLS and this thread's compile cache only.
#[cfg(any(test, feature = "test-support"))]
#[must_use = "the lists48-osr profile ends when the guard is dropped"]
pub(crate) fn enter_lists48_osr_profile_for_test() -> impl Drop {
    /// Threading: restored on this compiler thread; holds no Lisp state.
    struct Guard {
        previous: Option<Profile>,
        _thread: std::marker::PhantomData<*const ()>,
    }
    static_assertions::assert_not_impl_any!(Guard: Send, Sync);
    impl Drop for Guard {
        fn drop(&mut self) {
            crate::emacs_core::jit::cache::clear();
            TEST_PROFILE.with(|value| value.set(self.previous));
            super::force_opt_for_test(None, None);
            super::force_opt_profit_for_test(None);
            super::force_opt_require_osr_for_test(None);
            super::force_opt_early_for_test(None);
            super::force_opt_max_ops_for_test(None);
            super::force_inline2_for_test(None);
            super::force_tier2_for_test(None);
            super::force_deopt_for_test(false);
            crate::emacs_core::jit::bg::force_mode_for_test(None);
        }
    }
    super::force_opt_for_test(Some(super::OptMode::Opt), Some(super::OptAdmit::ALL));
    super::force_opt_profit_for_test(Some(super::OptProfitMode::Loops));
    super::force_opt_require_osr_for_test(Some(false));
    super::force_opt_early_for_test(Some(super::OptEarlyMode::Off));
    super::force_opt_max_ops_for_test(Some(0));
    super::force_inline2_for_test(Some(super::Inline2Mode::Off));
    super::force_tier2_for_test(Some(super::Tier2Knob::from_env(|_| None)));
    super::force_deopt_for_test(false);
    crate::emacs_core::jit::bg::force_mode_for_test(Some(crate::emacs_core::jit::bg::BgMode::Sync));
    let previous = TEST_PROFILE.with(|value| value.replace(Some(Profile::Lists48Osr)));
    super::force_opt_for_test(None, None);
    super::force_opt_profit_for_test(None);
    super::force_opt_require_osr_for_test(None);
    super::force_opt_early_for_test(None);
    super::force_opt_max_ops_for_test(None);
    crate::emacs_core::jit::cache::clear();
    Guard {
        previous,
        _thread: std::marker::PhantomData,
    }
}

#[cfg(test)]
#[path = "opt_profile/tests/precedence_test.rs"]
mod precedence_tests;

#[cfg(test)]
#[path = "opt_profile/tests/frontend_test.rs"]
mod frontend_tests;

#[cfg(test)]
#[path = "opt_profile/tests/ready_osr_profile_test.rs"]
mod ready_osr_tests;

#[cfg(test)]
#[path = "opt_profile/tests/default_test.rs"]
mod default_tests;
