//! The Lisp face of the isolate pool: `neomacs-worker-*`.
//!
//! ```elisp
//! (neomacs-worker-run FUNCTION &rest ARGS)      ; -> task
//! (neomacs-worker-wait TASK &optional TIMEOUT)   ; -> FUNCTION's value
//! (neomacs-worker-cancel TASK)                   ; -> t if it had not finished
//! (neomacs-worker-status TASK)                   ; -> queued|running|finished|cancelled
//! (neomacs-worker-pool-size)                     ; -> owner count
//! (neomacs-worker-stats)                         ; -> plist
//! ```
//!
//! `neomacs-worker-run` copies FUNCTION and ARGS into a worker isolate
//! ([`crate::transfer`]) and applies them there; nothing the worker does is
//! visible here except the value `neomacs-worker-wait` copies back, and an
//! error the function signals is re-signaled by the wait with the same
//! condition and (printed, re-read) data. A task is the record
//! `#s(neomacs-worker-task ID)`.
//!
//! The host registers these with [`register_subrs`] after its Context
//! boots, the way a dynamic module adds primitives: they are not in the
//! core subr manifest, so no worker isolate has them and a task cannot
//! spawn tasks of its own. Conditions defined here: `neomacs-worker-error`
//! (parent `error`) and its children `neomacs-worker-transfer-error`,
//! `neomacs-worker-timeout` and `neomacs-worker-cancelled`.

use crate::isolate::{self, ERROR_SYMBOL, IsolatePool, TRANSFER_ERROR_SYMBOL};
use crate::transfer::{self, Call};
use neovm_core::emacs_core::error::{EvalResult, LispCondition, signal};
use neovm_core::emacs_core::eval::Context;
use neovm_core::emacs_core::value::list_to_vec;
use neovm_core::emacs_core::value_reader::read_all;
use neovm_core::{TaskHandle, TaskScheduler, TaskStatus, Value};
use neovm_eval::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};
use neovm_host_abi::{LispValue, Signal, TaskError, TaskOptions};
use std::time::{Duration, Instant};

const TASK_TYPE: &str = "neomacs-worker-task";
const TIMEOUT_SYMBOL: &str = "neomacs-worker-timeout";
const CANCELLED_SYMBOL: &str = "neomacs-worker-cancelled";

/// How long one wait slice blocks before the main isolate checks for C-g.
const WAIT_SLICE: Duration = Duration::from_millis(50);

/// The primitives, as the host registers them.
pub const SUBRS: [SubrSpec; 6] = [
    SubrSpec::new(
        "neomacs-worker-run",
        NativeFn::ContextVec(builtin_run),
        SubrArity::new(1, None),
    ),
    SubrSpec::new(
        "neomacs-worker-wait",
        NativeFn::ContextVec(builtin_wait),
        SubrArity::new(1, Some(2)),
    ),
    SubrSpec::new(
        "neomacs-worker-cancel",
        NativeFn::ContextVec(builtin_cancel),
        SubrArity::new(1, Some(1)),
    ),
    SubrSpec::new(
        "neomacs-worker-status",
        NativeFn::ContextVec(builtin_status),
        SubrArity::new(1, Some(1)),
    ),
    SubrSpec::new(
        "neomacs-worker-pool-size",
        NativeFn::ContextVec(builtin_pool_size),
        SubrArity::new(0, Some(0)),
    ),
    SubrSpec::new(
        "neomacs-worker-stats",
        NativeFn::ContextVec(builtin_stats),
        SubrArity::new(0, Some(0)),
    ),
];

/// The evaluator checks arity before dispatch; a missing optional arrives
/// as nil here.
fn arg(args: &[Value], index: usize) -> Value {
    args.get(index).copied().unwrap_or(Value::NIL)
}

/// Install the primitives and their conditions into `ctx`.
pub fn register_subrs(ctx: &mut Context) {
    for spec in SUBRS {
        ctx.register_subr(spec);
    }
    define_error(ctx, ERROR_SYMBOL, "Worker isolate error", &["error"]);
    define_error(
        ctx,
        TRANSFER_ERROR_SYMBOL,
        "Value cannot be copied between isolates",
        &[ERROR_SYMBOL, "error"],
    );
    define_error(
        ctx,
        TIMEOUT_SYMBOL,
        "Timed out waiting for a worker task",
        &[ERROR_SYMBOL, "error"],
    );
    define_error(
        ctx,
        CANCELLED_SYMBOL,
        "Worker task was cancelled",
        &[ERROR_SYMBOL, "error"],
    );
}

/// GNU `define-error`: the two plist properties `signal` dispatch reads.
fn define_error(ctx: &mut Context, name: &str, message: &str, parents: &[&str]) {
    let conditions = Value::list(
        std::iter::once(name)
            .chain(parents.iter().copied())
            .map(Value::symbol)
            .collect(),
    );
    let _ = ctx
        .obarray_mut()
        .put_property(name, "error-conditions", conditions);
    let _ = ctx
        .obarray_mut()
        .put_property(name, "error-message", Value::string(message));
}

fn lisp_bool(flag: bool) -> Value {
    if flag { Value::T } else { Value::NIL }
}

fn pool_error(message: impl Into<String>) -> neovm_core::Flow {
    signal(ERROR_SYMBOL, vec![Value::string(message.into())])
}

fn pool() -> Result<&'static IsolatePool, neovm_core::Flow> {
    isolate::installed()
        .ok_or_else(|| pool_error("No worker isolate pool is installed in this process"))
}

fn task_value(handle: TaskHandle) -> Value {
    Value::make_record(vec![
        Value::symbol(TASK_TYPE),
        Value::fixnum(handle.0 as i64),
    ])
}

fn task_handle(value: Value) -> Result<TaskHandle, neovm_core::Flow> {
    let wrong_type = || {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("neomacs-worker-task-p"), value],
        )
    };
    let slots = value.as_record_data().ok_or_else(wrong_type)?;
    if slots.len() != 2 || slots[0].as_bare_symbol_name() != Some(TASK_TYPE) {
        return Err(wrong_type());
    }
    let id = slots[1].as_fixnum().ok_or_else(wrong_type)?;
    u64::try_from(id).map(TaskHandle).map_err(|_| wrong_type())
}

fn transfer_in(ctx: &Context, value: Value) -> Result<transfer::Transfer, neovm_core::Flow> {
    transfer::from_value(ctx, value).map_err(|err| signal(TRANSFER_ERROR_SYMBOL, err.signal_data()))
}

/// (neomacs-worker-run FUNCTION &rest ARGS) -> task
fn builtin_run(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    let pool = pool()?;
    let mut args = args.into_iter();
    let function = args.next().unwrap_or(Value::NIL);
    let call = Call {
        function: transfer_in(ctx, function)?,
        args: args
            .map(|arg| transfer_in(ctx, arg))
            .collect::<Result<Vec<_>, _>>()?,
    };
    pool.ensure_started();
    let handle = pool
        .runtime()
        .spawn_task(
            LispValue {
                bytes: transfer::encode_call(&call),
            },
            TaskOptions::default(),
        )
        .map_err(|err| pool_error(format!("Cannot queue worker task: {}", err.symbol)))?;
    Ok(task_value(handle))
}

fn timeout_from(value: Value) -> Result<Option<Duration>, neovm_core::Flow> {
    if value.is_nil() {
        return Ok(None);
    }
    let seconds = value
        .as_fixnum()
        .map(|n| n as f64)
        .or_else(|| value.as_float())
        .ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("numberp"), value],
            )
        })?;
    Ok(Some(Duration::from_secs_f64(seconds.max(0.0))))
}

/// Re-raise the worker's signal here: the condition by name, the data read
/// back from its printed form (or kept as that text when it does not read).
fn resignal(ctx: &mut Context, signal_in_worker: Signal) -> neovm_core::Flow {
    let Signal { symbol, data } = signal_in_worker;
    let symbol = if symbol == "worker-startup-failed" {
        ERROR_SYMBOL.to_string()
    } else {
        symbol
    };
    let data = match data {
        None => Vec::new(),
        Some(text) => match read_all(&text, ctx.variables()) {
            Ok(forms) => match forms.first() {
                Some(form) => list_to_vec(form).unwrap_or_else(|| vec![*form]),
                None => Vec::new(),
            },
            Err(_) => vec![Value::string(text)],
        },
    };
    signal(symbol.as_str(), data)
}

/// (neomacs-worker-wait TASK &optional TIMEOUT) -> value
fn builtin_wait(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    let (task, timeout) = (arg(&args, 0), arg(&args, 1));
    let pool = pool()?;
    let handle = task_handle(task)?;
    let deadline = timeout_from(timeout)?.map(|timeout| Instant::now() + timeout);
    let runtime = pool.runtime();
    if runtime.task_status(handle).is_none() {
        return Err(pool_error("No such worker task"));
    }
    let outcome = loop {
        let slice = match deadline {
            Some(deadline) => {
                let now = Instant::now();
                if now >= deadline {
                    return Err(signal(TIMEOUT_SYMBOL, vec![task]));
                }
                deadline.saturating_duration_since(now).min(WAIT_SLICE)
            }
            None => WAIT_SLICE,
        };
        match runtime.task_await(handle, Some(slice)) {
            Err(TaskError::TimedOut) => ctx.maybe_quit()?,
            outcome => break outcome,
        }
    };
    runtime.forget_task(handle);
    match outcome {
        Ok(bytes) => {
            let result = transfer::decode(&bytes.bytes)
                .map_err(|err| signal(TRANSFER_ERROR_SYMBOL, err.signal_data()))?;
            transfer::to_value(ctx, &result)
                .map_err(|err| signal(TRANSFER_ERROR_SYMBOL, err.signal_data()))
        }
        Err(TaskError::Failed(signal_in_worker)) => Err(resignal(ctx, signal_in_worker)),
        Err(TaskError::Cancelled) => Err(signal(CANCELLED_SYMBOL, vec![task])),
        Err(TaskError::TimedOut) => Err(signal(TIMEOUT_SYMBOL, vec![task])),
        Err(TaskError::Shutdown(order)) => Err(pool_error(format!(
            "Worker task called kill-emacs (exit code {})",
            order.exit_code
        ))),
    }
}

/// (neomacs-worker-cancel TASK) -> t when the task had not finished
fn builtin_cancel(_ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    let task = arg(&args, 0);
    let pool = pool()?;
    let handle = task_handle(task)?;
    let runtime = pool.runtime();
    let was_pending = matches!(
        runtime.task_status(handle),
        Some(TaskStatus::Queued | TaskStatus::Running)
    );
    runtime.cancel(handle);
    Ok(lisp_bool(was_pending))
}

/// (neomacs-worker-status TASK) -> symbol, nil for a forgotten task
fn builtin_status(_ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    let task = arg(&args, 0);
    let pool = pool()?;
    let handle = task_handle(task)?;
    Ok(match pool.runtime().task_status(handle) {
        None => Value::NIL,
        Some(TaskStatus::Queued) => Value::symbol("queued"),
        Some(TaskStatus::Running) => Value::symbol("running"),
        Some(TaskStatus::Completed) => Value::symbol("finished"),
        Some(TaskStatus::Cancelled) => Value::symbol("cancelled"),
    })
}

/// (neomacs-worker-pool-size) -> owner count, 0 without a pool
fn builtin_pool_size(_ctx: &mut Context, _args: Vec<Value>) -> EvalResult {
    Ok(Value::fixnum(
        isolate::installed().map(|pool| pool.threads()).unwrap_or(0) as i64,
    ))
}

/// (neomacs-worker-stats) -> plist
fn builtin_stats(_ctx: &mut Context, _args: Vec<Value>) -> EvalResult {
    let Some(pool) = isolate::installed() else {
        return Ok(Value::NIL);
    };
    let stats = pool.stats();
    let ms = |duration: Duration| Value::make_float(duration.as_secs_f64() * 1000.0);
    let count = |n: u64| Value::fixnum(n as i64);
    Ok(Value::list(vec![
        Value::symbol(":threads"),
        Value::fixnum(stats.threads as i64),
        Value::symbol(":started"),
        lisp_bool(stats.started),
        Value::symbol(":ready"),
        Value::fixnum(stats.ready as i64),
        Value::symbol(":startup-error"),
        stats.startup_error.map(Value::string).unwrap_or(Value::NIL),
        Value::symbol(":boot-max-ms"),
        ms(stats.boot_max),
        Value::symbol(":boot-mean-ms"),
        ms(stats.boot_mean),
        Value::symbol(":enqueued"),
        count(stats.enqueued),
        Value::symbol(":completed"),
        count(stats.completed),
        Value::symbol(":cancelled"),
        count(stats.cancelled),
    ]))
}
