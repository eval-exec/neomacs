use super::*;

fn standalone_sqlite_obj(is_statement: bool, id: i64) -> Box<neovm_value::tagged::header::SqliteObj> {
    Box::new(neovm_value::tagged::header::SqliteObj {
        header: neovm_value::tagged::header::VecLikeHeader::new(
            neovm_value::tagged::header::VecLikeType::Sqlite,
        ),
        is_statement,
        id,
    })
}

#[test]
fn installed_finalizer_removes_both_registry_kinds() {
    install_sqlite_finalizer_hook()
        .expect("the canonical sqlite finalizer table is process-unique");
    let database_id = NEXT_HANDLE.fetch_add(1, Ordering::SeqCst);
    let statement_id = NEXT_HANDLE.fetch_add(1, Ordering::SeqCst);
    let connection = Rc::new(Connection::open_in_memory().unwrap());
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
                stmt: ptr::null_mut(),
                eof: true,
            },
        );
    });

    drop(standalone_sqlite_obj(false, database_id));
    drop(standalone_sqlite_obj(true, statement_id));

    DB_HANDLES.with(|handles| assert!(!handles.borrow().contains_key(&database_id)));
    RESULT_SETS.with(|sets| assert!(!sets.borrow().contains_key(&statement_id)));
}
