//! Latest pending presentation per logical frame. Taking and replacing a
//! presentation use the same lock, so the producer can retire a replaced
//! revision knowing that no consumer has acquired it.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crossbeam_channel::{Receiver, SendError, Sender, TryRecvError, TrySendError, bounded};
use neomacs_display_protocol::SealedFramePresentation;

pub struct QueuedPresentation {
    pub(crate) state: SealedFramePresentation,
    pub(crate) skipped_predecessor: bool,
}

/// Proof that a revision was replaced while still in the mailbox. It was
/// never acquired by the renderer and may be retired by the evaluator.
#[must_use = "retire the superseded evaluator presentation with discard"]
#[derive(Debug)]
pub struct SupersededPresentation(SealedFramePresentation);

impl std::ops::Deref for SupersededPresentation {
    type Target = SealedFramePresentation;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::Deref for QueuedPresentation {
    type Target = SealedFramePresentation;
    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

type Pending = Arc<Mutex<VecDeque<QueuedPresentation>>>;

#[derive(Clone)]
pub struct FrameSender {
    pending: Pending,
    wake: Arc<Sender<()>>,
}

#[derive(Clone)]
pub struct FrameReceiver {
    pending: Pending,
    wake: std::sync::Weak<Sender<()>>,
    available: Receiver<()>,
}

pub(super) fn channel() -> (FrameSender, FrameReceiver) {
    let pending = Pending::default();
    let (wake, available) = bounded(1);
    let wake = Arc::new(wake);
    (
        FrameSender {
            pending: pending.clone(),
            wake: wake.clone(),
        },
        FrameReceiver {
            pending,
            wake: Arc::downgrade(&wake),
            available,
        },
    )
}

impl FrameSender {
    /// Submit without waiting for rendering. The returned revision was never
    /// acquired by the consumer; the evaluator MUST retire its presentation
    /// and interaction records. Independent logical frames retain their own
    /// slots, ordered by their latest submission.
    ///
    /// Storage is bounded by pending logical frames, not scroll-event count.
    /// This is not a global byte limit or a limit on the number of windows.
    pub fn submit(
        &self,
        state: SealedFramePresentation,
    ) -> Result<Option<SupersededPresentation>, SendError<SealedFramePresentation>> {
        let mut pending = self.pending.lock().unwrap();
        // Wake under the lock: the receiver cannot observe an empty slot
        // between notification and publication. A full notification channel
        // already carries a wake; it does not mean the presentation is lost.
        match self.wake.try_send(()) {
            Err(TrySendError::Disconnected(_)) => return Err(SendError(state)),
            Ok(()) | Err(TrySendError::Full(_)) => {}
        }
        let frame = state.frame_placement.frame();
        let old = pending
            .iter()
            .position(|queued| queued.frame_placement.frame() == frame)
            .and_then(|index| pending.remove(index));
        pending.push_back(QueuedPresentation {
            state,
            skipped_predecessor: old.is_some(),
        });
        Ok(old.map(|queued| SupersededPresentation(queued.state)))
    }
}

impl FrameReceiver {
    pub fn try_recv(&self) -> Result<QueuedPresentation, TryRecvError> {
        let mut pending = self.pending.lock().unwrap();
        if let Some(state) = pending.pop_front() {
            // More than one receiving handle may wait on the same mailbox.
            // Keep a notification available while any work remains. The weak
            // sender does not keep a disconnected producer artificially alive.
            if !pending.is_empty()
                && let Some(wake) = self.wake.upgrade()
            {
                let _ = wake.try_send(());
            }
            return Ok(state);
        }
        if self.wake.strong_count() == 0 {
            Err(TryRecvError::Disconnected)
        } else {
            Err(TryRecvError::Empty)
        }
    }

    pub fn recv(&self) -> Result<QueuedPresentation, crossbeam_channel::RecvError> {
        loop {
            match self.try_recv() {
                Ok(state) => return Ok(state),
                Err(TryRecvError::Disconnected) => return Err(crossbeam_channel::RecvError),
                Err(TryRecvError::Empty) => {}
            }
            // Recheck the queue even on disconnect: a producer can submit its
            // last frame and disconnect after the preceding empty check.
            let _ = self.available.recv();
        }
    }

    pub fn try_iter(&self) -> impl Iterator<Item = QueuedPresentation> + '_ {
        std::iter::from_fn(|| self.try_recv().ok())
    }

    /// One finite batch for the synchronous fallback. Submissions arriving
    /// while it materializes this batch wait for the next native-loop wake.
    pub(crate) fn drain_pending(&self) -> VecDeque<QueuedPresentation> {
        std::mem::take(&mut *self.pending.lock().unwrap())
    }

    pub(crate) fn available(&self) -> &Receiver<()> {
        &self.available
    }
}

impl SupersededPresentation {
    /// Retire a replaced revision on the evaluator thread.
    pub fn discard(self, evaluator: &mut neovm_core::emacs_core::eval::Context) -> bool {
        let presentation = neovm_core::window::geometry::PresentationId::try_new(self.presentation().get())
            .expect("sealed presentations have nonzero revisions");
        evaluator.retire_interaction_presentation(presentation.get());
        evaluator.frame_manager_mut().get_mut(neovm_core::window::FrameId(self.frame_placement.frame().get()))
            .is_some_and(|frame| frame.discard_display_presentation(presentation))
    }
}
