//! Evaluator-side ownership of layout, display queries, and frame publication.
//!
//! A frontend owns transport and rendering. This module owns the warmer,
//! stateful half that must stay on the evaluator thread: retained layout
//! state, reentrant GNU display queries, presentation tickets, and the rule
//! that a rejected frame is retired immediately.

use std::rc::Rc;

use neomacs_display_protocol::SealedFramePresentation;
use neomacs_display_protocol::glyph_matrix::FrameDisplayState;
use neomacs_layout_engine::font::sizing::FontSizing;
use neovm_core::emacs_core::eval::Context;
use neovm_core::window::{FrameId, RenderFrameScope, RenderFrameVisibility};

mod redisplay;

use redisplay::RedisplayRuntime;
pub use redisplay::{FrameLayoutPurpose, PreparedFrameDisplay, PreparedPresentationTicket};

/// Font-measurement policy owned by one editor presentation session.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PresentationMetrics {
    /// Fixed character cells, used by terminal presentation.
    CellGrid,
    /// Scalable font metrics with the frontend's DPI policy.
    Scalable(FontSizing),
}

/// Result of one attempt to publish all currently visible frames.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FramePublishResult {
    published: usize,
    rejected: usize,
}

impl FramePublishResult {
    /// Number of presentations accepted by the frontend transport.
    #[must_use]
    pub const fn published(self) -> usize {
        self.published
    }

    /// Number of presentations rejected and retired before activation.
    #[must_use]
    pub const fn rejected(self) -> usize {
        self.rejected
    }
}

/// Deep, cloneable handle to evaluator-thread presentation state.
///
/// Clones share one retained layout engine. This lets redisplay, synchronous
/// window queries, and explicit frame snapshots use the same semantic owner
/// without a process-global or thread-local singleton.
#[derive(Clone)]
pub struct EditorPresentationRuntime {
    runtime: Rc<RedisplayRuntime>,
}

impl EditorPresentationRuntime {
    /// Construct presentation state for one editor session.
    #[must_use]
    pub fn new(metrics: PresentationMetrics) -> Self {
        let runtime = RedisplayRuntime::new_without_font_metrics();
        if let PresentationMetrics::Scalable(font_sizing) = metrics {
            runtime.set_font_sizing(font_sizing);
            runtime.enable_cosmic_metrics();
        }
        Self {
            runtime: Rc::new(runtime),
        }
    }

    /// Switch this session to fixed character-cell measurement.
    pub fn use_cell_grid(&self) {
        self.runtime.disable_cosmic_metrics();
    }

    /// Switch this session to scalable fonts with the given DPI policy.
    pub fn use_scalable_metrics(&self, font_sizing: FontSizing) {
        self.runtime.set_font_sizing(font_sizing);
        self.runtime.enable_cosmic_metrics();
    }

    /// Produce one frame through the canonical retained layout engine.
    pub fn prepare_frame(
        &self,
        evaluator: &mut Context,
        frame_id: FrameId,
        purpose: FrameLayoutPurpose,
    ) -> Option<PreparedFrameDisplay> {
        self.runtime.prepare_frame(evaluator, frame_id, purpose)
    }

    /// Selected frame used as the root of an explicit snapshot request.
    #[must_use]
    pub fn current_frame_id(&self, evaluator: &Context) -> Option<FrameId> {
        evaluator
            .frame_manager()
            .selected_frame()
            .map(|frame| frame.id)
    }

    /// Lay out the frames covered by an explicit evaluator snapshot request.
    pub fn collect_snapshot_states(
        &self,
        evaluator: &mut Context,
        target: &neovm_core::emacs_core::xdisp::SnapshotTarget,
    ) -> Result<Vec<FrameDisplayState>, String> {
        use neovm_core::emacs_core::xdisp::SnapshotTarget;

        let selected = self
            .current_frame_id(evaluator)
            .ok_or_else(|| "no selected frame".to_owned())?;
        let tree = evaluator
            .frame_manager()
            .render_frame_forest(
                RenderFrameScope::TreeContaining(selected),
                RenderFrameVisibility::VisibleOnly,
            )
            .into_iter()
            .next()
            .ok_or_else(|| "no render frame tree for the selected frame".to_owned())?;

        let mut states = Vec::new();
        for node in tree.frames_bottom_to_top {
            let keep = match target {
                SnapshotTarget::All => true,
                SnapshotTarget::Selected => node.frame_id == selected,
                SnapshotTarget::Frame(id) => node.frame_id.0 == *id,
            };
            if !keep {
                continue;
            }
            let Some(prepared) =
                self.prepare_frame(evaluator, node.frame_id, FrameLayoutPurpose::Snapshot)
            else {
                continue;
            };
            states.push(prepared.discard(evaluator));
        }

        if states.is_empty()
            && let SnapshotTarget::Frame(id) = target
            && let Some(prepared) =
                self.prepare_frame(evaluator, FrameId(*id), FrameLayoutPurpose::Snapshot)
        {
            states.push(prepared.discard(evaluator));
        }

        if states.is_empty() {
            return Err("frame snapshot: no frame produced display state".to_owned());
        }
        Ok(states)
    }

    /// Install the explicit frame-snapshot adapter on an evaluator.
    pub fn install_frame_snapshot_hook(&self, evaluator: &mut Context) {
        use neovm_core::emacs_core::xdisp::SnapshotFormat;

        let snapshots = self.clone();
        evaluator.frame_snapshot_fn = Some(Box::new(move |eval, request| {
            let states = snapshots.collect_snapshot_states(eval, &request.target)?;
            Ok(match request.format {
                SnapshotFormat::Json => serde_json::to_string(&serde_json::json!({
                    "frames": states,
                }))
                .map_err(|error| format!("frame snapshot JSON serialization failed: {error}"))?,
                SnapshotFormat::Text => states
                    .iter()
                    .map(FrameDisplayState::render_text)
                    .collect::<Vec<_>>()
                    .join("\n"),
                SnapshotFormat::TextFaces => states
                    .iter()
                    .map(FrameDisplayState::render_text_faces)
                    .collect::<Vec<_>>()
                    .join("\n"),
            })
        }));
    }

    /// Install the synchronous window-layout query adapter on an evaluator.
    pub fn install_window_layout_query_hook(&self, evaluator: &mut Context) {
        let queries = self.clone();
        evaluator.install_window_layout_query(move |eval, frame_id, window_id| {
            queries.runtime.query_window(eval, frame_id, window_id)
        });
    }

    /// Install both evaluator observation adapters owned by this runtime.
    pub fn install_evaluator_query_hooks(&self, evaluator: &mut Context) {
        self.install_frame_snapshot_hook(evaluator);
        self.install_window_layout_query_hook(evaluator);
    }

    /// Publish every visible native-window frame through one host transport.
    ///
    /// Returning `false` from `try_publish` means the transport rejected the
    /// presentation. Its evaluator ticket is then retired immediately so
    /// future input cannot activate a frame the frontend never observed.
    pub fn publish_visible_frames(
        &self,
        evaluator: &mut Context,
        mut try_publish: impl FnMut(SealedFramePresentation) -> bool,
    ) -> FramePublishResult {
        let forest = evaluator.frame_manager().render_frame_forest(
            RenderFrameScope::AllNativeWindowTrees,
            RenderFrameVisibility::VisibleOnly,
        );
        let mut result = FramePublishResult::default();
        for node in forest
            .into_iter()
            .flat_map(|tree| tree.frames_bottom_to_top)
        {
            let Some(prepared) =
                self.prepare_frame(evaluator, node.frame_id, FrameLayoutPurpose::Redisplay)
            else {
                continue;
            };
            let (ticket, presentation) = prepared.into_submission();
            if try_publish(presentation) {
                result.published += 1;
            } else {
                ticket.discard(evaluator);
                result.rejected += 1;
            }
        }
        result
    }
}
