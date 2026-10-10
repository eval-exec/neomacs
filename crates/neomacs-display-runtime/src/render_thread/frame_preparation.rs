//! Prepare immutable presentations without occupying the native event loop.
//!
//! The active scene stays available while a worker materializes the next one.
//! The result owns glyphs, damage, and continuity metadata from one revision.

use super::frame_compositor::{ReflowImprintsByWindow, ScrollAnchorsByWindow};
use crossbeam_channel::{Receiver, Sender, bounded, select};
use neomacs_display_protocol::{FrameGlyphBuffer, SealedFramePresentation};

pub(super) struct PreparedFrame {
    pub state: SealedFramePresentation,
    pub frame: FrameGlyphBuffer,
    pub damage: neomacs_renderer_wgpu::FrameRowDamage,
    pub scroll: ScrollAnchorsByWindow,
    pub reflow: ReflowImprintsByWindow,
    pub received: neomacs_display_protocol::frame_time::EventTime,
}

impl PreparedFrame {
    /// Damage is relative to the producer's preceding presentation. If that
    /// presentation was skipped, conservatively rebuild every row instead of
    /// treating a delta against an unseen scene as a delta against the active one.
    pub(super) fn invalidate_row_reuse(&mut self) {
        for window in self.damage.windows.values_mut() {
            for row in &mut window.rows {
                row.damage = neomacs_display_protocol::glyph_matrix::RowDamage::New;
            }
        }
    }

    pub(super) fn from_queued(queued: crate::thread_comm::QueuedPresentation) -> Self {
        let mut prepared = Self::new(queued.state);
        if queued.skipped_predecessor {
            prepared.invalidate_row_reuse();
        }
        neomacs_display_protocol::present_trace::record(
            neomacs_display_protocol::present_trace::Stage::Prepared,
            prepared.state.frame_placement.frame().get(),
            prepared.state.presentation(),
        );
        prepared
    }

    pub(super) fn new(state: SealedFramePresentation) -> Self {
        let received = neomacs_display_protocol::frame_time::observe_platform_now();
        let scroll = super::frame_compositor::continuity::scroll::anchors_by_window(&state);
        let reflow = super::frame_compositor::continuity::reflow::imprints_by_window(&state);
        let damage = neomacs_renderer_wgpu::FrameRowDamage::from_display_state(&state);
        let frame = state.materialize();
        Self {
            state,
            frame,
            damage,
            scroll,
            reflow,
            received,
        }
    }
}

pub(super) struct FramePreparation {
    ready: Receiver<PreparedFrame>,
    stop: Sender<()>,
}

impl FramePreparation {
    /// Two materialized results may wait for the native loop. Upstream, the
    /// mailbox retains only the latest pending revision of each logical frame.
    pub(super) fn spawn(
        incoming: crate::thread_comm::FrameReceiver,
        wake: impl Fn() + Send + 'static,
    ) -> std::io::Result<Self> {
        let (completed, ready) = bounded(2);
        let (stop, stopped) = bounded(1);
        std::thread::Builder::new()
            .name("neomacs-frame-prepare".into())
            .spawn(move || {
                loop {
                    if stopped.try_recv().is_ok() {
                        break;
                    }
                    let state = match incoming.try_recv() {
                        Ok(state) => state,
                        Err(crossbeam_channel::TryRecvError::Disconnected) => break,
                        Err(crossbeam_channel::TryRecvError::Empty) => {
                            select! {
                                recv(stopped) -> _ => break,
                                recv(incoming.available()) -> _ => {},
                            }
                            continue;
                        }
                    };
                    let prepared = PreparedFrame::from_queued(state);
                    let frame = prepared.state.frame_placement.frame().get();
                    let presentation = prepared.state.presentation();
                    select! {
                        recv(stopped) -> _ => {
                            neomacs_display_protocol::present_trace::record(
                                neomacs_display_protocol::present_trace::Stage::Discarded,
                                frame,
                                presentation,
                            );
                            break;
                        },
                        send(completed, prepared) -> sent => if sent.is_err() {
                            neomacs_display_protocol::present_trace::record(
                                neomacs_display_protocol::present_trace::Stage::Discarded,
                                frame,
                                presentation,
                            );
                            break;
                        },
                    }
                    wake();
                }
            })?;
        Ok(Self { ready, stop })
    }

    pub(super) fn ready(&self) -> impl Iterator<Item = PreparedFrame> + '_ {
        // A producer replenishing the queue must not keep one native-loop
        // poll running indefinitely. Each subsequent send wakes the loop.
        self.ready.try_iter().take(2)
    }
}

impl Drop for FramePreparation {
    fn drop(&mut self) {
        // Never join from the native event loop. The worker owns only immutable
        // data and exits after its current finite materialization, even if its
        // result queue is full or the evaluator still owns the input sender.
        let _ = self.stop.try_send(());
    }
}
