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
use std::sync::atomic::Ordering;
use std::time::Instant;

use super::WORKER_STATS;
use super::queue::{BackendJob, Pool};
use crate::emacs_core::jit::compile::CompileError;
use crate::emacs_core::jit::compile::shared::WorkerBackend;
use crate::emacs_core::jit::compile::shared::batch::{BatchCapacity, WORKER_BATCH_LIMIT};
use crate::emacs_core::jit::stats::asm_dump;

mod completion;
mod hooks;
use completion::{CpuCharge, Failure, FailureStage, FinishOnDrop, Member, elapsed_cpu};
use hooks::BatchHooks;

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

/// `NEOVM_JIT_BG_NICE` and `NEOVM_JIT_BG_AFFINITY` for this worker.
fn apply_scheduling_knobs() {
    if let Some(nice) = super::worker_nice() {
        // SAFETY: setpriority on this thread's own id.
        let rc = unsafe {
            libc::setpriority(
                libc::PRIO_PROCESS,
                libc::syscall(libc::SYS_gettid) as libc::id_t,
                nice,
            )
        };
        if rc != 0 {
            tracing::warn!(target: "neovm_jit::bg", nice, "NEOVM_JIT_BG_NICE was refused");
        }
    }
    if let Some(cpus) = super::worker_affinity() {
        // SAFETY: a zeroed cpu_set_t filled with CPU_SET, applied to this
        // thread (pid 0).
        let rc = unsafe {
            let mut set: libc::cpu_set_t = std::mem::zeroed();
            for cpu in &cpus {
                libc::CPU_SET(cpu.index(), &mut set);
            }
            libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set)
        };
        if rc != 0 {
            tracing::warn!(target: "neovm_jit::bg", ?cpus, "NEOVM_JIT_BG_AFFINITY was refused");
        }
    }
}

struct PoppedJob<'a> {
    job: BackendJob,
    finish: FinishOnDrop<'a>,
}

impl<'a> PoppedJob<'a> {
    fn new(pool: &'a Pool, job: BackendJob) -> Self {
        #[cfg(test)]
        if pool.is_global_for_worker_test() {
            super::note_served_for_test(job.class, job.seq, job.cell.is_cancelled());
        }
        Self {
            job,
            finish: FinishOnDrop(pool),
        }
    }
}

fn run(pool: &'static Pool) {
    block_async_signals();
    apply_scheduling_knobs();
    let mut backend = WorkerBackend::new();
    let mut carry = None;
    loop {
        let first = carry
            .take()
            .unwrap_or_else(|| PoppedJob::new(pool, pool.pop()));
        carry = serve_batch(&mut backend, pool, first, &mut BatchHooks::default());
    }
}

/// Compile only immediately ready jobs, never wait with unsealed code.
/// `carry` already owns its dequeue lease: a module-limit boundary seals
/// prior jobs, then starts this one without requeueing/reprioritizing it.
fn serve_batch<'a>(
    backend: &mut WorkerBackend,
    pool: &'a Pool,
    first: PoppedJob<'a>,
    hooks: &mut BatchHooks,
) -> Option<PoppedJob<'a>> {
    let mut batch = backend.batch();
    let mut current = Some(first);
    let mut carry = None;
    let mut members = Vec::with_capacity(WORKER_BATCH_LIMIT);
    let mut failure = None;
    let mut considered = 0;
    while let Some(popped) = current.take() {
        considered += 1;
        if popped.job.cell.is_cancelled() {
            WORKER_STATS.skipped.fetch_add(1, Ordering::Relaxed);
            drop(popped);
        } else {
            match batch.capacity(popped.job.payload.regalloc) {
                BatchCapacity::SealFirst => {
                    carry = Some(popped);
                    break;
                }
                // An Aborted guard rejects prepare through its own state check.
                BatchCapacity::Available | BatchCapacity::Aborted => {}
            }
            let PoppedJob { job, finish } = popped;
            let BackendJob {
                payload,
                enqueued_at,
                cell,
                class,
                seq,
                ..
            } = job;
            if super::stress_enabled() {
                std::thread::sleep(std::time::Duration::from_micros(
                    super::stress_mix(seq) % 2_001,
                ));
            }
            // A cancellation during the stress delay can still avoid codegen.
            if cell.is_cancelled() {
                WORKER_STATS.skipped.fetch_add(1, Ordering::Relaxed);
                drop(finish);
            } else {
                // Each definition owns its own asm stash; stale text from an
                // aborted attempt must never contaminate the following job.
                let _ = asm_dump::take_stashed();
                let started = Instant::now();
                let cpu_started = (class == super::JobClass::Upgrade)
                    .then(crate::emacs_core::jit::tier2::cpu_time_us);
                let queue_wait_us =
                    started.saturating_duration_since(enqueued_at).as_micros() as u64;
                let index = members.len();
                let prepared = catch_unwind(AssertUnwindSafe(|| {
                    batch.prepare(payload)?;
                    // Force a panic after real preparation: this tests the
                    // worst unpublished-code case instead of an untouched module.
                    if take_forced_panic(pool) {
                        panic!("backend panic forced by a test");
                    }
                    hooks.prepared(seq, index);
                    Ok::<(), CompileError>(())
                }));
                let codegen_us = started.elapsed().as_micros() as u64;
                let codegen_cpu_us =
                    cpu_started.map_or(0, |started| elapsed_cpu(started, codegen_us));
                let asm = asm_dump::take_stashed();
                members.push(Member {
                    cell,
                    class,
                    codegen_us,
                    codegen_cpu_us,
                    queue_wait_us,
                    asm,
                    finish,
                });
                match prepared {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        failure = Some(Failure::error(FailureStage::Prepare, error));
                        break;
                    }
                    Err(_) => {
                        failure = Some(Failure::panic(FailureStage::Prepare));
                        break;
                    }
                }
            }
        }
        if considered >= WORKER_BATCH_LIMIT {
            break;
        }
        current = pool.try_pop().map(|job| PoppedJob::new(pool, job));
    }
    if members.is_empty() {
        // All selected jobs were already cancelled; Empty Drop does no reset.
        drop(batch);
        return carry;
    }
    WORKER_STATS.batches.fetch_add(1, Ordering::Relaxed);
    let charge_cpu = if members
        .iter()
        .any(|member| member.class == super::JobClass::Upgrade)
    {
        CpuCharge::ContainsUpgrade
    } else {
        CpuCharge::EntryOnly
    };
    let mut completed = completion::seal(batch, members.len(), charge_cpu, failure, hooks);
    if completed.requires_reset(members.len()) {
        let started = Instant::now();
        let cpu_started = charge_cpu.start();
        *backend = WorkerBackend::new();
        let cleanup_us = started.elapsed().as_micros() as u64;
        completed.charge_cleanup(
            cleanup_us,
            cpu_started.map_or(0, |started| elapsed_cpu(started, cleanup_us)),
        );
    }
    completed.publish(members, hooks);
    carry
}

fn take_forced_panic(_pool: &Pool) -> bool {
    #[cfg(test)]
    if !_pool.is_global_for_worker_test() {
        return false;
    }
    super::take_forced_panic()
}

#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
#[path = "worker/tests/batch_test.rs"]
mod batch_tests;
