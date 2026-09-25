//! The background backend threads (`neovm-jit-N`, P2.4 §3.3).
//!
//! A worker is not a mutator. It runs Cranelift on job payloads and
//! publishes entry addresses; it never touches the tagged heap, never runs
//! Lisp, never needs a GC handshake and never blocks the collector. This
//! file must name nothing of the Lisp heap (a test greps it). Each worker
//! owns a backend of its own (`compile::shared::WorkerBackend`): modules and
//! a code arena no eval thread shares, sealed read+execute before a result
//! is published.
//!
//! Detached: nothing joins a worker, and `kill-emacs` exits with jobs in
//! flight (their results are dropped with the process).

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use super::queue::{BackendJob, Pool};
use super::{BackendOut, WORKER_STATS};
use crate::emacs_core::jit::backend::BackendError;
use crate::emacs_core::jit::compile::CompileError;
use crate::emacs_core::jit::compile::shared::WorkerBackend;
use crate::emacs_core::jit::stats::asm_dump;

/// Stack of a worker thread: Cranelift recursion on large functions.
const WORKER_STACK_BYTES: usize = 16 << 20;

/// Start worker `index`, serving `pool` for the life of the process.
pub(super) fn spawn(index: usize, pool: &'static Pool) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name(format!("neovm-jit-{index}"))
        .stack_size(WORKER_STACK_BYTES)
        .spawn(move || run(pool))
        .map(drop)
}

/// Block the signals that are delivered to a process rather than raised by
/// an instruction, so the kernel picks a Lisp-relevant thread for them
/// (the quit and child signals, the profiler's timer). Faults stay
/// unblocked: a fault in the backend must be reported where it happens.
fn block_async_signals() {
    // SAFETY: plain signal-set manipulation on a local set, then this
    // thread's own mask.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigfillset(&mut set);
        for sig in [
            libc::SIGSEGV,
            libc::SIGBUS,
            libc::SIGFPE,
            libc::SIGILL,
            libc::SIGTRAP,
            libc::SIGABRT,
            libc::SIGSYS,
        ] {
            libc::sigdelset(&mut set, sig);
        }
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
    }
}

fn run(pool: &'static Pool) {
    block_async_signals();
    let mut backend = WorkerBackend::new();
    loop {
        let job = pool.pop();
        serve(&mut backend, job);
        pool.finish();
    }
}

/// Compile one job and publish its result (or skip it, cancelled).
fn serve(backend: &mut WorkerBackend, job: BackendJob) {
    let BackendJob {
        payload,
        enqueued_at,
        cell,
        ..
    } = job;
    if cell.is_cancelled() {
        WORKER_STATS.skipped.fetch_add(1, Ordering::Relaxed);
        return;
    }
    let started = Instant::now();
    let queue_wait_us = started.saturating_duration_since(enqueued_at).as_micros() as u64;
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        if super::take_forced_panic() {
            panic!("backend panic forced by a test");
        }
        backend.define(payload)
    }));
    let result = match outcome {
        Ok(result) => result,
        Err(_) => {
            // The backend's module and contexts may be mid-update: start a
            // fresh one (the old code stays mapped, as always).
            *backend = WorkerBackend::new();
            WORKER_STATS.panics.fetch_add(1, Ordering::Relaxed);
            tracing::error!(target: "neovm_jit::bg", "a JIT backend job panicked; the body stays interpreted");
            Err(CompileError::Backend(BackendError::Define(
                "the background backend panicked".into(),
            )))
        }
    };
    let backend_us = started.elapsed().as_micros() as u64;
    let asm = asm_dump::take_stashed();
    if let Ok(code) = &result {
        WORKER_STATS
            .code_bytes
            .fetch_add(code.code_bytes as u64, Ordering::Relaxed);
    }
    WORKER_STATS.jobs.fetch_add(1, Ordering::Relaxed);
    add_max(&WORKER_STATS.backend_max_us, backend_us);
    cell.publish(BackendOut {
        result: result.map(|code| code.entry),
        backend_us,
        queue_wait_us,
        asm,
    });
}

fn add_max(cell: &AtomicU64, value: u64) {
    cell.fetch_max(value, Ordering::Relaxed);
}
