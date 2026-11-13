//! A1's control transitions, called only under the canonical registry gate.
//! Frozen membership and resume debt are indexes into that registry, never
//! owners of registrations, roots, or GC images. Callers validate those facts
//! before entering Tracing and perform all image transfers under the same gate.

use std::num::NonZeroU64;

use super::super::MutatorKey;
use super::RendezvousError;

type Membership<'a> = &'a [(MutatorKey, Mode)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Stopping(NonZeroU64),
    Tracing(NonZeroU64),
    DrainingResumes,
    Poisoned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Running,
    Publishing,
    Stopped,
    NativeStopped,
    Resuming,
    StartingStopped,
    Retired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    Active(NonZeroU64),
    Queued,
}

#[derive(Debug)]
pub(in crate::tagged::gc) struct OwnerState {
    pub(in crate::tagged::gc) mode: Mode,
    pub(in crate::tagged::gc) coverage_epoch: u64,
    pub(in crate::tagged::gc) retiring: bool,
    pub(in crate::tagged::gc) image_epoch: u64,
    pub(in crate::tagged::gc) image_takes: usize,
}

impl OwnerState {
    pub(in crate::tagged::gc) const fn new(mode: Mode) -> Self {
        Self {
            mode,
            coverage_epoch: 0,
            retiring: false,
            image_epoch: 0,
            image_takes: 0,
        }
    }
}

#[derive(Debug)]
pub(in crate::tagged::gc) struct State {
    pub(in crate::tagged::gc) phase: Phase,
    pub(in crate::tagged::gc) epoch: u64,
    pub(in crate::tagged::gc) completed: u64,
    pub(in crate::tagged::gc) queued: bool,
    pub(in crate::tagged::gc) frozen: Vec<MutatorKey>,
    pub(in crate::tagged::gc) debt: Vec<MutatorKey>,
    pub(in crate::tagged::gc) traces: usize,
    pub(in crate::tagged::gc) active_readers: usize,
}

impl State {
    pub(in crate::tagged::gc) const fn new() -> Self {
        Self {
            phase: Phase::Idle,
            epoch: 0,
            completed: 0,
            queued: false,
            frozen: Vec::new(),
            debt: Vec::new(),
            traces: 0,
            active_readers: 0,
        }
    }

    pub(in crate::tagged::gc) fn live(&self) -> Result<(), RendezvousError> {
        if self.phase == Phase::Poisoned {
            Err(RendezvousError::Poisoned)
        } else {
            Ok(())
        }
    }

    pub(in crate::tagged::gc) fn poison(&mut self) {
        // Retain frozen keys, debt, and counters: poison discharges nothing.
        self.phase = Phase::Poisoned;
    }

    pub(in crate::tagged::gc) fn request(
        &mut self,
        membership: Membership<'_>,
    ) -> Result<Request, RendezvousError> {
        self.live()?;
        if let Phase::Stopping(epoch) | Phase::Tracing(epoch) = self.phase {
            return Ok(Request::Active(epoch));
        }
        if self.phase == Phase::DrainingResumes || !self.debt.is_empty() {
            self.queued = true;
            return Ok(Request::Queued);
        }
        self.activate(membership).map(Request::Active)
    }

    pub(in crate::tagged::gc) fn activate(
        &mut self,
        membership: Membership<'_>,
    ) -> Result<NonZeroU64, RendezvousError> {
        self.live()?;
        if self.phase != Phase::Idle
            || !self.debt.is_empty()
            || membership.iter().any(|(_, mode)| *mode == Mode::Resuming)
        {
            return Err(RendezvousError::Busy);
        }
        validate_membership(membership)?;
        let next = self
            .epoch
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .ok_or(RendezvousError::Generation)?;
        let mut frozen = Vec::new();
        frozen
            .try_reserve_exact(membership.len())
            .map_err(|_| RendezvousError::Capacity)?;
        frozen.extend(membership.iter().filter_map(|(key, mode)| {
            (!matches!(mode, Mode::Retired | Mode::StartingStopped)).then_some(*key)
        }));
        self.frozen = frozen;
        self.epoch = next.get();
        self.phase = Phase::Stopping(next);
        self.queued = false;
        Ok(next)
    }

    /// Control readiness only. The caller must separately validate publication
    /// identities, root readers, coverage epochs, images, and outstanding loans.
    pub(in crate::tagged::gc) fn ready(
        &self,
        membership: Membership<'_>,
    ) -> Result<bool, RendezvousError> {
        self.live()?;
        let Phase::Stopping(epoch) = self.phase else {
            return Ok(false);
        };
        if epoch.get() != self.epoch {
            return Err(RendezvousError::Publication);
        }
        self.stopped_membership(membership)
    }

    pub(in crate::tagged::gc) fn begin_trace(
        &mut self,
        epoch: NonZeroU64,
        membership: Membership<'_>,
    ) -> Result<(), RendezvousError> {
        self.live()?;
        match self.phase {
            Phase::Stopping(current) if current == epoch => {}
            Phase::Stopping(_) => return Err(RendezvousError::Publication),
            _ => return Err(RendezvousError::Busy),
        }
        if !self.ready(membership)? {
            return Err(RendezvousError::Busy);
        }
        self.phase = Phase::Tracing(epoch);
        Ok(())
    }

    /// Commit completion after the caller finishes every actual reader and
    /// installs the latest per-owner image. A4 performs no collector folding.
    pub(in crate::tagged::gc) fn complete(
        &mut self,
        epoch: NonZeroU64,
        membership: Membership<'_>,
    ) -> Result<(), RendezvousError> {
        self.live()?;
        match self.phase {
            Phase::Tracing(current) if current == epoch => {}
            Phase::Tracing(_) => return Err(RendezvousError::Publication),
            _ => return Err(RendezvousError::Busy),
        }
        if self.active_readers != 0 {
            return Err(RendezvousError::Readers);
        }
        if self.completed >= epoch.get() {
            return Err(RendezvousError::Publication);
        }
        if !self.stopped_membership(membership)? {
            return Err(RendezvousError::Busy);
        }
        let traces = self
            .traces
            .checked_add(1)
            .ok_or(RendezvousError::Generation)?;
        let mut debt = Vec::new();
        debt.try_reserve_exact(self.frozen.len())
            .map_err(|_| RendezvousError::Capacity)?;
        for key in &self.frozen {
            if mode_for(membership, *key)? == Mode::Stopped {
                debt.push(*key);
            }
        }
        self.debt = debt;
        self.completed = epoch.get();
        self.traces = traces;
        self.phase = Phase::DrainingResumes;
        Ok(())
    }

    pub(in crate::tagged::gc) fn begin_restore(
        &mut self,
        key: MutatorKey,
        owner: &mut OwnerState,
        native: bool,
    ) -> Result<u64, RendezvousError> {
        self.live()?;
        if !matches!(self.phase, Phase::Idle | Phase::DrainingResumes) {
            return Err(RendezvousError::Busy);
        }
        let expected = if native {
            Mode::NativeStopped
        } else {
            Mode::Stopped
        };
        if owner.mode != expected || owner.retiring {
            return Err(RendezvousError::Busy);
        }
        if owner.image_epoch > self.completed || (!native && owner.image_epoch != self.completed) {
            return Err(RendezvousError::Publication);
        }
        let owes_debt = self.debt.contains(&key);
        if native {
            if owes_debt {
                return Err(RendezvousError::Publication);
            }
            self.debt
                .try_reserve(1)
                .map_err(|_| RendezvousError::Capacity)?;
        } else if !owes_debt {
            return Err(RendezvousError::Publication);
        }
        // Every fallible preflight precedes both the native debt reservation
        // and Resuming. A request cannot activate between those transitions.
        if native {
            self.debt.push(key);
        }
        owner.mode = Mode::Resuming;
        self.phase = Phase::DrainingResumes;
        Ok(owner.image_epoch)
    }

    pub(in crate::tagged::gc) fn finish_restore(
        &mut self,
        key: MutatorKey,
        owner: &mut OwnerState,
    ) -> Result<(), RendezvousError> {
        self.live()?;
        if self.phase != Phase::DrainingResumes {
            return Err(RendezvousError::Busy);
        }
        if owner.mode != Mode::Resuming || owner.retiring {
            return Err(RendezvousError::Publication);
        }
        let index = self
            .debt
            .iter()
            .position(|candidate| *candidate == key)
            .ok_or(RendezvousError::Publication)?;
        let takes = owner
            .image_takes
            .checked_add(1)
            .ok_or(RendezvousError::Generation)?;
        owner.image_takes = takes;
        owner.mode = Mode::Running;
        self.debt.swap_remove(index);
        Ok(())
    }

    /// Return true only when this call opens Idle. The wrapper promotes every
    /// StartingStopped owner under the same gate, then notifies the keeper.
    pub(in crate::tagged::gc) fn finish_debt(
        &mut self,
        membership: Membership<'_>,
    ) -> Result<bool, RendezvousError> {
        self.live()?;
        if self.phase != Phase::DrainingResumes {
            return if self.phase == Phase::Idle && self.debt.is_empty() {
                Ok(false)
            } else {
                Err(RendezvousError::Busy)
            };
        }
        if !self.debt.is_empty() {
            return Ok(false);
        }
        validate_membership(membership)?;
        if membership.iter().any(|(_, mode)| *mode == Mode::Resuming) {
            return Err(RendezvousError::Busy);
        }
        self.phase = Phase::Idle;
        Ok(true)
    }

    fn stopped_membership(&self, membership: Membership<'_>) -> Result<bool, RendezvousError> {
        validate_membership(membership)?;
        if !self.debt.is_empty() || membership.iter().any(|(_, mode)| *mode == Mode::Resuming) {
            return Err(RendezvousError::Busy);
        }
        let mut ready = true;
        for key in &self.frozen {
            match mode_for(membership, *key)? {
                Mode::Stopped | Mode::NativeStopped => {}
                Mode::Retired => return Err(RendezvousError::Identity),
                _ => ready = false,
            }
        }
        Ok(ready)
    }
}

fn mode_for(membership: Membership<'_>, key: MutatorKey) -> Result<Mode, RendezvousError> {
    membership
        .iter()
        .find_map(|(candidate, mode)| (*candidate == key).then_some(*mode))
        .ok_or(RendezvousError::Identity)
}

fn validate_membership(membership: Membership<'_>) -> Result<(), RendezvousError> {
    for (index, (key, _)) in membership.iter().enumerate() {
        if membership[..index].iter().any(|(seen, _)| seen == key) {
            return Err(RendezvousError::Identity);
        }
        if let Some((first, _)) = membership.first()
            && (key.heap != first.heap || key.world != first.world)
        {
            return Err(RendezvousError::Identity);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "control_tests.rs"]
mod tests;
