//! Bounded queue admission independent of workers and process knobs.

use std::num::{NonZeroU64, NonZeroUsize};

use super::{BackendJob, BinaryHeap, PoolState, Queued};

/// Positive limits preserve the knob policy; there is no disabled-queue mode.
#[derive(Clone, Copy, Debug)]
pub(super) struct QueueLimits {
    jobs: NonZeroUsize,
    insts: NonZeroU64,
}

impl QueueLimits {
    pub(super) const fn new(jobs: NonZeroUsize, insts: NonZeroU64) -> Self {
        Self { jobs, insts }
    }

    /// Knobs already clamp to one; preserve that policy for test overrides.
    pub(super) fn configured() -> Self {
        Self::new(
            NonZeroUsize::new(super::super::queue_cap()).unwrap_or(NonZeroUsize::MIN),
            NonZeroU64::new(super::super::queue_insts_cap()).unwrap_or(NonZeroU64::MIN),
        )
    }

    pub(super) fn fits(self, insts: u64) -> bool {
        insts <= self.insts.get()
    }
}

impl PoolState {
    /// Err returns an unqueued job for the existing caller fallback.
    /// An intrinsically oversized job instead publishes a dropped result,
    /// preserving queued victims and install-time heat-doubling behavior.
    pub(super) fn push_bounded(
        &mut self,
        job: BackendJob,
        limits: QueueLimits,
    ) -> Result<(), BackendJob> {
        if !limits.fits(job.insts) {
            job.cell.publish_dropped();
            return Ok(());
        }
        // Checked subtraction is now safe: the job fits the positive cap.
        // Comparing remaining capacity avoids overflow in insts + job.insts.
        let remaining = limits.insts.get() - job.insts;
        while !self.jobs.is_empty()
            && (self.jobs.len() >= limits.jobs.get() || self.insts > remaining)
        {
            let victim = self.jobs.iter().map(Queued::key).max().expect("not empty");
            if victim < (job.class, job.seq) {
                return Err(job);
            }
            let mut jobs = std::mem::take(&mut self.jobs).into_vec();
            let at = jobs
                .iter()
                .position(|queued| queued.key() == victim)
                .expect("the victim is queued");
            let Queued(dropped) = jobs.swap_remove(at);
            self.jobs = BinaryHeap::from(jobs);
            self.insts -= dropped.insts;
            dropped.cell.publish_dropped();
        }
        // The remaining-capacity comparison (including the empty queue case)
        // proves this addition fits in the instruction cap and therefore u64.
        self.insts += job.insts;
        self.jobs.push(Queued(job));
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/admission_test.rs"]
mod tests;
