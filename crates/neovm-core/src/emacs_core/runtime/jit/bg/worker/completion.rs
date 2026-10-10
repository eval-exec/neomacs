//! Running-count leases and atomic whole-batch completion. Plain backend data only.
use super::super::queue::Pool;
use super::super::{BackendOut, JobCell, JobClass, WORKER_STATS};
use super::hooks::BatchHooks;
use crate::emacs_core::jit::backend::BackendError;
use crate::emacs_core::jit::compile::CompileError;
use crate::emacs_core::jit::compile::shared::batch::{SealedBatch, WorkerBatch};
use crate::emacs_core::jit::stats::asm_dump::{self, PendingAsm};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

pub(super) struct FinishOnDrop<'a>(pub(super) &'a Pool);

impl Drop for FinishOnDrop<'_> {
    fn drop(&mut self) {
        self.0.finish();
    }
}

pub(super) struct Member<'a> {
    pub(super) cell: Arc<JobCell>,
    pub(super) class: JobClass,
    pub(super) codegen_us: u64,
    pub(super) codegen_cpu_us: u64,
    pub(super) queue_wait_us: u64,
    pub(super) asm: Option<PendingAsm>,
    pub(super) finish: FinishOnDrop<'a>,
}

#[derive(Clone, Copy)]
pub(super) enum FailureStage {
    Prepare,
    Finalize,
}

pub(super) struct Failure {
    stage: FailureStage,
    message: String,
}

impl Failure {
    pub(super) fn error(stage: FailureStage, error: CompileError) -> Self {
        Self {
            stage,
            message: error.to_string(),
        }
    }

    pub(super) fn panic(stage: FailureStage) -> Self {
        WORKER_STATS.panics.fetch_add(1, Ordering::Relaxed);
        tracing::error!(target: "neovm_jit::bg", "a JIT backend batch panicked; unpublished jobs remain interpreted");
        Self {
            stage,
            message: "the background backend batch panicked".into(),
        }
    }

    fn for_member(&self) -> CompileError {
        CompileError::Backend(match self.stage {
            FailureStage::Prepare => BackendError::Define(self.message.clone()),
            FailureStage::Finalize => BackendError::Finalize(self.message.clone()),
        })
    }
}

enum BatchOutcome {
    Sealed(SealedBatch),
    Failed(Failure),
}

pub(super) struct Completion {
    outcome: BatchOutcome,
    finalize_us: u64,
    finalize_cpu_us: u64,
}

#[derive(Clone, Copy)]
pub(super) enum CpuCharge {
    EntryOnly,
    ContainsUpgrade,
}

impl CpuCharge {
    pub(super) fn start(self) -> Option<u64> {
        match self {
            Self::EntryOnly => None,
            Self::ContainsUpgrade => Some(crate::emacs_core::jit::tier2::cpu_time_us()),
        }
    }
}

pub(super) fn seal(
    batch: WorkerBatch<'_>,
    member_count: usize,
    charge_cpu: CpuCharge,
    failure: Option<Failure>,
    hooks: &mut BatchHooks,
) -> Completion {
    let finalize_started = Instant::now();
    let finalize_cpu_started = charge_cpu.start();
    let outcome = if let Some(error) = failure {
        drop(batch); // Guard resets any half-defined compiler bookkeeping.
        BatchOutcome::Failed(error)
    } else {
        let sealed = catch_unwind(AssertUnwindSafe(|| {
            hooks.before_finalize(member_count);
            batch.finish()
        }));
        match sealed {
            Ok(Ok(sealed)) => BatchOutcome::Sealed(sealed),
            Ok(Err(error)) => BatchOutcome::Failed(Failure::error(FailureStage::Finalize, error)),
            Err(_) => BatchOutcome::Failed(Failure::panic(FailureStage::Finalize)),
        }
    };
    let finalize_us = finalize_started.elapsed().as_micros() as u64;
    let finalize_cpu_us =
        finalize_cpu_started.map_or(0, |started| elapsed_cpu(started, finalize_us));

    Completion {
        outcome,
        finalize_us,
        finalize_cpu_us,
    }
}

impl Completion {
    /// A mismatched guard result is never allowed to publish a prefix.
    pub(super) fn requires_reset(&mut self, expected: usize) -> bool {
        if matches!(&self.outcome, BatchOutcome::Sealed(sealed) if sealed.codes.len() != expected) {
            self.outcome = BatchOutcome::Failed(Failure {
                stage: FailureStage::Finalize,
                message: "a sealed worker batch returned the wrong number of entries".into(),
            });
            true
        } else {
            false
        }
    }

    pub(super) fn charge_cleanup(&mut self, wall_us: u64, cpu_us: u64) {
        self.finalize_us = self.finalize_us.saturating_add(wall_us);
        self.finalize_cpu_us = self.finalize_cpu_us.saturating_add(cpu_us);
    }

    pub(super) fn publish(self, members: Vec<Member<'_>>, hooks: &mut BatchHooks) {
        let _ = asm_dump::take_stashed();
        let (codes, error) = match self.outcome {
            BatchOutcome::Sealed(sealed) => {
                WORKER_STATS
                    .published_modules_finalized
                    .fetch_add(sealed.modules_finalized as u64, Ordering::Relaxed);
                WORKER_STATS
                    .published_arena_seals
                    .fetch_add(sealed.arena_seals, Ordering::Relaxed);
                (sealed.codes, None)
            }
            BatchOutcome::Failed(error) => {
                WORKER_STATS.failed_batches.fetch_add(1, Ordering::Relaxed);
                (Vec::new(), Some(error))
            }
        };
        let count = members.len();
        hooks.before_publish(count, error.is_none());
        let mut codes = codes.into_iter();
        for (index, member) in members.into_iter().enumerate() {
            let Member {
                cell,
                class,
                codegen_us,
                codegen_cpu_us,
                queue_wait_us,
                asm,
                finish,
            } = member;
            let backend_us = codegen_us.saturating_add(share(self.finalize_us, index, count));
            let backend_cpu_us = if class == JobClass::Upgrade {
                codegen_cpu_us.saturating_add(share(self.finalize_cpu_us, index, count))
            } else {
                0
            };
            let result = if let Some(error) = &error {
                Err(error.for_member())
            } else {
                // Length checked above: every member has exactly one sealed code.
                let code = codes.next().expect("sealed entry count was validated");
                WORKER_STATS
                    .code_bytes
                    .fetch_add(code.code_bytes as u64, Ordering::Relaxed);
                Ok(code.entry)
            };
            WORKER_STATS.jobs.fetch_add(1, Ordering::Relaxed);
            add_max(&WORKER_STATS.backend_max_us, backend_us);
            cell.publish(BackendOut {
                result,
                backend_us,
                backend_cpu_us,
                queue_wait_us,
                asm: if error.is_none() { asm } else { None },
                dropped: false,
            });
            drop(finish); // Publish before releasing this job's running-count lease.
        }
    }
}

/// Divide an interval once, retaining the remainder instead of rounding
/// every member upward. All shares sum exactly to the interval.
pub(super) fn share(total: u64, index: usize, count: usize) -> u64 {
    let count = count as u64;
    total / count + u64::from((index as u64) < total % count)
}

pub(super) fn elapsed_cpu(started: u64, wall_us: u64) -> u64 {
    let finished = crate::emacs_core::jit::tier2::cpu_time_us();
    if started == 0 || finished == 0 {
        wall_us
    } else {
        finished.saturating_sub(started)
    }
}

fn add_max(cell: &AtomicU64, value: u64) {
    cell.fetch_max(value, Ordering::Relaxed);
}
