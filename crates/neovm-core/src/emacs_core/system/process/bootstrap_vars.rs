//! Process bootstrap variables: register_bootstrap_vars and the C-level DEFVARs that mirror GNU src/process.c's syms_of_process.
//!
//! Moved out of `mod.rs` unchanged; a child module so it keeps the
//! parent's view of its private items (`use super::*`).

use super::*;

pub fn register_bootstrap_vars(obarray: &mut super::super::symbol::Obarray) {
    obarray.set_symbol_value("process-connection-type", Value::T);
    obarray.make_special("process-connection-type");
    // GNU `process.c` `syms_of_process` DEFVAR_LISPs
    // `process-adaptive-read-buffering` (default nil); it controls the
    // short-read delay heuristic in `read_process_output` and is set per
    // process at `start-process'/`make-process' time
    // (`p->adaptive_read_buffering`).  It must be *bound* (to nil) so that
    // `(boundp 'process-adaptive-read-buffering)` is t and reading the
    // variable does not signal `void-variable`; e.g. `tramp-sh.el` binds it
    // with `(let ((process-adaptive-read-buffering nil)) ...)`.  Without this
    // DEFVAR, code that reads the variable before calling a (non-existent)
    // helper sees `void-variable` instead of reaching the real error.
    obarray.set_symbol_value("process-adaptive-read-buffering", Value::NIL);
    obarray.make_special("process-adaptive-read-buffering");
    obarray.set_symbol_value(
        "interrupt-process-functions",
        Value::list(vec![Value::symbol("internal-default-interrupt-process")]),
    );
    obarray.make_special("interrupt-process-functions");
    obarray.set_symbol_value(
        "signal-process-functions",
        Value::list(vec![Value::symbol("internal-default-signal-process")]),
    );
    obarray.make_special("signal-process-functions");
    obarray.set_symbol_value("internal--daemon-sockname", Value::NIL);
    obarray.make_special("internal--daemon-sockname");
    obarray.define_int_variable("read-process-output-max", 65536);
    obarray.define_int_variable("process-error-pause-time", 1);
    // GNU `gnutls.c` provides this via `DEFVAR_INT ("gnutls-log-level",
    // global_gnutls_log_level)` (default 0).  `gnutls.el` only forward-declares
    // it (`(defvar gnutls-log-level)  ; gnutls.c`), so without the C-side
    // definition it is void and `gnutls-negotiate` errors on
    // `:loglevel ,gnutls-log-level` before it ever reaches the (working,
    // TLS-capable) `gnutls-boot` -- breaking every package download and
    // thus `use-package`.  See https://github.com/eval-exec/neomacs/issues/121.
    obarray.define_int_variable("gnutls-log-level", 0);
    // GNU `gnutls.c` always DEFVAR_LISPs `libgnutls-version`; when Emacs is
    // built without libgnutls, the documented value is -1.  Neomacs exposes a
    // `gnutls-boot` compatibility API over Rust TLS rather than linking
    // libgnutls, so keep the variable bound without pretending to have a
    // libgnutls version.  `nsm.el` reads this during HTTPS package refresh.
    obarray.set_symbol_value("libgnutls-version", Value::fixnum(-1));
    obarray.make_special("libgnutls-version");
    for (symbol, code) in [
        ("gnutls-e-interrupted", -52),
        ("gnutls-e-again", -28),
        ("gnutls-e-invalid-session", -10),
        ("gnutls-e-not-ready-for-handshake", -65500),
    ] {
        obarray
            .put_property(symbol, "gnutls-code", Value::fixnum(code))
            .expect("bootstrap gnutls-code plist should be well formed");
    }
}

/// Check whether `process-connection-type` is truthy (non-nil).
///
/// GNU Emacs defaults this to `t`, meaning processes should use PTYs.
/// When nil, pipe-based I/O is used instead.
pub(super) fn process_connection_type_is_pty(obarray: &super::super::symbol::Obarray) -> bool {
    match obarray.symbol_value("process-connection-type") {
        Some(v) if v.is_nil() => false,
        Some(_) => true,
        // Default is t (PTY) when the variable has not been set.
        None => true,
    }
}

pub(super) fn signal_wrong_type_bufferp(value: Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("bufferp"), value],
    )
}

pub(super) fn signal_wrong_type_threadp(value: Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("threadp"), value],
    )
}

pub(super) fn signal_wrong_type_integerp(value: Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("integerp"), value],
    )
}

pub(super) fn signal_wrong_type_numberp(value: Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("numberp"), value],
    )
}

pub(super) fn signal_process_attributes_pid_range_error() -> Flow {
    signal(
        "error",
        vec![Value::string(
            "Not an in-range integer, integral float, or cons of integers",
        )],
    )
}

pub(super) fn signal_undefined_signal_name(name: &str) -> Flow {
    signal(
        "error",
        vec![Value::string(format!("Undefined signal name {name}"))],
    )
}

pub(super) fn resolve_optional_process_with_explicit_return_in_state(
    processes: &ProcessManager,
    buffers: &BufferManager,
    value: Option<&Value>,
) -> Result<(ProcessId, Value), Flow> {
    // `kill-process`, `stop-process`, `continue-process`, `quit-process` and
    // `interrupt-process' all reach GNU's `process_send_signal', whose TYPE
    // check precedes its `p->infd < 0' check -- and the two connection-shaped
    // subrs never reach it at all.  See
    // `is_stale_real_process_designator_in_manager'.
    if let Some(v) = value
        && !v.is_nil()
        && is_stale_real_process_designator_in_manager(processes, v)
        && let Some(id) = process_value_to_id(v)
    {
        return Err(signal_process_not_active_in_manager(processes, id));
    }
    if let Some(v) = value
        && !v.is_nil()
    {
        let id = resolve_get_process_designator_in_state(processes, buffers, v)?;
        return Ok((id, *v));
    }
    let id = resolve_optional_process_or_current_buffer_in_state(processes, buffers, value)?;
    Ok((id, Value::NIL))
}

pub(super) enum SignalProcessTarget {
    Process(ProcessId),
    MissingNamedProcess,
    Pid(i64),
}

pub(super) fn resolve_signal_process_target_in_state(
    processes: &ProcessManager,
    buffers: &BufferManager,
    value: Option<&Value>,
) -> Result<SignalProcessTarget, Flow> {
    if let Some(v) = value
        && !v.is_nil()
    {
        // A first-class process object designates that PROCESS, live or not.
        // GNU's `internal-default-signal-process` takes `XPROCESS
        // (process)->pid` (src/process.c:7380) after a plain `CHECK_PROCESS`
        // (:7379) -- no liveness test -- and raises "Cannot signal process %s"
        // when that pid is <= 0 (:7381-7382), which is what a pipe or network
        // process has.  Falling through to the pid branch here handed
        // `sys::send_signal` this port's `ProcessId` (1, 2, 3 ...) as though it
        // were an OS pid; before ledger 169 that was unreachable for a process
        // still in the alist, and retiring earlier made it reachable.
        if let Some(id) = v.as_process_id()
            && processes.get_any(id).is_some()
        {
            return Ok(SignalProcessTarget::Process(id));
        }
        return match v.kind() {
            ValueKind::String => {
                let name_str = process_owned_runtime_string(*v);
                Ok(match processes.find_by_name(&name_str) {
                    Some(id) => SignalProcessTarget::Process(id),
                    None => SignalProcessTarget::MissingNamedProcess,
                })
            }
            // GNU `Fsignal_process` treats a bare integer as a literal OS PID
            // and never looks it up: `internal-default-signal-process` calls
            // `get_process` only for a NON-number (src/process.c:7369-7370),
            // and a number goes straight to
            // `CONS_TO_INTEGER (process, pid_t, pid)` (:7375-7376).  The
            // docstring says the same (:7405-7407).
            //
            // Consulting the live process table here made the answer depend on
            // whether an unrelated process happened to hold that `ProcessId`.
            // Measured, `-Q --batch`, one live child, before this change:
            //
            //   (signal-process 1 0)   GNU -1 (EPERM from `kill (1, 0)`)
            //                          Neomacs 0 -- this port's process #1
            //
            // The comment above this arm already stated GNU's rule; the code
            // under it did not follow it.
            //
            // The domain is `NUMBERP`, not "non-negative fixnum": GNU calls
            // `get_process` only when `!NUMBERP (process)` (:7369-7370), so a
            // bignum, an integral float and a NEGATIVE integer all reach
            // `CONS_TO_INTEGER` and then `kill`.  A negative pid is a POSIX
            // process GROUP -- ledger 175, and ledger 169's fifth residual.
            ValueKind::Fixnum(_) | ValueKind::Float | ValueKind::Veclike(VecLikeType::Bignum) => {
                Ok(SignalProcessTarget::Pid(cons_to_os_pid(*v)?))
            }
            _ => Ok(SignalProcessTarget::Process(
                resolve_get_process_designator_in_state(processes, buffers, v)?,
            )),
        };
    }

    let id = resolve_optional_process_or_current_buffer_in_state(processes, buffers, value)?;
    Ok(SignalProcessTarget::Process(id))
}

pub(super) fn parse_signal_number(value: &Value) -> Result<i32, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n as i32),
        ValueKind::String => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), *value],
        )),
        _ => {
            // Borrow the symbol name before consuming it
            let sym_name = value.as_symbol_name().map(|s| s.to_owned());
            if let Some(name) = sym_name {
                sys::signal_name_number(&name).ok_or_else(|| signal_undefined_signal_name(&name))
            } else {
                Err(signal_wrong_type_integerp(*value))
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProcessSignalRecipient {
    ImmediateProcess,
    ProcessGroup,
}

/// GNU's `kill (p->pid, ...)` / `kill (- p->pid, ...)` for one of this port's
/// own children.
///
/// The pid comes from [`ChildOwnership::pid_if_unreaped`] and not from
/// `Process::os_pid`, and that IS GNU's `p->alive` gate.  `process_send_signal`
/// -- which every signal subr reaches -- is
///
/// ```c
///   /* Do not kill an already-reaped process, as that could kill an
///      innocent bystander that happens to have the same process ID.  */
///   block_child_signal (&oldset);
///   if (p->alive)
///     kill (pid, signo);                       /* src/process.c:7199-7205 */
///   unblock_child_signal (&oldset);
/// ```
///
/// and `Fdelete_process`'s is `if (p->alive) record_kill_process (p, Qnil);`
/// (:1134-1135, and src/callproc.c:202-207).  `os_pid` stays as GNU's `p->pid`,
/// the number `Fprocess_id` reports and GNU keeps after the reap; what goes
/// away with the child is the number that authorises a syscall.
pub(super) fn deliver_process_signal(
    proc: &Process,
    signal_num: i32,
    recipient: ProcessSignalRecipient,
) -> i32 {
    let Some(pid) = proc.live_io.child.pid_if_unreaped() else {
        return -1;
    };
    match recipient {
        ProcessSignalRecipient::ImmediateProcess => sys::send_signal(pid as i64, signal_num),
        ProcessSignalRecipient::ProcessGroup => sys::send_signal_to_group(pid as i64, signal_num),
    }
}

pub(super) fn process_has_subprocess_backing(proc: &Process) -> bool {
    proc.os_pid.is_some() || proc.live_io.child.has_child()
}

pub(super) fn record_unbacked_real_process_signal(proc: &mut Process, signal_num: i32) -> bool {
    if proc.kind != ProcessKind::Real || process_has_subprocess_backing(proc) {
        return false;
    }
    proc.status = process_status_signal_value(signal_num);
    proc.status_notify_pending = false;
    proc.pending_status = Value::NIL;
    true
}

pub(super) fn signal_process_or_unbacked_success(
    proc: &mut Process,
    signal_num: i32,
    recipient: ProcessSignalRecipient,
) -> i32 {
    let result = deliver_process_signal(proc, signal_num, recipient);
    if result == 0 {
        return 0;
    }
    if record_unbacked_real_process_signal(proc, signal_num) {
        return 0;
    }
    result
}

pub(super) fn kill_real_process_child(proc: &mut Process, signal_num: i32) {
    if deliver_process_signal(proc, signal_num, ProcessSignalRecipient::ProcessGroup) == 0 {
        return;
    }
    if record_unbacked_real_process_signal(proc, signal_num) {
        return;
    }
    proc.live_io.child.kill_handle();
}

/// GNU's `wait_for_termination (child, NULL, ...)` (src/sysdep.c:500, called
/// that way at src/callproc.c:257) on a child that is already terminal or has
/// just been killed explicitly.
///
/// Dropping either Rust child handle closes the handle but does not perform
/// Unix `waitpid`, which leaves a zombie.  Call this only on the synchronous
/// delete path; normal status polling has already reaped naturally exited
/// children -- and, since ledger 187, has also given up the pid, so a child
/// that was reaped there is not waited a second time here.
pub(super) fn wait_for_real_process_child_termination(proc: &mut Process) {
    proc.live_io.child.wait_for_termination();
}

pub(super) fn signal_hup_number() -> i32 {
    cfg_select! {
        unix => { libc::SIGHUP }
        _ => { 1 }
    }
}

pub(super) fn signal_kill_number() -> i32 {
    cfg_select! {
        unix => { libc::SIGKILL }
        _ => { 9 }
    }
}

pub(super) fn ticks_to_secs_usecs(ticks: i64, hz: i64) -> (i64, i64) {
    if hz <= 0 {
        return (0, 0);
    }
    let secs = ticks.div_euclid(hz);
    let rem = ticks.rem_euclid(hz);
    let usecs = ((rem as i128) * 1_000_000i128 / (hz as i128)) as i64;
    (secs, usecs)
}

pub(super) fn time_list_from_secs_usecs(secs: i64, usecs: i64) -> Value {
    let high = (secs >> 16) & 0xFFFF_FFFF;
    let low = secs & 0xFFFF;
    Value::list(vec![
        Value::fixnum(high),
        Value::fixnum(low),
        Value::fixnum(usecs.clamp(0, 999_999)),
        Value::fixnum(0),
    ])
}

pub(super) fn time_list_from_ticks(ticks: i64, hz: i64) -> Value {
    let (secs, usecs) = ticks_to_secs_usecs(ticks, hz);
    time_list_from_secs_usecs(secs, usecs)
}

pub(super) fn now_epoch_secs_usecs() -> Option<(i64, i64)> {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(dur) => Some((dur.as_secs() as i64, dur.subsec_micros() as i64)),
        Err(_) => None,
    }
}

pub(super) fn nonnegative_time_diff(now: (i64, i64), then: (i64, i64)) -> (i64, i64) {
    let (now_secs, now_usecs) = now;
    let (then_secs, then_usecs) = then;
    if (now_secs, now_usecs) < (then_secs, then_usecs) {
        return (0, 0);
    }
    let mut secs = now_secs - then_secs;
    let mut usecs = now_usecs - then_usecs;
    if usecs < 0 {
        secs -= 1;
        usecs += 1_000_000;
    }
    (secs, usecs)
}

pub(super) fn parse_make_process_command(value: &Value) -> Result<Vec<LispString>, Flow> {
    let as_vec: Option<Vec<Value>> = match value.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => Some(value.as_vector_data().unwrap().clone()),
        ValueKind::Cons | ValueKind::Nil => list_to_vec(value),
        _ => None,
    };

    let Some(items) = as_vec else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), *value],
        ));
    };

    items
        .into_iter()
        .map(|item| {
            super::super::builtins::expect_lisp_string(&item)
                .cloned()
                .map_err(|_| signal_wrong_type_string(item))
        })
        .collect()
}

pub(super) fn parse_make_process_buffer(
    eval: &mut super::super::eval::Context,
    value: &Value,
) -> Result<Value, Flow> {
    parse_make_process_buffer_in_state(&mut eval.buffers, value)
}

/// Every process constructor's `:buffer`, which in GNU is one call to
/// `Fget_buffer_create` -- `Fmake_process` at src/process.c:1849-1851,
/// `Fmake_pipe_process` at :3091-3094, `Fmake_serial_process` at :3223-3226
/// and `Fmake_network_process` at :4017.
///
/// A buffer OBJECT comes back as it was handed in, and GNU's docstring says so
/// in as many words: "If BUFFER-OR-NAME is a buffer instead of a string,
/// return it as given, even if it is dead.  The return value is never nil."
/// (src/buffer.c:581-582.)  That is not an oversight of GNU's -- it is the
/// state three later `BUFFER_LIVE_P` tests exist to handle:
/// `read_and_insert_process_output` (:6464),
/// `internal-default-process-sentinel`, whose own comment is "Avoid error if
/// buffer is deleted (probably that's why the process is dead, too)"
/// (:7969-7971), and `setup_process_coding_systems` (:8395).  Refusing the
/// dead buffer here made all three unreachable and turned `make-process` into
/// a signal where GNU returns a process.
pub(super) fn parse_make_process_buffer_in_state(
    buffers: &mut BufferManager,
    value: &Value,
) -> Result<Value, Flow> {
    match value.kind() {
        ValueKind::Nil => Ok(Value::NIL),
        ValueKind::String => {
            let name_str = process_owned_runtime_string(*value);
            let id = buffers
                .find_buffer_by_name(&name_str)
                .unwrap_or_else(|| buffers.create_buffer(&name_str));
            Ok(Value::make_buffer(id))
        }
        ValueKind::Veclike(VecLikeType::Buffer) => Ok(*value),
        _ => Err(signal_wrong_type_string(*value)),
    }
}

pub(super) fn expect_integer(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _ => Err(signal_wrong_type_integerp(*value)),
    }
}

pub(super) fn expect_ushort_dimension(value: &Value) -> Result<u16, Flow> {
    let n = expect_integer(value)?;
    u16::try_from(n).map_err(|_| {
        signal(
            LispCondition::ArgsOutOfRange,
            vec![*value, Value::fixnum(0), Value::fixnum(i64::from(u16::MAX))],
        )
    })
}

pub(super) fn value_as_nonnegative_integer(value: &Value) -> Option<i64> {
    match value.kind() {
        ValueKind::Fixnum(n) if n >= 0 => Some(n),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
pub(super) enum NetworkAddressFamily {
    #[strum(serialize = "ipv4")]
    Ipv4,
    #[strum(serialize = "ipv6")]
    Ipv6,
}

impl NetworkAddressFamily {
    pub(super) fn from_symbol_value(value: &Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }

    #[cfg(test)]
    pub(super) fn name(self) -> &'static str {
        self.into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NetworkProcessFamily {
    Unspecified,
    Local,
    Ipv4,
    Ipv6,
    Raw(i32),
}

impl NetworkProcessFamily {
    pub(super) fn is_local(self) -> bool {
        self == Self::Local
    }

    pub(super) fn loopback_host(self) -> &'static str {
        match self {
            Self::Ipv6 => "::1",
            _ => "127.0.0.1",
        }
    }

    pub(super) fn addrinfo_family(self) -> i32 {
        match self {
            Self::Unspecified => 0,
            Self::Local => sys::net::af_local(),
            Self::Ipv4 => sys::net::af_inet(),
            Self::Ipv6 => sys::net::af_inet6(),
            Self::Raw(raw) => raw,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub(super) enum NetworkProcessFamilySymbol {
    Local,
    Ipv4,
    Ipv6,
}

impl NetworkProcessFamilySymbol {
    pub(super) fn from_symbol_value(value: &Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }

    #[cfg(test)]
    pub(super) fn name(self) -> &'static str {
        self.into()
    }
}

pub(super) fn parse_network_host(
    value: &Value,
    family: NetworkProcessFamily,
) -> Result<Option<String>, Flow> {
    if value.is_nil() {
        return Ok(None);
    }
    if value.as_symbol_name() == Some("local") {
        return Ok(Some(family.loopback_host().to_string()));
    }
    match value.kind() {
        ValueKind::String => Ok(Some(process_owned_runtime_string(*value))),
        _ => Err(signal_wrong_type_string(*value)),
    }
}

pub(super) fn network_service_protocol(socket_type: NetworkSocketType) -> &'static str {
    match socket_type {
        NetworkSocketType::Datagram => "udp",
        _ => "tcp",
    }
}

pub(super) fn parse_network_numeric_service_port(service: &str) -> Option<u16> {
    let service = service.trim_start_matches(|ch: char| ch.is_ascii_whitespace());
    let service = service.strip_prefix('+').unwrap_or(service);
    if service.is_empty() || !service.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    service
        .parse::<u128>()
        .ok()
        .map(|port| (port % (1 << 16)) as u16)
}

pub(super) fn parse_network_service_port(
    value: &Value,
    server: bool,
    socket_type: NetworkSocketType,
) -> Result<u16, Flow> {
    match value.kind() {
        ValueKind::T if server => Ok(0),
        ValueKind::Fixnum(port) if port >= 0 => Ok((port as u64 % (1 << 16)) as u16),
        ValueKind::String => {
            let service = process_owned_runtime_string(*value);
            if service.is_empty() {
                return Ok(0);
            }
            if let Some(port) = parse_network_numeric_service_port(&service) {
                return Ok(port);
            }
            sys::net::service_port(&service, network_service_protocol(socket_type)).ok_or_else(
                || {
                    signal(
                        "error",
                        vec![Value::string(format!("Unknown service: {}", service))],
                    )
                },
            )
        }
        _ => Err(signal_wrong_type_string(*value)),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum NetworkAddressSpec {
    Inet(SocketAddr),
    #[cfg(unix)]
    Local(std::path::PathBuf),
}

pub(super) fn parse_network_address_spec(value: &Value) -> Result<NetworkAddressSpec, Flow> {
    #[cfg(unix)]
    if matches!(value.kind(), ValueKind::String) {
        return Ok(NetworkAddressSpec::Local(
            crate::emacs_core::fileio::lisp_file_name_to_path_buf(
                super::super::builtins::expect_lisp_string(value)?,
            ),
        ));
    }

    let Some(items) = value.as_vector_data() else {
        return Err(signal("error", vec![Value::string("Malformed :address")]));
    };

    match items.len() {
        5 => {
            let a = parse_lisp_sockaddr_part(items[0], 255)?;
            let b = parse_lisp_sockaddr_part(items[1], 255)?;
            let c = parse_lisp_sockaddr_part(items[2], 255)?;
            let d = parse_lisp_sockaddr_part(items[3], 255)?;
            let port = parse_lisp_sockaddr_part(items[4], u16::MAX as i64)?;
            Ok(NetworkAddressSpec::Inet(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(a as u8, b as u8, c as u8, d as u8)),
                port as u16,
            )))
        }
        9 => {
            let mut segments = [0_u16; 8];
            for (idx, segment) in segments.iter_mut().enumerate() {
                *segment = parse_lisp_sockaddr_part(items[idx], u16::MAX as i64)? as u16;
            }
            let port = parse_lisp_sockaddr_part(items[8], u16::MAX as i64)?;
            Ok(NetworkAddressSpec::Inet(SocketAddr::new(
                IpAddr::V6(Ipv6Addr::new(
                    segments[0],
                    segments[1],
                    segments[2],
                    segments[3],
                    segments[4],
                    segments[5],
                    segments[6],
                    segments[7],
                )),
                port as u16,
            )))
        }
        _ => Err(signal("error", vec![Value::string("Malformed :address")])),
    }
}

pub(super) fn parse_lisp_sockaddr_part(value: Value, max: i64) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) if (0..=max).contains(&n) => Ok(n),
        _ => Err(signal("error", vec![Value::string("Malformed :address")])),
    }
}

pub(super) fn socket_addr_to_lisp_value(addr: SocketAddr) -> Value {
    match addr {
        SocketAddr::V4(v4) => {
            let octets = v4.ip().octets();
            int_vector(&[
                octets[0] as i64,
                octets[1] as i64,
                octets[2] as i64,
                octets[3] as i64,
                v4.port() as i64,
            ])
        }
        SocketAddr::V6(v6) => {
            let segments = v6.ip().segments();
            let mut vals = [0_i64; 9];
            for (idx, &seg) in segments.iter().enumerate() {
                vals[idx] = seg as i64;
            }
            vals[8] = v6.port() as i64;
            int_vector(&vals)
        }
    }
}

#[cfg(unix)]
pub(super) fn unix_socket_addr_to_runtime_string(addr: Option<UnixSocketAddr>) -> String {
    addr.and_then(|addr| {
        addr.as_pathname()
            .map(|path| path.as_os_str().to_string_lossy().into_owned())
    })
    .unwrap_or_default()
}

#[cfg(unix)]
pub(super) fn socket2_unix_sockaddr_to_runtime_string(addr: Option<&SockAddr>) -> String {
    addr.and_then(|addr| {
        addr.as_pathname()
            .map(|path| path.as_os_str().to_string_lossy().into_owned())
    })
    .unwrap_or_default()
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(super) fn validate_network_process_family(value: &Value) -> Result<(), Flow> {
    if value.is_nil()
        || matches!(value.kind(), ValueKind::Fixnum(_))
        || NetworkProcessFamilySymbol::from_symbol_value(value).is_some()
    {
        Ok(())
    } else {
        Err(signal(
            "error",
            vec![Value::string("Unknown address family")],
        ))
    }
}

pub(super) fn parse_network_process_family(value: &Value) -> Result<NetworkProcessFamily, Flow> {
    if value.is_nil() {
        return Ok(NetworkProcessFamily::Unspecified);
    }
    match NetworkProcessFamilySymbol::from_symbol_value(value) {
        Some(NetworkProcessFamilySymbol::Local) => return Ok(NetworkProcessFamily::Local),
        Some(NetworkProcessFamilySymbol::Ipv4) => return Ok(NetworkProcessFamily::Ipv4),
        Some(NetworkProcessFamilySymbol::Ipv6) => return Ok(NetworkProcessFamily::Ipv6),
        None => {}
    }
    if let ValueKind::Fixnum(raw) = value.kind() {
        return network_process_family_from_raw(raw)
            .ok_or_else(|| signal("error", vec![Value::string("Unknown address family")]));
    }
    Err(signal(
        "error",
        vec![Value::string("Unknown address family")],
    ))
}

pub(super) fn network_process_family_from_raw(raw: i64) -> Option<NetworkProcessFamily> {
    let raw = i32::try_from(raw).ok()?;
    Some(match sys::net::classify_family(raw) {
        sys::net::NetFamily::Unspecified => NetworkProcessFamily::Unspecified,
        sys::net::NetFamily::Ipv4 => NetworkProcessFamily::Ipv4,
        sys::net::NetFamily::Ipv6 => NetworkProcessFamily::Ipv6,
        sys::net::NetFamily::Local => NetworkProcessFamily::Local,
        sys::net::NetFamily::Other(r) => NetworkProcessFamily::Raw(r),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub(super) enum NetworkLookupHint {
    Numeric,
}

impl NetworkLookupHint {
    pub(super) fn from_symbol_value(value: &Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }

    pub(super) fn addrinfo_flags(self) -> i32 {
        match self {
            Self::Numeric => sys::net::ai_numerichost(),
        }
    }

    #[cfg(test)]
    pub(super) fn name(self) -> &'static str {
        self.into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub(super) enum NumProcessorsQuery {
    All,
    Current,
}

impl NumProcessorsQuery {
    pub(super) fn from_symbol_value(value: &Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }

    #[cfg(test)]
    pub(super) fn name(self) -> &'static str {
        self.into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NetworkSocketType {
    Stream,
    Datagram,
    #[cfg(unix)]
    Seqpacket,
}

pub(super) fn parse_network_socket_type(value: &Value) -> Result<NetworkSocketType, Flow> {
    match value.as_symbol_name() {
        _ if value.is_nil() => Ok(NetworkSocketType::Stream),
        Some("datagram") => Ok(NetworkSocketType::Datagram),
        #[cfg(unix)]
        Some("seqpacket") => Ok(NetworkSocketType::Seqpacket),
        Some(_) | None => Err(signal(
            "error",
            vec![Value::string("Unsupported connection type")],
        )),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(super) fn validate_network_socket_type(value: &Value) -> Result<(), Flow> {
    parse_network_socket_type(value).map(|_| ())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub(super) enum ProcessConnectionType {
    Pipe,
    Pty,
}

impl ProcessConnectionType {
    pub(super) fn from_symbol_value(value: &Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }

    pub(super) fn uses_pty(self) -> bool {
        matches!(self, Self::Pty)
    }
}

pub(super) fn resolve_process_connection_type_use_pty(
    connection_type: Option<&Value>,
    default_use_pty: bool,
) -> Result<bool, Flow> {
    match connection_type {
        None => Ok(default_use_pty),
        Some(value) if value.is_nil() => Ok(default_use_pty),
        Some(value) => ProcessConnectionType::from_symbol_value(value)
            .map(ProcessConnectionType::uses_pty)
            .ok_or_else(|| {
                // GNU `is_pty_from_symbol` (process.c) signals this through
                // `report_file_error ("Unknown connection type", symbol)`, which
                // reads the live `errno`.  At this point in `make-process` (before
                // any program lookup) the residual errno is ENOENT, so GNU emits
                // `(file-missing "Unknown connection type" "No such file or
                // directory" SYMBOL)`.  Match that data list exactly.
                signal_file_errno("Unknown connection type", *value, libc::ENOENT)
            }),
    }
}

#[derive(Clone, Debug)]
pub(super) struct HostInterfaceEntry {
    pub(super) name: String,
    pub(super) family: NetworkAddressFamily,
    pub(super) address: Value,
    pub(super) list_broadcast: Value,
    pub(super) info_broadcast: Value,
    pub(super) netmask: Value,
    pub(super) hwaddr: Option<Value>,
    pub(super) flags: Value,
}

pub(super) fn vector_nonnegative_integers(value: &Value) -> Option<Vec<i64>> {
    if !value.is_vector() {
        return None;
    };
    let locked = value.as_vector_data().unwrap().clone();
    let mut out = Vec::with_capacity(locked.len());
    for item in locked.iter() {
        out.push(value_as_nonnegative_integer(item)?);
    }
    Some(out)
}

pub(super) fn int_vector(values: &[i64]) -> Value {
    Value::vector(values.iter().map(|v| Value::fixnum(*v)).collect())
}

pub(super) fn loopback_ipv4_address() -> Value {
    int_vector(&[127, 0, 0, 1, 0])
}

pub(super) fn loopback_ipv4_broadcast() -> Value {
    int_vector(&[0, 0, 0, 0, 0])
}

pub(super) fn loopback_ipv4_netmask() -> Value {
    int_vector(&[255, 0, 0, 0, 0])
}

pub(super) fn loopback_ipv6_address() -> Value {
    int_vector(&[0, 0, 0, 0, 0, 0, 0, 1, 0])
}

pub(super) fn loopback_ipv6_broadcast() -> Value {
    int_vector(&[0, 0, 0, 0, 0, 0, 0, 1, 0])
}

pub(super) fn loopback_ipv6_netmask() -> Value {
    int_vector(&[65535, 65535, 65535, 65535, 65535, 65535, 65535, 65535, 0])
}

pub(super) fn loopback_hwaddr() -> Value {
    Value::cons(Value::fixnum(772), int_vector(&[0, 0, 0, 0, 0, 0]))
}

pub(super) fn loopback_flags() -> Value {
    Value::list(vec![
        Value::symbol("running"),
        Value::symbol("loopback"),
        Value::symbol("up"),
    ])
}

pub(super) fn zero_network_address(family: NetworkAddressFamily) -> Value {
    match family {
        NetworkAddressFamily::Ipv4 => int_vector(&[0, 0, 0, 0, 0]),
        NetworkAddressFamily::Ipv6 => int_vector(&[0, 0, 0, 0, 0, 0, 0, 0, 0]),
    }
}

pub(super) fn network_directed_broadcast(
    family: NetworkAddressFamily,
    address: &Value,
    netmask: &Value,
) -> Option<Value> {
    let address_items = vector_nonnegative_integers(address)?;
    let netmask_items = vector_nonnegative_integers(netmask)?;
    match family {
        NetworkAddressFamily::Ipv4 => {
            if address_items.len() != 5 || netmask_items.len() != 5 {
                return None;
            }
            let mut out = [0_i64; 5];
            for idx in 0..4 {
                let addr = u8::try_from(address_items[idx]).ok()?;
                let mask = u8::try_from(netmask_items[idx]).ok()?;
                out[idx] = (addr | !mask) as i64;
            }
            Some(int_vector(&out))
        }
        NetworkAddressFamily::Ipv6 => {
            if address_items.len() != 9 || netmask_items.len() != 9 {
                return None;
            }
            let mut out = [0_i64; 9];
            for idx in 0..8 {
                let addr = u16::try_from(address_items[idx]).ok()?;
                let mask = u16::try_from(netmask_items[idx]).ok()?;
                out[idx] = (addr | !mask) as i64;
            }
            Some(int_vector(&out))
        }
    }
}

pub(super) fn derive_network_interface_list_broadcast(
    family: NetworkAddressFamily,
    address: &Value,
    netmask: &Value,
    raw_broadcast: &Value,
) -> Value {
    network_directed_broadcast(family, address, netmask).unwrap_or(*raw_broadcast)
}

pub(super) fn derive_network_interface_info_broadcast(
    family: NetworkAddressFamily,
    address: &Value,
    raw_broadcast: &Value,
) -> Value {
    if raw_broadcast == address {
        zero_network_address(family)
    } else {
        *raw_broadcast
    }
}

pub(super) fn ip_to_value(ip: IpAddr) -> (NetworkAddressFamily, Value) {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            (
                NetworkAddressFamily::Ipv4,
                int_vector(&[
                    octets[0] as i64,
                    octets[1] as i64,
                    octets[2] as i64,
                    octets[3] as i64,
                    0,
                ]),
            )
        }
        IpAddr::V6(v6) => {
            let segments = v6.segments();
            let mut vals = [0_i64; 9];
            for (idx, &seg) in segments.iter().enumerate() {
                vals[idx] = seg as i64;
            }
            (NetworkAddressFamily::Ipv6, int_vector(&vals))
        }
    }
}

pub(super) fn resolve_network_lookup_addresses(
    name: &str,
    family: Option<NetworkAddressFamily>,
    hint: Option<NetworkLookupHint>,
) -> Vec<Value> {
    use dns_lookup::{AddrFamily, AddrInfoHints, SockType};

    // Emacs forwards names through C APIs where embedded NUL terminates the
    // effective hostname. Match that behavior instead of rejecting interior NUL.
    let normalized_name = name.split('\0').next().unwrap_or_default();

    let hints = AddrInfoHints {
        flags: hint.map_or(0, NetworkLookupHint::addrinfo_flags),
        socktype: SockType::DGram.into(),
        address: match family {
            Some(NetworkAddressFamily::Ipv4) => AddrFamily::Inet.into(),
            Some(NetworkAddressFamily::Ipv6) => AddrFamily::Inet6.into(),
            None => 0, // AF_UNSPEC
        },
        ..AddrInfoHints::default()
    };

    let addrs = match dns_lookup::getaddrinfo(Some(normalized_name), None, Some(hints)) {
        Ok(iter) => iter,
        Err(_) => return Vec::new(),
    };

    let mut out = Vec::new();
    for result in addrs {
        let info = match result {
            Ok(info) => info,
            Err(_) => continue,
        };
        let (resolved_family, address) = ip_to_value(info.sockaddr.ip());
        let include = match family {
            Some(expected) => expected == resolved_family,
            None => true,
        };
        if include {
            out.push(address);
        }
    }

    out
}

pub(super) fn interface_entry(name: &str, address: Value, full: bool) -> Value {
    if !full {
        return Value::cons(Value::string(name), address);
    }

    let (broadcast, netmask) = match address.kind() {
        ValueKind::Veclike(VecLikeType::Vector) if address.as_vector_data().unwrap().len() == 9 => {
            (loopback_ipv6_broadcast(), loopback_ipv6_netmask())
        }
        _ => (loopback_ipv4_broadcast(), loopback_ipv4_netmask()),
    };

    Value::list(vec![Value::string(name), address, broadcast, netmask])
}

pub(super) fn format_ipv4_network_address(items: &[i64], omit_port: bool) -> Option<String> {
    if items.len() != 4 && items.len() != 5 {
        return None;
    }
    let octets: Vec<u8> = items[..4]
        .iter()
        .map(|v| u8::try_from(*v).ok())
        .collect::<Option<Vec<_>>>()?;
    let addr = format!("{}.{}.{}.{}", octets[0], octets[1], octets[2], octets[3]);
    if items.len() == 5 && !omit_port {
        let port = u16::try_from(items[4]).ok()?;
        Some(format!("{addr}:{port}"))
    } else {
        Some(addr)
    }
}

pub(super) fn format_ipv6_network_address(items: &[i64], omit_port: bool) -> Option<String> {
    if items.len() != 8 && items.len() != 9 {
        return None;
    }
    let mut segments = Vec::with_capacity(8);
    for value in &items[..8] {
        let segment = u16::try_from(*value).ok()?;
        segments.push(format!("{segment:x}"));
    }
    let addr = segments.join(":");
    if items.len() == 9 && !omit_port {
        let port = u16::try_from(items[8]).ok()?;
        Some(format!("[{addr}]:{port}"))
    } else {
        Some(addr)
    }
}
