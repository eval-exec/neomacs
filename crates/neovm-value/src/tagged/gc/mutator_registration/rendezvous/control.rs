//! A1's control transitions, called only under the canonical registry gate.
//! Frozen membership and resume debt are indexes into that registry, never
//! owners of registrations, roots, or GC images. Callers validate those facts
//! before entering Tracing and perform all image transfers under the same gate.

use std::collections::VecDeque;
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

/// One committed serial-request identity. The generator refuses to wrap, so a
/// retired identity is never reused (L6 condition e).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::tagged::gc) struct RequestId(NonZeroU64);

impl RequestId {
    pub(in crate::tagged::gc) const fn get(self) -> u64 {
        self.0.get()
    }
}

/// A ring entry is waiting for the head grant or holding it. Retirement
/// removes the entry; a second retirement is a typed `Identity` refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::tagged::gc) enum RingPhase {
    Queued,
    Granted,
}

/// One committed serial request. The owner has already published its image and
/// root lease before enqueueing (L6 condition b); `generation` names that
/// publication and is re-validated by the keeper before a grant (via `ready`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::tagged::gc) struct RingEntry {
    id: RequestId,
    owner: MutatorKey,
    generation: NonZeroU64,
    phase: RingPhase,
}

impl RingEntry {
    pub(in crate::tagged::gc) const fn id(self) -> RequestId {
        self.id
    }

    pub(in crate::tagged::gc) const fn owner(self) -> MutatorKey {
        self.owner
    }

    pub(in crate::tagged::gc) const fn generation(self) -> NonZeroU64 {
        self.generation
    }

    pub(in crate::tagged::gc) const fn phase(self) -> RingPhase {
        self.phase
    }
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
    /// The FIFO serial-request ring (L6). Opaque metadata only; ordering, not
    /// authority: the canonical phase word and the image ledger still decide.
    pub(in crate::tagged::gc) serial_ring: VecDeque<RingEntry>,
    /// Monotonic request-id generator; a wrap is a typed refusal.
    pub(in crate::tagged::gc) next_request: u64,
    /// The one granted head, if any. Cleared only by `retire_serial`.
    pub(in crate::tagged::gc) ring_head_granted: Option<RequestId>,
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
            serial_ring: VecDeque::new(),
            next_request: 0,
            ring_head_granted: None,
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

    /// Start a stop epoch. A granted serial turn does not block activation:
    /// that owner is a frozen stop participant and must ACK/park at its next
    /// poll; once the phase leaves Idle no new serial grant is issued.
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

    /// Commit one serial request at the tail. The owner has already published
    /// its image and root lease (L6 condition b); `generation` names that
    /// publication. Stop requests are not enqueued here.
    pub(in crate::tagged::gc) fn enqueue_serial(
        &mut self,
        owner: MutatorKey,
        generation: NonZeroU64,
    ) -> Result<RequestId, RendezvousError> {
        self.live()?;
        // One entry per client: the Baton group is one ring client (L6 c), and
        // a leftover entry would let one owner hold two serial positions.
        if self.serial_ring.iter().any(|entry| entry.owner() == owner) {
            return Err(RendezvousError::Identity);
        }
        self.serial_ring
            .try_reserve(1)
            .map_err(|_| RendezvousError::Capacity)?;
        let next = self
            .next_request
            .checked_add(1)
            .ok_or(RendezvousError::Generation)?;
        let id = RequestId(NonZeroU64::new(next).ok_or(RendezvousError::Generation)?);
        self.serial_ring.push_back(RingEntry {
            id,
            owner,
            generation,
            phase: RingPhase::Queued,
        });
        self.next_request = next;
        Ok(id)
    }

    /// Grant the head exactly once. A stop (Stopping/Tracing) defers the grant
    /// without touching the ring; DrainingResumes may grant when no resume debt
    /// or Resuming member remains. `ready` is the keeper's publication check;
    /// an unready head is never skipped.
    pub(in crate::tagged::gc) fn try_grant_head(
        &mut self,
        membership: Membership<'_>,
        ready: impl FnOnce(&RingEntry) -> bool,
    ) -> Result<Option<RequestId>, RendezvousError> {
        self.live()?;
        // A pending stop wins: `request()` sets `queued` while a stop waits for
        // resume debt to drain, and no serial grant may be issued ahead of it.
        if self.queued {
            return Ok(None);
        }
        if self.ring_head_granted.is_some() {
            return Ok(None);
        }
        match self.phase {
            Phase::Idle | Phase::DrainingResumes => {}
            Phase::Stopping(_) | Phase::Tracing(_) => return Ok(None),
            Phase::Poisoned => return Err(RendezvousError::Poisoned),
        }
        if !self.debt.is_empty() || membership.iter().any(|(_, mode)| *mode == Mode::Resuming) {
            return Ok(None);
        }
        let Some(head) = self.serial_ring.front() else {
            return Ok(None);
        };
        if head.phase != RingPhase::Queued || !ready(head) {
            return Ok(None);
        }
        let id = head.id();
        self.serial_ring
            .front_mut()
            .expect("front observed above")
            .phase = RingPhase::Granted;
        self.ring_head_granted = Some(id);
        Ok(Some(id))
    }

    /// Grant the head inside the caller's own stop (L6.7 K3). A serial turn
    /// runs inside a stop, so its grant is issued there: only `Tracing` for
    /// the caller's epoch grants, which leaves no window where a granted head
    /// exists without a stop. One ticket is out at a time; `ready` is the
    /// keeper's publication check, and an unready head is never skipped.
    pub(in crate::tagged::gc) fn grant_head_in_stop(
        &mut self,
        epoch: NonZeroU64,
        ready: impl FnOnce(&RingEntry) -> bool,
    ) -> Result<Option<RequestId>, RendezvousError> {
        self.live()?;
        if self.phase != Phase::Tracing(epoch) {
            return Err(RendezvousError::Publication);
        }
        if self.ring_head_granted.is_some() {
            return Ok(None);
        }
        let Some(head) = self.serial_ring.front_mut() else {
            return Ok(None);
        };
        if head.phase != RingPhase::Queued || !ready(head) {
            return Ok(None);
        }
        head.phase = RingPhase::Granted;
        let id = head.id();
        self.ring_head_granted = Some(id);
        Ok(Some(id))
    }

    /// Retire a queued or granted entry exactly once. This stays available
    /// after poison so a granted-but-unreceived ticket is not lost (L6 f); a
    /// second retire is a typed refusal.
    pub(in crate::tagged::gc) fn retire_serial(
        &mut self,
        id: RequestId,
    ) -> Result<(), RendezvousError> {
        let Some(index) = self.serial_ring.iter().position(|entry| entry.id() == id) else {
            return Err(RendezvousError::Identity);
        };
        self.serial_ring.remove(index);
        if self.ring_head_granted == Some(id) {
            self.ring_head_granted = None;
        }
        Ok(())
    }

    pub(in crate::tagged::gc) fn ring_head(&self) -> Option<RequestId> {
        self.serial_ring.front().map(|entry| entry.id())
    }

    pub(in crate::tagged::gc) fn ring_len(&self) -> usize {
        self.serial_ring.len()
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
