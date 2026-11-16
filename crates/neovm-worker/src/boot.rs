//! Context construction for one owner thread.

use crate::isolate::IsolateBoot;
use crate::{WorkerStartError, eval_error_to_task_error};
use neovm_core::Value;
use neovm_core::emacs_core::eval::Context;
use neovm_core::emacs_core::load::{
    RuntimeImageRole, apply_runtime_startup_state, runtime_image_available,
};
use neovm_core::emacs_core::load_boot::{
    create_bootstrap_evaluator_cached, load_runtime_image_with_features,
};
use std::time::{Duration, Instant};

const TRACE_ENV: &str = "NEOMACS_WORKER_BOOT_TRACE";

/// Build the Lisp Context owned by one worker thread.
pub(crate) fn context(boot: &IsolateBoot) -> Result<Box<Context>, WorkerStartError> {
    let started = Instant::now();
    let result = context_inner(boot);
    trace("owner_context_total", started.elapsed());
    result
}

fn context_inner(boot: &IsolateBoot) -> Result<Box<Context>, WorkerStartError> {
    let mut evaluator = match boot {
        IsolateBoot::CachedBootstrap => {
            let started = Instant::now();
            let mut evaluator = Box::new(create_bootstrap_evaluator_cached().map_err(|err| {
                WorkerStartError::Initialization(format!("bootstrap evaluator: {err:?}"))
            })?);
            trace("bootstrap_image_load_and_activation", started.elapsed());

            let started = Instant::now();
            evaluator.setup_thread_locals();
            trace("setup_thread_locals_initial", started.elapsed());

            let started = Instant::now();
            if let Err(err) = apply_runtime_startup_state(&mut evaluator) {
                let failure = format!("runtime startup state: {:?}", eval_error_to_task_error(err));
                return Err(WorkerStartError::Initialization(
                    match evaluator.shutdown() {
                        Ok(()) => failure,
                        Err(err) => format!("{failure}; worker shutdown: {err}"),
                    },
                ));
            }
            trace("runtime_startup_state", started.elapsed());
            evaluator
        }
        IsolateBoot::RuntimeImage { dump_file_override } => {
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
                Some(role) => {
                    let started = Instant::now();
                    let evaluator =
                        load_runtime_image_with_features(role, &[], dump_file_override.as_deref())
                            .map_err(|err| {
                                WorkerStartError::Initialization(format!(
                                    "runtime image {}: {:?}",
                                    role.image_file_name(),
                                    eval_error_to_task_error(err)
                                ))
                            })?;
                    trace(
                        "runtime_image_open_map_relocate_symbol_fixups_interner_activation",
                        started.elapsed(),
                    );
                    Box::new(evaluator)
                }
                None => return context_inner(&IsolateBoot::CachedBootstrap),
            }
        }
    };

    let started = Instant::now();
    evaluator.setup_thread_locals();
    trace("setup_thread_locals_final", started.elapsed());

    // A worker has no terminal and no command loop: `message` and the
    // startup paths that consult this must take the batch branch.
    let started = Instant::now();
    evaluator.set_variable("noninteractive", Value::T);
    trace("set_noninteractive", started.elapsed());
    Ok(evaluator)
}

fn trace(phase: &str, elapsed: Duration) {
    if std::env::var_os(TRACE_ENV).as_deref() == Some(std::ffi::OsStr::new("1")) {
        eprintln!(
            "neomacs-worker-boot phase={phase} elapsed_ms={:.3}",
            elapsed.as_secs_f64() * 1000.0
        );
    }
}
