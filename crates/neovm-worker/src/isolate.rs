//! The process's pool of worker isolates (RFC §8.1).
//!
//! An isolate is an owner thread with a Context of its own: its own heap,
//! obarray values, specpdl, handlers, buffers and collector. It shares with
//! the main isolate only what the process already shares -- the append-only
//! symbol-name registry and the loaded image file -- and never a `Value`.
//! Work reaches it as a transfer-encoded call ([`crate::transfer`]) through
//! the [`WorkerRuntime`] queue; the result comes back the same way.
//!
//! The pool is a process singleton installed by the host before any Lisp
//! runs ([`install`]) and started lazily by the first task ([`IsolatePool::
//! ensure_started`]): owners boot on a helper thread, so the first
//! `neomacs-worker-run` returns at once and its task waits in the queue
//! until an owner is ready.

use crate::transfer::{self, Call, TransferError};
use crate::{WorkerConfig, WorkerRuntime, WorkerStartError, eval_error_to_task_error};
use neovm_core::emacs_core::eval::Context;
use neovm_core::emacs_core::load::{
    RuntimeImageRole, apply_runtime_startup_state, runtime_image_available,
};
use neovm_core::emacs_core::load_boot::{
    create_bootstrap_evaluator_cached, load_runtime_image_with_features,
};
use neovm_core::emacs_core::print::print_value_with_eval;
use neovm_core::{TaskScheduler, Value};
use neovm_host_abi::{LispValue, Signal, TaskError};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

/// Environment override for the pool width.
pub const THREADS_ENV: &str = "NEOMACS_WORKER_THREADS";

/// The condition every worker-pool failure signals as.
pub const ERROR_SYMBOL: &str = "neomacs-worker-error";
/// The condition a value that cannot cross the boundary signals as.
pub const TRANSFER_ERROR_SYMBOL: &str = "neomacs-worker-transfer-error";

/// Stack for an owner thread: the GUI evaluator's 64 MiB, since an isolate
/// runs the same evaluator and a shipped binary sets no `RUST_MIN_STACK`.
const OWNER_STACK_BYTES: usize = 64 * 1024 * 1024;

/// How an owner obtains its Context.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IsolateBoot {
    /// The shared bootstrap cache plus runtime startup state: what the
    /// in-tree tests and the compat runner use.
    CachedBootstrap,
    /// The editor's own runtime image, with the host's `--dump-file`
    /// override and the same final -> bootstrap -> source fallback chain
    /// the host applies to its main Context.
    RuntimeImage { dump_file_override: Option<PathBuf> },
}

impl IsolateBoot {
    fn context(&self) -> Result<Box<Context>, WorkerStartError> {
        let mut evaluator = match self {
            Self::CachedBootstrap => {
                let mut evaluator =
                    Box::new(create_bootstrap_evaluator_cached().map_err(|err| {
                        WorkerStartError::Initialization(format!("bootstrap evaluator: {err:?}"))
                    })?);
                evaluator.setup_thread_locals();
                if let Err(err) = apply_runtime_startup_state(&mut evaluator) {
                    let failure =
                        format!("runtime startup state: {:?}", eval_error_to_task_error(err));
                    return Err(WorkerStartError::Initialization(
                        match evaluator.shutdown() {
                            Ok(()) => failure,
                            Err(err) => format!("{failure}; worker shutdown: {err}"),
                        },
                    ));
                }
                evaluator
            }
            Self::RuntimeImage { dump_file_override } => {
                let role = if dump_file_override.is_some()
                    || runtime_image_available(RuntimeImageRole::Final)
                {
                    Some(RuntimeImageRole::Final)
                } else if runtime_image_available(RuntimeImageRole::Bootstrap) {
                    Some(RuntimeImageRole::Bootstrap)
                } else {
                    None
                };
                match role {
                    Some(role) => Box::new(
                        load_runtime_image_with_features(role, &[], dump_file_override.as_deref())
                            .map_err(|err| {
                                WorkerStartError::Initialization(format!(
                                    "runtime image {}: {:?}",
                                    role.image_file_name(),
                                    eval_error_to_task_error(err)
                                ))
                            })?,
                    ),
                    None => return Self::CachedBootstrap.context(),
                }
            }
        };
        evaluator.setup_thread_locals();
        // A worker has no terminal and no command loop: `message` and the
        // startup paths that consult this must take the batch branch.
        evaluator.set_variable("noninteractive", Value::T);
        Ok(evaluator)
    }
}

/// Pool construction parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IsolatePoolConfig {
    pub threads: usize,
    pub queue_capacity: usize,
    pub boot: IsolateBoot,
}

impl IsolatePoolConfig {
    /// `NEOMACS_WORKER_THREADS`, else the available parallelism, at least 1.
    pub fn default_threads() -> usize {
        std::env::var(THREADS_ENV)
            .ok()
            .and_then(|raw| raw.trim().parse::<usize>().ok())
            .filter(|n| *n > 0)
            .unwrap_or_else(|| {
                thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(1)
            })
    }

    pub fn new(boot: IsolateBoot) -> Self {
        Self {
            threads: Self::default_threads(),
            queue_capacity: 4096,
            boot,
        }
    }
}

/// Counters a host or `neomacs-worker-stats` reads.
#[derive(Clone, Debug, PartialEq)]
pub struct IsolateStats {
    pub threads: usize,
    pub started: bool,
    /// Owners that have reported a Context.
    pub ready: usize,
    pub startup_error: Option<String>,
    pub boot_max: Duration,
    pub boot_mean: Duration,
    pub enqueued: u64,
    pub completed: u64,
    pub cancelled: u64,
}

/// The pool: a [`WorkerRuntime`] of isolate owners started on demand.
pub struct IsolatePool {
    runtime: WorkerRuntime,
    config: IsolatePoolConfig,
    started: OnceLock<()>,
    startup_error: Mutex<Option<String>>,
    starter: Mutex<Option<thread::JoinHandle<()>>>,
}

static_assertions::assert_impl_all!(IsolatePool: Send, Sync);

impl IsolatePool {
    pub fn new(config: IsolatePoolConfig) -> Self {
        let boot = config.boot.clone();
        let factory: Arc<crate::ElispFactoryFn> = Arc::new(move || boot.context());
        let runtime = WorkerRuntime::with_isolate_executor(
            WorkerConfig {
                threads: config.threads,
                queue_capacity: config.queue_capacity,
            },
            factory,
        )
        .with_worker_stack_bytes(OWNER_STACK_BYTES);
        Self {
            runtime,
            config,
            started: OnceLock::new(),
            startup_error: Mutex::new(None),
            starter: Mutex::new(None),
        }
    }

    pub fn config(&self) -> &IsolatePoolConfig {
        &self.config
    }

    pub fn runtime(&self) -> &WorkerRuntime {
        &self.runtime
    }

    pub fn threads(&self) -> usize {
        self.config.threads
    }

    /// Start the owners the first time; later calls return at once. The
    /// owners boot on a helper thread, so the caller never waits for a
    /// Context to load; a task spawned before they are ready waits in the
    /// queue, and a startup failure fails every queued task.
    pub fn ensure_started(&'static self) {
        self.started.get_or_init(|| {
            let starter = thread::Builder::new()
                .name("neomacs-worker-starter".to_string())
                .spawn(move || {
                    if let Err(err) = self.runtime.try_start_workers() {
                        *self
                            .startup_error
                            .lock()
                            .expect("startup error mutex poisoned") = Some(err.to_string());
                    }
                });
            match starter {
                Ok(handle) => {
                    *self.starter.lock().expect("starter mutex poisoned") = Some(handle);
                }
                Err(err) => {
                    *self
                        .startup_error
                        .lock()
                        .expect("startup error mutex poisoned") =
                        Some(format!("starter thread: {err}"));
                    self.runtime.close();
                }
            }
        });
    }

    /// Block until the owners have all reported (tests and benchmarks that
    /// measure boot cost; the Lisp API never calls this).
    pub fn wait_started(&'static self) {
        self.ensure_started();
        let handle = self.starter.lock().expect("starter mutex poisoned").take();
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }

    pub fn stats(&self) -> IsolateStats {
        let boots = self.runtime.boot_times();
        let runtime = self.runtime.stats();
        let boot_max = boots.iter().copied().max().unwrap_or_default();
        let boot_mean = if boots.is_empty() {
            Duration::default()
        } else {
            boots.iter().sum::<Duration>() / boots.len() as u32
        };
        IsolateStats {
            threads: self.config.threads,
            started: self.started.get().is_some(),
            ready: boots.len(),
            startup_error: self
                .startup_error
                .lock()
                .expect("startup error mutex poisoned")
                .clone(),
            boot_max,
            boot_mean,
            enqueued: runtime.enqueued,
            completed: runtime.completed,
            cancelled: runtime.cancelled,
        }
    }
}

static POOL: OnceLock<IsolatePool> = OnceLock::new();

/// The pool was already installed; the earlier installation stands.
#[derive(Debug, thiserror::Error)]
#[error("the worker isolate pool is already installed")]
pub struct AlreadyInstalled;

/// Install the process's pool. Nothing starts until the first task.
pub fn install(config: IsolatePoolConfig) -> Result<&'static IsolatePool, AlreadyInstalled> {
    let mut fresh = false;
    let pool = POOL.get_or_init(|| {
        fresh = true;
        IsolatePool::new(config)
    });
    if fresh {
        Ok(pool)
    } else {
        Err(AlreadyInstalled)
    }
}

/// The installed pool, if the host installed one.
pub fn installed() -> Option<&'static IsolatePool> {
    POOL.get()
}

fn transfer_failure(err: &TransferError) -> TaskError {
    TaskError::Failed(Signal {
        symbol: TRANSFER_ERROR_SYMBOL.to_string(),
        data: Some(format!("({})", transfer_signal_text(err))),
    })
}

/// The printed form of [`TransferError::signal_data`], without a heap.
fn transfer_signal_text(err: &TransferError) -> String {
    match err {
        TransferError::Unsupported(type_name) => format!("unsupported-object {type_name}"),
        TransferError::Circular => "circular-object".to_string(),
        TransferError::TooDeep => "too-deep".to_string(),
        TransferError::UserHashTableTest => "user-defined-hash-table-test".to_string(),
        TransferError::ByteCodeLiteral(message) => {
            format!("byte-code-literal {}", lisp_string_literal(message))
        }
        TransferError::Decode(message) => {
            format!("malformed-wire {}", lisp_string_literal(message))
        }
    }
}

fn lisp_string_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Run one transfer-encoded call on an owner's Context: rebuild the
/// function and arguments in this heap, apply, and encode the result. A
/// signal inside the call is reported as the task's failure with its data
/// printed; a result that cannot cross back is a transfer error.
pub fn run_call(evaluator: &mut Context, form: &LispValue) -> Result<LispValue, TaskError> {
    evaluator.setup_thread_locals();
    let Call { function, args } =
        transfer::decode_call(&form.bytes).map_err(|err| transfer_failure(&err))?;
    let function =
        transfer::to_value(evaluator, &function).map_err(|err| transfer_failure(&err))?;
    let mut values = Vec::with_capacity(args.len());
    for arg in &args {
        values.push(transfer::to_value(evaluator, arg).map_err(|err| transfer_failure(&err))?);
    }
    let applied = evaluator.apply(function, values);
    let value = evaluator
        .finalize_public_eval_result(applied)
        .map_err(eval_error_to_task_error)?;
    match transfer::from_value(evaluator, value) {
        Ok(result) => Ok(LispValue {
            bytes: transfer::encode(&result),
        }),
        Err(err) => Err(TaskError::Failed(Signal {
            symbol: TRANSFER_ERROR_SYMBOL.to_string(),
            data: Some(format!(
                "({} {})",
                transfer_signal_text(&err),
                lisp_string_literal(&print_value_with_eval(evaluator, &value))
            )),
        })),
    }
}

/// Spawn a call on the installed pool, for Rust callers and tests. The
/// Lisp entry points in [`crate::lisp`] do the same through the pool's
/// scheduler.
pub fn spawn_call(
    pool: &'static IsolatePool,
    call: &Call,
) -> Result<neovm_core::TaskHandle, Signal> {
    pool.ensure_started();
    pool.runtime().spawn_task(
        LispValue {
            bytes: transfer::encode_call(call),
        },
        neovm_host_abi::TaskOptions::default(),
    )
}
