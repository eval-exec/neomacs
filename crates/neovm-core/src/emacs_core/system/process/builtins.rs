//! Process builtins: the `builtin_*` subr handlers for process creation, signals, I/O, and status, both the eval-dependent set and the pure (no-evaluator) tail (GNU src/process.c DEFUNs).
//!
//! Moved out of `mod.rs` unchanged; a child module so it keeps the
//! parent's view of its private items (`use super::*`).

use super::*;

/// (internal-default-interrupt-process &optional PROCESS CURRENT-GROUP) -> process-or-nil
pub(crate) fn builtin_internal_default_interrupt_process(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_internal_default_interrupt_process_impl(&mut eval.processes, &eval.buffers, args)
}

pub(crate) fn builtin_internal_default_interrupt_process_impl(
    processes: &mut ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("internal-default-interrupt-process"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let (id, ret) =
        resolve_optional_process_with_explicit_return_in_state(processes, buffers, args.first())?;
    check_process_is_real_subprocess(processes, id)?;
    if let Some(proc) = processes.get_mut(id) {
        #[cfg(unix)]
        let _ = signal_process_or_unbacked_success(
            proc,
            libc::SIGINT,
            ProcessSignalRecipient::ProcessGroup,
        );
    }
    Ok(ret)
}

/// (internal-default-signal-process PROCESS SIGNAL &optional CURRENT-GROUP) -> int-or-nil
pub(crate) fn builtin_internal_default_signal_process(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_internal_default_signal_process_impl(&mut eval.processes, &eval.buffers, args)
}

pub(crate) fn builtin_internal_default_signal_process_impl(
    processes: &mut ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("internal-default-signal-process", &args, 2)?;
    if args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("internal-default-signal-process"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let signal_num = parse_signal_number(&args[1])?;
    match resolve_signal_process_target_in_state(processes, buffers, args.first())? {
        SignalProcessTarget::Process(id) => {
            // `get_any_mut`, not `get_mut`: GNU's `CHECK_PROCESS` +
            // `XPROCESS (process)->pid` (src/process.c:7379-7382) does not ask
            // whether the process is still in `Vprocess_alist`.
            if let Some(proc) = processes.get_any_mut(id) {
                if proc.kind != ProcessKind::Real {
                    return Err(signal_cannot_signal_process(proc));
                }
                return Ok(Value::fixnum(signal_process_or_unbacked_success(
                    proc,
                    signal_num,
                    ProcessSignalRecipient::ImmediateProcess,
                ) as i64));
            }
            Ok(Value::fixnum(-1))
        }
        SignalProcessTarget::MissingNamedProcess => Ok(Value::NIL),
        SignalProcessTarget::Pid(pid) => {
            Ok(Value::fixnum(sys::send_signal(pid, signal_num) as i64))
        }
    }
}

pub(super) fn process_mark_insert_emacs_byte_pos(
    buffers: &BufferManager,
    buf_id: BufferId,
    mark: Value,
) -> EmacsBytePos {
    match super::super::marker::marker_position_as_int_with_buffers(buffers, &mark) {
        Ok(pos) => buffers
            .get(buf_id)
            .map(|b| b.lisp_pos_to_full_buffer_emacs_byte_pos(LispCharPos1::new(pos)))
            .unwrap_or(EmacsBytePos::ZERO),
        Err(_) => buffers
            .get(buf_id)
            .map(|b| b.full_emacs_byte_range().end())
            .unwrap_or(EmacsBytePos::ZERO),
    }
}

pub(super) fn adjusted_process_output_point(
    old_point: EmacsBytePos,
    insert_pos: EmacsBytePos,
    inserted_len: EmacsByteLen,
) -> EmacsBytePos {
    if old_point >= insert_pos {
        old_point.add_len(inserted_len)
    } else {
        old_point
    }
}

/// The two GNU-owned writes performed by the default process callbacks.
///
/// The variants deliberately encode the observable current-buffer contract:
/// calling `internal-default-process-filter` directly leaves the process
/// buffer current, while the default sentinel restores its caller's buffer.
/// Keeping that distinction in an enum makes adding a third, accidentally
/// ambiguous write policy a compile-time decision instead of another boolean.
pub(super) enum DefaultProcessBufferInsertion<'a> {
    Output(&'a LispString),
    StatusMessage(&'a str),
}

impl DefaultProcessBufferInsertion<'_> {
    pub(super) fn restores_callers_current_buffer(&self) -> bool {
        matches!(self, Self::StatusMessage(_))
    }

    pub(super) fn fallback_byte_len(&self) -> usize {
        match self {
            Self::Output(text) => text.sbytes(),
            Self::StatusMessage(text) => text.len(),
        }
    }
}

/// Insert one default process callback payload at the process marker.
///
/// This is the NeoVM counterpart of GNU's process-buffer insertion state
/// machine in `read_process_output_before_insert` /
/// `read_process_output_after_insert` and
/// `internal-default-process-sentinel` (`src/process.c`).  The module owns the
/// target-buffer switch, read-only override, point adjustment, marker advance,
/// and callback-specific current-buffer restoration as one deep operation.
pub(super) fn insert_default_process_buffer_payload(
    eval: &mut super::super::eval::Context,
    id: ProcessId,
    insertion: DefaultProcessBufferInsertion<'_>,
) -> EvalResult {
    let (buffer, mark) = match eval.processes.get_any(id) {
        Some(process) => (process.buffer, process.mark),
        None => return Ok(Value::NIL),
    };
    let Some(buffer_id) = buffer.as_buffer_id() else {
        return Ok(Value::NIL);
    };
    if eval.buffers.get(buffer_id).is_none() {
        return Ok(Value::NIL);
    }

    let saved_current = insertion
        .restores_callers_current_buffer()
        .then(|| eval.buffers.current_buffer_id())
        .flatten();
    eval.set_current_buffer_unrecorded(buffer_id)?;

    let insert_pos = process_mark_insert_emacs_byte_pos(&eval.buffers, buffer_id, mark);
    let saved_point = eval
        .buffers
        .get(buffer_id)
        .map(|buffer| buffer.point_emacs_byte_pos());
    let saved_read_only = eval
        .buffers
        .get(buffer_id)
        .map(|buffer| buffer.get_read_only());

    if let Some(buffer) = eval.buffers.get_mut(buffer_id) {
        buffer.set_read_only_value(false);
    }
    let _ = eval
        .buffers
        .goto_buffer_emacs_byte_pos(buffer_id, insert_pos);

    let fallback_byte_len = insertion.fallback_byte_len();
    match insertion {
        DefaultProcessBufferInsertion::Output(text) => {
            let change = super::super::editfns::text_change_for_empty_insertion_at_emacs_byte_pos(
                &eval.buffers,
                buffer_id,
                insert_pos,
                super::super::editfns::lisp_string_text_extent(text),
            )?;
            super::super::editfns::signal_before_text_change(eval, change)?;
            eval.buffers
                .insert_lisp_string_into_buffer_before_markers(buffer_id, text);
            super::super::editfns::signal_after_text_change(eval, change)?;
        }
        DefaultProcessBufferInsertion::StatusMessage(text) => {
            let _ = eval
                .buffers
                .insert_into_buffer_before_markers(buffer_id, text);
        }
    }

    let new_mark = eval
        .buffers
        .get(buffer_id)
        .map(|buffer| buffer.point_emacs_byte_pos())
        .unwrap_or(insert_pos.add_len(EmacsByteLen::new(fallback_byte_len)));

    if let (Some(buffer), Some(read_only)) = (eval.buffers.get_mut(buffer_id), saved_read_only) {
        buffer.set_read_only_value(read_only);
    }

    let inserted_len = EmacsByteLen::new(new_mark.get().saturating_sub(insert_pos.get()));
    if let Some(old_point) = saved_point {
        let adjusted_point = adjusted_process_output_point(old_point, insert_pos, inserted_len);
        let _ = eval
            .buffers
            .goto_buffer_emacs_byte_pos(buffer_id, adjusted_point);
    }

    if let Some(process) = eval.processes.get_any_mut(id) {
        let new_mark_pos = eval
            .buffers
            .get(buffer_id)
            .map(|buffer| Value::fixnum(buffer.emacs_byte_pos_to_lisp_char_pos(new_mark).as_i64()))
            .unwrap_or(Value::NIL);
        super::super::marker::builtin_set_marker_in_buffers(
            &mut eval.buffers,
            vec![process.mark, new_mark_pos, process.buffer],
        )?;
    }

    if let Some(saved_buffer_id) = saved_current {
        eval.restore_current_buffer_if_live(saved_buffer_id);
    }

    Ok(Value::NIL)
}

/// (internal-default-process-filter PROCESS STRING) -> nil
///
/// When no custom filter is set, insert output into the process's associated
/// buffer at the process mark position (or end of buffer when mark is None).
/// This matches GNU Emacs's `internal-default-process-filter` behavior.
pub(crate) fn builtin_internal_default_process_filter(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("internal-default-process-filter", &args, 2)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(&eval.processes, &args[0])?;
    let text = match args[1].as_lisp_string() {
        Some(text) => text.clone(),
        None => return Err(signal_wrong_type_string(args[1])),
    };
    if text.is_empty() {
        return Ok(Value::NIL);
    }

    // GNU `read_process_output_before_insert` (src/process.c) makes the PROCESS
    // buffer current before touching anything: `Fset_buffer (p->buffer)`. Every
    // check the insertion then runs -- above all the read-only barf in
    // `prepare_to_modify_buffer` -- therefore tests the buffer being written, not
    // whatever buffer happened to be current when output arrived.
    //
    // Without it, a filter called while an unrelated read-only buffer is current
    // signals `buffer-read-only' for THAT buffer: magit-blame calls this filter
    // with the blamed (read-only) source buffer current, so every blame chunk
    // errored and no blame information ever appeared (neomacs#192).
    //
    // GNU does not restore the current buffer here either; its caller
    // (`read_process_output`) unwinds it, which
    // `run_async_process_callback_preserving_state` mirrors.
    insert_default_process_buffer_payload(eval, id, DefaultProcessBufferInsertion::Output(&text))
}

/// (internal-default-process-sentinel PROCESS STRING) -> nil
pub(crate) fn builtin_internal_default_process_sentinel(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("internal-default-process-sentinel", &args, 2)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(&eval.processes, &args[0])?;
    let msg = expect_string_strict(&args[1])?;

    let (name, status_symbol) = match eval.processes.get_any(id) {
        Some(process) => (
            process_name_runtime(process.name),
            ProcessStatusSymbol::from_status_value(process.status),
        ),
        None => return Err(signal_wrong_type_processp(args[0])),
    };

    if status_symbol == Some(ProcessStatusSymbol::Run) {
        return Ok(Value::NIL);
    }

    let text = format!("\nProcess {name} {msg}");
    insert_default_process_buffer_payload(
        eval,
        id,
        DefaultProcessBufferInsertion::StatusMessage(&text),
    )
}

/// (gnutls-boot PROCESS TYPE PROPLIST) -> t or error
///
/// Upgrade a network process to TLS through the GNU-compatible `gnutls-boot` API.
/// PROCESS must be a network process with an open TCP socket.
/// TYPE is the credential type.  PROPLIST is a keyword plist; `:hostname`
/// supplies SNI and certificate hostname validation, while `:trustfiles`
/// supplies additional PEM trust anchors.
pub(crate) fn builtin_gnutls_boot(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("gnutls-boot", &args, 3)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(&eval.processes, &args[0])?;
    let parameters = parse_gnutls_boot_parameters(args[1], args[2])?;
    upgrade_process_to_tls::<RustlsBackend>(
        &mut eval.processes,
        id,
        &parameters.client,
        "gnutls-boot",
        signal_gnutls_boot_error,
    )?;

    Ok(Value::T)
}

pub(crate) fn builtin_gnutls_asynchronous_parameters(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("gnutls-asynchronous-parameters", &args, 2)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(&eval.processes, &args[0])?;
    let proc = eval
        .processes
        .get_mut(id)
        .ok_or_else(|| signal("error", vec![Value::string("Process not found")]))?;
    proc.gnutls_boot_parameters = args[1];
    Ok(Value::NIL)
}

pub(crate) fn builtin_gnutls_bye(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("gnutls-bye", &args, 2)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(&eval.processes, &args[0])?;
    let proc = eval
        .processes
        .get_mut(id)
        .ok_or_else(|| signal("error", vec![Value::string("Process not found")]))?;
    let Some(tls_stream) = proc.live_io.tls_stream.as_mut() else {
        return Ok(Value::NIL);
    };
    match tls_stream.send_close_notify(args[1].is_nil()) {
        Ok(result) => Ok(gnutls_close_notify_result_value(result)),
        Err(err) => Err(signal_process_io("gnutls-bye", None, err)),
    }
}

pub(crate) fn builtin_gnutls_deinit(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("gnutls-deinit", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(&eval.processes, &args[0])?;
    let proc = eval
        .processes
        .get_mut(id)
        .ok_or_else(|| signal("error", vec![Value::string("Process not found")]))?;
    if proc.live_io.tls_stream.take().is_some() {
        proc.gnutls_initstage = GnutlsInitStage::Callbacks;
        proc.gnutls_boot_parameters = Value::NIL;
        Ok(Value::T)
    } else {
        Ok(Value::NIL)
    }
}

pub(crate) fn builtin_gnutls_get_initstage(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("gnutls-get-initstage", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(&eval.processes, &args[0])?;
    let proc = eval
        .processes
        .get(id)
        .ok_or_else(|| signal("error", vec![Value::string("Process not found")]))?;
    Ok(Value::fixnum(i64::from(proc.gnutls_initstage)))
}

pub(crate) fn builtin_gnutls_peer_status(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("gnutls-peer-status", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(&eval.processes, &args[0])?;
    let proc = eval
        .processes
        .get(id)
        .ok_or_else(|| signal("error", vec![Value::string("Process not found")]))?;
    if proc.gnutls_initstage == GnutlsInitStage::Ready {
        Ok(proc
            .live_io
            .tls_stream
            .as_ref()
            .map(|tls| gnutls_peer_status_to_value(&tls.peer_status()))
            .unwrap_or(Value::NIL))
    } else {
        Ok(Value::NIL)
    }
}

/// (neomacs-open-tls-stream NAME BUFFER HOST PORT) -> process
///
/// Open a TCP network process and immediately upgrade it through Neomacs'
/// native TLS backend. This is intentionally separate from GNU's `gnutls-*`
/// API: rustls provides TLS transport, not libgnutls semantics.
pub(crate) fn builtin_neomacs_open_tls_stream(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("neomacs-open-tls-stream", &args, 4)?;
    let host = expect_string_strict(&args[2])?;
    let process = builtin_make_network_process(
        eval,
        vec![
            Value::keyword(":name"),
            args[0],
            Value::keyword(":buffer"),
            args[1],
            Value::keyword(":host"),
            args[2],
            Value::keyword(":service"),
            args[3],
        ],
    )?;
    let id = resolve_process_or_wrong_type_any_in_manager(&eval.processes, &process)?;
    let parameters = TlsClientParameters::default_roots(host);
    upgrade_process_to_tls::<RustlsBackend>(
        &mut eval.processes,
        id,
        &parameters,
        "neomacs-open-tls-stream",
        signal_neomacs_tls_error,
    )?;
    Ok(process)
}

pub(super) fn upgrade_process_to_tls<B: TlsClientBackend>(
    processes: &mut ProcessManager,
    id: ProcessId,
    parameters: &TlsClientParameters,
    operation: &str,
    map_error: fn(TlsBackendError) -> Flow,
) -> Result<(), Flow> {
    let proc = processes
        .get_mut(id)
        .ok_or_else(|| signal("error", vec![Value::string("Process not found")]))?;

    if proc.kind != ProcessKind::Network {
        return Err(signal(
            "error",
            vec![Value::string(format!("{operation}: not a network process"))],
        ));
    }

    // Take the plain TCP stream; it will be owned by the TLS stream.
    let tcp_stream = match proc.live_io.network_socket.take() {
        Some(NetworkSocket::TcpStream(stream)) => stream,
        Some(other) => {
            proc.live_io.network_socket = Some(other);
            return Err(signal(
                "error",
                vec![Value::string(format!(
                    "{operation}: process is not a TCP stream"
                ))],
            ));
        }
        None => {
            return Err(signal(
                "error",
                vec![Value::string(format!(
                    "{operation}: no socket (already TLS or closed)"
                ))],
            ));
        }
    };

    proc.gnutls_initstage = GnutlsInitStage::HandshakeTried;
    let tls_stream = B::connect_client(tcp_stream, parameters).map_err(map_error)?;

    // Store the TLS stream. The poller still watches the underlying fd
    // (which is the same fd that was registered for the plain socket).
    proc.live_io.tls_stream = Some(tls_stream);
    proc.gnutls_initstage = GnutlsInitStage::Ready;
    proc.gnutls_boot_parameters = Value::NIL;

    Ok(())
}

pub(super) fn signal_gnutls_boot_error(err: TlsBackendError) -> Flow {
    match err {
        TlsBackendError::InvalidHostname(_)
        | TlsBackendError::TrustFile { .. }
        | TlsBackendError::Connect(_) => signal("error", vec![Value::string(err.to_string())]),
        TlsBackendError::UnexpectedEof => signal(
            "gnutls-error",
            vec![
                Value::fixnum(-1),
                Value::string("TLS handshake: unexpected EOF"),
            ],
        ),
        TlsBackendError::Io(err) => signal(
            "gnutls-error",
            vec![
                Value::fixnum(-1),
                Value::string(format!("TLS handshake: {}", err)),
            ],
        ),
    }
}

pub(super) fn signal_neomacs_tls_error(err: TlsBackendError) -> Flow {
    signal("error", vec![Value::string(err.to_string())])
}

/// (isearch-process-search-string STRING MESSAGE) -> nil
/// (minibuffer--sort-preprocess-history HISTORY) -> nil
/// (print--preprocess OBJECT) -> nil
///
/// Extracts sharing info from OBJECT needed to print it: fills the
/// `print-number-table` hash when `print-circle' is non-nil, and does nothing
/// otherwise.  Mirrors GNU `Fprint_preprocess` (src/print.c).
pub(crate) fn builtin_print_preprocess(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("print--preprocess", &args, 1)?;
    let object = args[0];

    // GNU: does nothing if `print-circle' is nil.
    let print_circle = eval
        .obarray
        .symbol_value("print-circle")
        .is_some_and(|v| v.is_truthy());
    if !print_circle {
        return Ok(Value::NIL);
    }

    let print_gensym = eval
        .obarray
        .symbol_value("print-gensym")
        .is_some_and(|v| v.is_truthy());
    let print_continuous_numbering = eval
        .obarray
        .symbol_value("print-continuous-numbering")
        .is_some_and(|v| v.is_truthy());

    // GNU: `if (!HASH_TABLE_P (Vprint_number_table)) Vprint_number_table = make-hash-table :test eq`.
    let table_value = match eval.obarray.symbol_value("print-number-table") {
        Some(v) if v.is_hash_table() => *v,
        _ => {
            let table = Value::hash_table(super::super::value::HashTableTest::Eq);
            eval.set_variable("print-number-table", table);
            table
        }
    };

    // Root the object and table across the (allocation-heavy) traversal.
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(object);
    eval.push_specpdl_root(table_value);
    super::super::print::preprocess_print_number_table(
        &object,
        table_value,
        print_gensym,
        print_continuous_numbering,
    );
    eval.restore_specpdl_roots(roots);

    Ok(Value::NIL)
}

/// (syntax-propertize--in-process-p) -> nil
/// (window--adjust-process-windows) -> nil
/// (window--process-window-list) -> nil
/// (window-adjust-process-window-size PROCESS WINDOW) -> nil
/// (window-adjust-process-window-size-largest PROCESS WINDOW) -> nil
/// (window-adjust-process-window-size-smallest PROCESS WINDOW) -> nil
/// (format-network-address ADDRESS &optional OMIT-PORT) -> string-or-nil
pub(crate) fn builtin_format_network_address(
    _eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_format_network_address_impl(args)
}

pub(crate) fn builtin_format_network_address_impl(args: Vec<Value>) -> EvalResult {
    expect_min_args("format-network-address", &args, 1)?;
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("format-network-address"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let omit_port = args.get(1).is_some_and(|v| v.is_truthy());
    match args[0].kind() {
        ValueKind::String => Ok(args[0]),
        ValueKind::Nil => Ok(Value::NIL),
        ValueKind::Veclike(VecLikeType::Vector) => {
            let Some(items) = vector_nonnegative_integers(&args[0]) else {
                return Ok(Value::NIL);
            };
            if let Some(ipv4) = format_ipv4_network_address(&items, omit_port) {
                return Ok(Value::string(ipv4));
            }
            if let Some(ipv6) = format_ipv6_network_address(&items, omit_port) {
                return Ok(Value::string(ipv6));
            }
            Ok(Value::NIL)
        }
        ValueKind::Cons => {
            if let ValueKind::Fixnum(family) = args[0].cons_car().kind() {
                return Ok(Value::string(format!("<Family {family}>")));
            }
            Err(signal(
                "error",
                vec![Value::string(
                    "Format specifier doesn't match argument type",
                )],
            ))
        }
        _ => Ok(Value::NIL),
    }
}

/// (network-interface-list &optional FULL FAMILY) -> interface-list
pub(crate) fn builtin_network_interface_list(
    _eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_network_interface_list_impl(args)
}

pub(crate) fn builtin_network_interface_list_impl(args: Vec<Value>) -> EvalResult {
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("network-interface-list"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let full = args.first().is_some_and(|v| v.is_truthy());
    let family = args.get(1).cloned().unwrap_or(Value::NIL);
    let requested_family = if family.is_nil() {
        None
    } else {
        Some(
            NetworkAddressFamily::from_symbol_value(&family).ok_or_else(|| {
                signal("error", vec![Value::string("Unsupported address family")])
            })?,
        )
    };
    let include_ipv4 = requested_family.is_none_or(|family| family == NetworkAddressFamily::Ipv4);
    let include_ipv6 = requested_family.is_none_or(|family| family == NetworkAddressFamily::Ipv6);

    let mut entries = Vec::new();
    if let Some(host_entries) = sys::interface_snapshot() {
        for entry in host_entries.into_iter().rev() {
            let include = match entry.family {
                NetworkAddressFamily::Ipv4 => include_ipv4,
                NetworkAddressFamily::Ipv6 => include_ipv6,
            };
            if !include {
                continue;
            }

            if full {
                entries.push(Value::list(vec![
                    Value::string(entry.name),
                    entry.address,
                    entry.list_broadcast,
                    entry.netmask,
                ]));
            } else {
                entries.push(Value::cons(Value::string(entry.name), entry.address));
            }
        }
    }

    if entries.is_empty() {
        if include_ipv6 {
            entries.push(interface_entry("lo", loopback_ipv6_address(), full));
        }
        if include_ipv4 {
            entries.push(interface_entry("lo", loopback_ipv4_address(), full));
        }
    }
    Ok(Value::list(entries))
}

/// (network-interface-info IFNAME) -> interface-info-or-nil
pub(crate) fn builtin_network_interface_info(
    _eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_network_interface_info_impl(args)
}

pub(crate) fn builtin_network_interface_info_impl(args: Vec<Value>) -> EvalResult {
    expect_args("network-interface-info", &args, 1)?;
    let ifname_raw = expect_string_strict(&args[0])?;
    // Match C-string interface-name handling: embedded NUL truncates lookup.
    let ifname = ifname_raw.split('\0').next().unwrap_or_default();
    // Emacs applies IFNAMSIZ-style byte limits, not character counts.
    if ifname.len() >= 16 {
        return Err(signal(
            "error",
            vec![Value::string("interface name too long")],
        ));
    }

    if let Some(host_entries) = sys::interface_snapshot() {
        let mut first_match: Option<HostInterfaceEntry> = None;
        let mut ipv4_match: Option<HostInterfaceEntry> = None;

        for entry in host_entries {
            if entry.name != ifname {
                continue;
            }
            if first_match.is_none() {
                first_match = Some(entry.clone());
            }
            if entry.family == NetworkAddressFamily::Ipv4 {
                ipv4_match = Some(entry);
                break;
            }
        }

        if let Some(entry) = ipv4_match.or(first_match) {
            return Ok(Value::list(vec![
                entry.address,
                entry.info_broadcast,
                entry.netmask,
                entry.hwaddr.unwrap_or(Value::NIL),
                entry.flags,
            ]));
        }
    }

    if ifname == "lo" {
        return Ok(Value::list(vec![
            loopback_ipv4_address(),
            loopback_ipv4_broadcast(),
            loopback_ipv4_netmask(),
            loopback_hwaddr(),
            loopback_flags(),
        ]));
    }

    Ok(Value::NIL)
}

/// (network-lookup-address-info NAME &optional FAMILY HINTS) -> address-list
pub(crate) fn builtin_network_lookup_address_info(
    _eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_network_lookup_address_info_impl(args)
}

pub(crate) fn builtin_network_lookup_address_info_impl(args: Vec<Value>) -> EvalResult {
    expect_min_args("network-lookup-address-info", &args, 1)?;
    if args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("network-lookup-address-info"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let name = expect_network_lookup_hostname(&args[0])?;

    let family = args.get(1).cloned().unwrap_or(Value::NIL);
    let hint_value = args.get(2).cloned().unwrap_or(Value::NIL);

    let lookup_family = if family.is_nil() {
        None
    } else {
        Some(
            NetworkAddressFamily::from_symbol_value(&family)
                .ok_or_else(|| signal("error", vec![Value::string("Unsupported family")]))?,
        )
    };
    let lookup_hint = if hint_value.is_nil() {
        None
    } else {
        Some(
            NetworkLookupHint::from_symbol_value(&hint_value)
                .ok_or_else(|| signal("error", vec![Value::string("Unsupported hints value")]))?,
        )
    };
    let entries = resolve_network_lookup_addresses(&name, lookup_family, lookup_hint);
    Ok(Value::list(entries))
}

/// (signal-names) -> list-of-signal-name-strings
pub(crate) fn builtin_signal_names(
    _eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_signal_names_impl(args)
}

pub(crate) fn builtin_signal_names_impl(args: Vec<Value>) -> EvalResult {
    expect_args("signal-names", &args, 0)?;
    let names = vec![
        "RTMAX", "RTMAX-1", "RTMAX-2", "RTMAX-3", "RTMAX-4", "RTMAX-5", "RTMAX-6", "RTMAX-7",
        "RTMAX-8", "RTMAX-9", "RTMAX-10", "RTMAX-11", "RTMAX-12", "RTMAX-13", "RTMAX-14",
        "RTMIN+15", "RTMIN+14", "RTMIN+13", "RTMIN+12", "RTMIN+11", "RTMIN+10", "RTMIN+9",
        "RTMIN+8", "RTMIN+7", "RTMIN+6", "RTMIN+5", "RTMIN+4", "RTMIN+3", "RTMIN+2", "RTMIN+1",
        "RTMIN", "SYS", "PWR", "POLL", "WINCH", "PROF", "VTALRM", "XFSZ", "XCPU", "URG", "TTOU",
        "TTIN", "TSTP", "STOP", "CONT", "CHLD", "STKFLT", "TERM", "ALRM", "PIPE", "USR2", "SEGV",
        "USR1", "KILL", "FPE", "BUS", "ABRT", "TRAP", "ILL", "QUIT", "INT", "HUP", "EXIT",
    ];
    Ok(Value::list(
        names.into_iter().map(Value::string).collect::<Vec<_>>(),
    ))
}

/// (list-system-processes) -> process-id-list
pub(crate) fn builtin_list_system_processes(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("list-system-processes", &args, 0)?;
    if let Some(default_directory) = visible_default_directory_lisp(eval) {
        let operation = Value::symbol("list-system-processes");
        let handler = super::super::fileio::find_file_name_handler_lisp_for_eval(
            eval,
            &default_directory,
            operation,
        );
        if !handler.is_nil() {
            return eval.funcall_general(handler, vec![operation]);
        }
    }
    builtin_list_system_processes_impl(args)
}

pub(crate) fn builtin_list_system_processes_impl(args: Vec<Value>) -> EvalResult {
    expect_args("list-system-processes", &args, 0)?;

    let mut pids = sys::list_process_ids();
    pids.sort_unstable();
    Ok(Value::list(pids.into_iter().map(Value::fixnum).collect()))
}

/// (num-processors &optional QUERY) -> integer
pub(crate) fn builtin_num_processors(
    _eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_num_processors_impl(args)
}

pub(crate) fn builtin_num_processors_impl(args: Vec<Value>) -> EvalResult {
    if args.len() > 1 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("num-processors"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let query = args.first().and_then(NumProcessorsQuery::from_symbol_value);
    Ok(Value::fixnum(num_processors_count(query) as i64))
}

pub(super) fn num_processors_count(query: Option<NumProcessorsQuery>) -> u64 {
    match query {
        Some(NumProcessorsQuery::All) => all_processors_count(),
        Some(NumProcessorsQuery::Current) => current_processors_count(),
        None => current_processors_count_overridable(),
    }
}

#[cfg(unix)]
pub(super) fn current_processors_count_overridable() -> u64 {
    let omp_threads = std::env::var_os("OMP_NUM_THREADS");
    let omp_limit = std::env::var_os("OMP_THREAD_LIMIT");
    current_processors_count_overridable_with_env(
        omp_threads.as_deref().map(OsStrExt::as_bytes),
        omp_limit.as_deref().map(OsStrExt::as_bytes),
        current_processors_count(),
    )
}

#[cfg(not(unix))]
pub(super) fn current_processors_count_overridable() -> u64 {
    let omp_threads = std::env::var("OMP_NUM_THREADS").ok();
    let omp_limit = std::env::var("OMP_THREAD_LIMIT").ok();
    current_processors_count_overridable_with_env(
        omp_threads.as_deref().map(str::as_bytes),
        omp_limit.as_deref().map(str::as_bytes),
        current_processors_count(),
    )
}

pub(super) fn current_processors_count_overridable_with_env(
    omp_threads: Option<&[u8]>,
    omp_limit: Option<&[u8]>,
    current_count: u64,
) -> u64 {
    let omp_threads = omp_threads.and_then(parse_openmp_threads).unwrap_or(0);
    let mut omp_limit = omp_limit.and_then(parse_openmp_threads).unwrap_or(u64::MAX);
    if omp_limit == 0 {
        omp_limit = u64::MAX;
    }

    if omp_threads != 0 {
        return omp_threads.min(omp_limit);
    }

    current_count.min(omp_limit).max(1)
}

pub(super) fn parse_openmp_threads(bytes: &[u8]) -> Option<u64> {
    let mut idx = 0;
    while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
        idx += 1;
    }
    if idx == bytes.len() || !bytes[idx].is_ascii_digit() {
        return None;
    }

    let start = idx;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    let value = std::str::from_utf8(&bytes[start..idx])
        .ok()?
        .parse::<u64>()
        .ok()?;

    while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
        idx += 1;
    }

    if idx == bytes.len() || bytes[idx] == b',' {
        Some(value)
    } else {
        None
    }
}

pub(super) fn current_processors_count() -> u64 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u64)
        .unwrap_or(1)
        .max(1)
}

pub(super) fn all_processors_count() -> u64 {
    let mut system = sysinfo::System::new();
    system.refresh_cpu_list(sysinfo::CpuRefreshKind::nothing());
    let count = system.cpus().len() as u64;
    if count == 0 {
        current_processors_count()
    } else {
        count
    }
}

/// (make-network-process &rest ARGS) -> process-or-nil
/// `make-network-process` with an explicit `:local` / `:remote` address
/// spec: binds or connects exactly the given inet or unix-domain address,
/// bypassing family/host/service resolution. Both arms return, so the
/// general resolution path below never runs for explicit addresses.
/// Extracted verbatim from builtin_make_network_process.
#[allow(clippy::too_many_arguments)]
pub(super) fn connect_network_process_at_explicit_address(
    eval: &mut super::super::eval::Context,
    explicit_address: Value,
    remote_address_value: Value,
    name: LispString,
    mut contact: Value,
    filter_val: Value,
    sentinel_val: Value,
    log_val: Value,
    resolved_coding: ProcessCodingSystems,
    buffer: Value,
    plist_val: Value,
    nowait: bool,
    server: bool,
    noquery: bool,
    stop: bool,
    server_backlog: Option<i32>,
    socket_type: NetworkSocketType,
    socket_options: Vec<NetworkSocketOptionSpec>,
    tls_parameters: Option<super::super::tls::GnutlsBootParameters>,
    tls_parameters_value: Value,
) -> EvalResult {
    let address = parse_network_address_spec(&explicit_address)?;
    match address {
        NetworkAddressSpec::Inet(addr) => {
            if socket_type == NetworkSocketType::Datagram {
                if server {
                    let effective_options = tcp_server_socket_options(&socket_options);
                    let socket = bind_udp_socket(addr, &effective_options)?;
                    let local_addr = socket.local_addr().map_err(|e| {
                        signal(
                            LispCondition::FileError,
                            vec![Value::string(format!("getsockname: {}", e))],
                        )
                    })?;
                    let zero_datagram = datagram_zero_address_for(local_addr);
                    let (datagram_socket_addr, datagram_address) = if !remote_address_value.is_nil()
                    {
                        match parse_network_address_spec(&remote_address_value)? {
                            NetworkAddressSpec::Inet(remote) => {
                                (Some(remote), socket_addr_to_lisp_value(remote))
                            }
                            #[cfg(unix)]
                            NetworkAddressSpec::Local(_) => (None, zero_datagram),
                        }
                    } else {
                        (None, zero_datagram)
                    };
                    contact = process_contact_plist_put(
                        contact,
                        ProcessKeyword::Service.value(),
                        Value::fixnum(local_addr.port() as i64),
                    )?;
                    contact = process_contact_plist_put(
                        contact,
                        ProcessKeyword::Local.value(),
                        socket_addr_to_lisp_value(local_addr),
                    )?;
                    contact = process_contact_plist_put(
                        contact,
                        ProcessKeyword::Remote.value(),
                        datagram_address,
                    )?;

                    let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
                    eval.processes.sync_process_mark(&mut eval.buffers, id)?;
                    if let Some(proc) = eval.processes.get_mut(id) {
                        proc.childp = contact;
                        proc.thread = current_thread_handle(&eval.threads);
                        proc.plist = plist_val;
                        proc.status = ProcessStatusSymbol::Open.value();
                        proc.live_io.network_socket = Some(NetworkSocket::UdpSocket(socket));
                        proc.datagram_socket_addr = datagram_socket_addr;
                        proc.datagram_address = datagram_address;
                        if !filter_val.is_nil() {
                            proc.filter = filter_val;
                            proc.childp = process_contact_plist_put(
                                proc.childp,
                                ProcessKeyword::Filter.value(),
                                proc.filter,
                            )?;
                        }
                        if !sentinel_val.is_nil() {
                            proc.sentinel = sentinel_val;
                            proc.childp = process_contact_plist_put(
                                proc.childp,
                                ProcessKeyword::Sentinel.value(),
                                proc.sentinel,
                            )?;
                        }
                        if !log_val.is_nil() {
                            proc.log = log_val;
                            proc.childp = process_contact_plist_put(
                                proc.childp,
                                ProcessKeyword::Log.value(),
                                proc.log,
                            )?;
                        }
                        if !buffer.is_nil() {
                            proc.childp = process_contact_plist_put(
                                proc.childp,
                                ProcessKeyword::Buffer.value(),
                                buffer,
                            )?;
                        }
                        apply_connection_process_flags(proc, noquery, stop);
                    }
                    eval.processes.register_socket_fd(id).ok();
                    return Ok(Value::make_process(id));
                }

                let socket = bind_udp_client_socket(addr, &socket_options)?;
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Remote.value(),
                    socket_addr_to_lisp_value(addr),
                )?;
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Local.value(),
                    socket_addr_to_lisp_value(udp_unspecified_addr_for(addr)),
                )?;

                let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
                eval.processes.sync_process_mark(&mut eval.buffers, id)?;
                if let Some(proc) = eval.processes.get_mut(id) {
                    proc.live_io.network_socket = Some(NetworkSocket::UdpSocket(socket));
                    proc.datagram_socket_addr = Some(addr);
                    proc.datagram_address = socket_addr_to_lisp_value(addr);
                    proc.status = ProcessStatusSymbol::Open.value();
                    proc.childp = contact;
                    proc.plist = plist_val;
                    proc.thread = current_thread_handle(&eval.threads);
                    if !filter_val.is_nil() {
                        proc.filter = filter_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Filter.value(),
                            proc.filter,
                        )?;
                    }
                    if !sentinel_val.is_nil() {
                        proc.sentinel = sentinel_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Sentinel.value(),
                            proc.sentinel,
                        )?;
                    }
                    if !buffer.is_nil() {
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Buffer.value(),
                            buffer,
                        )?;
                    }
                    apply_connection_process_flags(proc, noquery, stop);
                }
                eval.processes.register_socket_fd(id).ok();
                return Ok(Value::make_process(id));
            }

            #[cfg(unix)]
            if socket_type == NetworkSocketType::Seqpacket {
                return Err(signal(
                    "error",
                    vec![Value::string("Unsupported connection type")],
                ));
            }

            if server {
                let effective_options = tcp_server_socket_options(&socket_options);
                let listener = bind_tcp_listener_socket(
                    addr,
                    server_backlog.unwrap_or(5),
                    &effective_options,
                )?;
                let local_addr = listener.local_addr().map_err(|e| {
                    signal(
                        LispCondition::FileError,
                        vec![Value::string(format!("getsockname: {}", e))],
                    )
                })?;
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Service.value(),
                    Value::fixnum(local_addr.port() as i64),
                )?;
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Local.value(),
                    socket_addr_to_lisp_value(local_addr),
                )?;

                let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
                eval.processes.sync_process_mark(&mut eval.buffers, id)?;
                if let Some(proc) = eval.processes.get_mut(id) {
                    proc.childp = contact;
                    proc.thread = current_thread_handle(&eval.threads);
                    proc.plist = plist_val;
                    proc.live_io.network_socket = Some(NetworkSocket::TcpListener(listener));
                    if !filter_val.is_nil() {
                        proc.filter = filter_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Filter.value(),
                            proc.filter,
                        )?;
                    }
                    if !sentinel_val.is_nil() {
                        proc.sentinel = sentinel_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Sentinel.value(),
                            proc.sentinel,
                        )?;
                    }
                    if !log_val.is_nil() {
                        proc.log = log_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Log.value(),
                            proc.log,
                        )?;
                    }
                    if !buffer.is_nil() {
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Buffer.value(),
                            buffer,
                        )?;
                    }
                    apply_connection_process_flags(proc, noquery, stop);
                }
                eval.processes.register_socket_fd(id).ok();
                return Ok(Value::make_process(id));
            }

            if nowait {
                let start = start_pending_tcp_stream_connect(vec![addr], &socket_options)?;
                let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
                eval.processes.sync_process_mark(&mut eval.buffers, id)?;
                if let Some(proc) = eval.processes.get_mut(id) {
                    proc.status = process_status_connect_value();
                    proc.childp = contact;
                    proc.plist = plist_val;
                    proc.thread = current_thread_handle(&eval.threads);
                    if !filter_val.is_nil() {
                        proc.filter = filter_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Filter.value(),
                            proc.filter,
                        )?;
                    }
                    if !sentinel_val.is_nil() {
                        proc.sentinel = sentinel_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Sentinel.value(),
                            proc.sentinel,
                        )?;
                    }
                    if !buffer.is_nil() {
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Buffer.value(),
                            buffer,
                        )?;
                    }
                    match start {
                        PendingNetworkConnectStart::Started(started) => {
                            let local_addr = started.stream.local_addr().ok();
                            proc.live_io.network_socket =
                                Some(NetworkSocket::TcpStream(started.stream));
                            proc.live_io.pending_network_connect =
                                Some(PendingNetworkConnect::Tcp {
                                    remaining_addrs: started.remaining_addrs,
                                    socket_options: socket_options.clone(),
                                });
                            ProcessManager::update_tcp_client_contact(
                                proc,
                                started.remote_addr,
                                local_addr,
                            )?;
                        }
                        PendingNetworkConnectStart::Failed(code) => {
                            proc.status = process_status_failed_value(code);
                        }
                    }
                    if proc.live_io.pending_network_connect.is_some() {
                        proc.gnutls_boot_parameters = tls_parameters_value;
                    }
                    apply_connection_process_flags(proc, noquery, stop);
                }
                if eval
                    .processes
                    .get(id)
                    .is_some_and(|proc| proc.live_io.pending_network_connect.is_some())
                {
                    eval.processes.register_socket_writable_fd(id).ok();
                }
                return Ok(Value::make_process(id));
            }

            let stream = connect_tcp_stream_socket(addr, &socket_options, contact)?;
            let remote_addr = stream.peer_addr().ok();
            let local_addr = stream.local_addr().ok();

            let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
            eval.processes.sync_process_mark(&mut eval.buffers, id)?;
            if let Some(addr) = remote_addr {
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Remote.value(),
                    socket_addr_to_lisp_value(addr),
                )?;
            }
            if let Some(addr) = local_addr {
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Local.value(),
                    socket_addr_to_lisp_value(addr),
                )?;
            }
            if let Some(proc) = eval.processes.get_mut(id) {
                proc.live_io.network_socket = Some(NetworkSocket::TcpStream(stream));
                proc.status = process_status_run_value();
                proc.childp = contact;
                proc.plist = plist_val;
                proc.thread = current_thread_handle(&eval.threads);
                if !filter_val.is_nil() {
                    proc.filter = filter_val;
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Filter.value(),
                        proc.filter,
                    )?;
                }
                if !sentinel_val.is_nil() {
                    proc.sentinel = sentinel_val;
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Sentinel.value(),
                        proc.sentinel,
                    )?;
                }
                if !buffer.is_nil() {
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Buffer.value(),
                        buffer,
                    )?;
                }
                apply_connection_process_flags(proc, noquery, stop);
            }
            if let Some(parameters) = tls_parameters.clone() {
                upgrade_process_to_tls::<RustlsBackend>(
                    &mut eval.processes,
                    id,
                    &parameters.client,
                    "make-network-process",
                    signal_gnutls_boot_error,
                )?;
            }
            eval.processes.register_socket_fd(id).ok();
            // GNU fires NO sentinel for a synchronous (non-:nowait)
            // connect: `connect_network_socket` (process.c) sets the
            // status without `exec_sentinel`; only the deferred `:nowait`
            // completion path in `wait_reading_process_output` delivers
            // "open\n" / "failed with code N\n".
            Ok(Value::make_process(id))
        }
        #[cfg(unix)]
        NetworkAddressSpec::Local(path) => {
            if socket_type == NetworkSocketType::Datagram {
                let path_value =
                    Value::heap_string(crate::emacs_core::fileio::path_to_lisp_file_name(&path));
                if server {
                    let socket = bind_unix_datagram_socket(&path, &socket_options)?;
                    let zero_datagram = datagram_zero_unix_address();
                    let (datagram_unix_path, datagram_address) = if !remote_address_value.is_nil() {
                        match parse_network_address_spec(&remote_address_value)? {
                            NetworkAddressSpec::Local(remote_path) => {
                                let remote_value = Value::heap_string(
                                    crate::emacs_core::fileio::path_to_lisp_file_name(&remote_path),
                                );
                                (Some(remote_path), remote_value)
                            }
                            NetworkAddressSpec::Inet(_) => (None, zero_datagram),
                        }
                    } else {
                        (None, zero_datagram)
                    };
                    contact = process_contact_plist_put(
                        contact,
                        ProcessKeyword::Local.value(),
                        path_value,
                    )?;
                    contact = process_contact_plist_put(
                        contact,
                        ProcessKeyword::Remote.value(),
                        datagram_address,
                    )?;

                    let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
                    eval.processes.sync_process_mark(&mut eval.buffers, id)?;
                    if let Some(proc) = eval.processes.get_mut(id) {
                        proc.childp = contact;
                        proc.thread = current_thread_handle(&eval.threads);
                        proc.plist = plist_val;
                        proc.status = ProcessStatusSymbol::Open.value();
                        proc.live_io.network_socket = Some(NetworkSocket::UnixDatagram(socket));
                        proc.datagram_address = datagram_address;
                        proc.datagram_unix_path = datagram_unix_path;
                        if !filter_val.is_nil() {
                            proc.filter = filter_val;
                            proc.childp = process_contact_plist_put(
                                proc.childp,
                                ProcessKeyword::Filter.value(),
                                proc.filter,
                            )?;
                        }
                        if !sentinel_val.is_nil() {
                            proc.sentinel = sentinel_val;
                            proc.childp = process_contact_plist_put(
                                proc.childp,
                                ProcessKeyword::Sentinel.value(),
                                proc.sentinel,
                            )?;
                        }
                        if !log_val.is_nil() {
                            proc.log = log_val;
                            proc.childp = process_contact_plist_put(
                                proc.childp,
                                ProcessKeyword::Log.value(),
                                proc.log,
                            )?;
                        }
                        if !buffer.is_nil() {
                            proc.childp = process_contact_plist_put(
                                proc.childp,
                                ProcessKeyword::Buffer.value(),
                                buffer,
                            )?;
                        }
                        apply_connection_process_flags(proc, noquery, stop);
                    }
                    eval.processes.register_socket_fd(id).ok();
                    return Ok(Value::make_process(id));
                }

                let socket = unbound_unix_datagram_socket(&socket_options)?;
                contact =
                    process_contact_plist_put(contact, ProcessKeyword::Remote.value(), path_value)?;
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Local.value(),
                    Value::string(""),
                )?;

                let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
                eval.processes.sync_process_mark(&mut eval.buffers, id)?;
                if let Some(proc) = eval.processes.get_mut(id) {
                    proc.live_io.network_socket = Some(NetworkSocket::UnixDatagram(socket));
                    proc.status = ProcessStatusSymbol::Open.value();
                    proc.childp = contact;
                    proc.plist = plist_val;
                    proc.thread = current_thread_handle(&eval.threads);
                    proc.datagram_address = path_value;
                    proc.datagram_unix_path = Some(path);
                    if !filter_val.is_nil() {
                        proc.filter = filter_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Filter.value(),
                            proc.filter,
                        )?;
                    }
                    if !sentinel_val.is_nil() {
                        proc.sentinel = sentinel_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Sentinel.value(),
                            proc.sentinel,
                        )?;
                    }
                    if !buffer.is_nil() {
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Buffer.value(),
                            buffer,
                        )?;
                    }
                    apply_connection_process_flags(proc, noquery, stop);
                }
                eval.processes.register_socket_fd(id).ok();
                return Ok(Value::make_process(id));
            }

            if socket_type == NetworkSocketType::Seqpacket {
                if server {
                    let listener = bind_unix_seqpacket_listener_socket(
                        &path,
                        server_backlog.unwrap_or(5),
                        &socket_options,
                    )?;
                    contact = process_contact_plist_put(
                        contact,
                        ProcessKeyword::Local.value(),
                        Value::heap_string(crate::emacs_core::fileio::path_to_lisp_file_name(
                            &path,
                        )),
                    )?;

                    let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
                    eval.processes.sync_process_mark(&mut eval.buffers, id)?;
                    if let Some(proc) = eval.processes.get_mut(id) {
                        proc.childp = contact;
                        proc.thread = current_thread_handle(&eval.threads);
                        proc.plist = plist_val;
                        proc.live_io.network_socket =
                            Some(NetworkSocket::SeqpacketListener(listener));
                        if !filter_val.is_nil() {
                            proc.filter = filter_val;
                            proc.childp = process_contact_plist_put(
                                proc.childp,
                                ProcessKeyword::Filter.value(),
                                proc.filter,
                            )?;
                        }
                        if !sentinel_val.is_nil() {
                            proc.sentinel = sentinel_val;
                            proc.childp = process_contact_plist_put(
                                proc.childp,
                                ProcessKeyword::Sentinel.value(),
                                proc.sentinel,
                            )?;
                        }
                        if !log_val.is_nil() {
                            proc.log = log_val;
                            proc.childp = process_contact_plist_put(
                                proc.childp,
                                ProcessKeyword::Log.value(),
                                proc.log,
                            )?;
                        }
                        if !buffer.is_nil() {
                            proc.childp = process_contact_plist_put(
                                proc.childp,
                                ProcessKeyword::Buffer.value(),
                                buffer,
                            )?;
                        }
                        apply_connection_process_flags(proc, noquery, stop);
                    }
                    eval.processes.register_socket_fd(id).ok();
                    return Ok(Value::make_process(id));
                }

                let socket = connect_unix_seqpacket_socket(&path, &socket_options)?;
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Remote.value(),
                    Value::heap_string(crate::emacs_core::fileio::path_to_lisp_file_name(&path)),
                )?;
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Local.value(),
                    Value::string(""),
                )?;

                let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
                eval.processes.sync_process_mark(&mut eval.buffers, id)?;
                if let Some(proc) = eval.processes.get_mut(id) {
                    proc.live_io.network_socket = Some(NetworkSocket::SeqpacketStream(socket));
                    proc.status = process_status_run_value();
                    proc.childp = contact;
                    proc.plist = plist_val;
                    proc.thread = current_thread_handle(&eval.threads);
                    if !filter_val.is_nil() {
                        proc.filter = filter_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Filter.value(),
                            proc.filter,
                        )?;
                    }
                    if !sentinel_val.is_nil() {
                        proc.sentinel = sentinel_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Sentinel.value(),
                            proc.sentinel,
                        )?;
                    }
                    if !buffer.is_nil() {
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Buffer.value(),
                            buffer,
                        )?;
                    }
                    apply_connection_process_flags(proc, noquery, stop);
                }

                eval.processes.register_socket_fd(id).ok();

                return Ok(Value::make_process(id));
            }

            if server {
                let listener =
                    bind_unix_listener_socket(&path, server_backlog.unwrap_or(5), &socket_options)?;
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Local.value(),
                    Value::heap_string(crate::emacs_core::fileio::path_to_lisp_file_name(&path)),
                )?;

                let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
                eval.processes.sync_process_mark(&mut eval.buffers, id)?;
                if let Some(proc) = eval.processes.get_mut(id) {
                    proc.childp = contact;
                    proc.thread = current_thread_handle(&eval.threads);
                    proc.plist = plist_val;
                    proc.live_io.network_socket = Some(NetworkSocket::UnixListener(listener));
                    if !filter_val.is_nil() {
                        proc.filter = filter_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Filter.value(),
                            proc.filter,
                        )?;
                    }
                    if !sentinel_val.is_nil() {
                        proc.sentinel = sentinel_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Sentinel.value(),
                            proc.sentinel,
                        )?;
                    }
                    if !log_val.is_nil() {
                        proc.log = log_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Log.value(),
                            proc.log,
                        )?;
                    }
                    if !buffer.is_nil() {
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Buffer.value(),
                            buffer,
                        )?;
                    }
                    apply_connection_process_flags(proc, noquery, stop);
                }
                eval.processes.register_socket_fd(id).ok();
                return Ok(Value::make_process(id));
            }

            contact = process_contact_plist_put(
                contact,
                ProcessKeyword::Remote.value(),
                Value::heap_string(crate::emacs_core::fileio::path_to_lisp_file_name(&path)),
            )?;
            contact = process_contact_plist_put(
                contact,
                ProcessKeyword::Local.value(),
                Value::string(""),
            )?;

            if nowait {
                let start = start_nonblocking_unix_stream_socket(&path, &socket_options)?;
                let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
                eval.processes.sync_process_mark(&mut eval.buffers, id)?;
                if let Some(proc) = eval.processes.get_mut(id) {
                    proc.status = process_status_connect_value();
                    proc.childp = contact;
                    proc.plist = plist_val;
                    proc.thread = current_thread_handle(&eval.threads);
                    if !filter_val.is_nil() {
                        proc.filter = filter_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Filter.value(),
                            proc.filter,
                        )?;
                    }
                    if !sentinel_val.is_nil() {
                        proc.sentinel = sentinel_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Sentinel.value(),
                            proc.sentinel,
                        )?;
                    }
                    match start {
                        Ok(stream) => {
                            proc.live_io.network_socket = Some(NetworkSocket::UnixStream(stream));
                            proc.live_io.pending_network_connect =
                                Some(PendingNetworkConnect::Local);
                        }
                        Err(err) => {
                            proc.status = process_status_failed_value(io_error_status_code(&err));
                        }
                    }
                    apply_connection_process_flags(proc, noquery, stop);
                }
                if eval
                    .processes
                    .get(id)
                    .is_some_and(|proc| proc.live_io.pending_network_connect.is_some())
                {
                    eval.processes.register_socket_writable_fd(id).ok();
                }
                return Ok(Value::make_process(id));
            }

            let stream = connect_unix_stream_socket(&path, &socket_options)?;
            let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
            eval.processes.sync_process_mark(&mut eval.buffers, id)?;
            if let Some(proc) = eval.processes.get_mut(id) {
                proc.live_io.network_socket = Some(NetworkSocket::UnixStream(stream));
                proc.status = process_status_run_value();
                proc.childp = contact;
                proc.plist = plist_val;
                proc.thread = current_thread_handle(&eval.threads);
                if !filter_val.is_nil() {
                    proc.filter = filter_val;
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Filter.value(),
                        proc.filter,
                    )?;
                }
                if !sentinel_val.is_nil() {
                    proc.sentinel = sentinel_val;
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Sentinel.value(),
                        proc.sentinel,
                    )?;
                }
                if !buffer.is_nil() {
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Buffer.value(),
                        buffer,
                    )?;
                }
                apply_connection_process_flags(proc, noquery, stop);
            }

            eval.processes.register_socket_fd(id).ok();

            Ok(Value::make_process(id))
        }
    }
}

/// `make-network-process` for `:type 'datagram` (UDP): binds a server
/// socket or creates a connected client socket. Every path returns, so
/// the stream-mode server/client paths below never see datagram type.
/// Extracted verbatim from builtin_make_network_process.
#[allow(clippy::too_many_arguments)]
pub(super) fn connect_datagram_network_process(
    eval: &mut super::super::eval::Context,
    family: NetworkProcessFamily,
    host: Option<String>,
    service: Value,
    name: LispString,
    mut contact: Value,
    filter_val: Value,
    sentinel_val: Value,
    log_val: Value,
    resolved_coding: ProcessCodingSystems,
    buffer: Value,
    plist_val: Value,
    server: bool,
    noquery: bool,
    stop: bool,
    socket_type: NetworkSocketType,
    socket_options: Vec<NetworkSocketOptionSpec>,
) -> EvalResult {
    let port = parse_network_service_port(&service, server, socket_type)?;
    let host_str = host
        .clone()
        .unwrap_or_else(|| family.loopback_host().to_string());
    if server {
        let effective_options = tcp_server_socket_options(&socket_options);
        let socket = bind_udp_socket_host(host_str.as_str(), port, family, &effective_options)?;
        let local_addr = socket.local_addr().map_err(|e| {
            signal(
                LispCondition::FileError,
                vec![Value::string(format!("getsockname: {}", e))],
            )
        })?;
        let zero_datagram = datagram_zero_address_for(local_addr);
        contact = process_contact_plist_put(
            contact,
            ProcessKeyword::Service.value(),
            Value::fixnum(local_addr.port() as i64),
        )?;
        contact = process_contact_plist_put(
            contact,
            ProcessKeyword::Local.value(),
            socket_addr_to_lisp_value(local_addr),
        )?;
        contact =
            process_contact_plist_put(contact, ProcessKeyword::Remote.value(), zero_datagram)?;

        let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
        eval.processes.sync_process_mark(&mut eval.buffers, id)?;
        if let Some(proc) = eval.processes.get_mut(id) {
            proc.childp = contact;
            proc.thread = current_thread_handle(&eval.threads);
            proc.plist = plist_val;
            proc.status = ProcessStatusSymbol::Open.value();
            proc.live_io.network_socket = Some(NetworkSocket::UdpSocket(socket));
            proc.datagram_address = zero_datagram;
            proc.datagram_socket_addr = None;
            if !filter_val.is_nil() {
                proc.filter = filter_val;
                proc.childp = process_contact_plist_put(
                    proc.childp,
                    ProcessKeyword::Filter.value(),
                    proc.filter,
                )?;
            }
            if !sentinel_val.is_nil() {
                proc.sentinel = sentinel_val;
                proc.childp = process_contact_plist_put(
                    proc.childp,
                    ProcessKeyword::Sentinel.value(),
                    proc.sentinel,
                )?;
            }
            if !log_val.is_nil() {
                proc.log = log_val;
                proc.childp =
                    process_contact_plist_put(proc.childp, ProcessKeyword::Log.value(), proc.log)?;
            }
            if !buffer.is_nil() {
                proc.childp =
                    process_contact_plist_put(proc.childp, ProcessKeyword::Buffer.value(), buffer)?;
            }
            apply_connection_process_flags(proc, noquery, stop);
        }
        eval.processes.register_socket_fd(id).ok();
        return Ok(Value::make_process(id));
    }

    let (socket, remote_addr) =
        connect_udp_socket_host(host_str.as_str(), port, family, &socket_options)?;
    contact = process_contact_plist_put(
        contact,
        ProcessKeyword::Remote.value(),
        socket_addr_to_lisp_value(remote_addr),
    )?;
    contact = process_contact_plist_put(
        contact,
        ProcessKeyword::Local.value(),
        socket_addr_to_lisp_value(udp_unspecified_addr_for(remote_addr)),
    )?;

    let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
    eval.processes.sync_process_mark(&mut eval.buffers, id)?;
    if let Some(proc) = eval.processes.get_mut(id) {
        proc.live_io.network_socket = Some(NetworkSocket::UdpSocket(socket));
        proc.datagram_socket_addr = Some(remote_addr);
        proc.datagram_address = socket_addr_to_lisp_value(remote_addr);
        proc.status = ProcessStatusSymbol::Open.value();
        proc.childp = contact;
        proc.plist = plist_val;
        proc.thread = current_thread_handle(&eval.threads);
        if !filter_val.is_nil() {
            proc.filter = filter_val;
            proc.childp = process_contact_plist_put(
                proc.childp,
                ProcessKeyword::Filter.value(),
                proc.filter,
            )?;
        }
        if !sentinel_val.is_nil() {
            proc.sentinel = sentinel_val;
            proc.childp = process_contact_plist_put(
                proc.childp,
                ProcessKeyword::Sentinel.value(),
                proc.sentinel,
            )?;
        }
        if !buffer.is_nil() {
            proc.childp =
                process_contact_plist_put(proc.childp, ProcessKeyword::Buffer.value(), buffer)?;
        }
        apply_connection_process_flags(proc, noquery, stop);
    }
    eval.processes.register_socket_fd(id).ok();
    Ok(Value::make_process(id))
}

/// `make-network-process` server mode for stream sockets: bind, listen,
/// and register the accepting process. Always returns. Extracted
/// verbatim from builtin_make_network_process.
#[allow(clippy::too_many_arguments)]
pub(super) fn listen_stream_network_process(
    eval: &mut super::super::eval::Context,
    family: NetworkProcessFamily,
    host: Option<String>,
    service: Value,
    name: LispString,
    mut contact: Value,
    filter_val: Value,
    sentinel_val: Value,
    log_val: Value,
    resolved_coding: ProcessCodingSystems,
    buffer: Value,
    plist_val: Value,
    noquery: bool,
    stop: bool,
    server_backlog: Option<i32>,
    socket_type: NetworkSocketType,
    socket_options: Vec<NetworkSocketOptionSpec>,
) -> EvalResult {
    let port = parse_network_service_port(&service, true, socket_type)?;
    let host_str = host
        .clone()
        .unwrap_or_else(|| family.loopback_host().to_string());
    let effective_options = tcp_server_socket_options(&socket_options);
    let listener = bind_tcp_listener_host(
        host_str.as_str(),
        port,
        family,
        server_backlog.unwrap_or(5),
        &effective_options,
    )?;
    let local_addr = listener.local_addr().map_err(|e| {
        signal(
            LispCondition::FileError,
            vec![Value::string(format!("getsockname: {}", e))],
        )
    })?;
    let local = socket_addr_to_lisp_value(local_addr);
    let actual_service = Value::fixnum(local_addr.port() as i64);
    contact = process_contact_plist_put(contact, ProcessKeyword::Service.value(), actual_service)?;
    contact = process_contact_plist_put(contact, ProcessKeyword::Local.value(), local)?;

    let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
    eval.processes.sync_process_mark(&mut eval.buffers, id)?;
    if let Some(proc) = eval.processes.get_mut(id) {
        proc.childp = contact;
        proc.thread = current_thread_handle(&eval.threads);
        proc.plist = plist_val;
        proc.live_io.network_socket = Some(NetworkSocket::TcpListener(listener));
        if !filter_val.is_nil() {
            proc.filter = filter_val;
            proc.childp = process_contact_plist_put(
                proc.childp,
                ProcessKeyword::Filter.value(),
                proc.filter,
            )?;
        }
        if !sentinel_val.is_nil() {
            proc.sentinel = sentinel_val;
            proc.childp = process_contact_plist_put(
                proc.childp,
                ProcessKeyword::Sentinel.value(),
                proc.sentinel,
            )?;
        }
        if !log_val.is_nil() {
            proc.log = log_val;
            proc.childp =
                process_contact_plist_put(proc.childp, ProcessKeyword::Log.value(), proc.log)?;
        }
        if !buffer.is_nil() {
            proc.childp =
                process_contact_plist_put(proc.childp, ProcessKeyword::Buffer.value(), buffer)?;
        }
        apply_connection_process_flags(proc, noquery, stop);
    }
    eval.processes.register_socket_fd(id).ok();
    Ok(Value::make_process(id))
}

/// `make-network-process` for `:family 'local` (unix-domain sockets):
/// server bind/listen or client connect on a filesystem socket path,
/// stream or datagram. Diverges on every path (non-unix builds signal),
/// so the inet resolution below never runs for local family. Extracted
/// verbatim from builtin_make_network_process.
#[allow(clippy::too_many_arguments)]
#[allow(unused_variables)] // family/host_value feed cfg(unix)-gated arms
pub(super) fn connect_local_socket_process(
    eval: &mut super::super::eval::Context,
    family: NetworkProcessFamily,
    host_value: Value,
    service: Value,
    name: LispString,
    mut contact: Value,
    filter_val: Value,
    sentinel_val: Value,
    log_val: Value,
    resolved_coding: ProcessCodingSystems,
    buffer: Value,
    plist_val: Value,
    nowait: bool,
    server: bool,
    noquery: bool,
    stop: bool,
    server_backlog: Option<i32>,
    socket_type: NetworkSocketType,
    socket_options: Vec<NetworkSocketOptionSpec>,
    tls_parameters: Option<super::super::tls::GnutlsBootParameters>,
    remote_address_value: Value,
) -> EvalResult {
    #[cfg(not(unix))]
    {
        return Err(signal(
            "error",
            vec![Value::string("Unknown address family")],
        ));
    }

    #[cfg(unix)]
    {
        let service_path = crate::emacs_core::fileio::lisp_file_name_to_path_buf(
            super::super::builtins::expect_lisp_string(&service)?,
        );
        if !host_value.is_nil() {
            contact = process_contact_plist_put(contact, ProcessKeyword::Host.value(), Value::NIL)?;
        }

        if socket_type == NetworkSocketType::Datagram {
            let service_path_value = Value::heap_string(
                crate::emacs_core::fileio::path_to_lisp_file_name(&service_path),
            );
            if server {
                let socket = bind_unix_datagram_socket(&service_path, &socket_options)?;
                let zero_datagram = datagram_zero_unix_address();
                let (datagram_unix_path, datagram_address) = if !remote_address_value.is_nil() {
                    match parse_network_address_spec(&remote_address_value)? {
                        NetworkAddressSpec::Local(remote_path) => {
                            let remote_value = Value::heap_string(
                                crate::emacs_core::fileio::path_to_lisp_file_name(&remote_path),
                            );
                            (Some(remote_path), remote_value)
                        }
                        NetworkAddressSpec::Inet(_) => (None, zero_datagram),
                    }
                } else {
                    (None, zero_datagram)
                };
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Local.value(),
                    service_path_value,
                )?;
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Remote.value(),
                    datagram_address,
                )?;

                let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
                eval.processes.sync_process_mark(&mut eval.buffers, id)?;
                if let Some(proc) = eval.processes.get_mut(id) {
                    proc.childp = contact;
                    proc.thread = current_thread_handle(&eval.threads);
                    proc.plist = plist_val;
                    proc.status = ProcessStatusSymbol::Open.value();
                    proc.live_io.network_socket = Some(NetworkSocket::UnixDatagram(socket));
                    proc.datagram_address = datagram_address;
                    proc.datagram_unix_path = datagram_unix_path;
                    if !filter_val.is_nil() {
                        proc.filter = filter_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Filter.value(),
                            proc.filter,
                        )?;
                    }
                    if !sentinel_val.is_nil() {
                        proc.sentinel = sentinel_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Sentinel.value(),
                            proc.sentinel,
                        )?;
                    }
                    if !log_val.is_nil() {
                        proc.log = log_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Log.value(),
                            proc.log,
                        )?;
                    }
                    if !buffer.is_nil() {
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Buffer.value(),
                            buffer,
                        )?;
                    }
                    apply_connection_process_flags(proc, noquery, stop);
                }
                eval.processes.register_socket_fd(id).ok();
                return Ok(Value::make_process(id));
            }

            let socket = unbound_unix_datagram_socket(&socket_options)?;
            contact = process_contact_plist_put(
                contact,
                ProcessKeyword::Remote.value(),
                service_path_value,
            )?;
            contact = process_contact_plist_put(
                contact,
                ProcessKeyword::Local.value(),
                Value::string(""),
            )?;

            let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
            eval.processes.sync_process_mark(&mut eval.buffers, id)?;
            if let Some(proc) = eval.processes.get_mut(id) {
                proc.live_io.network_socket = Some(NetworkSocket::UnixDatagram(socket));
                proc.status = ProcessStatusSymbol::Open.value();
                proc.childp = contact;
                proc.plist = plist_val;
                proc.thread = current_thread_handle(&eval.threads);
                proc.datagram_address = service_path_value;
                proc.datagram_unix_path = Some(service_path);
                if !filter_val.is_nil() {
                    proc.filter = filter_val;
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Filter.value(),
                        proc.filter,
                    )?;
                }
                if !sentinel_val.is_nil() {
                    proc.sentinel = sentinel_val;
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Sentinel.value(),
                        proc.sentinel,
                    )?;
                }
                if !buffer.is_nil() {
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Buffer.value(),
                        buffer,
                    )?;
                }
                apply_connection_process_flags(proc, noquery, stop);
            }

            eval.processes.register_socket_fd(id).ok();
            return Ok(Value::make_process(id));
        }

        if socket_type == NetworkSocketType::Seqpacket {
            if server {
                let listener = bind_unix_seqpacket_listener_socket(
                    &service_path,
                    server_backlog.unwrap_or(5),
                    &socket_options,
                )?;
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Local.value(),
                    Value::heap_string(crate::emacs_core::fileio::path_to_lisp_file_name(
                        &service_path,
                    )),
                )?;

                let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
                eval.processes.sync_process_mark(&mut eval.buffers, id)?;
                if let Some(proc) = eval.processes.get_mut(id) {
                    proc.childp = contact;
                    proc.thread = current_thread_handle(&eval.threads);
                    proc.plist = plist_val;
                    proc.live_io.network_socket = Some(NetworkSocket::SeqpacketListener(listener));
                    if !filter_val.is_nil() {
                        proc.filter = filter_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Filter.value(),
                            proc.filter,
                        )?;
                    }
                    if !sentinel_val.is_nil() {
                        proc.sentinel = sentinel_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Sentinel.value(),
                            proc.sentinel,
                        )?;
                    }
                    if !log_val.is_nil() {
                        proc.log = log_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Log.value(),
                            proc.log,
                        )?;
                    }
                    if !buffer.is_nil() {
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Buffer.value(),
                            buffer,
                        )?;
                    }
                    apply_connection_process_flags(proc, noquery, stop);
                }
                eval.processes.register_socket_fd(id).ok();
                return Ok(Value::make_process(id));
            }

            let socket = connect_unix_seqpacket_socket(&service_path, &socket_options)?;
            contact = process_contact_plist_put(
                contact,
                ProcessKeyword::Remote.value(),
                Value::heap_string(crate::emacs_core::fileio::path_to_lisp_file_name(
                    &service_path,
                )),
            )?;
            contact = process_contact_plist_put(
                contact,
                ProcessKeyword::Local.value(),
                Value::string(""),
            )?;

            let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
            eval.processes.sync_process_mark(&mut eval.buffers, id)?;
            if let Some(proc) = eval.processes.get_mut(id) {
                proc.live_io.network_socket = Some(NetworkSocket::SeqpacketStream(socket));
                proc.status = process_status_run_value();
                proc.childp = contact;
                proc.plist = plist_val;
                proc.thread = current_thread_handle(&eval.threads);
                if !filter_val.is_nil() {
                    proc.filter = filter_val;
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Filter.value(),
                        proc.filter,
                    )?;
                }
                if !sentinel_val.is_nil() {
                    proc.sentinel = sentinel_val;
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Sentinel.value(),
                        proc.sentinel,
                    )?;
                }
                if !buffer.is_nil() {
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Buffer.value(),
                        buffer,
                    )?;
                }
                apply_connection_process_flags(proc, noquery, stop);
            }

            eval.processes.register_socket_fd(id).ok();

            return Ok(Value::make_process(id));
        }

        if server {
            let listener = bind_unix_listener_socket(
                &service_path,
                server_backlog.unwrap_or(5),
                &socket_options,
            )?;
            contact = process_contact_plist_put(
                contact,
                ProcessKeyword::Local.value(),
                Value::heap_string(crate::emacs_core::fileio::path_to_lisp_file_name(
                    &service_path,
                )),
            )?;

            let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
            eval.processes.sync_process_mark(&mut eval.buffers, id)?;
            if let Some(proc) = eval.processes.get_mut(id) {
                proc.childp = contact;
                proc.thread = current_thread_handle(&eval.threads);
                proc.plist = plist_val;
                proc.live_io.network_socket = Some(NetworkSocket::UnixListener(listener));
                if !filter_val.is_nil() {
                    proc.filter = filter_val;
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Filter.value(),
                        proc.filter,
                    )?;
                }
                if !sentinel_val.is_nil() {
                    proc.sentinel = sentinel_val;
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Sentinel.value(),
                        proc.sentinel,
                    )?;
                }
                if !log_val.is_nil() {
                    proc.log = log_val;
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Log.value(),
                        proc.log,
                    )?;
                }
                if !buffer.is_nil() {
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Buffer.value(),
                        buffer,
                    )?;
                }
                apply_connection_process_flags(proc, noquery, stop);
            }
            if let Some(parameters) = tls_parameters.clone() {
                upgrade_process_to_tls::<RustlsBackend>(
                    &mut eval.processes,
                    id,
                    &parameters.client,
                    "make-network-process",
                    signal_gnutls_boot_error,
                )?;
            }
            eval.processes.register_socket_fd(id).ok();
            return Ok(Value::make_process(id));
        }

        contact = process_contact_plist_put(
            contact,
            ProcessKeyword::Remote.value(),
            Value::heap_string(crate::emacs_core::fileio::path_to_lisp_file_name(
                &service_path,
            )),
        )?;
        contact =
            process_contact_plist_put(contact, ProcessKeyword::Local.value(), Value::string(""))?;

        if nowait {
            let start = start_nonblocking_unix_stream_socket(&service_path, &socket_options)?;
            let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
            eval.processes.sync_process_mark(&mut eval.buffers, id)?;
            if let Some(proc) = eval.processes.get_mut(id) {
                proc.status = process_status_connect_value();
                proc.childp = contact;
                proc.plist = plist_val;
                proc.thread = current_thread_handle(&eval.threads);
                if !filter_val.is_nil() {
                    proc.filter = filter_val;
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Filter.value(),
                        proc.filter,
                    )?;
                }
                if !sentinel_val.is_nil() {
                    proc.sentinel = sentinel_val;
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Sentinel.value(),
                        proc.sentinel,
                    )?;
                }
                if !buffer.is_nil() {
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Buffer.value(),
                        buffer,
                    )?;
                }
                match start {
                    Ok(stream) => {
                        proc.live_io.network_socket = Some(NetworkSocket::UnixStream(stream));
                        proc.live_io.pending_network_connect = Some(PendingNetworkConnect::Local);
                    }
                    Err(err) => {
                        proc.status = process_status_failed_value(io_error_status_code(&err));
                    }
                }
                apply_connection_process_flags(proc, noquery, stop);
            }
            if eval
                .processes
                .get(id)
                .is_some_and(|proc| proc.live_io.pending_network_connect.is_some())
            {
                eval.processes.register_socket_writable_fd(id).ok();
            }
            return Ok(Value::make_process(id));
        }

        let stream = connect_unix_stream_socket(&service_path, &socket_options)?;
        let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
        eval.processes.sync_process_mark(&mut eval.buffers, id)?;
        if let Some(proc) = eval.processes.get_mut(id) {
            proc.live_io.network_socket = Some(NetworkSocket::UnixStream(stream));
            proc.status = process_status_run_value();
            proc.childp = contact;
            proc.plist = plist_val;
            proc.thread = current_thread_handle(&eval.threads);
            if !filter_val.is_nil() {
                proc.filter = filter_val;
                proc.childp = process_contact_plist_put(
                    proc.childp,
                    ProcessKeyword::Filter.value(),
                    proc.filter,
                )?;
            }
            if !sentinel_val.is_nil() {
                proc.sentinel = sentinel_val;
                proc.childp = process_contact_plist_put(
                    proc.childp,
                    ProcessKeyword::Sentinel.value(),
                    proc.sentinel,
                )?;
            }
            if !buffer.is_nil() {
                proc.childp =
                    process_contact_plist_put(proc.childp, ProcessKeyword::Buffer.value(), buffer)?;
            }
            apply_connection_process_flags(proc, noquery, stop);
        }

        eval.processes.register_socket_fd(id).ok();

        Ok(Value::make_process(id))
    }
}

pub(crate) fn builtin_make_network_process(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if args.is_empty() {
        return Ok(Value::NIL);
    }
    check_keyword_arg_pairs(&args)?;
    eval.sync_process_read_config_from_visible_variables();

    // ---- Parse all keyword arguments ----
    let mut name: Option<LispString> = None;
    let mut host_value = Value::NIL;
    let mut service: Option<Value> = None;
    let mut server = false;
    let mut server_value = Value::NIL;
    let mut family_value = Value::NIL;
    let mut local_address_value = Value::NIL;
    let mut remote_address_value = Value::NIL;
    let mut nowait = false;
    let mut socket_type = NetworkSocketType::Stream;
    let mut contact = Value::list(args.clone());
    let mut filter_val = Value::NIL;
    let mut sentinel_val = Value::NIL;
    let mut log_val = Value::NIL;
    let mut buffer_val = Value::NIL;
    let mut coding_val = Value::NIL;
    let mut tls_parameters_val = Value::NIL;
    let mut noquery = false;
    let mut plist_val = Value::NIL;
    let mut stop_val = Value::NIL;
    let socket_options = collect_network_socket_options(&args);

    let mut seen_keywords = Vec::new();
    let mut i = 0usize;
    while i < args.len() {
        let key = &args[i];
        let value = args.get(i + 1).cloned().unwrap_or(Value::NIL);
        let Some(keyword) = ProcessKeyword::from_value(key) else {
            i += 2;
            continue;
        };
        if process_keyword_already_seen(&mut seen_keywords, keyword) {
            i += 2;
            continue;
        }
        match keyword {
            ProcessKeyword::Name => name = Some(expect_process_name_lisp_string(&value)?),
            ProcessKeyword::Host => host_value = value,
            ProcessKeyword::Service => service = Some(value),
            ProcessKeyword::Server => {
                server = value.is_truthy();
                server_value = value;
            }
            ProcessKeyword::Family => family_value = value,
            ProcessKeyword::Type => socket_type = parse_network_socket_type(&value)?,
            ProcessKeyword::Nowait => nowait = value.is_truthy(),
            ProcessKeyword::Filter => filter_val = value,
            ProcessKeyword::Sentinel => sentinel_val = value,
            ProcessKeyword::Log => log_val = value,
            ProcessKeyword::Buffer => buffer_val = value,
            ProcessKeyword::Coding => coding_val = value,
            ProcessKeyword::TlsParameters => tls_parameters_val = value,
            ProcessKeyword::Noquery => noquery = value.is_truthy(),
            ProcessKeyword::Stop => stop_val = value,
            ProcessKeyword::Local => local_address_value = value,
            ProcessKeyword::Remote => remote_address_value = value,
            ProcessKeyword::Plist => plist_val = value,
            _ => {}
        }
        i += 2;
    }

    let Some(name) = name else {
        return Err(signal(
            "error",
            vec![Value::string("Missing :name keyword parameter")],
        ));
    };

    if server && nowait {
        return Err(signal(
            "error",
            vec![Value::string("`:server' is incompatible with `:nowait'")],
        ));
    }
    let plist_val = copy_process_plist(plist_val)?;
    let stop = stop_val.is_truthy();
    let server_backlog = if server {
        Some(network_server_backlog(server_value)?)
    } else {
        None
    };
    let tls_parameters = parse_make_network_tls_parameters(tls_parameters_val)?;

    // Resolve :buffer to a buffer name (creating buffer if needed).
    let buffer = if !buffer_val.is_nil() {
        parse_make_process_buffer(eval, &buffer_val)?
    } else {
        Value::NIL
    };

    // Capture the dynamic coding environment once and pass it through every
    // connection strategy.  GNU's `set_network_socket_coding_system` gives
    // `coding-system-for-read/write` precedence over the operation/default
    // codings; URL binds the read side to `binary` around this call.
    let multibyte = ProcessBufferMultibyteness {
        process_buffer: match resolve_buffer_for_process_lookup_in_state(&eval.buffers, &buffer) {
            Ok(Some(bid)) => eval
                .buffers
                .get(bid)
                .map(|b| b.get_multibyte())
                .unwrap_or(true),
            // No buffer (or unresolved) -> `buffer_defaults` is multibyte.
            _ => true,
        },
        current_buffer: eval
            .buffers
            .current_buffer()
            .map(|buffer| buffer.get_multibyte())
            .unwrap_or(true),
    };
    let mut coding_environment = NetworkProcessCodingEnvironment {
        coding_system_for_read: eval.visible_variable_value_or_nil("coding-system-for-read"),
        coding_system_for_write: eval.visible_variable_value_or_nil("coding-system-for-write"),
        operation_coding_system: Value::NIL,
        default_process_coding_system: eval
            .visible_variable_value_or_nil("default-process-coding-system"),
        short_circuit: ConnectionProcessUnibyteShortCircuit::network(multibyte),
    };

    let explicit_address = if server {
        local_address_value
    } else {
        remote_address_value
    };
    if !explicit_address.is_nil() {
        // A `:local`/`:remote` address carries no HOST and SERVICE, so GNU's
        // `NILP (host) || NILP (service)` guard leaves `coding_systems` nil and
        // the alist is never reached (src/process.c:3325-3330).
        let resolved_coding =
            resolve_network_process_coding_systems(coding_val, coding_environment);
        return connect_network_process_at_explicit_address(
            eval,
            explicit_address,
            remote_address_value,
            name,
            contact,
            filter_val,
            sentinel_val,
            log_val,
            resolved_coding,
            buffer,
            plist_val,
            nowait,
            server,
            noquery,
            stop,
            server_backlog,
            socket_type,
            socket_options,
            tls_parameters,
            tls_parameters_val,
        );
    }

    let family = parse_network_process_family(&family_value)?;
    let host = parse_network_host(&host_value, family)?;

    let service = service.unwrap_or(Value::NIL);
    if service.is_nil() {
        return Err(signal_wrong_type_string(Value::NIL));
    }
    if coding_val.is_nil() {
        coding_environment.operation_coding_system =
            find_network_operation_coding_system(eval, &name, buffer, host_value, service)?;
    }
    let resolved_coding = resolve_network_process_coding_systems(coding_val, coding_environment);

    if family.is_local() {
        return connect_local_socket_process(
            eval,
            family,
            host_value,
            service,
            name,
            contact,
            filter_val,
            sentinel_val,
            log_val,
            resolved_coding,
            buffer,
            plist_val,
            nowait,
            server,
            noquery,
            stop,
            server_backlog,
            socket_type,
            socket_options,
            tls_parameters,
            remote_address_value,
        );
    }

    if socket_type == NetworkSocketType::Datagram {
        return connect_datagram_network_process(
            eval,
            family,
            host,
            service,
            name,
            contact,
            filter_val,
            sentinel_val,
            log_val,
            resolved_coding,
            buffer,
            plist_val,
            server,
            noquery,
            stop,
            socket_type,
            socket_options,
        );
    }

    #[cfg(unix)]
    if socket_type == NetworkSocketType::Seqpacket {
        return Err(signal(
            "error",
            vec![Value::string("Unsupported connection type")],
        ));
    }

    if server {
        return listen_stream_network_process(
            eval,
            family,
            host,
            service,
            name,
            contact,
            filter_val,
            sentinel_val,
            log_val,
            resolved_coding,
            buffer,
            plist_val,
            noquery,
            stop,
            server_backlog,
            socket_type,
            socket_options,
        );
    }

    // ---- Client mode: establish TCP connection ----
    let host_str = host.unwrap_or_else(|| family.loopback_host().to_string());
    let port = parse_network_service_port(&service, false, socket_type)?;

    if nowait {
        let immediate_start = nowait_tcp_immediate_addrs(host_value, &host_str, port, family)
            .map(|addrs| start_pending_tcp_stream_connect(addrs, &socket_options))
            .transpose()?;
        let pending_dns = if immediate_start.is_none() {
            Some(start_async_network_dns_lookup(
                host_str.clone(),
                port,
                family,
                NetworkSocketType::Stream,
                socket_options.clone(),
                eval.processes.wait_notifier(),
            ))
        } else {
            None
        };

        let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
        eval.processes.sync_process_mark(&mut eval.buffers, id)?;
        if let Some(proc) = eval.processes.get_mut(id) {
            proc.status = process_status_connect_value();
            proc.childp = contact;
            proc.plist = plist_val;
            proc.thread = current_thread_handle(&eval.threads);
            if !filter_val.is_nil() {
                proc.filter = filter_val;
                proc.childp = process_contact_plist_put(
                    proc.childp,
                    ProcessKeyword::Filter.value(),
                    proc.filter,
                )?;
            }
            if !sentinel_val.is_nil() {
                proc.sentinel = sentinel_val;
                proc.childp = process_contact_plist_put(
                    proc.childp,
                    ProcessKeyword::Sentinel.value(),
                    proc.sentinel,
                )?;
            }
            if !buffer.is_nil() {
                proc.childp =
                    process_contact_plist_put(proc.childp, ProcessKeyword::Buffer.value(), buffer)?;
            }
            if let Some(start) = immediate_start {
                match start {
                    PendingNetworkConnectStart::Started(started) => {
                        let local_addr = started.stream.local_addr().ok();
                        proc.live_io.network_socket =
                            Some(NetworkSocket::TcpStream(started.stream));
                        proc.live_io.pending_network_connect = Some(PendingNetworkConnect::Tcp {
                            remaining_addrs: started.remaining_addrs,
                            socket_options: socket_options.clone(),
                        });
                        ProcessManager::update_tcp_client_contact(
                            proc,
                            started.remote_addr,
                            local_addr,
                        )?;
                    }
                    PendingNetworkConnectStart::Failed(code) => {
                        proc.status = process_status_failed_value(code);
                    }
                }
            } else if let Some(pending_dns) = pending_dns {
                proc.live_io.pending_network_connect =
                    Some(PendingNetworkConnect::Dns(pending_dns));
            }
            if proc.live_io.pending_network_connect.is_some() {
                proc.gnutls_boot_parameters = tls_parameters_val;
            }
            apply_connection_process_flags(proc, noquery, stop);
        }
        if eval.processes.get(id).is_some_and(|proc| {
            proc.live_io.network_socket.is_some() && proc.live_io.pending_network_connect.is_some()
        }) {
            eval.processes.register_socket_writable_fd(id).ok();
        }
        return Ok(Value::make_process(id));
    }

    let stream =
        connect_tcp_stream_host(host_str.as_str(), port, family, &socket_options, contact)?;
    let remote_addr = stream.peer_addr().ok();
    let local_addr = stream.local_addr().ok();

    let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
    eval.processes.sync_process_mark(&mut eval.buffers, id)?;
    if let Some(addr) = remote_addr {
        contact = process_contact_plist_put(
            contact,
            ProcessKeyword::Remote.value(),
            socket_addr_to_lisp_value(addr),
        )?;
    }
    if let Some(addr) = local_addr {
        contact = process_contact_plist_put(
            contact,
            ProcessKeyword::Local.value(),
            socket_addr_to_lisp_value(addr),
        )?;
    }
    if let Some(proc) = eval.processes.get_mut(id) {
        proc.live_io.network_socket = Some(NetworkSocket::TcpStream(stream));
        proc.status = process_status_run_value();
        proc.childp = contact;
        proc.plist = plist_val;
        proc.thread = current_thread_handle(&eval.threads);
        if !filter_val.is_nil() {
            proc.filter = filter_val;
            proc.childp = process_contact_plist_put(
                proc.childp,
                ProcessKeyword::Filter.value(),
                proc.filter,
            )?;
        }
        if !sentinel_val.is_nil() {
            proc.sentinel = sentinel_val;
            proc.childp = process_contact_plist_put(
                proc.childp,
                ProcessKeyword::Sentinel.value(),
                proc.sentinel,
            )?;
        }
        if !buffer.is_nil() {
            proc.childp =
                process_contact_plist_put(proc.childp, ProcessKeyword::Buffer.value(), buffer)?;
        }
        apply_connection_process_flags(proc, noquery, stop);
    }

    if let Some(parameters) = tls_parameters {
        upgrade_process_to_tls::<RustlsBackend>(
            &mut eval.processes,
            id,
            &parameters.client,
            "make-network-process",
            signal_gnutls_boot_error,
        )?;
    }

    eval.processes.register_socket_fd(id).ok();

    // GNU fires NO sentinel for a synchronous (non-:nowait) connect -- see the
    // TCP branch above.
    Ok(Value::make_process(id))
}

/// (make-pipe-process &rest ARGS) -> process-or-nil
pub(crate) fn builtin_make_pipe_process(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    eval.sync_process_read_config_from_visible_variables();
    let coding_variables = read_connection_process_coding_variables(eval);
    builtin_make_pipe_process_impl(
        &mut eval.processes,
        &mut eval.buffers,
        &eval.threads,
        Some(&eval.coding_systems),
        coding_variables,
        args,
    )
}

/// Capture the dynamic coding variables the way GNU reads them: at the moment
/// the primitive runs, in the buffer that is current then.
pub(super) fn read_connection_process_coding_variables(
    eval: &mut super::super::eval::Context,
) -> ConnectionProcessCodingVariables {
    ConnectionProcessCodingVariables {
        coding_system_for_read: eval.visible_variable_value_or_nil("coding-system-for-read"),
        coding_system_for_write: eval.visible_variable_value_or_nil("coding-system-for-write"),
        default_process_coding_system: eval
            .visible_variable_value_or_nil("default-process-coding-system"),
    }
}

pub(crate) fn builtin_make_pipe_process_impl(
    processes: &mut ProcessManager,
    buffers: &mut BufferManager,
    threads: &ThreadManager,
    coding_systems: Option<&super::super::coding::CodingSystemManager>,
    coding_variables: ConnectionProcessCodingVariables,
    args: Vec<Value>,
) -> EvalResult {
    if args.is_empty() {
        return Ok(Value::NIL);
    }
    check_keyword_arg_pairs(&args)?;

    let contact = Value::list(args.clone());
    let mut name: Option<LispString> = None;
    let mut buffer: Option<Value> = None;
    let mut filter = Value::NIL;
    let mut sentinel = Value::NIL;
    let mut coding = Value::NIL;
    let mut coding_present = false;
    let mut noquery = false;
    let mut stop = false;
    let mut plist = Value::NIL;

    let mut seen_keywords = Vec::new();
    let mut i = 0usize;
    while i < args.len() {
        let key = &args[i];
        let value = args.get(i + 1).cloned().unwrap_or(Value::NIL);
        let Some(keyword) = ProcessKeyword::from_value(key) else {
            i += 2;
            continue;
        };
        if process_keyword_already_seen(&mut seen_keywords, keyword) {
            i += 2;
            continue;
        }
        match keyword {
            ProcessKeyword::Name => {
                name = Some(expect_process_name_lisp_string(&value)?);
            }
            ProcessKeyword::Buffer => {
                buffer = Some(parse_make_process_buffer_in_state(buffers, &value)?);
            }
            ProcessKeyword::Filter => filter = value,
            ProcessKeyword::Sentinel => sentinel = value,
            ProcessKeyword::Coding => {
                coding = value;
                coding_present = true;
            }
            ProcessKeyword::Noquery => noquery = value.is_truthy(),
            ProcessKeyword::Stop => stop = value.is_truthy(),
            ProcessKeyword::Plist => plist = value,
            _ => {}
        }
        i += 2;
    }

    let Some(name) = name else {
        return Err(signal(
            "error",
            vec![Value::string("Missing :name keyword parameter")],
        ));
    };

    let resolved_buffer = match buffer {
        Some(explicit) => explicit,
        None => {
            // Issue #131: buffer-name lookup/creation takes a `&str`; a lossy
            // UTF-8 rendering is the right display form here and avoids the
            // buggy storage-string sentinels.
            let name_runtime = crate::emacs_core::emacs_char::to_utf8_lossy(name.as_bytes());
            let id = buffers
                .find_buffer_by_name(&name_runtime)
                .unwrap_or_else(|| buffers.create_buffer(&name_runtime));
            Value::make_buffer(id)
        }
    };
    // GNU runs `Fmake_pipe_process`'s own chain here (src/process.c:2517-2570)
    // and then validates the RESULT, not the `:coding` keyword, by handing it
    // to `setup_process_coding_systems` -> `setup_coding_system`
    // ("This may signal an error", :2573).  So a bad `coding-system-for-read`
    // signals too -- measured under GNU 31.0.90.
    let resolved_coding = resolve_pipe_process_coding_systems(
        coding,
        coding_variables.pipe(process_buffer_multibyteness(buffers, resolved_buffer)),
    );
    validate_resolved_process_coding_systems(coding_systems, resolved_coding)?;
    let plist = copy_process_plist(plist)?;
    let (pipe_reader, pipe_writer) = os_pipe::pipe().map_err(|error| {
        signal(
            LispCondition::FileError,
            vec![Value::string(format!("Creating pipe: {error}"))],
        )
    })?;
    let pipe_reader = ChildOutputReader::Shared(pipe_reader);

    let id = processes.create_process_with_kind_lisp(
        name,
        resolved_buffer,
        LispString::from_utf8("pipe"),
        Vec::new(),
        ProcessKindWithoutDevice::Pipe,
        resolved_coding,
    );
    processes.sync_process_mark(buffers, id)?;
    if let Some(proc) = processes.get_mut(id) {
        proc.childp = contact;
        proc.thread = current_thread_handle(threads);
        proc.plist = plist;
        if !filter.is_nil() {
            proc.filter = filter;
        }
        if !sentinel.is_nil() {
            proc.sentinel = sentinel;
        }
        proc.coding_explicitly_set = coding_present;
        apply_connection_process_flags(proc, noquery, stop);
    }
    if processes.get(id).is_some_and(process_filter_accepts_output)
        && let Some(poller) = processes.wait_backend.poller()
    {
        ProcessManager::register_child_stdout_with_poller(poller, &pipe_reader, id);
    }
    if let Some(proc) = processes.get_mut(id) {
        proc.live_io.child_stdout = Some(pipe_reader);
        proc.live_io.module_pipe_writer = Some(pipe_writer);
    }
    Ok(Value::make_process(id))
}

/// (make-serial-process &rest ARGS) -> process-or-nil
pub(crate) fn builtin_make_serial_process(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    eval.sync_process_read_config_from_visible_variables();
    let coding_variables = read_connection_process_coding_variables(eval);
    builtin_make_serial_process_impl(
        &mut eval.processes,
        &mut eval.buffers,
        &eval.threads,
        Some(&eval.coding_systems),
        coding_variables,
        args,
    )
}

pub(crate) fn builtin_make_serial_process_impl(
    processes: &mut ProcessManager,
    buffers: &mut BufferManager,
    threads: &ThreadManager,
    coding_systems: Option<&super::super::coding::CodingSystemManager>,
    coding_variables: ConnectionProcessCodingVariables,
    args: Vec<Value>,
) -> EvalResult {
    if args.is_empty() {
        return Ok(Value::NIL);
    }
    check_keyword_arg_pairs(&args)?;

    let contact = Value::list(args.clone());
    let mut name: Option<LispString> = None;
    let mut port: Option<Value> = None;
    let mut port_name: Option<LispString> = None;
    let mut speed: Option<Value> = None;
    let mut buffer: Option<Value> = None;
    let mut filter = Value::NIL;
    let mut sentinel = Value::NIL;
    let mut coding = Value::NIL;
    let mut coding_present = false;
    let mut noquery = false;
    let mut stop = false;
    let mut plist = Value::NIL;

    let mut seen_keywords = Vec::new();
    let mut i = 0usize;
    while i < args.len() {
        let key = &args[i];
        let value = args.get(i + 1).cloned().unwrap_or(Value::NIL);
        let Some(keyword) = ProcessKeyword::from_value(key) else {
            i += 2;
            continue;
        };
        if process_keyword_already_seen(&mut seen_keywords, keyword) {
            i += 2;
            continue;
        }
        match keyword {
            ProcessKeyword::Name => {
                name = Some(expect_process_name_lisp_string(&value)?);
            }
            ProcessKeyword::Port => {
                if value.is_nil() {
                    port = None;
                } else {
                    let string = super::super::builtins::expect_lisp_string(&value)?.clone();
                    port = Some(value);
                    port_name = Some(string);
                }
            }
            ProcessKeyword::Speed => speed = Some(value),
            ProcessKeyword::Buffer => buffer = Some(value),
            ProcessKeyword::Filter => filter = value,
            ProcessKeyword::Sentinel => sentinel = value,
            ProcessKeyword::Coding => {
                coding = value;
                coding_present = true;
            }
            ProcessKeyword::Noquery => noquery = value.is_truthy(),
            ProcessKeyword::Stop => stop = value.is_truthy(),
            ProcessKeyword::Plist => plist = value,
            _ => {}
        }
        i += 2;
    }

    // GNU checks the PORT before it looks at `:speed` at all
    // (src/process.c:3193-3200), so a missing or non-string port beats a
    // non-fixnum speed.  Measured, GNU 31.0.90:
    //   (make-serial-process :speed "x")          => (error "No port specified")
    //   (make-serial-process :port 1 :speed "x")  => (wrong-type-argument stringp 1)
    let (Some(port), Some(port_name)) = (port, port_name) else {
        return Err(signal("error", vec![Value::string("No port specified")]));
    };
    let Some(speed) = speed else {
        return Err(signal("error", vec![Value::string(":speed not specified")]));
    };
    if !speed.is_nil() && !speed.is_fixnum() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("fixnump"), speed],
        ));
    }
    let name = name.unwrap_or_else(|| port_name.clone());

    // GNU `serial_open`, src/process.c:3212 -- and everything below this line
    // is downstream of it.  A port that cannot be opened is reported before
    // the process buffer is created, before the coding chain runs and before
    // `serial-process-configure` is called, so `:buffer "x" :bytesize 5` on a
    // nonexistent port still reports `file-missing` and still leaves no buffer
    // named "x" behind.  All three orderings measured against GNU 31.0.90.
    let device = open_serial_port(port, &port_name)?;

    // GNU creates the buffer here (:3226), which is why a LATER failure
    // -- an undefined coding system, an invalid `:bytesize`, a `tcgetattr` on
    // something that is not a tty -- leaves the buffer behind even though it
    // unwinds the process record itself.
    let resolved_buffer = match buffer {
        Some(explicit) if !explicit.is_nil() => {
            parse_make_process_buffer_in_state(buffers, &explicit)?
        }
        _ => {
            let name_runtime = crate::emacs_core::emacs_char::to_utf8_lossy(name.as_bytes());
            let id = buffers
                .find_buffer_by_name(&name_runtime)
                .unwrap_or_else(|| buffers.create_buffer(&name_runtime));
            Value::make_buffer(id)
        }
    };

    // GNU resolves and then validates through `setup_process_coding_systems`
    // (src/process.c:3277), the same two steps `make-pipe-process` takes.
    let resolved_coding = resolve_serial_process_coding_systems(coding, coding_variables.serial());
    validate_resolved_process_coding_systems(coding_systems, resolved_coding)?;
    let plist = copy_process_plist(plist)?;

    // GNU `Fserial_process_configure (nargs, args)`, src/process.c:3284, whose
    // first act is to return without touching the device when the contact's
    // `:speed` is nil (:3098-3099, documented as "the serial port is not
    // configured any further").  That is measurable and not a shortcut: a
    // `:speed nil` port on `/dev/null` is created successfully, where the same
    // port with `:speed 9600` reports `Failed tcgetattr`.
    let childp = if process_contact_plist_get(contact, ProcessKeyword::Speed.value()).is_nil() {
        contact
    } else {
        configure_serial_device(&device, contact, contact)?
    };

    let id = processes.create_serial_process(name, resolved_buffer, device, resolved_coding);
    processes.sync_process_mark(buffers, id)?;
    if let Some(proc) = processes.get_mut(id) {
        proc.childp = childp;
        proc.thread = current_thread_handle(threads);
        proc.plist = plist;
        if !filter.is_nil() {
            proc.filter = filter;
        }
        if !sentinel.is_nil() {
            proc.sentinel = sentinel;
        }
        proc.coding_explicitly_set = coding_present;
        apply_connection_process_flags(proc, noquery, stop);
    }
    processes.register_serial_read_fd(id);
    Ok(Value::make_process(id))
}

/// (serial-process-configure &rest ARGS) -> nil
pub(crate) fn builtin_serial_process_configure(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_serial_process_configure_impl(&mut eval.processes, &eval.buffers, args)
}

pub(crate) fn builtin_serial_process_configure_impl(
    processes: &mut ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    check_keyword_arg_pairs(&args)?;

    let contact = Value::list(args);
    let process_designator = [
        ProcessKeyword::Process,
        ProcessKeyword::Name,
        ProcessKeyword::Buffer,
        ProcessKeyword::Port,
    ]
    .into_iter()
    .map(|keyword| process_contact_plist_get(contact, keyword.value()))
    .find(|value| !value.is_nil())
    .unwrap_or(Value::NIL);
    let id = resolve_get_process_designator_in_state(processes, buffers, &process_designator)?;
    let proc = processes
        .get_mut(id)
        .ok_or_else(|| signal_wrong_type_processp(Value::make_process(id)))?;
    if proc.kind != ProcessKind::Serial {
        return Err(signal("error", vec![Value::string("Not a serial process")]));
    }
    // GNU `Fserial_process_configure`, src/process.c:3098-3099: a contact whose
    // `:speed` is nil means "do not configure the port any further", so the
    // primitive returns before `serial_configure` and never touches the device.
    if process_contact_plist_get(proc.childp, ProcessKeyword::Speed.value()).is_nil() {
        return Ok(Value::NIL);
    }
    // GNU reaches the device through `p->outfd` (src/sysdep.c:3162, :3303).
    // A serial process that has been `delete-process`ed has no device left;
    // GNU would `tcgetattr` a closed descriptor and report `Failed tcgetattr`,
    // which is what an `EBADF` here produces too.
    let current = proc.childp;
    let result = match proc.live_io.serial_port.as_ref() {
        Some(device) => configure_serial_device(device, current, contact),
        None => Err(signal_file_errno(
            "Failed tcgetattr",
            Value::NIL,
            libc::EBADF,
        )),
    };
    proc.childp = result?;
    Ok(Value::NIL)
}

/// (set-network-process-option PROCESS OPTION VALUE &optional NO-ERROR) -> t-or-nil
pub(crate) fn builtin_set_network_process_option(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if (3..=4).contains(&args.len())
        && let Some(id) = pending_network_connect_id(&eval.processes, args[0])?
    {
        eval.wait_while_network_process_connecting(id)?;
    }
    builtin_set_network_process_option_impl(&mut eval.processes, args)
}

pub(crate) fn builtin_set_network_process_option_impl(
    processes: &mut ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() < 3 || args.len() > 4 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("set-network-process-option"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let id = resolve_live_process_or_wrong_type_in_manager(processes, &args[0])?;
    let proc = processes.get_mut(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    if proc.kind != ProcessKind::Network {
        return Err(signal(
            "error",
            vec![Value::string("Process is not a network process")],
        ));
    }

    if args[1].as_symbol_name().is_none() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[1]],
        ));
    }
    let no_error = args.get(3).is_some_and(|v| v.is_truthy());
    let Some(keyword) = ProcessKeyword::from_value(&args[1]) else {
        return if no_error {
            Ok(Value::NIL)
        } else {
            Err(signal(
                "error",
                vec![Value::string("Unknown or unsupported option")],
            ))
        };
    };
    let Some(option) = NetworkSocketOption::from_keyword(keyword) else {
        return if no_error {
            Ok(Value::NIL)
        } else {
            Err(signal(
                "error",
                vec![Value::string("Unknown or unsupported option")],
            ))
        };
    };

    let spec = NetworkSocketOptionSpec {
        keyword,
        option,
        value: args[2],
    };
    apply_network_socket_option_to_process(proc, spec)?;
    proc.childp = process_contact_plist_put(proc.childp, args[1], args[2])?;
    Ok(Value::T)
}

// `start-process', `start-process-shell-command', `start-file-process' and
// `start-file-process-shell-command' are NOT here: GNU has no C version of
// any of them.  They are Lisp over `make-process' (which IS in C,
// src/process.c:1767) -- lisp/subr.el:3466, lisp/subr.el:5063,
// lisp/simple.el:5249 and lisp/subr.el:5076 -- and we load those files.
// DIVERGENCES.md 149 deleted the Rust subrs that used to shadow them.

/// (call-process PROGRAM &optional INFILE DESTINATION DISPLAY &rest ARGS)
///
/// Runs the command synchronously using `std::process::Command`, captures
/// output.  Returns the exit code as an integer.
pub(crate) fn builtin_call_process(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    super::super::callproc::builtin_call_process(eval, args)
}

/// (call-process-shell-command COMMAND &optional INFILE DESTINATION DISPLAY &rest ARGS)
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_call_process_shell_command(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    super::super::callproc::builtin_call_process_shell_command(eval, args)
}

/// (process-file PROGRAM &optional INFILE DESTINATION DISPLAY &rest ARGS)
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_process_file(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    super::super::callproc::builtin_process_file(eval, args)
}

/// (process-file-shell-command COMMAND &optional INFILE DESTINATION DISPLAY &rest ARGS)
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_process_file_shell_command(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    super::super::callproc::builtin_process_file_shell_command(eval, args)
}

/// (process-lines PROGRAM &rest ARGS) -> list of lines
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_process_lines(
    _eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    super::super::callproc::builtin_process_lines(_eval, args)
}

/// (process-lines-ignore-status PROGRAM &rest ARGS) -> list of lines
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_process_lines_ignore_status(
    _eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    super::super::callproc::builtin_process_lines_ignore_status(_eval, args)
}

/// (process-lines-handling-status PROGRAM STATUS-HANDLER &rest ARGS) -> list of lines
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_process_lines_handling_status(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    super::super::callproc::builtin_process_lines_handling_status(eval, args)
}

/// (call-process-region START END PROGRAM &optional DELETE DESTINATION DISPLAY &rest ARGS)
///
/// Pipes buffer region from START to END through PROGRAM.
pub(crate) fn builtin_call_process_region(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    super::super::callproc::builtin_call_process_region(eval, args)
}

/// (delete-process PROCESS) -> nil
pub(crate) fn builtin_delete_process(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 1 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("delete-process"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let id = if let Some(process) = args.first() {
        if process.as_symbol_name() == Some("message") {
            resolve_get_process_designator_in_state(&eval.processes, &eval.buffers, &Value::NIL)?
        } else {
            resolve_get_process_designator_in_state(&eval.processes, &eval.buffers, process)?
        }
    } else {
        resolve_optional_process_or_current_buffer_in_state(&eval.processes, &eval.buffers, None)?
    };
    let was_terminal = eval
        .processes
        .get(id)
        .is_some_and(|proc| process_status_is_terminal_for_notify(&proc.status));
    let was_pending_notification = eval
        .processes
        .get(id)
        .is_some_and(|proc| proc.status_notify_pending);
    eval.delete_process_running_its_sentinel(id, !was_terminal || was_pending_notification)?;
    Ok(Value::NIL)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_delete_process_impl(
    processes: &mut ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 1 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("delete-process"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let id = if let Some(process) = args.first() {
        if process.as_symbol_name() == Some("message") {
            resolve_get_process_designator_in_state(processes, buffers, &Value::NIL)?
        } else {
            resolve_get_process_designator_in_state(processes, buffers, process)?
        }
    } else {
        resolve_optional_process_or_current_buffer_in_state(processes, buffers, None)?
    };
    processes.delete_process(id);
    Ok(Value::NIL)
}

/// (continue-process &optional PROCESS CURRENT-GROUP) -> process-or-nil
pub(crate) fn builtin_continue_process(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("continue-process"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let (id, _) = resolve_optional_process_with_explicit_return_in_state(
        &eval.processes,
        &eval.buffers,
        args.first(),
    )?;
    let ret = builtin_continue_process_impl(&mut eval.processes, &eval.buffers, args)?;
    if eval
        .processes
        .get_any(id)
        .is_some_and(|proc| proc.kind == ProcessKind::Real && process_status_is_run(&proc.status))
    {
        eval.notify_process_status_sentinel(id)?;
    }
    Ok(ret)
}

pub(crate) fn builtin_continue_process_impl(
    processes: &mut ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("continue-process"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let (id, ret) =
        resolve_optional_process_with_explicit_return_in_state(processes, buffers, args.first())?;
    // GNU's `Fcontinue_process` handles a network, serial or pipe process
    // before it resolves anything (src/process.c:7294-7315) and sets
    // `p->command' whether or not the connection is still listed -- hence
    // `get_any_mut'.  The signalling branch stays live-only: a retired REAL
    // process was rejected by the resolver above, as GNU's `p->infd < 0'
    // rejects it at :7087-7089.
    let is_live = processes.get(id).is_some();
    let mut continued = false;
    if let Some(proc) = processes.get_any_mut(id) {
        if matches!(
            proc.kind,
            ProcessKind::Network | ProcessKind::Pipe | ProcessKind::Serial
        ) {
            proc.command = Value::NIL;
            if proc.kind == ProcessKind::Serial {
                proc.status = ProcessStatusSymbol::Open.value();
            }
        } else if is_live {
            // GNU `process_send_signal(SIGCONT)` discards `raw_status_new`
            // before publishing `run`, so a queued stop transition cannot
            // overwrite the explicit continuation on the next status pass.
            proc.status_notify_pending = false;
            proc.pending_status = Value::NIL;
            proc.status = process_status_run_value();
            #[cfg(unix)]
            let _ =
                deliver_process_signal(proc, libc::SIGCONT, ProcessSignalRecipient::ProcessGroup);
            continued = true;
        }
    }
    if continued {
        // GNU's `p->tick = ++process_tick;` at src/process.c:7178, between the
        // `pset_status (p, Qrun)` above and the `status_notify (NULL, NULL)`
        // at :7181 -- which `builtin_continue_process` runs, so the tick is
        // consumed in the same call.
        processes.record_status_change(StatusChangeSite::ProcessSendSignalSigcont, id);
    }
    Ok(ret)
}

/// (interrupt-process &optional PROCESS CURRENT-GROUP) -> process-or-nil
pub(crate) fn builtin_interrupt_process(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("interrupt-process"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    eval.funcall_general(
        Value::symbol("run-hook-with-args-until-success"),
        vec![
            Value::symbol("interrupt-process-functions"),
            args.first().copied().unwrap_or(Value::NIL),
            args.get(1).copied().unwrap_or(Value::NIL),
        ],
    )
}

/// (kill-process &optional PROCESS CURRENT-GROUP) -> process-or-nil
pub(crate) fn builtin_kill_process(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_kill_process_impl(&mut eval.processes, &eval.buffers, args)
}

pub(crate) fn builtin_kill_process_impl(
    processes: &mut ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("kill-process"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let (id, ret) =
        resolve_optional_process_with_explicit_return_in_state(processes, buffers, args.first())?;
    check_process_is_real_subprocess(processes, id)?;
    if let Some(proc) = processes.get_mut(id) {
        kill_real_process_child(proc, signal_kill_number());
    }
    Ok(ret)
}

/// (signal-process PROCESS SIGNAL &optional CURRENT-GROUP) -> int-or-nil
pub(crate) fn builtin_signal_process(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("signal-process", &args, 2)?;
    if args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("signal-process"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    eval.funcall_general(
        Value::symbol("run-hook-with-args-until-success"),
        vec![
            Value::symbol("signal-process-functions"),
            args[0],
            args[1],
            args.get(2).copied().unwrap_or(Value::NIL),
        ],
    )
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_signal_process_impl(
    processes: &mut ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("signal-process", &args, 2)?;
    if args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("signal-process"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    // The type check precedes the liveness check here too: a retired pipe must
    // reach `signal_cannot_signal_process` below (GNU's `p->pid <= 0` at
    // src/process.c:7381-7382), not this early -1.
    if let Some(process) = args.first()
        && !process.is_nil()
        && is_stale_real_process_designator_in_manager(processes, process)
    {
        return Ok(Value::fixnum(-1));
    }

    let signal_num = parse_signal_number(&args[1])?;
    match resolve_signal_process_target_in_state(processes, buffers, args.first())? {
        SignalProcessTarget::Process(id) => {
            // `get_any_mut`, matching `internal-default-signal-process` above:
            // GNU's `CHECK_PROCESS` + `XPROCESS (process)->pid`
            // (src/process.c:7379-7382) asks the object, not the alist.  This
            // twin is only reachable from tests, but it carried the shape the
            // rest of this file no longer has, which is how the class comes
            // back.
            if let Some(proc) = processes.get_any_mut(id) {
                if proc.kind != ProcessKind::Real {
                    return Err(signal_cannot_signal_process(proc));
                }
                return Ok(Value::fixnum(signal_process_or_unbacked_success(
                    proc,
                    signal_num,
                    ProcessSignalRecipient::ImmediateProcess,
                ) as i64));
            }
            Ok(Value::fixnum(-1))
        }
        SignalProcessTarget::MissingNamedProcess => Ok(Value::NIL),
        SignalProcessTarget::Pid(pid) => {
            Ok(Value::fixnum(sys::send_signal(pid, signal_num) as i64))
        }
    }
}

/// (stop-process &optional PROCESS CURRENT-GROUP) -> process-or-nil
pub(crate) fn builtin_stop_process(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_stop_process_impl(&mut eval.processes, &eval.buffers, args)
}

pub(crate) fn builtin_stop_process_impl(
    processes: &mut ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("stop-process"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let (id, ret) =
        resolve_optional_process_with_explicit_return_in_state(processes, buffers, args.first())?;
    // GNU's `Fstop_process` special-cases a network, serial or pipe process
    // before it resolves anything (src/process.c:7267-7278): it sets
    // `p->command' to t and returns the process, with no liveness test at all.
    let is_live = processes.get(id).is_some();
    if let Some(proc) = processes.get_any_mut(id) {
        if matches!(
            proc.kind,
            ProcessKind::Network | ProcessKind::Pipe | ProcessKind::Serial
        ) {
            proc.command = Value::T;
        } else if is_live {
            #[cfg(unix)]
            let _ =
                deliver_process_signal(proc, libc::SIGTSTP, ProcessSignalRecipient::ProcessGroup);
        }
    }
    Ok(ret)
}

/// (quit-process &optional PROCESS CURRENT-GROUP) -> process-or-nil
pub(crate) fn builtin_quit_process(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_quit_process_impl(&mut eval.processes, &eval.buffers, args)
}

pub(crate) fn builtin_quit_process_impl(
    processes: &mut ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("quit-process"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let (id, ret) =
        resolve_optional_process_with_explicit_return_in_state(processes, buffers, args.first())?;
    check_process_is_real_subprocess(processes, id)?;
    if let Some(proc) = processes.get_mut(id) {
        // Send SIGQUIT to the child process.
        #[cfg(unix)]
        let _ = deliver_process_signal(proc, libc::SIGQUIT, ProcessSignalRecipient::ProcessGroup);
    }
    Ok(ret)
}

/// (process-attributes PID) -> alist-or-nil
pub(crate) fn builtin_process_attributes(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-attributes", &args, 1)?;
    if let Some(default_directory) = visible_default_directory_lisp(eval) {
        let operation = Value::symbol("process-attributes");
        let handler = super::super::fileio::find_file_name_handler_lisp_for_eval(
            eval,
            &default_directory,
            operation,
        );
        if !handler.is_nil() {
            return eval.funcall_general(handler, vec![operation, args[0]]);
        }
    }
    builtin_process_attributes_impl(args)
}

pub(crate) fn builtin_process_attributes_impl(args: Vec<Value>) -> EvalResult {
    expect_args("process-attributes", &args, 1)?;
    let pid = cons_to_os_pid(args[0])?;
    if !sys::process_is_alive(pid) {
        return Ok(Value::NIL);
    }

    let mut attrs = Vec::new();
    if let Some((euid, egid)) = sys::process_effective_ids(pid) {
        attrs.push(Value::cons(
            Value::symbol("group"),
            Value::string(sys::group_name(egid).unwrap_or_else(|| egid.to_string())),
        ));
        attrs.push(Value::cons(
            Value::symbol("egid"),
            Value::fixnum(egid as i64),
        ));
        attrs.push(Value::cons(
            Value::symbol("user"),
            Value::string(sys::user_name(euid).unwrap_or_else(|| euid.to_string())),
        ));
        attrs.push(Value::cons(
            Value::symbol("euid"),
            Value::fixnum(euid as i64),
        ));
    }

    let stat = sys::process_stat(pid).unwrap_or_else(|| sys::ProcStatSnapshot::fallback(pid));
    attrs.push(Value::cons(
        Value::symbol("comm"),
        Value::string(stat.comm.clone()),
    ));
    attrs.push(Value::cons(
        Value::symbol("state"),
        Value::string(stat.state),
    ));
    attrs.push(Value::cons(Value::symbol("ppid"), Value::fixnum(stat.ppid)));
    attrs.push(Value::cons(Value::symbol("pgrp"), Value::fixnum(stat.pgrp)));
    attrs.push(Value::cons(Value::symbol("sess"), Value::fixnum(stat.sess)));
    attrs.push(Value::cons(
        Value::symbol("tpgid"),
        Value::fixnum(stat.tpgid),
    ));
    attrs.push(Value::cons(
        Value::symbol("minflt"),
        Value::fixnum(stat.minflt),
    ));
    attrs.push(Value::cons(
        Value::symbol("majflt"),
        Value::fixnum(stat.majflt),
    ));
    attrs.push(Value::cons(
        Value::symbol("cminflt"),
        Value::fixnum(stat.cminflt),
    ));
    attrs.push(Value::cons(
        Value::symbol("cmajflt"),
        Value::fixnum(stat.cmajflt),
    ));
    attrs.push(Value::cons(
        Value::symbol("utime"),
        time_list_from_ticks(stat.utime_ticks, sys::clock_ticks_per_second()),
    ));
    attrs.push(Value::cons(
        Value::symbol("stime"),
        time_list_from_ticks(stat.stime_ticks, sys::clock_ticks_per_second()),
    ));
    let total_ticks = stat.utime_ticks.saturating_add(stat.stime_ticks);
    attrs.push(Value::cons(
        Value::symbol("time"),
        time_list_from_ticks(total_ticks, sys::clock_ticks_per_second()),
    ));
    attrs.push(Value::cons(
        Value::symbol("cutime"),
        time_list_from_ticks(stat.cutime_ticks, sys::clock_ticks_per_second()),
    ));
    attrs.push(Value::cons(
        Value::symbol("cstime"),
        time_list_from_ticks(stat.cstime_ticks, sys::clock_ticks_per_second()),
    ));
    let total_child_ticks = stat.cutime_ticks.saturating_add(stat.cstime_ticks);
    attrs.push(Value::cons(
        Value::symbol("ctime"),
        time_list_from_ticks(total_child_ticks, sys::clock_ticks_per_second()),
    ));
    attrs.push(Value::cons(Value::symbol("pri"), Value::fixnum(stat.pri)));
    attrs.push(Value::cons(Value::symbol("nice"), Value::fixnum(stat.nice)));
    attrs.push(Value::cons(
        Value::symbol("thcount"),
        Value::fixnum(stat.thcount),
    ));
    let hz = sys::clock_ticks_per_second();
    let start_epoch_time = sys::boot_time_secs().map(|boot_secs| {
        let (start_rel_secs, start_rel_usecs) = ticks_to_secs_usecs(stat.start_ticks, hz);
        (boot_secs.saturating_add(start_rel_secs), start_rel_usecs)
    });
    let (start_secs, start_usecs) = start_epoch_time.unwrap_or((0, 0));
    attrs.push(Value::cons(
        Value::symbol("start"),
        time_list_from_secs_usecs(start_secs, start_usecs),
    ));
    attrs.push(Value::cons(
        Value::symbol("vsize"),
        Value::fixnum(stat.vsize / 1024),
    ));
    attrs.push(Value::cons(Value::symbol("rss"), Value::fixnum(stat.rss)));
    let elapsed = match (now_epoch_secs_usecs(), start_epoch_time) {
        (Some(now), Some(start)) => nonnegative_time_diff(now, start),
        _ => (0, 0),
    };
    attrs.push(Value::cons(
        Value::symbol("etime"),
        time_list_from_secs_usecs(elapsed.0, elapsed.1),
    ));
    let elapsed_secs = elapsed.0 as f64 + (elapsed.1 as f64 / 1_000_000.0);
    let total_cpu_secs = if hz > 0 {
        (total_ticks as f64) / (hz as f64)
    } else {
        0.0
    };
    let pcpu = if elapsed_secs > 0.0 {
        (total_cpu_secs * 100.0) / elapsed_secs
    } else {
        0.0
    };
    attrs.push(Value::cons(
        Value::symbol("pcpu"),
        Value::make_float(if pcpu.is_finite() { pcpu.max(0.0) } else { 0.0 }),
    ));
    let pmem = sys::total_memory_kb()
        .filter(|mem_total_kb| *mem_total_kb > 0)
        .map(|mem_total_kb| (stat.rss as f64 * 100.0) / mem_total_kb as f64)
        .unwrap_or(0.0);
    attrs.push(Value::cons(
        Value::symbol("pmem"),
        Value::make_float(if pmem.is_finite() { pmem.max(0.0) } else { 0.0 }),
    ));
    attrs.push(Value::cons(
        Value::symbol("args"),
        Value::string(sys::process_cmdline(pid, &stat.comm)),
    ));
    attrs.push(Value::cons(
        Value::symbol("ttname"),
        Value::string(stat.ttname),
    ));

    Ok(Value::list(attrs))
}

/// GNU's `CONS_TO_INTEGER (x, pid_t, pid)` (src/lisp.h:4188-4191) -- the one
/// conversion from a Lisp NUMBER to an OS pid, shared by `Fprocess_attributes`
/// and `internal-default-signal-process` (src/process.c:7375-7376).
///
/// It is `cons_to_signed` over `pid_t`'s FULL signed range, so a NEGATIVE
/// result is in domain: at the POSIX level `kill (-pgid, sig)` signals a
/// process GROUP, and GNU relies on that rather than range-checking the
/// argument.  Non-numbers are the caller's business -- `Fprocess_attributes`
/// raises `numberp` here, while `internal-default-signal-process` sends them
/// to `get_process` instead (:7369-7370) -- so the `numberp` arm below is only
/// reachable from the former.
pub(super) fn cons_to_os_pid(value: Value) -> Result<i64, Flow> {
    let pid = match value.kind() {
        ValueKind::Fixnum(n) => n,
        ValueKind::Veclike(VecLikeType::Bignum) => i64::try_from(value.as_bignum().unwrap())
            .map_err(|_| signal_process_attributes_pid_range_error())?,
        ValueKind::Float => {
            let f = value.xfloat();
            if !f.is_finite()
                || f.fract() != 0.0
                || f < process_id_min() as f64
                || f > process_id_max() as f64
            {
                return Err(signal_process_attributes_pid_range_error());
            }
            f as i64
        }
        _ => return Err(signal_wrong_type_numberp(value)),
    };
    if pid < process_id_min() || pid > process_id_max() {
        return Err(signal_process_attributes_pid_range_error());
    }
    Ok(pid)
}

pub(super) fn process_id_min() -> i64 {
    cfg_select! {
        unix => { libc::pid_t::MIN as i64 }
        _ => { i32::MIN as i64 }
    }
}

pub(super) fn process_id_max() -> i64 {
    cfg_select! {
        unix => { libc::pid_t::MAX as i64 }
        _ => { i32::MAX as i64 }
    }
}

/// (make-process &rest ARGS) -> process-or-nil
pub(crate) fn builtin_make_process(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if !args.is_empty() {
        check_keyword_arg_pairs(&args)?;
        if make_process_keyword_arg(&args, ProcessKeyword::FileHandler).is_truthy() {
            let default_directory = visible_default_directory_lisp(eval);
            if let Some(default_directory) = default_directory {
                let operation = Value::symbol("make-process");
                let handler = super::super::fileio::find_file_name_handler_lisp_for_eval(
                    eval,
                    &default_directory,
                    operation,
                );
                if !handler.is_nil() {
                    let mut call_args = Vec::with_capacity(args.len() + 1);
                    call_args.push(operation);
                    call_args.extend_from_slice(&args);
                    return eval.funcall_general(handler, call_args);
                }
            }
        }
    }

    let use_pty = process_connection_type_is_pty(&eval.obarray);
    let executable_search = ExecutableSearch::capture(eval);
    let subprocess_cwd = super::super::callproc::subprocess_default_directory(eval);
    let child_environment = Some(super::super::environment::ChildEnvironment::materialize(
        eval,
        subprocess_cwd.as_deref(),
    ));
    let coding_environment = make_process_coding_environment(eval, &args)?;
    eval.sync_process_read_config_from_visible_variables();
    // `find-operation-coding-system` can hand back a cons this call just
    // allocated (`Fcons (val, val)` at src/coding.c:10861, or whatever a
    // function-valued alist entry returned), and process creation allocates.
    // The other three inputs are values of live symbols and are reachable
    // through the obarray.
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(coding_environment.operation_coding_system);
    let process = builtin_make_process_impl_with_environment(
        &mut eval.processes,
        &mut eval.buffers,
        &eval.threads,
        args,
        use_pty,
        child_environment,
        Some(executable_search),
        subprocess_cwd,
        Some(&eval.coding_systems),
        coding_environment,
    );
    eval.restore_specpdl_roots(roots);
    process
}

/// Read the ambient half of GNU's `Fmake_process` coding chain, including the
/// one step that can run Lisp.
///
/// `(find-operation-coding-system 'start-process NAME BUFFER COMMAND...)` is
/// asked only when the chain would reach it and only when there is a PROGRAM to
/// match, because `start-process` has `target-idx` 2 (src/coding.c:11784) --
/// the alist is matched against the program, not against the process name
/// (measured: an entry keyed on the name does not fire).  GNU guards the call
/// the same way at src/process.c:1970.
pub(super) fn make_process_coding_environment(
    eval: &mut super::super::eval::Context,
    args: &[Value],
) -> Result<MakeProcessCodingEnvironment, Flow> {
    let mut env = MakeProcessCodingEnvironment {
        coding_system_for_read: eval.visible_variable_value_or_nil("coding-system-for-read"),
        coding_system_for_write: eval.visible_variable_value_or_nil("coding-system-for-write"),
        default_process_coding_system: eval
            .visible_variable_value_or_nil("default-process-coding-system"),
        operation_coding_system: Value::NIL,
    };
    let coding = make_process_keyword_arg(args, ProcessKeyword::Coding);
    if !make_process_consults_coding_alist(coding, env)
        || eval
            .visible_variable_value_or_nil("process-coding-system-alist")
            .is_nil()
    {
        return Ok(env);
    }
    let command = make_process_keyword_arg(args, ProcessKeyword::Command);
    if !command.is_cons() {
        return Ok(env);
    }

    let name = make_process_keyword_arg(args, ProcessKeyword::Name);
    let buffer_arg = make_process_keyword_arg(args, ProcessKeyword::Buffer);
    // GNU has already run `Fget_buffer_create` on `:buffer` by this point
    // (src/process.c:1849-1851), so a function-valued alist entry sees the
    // buffer object rather than its name.
    let buffer = if buffer_arg.is_nil() {
        Value::NIL
    } else {
        parse_make_process_buffer(eval, &buffer_arg)?
    };

    let mut operation_args = vec![Value::symbol("start-process"), name, buffer];
    let mut rest = command;
    while rest.is_cons() {
        operation_args.push(rest.cons_car());
        rest = rest.cons_cdr();
    }

    // Root every heap value: a function-valued alist entry runs arbitrary Lisp
    // and can trigger GC.
    let roots = eval.save_specpdl_roots();
    for value in &operation_args {
        eval.push_specpdl_root(*value);
    }
    let result = super::super::builtins::builtin_find_operation_coding_system(eval, operation_args);
    eval.restore_specpdl_roots(roots);
    env.operation_coding_system = result?;
    Ok(env)
}

/// The first value a `make-process`-shaped keyword list gives for KEYWORD, or
/// nil -- GNU's `plist_get (contact, ...)`, which every one of `Fmake_process`'s
/// reads goes through (src/process.c:1849-1910).
pub(super) fn make_process_keyword_arg(args: &[Value], keyword: ProcessKeyword) -> Value {
    let mut i = 0usize;
    while i + 1 < args.len() {
        if ProcessKeyword::from_value(&args[i]) == Some(keyword) {
            return args[i + 1];
        }
        i += 2;
    }
    Value::NIL
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_make_process_impl(
    processes: &mut ProcessManager,
    buffers: &mut BufferManager,
    threads: &ThreadManager,
    args: Vec<Value>,
    default_use_pty: bool,
) -> EvalResult {
    builtin_make_process_impl_with_environment(
        processes,
        buffers,
        threads,
        args,
        default_use_pty,
        None,
        None,
        None,
        None,
        MakeProcessCodingEnvironment::unbound(),
    )
}

#[allow(clippy::too_many_arguments)] // process creation keeps host/runtime services independently borrowed
pub(super) fn builtin_make_process_impl_with_environment(
    processes: &mut ProcessManager,
    buffers: &mut BufferManager,
    threads: &ThreadManager,
    args: Vec<Value>,
    default_use_pty: bool,
    child_environment: Option<super::super::environment::ChildEnvironment>,
    executable_search: Option<ExecutableSearch>,
    subprocess_cwd: Option<PathBuf>,
    coding_systems: Option<&super::super::coding::CodingSystemManager>,
    coding_environment: MakeProcessCodingEnvironment,
) -> EvalResult {
    if args.is_empty() {
        return Ok(Value::NIL);
    }
    check_keyword_arg_pairs(&args)?;

    let mut name: Option<LispString> = None;
    let mut buffer: Option<Value> = None;
    let mut command: Option<Vec<LispString>> = None;
    let mut filter = Value::NIL;
    let mut sentinel = Value::NIL;
    let mut connection_type: Option<Value> = None;
    let mut stderr_target = Value::NIL;
    let mut coding_val: Option<Value> = None;
    let mut noquery = false;
    let mut stop_val = Value::NIL;

    let mut seen_keywords = Vec::new();
    let mut i = 0usize;
    while i < args.len() {
        let key = &args[i];
        let value = args.get(i + 1).cloned().unwrap_or(Value::NIL);
        let Some(keyword) = ProcessKeyword::from_value(key) else {
            i += 2;
            continue;
        };
        if process_keyword_already_seen(&mut seen_keywords, keyword) {
            i += 2;
            continue;
        }
        match keyword {
            ProcessKeyword::Name => name = Some(expect_process_name_lisp_string(&value)?),
            ProcessKeyword::Buffer => {
                buffer = Some(parse_make_process_buffer_in_state(buffers, &value)?)
            }
            ProcessKeyword::Command => command = Some(parse_make_process_command(&value)?),
            ProcessKeyword::Filter => filter = value,
            ProcessKeyword::Sentinel => sentinel = value,
            ProcessKeyword::ConnectionType => connection_type = Some(value),
            ProcessKeyword::Stderr => stderr_target = value,
            ProcessKeyword::Coding => coding_val = Some(value),
            ProcessKeyword::Noquery => noquery = value.is_truthy(),
            ProcessKeyword::Stop => stop_val = value,
            _ => {}
        }
        i += 2;
    }

    let Some(name) = name else {
        return Err(signal(
            "error",
            vec![Value::string("Missing :name keyword parameter")],
        ));
    };

    // Determine PTY vs pipe exactly as GNU's `is_pty_from_symbol` does:
    // nil inherits `process-connection-type`; only `pipe` and `pty` are
    // accepted explicit symbols.
    //
    // GNU's `Fmake_process` (src/process.c) decides STDIN/STDOUT's pty-vs-pipe
    // *solely* from `:connection-type` / `process-connection-type` and stores it
    // in `pty_in`/`pty_out`.  Supplying `:stderr` only routes the child's
    // *stderr* to a separate pipe process (`stderrproc`); it does NOT flip
    // stdin/stdout to a pipe.  In `create_process`, the pty is allocated when
    // `pty_in || pty_out`, stdin/stdout use the pty channels, and the stderr
    // pipe is wired through a wholly separate `forkerr` fd.  Hence with the
    // default connection-type (pty) and `:stderr`, GNU reports
    // `(process-tty-name p 'stdout)` => "/dev/pts/N" and `'stderr` => nil.
    //
    // The previous code wrongly forced `use_pty = false` whenever `:stderr` was
    // given, downgrading stdout from a pty to a pipe and diverging from GNU.
    let use_pty =
        resolve_process_connection_type_use_pty(connection_type.as_ref(), default_use_pty)?;

    let command = command.unwrap_or_default();
    if !stop_val.is_nil() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("null"), stop_val],
        ));
    }
    // NOTE: `:coding` is deliberately NOT validated here.  GNU validates the
    // RESOLVED pair, and it does so inside `create_process`, after the program
    // has been found -- measured, `(make-process :coding 'no-such-xyz :command
    // '("no-such-program-xyz"))` signals `file-missing`, not
    // `coding-system-error`.  The check now lives beside the resolver below.
    let (program, argv) = if command.is_empty() {
        (LispString::from_utf8(""), Vec::new())
    } else {
        (command[0].clone(), command[1..].to_vec())
    };
    let executable = if program.is_empty() {
        None
    } else if let Some(search) = executable_search {
        Some(search.resolve(&program, ExecutableLookupMode::MakeProcess)?)
    } else {
        None
    };

    // GNU resolves the pair at src/process.c:1950-2008 and only validates it
    // later, inside `create_process` -> `setup_process_coding_systems` ->
    // `setup_coding_system` (src/process.c:2277, src/coding.c:5678).  That
    // ordering is measurable and this is the point that reproduces it: a
    // program that cannot be found still signals `file-missing` first, an
    // undefined coding system signals `coding-system-error`, and neither leaves
    // a process behind.
    let resolved_coding =
        resolve_make_process_coding_systems(coding_val.unwrap_or(Value::NIL), coding_environment);
    validate_process_coding_component(coding_systems, resolved_coding.decode)?;
    validate_process_coding_component(coding_systems, resolved_coding.encode)?;
    let stderrproc = if stderr_target.is_nil() {
        Value::NIL
    } else if let Some(stderr_id) = process_value_to_id(&stderr_target) {
        // An existing process (object or legacy id) is reused as the stderr
        // pipe; GNU requires it to be a pipe process.
        let stderr_proc = processes
            .get_any(stderr_id)
            .ok_or_else(|| signal_wrong_type_processp(stderr_target))?;
        if stderr_proc.kind != ProcessKind::Pipe {
            return Err(signal(
                "error",
                vec![Value::string("Process is not a pipe process")],
            ));
        }
        Value::make_process(stderr_id)
    } else {
        builtin_make_pipe_process_impl(
            processes,
            buffers,
            threads,
            coding_systems,
            // GNU builds this pipe with `CALLN (Fmake_pipe_process, ...)`
            // (src/process.c:1883), so the stderr pipe runs the PIPE chain --
            // it is not handed `Fmake_process`'s answer.  Measured: a
            // `coding-system-for-read` binding decodes the child's stderr.
            coding_environment.connection_variables(),
            vec![
                ProcessKeyword::Name.value(),
                Value::heap_string(name.concat(&LispString::from_unibyte(b" stderr".to_vec()))),
                ProcessKeyword::Buffer.value(),
                stderr_target,
                ProcessKeyword::Noquery.value(),
                Value::bool_val(noquery),
            ],
        )?
    };
    let id = processes.create_process_lisp_resolved(
        name,
        buffer.unwrap_or(Value::NIL),
        program,
        argv,
        executable,
        resolved_coding,
    );
    processes.sync_process_mark(buffers, id)?;

    // GNU `make_process` (src/process.c) initialises every process's locking
    // thread to the creating thread (`pset_thread (p, Fcurrent_thread ())`),
    // so `process-thread` returns that thread rather than nil.  The network /
    // serial / pipe creators already do this; the subprocess path must too.
    if let Some(proc) = processes.get_mut(id) {
        proc.thread = current_thread_handle(threads);
    }

    // Set filter and sentinel if provided.
    if !filter.is_nil()
        && let Some(proc) = processes.get_mut(id)
    {
        proc.filter = filter;
    }
    if !sentinel.is_nil()
        && let Some(proc) = processes.get_mut(id)
    {
        proc.sentinel = sentinel;
    }
    if !stderrproc.is_nil()
        && let Some(proc) = processes.get_mut(id)
    {
        proc.stderrproc = stderrproc;
    }
    #[cfg(windows)]
    if let Some(stderr_id) = stderrproc.as_process_id()
        && let Some(stderr_proc) = processes.get_mut(stderr_id)
    {
        stderr_proc.stderr_pipe_owner_status_deferred_at = None;
    }
    if let Some(proc) = processes.get_mut(id) {
        proc.default_directory = subprocess_cwd;
        if noquery {
            proc.exit_query_policy = ExitQueryPolicy::NoQuery;
        }
    }

    // The pair resolved above is installed unconditionally: there is no branch
    // in which a real subprocess keeps a coding system nobody resolved.  GNU
    // does NOT run these through `coding_inherit_eol_type` here (unlike
    // `set-process-coding-system`).
    //
    // `coding_explicitly_set` stays a record of whether the CALLER passed
    // `:coding`, because a PTY status heuristic keys off it; the resolver
    // answering for an absent `:coding` must not look like an explicit one.
    if let Some(proc) = processes.get_mut(id) {
        proc.coding_decode = resolved_coding.decode;
        proc.coding_encode = resolved_coding.encode;
        proc.coding_explicitly_set = coding_val.is_some_and(|coding| !coding.is_nil());
    }

    match processes.spawn_child_with_environment(id, use_pty, child_environment) {
        Ok(ChildSpawnOutcome::Spawned) => {}
        // GNU's parent never sees the exec failure: the forked child reports it
        // on its own stderr and exits 127/126 (src/callproc.c:1206-1216), so
        // `make-process` returns a live process object that dies immediately.
        Ok(ChildSpawnOutcome::ExecFailed(errno)) => {
            processes.set_child_status_pending(
                id,
                process_status_exit_value(ChildSpawnOutcome::exec_failure_exit_code(errno)),
            );
        }
        // A launcher failure proper -- no pty could be allocated, the record
        // vanished.  The pipe path still reports a failed exec this way; that
        // asymmetry is measured but not reproduced, see DIVERGENCES.md 174.
        Err(e) => {
            return Err(signal(
                LispCondition::FileMissing,
                vec![Value::string("Doing vfork"), Value::string(e)],
            ));
        }
    }

    Ok(Value::make_process(id))
}

#[derive(Clone, Copy, Debug)]
pub(super) struct AcceptProcessOutputRequest {
    pub(super) wait: ProcessOutputWaitRequest,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) target_process: Option<ProcessId>,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) just_this_one: bool,
}

impl AcceptProcessOutputRequest {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn wait_timing_is_poll(self) -> bool {
        self.wait.timing().is_poll()
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn wait_timing_is_finite(self) -> bool {
        self.wait.timing().is_finite()
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn wait_timing_is_forever(self) -> bool {
        self.wait.timing().is_forever()
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn completes_on_any_process_activity(self) -> bool {
        self.target_process.is_none()
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn completes_on_target_process_activity(self, process: ProcessId) -> bool {
        self.target_process == Some(process)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn services_only_target_process_output(self) -> bool {
        self.just_this_one
    }
}

pub(super) fn parse_accept_process_output_request(
    processes: &mut ProcessManager,
    args: &[Value],
) -> Result<Option<AcceptProcessOutputRequest>, Flow> {
    if args.len() > 4 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("accept-process-output"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    if let Some(process) = args.first()
        && !process.is_nil()
        && resolve_live_process_designator_in_manager(processes, process).is_none()
    {
        if is_stale_process_id_designator_in_manager(processes, process) {
            return Ok(None);
        }
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), *process],
        ));
    }

    if let Some(seconds) = args.get(1) {
        if let Some(milliseconds) = args.get(2) {
            if !milliseconds.is_nil() && !milliseconds.is_fixnum() {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("fixnump"), *milliseconds],
                ));
            }
            if milliseconds.is_nil() {
                if !seconds.is_nil() && !seconds.is_number() {
                    return Err(signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("numberp"), *seconds],
                    ));
                }
            } else if !seconds.is_nil() && !seconds.is_fixnum() {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("fixnump"), *seconds],
                ));
            }
        } else if !seconds.is_nil() && !seconds.is_number() {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("numberp"), *seconds],
            ));
        }
    }

    let target_id = if let Some(process) = args.first() {
        if !process.is_nil() {
            resolve_live_process_designator_in_manager(processes, process)
        } else {
            None
        }
    } else {
        None
    };

    let just_this_one = target_id.is_some() && args.get(3).is_some_and(|value| value.is_truthy());
    let allow_timers = if target_id.is_some() {
        !args.get(3).is_some_and(|v| v.is_fixnum())
    } else {
        true
    };
    let milliseconds_supplied = args.get(2).is_some_and(|value| !value.is_nil());
    let positive_timeout = accept_process_output_positive_timeout(args);
    let timing = if let Some(timeout) = positive_timeout {
        ProcessOutputWaitTiming::For(timeout)
    } else if target_id.is_some()
        && !milliseconds_supplied
        && args.get(1).is_none_or(|value| value.is_nil())
    {
        ProcessOutputWaitTiming::Forever
    } else {
        ProcessOutputWaitTiming::Poll
    };
    Ok(Some(AcceptProcessOutputRequest {
        wait: ProcessOutputWaitRequest::new(timing, target_id, just_this_one, allow_timers),
        target_process: target_id,
        just_this_one,
    }))
}

pub(super) fn accept_process_output_positive_timeout(args: &[Value]) -> Option<Duration> {
    let total_seconds = if let Some(milliseconds) = args.get(2).filter(|value| !value.is_nil()) {
        let milliseconds = milliseconds.as_fixnum().unwrap_or(0) as f64 / 1000.0;
        let seconds = args
            .get(1)
            .filter(|value| !value.is_nil())
            .and_then(|value| value.as_fixnum())
            .unwrap_or(0) as f64;
        seconds + milliseconds
    } else if let Some(seconds) = args.get(1).filter(|value| !value.is_nil()) {
        seconds
            .as_fixnum()
            .map(|value| value as f64)
            .or_else(|| seconds.as_float())
            .unwrap_or(0.0)
    } else {
        return None;
    };

    (total_seconds > 0.0).then(|| Duration::from_secs_f64(total_seconds))
}

/// (process-send-string PROCESS STRING) -> nil
pub(crate) fn builtin_process_send_string(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-send-string", &args, 2)?;
    let input = args[1]
        .as_lisp_string()
        .cloned()
        .ok_or_else(|| signal_wrong_type_string(args[1]))?;
    if let Some(id) = process_value_to_id(&args[0])
        && is_stale_process_id_designator_in_manager(&eval.processes, &args[0])
    {
        return Err(signal_process_not_running_in_manager(&eval.processes, id));
    }
    let id = resolve_get_process_designator_in_state(&eval.processes, &eval.buffers, &args[0])?;
    eval.wait_while_network_process_connecting(id)?;
    // GNU `send_process` runs `update_status` (:6725-6726) before it tests
    // `p->status`, and by then `handle_child_signal` has already recorded a
    // child that exited.  This port makes the recording here.
    if eval
        .processes
        .observe(UpdateStatusSite::SendProcess, id)
        .is_some_and(|observed| !observed.allows_send())
    {
        return Err(signal_process_not_running_in_manager(&eval.processes, id));
    }
    let encoded = encode_process_send_input(&eval.processes, id, &input, eval.eol_conversion());
    eval.send_process_input_reentrant(id, &encoded)?;
    Ok(Value::NIL)
}

/// The `ProcessManager`-only spelling of `process-send-string`, for unit
/// fixtures that have no `Context`.
///
/// `#[cfg(test)]` since entry 143: with no `Context` there is no
/// `inhibit-eol-conversion' to read, so it names `EolConversion::Enabled`
/// below.  A production caller must not be able to inherit that assumption by
/// reaching for the shorter spelling.
#[cfg(test)]
pub(crate) fn builtin_process_send_string_impl(
    processes: &mut ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-send-string", &args, 2)?;
    let input = args[1]
        .as_lisp_string()
        .cloned()
        .ok_or_else(|| signal_wrong_type_string(args[1]))?;
    if let Some(id) = process_value_to_id(&args[0])
        && is_stale_process_id_designator_in_manager(processes, &args[0])
    {
        return Err(signal_process_not_running_in_manager(processes, id));
    }
    let id = resolve_get_process_designator_in_state(processes, buffers, &args[0])?;
    if processes
        .observe(UpdateStatusSite::SendProcess, id)
        .is_some_and(|observed| !observed.allows_send())
    {
        return Err(signal_process_not_running_in_manager(processes, id));
    }
    // GNU `send_process` (src/process.c) encodes the data through the process's
    // ENCODE coding system before writing it to the child's fd, applying both
    // character-code and EOL conversion.  Encode the input here so a process
    // whose output coding requests CRLF/CR (e.g. `dos`/`mac`/`utf-8-dos`) sends
    // the converted bytes; binary/raw-text encode systems pass the bytes
    // through unchanged.
    let encoded = encode_process_send_input(
        processes,
        id,
        &input,
        crate::emacs_core::coding::EolConversion::Enabled,
    );
    if !processes.send_input(id, &encoded)? {
        return Err(signal("error", vec![Value::string("Process not found")]));
    }
    Ok(Value::NIL)
}

/// (process-status PROCESS) -> symbol
pub(crate) fn builtin_process_status(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_status_impl(&mut eval.processes, &eval.buffers, args)
}

pub(crate) fn builtin_process_status_impl(
    processes: &mut ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-status", &args, 1)?;
    let Some(id) = resolve_process_for_status_in_state(processes, buffers, &args[0])? else {
        return Ok(Value::NIL);
    };
    // GNU `Fprocess_status` runs `update_status` when `raw_status_new` is set
    // (src/process.c:1188-1189), and `raw_status_new` is already set by then
    // because `handle_child_signal` recorded it from the SIGCHLD handler
    // (:7746-7747).  This port cannot record from a handler, so `observe`
    // makes GNU's recording here instead -- see `process/child_status.rs`.
    // `process-live-p` rides on this one: lisp/subr.el:3538-3540 defines it
    // as `(memq (process-status process) '(run open listen connect stop))`.
    match processes.observe(UpdateStatusSite::ProcessStatus, id) {
        Some(observed) => Ok(observed.public_status_symbol()),
        None => Ok(Value::NIL),
    }
}

/// (process-exit-status PROCESS) -> integer
pub(crate) fn builtin_process_exit_status(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_exit_status_impl(&mut eval.processes, args)
}

pub(crate) fn builtin_process_exit_status_impl(
    processes: &mut ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-exit-status", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    // GNU `Fprocess_exit_status` runs `update_status` too (src/process.c:
    // 1212-1213), on a record `handle_child_signal` has already made.  Same
    // recording as `builtin_process_status_impl`; see `child_status.rs`.
    let observed = processes
        .observe(UpdateStatusSite::ProcessExitStatus, id)
        .ok_or_else(|| signal_wrong_type_processp(args[0]))?;
    let status = observed.settled_status();
    let proc = observed.process();
    match ProcessStatusSymbol::from_status_value(status) {
        Some(ProcessStatusSymbol::Exit) => Ok(Value::fixnum(process_status_code_value(status))),
        Some(ProcessStatusSymbol::Failed) => Ok(Value::fixnum(process_status_code_value(status))),
        Some(ProcessStatusSymbol::Signal) => {
            if proc.kind == ProcessKind::Real {
                Ok(Value::fixnum(process_status_code_value(status)))
            } else {
                Ok(Value::fixnum(0))
            }
        }
        Some(ProcessStatusSymbol::Stop) => {
            if proc.kind == ProcessKind::Real {
                Ok(Value::fixnum(process_status_code_value(status)))
            } else {
                Ok(Value::fixnum(0))
            }
        }
        _ => Ok(Value::fixnum(0)),
    }
}

/// (process-list) -> list of process ids
pub(crate) fn builtin_process_list(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_list_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_list_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-list", &args, 0)?;
    let ids = processes.list_processes();
    let values: Vec<Value> = ids.iter().map(|id| Value::make_process(*id)).collect();
    Ok(Value::list(values))
}

/// (process-name PROCESS) -> string
pub(crate) fn builtin_process_name(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_name_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_name_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-name", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    match processes.get_any(id) {
        Some(proc) => Ok(proc.name),
        None => Err(signal_wrong_type_processp(args[0])),
    }
}

/// (process-buffer PROCESS) -> buffer or nil
pub(crate) fn builtin_process_buffer(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_buffer_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_buffer_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-buffer", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    match processes.get_any(id) {
        Some(proc) => Ok(proc.buffer),
        None => Err(signal_wrong_type_processp(args[0])),
    }
}

/// (process-coding-system PROCESS) -> (decode . encode)
pub(crate) fn builtin_process_coding_system(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_coding_system_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_coding_system_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-coding-system", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let proc = processes.get_any(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    Ok(Value::cons(proc.coding_decode, proc.coding_encode))
}

/// (process-datagram-address PROCESS) -> address-or-nil
pub(crate) fn builtin_process_datagram_address(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() == 1
        && let Some(id) = pending_network_connect_id(&eval.processes, args[0])?
    {
        eval.wait_while_network_process_connecting(id)?;
    }
    builtin_process_datagram_address_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_datagram_address_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-datagram-address", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let Some(proc) = processes.get_any(id) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        ));
    };
    if process_has_datagram_semantics(proc) {
        Ok(proc.datagram_address)
    } else {
        Ok(Value::NIL)
    }
}

/// (process-inherit-coding-system-flag PROCESS) -> bool
pub(crate) fn builtin_process_inherit_coding_system_flag(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_inherit_coding_system_flag_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_inherit_coding_system_flag_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-inherit-coding-system-flag", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let proc = processes.get_any(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    Ok(Value::bool_val(proc.inherit_coding_system_flag))
}

/// (set-process-buffer PROCESS BUFFER) -> BUFFER
pub(crate) fn builtin_set_process_buffer(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_set_process_buffer_impl(&mut eval.processes, &mut eval.buffers, args)
}

pub(crate) fn builtin_set_process_buffer_impl(
    processes: &mut ProcessManager,
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-process-buffer", &args, 2)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    // `if (!NILP (buffer)) CHECK_BUFFER (buffer);` (src/process.c:1302-1303),
    // and `CHECK_BUFFER` is `CHECK_TYPE (BUFFERP (x), Qbufferp, x)` and
    // nothing else (src/buffer.h:762-766).  A DEAD buffer is a buffer, and
    // handing one over is how a process legitimately reaches the state
    // `read_and_insert_process_output` (:6464),
    // `internal-default-process-sentinel` (:7969-7971) and
    // `setup_process_coding_systems` (:8395) each guard against.
    match args[1].kind() {
        ValueKind::Nil | ValueKind::Veclike(VecLikeType::Buffer) => {}
        _ => return Err(signal_wrong_type_bufferp(args[1])),
    };
    let proc = processes.get_any_mut(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    if proc.buffer != args[1] {
        proc.buffer = args[1];
        update_process_mark(buffers, proc)?;
    }
    if process_uses_contact_plist(proc) {
        proc.childp =
            process_contact_plist_put(proc.childp, ProcessKeyword::Buffer.value(), args[1])?;
    }
    Ok(args[1])
}

/// (set-process-coding-system PROCESS &optional DECODING ENCODING) -> nil
pub(crate) fn builtin_set_process_coding_system(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_set_process_coding_system_impl(&mut eval.processes, &eval.coding_systems, args)
}

pub(crate) fn builtin_set_process_coding_system_impl(
    processes: &mut ProcessManager,
    coding_systems: &super::super::coding::CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("set-process-coding-system", &args, 1)?;
    if args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("set-process-coding-system"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    // GNU `Fset_process_coding_system` (src/process.c): CHECK_PROCESS first,
    // then DECODING and ENCODING (both defaulting to nil) are validated, then
    // ENCODING (only) is passed through `coding_inherit_eol_type` so a
    // nil/undecided-EOL encode coding normalizes (e.g. nil -> raw-text-unix,
    // utf-8 -> utf-8-unix). DECODING is stored as-is (nil stays nil).
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let decoding = args.get(1).cloned().unwrap_or(Value::NIL);
    let encoding = args.get(2).cloned().unwrap_or(Value::NIL);
    super::super::coding::builtin_check_coding_system(coding_systems, vec![decoding])?;
    super::super::coding::builtin_check_coding_system(coding_systems, vec![encoding])?;
    let encoding = super::super::coding::coding_inherit_eol_type_unix(coding_systems, encoding);

    let proc = processes.get_any_mut(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    proc.coding_decode = decoding;
    // GNU `set-process-coding-system` ends in `setup_process_coding_systems`
    // (src/process.c:8036), which re-runs `setup_coding_system` -- and that
    // zeroes both `coding->mode` (:5683, so the `CODING_MODE_LAST_BLOCK` latch
    // goes down) and `coding->carryover_bytes` (:5703).
    proc.coding_state.reset();
    proc.coding_encode = encoding;
    proc.coding_explicitly_set = true;
    Ok(Value::NIL)
}

/// (set-process-datagram-address PROCESS ADDRESS) -> nil
pub(crate) fn builtin_set_process_datagram_address(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() == 2
        && let Some(id) = pending_network_connect_id(&eval.processes, args[0])?
    {
        eval.wait_while_network_process_connecting(id)?;
    }
    builtin_set_process_datagram_address_impl(&mut eval.processes, args)
}

pub(crate) fn builtin_set_process_datagram_address_impl(
    processes: &mut ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-process-datagram-address", &args, 2)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let Some(proc) = processes.get_any_mut(id) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        ));
    };
    match proc.live_io.network_socket.as_ref() {
        Some(NetworkSocket::UdpSocket(socket)) => {
            let Ok(NetworkAddressSpec::Inet(addr)) = parse_network_address_spec(&args[1]) else {
                return Ok(Value::NIL);
            };
            let family_matches = socket
                .local_addr()
                .ok()
                .map(|local_addr| local_addr.is_ipv4() == addr.is_ipv4())
                .or_else(|| {
                    proc.datagram_socket_addr
                        .map(|remote_addr| remote_addr.is_ipv4() == addr.is_ipv4())
                })
                .unwrap_or(true);
            if !family_matches {
                return Ok(Value::NIL);
            }
            proc.datagram_socket_addr = Some(addr);
            proc.datagram_address = socket_addr_to_lisp_value(addr);
            Ok(args[1])
        }
        #[cfg(unix)]
        Some(NetworkSocket::UnixDatagram(_)) => {
            let Ok(NetworkAddressSpec::Local(path)) = parse_network_address_spec(&args[1]) else {
                return Ok(Value::NIL);
            };
            proc.datagram_unix_path = Some(path);
            proc.datagram_address = args[1];
            Ok(args[1])
        }
        _ => Ok(Value::NIL),
    }
}

/// (set-process-inherit-coding-system-flag PROCESS FLAG) -> FLAG
pub(crate) fn builtin_set_process_inherit_coding_system_flag(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_set_process_inherit_coding_system_flag_impl(&mut eval.processes, args)
}

pub(crate) fn builtin_set_process_inherit_coding_system_flag_impl(
    processes: &mut ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-process-inherit-coding-system-flag", &args, 2)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let proc = processes.get_any_mut(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    proc.inherit_coding_system_flag = args[1].is_truthy();
    Ok(args[1])
}

/// (set-process-thread PROCESS THREAD) -> thread-or-nil
pub(crate) fn builtin_set_process_thread(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_set_process_thread_impl(&mut eval.processes, &eval.threads, args)
}

pub(crate) fn builtin_set_process_thread_impl(
    processes: &mut ProcessManager,
    threads: &ThreadManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-process-thread", &args, 2)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let value = if args[1].is_nil() {
        Value::NIL
    } else if threads.thread_id_from_handle(&args[1]).is_some() {
        args[1]
    } else {
        return Err(signal_wrong_type_threadp(args[1]));
    };
    let proc = processes.get_any_mut(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    proc.thread = value;
    Ok(value)
}

/// (set-process-window-size PROCESS HEIGHT WIDTH) -> t-or-nil
pub(crate) fn builtin_set_process_window_size(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_set_process_window_size_impl(&mut eval.processes, args)
}

pub(crate) fn builtin_set_process_window_size_impl(
    processes: &mut ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-process-window-size", &args, 3)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let height = expect_ushort_dimension(&args[1])?;
    let width = expect_ushort_dimension(&args[2])?;
    let is_live = processes.get(id).is_some();
    let proc = processes.get_any_mut(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    proc.window_rows = Some(i64::from(height));
    proc.window_cols = Some(i64::from(width));
    if !is_live {
        return Ok(Value::NIL);
    }
    if let Some(ref pty_master) = proc.live_io.pty_master {
        let pty_size = portable_pty::PtySize {
            rows: height,
            cols: width,
            pixel_width: 0,
            pixel_height: 0,
        };
        return Ok(Value::bool_val(pty_master.resize(pty_size).is_ok()));
    }
    if proc.kind == ProcessKind::Real && !process_has_subprocess_backing(proc) {
        return Ok(Value::T);
    }
    Ok(Value::NIL)
}

/// (process-menu-visit-buffer LINE) -> nil
/// (process-tty-name PROCESS &optional STREAM) -> string-or-nil
pub(crate) fn builtin_process_tty_name(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_tty_name_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_tty_name_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("process-tty-name", &args, 1)?;
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("process-tty-name"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let proc = processes.get_any(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    let stream = args.get(1).cloned().unwrap_or(Value::NIL);
    let tty_value = || proc.tty_name;

    match ProcessTtyStream::from_value(&stream) {
        None if stream.is_nil() => Ok(tty_value()),
        Some(ProcessTtyStream::Stdin) => {
            if proc.tty_stdin {
                Ok(tty_value())
            } else {
                Ok(Value::NIL)
            }
        }
        Some(ProcessTtyStream::Stdout) => {
            if proc.tty_stdout {
                Ok(tty_value())
            } else {
                Ok(Value::NIL)
            }
        }
        Some(ProcessTtyStream::Stderr) => {
            if proc.tty_stderr && proc.stderrproc.is_nil() {
                Ok(tty_value())
            } else {
                Ok(Value::NIL)
            }
        }
        None => Err(signal(
            "error",
            vec![Value::string("Unknown stream"), stream],
        )),
    }
}

/// (process-mark PROCESS) -> marker
pub(crate) fn builtin_process_mark(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_mark_impl(&eval.processes, &eval.buffers, args)
}

pub(crate) fn builtin_process_mark_impl(
    processes: &ProcessManager,
    _buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-mark", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let proc = processes.get_any(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    Ok(proc.mark)
}

/// (process-type PROCESS) -> symbol
pub(crate) fn builtin_process_type(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_type_impl(&eval.processes, &eval.buffers, args)
}

pub(crate) fn builtin_process_type_impl(
    processes: &ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-type", &args, 1)?;
    let id = resolve_get_process_designator_in_state(processes, buffers, &args[0])?;
    let proc = processes.get_any(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    Ok(proc.proc_type)
}

/// (process-thread PROCESS) -> object-or-nil
pub(crate) fn builtin_process_thread(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_thread_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_thread_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-thread", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let proc = processes.get_any(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    Ok(proc.thread)
}

/// (process-send-region PROCESS START END) -> nil
pub(crate) fn builtin_process_send_region(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-send-region", &args, 3)?;

    if let Some(id) = process_value_to_id(&args[0])
        && is_stale_process_id_designator_in_manager(&eval.processes, &args[0])
    {
        let _ =
            super::super::position::LispRegionArgs::from_values(&eval.buffers, args[1], args[2])?;
        return Err(signal_process_not_running_in_manager(&eval.processes, id));
    }

    let id = resolve_get_process_designator_in_state(&eval.processes, &eval.buffers, &args[0])?;
    eval.wait_while_network_process_connecting(id)?;
    // `process-send-region` reaches GNU's `send_process` too, and therefore
    // the same `update_status` at :6726.
    if eval
        .processes
        .observe(UpdateStatusSite::SendProcess, id)
        .is_some_and(|observed| !observed.allows_send())
    {
        return Err(signal_process_not_running_in_manager(&eval.processes, id));
    }
    let region_args =
        super::super::position::LispRegionArgs::from_values(&eval.buffers, args[1], args[2])?;

    let region_text = {
        let buf = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let region = checked_region_bytes(buf, region_args)?;
        buf.buffer_substring_lisp_string_range(region)
    };

    let encoded =
        encode_process_send_input(&eval.processes, id, &region_text, eval.eol_conversion());
    eval.send_process_input_reentrant(id, &encoded)?;
    Ok(Value::NIL)
}

/// The `ProcessManager`-only spelling of `process-send-region`; `#[cfg(test)]`
/// for the reason [`builtin_process_send_string_impl`] gives.
#[cfg(test)]
pub(crate) fn builtin_process_send_region_impl(
    processes: &mut ProcessManager,
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-send-region", &args, 3)?;

    if let Some(id) = process_value_to_id(&args[0])
        && is_stale_process_id_designator_in_manager(processes, &args[0])
    {
        let _ = super::super::position::LispRegionArgs::from_values(&*buffers, args[1], args[2])?;
        return Err(signal_process_not_running_in_manager(processes, id));
    }

    let id = resolve_get_process_designator_in_state(processes, buffers, &args[0])?;
    if processes
        .observe(UpdateStatusSite::SendProcess, id)
        .is_some_and(|observed| !observed.allows_send())
    {
        return Err(signal_process_not_running_in_manager(processes, id));
    }
    let region_args =
        super::super::position::LispRegionArgs::from_values(&*buffers, args[1], args[2])?;

    let region_text = {
        let buf = buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let region = checked_region_bytes(buf, region_args)?;
        buf.buffer_substring_lisp_string_range(region)
    };

    // Encode the region text through the process's ENCODE coding system, exactly
    // like `process-send-string` (GNU `send_process`).
    let encoded = encode_process_send_input(
        processes,
        id,
        &region_text,
        crate::emacs_core::coding::EolConversion::Enabled,
    );
    if !processes.send_input(id, &encoded)? {
        return Err(signal("error", vec![Value::string("Process not found")]));
    }
    Ok(Value::NIL)
}

/// (process-send-eof &optional PROCESS) -> process-or-nil
pub(crate) fn builtin_process_send_eof(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() <= 1 {
        let maybe_id = args
            .first()
            .map(|process| {
                resolve_get_process_designator_in_state(&eval.processes, &eval.buffers, process)
            })
            .unwrap_or_else(|| {
                resolve_optional_process_or_current_buffer_in_state(
                    &eval.processes,
                    &eval.buffers,
                    None,
                )
            })
            .ok();
        if let Some(id) = maybe_id
            && eval.processes.get(id).is_some_and(|proc| {
                proc.kind == ProcessKind::Network && proc.live_io.pending_network_connect.is_some()
            })
        {
            eval.wait_while_network_process_connecting(id)?;
        }
        if let Some(id) = maybe_id
            && eval.processes.get(id).is_some_and(|proc| proc.tty_stdin)
        {
            if eval
                .processes
                .observe(UpdateStatusSite::ProcessSendEof, id)
                .is_some_and(|observed| !observed.allows_send())
            {
                return Err(signal_process_not_running_in_manager(&eval.processes, id));
            }
            if let Some(proc) = eval.processes.get_mut(id) {
                proc.eof_sent_to_process = true;
            }
            // GNU process.c sends an unencoded EOT byte through `send_process'
            // when the child's input is a PTY.  Route it through the same
            // reentrant write queue as ordinary input so EOF is ordered after
            // every byte already accepted by `process-send-string'.
            eval.send_process_input_reentrant(id, &LispString::from_unibyte(vec![0x04]))?;
            return Ok(args.first().copied().unwrap_or(Value::NIL));
        }
    }
    builtin_process_send_eof_impl(&mut eval.processes, &eval.buffers, args)
}

pub(super) fn send_eof_to_process(proc: &mut Process) -> EvalResult {
    // GNU returns a datagram process untouched before both the liveness gate
    // and every EOF state mutation (src/process.c:7444-7445).
    if process_has_datagram_semantics(proc) {
        return Ok(Value::NIL);
    }
    proc.eof_sent_to_process = true;

    // GNU's serial branch only drains the device; it neither closes the
    // writer nor replaces it with /dev/null (src/process.c:7470-7478).
    if proc.kind == ProcessKind::Serial {
        return Ok(Value::NIL);
    }

    // EPIPE already changed GNU's `outfd` to -1. `process-send-eof` still
    // enters the non-PTY/non-serial branch and installs /dev/null, but it does
    // not try to shut down the old descriptor because it is negative. Mirror
    // that semantic state transition without touching the retained readable
    // owner of a bidirectional Rust transport.
    if proc.input_disposition == ProcessInputDisposition::Disconnected {
        proc.input_disposition = ProcessInputDisposition::Discard;
        return Ok(Value::NIL);
    }

    if let Some(tls) = proc.live_io.tls_stream.as_mut() {
        tls.send_close_notify(false)
            .map(|_| ())
            .map_err(|err| signal_process_io("Sending EOF to process", None, err))?;
        proc.input_disposition = ProcessInputDisposition::Discard;
        return Ok(Value::NIL);
    }

    if let Some(socket) = proc.live_io.network_socket.as_ref() {
        if let Some(result) = socket.shutdown_write() {
            result.map_err(|err| signal_process_io("Sending EOF to process", None, err))?;
            proc.input_disposition = ProcessInputDisposition::Discard;
        }
        return Ok(Value::NIL);
    }

    let _ = proc.live_io.child.close_stdin();
    // GNU opens the null output device even when there was no old descriptor
    // to close. `Discard` models that sink without allocating an OS fd.
    proc.input_disposition = ProcessInputDisposition::Discard;
    Ok(Value::NIL)
}

pub(crate) fn builtin_process_send_eof_impl(
    processes: &mut ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 1 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("process-send-eof"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    if let Some(process) = args.first()
        && !process.is_nil()
    {
        if let Some(id) = process_value_to_id(process)
            && is_stale_process_id_designator_in_manager(processes, process)
        {
            return Err(signal_process_not_running_in_manager(processes, id));
        }
        let id = resolve_get_process_designator_in_state(processes, buffers, process)?;
        process_send_eof_liveness_gate(processes, id)?;
        if let Some(proc) = processes.get_mut(id) {
            send_eof_to_process(proc)?;
        }
        return Ok(*process);
    }
    let id = resolve_optional_process_or_current_buffer_in_state(processes, buffers, args.first())?;
    process_send_eof_liveness_gate(processes, id)?;
    if let Some(proc) = processes.get_mut(id) {
        send_eof_to_process(proc)?;
    }
    Ok(Value::NIL)
}

/// GNU `Fprocess_send_eof`'s "Make sure the process is really alive" gate
/// (src/process.c:7451-7455), which this port did not have on the non-pty
/// path at all -- `process-send-eof` answered `ok` for a child that had
/// exited where GNU raises `Process NAME not running: finished`.
///
/// The datagram exemption above it is GNU's own: `Fprocess_send_eof` returns
/// the process untouched for a datagram connection at :7444-7445, BEFORE the
/// gate, so a datagram is never rejected by it.
pub(super) fn process_send_eof_liveness_gate(
    processes: &mut ProcessManager,
    id: ProcessId,
) -> Result<(), Flow> {
    if processes
        .get(id)
        .is_some_and(process_has_datagram_semantics)
    {
        return Ok(());
    }
    if processes
        .observe(UpdateStatusSite::ProcessSendEof, id)
        .is_some_and(|observed| !observed.allows_send())
    {
        return Err(signal_process_not_running_in_manager(processes, id));
    }
    Ok(())
}

/// (process-running-child-p &optional PROCESS) -> bool
pub(crate) fn builtin_process_running_child_p(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_running_child_p_impl(&eval.processes, &eval.buffers, args)
}

#[cfg(unix)]
pub(super) fn process_tty_foreground_group(proc: &Process) -> Option<i32> {
    if let Some(gid) = proc
        .live_io
        .pty_master
        .as_ref()
        .and_then(|master| master.as_raw_fd())
        .and_then(sys::fd_foreground_pgrp)
    {
        return Some(gid);
    }

    let tty_name = proc.tty_name.as_lisp_string()?;
    if tty_name.as_bytes().is_empty() {
        return None;
    }
    let tty_path = lisp_string_to_os_string(tty_name);
    sys::tty_path_foreground_pgrp(tty_path.as_os_str())
}

#[cfg(not(unix))]
pub(super) fn process_tty_foreground_group(_proc: &Process) -> Option<i32> {
    None
}

pub(super) fn process_running_child_value(proc: &Process) -> Value {
    if !process_has_subprocess_backing(proc) {
        return Value::NIL;
    }
    if let Some(gid) = process_tty_foreground_group(proc) {
        if proc.os_pid.is_some_and(|pid| pid as i64 == gid as i64) {
            Value::NIL
        } else {
            Value::fixnum(gid as i64)
        }
    } else {
        Value::T
    }
}

pub(crate) fn builtin_process_running_child_p_impl(
    processes: &ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 1 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("process-running-child-p"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    // GNU checks `!EQ (p->type, Qreal)` at src/process.c:7042-7044 BEFORE
    // `p->infd < 0` at :7045-7047, so a pipe answers "is not a subprocess"
    // however long it has been dead.
    if let Some(process) = args.first()
        && let Some(id) = process_value_to_id(process)
        && is_stale_real_process_designator_in_manager(processes, process)
    {
        return Err(signal_process_not_active_in_manager(processes, id));
    }
    let id = resolve_optional_process_or_current_buffer_in_state(processes, buffers, args.first())?;
    let proc = processes
        .get_any(id)
        .ok_or_else(|| signal_process_not_active_in_manager(processes, id))?;
    if proc.kind != ProcessKind::Real {
        return Err(signal_process_not_subprocess(proc));
    }
    if processes.get(id).is_none() {
        return Err(signal_process_not_active_in_manager(processes, id));
    }
    Ok(process_running_child_value(proc))
}

/// (accept-process-output &optional PROCESS SECONDS MILLISECS JUST-THIS-ONE) -> bool
pub(crate) fn builtin_accept_process_output(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let Some(request) = parse_accept_process_output_request(&mut eval.processes, &args)? else {
        return Ok(Value::NIL);
    };

    match eval.wait_for_process_output(request.wait)? {
        ProcessOutputWaitOutcome::ProcessActivity => Ok(Value::T),
        ProcessOutputWaitOutcome::NoProcessActivity => Ok(Value::NIL),
    }
}

/// (get-process NAME) -> process-or-nil
pub(crate) fn builtin_get_process(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_get_process_impl(&eval.processes, args)
}

pub(crate) fn builtin_get_process_impl(processes: &ProcessManager, args: Vec<Value>) -> EvalResult {
    expect_args("get-process", &args, 1)?;
    // GNU `Fget_process`: a process object is returned unchanged; otherwise the
    // argument must be a name string.
    if args[0].is_process() {
        return Ok(args[0]);
    }
    let name = expect_string_strict(&args[0])?;
    match processes.find_by_name(&name) {
        Some(id) => Ok(Value::make_process(id)),
        None => Ok(Value::NIL),
    }
}

/// (get-buffer-process BUFFER-OR-NAME) -> process-or-nil
pub(crate) fn builtin_get_buffer_process(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_get_buffer_process_impl(&eval.buffers, &eval.processes, args)
}

pub(crate) fn builtin_get_buffer_process_impl(
    buffers: &BufferManager,
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("get-buffer-process", &args, 1)?;
    let Some(buffer_id) = resolve_buffer_for_process_lookup_in_state(buffers, &args[0])? else {
        return Ok(Value::NIL);
    };
    match processes.find_by_buffer_id(buffer_id) {
        Some(id) => Ok(Value::make_process(id)),
        None => Ok(Value::NIL),
    }
}

/// (processp OBJECT) -> bool
pub(crate) fn builtin_processp(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_processp_impl(&eval.processes, args)
}

pub(crate) fn builtin_processp_impl(_processes: &ProcessManager, args: Vec<Value>) -> EvalResult {
    expect_args("processp", &args, 1)?;
    // GNU `Fprocessp` is purely structural: any process object is `t`, even
    // after it has exited (it stays a process object).  A bare integer is not a
    // process.
    Ok(Value::bool_val(args[0].is_process()))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_process_live_p_impl(
    processes: &mut ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-live-p", &args, 1)?;
    let Some(id) = process_value_to_id(&args[0]).and_then(|id| processes.get(id).map(|_| id))
    else {
        return Ok(Value::NIL);
    };
    // Keep this a recorded-state query, like GNU's process predicates.  Wait
    // paths observe child exits and update `status`; `process-live-p` must not
    // perform a fresh no-wait child probe on its own.
    let proc = processes.get(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    Ok(process_live_status_value(proc))
}

/// (process-id PROCESS) -> integer
pub(crate) fn builtin_process_id(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_id_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_id_impl(processes: &ProcessManager, args: Vec<Value>) -> EvalResult {
    expect_args("process-id", &args, 1)?;
    // GNU `Fprocess_id` uses CHECK_PROCESS — it requires a genuine process
    // object (no name-string designator), so resolve structurally only.
    let id = process_value_to_id(&args[0])
        .filter(|id| processes.get_any(*id).is_some())
        .ok_or_else(|| signal_wrong_type_processp(args[0]))?;
    let proc = processes
        .get_any(id)
        .ok_or_else(|| signal_wrong_type_processp(args[0]))?;
    // GNU `Fprocess_id` returns the child's real OS pid as an integer
    // (`XPROCESS (process)->pid`), or nil when there is none (pid == 0), as
    // for network/serial/pipe connections.  The internal `ProcessId` used to
    // key the manager is kept separate and never exposed here.
    match proc.os_pid {
        Some(pid) => Ok(Value::fixnum(i64::from(pid))),
        None => Ok(Value::NIL),
    }
}

/// (process-query-on-exit-flag PROCESS) -> bool
pub(crate) fn builtin_process_query_on_exit_flag(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_query_on_exit_flag_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_query_on_exit_flag_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-query-on-exit-flag", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let proc = processes.get_any(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    Ok(Value::bool_val(proc.exit_query_policy.queries_on_exit()))
}

/// (set-process-query-on-exit-flag PROCESS FLAG) -> FLAG
pub(crate) fn builtin_set_process_query_on_exit_flag(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_set_process_query_on_exit_flag_impl(&mut eval.processes, args)
}

pub(crate) fn builtin_set_process_query_on_exit_flag_impl(
    processes: &mut ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-process-query-on-exit-flag", &args, 2)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let policy = ExitQueryPolicy::from_lisp_query_flag(args[1]);
    let proc = processes.get_any_mut(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    proc.exit_query_policy = policy;
    Ok(args[1])
}

/// (process-command PROCESS) -> list
pub(crate) fn builtin_process_command(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_command_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_command_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-command", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let proc = processes.get_any(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    Ok(proc.command)
}

/// (process-contact PROCESS &optional KEY NO-BLOCK) -> value
pub(crate) fn builtin_process_contact(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if (1..=3).contains(&args.len()) {
        let no_block = args.get(2).is_some_and(|value| value.is_truthy());
        if let Some(id) = pending_network_connect_id(&eval.processes, args[0])? {
            if no_block {
                return Ok(Value::NIL);
            }
            eval.wait_while_network_process_connecting(id)?;
        }
    }
    builtin_process_contact_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_contact_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("process-contact", &args, 1)?;
    if args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("process-contact"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let proc = processes.get_any(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    let key = args.get(1).copied().unwrap_or(Value::NIL);
    let mut contact = proc.childp;
    match proc.proc_type.as_symbol_name() {
        Some("network") => {
            if process_is_datagram_network(proc)
                && (key == Value::T || key == ProcessKeyword::Remote.value())
            {
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Remote.value(),
                    proc.datagram_address,
                )?;
            }
            if key == Value::T {
                Ok(contact)
            } else if key.is_nil() {
                Ok(Value::list(vec![
                    process_contact_plist_get(contact, ProcessKeyword::Host.value()),
                    process_contact_plist_get(contact, ProcessKeyword::Service.value()),
                ]))
            } else {
                Ok(process_contact_plist_get(contact, key))
            }
        }
        Some("serial") => {
            if key == Value::T {
                Ok(contact)
            } else if key.is_nil() {
                Ok(Value::list(vec![
                    process_contact_plist_get(contact, ProcessKeyword::Port.value()),
                    process_contact_plist_get(contact, ProcessKeyword::Speed.value()),
                ]))
            } else {
                Ok(process_contact_plist_get(contact, key))
            }
        }
        Some("pipe") => {
            if key == Value::T {
                Ok(contact)
            } else if key.is_nil() {
                Ok(Value::T)
            } else {
                Ok(process_contact_plist_get(contact, key))
            }
        }
        _ => Ok(contact),
    }
}

/// (process-filter PROCESS) -> function
pub(crate) fn builtin_process_filter(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_filter_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_filter_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-filter", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let proc = processes.get_any(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    Ok(proc.filter)
}

/// (set-process-filter PROCESS FILTER) -> FILTER
pub(crate) fn builtin_set_process_filter(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_set_process_filter_impl(&mut eval.processes, args)
}

pub(crate) fn builtin_set_process_filter_impl(
    processes: &mut ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-process-filter", &args, 2)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let stored = if args[1].is_nil() {
        Value::symbol(DEFAULT_PROCESS_FILTER_SYMBOL)
    } else {
        args[1]
    };
    let (accepted_output_before, accepts_output_after) = {
        let proc = processes.get_any_mut(id).ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("processp"), args[0]],
            )
        })?;
        let accepted_output_before = process_filter_accepts_output(proc);
        proc.filter = stored;
        if process_uses_contact_plist(proc) {
            proc.childp =
                process_contact_plist_put(proc.childp, ProcessKeyword::Filter.value(), stored)?;
        }
        (accepted_output_before, process_filter_accepts_output(proc))
    };
    if accepted_output_before != accepts_output_after {
        processes.set_process_output_read_interest(id, accepts_output_after);
    }
    Ok(stored)
}

/// (process-sentinel PROCESS) -> function
pub(crate) fn builtin_process_sentinel(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_sentinel_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_sentinel_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-sentinel", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let proc = processes.get_any(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    Ok(proc.sentinel)
}

/// (set-process-sentinel PROCESS SENTINEL) -> SENTINEL
pub(crate) fn builtin_set_process_sentinel(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_set_process_sentinel_impl(&mut eval.processes, args)
}

pub(crate) fn builtin_set_process_sentinel_impl(
    processes: &mut ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-process-sentinel", &args, 2)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let stored = if args[1].is_nil() {
        Value::symbol(DEFAULT_PROCESS_SENTINEL_SYMBOL)
    } else {
        args[1]
    };
    let proc = processes.get_any_mut(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    proc.sentinel = stored;
    if process_uses_contact_plist(proc) {
        proc.childp =
            process_contact_plist_put(proc.childp, ProcessKeyword::Sentinel.value(), stored)?;
    }
    Ok(stored)
}

/// (process-plist PROCESS) -> plist
pub(crate) fn builtin_process_plist(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_plist_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_plist_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-plist", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let proc = processes.get_any(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    Ok(proc.plist)
}

/// (set-process-plist PROCESS PLIST) -> plist
pub(crate) fn builtin_set_process_plist(
    eval: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_set_process_plist_impl(&mut eval.processes, args)
}

pub(crate) fn builtin_set_process_plist_impl(
    processes: &mut ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-process-plist", &args, 2)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    if !args[1].is_list() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), args[1]],
        ));
    }
    let proc = processes.get_any_mut(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    proc.plist = args[1];
    Ok(proc.plist)
}

// ---------------------------------------------------------------------------
// Builtins (pure — no evaluator needed)
// ---------------------------------------------------------------------------

pub(crate) fn make_network_process_subfeatures() -> Option<Value> {
    // Advertise only behavior that this runtime actually implements.  Packages
    // use `featurep' to choose code paths, so keep this list tied to backed
    // behavior, not parser acceptance.
    let mut features = vec![
        Value::keyword("nodelay"),
        Value::keyword("reuseaddr"),
        Value::keyword("oobinline"),
        Value::keyword("linger"),
        Value::keyword("keepalive"),
        Value::keyword("dontroute"),
        Value::keyword("broadcast"),
        // GNU's eight `ADD_SUBFEATURE' calls, in the order the finished list
        // reads.  `src/process.c:9072-9089' conses each onto the front, so the
        // list is the REVERSE of the source order: `:nowait' is added first and
        // ends up last, `:server' is added last and ends up first.  Ledger 197
        // put these in GNU's order; before it they ran the other way, which no
        // check could see because the only comparison against GNU
        // (`crates/neovm-oracle-tests/src/process/feature_advertisement_semantics.rs:28')
        // `sort's the list first, and a sorted comparison is a comparison of
        // sets.
        Value::list(vec![Value::keyword("server"), Value::T]),
        Value::list(vec![Value::keyword("service"), Value::T]),
        Value::list(vec![Value::keyword("family"), Value::symbol("ipv6")]),
        Value::list(vec![Value::keyword("family"), Value::symbol("ipv4")]),
        Value::list(vec![Value::keyword("family"), Value::symbol("local")]),
        // Local SOCK_SEQPACKET connections are fully backed (server accept +
        // client + data delivery verified against GNU); GNU advertises this
        // under HAVE_SEQPACKET (process.c `ADD_SUBFEATURE (QCtype,
        // Qseqpacket)`).
        Value::list(vec![Value::keyword("type"), Value::symbol("seqpacket")]),
        Value::list(vec![Value::keyword("type"), Value::symbol("datagram")]),
        Value::list(vec![Value::keyword("nowait"), Value::T]),
    ];
    cfg_select! {
        any(target_os = "linux", target_os = "android") => {
            features.insert(2, Value::keyword("priority"));
            features.insert(8, Value::keyword("bindtodevice"));
        }
        _ => {}
    }
    Some(Value::list(features))
}

impl GcTrace for ProcessManager {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        for process in self
            .processes
            .values()
            .chain(self.deleted_processes.values())
        {
            roots.push(process.name);
            roots.push(process.proc_type);
            roots.push(process.buffer);
            roots.push(process.mark);
            roots.push(process.command);
            roots.push(process.childp);
            roots.push(process.status);
            roots.push(process.pending_status);
            roots.push(process.tty_name);
            roots.push(process.write_queue);
            roots.push(process.filter);
            roots.push(process.sentinel);
            roots.push(process.log);
            roots.push(process.plist);
            roots.push(process.stderrproc);
            roots.push(process.datagram_address);
            roots.push(process.coding_decode);
            roots.push(process.coding_encode);
            roots.push(process.thread);
            roots.push(process.gnutls_boot_parameters);
        }
    }
}
