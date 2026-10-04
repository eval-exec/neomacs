//! Attempt-borrowed numeric exclusion proof for the old fontification query.
//! OnceLock publishes immutable process policy. A proof borrows only current
//! row descriptors and retains no source, Lisp owner, cache or shared mutable
//! state; independent mutators may use different immutable snapshots at once.
use neovm_core::buffer::CharPos0;
use neovm_core::window::WindowDisplaySnapshot;
use std::ffi::OsStr;

/// Pure startup parser. Only explicit ON aliases enable this default-OFF flag.
/// No Lisp or mutable per-buffer state is read, retained or published.
#[cold]
#[inline(never)]
fn parse(value: Option<&OsStr>) -> bool {
    value.and_then(OsStr::to_str).is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "on" | "1" | "true" | "yes"
        )
    })
}
#[inline]
pub(super) fn enabled() -> bool {
    #[cfg(test)]
    if let Some(value) =
        crate::incremental_layout::edit_sync::fontify_coverage_test_support::forced()
    {
        return value;
    }
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED
        .get_or_init(|| parse(std::env::var_os("NEOMACS_EDIT_SYNC_FONTIFY_COVERAGE").as_deref()))
}

/// Borrow only initialized immutable numeric row extrema. The exact old query
/// interval is [prepass_end,accessible_end) after its one-based conversion.
/// Empty intervals are universally inert; otherwise Some(rows) is authoritative
/// even when flat points coexist. Unknown/nonpositive/unrepresentable bounds,
/// possible intersection and nonincreasing/overlapping row ranges conservatively
/// refuse the whole proof. Current placed extrema include accepted source shifts.
/// Checked producers establish private descriptor arithmetic; no invalid private
/// placement, clamp, inferred endpoint or source-owner cache is manufactured.
#[inline]
pub(super) fn excludes_every_query(
    snapshot: &WindowDisplaySnapshot,
    prepass_end: CharPos0,
    accessible_end: CharPos0,
) -> bool {
    let start = prepass_end.get();
    let end = accessible_end.get();
    if end <= start {
        return true;
    }
    let Some(rows) = &snapshot.point_rows else {
        return false;
    };
    let mut previous_max = None;
    for row in &rows.rows {
        if row.point_count() == 0 {
            continue;
        }
        let (Some(min), Some(max)) = (row.min_buffer_position(), row.max_buffer_position()) else {
            return false;
        };
        let min = min.as_i64();
        let max = max.as_i64();
        if min <= 0 || max < min || previous_max.is_some_and(|prior| min <= prior) {
            return false;
        }
        previous_max = Some(max);
        let Some(first) = min
            .checked_sub(1)
            .and_then(|value| usize::try_from(value).ok())
        else {
            return false;
        };
        let Some(last) = max
            .checked_sub(1)
            .and_then(|value| usize::try_from(value).ok())
        else {
            return false;
        };
        if !(last < start || first >= end) {
            return false;
        }
    }
    true
}

#[cfg(test)]
#[path = "tests/fontify_coverage_policy_test.rs"]
mod policy_tests;
