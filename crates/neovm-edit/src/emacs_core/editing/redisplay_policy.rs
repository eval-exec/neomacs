//! GNU redisplay-hook policy selection.
//!
//! The policy is immutable process data: one environment selector published by
//! `OnceLock`, read concurrently by independent Context mutators. It binds no
//! evaluator or display state, so it lives in the edit layer; the evaluator and
//! the display layer read it downward.
//!
//! The test fixture selector below is the only thread-local, is independently
//! owned by each test thread, holds no Lisp values, and exists only under
//! `cfg(any(test, feature = "redisplay-test-policy", feature = "test-support"))`.

use std::ffi::OsStr;
use std::sync::OnceLock;

/// Immutable process policy published by OnceLock for concurrent numeric reads.
/// Independent Context mutators share only this selector, with no Lisp-state cache.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum RedisplayHookPolicy {
    #[default]
    Legacy,
    Gnu,
}

pub(crate) fn parse_redisplay_hooks(value: Option<&OsStr>) -> RedisplayHookPolicy {
    match value.and_then(OsStr::to_str) {
        Some("on" | "1" | "true" | "yes") => RedisplayHookPolicy::Gnu,
        _ => RedisplayHookPolicy::Legacy,
    }
}

#[inline]
#[doc(hidden)]
pub fn gnu_redisplay_hooks_enabled() -> bool {
    #[cfg(any(test, feature = "redisplay-test-policy", feature = "test-support"))]
    if let Some(policy) = TEST_REDISPLAY_HOOK_POLICY.with(std::cell::Cell::get) {
        return policy == RedisplayHookPolicy::Gnu;
    }
    static POLICY: OnceLock<RedisplayHookPolicy> = OnceLock::new();
    *POLICY.get_or_init(|| {
        parse_redisplay_hooks(std::env::var_os("NEOMACS_REDISPLAY_GNU_HOOKS").as_deref())
    }) == RedisplayHookPolicy::Gnu
}

#[cfg(any(test, feature = "redisplay-test-policy", feature = "test-support"))]
thread_local! {
    /// Numeric test policy only, independently owned by each test thread. It
    /// holds no Lisp values, caches or runtime state and does not exist in
    /// builds without the test feature or alter the process-once policy lookup.
    static TEST_REDISPLAY_HOOK_POLICY: std::cell::Cell<Option<RedisplayHookPolicy>> = const {
        std::cell::Cell::new(None)
    };
}

/// Restore a numeric fixture selector on normal return and Rust unwinding.
/// The !Send/!Sync marker keeps this guard on its owning test mutator thread;
/// independent test threads share no mutable selector or Lisp state.
#[cfg(any(test, feature = "redisplay-test-policy", feature = "test-support"))]
pub struct RedisplayHookPolicyGuard {
    previous: Option<RedisplayHookPolicy>,
    _same_thread: std::marker::PhantomData<std::rc::Rc<()>>,
}

#[cfg(any(test, feature = "redisplay-test-policy", feature = "test-support"))]
impl RedisplayHookPolicyGuard {
    pub fn legacy() -> Self {
        Self::set(RedisplayHookPolicy::Legacy)
    }

    pub fn gnu() -> Self {
        Self::set(RedisplayHookPolicy::Gnu)
    }

    fn set(policy: RedisplayHookPolicy) -> Self {
        Self {
            previous: TEST_REDISPLAY_HOOK_POLICY.with(|slot| slot.replace(Some(policy))),
            _same_thread: std::marker::PhantomData,
        }
    }
}

#[cfg(any(test, feature = "redisplay-test-policy", feature = "test-support"))]
impl Drop for RedisplayHookPolicyGuard {
    fn drop(&mut self) {
        TEST_REDISPLAY_HOOK_POLICY.with(|slot| slot.set(self.previous));
    }
}

/// E3-1: the parser's own unit tests live in the owner crate, where the
/// private `RedisplayHookPolicy`/`parse_redisplay_hooks` are natively
/// visible.  (They were previously re-homed in eval, which forced a
/// non-private test-support widening of both items.)
#[cfg(test)]
mod tests {
    use super::{RedisplayHookPolicy, parse_redisplay_hooks};
    use std::ffi::OsStr;

    #[test]
    fn explicit_policy_is_required_and_invalid_values_preserve_baseline() {
        for value in [
            None,
            Some(OsStr::new("")),
            Some(OsStr::new("off")),
            Some(OsStr::new("unknown")),
        ] {
            assert_eq!(parse_redisplay_hooks(value), RedisplayHookPolicy::Legacy);
        }
        for value in ["on", "1", "true", "yes"] {
            assert_eq!(
                parse_redisplay_hooks(Some(OsStr::new(value))),
                RedisplayHookPolicy::Gnu
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn non_unicode_policy_is_baseline() {
        use std::os::unix::ffi::OsStrExt;
        assert_eq!(
            parse_redisplay_hooks(Some(OsStr::from_bytes(&[0xff]))),
            RedisplayHookPolicy::Legacy
        );
    }
}
