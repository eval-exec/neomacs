//! Numeric mode-line padding provenance, separate from source-string fields.

use std::ffi::OsStr;
use std::ops::Range;

/// Immutable process policy published once by OnceLock; no Lisp state is
/// cached and independent mutators may read the selector concurrently.
#[inline]
pub(super) fn enabled() -> bool {
    #[cfg(test)]
    if let Some(enabled) = OVERRIDE.with(std::cell::Cell::get) {
        return enabled;
    }
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| parse(std::env::var_os("NEOMACS_MODE_LINE_NUMERIC_PADDING").as_deref()))
}

pub(super) fn parse(value: Option<&OsStr>) -> bool {
    if value.is_none() {
        return true;
    }
    matches!(
        value
            .and_then(OsStr::to_str)
            .map(|v| v.trim().to_ascii_lowercase())
            .as_deref(),
        Some("on" | "1" | "true" | "yes")
    )
}

#[cfg(test)]
thread_local! { static OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) }; }
#[cfg(test)]
pub(super) fn force_for_test(value: Option<bool>) -> Option<bool> {
    OVERRIDE.with(|v| v.replace(value))
}

/// Sorted, disjoint character ranges produced by numeric wrapper padding.
/// This numeric-only sidecar belongs exclusively to one synchronous formatter
/// accumulator; it retains no Lisp pointers and is never shared across mutators.
#[derive(Clone, Debug, Default)]
pub(super) struct PaddingRanges(Vec<Range<usize>>);

impl PaddingRanges {
    pub(super) fn mark(&mut self, range: Range<usize>) {
        if range.is_empty() {
            return;
        }
        if let Some(last) = self.0.last_mut() {
            debug_assert!(last.end <= range.start);
            if last.end == range.start {
                last.end = range.end;
                return;
            }
        }
        self.0.push(range);
    }

    pub(super) fn append_shifted(&mut self, other: &Self, offset: usize) {
        for range in &other.0 {
            self.mark(range.start.saturating_add(offset)..range.end.saturating_add(offset));
        }
    }

    pub(super) fn clipped(&self, end: usize) -> Self {
        Self(
            self.0
                .iter()
                .filter(|r| r.start < end)
                .map(|r| r.start..r.end.min(end))
                .collect(),
        )
    }

    /// A display min-width stretch is new output and inherits that run's own
    /// appearance. Split a crossing numeric interval rather than excluding it.
    pub(super) fn insert_unmarked(&mut self, at: usize, len: usize) {
        if len == 0 || self.0.is_empty() {
            return;
        }
        let mut shifted = Self::default();
        for r in &self.0 {
            if r.end <= at {
                shifted.mark(r.clone());
            } else if r.start >= at {
                shifted.mark(r.start.saturating_add(len)..r.end.saturating_add(len));
            } else {
                shifted.mark(r.start..at);
                shifted.mark(at.saturating_add(len)..r.end.saturating_add(len));
            }
        }
        *self = shifted;
    }

    pub(super) fn for_each_unmarked_range(&self, end: usize, mut f: impl FnMut(Range<usize>)) {
        let mut cursor = 0;
        for r in &self.0 {
            let start = r.start.min(end);
            if cursor < start {
                f(cursor..start);
            }
            cursor = r.end.min(end);
            if cursor == end {
                break;
            }
        }
        if cursor < end {
            f(cursor..end);
        }
    }
}

#[cfg(test)]
#[path = "tests/numeric_padding_default_policy.rs"]
mod numeric_padding_default_policy_tests;
