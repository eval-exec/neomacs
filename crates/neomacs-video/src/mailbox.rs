use crate::backend::DecodedFrame;

pub(crate) struct PendingFrame<F> {
    pub(crate) frame: DecodedFrame<F>,
}

/// Bounded latest-frame mailbox. Publishing never accumulates decoder surfaces.
pub(crate) struct LatestFrameMailbox<F> {
    pending: Option<PendingFrame<F>>,
}

impl<F> Default for LatestFrameMailbox<F> {
    fn default() -> Self {
        Self { pending: None }
    }
}

impl<F> LatestFrameMailbox<F> {
    pub(crate) fn publish(&mut self, frame: PendingFrame<F>) -> Option<PendingFrame<F>> {
        self.pending.replace(frame)
    }

    pub(crate) fn take(&mut self) -> Option<PendingFrame<F>> {
        self.pending.take()
    }

    pub(crate) fn timing(&self) -> Option<crate::FrameTiming> {
        self.pending.as_ref().map(|pending| pending.frame.timing)
    }
}
