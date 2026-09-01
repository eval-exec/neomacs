//! SQLite support matching GNU src/sqlite.c.
//!
//! Rust-backed Elisp implementations live in this subsystem root; the
//! feature-gated inline module keeps rusqlite out of disabled builds while
//! subrs.rs owns the Lisp declarations and startup registration.

#[cfg(feature = "sqlite")]
mod enabled {
    //! Feature-enabled SQLite backend, matching GNU Emacs's sqlite.c.
    //!
    //! Provides the full sqlite Elisp API surface using rusqlite as the backend.
    //! Handle tracking uses thread-local storage to map integer IDs to rusqlite
    //! Connection objects. For 'set mode queries, live prepared statements are
    //! kept in `RESULT_SETS`, mirroring GNU's PVEC_SQLITE statement objects.

    use crate::emacs_core::error::LispCondition;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::ffi::CString;
    use std::ptr;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicI64, Ordering};
    use std::time::Duration;

    use rusqlite::{Connection, OpenFlags, ffi};
    use strum::{EnumString, IntoStaticStr};

    use crate::buffer::CharPos0;
    use crate::emacs_core::error::{EvalResult, Flow, signal};
    use crate::emacs_core::value::ValueKind;
    use crate::emacs_core::value::*;
    use crate::heap_types::LispString;

    // ---------------------------------------------------------------------------
    // Thread-local handle storage
    // ---------------------------------------------------------------------------

    static NEXT_HANDLE: AtomicI64 = AtomicI64::new(1);

    thread_local! {
        /// Open database connections: handle_id -> shared Connection.  Result sets
        /// retain the connection so finalizing their raw statement can never race
        /// the connection's last close.
        static DB_HANDLES: RefCell<HashMap<i64, Rc<Connection>>> = RefCell::new(HashMap::new());

        /// Live result sets for 'set mode: stmt_handle_id -> sqlite3_stmt.
        static RESULT_SETS: RefCell<HashMap<i64, ResultSet>> = RefCell::new(HashMap::new());
    }

    /// A live SQLite result set for incremental iteration.
    struct ResultSet {
        connection: Rc<Connection>,
        stmt: *mut ffi::sqlite3_stmt,
        eof: bool,
    }

    impl Drop for ResultSet {
        fn drop(&mut self) {
            if !self.stmt.is_null() {
                unsafe {
                    ffi::sqlite3_finalize(self.stmt);
                }
                self.stmt = ptr::null_mut();
            }
        }
    }

    impl Drop for crate::tagged::header::SqliteObj {
        fn drop(&mut self) {
            // This Drop can run during thread-local DESTRUCTION (the heap is dropped
            // when its thread exits, which drops the SqliteObj it owns). At that
            // point the `RESULT_SETS`/`DB_HANDLES` thread-locals may already be
            // destroyed, so `with` would panic with `AccessError` ("cannot access a
            // Thread Local Storage value during or after destruction") and abort the
            // process. `try_with` tolerates that — if the registry is already gone
            // there is nothing left to remove.
            if self.is_statement {
                let _ = RESULT_SETS.try_with(|h| {
                    h.borrow_mut().remove(&self.id);
                });
            } else {
                let _ = DB_HANDLES.try_with(|h| {
                    h.borrow_mut().remove(&self.id);
                });
            }
        }
    }

    /// Reset all thread-local state (called between test runs).
    pub(super) fn reset_thread_locals() {
        NEXT_HANDLE.store(1, Ordering::SeqCst);
        DB_HANDLES.with(|h| h.borrow_mut().clear());
        RESULT_SETS.with(|h| h.borrow_mut().clear());
    }

    // ---------------------------------------------------------------------------
    // Handle helpers
    // ---------------------------------------------------------------------------

    /// Extract a DB handle ID from an opaque sqlite Elisp value.
    fn sqlite_db_handle_id(value: &Value) -> Option<i64> {
        let obj = value.as_sqlite()?;
        (!obj.is_statement).then_some(obj.id)
    }

    /// Extract a statement handle ID from an opaque sqlite Elisp value.
    fn sqlite_stmt_handle_id(value: &Value) -> Option<i64> {
        let obj = value.as_sqlite()?;
        obj.is_statement.then_some(obj.id)
    }

    /// Check if a DB handle ID refers to an open connection.
    fn is_open_db(id: i64) -> bool {
        DB_HANDLES.with(|h| h.borrow().contains_key(&id))
    }

    fn expect_db_object(value: &Value) -> Result<i64, Flow> {
        if sqlite_stmt_handle_id(value).is_some() {
            return Err(sqlite_err("Invalid database object"));
        }
        sqlite_db_handle_id(value).ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("sqlitep"), *value],
            )
        })
    }

    /// Expect an open sqlite DB handle, returning the handle ID.
    fn expect_db(value: &Value) -> Result<i64, Flow> {
        let id = expect_db_object(value)?;
        if !is_open_db(id) {
            return Err(sqlite_err("Database closed"));
        }
        Ok(id)
    }

    /// Expect a sqlite statement handle, returning the handle ID.
    fn expect_stmt(value: &Value) -> Result<i64, Flow> {
        // GNU's sqlite-next etc. accept both DB and statement objects,
        // but reject DB objects with "Invalid set object".
        if sqlite_db_handle_id(value).is_some() {
            return Err(signal(
                LispCondition::SqliteError,
                vec![Value::string("Invalid set object")],
            ));
        }
        let id = sqlite_stmt_handle_id(value).ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("sqlitep"), *value],
            )
        })?;
        if !RESULT_SETS.with(|h| h.borrow().contains_key(&id)) {
            return Err(signal(
                LispCondition::SqliteError,
                vec![Value::string("Statement closed")],
            ));
        }
        Ok(id)
    }

    fn make_db_handle(id: i64) -> Value {
        Value::make_sqlite(false, id)
    }

    fn make_stmt_handle(id: i64) -> Value {
        Value::make_sqlite(true, id)
    }

    fn alloc_handle_id() -> i64 {
        NEXT_HANDLE.fetch_add(1, Ordering::SeqCst)
    }

    // ---------------------------------------------------------------------------
    // Argument helpers
    // ---------------------------------------------------------------------------

    fn expect_strict_lisp_string(v: &Value) -> Result<&LispString, Flow> {
        v.as_lisp_string().ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), *v],
            )
        })
    }

    fn sqlite_text_bytes(v: &Value) -> Result<Vec<u8>, Flow> {
        let string = expect_strict_lisp_string(v)?;
        Ok(if string.is_multibyte() {
            crate::encoding::encode_lisp_string(
                string,
                "utf-8",
                crate::emacs_core::coding::EolConversion::Inhibited,
            )
        } else {
            string.as_bytes().to_vec()
        })
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
    #[strum(serialize_all = "kebab-case")]
    pub(super) enum SqliteReturnType {
        Set,
        Full,
    }

    impl SqliteReturnType {
        pub(super) fn from_value(value: &Value) -> Option<Self> {
            value.as_symbol_name()?.parse().ok()
        }

        #[cfg(test)]
        pub(super) fn symbol_name(self) -> &'static str {
            self.into()
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
    #[strum(serialize_all = "kebab-case")]
    pub(super) enum SqliteBindSymbol {
        False,
    }

    impl SqliteBindSymbol {
        pub(super) fn from_value(value: &Value) -> Option<Self> {
            value.as_symbol_name()?.parse().ok()
        }

        #[cfg(test)]
        pub(super) fn symbol_name(self) -> &'static str {
            self.into()
        }
    }

    pub(super) fn value_is_false_symbol(v: &Value) -> bool {
        SqliteBindSymbol::from_value(v) == Some(SqliteBindSymbol::False)
    }

    enum BindValue {
        Null,
        Integer(i64),
        Real(f64),
        Text(Vec<u8>),
        Blob(Vec<u8>),
    }

    fn collect_bind_values(
        eval: &mut crate::emacs_core::eval::Context,
        values: &Value,
    ) -> Result<Vec<BindValue>, Flow> {
        if values.is_nil() {
            return Ok(Vec::new());
        }
        let items = match values.kind() {
            ValueKind::Cons => crate::emacs_core::value::list_to_vec(values).ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("listp"), *values],
                )
            })?,
            ValueKind::Veclike(crate::tagged::header::VecLikeType::Vector) => {
                values.as_vector_data().unwrap().to_vec()
            }
            _ => return Err(sqlite_err("VALUES must be a list or a vector")),
        };

        items
            .into_iter()
            .map(|value| match value.kind() {
                ValueKind::Nil => Ok(BindValue::Null),
                ValueKind::T => Ok(BindValue::Integer(1)),
                ValueKind::Symbol(_) if value_is_false_symbol(&value) => Ok(BindValue::Integer(0)),
                ValueKind::Fixnum(n) => Ok(BindValue::Integer(n)),
                ValueKind::Veclike(crate::tagged::header::VecLikeType::Bignum) => value
                    .as_bignum()
                    .and_then(|n| i64::try_from(n).ok())
                    .map(BindValue::Integer)
                    .ok_or_else(|| sqlite_err("bignum value out of range")),
                ValueKind::Float => Ok(BindValue::Real(value.xfloat())),
                ValueKind::String => {
                    let string = value.as_lisp_string().unwrap();
                    if string.sbytes() == 0 {
                        return Ok(BindValue::Text(Vec::new()));
                    }
                    let coding_system =
                        get_string_text_properties_table_for_value(value).and_then(|table| {
                            table.get_property_at_char_pos(
                                CharPos0::ZERO,
                                Value::symbol("coding-system"),
                            )
                        });
                    if coding_system.is_some_and(|coding| coding.is_symbol_named("binary")) {
                        if string.is_multibyte() {
                            return Err(sqlite_err("BLOB values must be unibyte"));
                        }
                        return Ok(BindValue::Blob(string.as_bytes().to_vec()));
                    }
                    if let Some(coding) = coding_system {
                        let encoded = crate::encoding::builtin_encode_coding_string_in_context(
                            eval,
                            vec![value, coding, Value::NIL, Value::NIL],
                        )?;
                        let encoded = eval.lisp_string(encoded).ok_or_else(|| {
                            signal(
                                LispCondition::WrongTypeArgument,
                                vec![Value::symbol("stringp"), encoded],
                            )
                        })?;
                        return Ok(BindValue::Text(encoded.as_bytes().to_vec()));
                    }
                    Ok(BindValue::Text(sqlite_text_bytes(&value)?))
                }
                _ => Err(sqlite_err("invalid argument")),
            })
            .collect()
    }

    unsafe fn sqlite_errmsg_for_db(db: *mut ffi::sqlite3) -> String {
        if db.is_null() {
            return "sqlite error".to_string();
        }
        let msg = unsafe { ffi::sqlite3_errmsg(db) };
        if msg.is_null() {
            "sqlite error".to_string()
        } else {
            unsafe { std::ffi::CStr::from_ptr(msg) }
                .to_string_lossy()
                .into_owned()
        }
    }

    /// The GNU API deliberately assigns different Lisp conditions to the same
    /// SQLite status depending on which operation observed it.  Keep that policy
    /// typed and exhaustive so a shared helper cannot silently broaden the
    /// `sqlite-locked-error` contract again.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(super) enum SqliteOperation {
        Execute,
        Select,
        Next,
    }

    impl SqliteOperation {
        pub(super) fn condition_for_code(self, code: i32) -> LispCondition {
            match self {
                Self::Execute if code == ffi::SQLITE_LOCKED || code == ffi::SQLITE_BUSY => {
                    LispCondition::SqliteLockedError
                }
                Self::Execute | Self::Select | Self::Next => LispCondition::SqliteError,
            }
        }
    }

    unsafe fn sqlite_prepare_error(
        operation: SqliteOperation,
        db: *mut ffi::sqlite3,
        code: i32,
    ) -> Flow {
        let errstr = unsafe { ffi::sqlite3_errstr(code) };
        let errstr = if errstr.is_null() {
            "sqlite error".to_string()
        } else {
            unsafe { std::ffi::CStr::from_ptr(errstr) }
                .to_string_lossy()
                .into_owned()
        };
        let errmsg = unsafe { sqlite_errmsg_for_db(db) };
        let extended = if db.is_null() {
            0
        } else {
            unsafe { ffi::sqlite3_extended_errcode(db) }
        };
        signal(
            operation.condition_for_code(code),
            vec![Value::list(vec![
                Value::string(errstr),
                Value::string(errmsg),
                Value::make_int(code.into()),
                Value::make_int(extended.into()),
            ])],
        )
    }

    unsafe fn sqlite_step_error(
        operation: SqliteOperation,
        db: *mut ffi::sqlite3,
        code: i32,
    ) -> Flow {
        let message = unsafe { sqlite_errmsg_for_db(db) };
        signal(
            operation.condition_for_code(code),
            vec![Value::string(message)],
        )
    }

    struct PreparedStatement {
        stmt: *mut ffi::sqlite3_stmt,
    }

    impl PreparedStatement {
        fn into_raw(mut self) -> *mut ffi::sqlite3_stmt {
            let stmt = self.stmt;
            self.stmt = ptr::null_mut();
            stmt
        }
    }

    impl Drop for PreparedStatement {
        fn drop(&mut self) {
            if !self.stmt.is_null() {
                unsafe {
                    ffi::sqlite3_finalize(self.stmt);
                }
                self.stmt = ptr::null_mut();
            }
        }
    }

    fn prepare_statement(
        operation: SqliteOperation,
        connection: &Connection,
        sql: &[u8],
    ) -> Result<PreparedStatement, Flow> {
        let sql = CString::new(sql).map_err(|_| sqlite_err("embedded null byte"))?;
        let db = unsafe { connection.handle() };
        let mut stmt = ptr::null_mut();
        let ret =
            unsafe { ffi::sqlite3_prepare_v2(db, sql.as_ptr(), -1, &mut stmt, ptr::null_mut()) };
        if ret == ffi::SQLITE_OK {
            Ok(PreparedStatement { stmt })
        } else {
            if !stmt.is_null() {
                unsafe {
                    ffi::sqlite3_finalize(stmt);
                }
            }
            Err(unsafe { sqlite_prepare_error(operation, db, ret) })
        }
    }

    fn bind_values(
        connection: &Connection,
        stmt: *mut ffi::sqlite3_stmt,
        values: &[BindValue],
    ) -> Result<(), Flow> {
        let db = unsafe { connection.handle() };
        unsafe {
            ffi::sqlite3_reset(stmt);
        }
        for (index, value) in values.iter().enumerate() {
            let index = index as i32 + 1;
            let ret = match value {
                BindValue::Null => unsafe { ffi::sqlite3_bind_null(stmt, index) },
                BindValue::Integer(value) => unsafe {
                    ffi::sqlite3_bind_int64(stmt, index, *value)
                },
                BindValue::Real(value) => unsafe { ffi::sqlite3_bind_double(stmt, index, *value) },
                BindValue::Text(bytes) => unsafe {
                    ffi::sqlite3_bind_text(
                        stmt,
                        index,
                        bytes.as_ptr().cast(),
                        bytes.len() as i32,
                        ffi::SQLITE_TRANSIENT(),
                    )
                },
                BindValue::Blob(bytes) => unsafe {
                    ffi::sqlite3_bind_blob(
                        stmt,
                        index,
                        bytes.as_ptr().cast(),
                        bytes.len() as i32,
                        ffi::SQLITE_TRANSIENT(),
                    )
                },
            };
            if ret != ffi::SQLITE_OK {
                return Err(sqlite_err(&unsafe { sqlite_errmsg_for_db(db) }));
            }
        }
        Ok(())
    }

    unsafe fn row_to_value(stmt: *mut ffi::sqlite3_stmt) -> Value {
        let len = unsafe { ffi::sqlite3_column_count(stmt) };
        let mut row = Vec::with_capacity(len as usize);
        for col in 0..len {
            let value = match unsafe { ffi::sqlite3_column_type(stmt, col) } {
                ffi::SQLITE_INTEGER => {
                    Value::make_int(unsafe { ffi::sqlite3_column_int64(stmt, col) })
                }
                ffi::SQLITE_FLOAT => {
                    Value::make_float(unsafe { ffi::sqlite3_column_double(stmt, col) })
                }
                ffi::SQLITE_BLOB => {
                    let len = unsafe { ffi::sqlite3_column_bytes(stmt, col) };
                    let data = unsafe { ffi::sqlite3_column_blob(stmt, col) };
                    let bytes = if data.is_null() || len <= 0 {
                        Vec::new()
                    } else {
                        unsafe { std::slice::from_raw_parts(data.cast::<u8>(), len as usize) }
                            .to_vec()
                    };
                    Value::heap_string(LispString::from_unibyte(bytes))
                }
                ffi::SQLITE_TEXT => {
                    let len = unsafe { ffi::sqlite3_column_bytes(stmt, col) };
                    let data = unsafe { ffi::sqlite3_column_text(stmt, col) };
                    let bytes = if data.is_null() || len <= 0 {
                        Vec::new()
                    } else {
                        unsafe { std::slice::from_raw_parts(data.cast::<u8>(), len as usize) }
                            .to_vec()
                    };
                    Value::multibyte_string(String::from_utf8_lossy(&bytes).into_owned())
                }
                ffi::SQLITE_NULL => Value::NIL,
                _ => Value::NIL,
            };
            row.push(value);
        }
        Value::list(row)
    }

    unsafe fn column_names(stmt: *mut ffi::sqlite3_stmt) -> Value {
        let count = unsafe { ffi::sqlite3_column_count(stmt) };
        let mut columns = Vec::with_capacity(count as usize);
        for index in 0..count {
            let name = unsafe { ffi::sqlite3_column_name(stmt, index) };
            let name = if name.is_null() {
                "?".to_string()
            } else {
                unsafe { std::ffi::CStr::from_ptr(name) }
                    .to_string_lossy()
                    .into_owned()
            };
            columns.push(Value::string(name));
        }
        Value::list(columns)
    }

    fn connection_for_db(id: i64) -> Result<Rc<Connection>, Flow> {
        DB_HANDLES.with(|handles| {
            handles
                .borrow()
                .get(&id)
                .cloned()
                .ok_or_else(|| sqlite_err("Database closed"))
        })
    }

    fn sqlite_err(msg: &str) -> Flow {
        signal(LispCondition::SqliteError, vec![Value::string(msg)])
    }

    fn sqlite_exec(connection: &Connection, sql: &[u8]) -> bool {
        let Ok(sql) = CString::new(sql) else {
            return false;
        };
        unsafe {
            ffi::sqlite3_exec(
                connection.handle(),
                sql.as_ptr(),
                None,
                ptr::null_mut(),
                ptr::null_mut(),
            ) == ffi::SQLITE_OK
        }
    }

    struct ExtensionLoadingGuard {
        db: *mut ffi::sqlite3,
    }

    impl ExtensionLoadingGuard {
        fn enable(connection: &Connection) -> Option<Self> {
            let db = unsafe { connection.handle() };
            let result = unsafe {
                ffi::sqlite3_db_config(
                    db,
                    ffi::SQLITE_DBCONFIG_ENABLE_LOAD_EXTENSION,
                    1,
                    ptr::null_mut::<std::ffi::c_int>(),
                )
            };
            (result == ffi::SQLITE_OK).then_some(Self { db })
        }
    }

    impl Drop for ExtensionLoadingGuard {
        fn drop(&mut self) {
            unsafe {
                ffi::sqlite3_db_config(
                    self.db,
                    ffi::SQLITE_DBCONFIG_ENABLE_LOAD_EXTENSION,
                    0,
                    ptr::null_mut::<std::ffi::c_int>(),
                );
            }
        }
    }

    // ---------------------------------------------------------------------------
    // Builtin functions
    // ---------------------------------------------------------------------------

    /// Report the compiled-in SQLite backend.
    pub(crate) fn available_p(args: Vec<Value>) -> EvalResult {
        crate::emacs_core::builtins::expect_args("sqlite-available-p", &args, 0)?;
        Ok(Value::T)
    }

    /// (sqlite-version) → version string
    pub(crate) fn version(args: Vec<Value>) -> EvalResult {
        crate::emacs_core::builtins::expect_args("sqlite-version", &args, 0)?;
        Ok(Value::string(rusqlite::version()))
    }

    /// (sqlitep OBJECT) → t or nil
    pub(crate) fn is_sqlite_object(args: Vec<Value>) -> EvalResult {
        crate::emacs_core::builtins::expect_args("sqlitep", &args, 1)?;
        Ok(Value::bool_val(args[0].is_sqlite()))
    }

    /// (sqlite-open &optional FILE READONLY DISABLE-URI) → db-handle or nil
    pub(crate) fn open(
        eval: &mut crate::emacs_core::eval::Context,
        args: Vec<Value>,
    ) -> EvalResult {
        crate::emacs_core::builtins::expect_args_range("sqlite-open", &args, 0, 3)?;
        let file = args
            .first()
            .and_then(|value| (!value.is_nil()).then_some(*value));
        let readonly = args.get(1).is_some_and(|value| value.is_truthy());
        let disable_uri = args.get(2).is_some_and(|value| value.is_truthy());

        let mut flags = if readonly {
            OpenFlags::SQLITE_OPEN_READ_ONLY
        } else {
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
        };
        flags |= OpenFlags::SQLITE_OPEN_FULL_MUTEX;
        if !disable_uri {
            flags |= OpenFlags::SQLITE_OPEN_URI;
        }

        let connection = match file {
            None => Connection::open_in_memory_with_flags(flags | OpenFlags::SQLITE_OPEN_MEMORY),
            Some(file) => {
                let expanded = crate::emacs_core::fileio::builtin_expand_file_name(
                    eval,
                    vec![file, Value::NIL],
                )?;
                let expanded_string = eval.lisp_string(expanded).ok_or_else(|| {
                    signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("stringp"), expanded],
                    )
                })?;
                let path = crate::emacs_core::fileio::lisp_file_name_to_path_buf(expanded_string);
                Connection::open_with_flags(path, flags)
            }
        };
        let Ok(connection) = connection else {
            return Ok(Value::NIL);
        };
        // rusqlite installs a five-second busy timeout on every new connection.
        // GNU Emacs uses sqlite3_open_v2 directly, whose default busy handler
        // returns SQLITE_BUSY immediately.
        if connection.busy_timeout(Duration::ZERO).is_err() {
            return Ok(Value::NIL);
        }

        let id = alloc_handle_id();
        DB_HANDLES.with(|handles| {
            handles.borrow_mut().insert(id, Rc::new(connection));
        });
        Ok(make_db_handle(id))
    }

    /// (sqlite-close DB) → t
    pub(crate) fn close(args: Vec<Value>) -> EvalResult {
        crate::emacs_core::builtins::expect_args("sqlite-close", &args, 1)?;
        let id = expect_db_object(&args[0])?;
        DB_HANDLES.with(|handles| {
            handles.borrow_mut().remove(&id);
        });
        Ok(Value::T)
    }

    /// (sqlite-execute DB QUERY &optional VALUES) → affected-rows or result rows
    pub(crate) fn execute(
        eval: &mut crate::emacs_core::eval::Context,
        args: Vec<Value>,
    ) -> EvalResult {
        crate::emacs_core::builtins::expect_args_range("sqlite-execute", &args, 2, 3)?;
        let id = expect_db(&args[0])?;
        let sql = sqlite_text_bytes(&args[1])?;
        let values = collect_bind_values(eval, &args.get(2).copied().unwrap_or(Value::NIL))?;
        let connection = connection_for_db(id)?;
        let statement = prepare_statement(SqliteOperation::Execute, &connection, &sql)?;
        bind_values(&connection, statement.stmt, &values)?;

        let db = unsafe { connection.handle() };
        let ret = unsafe { ffi::sqlite3_step(statement.stmt) };
        if ret == ffi::SQLITE_ROW {
            let mut rows = Vec::new();
            let mut current = ret;
            while current == ffi::SQLITE_ROW {
                rows.push(unsafe { row_to_value(statement.stmt) });
                current = unsafe { ffi::sqlite3_step(statement.stmt) };
            }
            Ok(Value::list(rows))
        } else if ret == ffi::SQLITE_OK || ret == ffi::SQLITE_DONE {
            Ok(Value::make_int(connection.changes() as i64))
        } else {
            Err(unsafe { sqlite_step_error(SqliteOperation::Execute, db, ret) })
        }
    }

    /// (sqlite-select DB QUERY &optional VALUES RETURN-TYPE) → results
    pub(crate) fn select(
        eval: &mut crate::emacs_core::eval::Context,
        args: Vec<Value>,
    ) -> EvalResult {
        crate::emacs_core::builtins::expect_args_range("sqlite-select", &args, 2, 4)?;
        let id = expect_db(&args[0])?;
        let sql = sqlite_text_bytes(&args[1])?;
        let values = collect_bind_values(eval, &args.get(2).copied().unwrap_or(Value::NIL))?;
        let return_type = args.get(3).and_then(SqliteReturnType::from_value);
        let connection = connection_for_db(id)?;
        let statement = prepare_statement(SqliteOperation::Select, &connection, &sql)?;
        bind_values(&connection, statement.stmt, &values)?;

        if return_type == Some(SqliteReturnType::Set) {
            let stmt_id = alloc_handle_id();
            let stmt = statement.into_raw();
            RESULT_SETS.with(|sets| {
                sets.borrow_mut().insert(
                    stmt_id,
                    ResultSet {
                        connection,
                        stmt,
                        eof: false,
                    },
                );
            });
            return Ok(make_stmt_handle(stmt_id));
        }

        let columns = (return_type == Some(SqliteReturnType::Full))
            .then(|| unsafe { column_names(statement.stmt) });
        let mut rows = Vec::new();
        let mut status = unsafe { ffi::sqlite3_step(statement.stmt) };
        while status == ffi::SQLITE_ROW {
            rows.push(unsafe { row_to_value(statement.stmt) });
            status = unsafe { ffi::sqlite3_step(statement.stmt) };
        }
        if let Some(columns) = columns {
            let mut full = Vec::with_capacity(rows.len() + 1);
            full.push(columns);
            full.extend(rows);
            Ok(Value::list(full))
        } else {
            Ok(Value::list(rows))
        }
    }

    /// (sqlite-next SET) → row or nil
    pub(crate) fn next(args: Vec<Value>) -> EvalResult {
        crate::emacs_core::builtins::expect_args("sqlite-next", &args, 1)?;
        let id = expect_stmt(&args[0])?;
        RESULT_SETS.with(|sets| {
            let mut sets = sets.borrow_mut();
            let set = sets
                .get_mut(&id)
                .ok_or_else(|| sqlite_err("Statement closed"))?;
            if set.eof {
                return Ok(Value::NIL);
            }
            let ret = unsafe { ffi::sqlite3_step(set.stmt) };
            if ret == ffi::SQLITE_ROW {
                Ok(unsafe { row_to_value(set.stmt) })
            } else if ret == ffi::SQLITE_OK || ret == ffi::SQLITE_DONE {
                set.eof = true;
                Ok(Value::NIL)
            } else {
                Err(unsafe {
                    sqlite_step_error(SqliteOperation::Next, set.connection.handle(), ret)
                })
            }
        })
    }

    /// (sqlite-more-p SET) → t or nil
    pub(crate) fn more_p(args: Vec<Value>) -> EvalResult {
        crate::emacs_core::builtins::expect_args("sqlite-more-p", &args, 1)?;
        let id = expect_stmt(&args[0])?;
        let has_more = RESULT_SETS.with(|sets| {
            sets.borrow()
                .get(&id)
                .is_some_and(|result_set| !result_set.eof)
        });
        Ok(Value::bool_val(has_more))
    }

    /// (sqlite-columns SET) → list of column name strings
    pub(crate) fn columns(args: Vec<Value>) -> EvalResult {
        crate::emacs_core::builtins::expect_args("sqlite-columns", &args, 1)?;
        let id = expect_stmt(&args[0])?;
        RESULT_SETS.with(|sets| {
            let sets = sets.borrow();
            let set = sets
                .get(&id)
                .ok_or_else(|| sqlite_err("Statement closed"))?;
            Ok(unsafe { column_names(set.stmt) })
        })
    }

    /// (sqlite-finalize SET) → t
    pub(crate) fn finalize(args: Vec<Value>) -> EvalResult {
        crate::emacs_core::builtins::expect_args("sqlite-finalize", &args, 1)?;
        let id = expect_stmt(&args[0])?;
        RESULT_SETS.with(|sets| {
            sets.borrow_mut().remove(&id);
        });
        Ok(Value::T)
    }

    /// (sqlite-execute-batch DB STATEMENTS) → t or nil
    pub(crate) fn execute_batch(
        _eval: &mut crate::emacs_core::eval::Context,
        args: Vec<Value>,
    ) -> EvalResult {
        crate::emacs_core::builtins::expect_args("sqlite-execute-batch", &args, 2)?;
        let id = expect_db(&args[0])?;
        let statements = sqlite_text_bytes(&args[1])?;
        let connection = connection_for_db(id)?;
        Ok(Value::bool_val(sqlite_exec(&connection, &statements)))
    }

    /// (sqlite-transaction DB) → t or nil
    pub(crate) fn transaction(args: Vec<Value>) -> EvalResult {
        crate::emacs_core::builtins::expect_args("sqlite-transaction", &args, 1)?;
        let connection = connection_for_db(expect_db(&args[0])?)?;
        Ok(Value::bool_val(sqlite_exec(&connection, b"begin")))
    }

    /// (sqlite-commit DB) → t or nil
    pub(crate) fn commit(args: Vec<Value>) -> EvalResult {
        crate::emacs_core::builtins::expect_args("sqlite-commit", &args, 1)?;
        let connection = connection_for_db(expect_db(&args[0])?)?;
        Ok(Value::bool_val(sqlite_exec(&connection, b"commit")))
    }

    /// (sqlite-rollback DB) → t or nil
    pub(crate) fn rollback(args: Vec<Value>) -> EvalResult {
        crate::emacs_core::builtins::expect_args("sqlite-rollback", &args, 1)?;
        let connection = connection_for_db(expect_db(&args[0])?)?;
        Ok(Value::bool_val(sqlite_exec(&connection, b"rollback")))
    }

    /// (sqlite-pragma DB PRAGMA) → t or nil
    pub(crate) fn pragma(args: Vec<Value>) -> EvalResult {
        crate::emacs_core::builtins::expect_args("sqlite-pragma", &args, 2)?;
        let connection = connection_for_db(expect_db(&args[0])?)?;
        let pragma = sqlite_text_bytes(&args[1])?;
        let mut statement = Vec::with_capacity(b"PRAGMA ".len() + pragma.len());
        statement.extend_from_slice(b"PRAGMA ");
        statement.extend_from_slice(&pragma);
        Ok(Value::bool_val(sqlite_exec(&connection, &statement)))
    }

    /// (sqlite-load-extension DB MODULE) → t
    ///
    /// GNU semantics: load a SQLite extension, restricted to an allowlist.
    pub(crate) fn load_extension(
        eval: &mut crate::emacs_core::eval::Context,
        args: Vec<Value>,
    ) -> EvalResult {
        crate::emacs_core::builtins::expect_args("sqlite-load-extension", &args, 2)?;
        let id = expect_db(&args[0])?;
        let module = crate::emacs_core::emacs_char::to_utf8_lossy(
            expect_strict_lisp_string(&args[1])?.as_bytes(),
        );

        // GNU's allowlist of allowed extension names.
        const ALLOWED_EXTENSIONS: &[&str] = &[
            "base64",
            "cksumvfs",
            "compress",
            "csv",
            "csvtable",
            "fts3",
            "icu",
            "pcre",
            "percentile",
            "regexp",
            "rot13",
            "rtree",
            "sha1",
            "uuid",
            "vec0",
            "vector0",
            "vfslog",
            "vss0",
            "zipfile",
        ];

        let file_name = std::path::Path::new(&module)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(module.as_str());
        let module_name = file_name
            .strip_prefix("libsqlite3_mod_")
            .unwrap_or(file_name);
        let allowed = ALLOWED_EXTENSIONS.iter().any(|allow| {
            let Some(suffix) = module_name.strip_prefix(allow) else {
                return false;
            };
            !suffix.is_empty()
                && (suffix == ".so" || suffix == ".dylib" || suffix.eq_ignore_ascii_case(".dll"))
        });
        if !allowed {
            return Err(signal(
                LispCondition::SqliteError,
                vec![Value::string("Module name not on allowlist")],
            ));
        }

        let expanded =
            crate::emacs_core::fileio::builtin_expand_file_name(eval, vec![args[1], Value::NIL])?;
        let Some(expanded_ls) = eval.lisp_string(expanded) else {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), expanded],
            ));
        };
        // Issue #131: build the extension path CString from the real Emacs/OS bytes
        // (on Unix this is the file-name byte sequence), not the PUA-sentinel storage
        // string, so a non-UTF-8 path is preserved.
        let Ok(ext_fn) = CString::new(expanded_ls.as_bytes()) else {
            return Ok(Value::NIL);
        };

        let connection = connection_for_db(id)?;
        let loaded = if let Some(_guard) = ExtensionLoadingGuard::enable(&connection) {
            let mut err_msg: *mut std::os::raw::c_char = ptr::null_mut();
            let result = unsafe {
                ffi::sqlite3_load_extension(
                    connection.handle(),
                    ext_fn.as_ptr(),
                    ptr::null(),
                    &mut err_msg,
                )
            };
            if !err_msg.is_null() {
                unsafe {
                    ffi::sqlite3_free(err_msg.cast());
                }
            }
            result == ffi::SQLITE_OK
        } else {
            false
        };

        Ok(Value::bool_val(loaded))
    }
}

#[cfg(feature = "sqlite")]
pub(crate) use enabled::{
    available_p, close, columns, commit, execute, execute_batch, finalize, is_sqlite_object,
    load_extension, more_p, next, open, pragma, rollback, select, transaction, version,
};

#[cfg(not(feature = "sqlite"))]
pub(crate) fn is_sqlite_object(
    args: Vec<crate::emacs_core::value::Value>,
) -> crate::emacs_core::error::EvalResult {
    crate::emacs_core::builtins::expect_args("sqlitep", &args, 1)?;
    Ok(crate::emacs_core::value::Value::NIL)
}

#[cfg(not(feature = "sqlite"))]
pub(crate) fn available_p(
    args: Vec<crate::emacs_core::value::Value>,
) -> crate::emacs_core::error::EvalResult {
    crate::emacs_core::builtins::expect_args("sqlite-available-p", &args, 0)?;
    Ok(crate::emacs_core::value::Value::NIL)
}

mod subrs;
#[cfg(test)]
pub(crate) use subrs::SUBRS;
pub(crate) use subrs::register_subrs;

pub(super) fn reset_sqlite_thread_locals() {
    #[cfg(feature = "sqlite")]
    enabled::reset_thread_locals();
}

#[cfg(all(test, feature = "sqlite"))]
use crate::emacs_core::error::LispCondition;
#[cfg(all(test, feature = "sqlite"))]
use enabled::{SqliteBindSymbol, SqliteOperation, SqliteReturnType, value_is_false_symbol};
#[cfg(all(test, feature = "sqlite"))]
use rusqlite::ffi;

#[cfg(all(test, feature = "sqlite"))]
#[path = "tests/enabled.rs"]
mod tests;

#[cfg(all(test, not(feature = "sqlite")))]
#[path = "tests/disabled.rs"]
mod tests;
