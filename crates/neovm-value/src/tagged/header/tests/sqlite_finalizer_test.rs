//! Deterministic tests for the two `SqliteObj` finalizer branches.
//!
//! The process-global OnceLock cannot be reset, so the typed no-op and the
//! delegation branch are exercised through an explicit table probe; the
//! installed registry path is covered by the sqlite module's
//! `tests/finalizer_hook.rs`.

use super::sqlite_finalizer::{SqliteObjFinalizer, finalize_with};
use super::{SqliteObj, VecLikeHeader, VecLikeType};
use std::sync::atomic::{AtomicUsize, Ordering};

static INSTALLED_CALLS: AtomicUsize = AtomicUsize::new(0);

fn count_installed(_obj: &SqliteObj) {
    INSTALLED_CALLS.fetch_add(1, Ordering::Relaxed);
}

fn object() -> SqliteObj {
    SqliteObj {
        header: VecLikeHeader::new(VecLikeType::Sqlite),
        is_statement: false,
        id: 1,
    }
}

#[test]
fn uninstalled_table_is_the_signed_typed_noop() {
    let obj = object();
    finalize_with(&obj, None);
}

#[test]
fn installed_table_receives_the_object() {
    let obj = object();
    let before = INSTALLED_CALLS.load(Ordering::Relaxed);
    let table = SqliteObjFinalizer {
        finalize: count_installed,
    };
    finalize_with(&obj, Some(&table));
    assert_eq!(INSTALLED_CALLS.load(Ordering::Relaxed), before + 1);
}
