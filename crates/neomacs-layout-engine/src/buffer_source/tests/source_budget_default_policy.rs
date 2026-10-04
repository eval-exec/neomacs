//! Conditional absence-only regression; explicit values are separate controls.
//! Pure OsStr input does not mutate environment or process policy caches.
use super::parse_sync_source_budget;

#[test]
fn promoted_source_budget_defaults_only_when_absent() {
    assert_eq!(parse_sync_source_budget(None), true);
}
