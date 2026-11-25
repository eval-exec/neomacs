//! Native auxiliary scroll-coverage publication over an accepted, active base.
//!
//! Background row acquisition never creates evaluator redisplay debt. Once a
//! worker prefix or dependency invalidation is ready, this module composes the
//! existing base-identity, hook-free redisplay-readiness, exact per-window
//! original freshness (#832), and page/resource proofs before minting an
//! immutable [`AuxiliaryScrollCoverage`] snapshot for the render thread.

use super::*;
use neomacs_display_protocol::scroll_coverage::{
    AuxiliaryScrollCoverage, ScrollCoverage, ScrollSurface,
};
use neomacs_display_protocol::{DisplayFrameId, FrameDisplayState, PresentationId};
use neovm_core::buffer::CharPos0;
use neovm_core::emacs_core::eval::AuxiliaryRedisplayRefusal;
use neovm_core::emacs_core::neo::scroll::ScrollContextExt;
use neovm_core::window::display::WindowDisplayContextExt;
use neovm_core::window::{DisplayFrameExt, DisplayFrameManagerExt, FrameId, WindowId};
use std::sync::Arc;

/// Immutable metadata and resource footprint retained from the latest sealed
/// presentation of one logical frame.
///
/// Ordinary GUI redisplay moves the full [`SealedFramePresentation`] out of
/// `LayoutEngine` on `prepare_frame`. Keeping this compact template (with empty
/// window glyph matrices) lets auxiliary publication prove base identity (R3),
/// verify resource closure (R7), and pre-materialize [`ScrollSurface`] values
/// without mutating or retaining full frame glyph buffers.
pub(super) struct AuxiliaryCoverageBase {
    frame_id: FrameId,
    display_frame: DisplayFrameId,
    presentation_id: PresentationId,
    redisplay_generation: u64,
    /// `None` for non-GUI, snapshot, or exhaustion-snapshot presentations:
    /// those still advance `presentation_id` so R3 refuses stale active bases,
    /// while refusing auxiliary surface minting on themselves.
    template: Option<FrameDisplayState>,
}

impl AuxiliaryCoverageBase {
    pub(super) fn from_sealed(
        frame_id: FrameId,
        sealed: &neomacs_display_protocol::SealedFramePresentation,
        redisplay_generation: u64,
        eligible_template: bool,
    ) -> Self {
        let state = sealed.state();
        let template = eligible_template.then(|| {
            let mut template = FrameDisplayState::new(0, 0, state.char_width, state.char_height);
            template.presentation_id = state.presentation_id;
            template.font_catalog_generation = state.font_catalog_generation;
            template.font_pixel_size = state.font_pixel_size;
            template.faces = state.faces.clone();
            template.fonts = state.fonts.clone();
            template.char_fonts = state.char_fonts.clone();
            template.shaped_clusters = state.shaped_clusters.clone();
            template.scroll_bars = state.scroll_bars.clone();
            template.fringe_bitmaps = state.fringe_bitmaps.clone();
            template.window_matrices = state
                .window_matrices
                .iter()
                .map(|entry| neomacs_display_protocol::WindowMatrixEntry {
                    window_id: entry.window_id,
                    matrix: neomacs_display_protocol::glyph_matrix::GlyphMatrix::new(0, 0),
                    pixel_bounds: entry.pixel_bounds,
                    text_pixel_bounds: entry.text_pixel_bounds,
                    text_clip_bounds: entry.text_clip_bounds,
                    selected: entry.selected,
                })
                .collect();
            template
        });
        Self {
            frame_id,
            display_frame: sealed.frame_placement.frame(),
            presentation_id: sealed.presentation(),
            redisplay_generation,
            template,
        }
    }

    pub(super) fn presentation_id(&self) -> PresentationId {
        self.presentation_id
    }

    #[cfg(test)]
    pub(super) fn template(&self) -> Option<&FrameDisplayState> {
        self.template.as_ref()
    }

    #[cfg(test)]
    pub(super) fn template_mut(&mut self) -> Option<&mut FrameDisplayState> {
        self.template.as_mut()
    }
}

/// Exhaustive reason why native auxiliary publication was refused for the
/// current base presentation (R7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuxiliaryPublicationRefusal {
    NoSelectedFrame,
    FrameNotVisible,
    InitialFrameSkipped,
    NoFontMetrics,
    NoRetainedFrame,
    NoSealedBase,
    IneligibleBaseTemplate,
    BaseNotActive {
        latest_sealed: PresentationId,
        active: Option<PresentationId>,
    },
    NoFaceArena,
    EvaluatorRedisplay(AuxiliaryRedisplayRefusal),
    WindowSetChanged,
    WindowFreshnessChanged(DisplayWindowId),
    ResourceConflict(DisplayWindowId),
    SurfaceMaterializationRejected(DisplayWindowId),
}

/// Identity of a refused publication attempt so idle maintenance never spins
/// on an unchanged refused base (R8). Any base change (new seal, new
/// activation) or new worker/invalidation progress changes this key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RefusedAuxiliaryKey {
    frame: FrameId,
    latest_sealed: Option<PresentationId>,
    active: Option<PresentationId>,
    progress_serial: u64,
}

pub(super) struct StagedAuxiliaryExport {
    pub(super) window: DisplayWindowId,
    pub(super) backward_start: Option<CharPos0>,
    pub(super) published: bool,
}

/// A complete desired auxiliary snapshot staged for native transport delivery.
///
/// Engine-owned publication records (`ScrollPreviewCoverage` and conservative
/// export metadata) advance only when the caller commits after the transport
/// accepts the payload.
#[must_use = "deliver and commit or defer a prepared auxiliary publication"]
pub struct PreparedAuxiliaryScrollCoverage {
    frame_id: FrameId,
    update: AuxiliaryScrollCoverage,
    staged_exports: Option<Vec<StagedAuxiliaryExport>>,
    refusal_after_withdrawal: Option<(AuxiliaryPublicationRefusal, RefusedAuxiliaryKey)>,
}

impl PreparedAuxiliaryScrollCoverage {
    pub fn update(&self) -> &AuxiliaryScrollCoverage {
        &self.update
    }

    pub fn clone_update(&self) -> AuxiliaryScrollCoverage {
        self.update.clone()
    }

    /// True when this payload carries no surfaces and exists only to revoke
    /// previously published coverage unconditionally (R5).
    pub fn is_withdrawal_only(&self) -> bool {
        self.update.surfaces.is_empty()
    }

    /// When a refusal (such as an #832 own-chrome-stale base) triggered an
    /// unconditional withdrawal, report the underlying refusal reason.
    pub fn refusal_reason(&self) -> Option<AuxiliaryPublicationRefusal> {
        self.refusal_after_withdrawal.map(|(refusal, _)| refusal)
    }
}

/// Outcome of one auxiliary publication preparation step.
pub enum AuxiliaryPreparation {
    /// Deliver this complete desired snapshot (install or withdrawal) and then
    /// call [`LayoutEngine::commit_auxiliary_scroll_coverage`].
    Ready(PreparedAuxiliaryScrollCoverage),
    /// No surfaces are eligible and nothing was published to withdraw.
    NoChange,
    /// Eligibility refused auxiliary installation; refusal is recorded against
    /// the current base so repeated idle ticks do not spin (R8).
    Refused(AuxiliaryPublicationRefusal),
}

impl LayoutEngine {
    /// Engine-owned record of the last published coverage for one window (R4),
    /// updated by both ordinary frame sealing and committed auxiliary sends.
    pub fn published_scroll_coverage(
        &self,
        frame: FrameId,
        window: DisplayWindowId,
    ) -> Option<&Arc<ScrollCoverage>> {
        self.scroll_preview_coverage.coverage(frame, window)
    }

    /// Whether a pending worker/invalidation publication can be attempted for
    /// the current base without spinning on a previously refused base (R8).
    pub fn has_actionable_scroll_coverage_publication(
        &self,
        evaluator: &neovm_core::emacs_core::Context,
    ) -> bool {
        if !self.scroll_coverage.publication_pending {
            return false;
        }
        let Some(key) = self.current_refused_auxiliary_key(evaluator) else {
            return false;
        };
        self.scroll_coverage.refused_publication != Some(key)
    }

    fn current_refused_auxiliary_key(
        &self,
        evaluator: &neovm_core::emacs_core::Context,
    ) -> Option<RefusedAuxiliaryKey> {
        let frame = evaluator.frame_manager().selected_frame()?;
        let latest_sealed = self
            .auxiliary_base
            .as_ref()
            .filter(|base| base.frame_id == frame.id)
            .map(AuxiliaryCoverageBase::presentation_id);
        let active = frame
            .active_presentation()
            .map(|id| PresentationId::new(id.get()));
        Some(RefusedAuxiliaryKey {
            frame: frame.id,
            latest_sealed,
            active,
            progress_serial: self.scroll_coverage.publication_serial,
        })
    }

    /// Build a complete desired auxiliary coverage snapshot for the selected
    /// frame if all four R7 proofs hold, or refuse cleanly without mutating
    /// sealed presentations or prepared pages.
    pub fn prepare_auxiliary_scroll_coverage(
        &mut self,
        evaluator: &neovm_core::emacs_core::Context,
    ) -> AuxiliaryPreparation {
        self.retire_removed_frame_owners(evaluator);
        self.prepared_viewports.retire_invalid_dependencies();
        if !self.prepared_viewports.invalidated.is_empty() {
            self.scroll_coverage.note_publication_pending();
            for (_, window) in self.prepared_viewports.invalidated.drain(..) {
                self.scroll_coverage.windows.remove(&window);
            }
        }
        let refused_key = self.current_refused_auxiliary_key(evaluator);
        match self.try_build_auxiliary_update(evaluator) {
            Ok((frame_id, update, staged_exports)) => {
                if update.surfaces.is_empty() && !self.scroll_preview_coverage.has_any_for(frame_id)
                {
                    self.scroll_coverage.clear_publication_pending();
                    return AuxiliaryPreparation::NoChange;
                }
                AuxiliaryPreparation::Ready(PreparedAuxiliaryScrollCoverage {
                    frame_id,
                    update,
                    staged_exports: Some(staged_exports),
                    refusal_after_withdrawal: None,
                })
            }
            Err(refusal) => {
                tracing::debug!(
                    target: "neomacs_layout_engine::scroll_coverage",
                    ?refusal,
                    "refused auxiliary compositor coverage"
                );
                if let Some(withdrawal) =
                    self.maybe_build_refusal_withdrawal(evaluator, refusal, refused_key)
                {
                    return AuxiliaryPreparation::Ready(withdrawal);
                }
                if let Some(key) = refused_key {
                    self.scroll_coverage.refused_publication = Some(key);
                } else {
                    self.scroll_coverage.clear_publication_pending();
                }
                AuxiliaryPreparation::Refused(refusal)
            }
        }
    }

    /// Commit an auxiliary publication after native transport accepted it.
    pub fn commit_auxiliary_scroll_coverage(&mut self, prepared: PreparedAuxiliaryScrollCoverage) {
        if prepared.update.surfaces.is_empty() {
            tracing::debug!(
                target: "neomacs_layout_engine::scroll_coverage",
                base = prepared.update.base.get(),
                refusal = ?prepared.refusal_reason(),
                "withdrew auxiliary compositor coverage"
            );
        } else {
            for surface in &prepared.update.surfaces {
                let coverage = surface.coverage();
                let window = WindowId(coverage.content.window_id.get() as u64);
                tracing::debug!(
                    target: "neomacs_layout_engine::scroll_coverage",
                    predict_pixels = coverage.predict_pixels,
                    compositor_enabled = coverage.compositor_enabled,
                    window = window.0,
                    base = prepared.update.base.get(),
                    epoch = coverage.epoch,
                    publication = coverage.publication.get(),
                    anchor_row = coverage.anchor_row,
                    origin = coverage.origin,
                    source_first = ?coverage.content.matrix.rows.first().map(|row| row.start_charpos),
                    source_last = ?coverage.content.matrix.rows.last().map(|row| row.end_charpos),
                    text_bounds = ?coverage.content.text_clip_bounds,
                    "published auxiliary compositor coverage"
                );
            }
        }
        self.scroll_preview_coverage
            .apply_auxiliary(&prepared.update);
        if let Some(staged) = prepared.staged_exports {
            self.prepared_viewports
                .commit_auxiliary_exports(prepared.frame_id, &staged);
        } else {
            // Refusal withdrawal: revoke certified export state without
            // dropping prepared pages (#832 / design review §5).
            self.prepared_viewports
                .clear_certified_exports_for_frame(prepared.frame_id);
        }
        if let Some((_, refused_key)) = prepared.refusal_after_withdrawal {
            self.scroll_coverage.refused_publication = Some(refused_key);
        } else {
            self.scroll_coverage.clear_publication_pending();
        }
    }

    /// Handle a transport delivery failure without advancing engine records.
    ///
    /// A full command queue keeps `publication_pending` actionable so the next
    /// maintenance wake retries; a disconnected renderer records a refusal so
    /// the evaluator does not spin.
    pub fn defer_auxiliary_scroll_coverage(
        &mut self,
        evaluator: &neovm_core::emacs_core::Context,
        prepared: PreparedAuxiliaryScrollCoverage,
        disconnected: bool,
    ) {
        drop(prepared);
        if disconnected {
            if let Some(key) = self.current_refused_auxiliary_key(evaluator) {
                self.scroll_coverage.refused_publication = Some(key);
            } else {
                self.scroll_coverage.clear_publication_pending();
            }
        }
    }

    fn maybe_build_refusal_withdrawal(
        &self,
        evaluator: &neovm_core::emacs_core::Context,
        refusal: AuxiliaryPublicationRefusal,
        refused_key: Option<RefusedAuxiliaryKey>,
    ) -> Option<PreparedAuxiliaryScrollCoverage> {
        let should_withdraw = matches!(
            refusal,
            AuxiliaryPublicationRefusal::EvaluatorRedisplay(_)
                | AuxiliaryPublicationRefusal::WindowSetChanged
                | AuxiliaryPublicationRefusal::WindowFreshnessChanged(_)
                | AuxiliaryPublicationRefusal::IneligibleBaseTemplate
        );
        if !should_withdraw {
            return None;
        }
        let frame = evaluator.frame_manager().selected_frame()?;
        if !self.scroll_preview_coverage.has_any_for(frame.id) {
            return None;
        }
        let base = self
            .auxiliary_base
            .as_ref()
            .filter(|base| base.frame_id == frame.id)?;
        let refused_key = refused_key?;
        Some(PreparedAuxiliaryScrollCoverage {
            frame_id: frame.id,
            update: AuxiliaryScrollCoverage {
                frame: base.display_frame,
                base: base.presentation_id,
                surfaces: Vec::new(),
            },
            staged_exports: None,
            refusal_after_withdrawal: Some((refusal, refused_key)),
        })
    }

    fn try_build_auxiliary_update(
        &mut self,
        evaluator: &neovm_core::emacs_core::Context,
    ) -> Result<
        (FrameId, AuxiliaryScrollCoverage, Vec<StagedAuxiliaryExport>),
        AuxiliaryPublicationRefusal,
    > {
        use AuxiliaryPublicationRefusal as Refusal;

        // Proof 1: Base identity (R3 + frame visibility/initial guards).
        let frame = evaluator
            .frame_manager()
            .selected_frame()
            .ok_or(Refusal::NoSelectedFrame)?;
        if !frame.visibility.is_visible() {
            return Err(Refusal::FrameNotVisible);
        }
        if !evaluator.input_wait_redisplay_allowed() {
            return Err(Refusal::InitialFrameSkipped);
        }
        if frame.effective_window_system().is_none() || self.font_metrics.is_none() {
            return Err(Refusal::NoFontMetrics);
        }
        let frame_id = frame.id;
        if self.retained_frame != Some(frame_id) {
            return Err(Refusal::NoRetainedFrame);
        }
        let base = self
            .auxiliary_base
            .as_ref()
            .filter(|base| base.frame_id == frame_id)
            .ok_or(Refusal::NoSealedBase)?;
        let active = frame
            .active_presentation()
            .map(|id| PresentationId::new(id.get()));
        if active != Some(base.presentation_id) {
            return Err(Refusal::BaseNotActive {
                latest_sealed: base.presentation_id,
                active,
            });
        }
        let template = base
            .template
            .as_ref()
            .ok_or(Refusal::IneligibleBaseTemplate)?;
        let display_frame = base.display_frame;
        let base_presentation = base.presentation_id;
        let base_generation = base.redisplay_generation;

        // Proof 2: Context unchanged since base (without running hooks).
        evaluator
            .auxiliary_redisplay_readiness(base_generation)
            .map_err(Refusal::EvaluatorRedisplay)?;

        // Proof 3: Exact original body freshness per window (#832) + topology.
        let mut live_windows = frame.window_list();
        live_windows.retain(|window| frame.minibuffer_window != Some(*window));
        for window in &live_windows {
            let owner = DisplayWindowId::new(window.0 as i64);
            if !template
                .window_matrices
                .iter()
                .any(|entry| entry.window_id == owner)
            {
                return Err(Refusal::WindowSetChanged);
            }
        }
        for (owner, retained) in &self.retained_window_matrices {
            let window = WindowId(owner.get() as u64);
            if frame.minibuffer_window == Some(window) {
                continue;
            }
            let Some(live_window) = frame.find_window(window) else {
                return Err(Refusal::WindowSetChanged);
            };
            let Some(buffer_id) = live_window.buffer_id() else {
                return Err(Refusal::WindowSetChanged);
            };
            let Some(original_freshness) = retained.display_snapshot.layout_freshness.as_ref()
            else {
                return Err(Refusal::WindowFreshnessChanged(*owner));
            };
            let Some(current_freshness) =
                evaluator.window_display_snapshot_freshness(frame_id, window, buffer_id)
            else {
                return Err(Refusal::WindowFreshnessChanged(*owner));
            };
            if original_freshness != &current_freshness {
                return Err(Refusal::WindowFreshnessChanged(*owner));
            }
        }

        // Proof 4: Existing page eligibility, self-closed & conflict-free
        // resource compatibility (Aux v2), and evaluator-side surface
        // materialization.
        let arena = self
            .frame_face_arenas
            .get(&frame_id)
            .ok_or(Refusal::NoFaceArena)?;
        let (surfaces, staged): (Vec<Arc<ScrollSurface>>, Vec<StagedAuxiliaryExport>) =
            self.prepared_viewports.export_auxiliary(
                frame_id,
                &self.retained_window_matrices,
                template,
                arena,
                &mut self.font_metrics,
                |owner| {
                    let window = WindowId(owner.get() as u64);
                    (
                        evaluator.compositor_scrolling_enabled(window),
                        evaluator.permits_compositor_pixel_scroll(window),
                    )
                },
            )?;

        Ok((
            frame_id,
            AuxiliaryScrollCoverage {
                frame: display_frame,
                base: base_presentation,
                surfaces,
            },
            staged,
        ))
    }
}
