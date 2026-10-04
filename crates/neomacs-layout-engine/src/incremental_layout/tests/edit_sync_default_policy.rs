//! Conditional absence-only regression; explicit values are separate controls.
//! Pure OsStr input does not mutate environment or process policy caches.
use super::{EditSyncMode, parse_edit_sync_mode};

#[test]
fn promoted_edit_sync_defaults_only_when_absent() {
    assert_eq!(parse_edit_sync_mode(None), EditSyncMode::Sync);
}
