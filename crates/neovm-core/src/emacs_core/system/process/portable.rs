//! Process semantics for hosts without native process transports.
//!
//! GNU Emacs keeps process support behind its `subprocesses` build contract.
//! This module is the corresponding capability backend: process values retain
//! a stable type in the evaluator, the primitives GNU defines outside
//! `#ifdef subprocesses` keep their GNU behavior, neutral queries return GNU's
//! documented "unsupported" answer, and every operation that would create or
//! control an OS process fails at this boundary with the one deterministic
//! Lisp error GNU raises from `callproc.c` on such hosts.
//!
//! Everything that is host-independent lives in the sibling modules shared
//! with the native backend (`nproc`, `async_callback`, `bootstrap_vars`,
//! `system_processes`), so GNU policy is edited once.

#[path = "async_callback.rs"]
mod async_callback;
#[path = "bootstrap_vars.rs"]
mod bootstrap_vars;
#[path = "nproc.rs"]
mod nproc;
#[path = "system_processes.rs"]
mod system_processes;

use crate::buffer::BufferId;
use crate::emacs_core::error::{EvalResult, Flow, LispCondition, expect_args, signal};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;
use crate::gc_trace::GcTrace;

pub(crate) use async_callback::AsyncCallbackKind;
pub use bootstrap_vars::register_bootstrap_vars;
pub(crate) use nproc::builtin_num_processors;
pub(crate) use system_processes::{builtin_list_system_processes, builtin_process_attributes};

pub type ProcessId = u64;

#[derive(Default)]
pub struct ProcessManager;

impl ProcessManager {
    pub fn new() -> Self {
        Self
    }

    /// No wake-up mechanism exists on this host: there is no process poller
    /// for a producer to interrupt. `None` is the documented contract for
    /// that, so an input bridge never believes it woke a sleeping evaluator.
    pub fn wait_notifier(&self) -> Option<WaitNotifier> {
        None
    }

    pub(crate) fn find_by_buffer_id(&self, _buffer_id: BufferId) -> Option<ProcessId> {
        None
    }

    pub(crate) fn read_status_without_recording(
        &self,
        _site: UnrecordedStatusRead,
        _id: ProcessId,
    ) -> Option<PortableProcessStatus> {
        None
    }

    pub(crate) fn live_process_ids(&self) -> Vec<ProcessId> {
        Vec::new()
    }

    pub(crate) fn open_channel_for_module(&self, _process: Value) -> Result<std::ffi::c_int, Flow> {
        Err(unavailable_process_error())
    }
}

impl GcTrace for ProcessManager {
    fn trace_roots(&self, _roots: &mut Vec<Value>) {}
}

/// The wake handle type of a host that has none. It is uninhabited on
/// purpose: the API shape (`Option<WaitNotifier>`) is shared with the native
/// backend, but no value of this type can ever exist here, so `notify` cannot
/// be called and therefore cannot fake a wake-up.
#[derive(Clone, Debug)]
pub enum WaitNotifier {}

impl WaitNotifier {
    #[must_use = "a failed notification can leave the evaluator blocked"]
    pub fn notify(&self) -> std::io::Result<()> {
        match *self {}
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UnrecordedStatusRead {
    ModeLinePercentS,
}

pub(crate) struct PortableProcessStatus;

impl PortableProcessStatus {
    pub(crate) fn public_status_symbol(&self) -> Value {
        Value::NIL
    }
}

impl Context {
    pub(crate) fn kill_buffer_processes(&mut self, _buffer_id: BufferId) -> Result<(), Flow> {
        Ok(())
    }
}

pub(crate) fn print_process_handle(value: &Value) -> Option<String> {
    value.as_process_id().map(|_| "#<process>".to_owned())
}

/// GNU provides the `make-network-process` feature only inside
/// `#ifdef subprocesses` (process.c `syms_of_process`); a host without
/// network processes must not let `(featurep 'make-network-process)` answer t.
pub(crate) fn make_network_process_subfeatures() -> Option<Value> {
    None
}

/// GNU `callproc.c`: `error ("Operating system cannot handle asynchronous
/// subprocesses")`.
fn unavailable_process_error() -> Flow {
    signal(
        LispCondition::Error,
        vec![Value::string(
            "Operating system cannot handle asynchronous subprocesses",
        )],
    )
}

pub(crate) fn unsupported(_ctx: &mut Context, _args: Vec<Value>) -> EvalResult {
    Err(unavailable_process_error())
}

macro_rules! unsupported_process_subrs {
    ($($name:ident),+ $(,)?) => {
        $(pub(crate) use unsupported as $name;)+
    };
}

unsupported_process_subrs!(
    builtin_print_preprocess,
    builtin_format_network_address,
    builtin_network_interface_info,
    builtin_accept_process_output,
    builtin_make_process,
    builtin_make_network_process,
    builtin_neomacs_open_tls_stream,
    builtin_make_pipe_process,
    builtin_gnutls_boot,
    builtin_make_serial_process,
    builtin_serial_process_configure,
    builtin_call_process,
    builtin_call_process_region,
    builtin_continue_process,
    builtin_delete_process,
    builtin_interrupt_process,
    builtin_kill_process,
    builtin_quit_process,
    builtin_signal_process,
    builtin_stop_process,
    builtin_process_id,
    builtin_process_command,
    builtin_process_contact,
    builtin_process_filter,
    builtin_set_process_filter,
    builtin_process_sentinel,
    builtin_set_process_sentinel,
    builtin_process_coding_system,
    builtin_process_datagram_address,
    builtin_set_process_buffer,
    builtin_set_process_thread,
    builtin_set_process_window_size,
    builtin_process_tty_name,
    builtin_process_plist,
    builtin_set_process_plist,
    builtin_process_mark,
    builtin_process_type,
    builtin_process_thread,
    builtin_process_running_child_p,
    builtin_process_send_region,
    builtin_process_send_eof,
    builtin_process_send_string,
    builtin_process_status,
    builtin_process_exit_status,
    builtin_process_name,
    builtin_process_buffer,
    builtin_set_process_inherit_coding_system_flag,
    builtin_gnutls_asynchronous_parameters,
    builtin_gnutls_bye,
    builtin_gnutls_deinit,
    builtin_gnutls_get_initstage,
    builtin_gnutls_peer_status,
    builtin_internal_default_interrupt_process,
    builtin_internal_default_process_filter,
    builtin_internal_default_process_sentinel,
    builtin_internal_default_signal_process,
    builtin_network_lookup_address_info,
    builtin_set_network_process_option,
    builtin_process_query_on_exit_flag,
    builtin_set_process_query_on_exit_flag,
    builtin_set_process_coding_system,
    builtin_set_process_datagram_address,
);

/// GNU `process.c` `#else /* not subprocesses */`: ignore the argument and
/// answer with the `inherit-process-coding-system` variable.
pub(crate) fn builtin_process_inherit_coding_system_flag(
    ctx: &mut Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-inherit-coding-system-flag", &args, 1)?;
    Ok(Value::bool(
        ctx.visible_variable_value_or_nil("inherit-process-coding-system")
            .is_truthy(),
    ))
}

/// GNU `sysdep.c` documented answer when interface enumeration is unsupported.
pub(crate) fn builtin_network_interface_list(_ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("network-interface-list"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    Ok(Value::NIL)
}

/// GNU `process.c` `Fsignal_names` MSDOS branch: no signal table, nil.
pub(crate) fn builtin_signal_names(_ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("signal-names", &args, 0)?;
    Ok(Value::NIL)
}

/// GNU `sysdep.c` fallback `list_system_processes`: nil when unsupported.
pub(crate) fn list_system_processes_leaf() -> EvalResult {
    Ok(Value::NIL)
}

/// GNU `sysdep.c` fallback `system_process_attributes`: nil when unsupported.
pub(crate) fn process_attributes_leaf(_pid: Value) -> EvalResult {
    Ok(Value::NIL)
}

/// GNU `Fget_process`: a process object is returned unchanged; otherwise the
/// argument must be a name string, and no process exists here to match it.
pub(crate) fn builtin_get_process(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("get-process", &args, 1)?;
    if args[0].is_process() {
        return Ok(args[0]);
    }
    ctx.expect_lisp_string(args[0])?;
    Ok(Value::NIL)
}

/// GNU `#else /* not subprocesses */` `Fget_buffer_process`: nil, with no
/// buffer lookup at all.
pub(crate) fn builtin_get_buffer_process(_ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("get-buffer-process", &args, 1)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_processp(_ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("processp", &args, 1)?;
    Ok(Value::bool(args[0].as_process_id().is_some()))
}

pub(crate) fn builtin_process_list(_ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("process-list", &args, 0)?;
    Ok(Value::NIL)
}
