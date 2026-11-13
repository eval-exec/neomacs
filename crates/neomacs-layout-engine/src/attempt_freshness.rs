//! Complete input tokens passed through one native-only attempt span.
//!
//! Tokens are borrowed only until the next possible Lisp entry. No state is
//! retained across attempts or shared between evaluator mutators. Native
//! publication and temporary buffer selection are checked by core's complete
//! token handoff, including the revisions which can move without local Lisp.
//!
//! | Knob | Default | Values | Gate |
//! | --- | --- | --- | --- |
//! | `NEOMACS_LAYOUT_FRESHNESS_SPANS` | `on` | `off`; `on`/`1`/`true`/`yes` | Pass the complete token through proven native preparation and chrome publication. |

use neovm_core::window::display::WindowDisplayContextExt;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FreshnessSpanMode {
    Capture,
    Reuse,
}

impl FreshnessSpanMode {
    fn parse(value: Option<&std::ffi::OsStr>) -> Self {
        match value {
            None => Self::Reuse,
            Some(value)
                if value.to_str().is_some_and(|value| {
                    matches!(
                        value.trim().to_ascii_lowercase().as_str(),
                        "on" | "1" | "true" | "yes"
                    )
                }) =>
            {
                Self::Reuse
            }
            Some(_) => Self::Capture,
        }
    }
}

pub(crate) fn mode() -> FreshnessSpanMode {
    static MODE: std::sync::OnceLock<FreshnessSpanMode> = std::sync::OnceLock::new();
    *MODE.get_or_init(|| {
        FreshnessSpanMode::parse(std::env::var_os("NEOMACS_LAYOUT_FRESHNESS_SPANS").as_deref())
    })
}

pub(crate) use neovm_core::window::WindowLayoutLispAdmission as LayoutLispAdmission;

/// Signals and nonmarking callbacks are still Lisp boundaries. The display
/// scanner records every evaluated form, including false results; Structural
/// includes failed scans and cannot prove native preparation.
pub(crate) fn preparation_admission(
    fontification: neovm_core::emacs_core::xdisp::RedisplayFontificationOutcome,
    display_when: &crate::display_when::DisplayWhenConditions,
) -> LayoutLispAdmission {
    if !fontification.entered_callbacks() && display_when.allows_body_reuse() {
        LayoutLispAdmission::NativeOnly
    } else {
        LayoutLispAdmission::MayHaveEntered
    }
}

pub(crate) fn after_span<'a>(
    evaluator: &neovm_core::emacs_core::Context,
    before: &'a neovm_core::window::WindowLayoutAttemptFreshness,
    admission: LayoutLispAdmission,
    mode: FreshnessSpanMode,
) -> Option<std::borrow::Cow<'a, neovm_core::window::WindowLayoutAttemptFreshness>> {
    let admission = match mode {
        FreshnessSpanMode::Capture => LayoutLispAdmission::MayHaveEntered,
        FreshnessSpanMode::Reuse => admission,
    };
    evaluator.window_layout_attempt_freshness_after_span(before, admission)
}

#[cfg(test)]
#[path = "tests/attempt_freshness_test.rs"]
mod tests;
