//! Numeric retained-face collection policy; no source/proof or callback gate.
//!
//! | Knob | Default | Values | Gate |
//! | --- | --- | --- | --- |
//! | `NEOMACS_RETAINED_FACE_GATHER` | `off` | `off`, `on` | Gather ordinary retained row dependencies directly into the frame sorted set; prepared admission is unchanged. |

/// Pure numeric parser. Absent, empty, invalid and nonUnicode values are OFF;
/// concurrent callers read no Lisp state or mutable runtime ownership.
#[inline]
fn parse(value: Option<&std::ffi::OsStr>) -> bool {
    value
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "on" | "true" | "yes"
            )
        })
}

/// Immutable process selector published after initialization by OnceLock.
/// Independent mutators read one initialized numeric policy. Test-only scoped
/// overrides retain no Lisp state, Context pointer, row or accepted frame.
#[inline]
pub(super) fn enabled() -> bool {
    #[cfg(test)]
    if let Some(value) = super::retained_face_gather_test_support::forced() {
        return value;
    }
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| parse(std::env::var_os("NEOMACS_RETAINED_FACE_GATHER").as_deref()))
}

#[cfg(test)]
#[path = "tests/retained_face_gather_policy_test.rs"]
mod tests;
