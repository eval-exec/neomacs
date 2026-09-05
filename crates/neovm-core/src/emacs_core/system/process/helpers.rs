//! Process helpers: argument validation, string/list coercion, and the small shared utilities the process builtins call.
//!
//! Moved out of `mod.rs` unchanged; a child module so it keeps the
//! parent's view of its private items (`use super::*`).

use super::*;

pub(super) fn check_keyword_arg_pairs(args: &[Value]) -> Result<(), Flow> {
    if args.len().is_multiple_of(2) {
        Ok(())
    } else {
        Err(signal(LispCondition::MalformedKeywordArgList, vec![]))
    }
}

pub(super) fn process_owned_runtime_string(value: Value) -> String {
    value
        .as_lisp_string()
        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
        .expect("ValueKind::String must carry LispString payload")
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(super) fn expect_list(value: &Value) -> Result<(), Flow> {
    if value.is_list() {
        Ok(())
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), *value],
        ))
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(super) fn signal_wrong_type_sequence(value: Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("sequencep"), value],
    )
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(super) fn signal_wrong_type_character(value: Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("characterp"), value],
    )
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(super) fn char_code_from_value(value: &Value) -> Result<u32, Flow> {
    match value.kind() {
        ValueKind::Fixnum(_) => Ok(super::super::builtins::expect_character_code(value)? as u32),
        _ => Err(signal_wrong_type_character(*value)),
    }
}

/// Append the Emacs-internal byte encoding of a single character code.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(super) fn push_char_code_bytes(code: u32, bytes: &mut Vec<u8>) {
    let mut buf = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
    let len = crate::emacs_core::emacs_char::char_string(code, &mut buf);
    bytes.extend_from_slice(&buf[..len]);
}

/// Convert a string / character-code vector / character-code list into a
/// faithful multibyte `LispString`, encoding each character code directly to
/// Emacs bytes via `char_string`.
///
/// Issue #131: this replaces a storage-String round-trip that corrupted real
/// character codes in the PUA sentinel ranges — e.g. the nerd-font glyph
/// U+E0B0 was rewritten to the eight-bit code 0x3FFFB0. Building the bytes
/// directly keeps every code intact.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn char_sequence_to_lisp_string(value: &Value) -> Result<LispString, Flow> {
    if let Some(string) = value.as_lisp_string() {
        return Ok(string.clone());
    }
    let mut bytes = Vec::new();
    match value.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let vec = value.as_vector_data().unwrap().clone();
            for elt in vec.iter() {
                push_char_code_bytes(char_code_from_value(elt)?, &mut bytes);
            }
        }
        ValueKind::Cons | ValueKind::Nil => {
            let mut cursor = *value;
            loop {
                match cursor.kind() {
                    ValueKind::Nil => break,
                    ValueKind::Cons => {
                        let car = cursor.cons_car();
                        let cdr = cursor.cons_cdr();
                        push_char_code_bytes(char_code_from_value(&car)?, &mut bytes);
                        cursor = cdr;
                    }
                    _ => {
                        return Err(signal(
                            LispCondition::WrongTypeArgument,
                            vec![Value::symbol("listp"), cursor],
                        ));
                    }
                }
            }
        }
        _ => return Err(signal_wrong_type_sequence(*value)),
    }
    Ok(crate::heap_types::LispString::from_emacs_bytes(bytes))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn expect_int_or_marker(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        ValueKind::Veclike(VecLikeType::Marker) => {
            super::super::marker::marker_position_as_int(value)
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), *value],
        )),
    }
}

pub(crate) fn checked_region_bytes(
    buf: &crate::buffer::Buffer,
    region: super::super::position::LispRegionArgs,
) -> Result<EmacsByteRange, Flow> {
    region.accessible_byte_range(buf)
}

pub(super) fn file_error_symbol(kind: std::io::ErrorKind) -> &'static str {
    match kind {
        std::io::ErrorKind::NotFound => "file-missing",
        std::io::ErrorKind::AlreadyExists => "file-already-exists",
        std::io::ErrorKind::PermissionDenied => "permission-denied",
        _ => "file-error",
    }
}

pub(crate) fn signal_process_io(action: &str, target: Option<&str>, err: std::io::Error) -> Flow {
    let mut data = vec![Value::string(action), Value::string(err.to_string())];
    if let Some(target) = target {
        data.push(Value::string(target));
    }
    signal(file_error_symbol(err.kind()), data)
}

/// GNU `report_file_error (STRING, FILENAME)` (callproc.c/fileio.c) for a
/// subprocess file-open/IO failure: signal a file-error-family condition whose
/// DATA is `(STRING STRERROR FILENAME)`, deriving the error SYMBOL and the bare
/// `strerror` string (no Rust "(os error N)" suffix) from the underlying
/// `errno`.  Use this instead of `signal_process_io` whenever the failing
/// operation has a Lisp filename to report — GNU always includes it.
#[cfg(unix)]
pub(crate) fn signal_process_file_error(
    action: &str,
    filename: Value,
    err: std::io::Error,
) -> Flow {
    let errno = err.raw_os_error().unwrap_or(libc::EIO);
    signal_file_errno(action, filename, errno)
}

#[cfg(not(unix))]
pub(crate) fn signal_process_file_error(
    action: &str,
    filename: Value,
    err: std::io::Error,
) -> Flow {
    let mut data = vec![
        Value::string(action),
        Value::string(err.to_string()),
        filename,
    ];
    signal(file_error_symbol(err.kind()), data)
}

/// The bare strerror string for an errno, matching GNU's `emacs_strerror`
/// (e.g. ENOENT -> "No such file or directory").  Rust's
/// `io::Error::to_string()` appends "(os error N)", which GNU never emits, so
/// go through libc directly.
#[cfg(unix)]
pub(super) fn errno_message(errno: libc::c_int) -> String {
    // SAFETY: strerror returns a pointer to a static (per-thread) C string.
    unsafe {
        let ptr = libc::strerror(errno);
        if ptr.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned()
        }
    }
}

#[cfg(not(unix))]
pub(super) fn errno_message(errno: libc::c_int) -> String {
    std::io::Error::from_raw_os_error(errno).to_string()
}

/// GNU `report_file_errno` (fileio.c): signal a file-error-family condition
/// whose DATA is `(STRING ERRNO-STRING . NAME-LIST)` and whose error SYMBOL is
/// derived from ERRNO (ENOENT -> `file-missing`, EEXIST -> `file-already-exists`,
/// EACCES -> `permission-denied`, else `file-error`).  NAME is wrapped in a
/// one-element list unless it is itself a list (or nil), exactly like
/// `get_file_errno_data`.
pub(crate) fn signal_file_errno(string: &str, name: Value, errno: libc::c_int) -> Flow {
    let symbol = match errno {
        libc::ENOENT => "file-missing",
        libc::EEXIST => "file-already-exists",
        libc::EACCES => "permission-denied",
        _ => "file-error",
    };
    let mut data = vec![Value::string(string), Value::string(errno_message(errno))];
    if name.is_cons() || name.is_nil() {
        if let Some(items) = super::super::value::list_to_vec(&name) {
            data.extend(items);
        }
    } else {
        data.push(name);
    }
    signal(symbol, data)
}

pub(super) fn signal_wrong_type_string(value: Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("stringp"), value],
    )
}

pub(crate) fn expect_string_strict(value: &Value) -> Result<String, Flow> {
    match value.kind() {
        ValueKind::String => Ok(process_owned_runtime_string(*value)),
        _ => Err(signal_wrong_type_string(*value)),
    }
}

pub(super) fn expect_network_lookup_hostname(value: &Value) -> Result<String, Flow> {
    let string = match value.kind() {
        ValueKind::String => value
            .as_lisp_string()
            .expect("ValueKind::String must carry LispString payload"),
        _ => return Err(signal_wrong_type_string(*value)),
    };

    if string.is_multibyte() && string.sbytes() != string.schars() {
        let hostname = crate::emacs_core::emacs_char::to_utf8_lossy(string.as_bytes());
        return Err(signal(
            "error",
            vec![Value::string(format!(
                "Non-ASCII hostname {hostname} detected, please use \u{2018}puny-encode-domain\u{2019}"
            ))],
        ));
    }

    Ok(crate::emacs_core::emacs_char::to_utf8_lossy(
        string.as_bytes(),
    ))
}

pub(super) fn expect_process_name_lisp_string(value: &Value) -> Result<LispString, Flow> {
    match value.kind() {
        ValueKind::String => Ok(value
            .as_lisp_string()
            .expect("ValueKind::String must carry LispString payload")
            .clone()),
        _ => Err(signal(
            "error",
            vec![Value::string(":name value not a string")],
        )),
    }
}

pub(super) fn keyword_name(value: &Value) -> Option<&str> {
    match value.kind() {
        ValueKind::Symbol(k) => Some(resolve_sym(k)),
        _ => None,
    }
}
pub(crate) fn parse_lisp_string_args_strict(args: &[Value]) -> Result<Vec<LispString>, Flow> {
    args.iter()
        .map(|arg| {
            super::super::builtins::expect_lisp_string(arg)
                .cloned()
                .map_err(|_| signal_wrong_type_string(*arg))
        })
        .collect()
}

pub(super) fn signal_wrong_type_processp(value: Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("processp"), value],
    )
}

pub(super) fn signal_process_does_not_exist(name: &str) -> Flow {
    signal(
        "error",
        vec![Value::string(format!("Process {name} does not exist"))],
    )
}

pub(super) fn signal_buffer_has_no_process(buffers: &BufferManager, buffer_id: BufferId) -> Flow {
    signal(
        "error",
        vec![Value::string(format!(
            "Buffer {} has no process",
            buffers
                .get(buffer_id)
                .map(|buffer| buffer.name_runtime_string_owned())
                .unwrap_or_else(|| "<deleted buffer>".to_string())
        ))],
    )
}

pub(super) fn signal_process_not_active_in_manager(
    processes: &ProcessManager,
    id: ProcessId,
) -> Flow {
    let name = processes
        .get_any(id)
        .map(|proc| process_name_runtime(proc.name))
        .unwrap_or_else(|| id.to_string());
    signal(
        "error",
        vec![Value::string(format!("Process {name} is not active"))],
    )
}

/// GNU `process_send_signal`'s first guard, `!EQ (p->type, Qreal)`
/// (src/process.c:7084-7086).
///
/// It is asked of the process OBJECT, which `get_process` (:7081) resolves
/// without consulting liveness, so it must be asked through `get_any` here.
/// Asking it through the live table instead let a retired network, serial or
/// pipe process fall past it and be treated as signalable -- invisible until
/// ledger 169 started retiring processes before their sentinels run.
pub(super) fn check_process_is_real_subprocess(
    processes: &ProcessManager,
    id: ProcessId,
) -> Result<(), Flow> {
    match processes.get_any(id) {
        Some(proc) if proc.kind != ProcessKind::Real => Err(signal_process_not_subprocess(proc)),
        _ => Ok(()),
    }
}

pub(super) fn signal_process_not_subprocess(proc: &Process) -> Flow {
    signal(
        "error",
        vec![Value::string(format!(
            "Process {} is not a subprocess",
            process_name_runtime(proc.name)
        ))],
    )
}

pub(super) fn signal_cannot_signal_process(proc: &Process) -> Flow {
    signal(
        "error",
        vec![Value::string(format!(
            "Cannot signal process {}",
            process_name_runtime(proc.name)
        ))],
    )
}

pub(super) fn process_not_running_reason(proc: &Process) -> String {
    if process_is_listening(proc) {
        "listen".to_string()
    } else {
        // GNU's `error ("Process %s not running: %s", ..., status_message (p))`
        // (:6728, :7455) runs one line after `update_status`, so the reason
        // names the status the gate just rejected.
        gnu_process_status_message_for_status(proc, process_effective_status(proc))
    }
}

pub(super) fn signal_process_not_running_in_manager(
    processes: &ProcessManager,
    id: ProcessId,
) -> Flow {
    let (name, reason) = processes
        .get_any(id)
        .map(|proc| {
            (
                process_name_runtime(proc.name),
                process_not_running_reason(proc),
            )
        })
        .unwrap_or_else(|| (id.to_string(), "inactive".to_string()));
    signal(
        "error",
        vec![Value::string(format!(
            "Process {name} not running: {reason}"
        ))],
    )
}

/// Decode a process designator into a raw `ProcessId` candidate.
///
/// This is the single root that maps a Lisp value to a process key.  Like GNU's
/// `get_process` / `CHECK_PROCESS`, only a genuine process object designates a
/// process by identity — a bare integer is NOT a process (GNU signals
/// `wrong-type-argument processp`).  It does NOT validate that the id still
/// names a live/known process; callers layer their own `get`/`get_any` checks
/// on top.  Name-string and nil (current-buffer) designators are handled by the
/// individual resolvers since they need manager/buffer state.
pub(crate) fn process_value_to_id(value: &Value) -> Option<ProcessId> {
    value.as_process_id()
}

pub(super) fn resolve_process_or_wrong_type_any_in_manager(
    processes: &ProcessManager,
    value: &Value,
) -> Result<ProcessId, Flow> {
    if let Some(id) = process_value_to_id(value) {
        return if processes.get_any(id).is_some() {
            Ok(id)
        } else {
            Err(signal_wrong_type_processp(*value))
        };
    }
    match value.kind() {
        ValueKind::String => {
            let name = process_owned_runtime_string(*value);
            processes
                .find_by_name(&name)
                .ok_or_else(|| signal_wrong_type_processp(*value))
        }
        _ => Err(signal_wrong_type_processp(*value)),
    }
}

pub(super) fn resolve_process_object_or_wrong_type_any_in_manager(
    processes: &ProcessManager,
    value: &Value,
) -> Result<ProcessId, Flow> {
    process_value_to_id(value)
        .filter(|id| processes.get_any(*id).is_some())
        .ok_or_else(|| signal_wrong_type_processp(*value))
}

pub(super) fn resolve_process_for_status_in_state(
    processes: &ProcessManager,
    buffers: &BufferManager,
    value: &Value,
) -> Result<Option<ProcessId>, Flow> {
    if let Some(id) = process_value_to_id(value) {
        return if processes.get_any(id).is_some() {
            Ok(Some(id))
        } else {
            Err(signal_wrong_type_processp(*value))
        };
    }
    match value.kind() {
        ValueKind::String => {
            let name = process_owned_runtime_string(*value);
            Ok(processes.find_by_name(&name))
        }
        ValueKind::Nil => {
            let current_buffer = buffers
                .current_buffer_id()
                .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
            processes
                .find_by_buffer_id(current_buffer)
                .map(Some)
                .ok_or_else(|| signal_buffer_has_no_process(buffers, current_buffer))
        }
        ValueKind::Veclike(VecLikeType::Buffer) => {
            let buffer_id = value.as_buffer_id().unwrap();
            if buffers.get(buffer_id).is_none() {
                return Err(signal(
                    "error",
                    vec![Value::string("Attempt to get process for a dead buffer")],
                ));
            }
            processes
                .find_by_buffer_id(buffer_id)
                .map(Some)
                .ok_or_else(|| signal_buffer_has_no_process(buffers, buffer_id))
        }
        _ => Err(signal_wrong_type_processp(*value)),
    }
}

pub(super) fn resolve_get_process_designator_in_state(
    processes: &ProcessManager,
    buffers: &BufferManager,
    value: &Value,
) -> Result<ProcessId, Flow> {
    if let Some(id) = process_value_to_id(value) {
        return if processes.get_any(id).is_some() {
            Ok(id)
        } else {
            Err(signal_wrong_type_processp(*value))
        };
    }

    match value.kind() {
        ValueKind::String => {
            let name = process_owned_runtime_string(*value);
            if let Some(id) = processes.find_by_name(&name) {
                return Ok(id);
            }
            if let Some(buffer_id) = buffers.find_buffer_by_name(&name) {
                return processes
                    .find_by_buffer_id(buffer_id)
                    .ok_or_else(|| signal_buffer_has_no_process(buffers, buffer_id));
            }
            Err(signal_process_does_not_exist(&name))
        }
        ValueKind::Nil => {
            let current_buffer = buffers
                .current_buffer_id()
                .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
            processes
                .find_by_buffer_id(current_buffer)
                .ok_or_else(|| signal_buffer_has_no_process(buffers, current_buffer))
        }
        ValueKind::Veclike(VecLikeType::Buffer) => {
            let buffer_id = value.as_buffer_id().unwrap();
            if buffers.get(buffer_id).is_none() {
                return Err(signal(
                    "error",
                    vec![Value::string("Attempt to get process for a dead buffer")],
                ));
            }
            processes
                .find_by_buffer_id(buffer_id)
                .ok_or_else(|| signal_buffer_has_no_process(buffers, buffer_id))
        }
        _ => Err(signal_wrong_type_processp(*value)),
    }
}

/// GNU's `Fget_buffer` (src/buffer.c:479-491), which is how both of this
/// port's buffer-keyed process lookups name a buffer:
///
/// ```c
///   if (BUFFERP (buffer_or_name))
///     return buffer_or_name;
///   CHECK_STRING (buffer_or_name);
///   return Fcdr (assoc_ignore_text_properties (buffer_or_name, Vbuffer_alist));
/// ```
///
/// Three things it does NOT do, each of which this function used to.  A buffer
/// OBJECT comes back as given, dead or alive -- the docstring's "If
/// BUFFER-OR-NAME is a buffer, return it as given" (:483) -- so a process
/// whose buffer was killed is still findable by that buffer, which is the
/// state `Fget_buffer_process`'s own docstring describes ("Return nil if all
/// processes associated with BUFFER have been deleted or killed",
/// src/process.c:8414-8415: the BUFFER may outlive nothing at all).  A name is
/// matched against `Vbuffer_alist`, which holds only live buffers.  And `nil`
/// is not a designator for anything: `Fget_buffer_process` answers it with
/// `if (NILP (buffer)) return Qnil;` (:8421) rather than reaching for the
/// selected window, and `Fmake_network_process` reads `buffer_defaults` for it
/// (:4132-4135).
///
/// This is deliberately NOT `get_process`'s rule (src/process.c:1045-1048),
/// which errors with "Attempt to get process for a dead buffer": that is a
/// PROCESS designator and this is a buffer one.  The two neighbours above it
/// implement `get_process`, and they are right to differ.
pub(super) fn resolve_buffer_for_process_lookup_in_state(
    buffers: &BufferManager,
    value: &Value,
) -> Result<Option<crate::buffer::BufferId>, Flow> {
    match value.kind() {
        ValueKind::Nil => Ok(None),
        ValueKind::String => {
            let name_str = process_owned_runtime_string(*value);
            Ok(buffers.find_buffer_by_name(&name_str))
        }
        ValueKind::Veclike(VecLikeType::Buffer) => Ok(value.as_buffer_id()),
        _ => Err(signal_wrong_type_string(*value)),
    }
}

pub(super) fn resolve_live_process_designator_in_manager(
    processes: &ProcessManager,
    value: &Value,
) -> Option<ProcessId> {
    let id = process_value_to_id(value)?;
    processes.get(id).map(|_| id)
}

pub(super) fn resolve_live_process_or_wrong_type_in_manager(
    processes: &ProcessManager,
    value: &Value,
) -> Result<ProcessId, Flow> {
    resolve_live_process_designator_in_manager(processes, value).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), *value],
        )
    })
}

pub(super) fn current_thread_handle(threads: &ThreadManager) -> Value {
    threads
        .thread_handle(threads.current_thread_id())
        .unwrap_or(Value::NIL)
}

pub(super) fn is_stale_process_id_designator_in_manager(
    processes: &ProcessManager,
    value: &Value,
) -> bool {
    match process_value_to_id(value) {
        Some(id) if id > 0 => {
            processes.get(id).is_none()
                && (processes.get_any(id).is_some() || processes.was_issued_id(id))
        }
        _ => false,
    }
}

/// The same staleness, restricted to the kind GNU's TYPE check lets through.
///
/// GNU's "Process NAME is not active" is `p->infd < 0`, and in every subr that
/// raises it the type check comes FIRST: `process_send_signal` tests
/// `!EQ (p->type, Qreal)` at src/process.c:7084-7086 before `p->infd < 0` at
/// :7087-7089, and `Fprocess_running_child_p` does the same at :7042-7047.  A
/// network, serial or pipe process is never `Qreal`, so for those the type
/// check always wins -- "is not a subprocess", never "is not active" -- and
/// `stop-process`/`continue-process` do not reach either test, because they
/// handle those three kinds first and return the process (:7267-7278,
/// :7294-7315).
///
/// This port's analogue of `p->infd < 0` is "no longer in the live table", and
/// ledger 169 made that true at the retirement, which is where GNU puts it.
/// Answering it ahead of the type check made a retired `:stderr` pipe report
/// "is not active" inside its own sentinel where GNU reports "is not a
/// subprocess" -- six rows of the neighbour audit, measured.  So the guard is
/// asked only about the kind GNU would have let past.
///
/// An id in neither table cannot be asked its kind; it keeps the old answer.
pub(super) fn is_stale_real_process_designator_in_manager(
    processes: &ProcessManager,
    value: &Value,
) -> bool {
    is_stale_process_id_designator_in_manager(processes, value)
        && process_value_to_id(value)
            .and_then(|id| processes.get_any(id))
            .is_none_or(|proc| proc.kind == ProcessKind::Real)
}

pub(super) fn resolve_optional_process_or_current_buffer_in_state(
    processes: &ProcessManager,
    buffers: &BufferManager,
    value: Option<&Value>,
) -> Result<ProcessId, Flow> {
    if let Some(v) = value
        && !v.is_nil()
    {
        return resolve_get_process_designator_in_state(processes, buffers, v);
    }

    let current_buffer = buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    processes
        .find_by_buffer_id(current_buffer)
        .ok_or_else(|| signal_buffer_has_no_process(buffers, current_buffer))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(super) fn process_live_status_value(process: &Process) -> Value {
    if process_stopped_for_io(process) {
        return Value::list(vec![Value::symbol("stop")]);
    }
    // GNU decodes a pending child status at observation (`update_status`), so
    // a process whose exit has been reaped-but-not-yet-notified is already
    // dead to `process-live-p`.
    let status = process_effective_status(process);
    let kind = process.kind;
    match ProcessStatusSymbol::from_status_value(status) {
        Some(ProcessStatusSymbol::Run) => process_live_running_status_value(kind),
        Some(ProcessStatusSymbol::Stop) => Value::list(vec![Value::symbol("stop")]),
        Some(ProcessStatusSymbol::Open) => Value::list(vec![
            Value::symbol("open"),
            Value::symbol("listen"),
            Value::symbol("connect"),
            Value::symbol("stop"),
        ]),
        Some(ProcessStatusSymbol::Listen) => Value::list(vec![
            Value::symbol("listen"),
            Value::symbol("connect"),
            Value::symbol("stop"),
        ]),
        Some(ProcessStatusSymbol::Connect) => Value::list(vec![Value::symbol("connect")]),
        _ => Value::NIL,
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(super) fn process_live_running_status_value(kind: ProcessKind) -> Value {
    match kind {
        ProcessKind::Network => Value::list(vec![
            Value::symbol("listen"),
            Value::symbol("connect"),
            Value::symbol("stop"),
        ]),
        ProcessKind::Pipe => Value::list(vec![
            Value::symbol("open"),
            Value::symbol("listen"),
            Value::symbol("connect"),
            Value::symbol("stop"),
        ]),
        _ => Value::list(vec![
            Value::symbol("run"),
            Value::symbol("open"),
            Value::symbol("listen"),
            Value::symbol("connect"),
            Value::symbol("stop"),
        ]),
    }
}

/// GNU `update_status` view of a process: a pending-but-unnotified child
/// status (`raw_status_new` in GNU, `status_notify_pending` +
/// `pending_status` here) is DECODED at observation points --
/// `Fprocess_status` and `Fprocess_exit_status` both run `update_status`
/// before reading -- while the sentinel notification stays pending for the
/// wait loop's `status_notify` pass.
pub(super) fn process_effective_status(process: &Process) -> Value {
    if process.status_notify_pending && !process.pending_status.is_nil() {
        process.pending_status
    } else {
        process.status
    }
}

/// GNU `Fprocess_status`'s connection remapping (src/process.c:1193-1201):
///
/// ```c
///   if (NETCONN1_P (p) || SERIALCONN1_P (p) || PIPECONN1_P (p))
///     {
///       if (EQ (status, Qexit))          status = Qclosed;   /* :1195-1196 */
///       else if (EQ (p->command, Qt))    status = Qstop;     /* :1197-1198 */
///       else if (EQ (status, Qrun))      status = Qopen;     /* :1199-1200 */
///     }
/// ```
///
/// The chain is an `else if`, so `exit -> closed` WINS over the
/// `command == t` stop: a connection that has finished reports `closed`
/// however many times `stop-process` was called on it.  This port answered
/// `command == t` first, which reported `stop` for a `:stderr` pipe that had
/// already closed -- the last divergent row of ledger 169's three-kind
/// neighbour sweep, and reachable only once `stop-process` started setting
/// `p->command' on a retired connection the way GNU does.
pub(super) fn process_public_status_symbol(process: &Process) -> Value {
    if process_stopped_for_io(process)
        && !matches!(
            ProcessStatusSymbol::from_status_value(process_effective_status(process)),
            Some(
                ProcessStatusSymbol::Exit
                    | ProcessStatusSymbol::Signal
                    | ProcessStatusSymbol::Closed
            )
        )
    {
        return ProcessStatusSymbol::Stop.value();
    }
    match ProcessStatusSymbol::from_status_value(process_effective_status(process)) {
        Some(ProcessStatusSymbol::Run) => match process.kind {
            ProcessKind::Network => {
                if process_contact_server_p(process) {
                    Value::symbol("listen")
                } else {
                    Value::symbol("open")
                }
            }
            ProcessKind::Pipe => Value::symbol("open"),
            _ => Value::symbol("run"),
        },
        Some(ProcessStatusSymbol::Stop) => ProcessStatusSymbol::Stop.value(),
        Some(ProcessStatusSymbol::Exit) => match process.kind {
            ProcessKind::Real => ProcessStatusSymbol::Exit.value(),
            _ => ProcessStatusSymbol::Closed.value(),
        },
        Some(ProcessStatusSymbol::Signal) => match process.kind {
            ProcessKind::Real => Value::symbol("signal"),
            _ => Value::symbol("closed"),
        },
        Some(ProcessStatusSymbol::Open) => ProcessStatusSymbol::Open.value(),
        Some(ProcessStatusSymbol::Listen) => ProcessStatusSymbol::Listen.value(),
        Some(ProcessStatusSymbol::Closed) => ProcessStatusSymbol::Closed.value(),
        Some(ProcessStatusSymbol::Connect) => ProcessStatusSymbol::Connect.value(),
        Some(ProcessStatusSymbol::Failed) => ProcessStatusSymbol::Failed.value(),
        _ => Value::NIL,
    }
}

pub(super) fn default_process_tty_name() -> String {
    // Fallback TTY name when the actual PTY slave path is not available.
    "/dev/pts/0".to_string()
}
