//! Typed, acknowledgement-safe transport handles owned by a frontend.

use std::fmt::{Display, Formatter};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam_channel::{Receiver, Sender, TryRecvError};
use neomacs_display_protocol::FrameDisplayState;
use neomacs_display_protocol::{FrameGlyphBuffer, SealedFramePresentation};
use neovm_core::emacs_core::process::WaitNotifier;
use neovm_core::keyboard::InputEvent;

use crate::evaluator_input::EvaluatorInputBatch;
use crate::frontend_event::{FrontendEvent, FrontendFrameId, FrontendPresentationId};

/// Result of asking the evaluator's wait boundary to observe queued input.
#[derive(Debug)]
pub enum FrontendWake {
    /// The frontend event produced no evaluator input.
    NotNeeded,
    /// The host wait implementation blocks directly on the input channel.
    ChannelOnly,
    /// The native evaluator poller was notified after input was queued.
    Notified,
    /// Input is queued, but the native poller notification failed.
    Failed(std::io::Error),
}

/// Successful submission of one typed frontend observation.
#[derive(Debug)]
pub struct FrontendInputSubmission {
    queued: usize,
    wake: FrontendWake,
}

impl FrontendInputSubmission {
    /// Number of evaluator input facts produced by this observation.
    #[must_use]
    pub const fn queued(&self) -> usize {
        self.queued
    }

    /// How the evaluator was made aware of the queued input.
    #[must_use]
    pub const fn wake(&self) -> &FrontendWake {
        &self.wake
    }
}

/// The evaluator input receiver disappeared during one submission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrontendInputDisconnected {
    queued: usize,
}

impl FrontendInputDisconnected {
    /// Number of input facts delivered before disconnection was observed.
    #[must_use]
    pub const fn queued(self) -> usize {
        self.queued
    }
}

impl Display for FrontendInputDisconnected {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "editor session disconnected after accepting {} input event(s)",
            self.queued
        )
    }
}

impl std::error::Error for FrontendInputDisconnected {}

/// Cloneable producer used by a platform event loop.
#[derive(Clone)]
pub struct FrontendInputPort {
    input_tx: Sender<InputEvent>,
    wait_notifier: Option<WaitNotifier>,
    quit_requested: Arc<AtomicBool>,
}

impl FrontendInputPort {
    pub(super) fn new(
        input_tx: Sender<InputEvent>,
        wait_notifier: Option<WaitNotifier>,
        quit_requested: Arc<AtomicBool>,
    ) -> Self {
        Self {
            input_tx,
            wait_notifier,
            quit_requested,
        }
    }

    /// Translate, enqueue, and wake for one atomic frontend observation.
    pub fn submit(
        &self,
        event: &FrontendEvent,
    ) -> Result<FrontendInputSubmission, FrontendInputDisconnected> {
        let mut queued = 0;
        for event in EvaluatorInputBatch::from_frontend_event(event) {
            if event.requests_default_quit() {
                self.quit_requested.store(true, Ordering::Relaxed);
            }
            if self.input_tx.send(event).is_err() {
                return Err(FrontendInputDisconnected { queued });
            }
            queued += 1;
        }

        let wake = if queued == 0 {
            FrontendWake::NotNeeded
        } else if let Some(notifier) = &self.wait_notifier {
            match notifier.notify() {
                Ok(()) => FrontendWake::Notified,
                Err(error) => FrontendWake::Failed(error),
            }
        } else {
            FrontendWake::ChannelOnly
        };
        Ok(FrontendInputSubmission { queued, wake })
    }
}

/// Frontend-owned halves of one attached editor session.
pub struct EditorFrontend {
    input: FrontendInputPort,
    frames: FrontendFrameInbox,
}

impl EditorFrontend {
    pub(super) fn new(
        input: FrontendInputPort,
        frames: Receiver<Box<SealedFramePresentation>>,
    ) -> Self {
        Self {
            frames: FrontendFrameInbox {
                frames,
                input: input.clone(),
            },
            input,
        }
    }

    /// Submit host input and renderer feedback to the evaluator.
    #[must_use]
    pub const fn input(&self) -> &FrontendInputPort {
        &self.input
    }

    /// Drain evaluator presentations and retain their acknowledgement guards.
    #[must_use]
    pub const fn frames(&mut self) -> &mut FrontendFrameInbox {
        &mut self.frames
    }

    /// Split the input producer from the frame consumer for adapter ownership.
    #[must_use]
    pub fn split(self) -> (FrontendInputPort, FrontendFrameInbox) {
        (self.input, self.frames)
    }
}

/// Nonblocking result of checking the evaluator's presentation stream.
pub enum FrontendFrameReceive {
    /// No presentation is currently queued.
    Empty,
    /// The evaluator has exited and no presentation remains queued.
    Disconnected,
    /// Latest queued presentation; older queued revisions were discarded.
    Frame(PendingFrontendFrame),
}

/// Frontend-side presentation queue with automatic stale-frame rejection.
pub struct FrontendFrameInbox {
    frames: Receiver<Box<SealedFramePresentation>>,
    input: FrontendInputPort,
}

impl FrontendFrameInbox {
    /// Take the newest queued presentation without blocking.
    ///
    /// Every older queued revision is dropped through
    /// [`PendingFrontendFrame`], which reports it discarded to the evaluator.
    pub fn try_latest(&mut self) -> FrontendFrameReceive {
        let first = match self.frames.try_recv() {
            Ok(frame) => frame,
            Err(TryRecvError::Empty) => return FrontendFrameReceive::Empty,
            Err(TryRecvError::Disconnected) => return FrontendFrameReceive::Disconnected,
        };
        let mut latest = PendingFrontendFrame::new(first, self.input.clone());
        loop {
            match self.frames.try_recv() {
                Ok(frame) => latest = PendingFrontendFrame::new(frame, self.input.clone()),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => {
                    return FrontendFrameReceive::Frame(latest);
                }
            }
        }
    }
}

/// Presentation which must be activated or discarded exactly once.
#[must_use = "a pending frontend frame must be activated or discarded"]
pub struct PendingFrontendFrame {
    frame: Option<Box<SealedFramePresentation>>,
    input: FrontendInputPort,
}

impl PendingFrontendFrame {
    fn new(frame: Box<SealedFramePresentation>, input: FrontendInputPort) -> Self {
        Self {
            frame: Some(frame),
            input,
        }
    }

    /// Evaluator frame targeted by this immutable presentation.
    #[must_use]
    pub fn target(&self) -> FrontendFrameId {
        FrontendFrameId::new(self.frame().frame_placement.frame().get())
    }

    /// Immutable presentation revision.
    #[must_use]
    pub fn presentation(&self) -> FrontendPresentationId {
        FrontendPresentationId::new(self.frame().presentation().get())
    }

    /// Materialize the renderer-facing glyph buffer without acknowledging it.
    #[must_use]
    pub fn materialize(&self) -> FrameGlyphBuffer {
        self.frame().materialize()
    }

    /// Immutable retained display state to encode for a remote renderer.
    #[must_use]
    pub fn state(&self) -> &FrameDisplayState {
        self.frame().state()
    }

    /// Transfer renderer-feedback responsibility to a remote frontend.
    ///
    /// Unlike [`Self::activate`] or [`Self::discard`], this sends no immediate
    /// feedback. The receiver must eventually submit exactly one activated or
    /// discarded observation, and later retire an activated presentation.
    #[must_use]
    pub fn hand_off_to_remote_frontend(mut self) -> FrameDisplayState {
        let frame = self.frame.take().expect("pending frame already consumed");
        (*frame).into_state()
    }

    /// Report successful installation and return its retirement guard.
    pub fn activate(mut self) -> Result<ActiveFrontendPresentation, FrontendInputDisconnected> {
        self.input.submit(&FrontendEvent::PresentationActivated {
            presentation: self.presentation(),
            target: self.target(),
        })?;
        let presentation = self.presentation();
        self.frame.take();
        Ok(ActiveFrontendPresentation {
            presentation: Some(presentation),
            input: self.input.clone(),
        })
    }

    /// Explicitly reject this revision before it becomes active.
    pub fn discard(mut self) -> Result<(), FrontendInputDisconnected> {
        self.send_discard()?;
        self.frame.take();
        Ok(())
    }

    fn frame(&self) -> &SealedFramePresentation {
        self.frame
            .as_deref()
            .expect("pending frame already consumed")
    }

    fn send_discard(&self) -> Result<FrontendInputSubmission, FrontendInputDisconnected> {
        self.input.submit(&FrontendEvent::PresentationDiscarded {
            presentation: self.presentation(),
            target: self.target(),
        })
    }
}

impl Drop for PendingFrontendFrame {
    fn drop(&mut self) {
        if self.frame.is_some() {
            let _ = self.send_discard();
        }
    }
}

/// Guard proving one presentation may still generate renderer input hits.
#[must_use = "retain the active presentation guard while its frame is visible"]
pub struct ActiveFrontendPresentation {
    presentation: Option<FrontendPresentationId>,
    input: FrontendInputPort,
}

impl ActiveFrontendPresentation {
    /// Revision retained by the frontend.
    #[must_use]
    pub fn presentation(&self) -> FrontendPresentationId {
        self.presentation
            .expect("active presentation already retired")
    }

    /// Retire this revision before destroying or replacing its renderer state.
    pub fn retire(mut self) -> Result<(), FrontendInputDisconnected> {
        self.send_retired()?;
        self.presentation.take();
        Ok(())
    }

    fn send_retired(&self) -> Result<FrontendInputSubmission, FrontendInputDisconnected> {
        self.input.submit(&FrontendEvent::PresentationRetired {
            presentation: self.presentation(),
        })
    }
}

impl Drop for ActiveFrontendPresentation {
    fn drop(&mut self) {
        if self.presentation.is_some() {
            let _ = self.send_retired();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend_event::{
        FrontendKeyEvent, FrontendKeyState, FrontendKeySymbol, FrontendModifiers,
    };
    use neomacs_display_protocol::{
        DisplayFrameId, FrameDisplayState, ParentFrameRect, PresentationId,
        PresentedFramePlacement, PresentedHitIndex,
    };

    fn sealed_frame(revision: u64) -> SealedFramePresentation {
        let mut state = FrameDisplayState::new(80, 24, 8.0, 16.0);
        state.presentation_id = PresentationId::new(revision);
        state.frame_placement = PresentedFramePlacement::new(
            DisplayFrameId::new(7),
            state.presentation_id,
            None,
            ParentFrameRect::new(0.0, 0.0, 640.0, 384.0).unwrap(),
            0,
        );
        state.presented_hit_index =
            PresentedHitIndex::from_parts(state.presentation_id, vec![], vec![]).unwrap();
        SealedFramePresentation::seal(state).unwrap()
    }

    #[test]
    fn input_port_expands_one_atomic_text_commit_in_order() {
        let (input_tx, input_rx) = crossbeam_channel::unbounded();
        let port = FrontendInputPort::new(input_tx, None, Arc::new(AtomicBool::new(false)));

        let submission = port
            .submit(&FrontendEvent::text_committed(
                "λ🙂",
                FrontendFrameId::new(7),
            ))
            .expect("connected input port");

        assert_eq!(submission.queued(), 2);
        assert!(matches!(submission.wake(), FrontendWake::ChannelOnly));
        let received = input_rx.try_iter().collect::<Vec<_>>();
        assert!(matches!(
            received.as_slice(),
            [
                InputEvent::KeyPress {
                    emacs_frame_id: 7,
                    ..
                },
                InputEvent::KeyPress {
                    emacs_frame_id: 7,
                    ..
                }
            ]
        ));
    }

    #[test]
    fn ignored_key_release_does_not_wake_the_evaluator() {
        let (input_tx, _input_rx) = crossbeam_channel::unbounded();
        let port = FrontendInputPort::new(input_tx, None, Arc::new(AtomicBool::new(false)));
        let released = FrontendEvent::Key(FrontendKeyEvent::new(
            FrontendKeySymbol::new('x' as u32),
            FrontendModifiers::default(),
            FrontendKeyState::Released,
            FrontendFrameId::new(7),
        ));

        let submission = port.submit(&released).expect("connected input port");

        assert_eq!(submission.queued(), 0);
        assert!(matches!(submission.wake(), FrontendWake::NotNeeded));
    }

    #[test]
    fn latest_frame_guard_discards_superseded_and_retires_active_revisions() {
        let (input_tx, input_rx) = crossbeam_channel::unbounded();
        let input = FrontendInputPort::new(input_tx, None, Arc::new(AtomicBool::new(false)));
        let (frame_tx, frame_rx) = crossbeam_channel::unbounded();
        let mut inbox = FrontendFrameInbox {
            frames: frame_rx,
            input,
        };
        frame_tx.send(Box::new(sealed_frame(40))).unwrap();
        frame_tx.send(Box::new(sealed_frame(41))).unwrap();

        let FrontendFrameReceive::Frame(pending) = inbox.try_latest() else {
            panic!("latest frame should be ready");
        };
        assert_eq!(pending.presentation(), FrontendPresentationId::new(41));
        assert!(matches!(
            input_rx.try_recv().unwrap(),
            InputEvent::PresentationDiscarded {
                presentation: 40,
                emacs_frame_id: 7,
            }
        ));

        let active = pending.activate().expect("session remains connected");
        assert!(matches!(
            input_rx.try_recv().unwrap(),
            InputEvent::PresentationActivated {
                presentation: 41,
                emacs_frame_id: 7,
            }
        ));
        drop(active);
        assert!(matches!(
            input_rx.try_recv().unwrap(),
            InputEvent::PresentationRetired { presentation: 41 }
        ));
    }
    #[test]
    fn remote_handoff_transfers_feedback_responsibility_without_discarding() {
        let (input_tx, input_rx) = crossbeam_channel::unbounded();
        let port = FrontendInputPort::new(input_tx, None, Arc::new(AtomicBool::new(false)));
        let pending = PendingFrontendFrame::new(Box::new(sealed_frame(17)), port);

        assert_eq!(pending.state().presentation_id, PresentationId::new(17));
        let _state = pending.hand_off_to_remote_frontend();

        assert!(input_rx.try_recv().is_err());
    }
}
