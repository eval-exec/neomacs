use super::*;

fn register_heap_sqlite_pair() -> (crate::tagged::gc::TaggedHeap, i64, i64) {
    let database_id = NEXT_HANDLE.fetch_add(1, Ordering::SeqCst);
    let statement_id = NEXT_HANDLE.fetch_add(1, Ordering::SeqCst);
    let connection = Rc::new(Connection::open_in_memory().unwrap());
    let statement = prepare_statement(SqliteOperation::Select, &connection, b"SELECT 1")
        .unwrap()
        .into_raw();
    DB_HANDLES.with(|handles| {
        handles
            .borrow_mut()
            .insert(database_id, Rc::clone(&connection));
    });
    RESULT_SETS.with(|sets| {
        sets.borrow_mut().insert(
            statement_id,
            ResultSet {
                connection,
                stmt: statement,
                eof: false,
            },
        );
    });
    let mut heap = crate::tagged::gc::TaggedHeap::new();
    heap.alloc_sqlite(false, database_id);
    heap.alloc_sqlite(true, statement_id);
    (heap, database_id, statement_id)
}

#[test]
fn heap_drop_retains_sqlite_resources_without_registry_borrow() {
    let (heap, database_id, statement_id) = register_heap_sqlite_pair();
    DB_HANDLES.with(|handles| {
        RESULT_SETS.with(|sets| {
            // An accidental SqliteObj destructor would panic on either
            // already-borrowed registry before reaching native teardown.
            let mut handles = handles.borrow_mut();
            let mut sets = sets.borrow_mut();
            drop(heap);
            assert!(handles.contains_key(&database_id));
            assert!(sets.contains_key(&statement_id));
            sets.remove(&statement_id);
            handles.remove(&database_id);
        });
    });
}

#[test]
fn explicit_heap_shutdown_reclaims_sqlite_resources() {
    let (heap, database_id, statement_id) = register_heap_sqlite_pair();
    heap.shutdown().unwrap();
    DB_HANDLES.with(|handles| assert!(!handles.borrow().contains_key(&database_id)));
    RESULT_SETS.with(|sets| assert!(!sets.borrow().contains_key(&statement_id)));
}

#[test]
fn ordinary_sweep_reclaims_sqlite_resources() {
    let (mut heap, database_id, statement_id) = register_heap_sqlite_pair();
    heap.collect_exact(std::iter::empty());
    DB_HANDLES.with(|handles| assert!(!handles.borrow().contains_key(&database_id)));
    RESULT_SETS.with(|sets| assert!(!sets.borrow().contains_key(&statement_id)));
    heap.shutdown().unwrap();
}

#[test]
fn explicit_shutdown_unwind_does_not_repeat_foreign_finalizers() {
    unsafe extern "C" fn count_finalizer(data: *mut std::ffi::c_void) {
        // SAFETY: this fixture's counter outlives the synchronous explicit
        // shutdown, including its caught unwind and automatic Drop fallback.
        unsafe { &*data.cast::<std::sync::atomic::AtomicUsize>() }.fetch_add(1, Ordering::Relaxed);
    }

    let (mut heap, database_id, statement_id) = register_heap_sqlite_pair();
    let count = std::sync::atomic::AtomicUsize::new(0);
    heap.alloc_user_ptr(
        std::ptr::from_ref(&count).cast_mut().cast(),
        Some(count_finalizer),
    );
    DB_HANDLES.with(|handles| {
        let mut held = handles.borrow_mut();
        // The user pointer runs first, followed by statement cleanup, then
        // database cleanup encounters this conflicting registry loan. The
        // detached owner lists keep the unwinding fallback from revisiting
        // the callback or either already-reclaimed identity allocation.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            heap.shutdown().unwrap();
        }));
        assert!(result.is_err());
        assert_eq!(count.load(Ordering::Relaxed), 1);
        assert!(held.contains_key(&database_id));
        held.remove(&database_id);
    });
    RESULT_SETS.with(|sets| assert!(!sets.borrow().contains_key(&statement_id)));
}
