//! Evaluator-thread redisplay ownership and reentrant display queries.
//!
//! Renderer presentation and GNU's stack-local display queries are different
//! ownership domains. This module owns both engines and routes nested queries
//! away from an in-progress presentation without weakening either engine's
//! exclusive mutable state.

mod maintenance;

use neovm_core::window::DisplayFrameExt;
use neovm_core::window::DisplayFrameManagerExt;
use std::cell::{Cell, RefCell};

use neomacs_display_protocol::glyph_matrix::FrameDisplayState;
use neomacs_display_protocol::{GraphicalDisplayIdentity, SealedFramePresentation};
use neomacs_layout_engine::LayoutEngine;
use neomacs_layout_engine::engine::{
    FrameLayoutAttempt, WindowLayoutQueryEngine, WindowLayoutQuerySeed,
};
use neomacs_layout_engine::font::metrics::FontMetricsService;
use neomacs_layout_engine::font::sizing::FontSizing;
use neovm_core::emacs_core::eval::Context;
use neovm_core::emacs_core::{Value, error::EvalResult};
use neovm_core::window::geometry::{PresentationActivateError, PresentationId};
use neovm_core::window::{FrameId, WindowId, WindowLayoutQueryOutcome};

/// A renderer-facing reason for producing a frame presentation.
///
/// Logical window queries deliberately use [`RedisplayRuntime::query_window`]
/// instead, so their output cannot be mistaken for a renderer presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameLayoutPurpose {
    Redisplay,
    Snapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreparedPresentationTicket {
    frame_id: FrameId,
    presentation: PresentationId,
}

impl crate::thread_comm::SupersededPresentation {
    /// Retire a revision which the mailbox proves was never acquired by the
    /// renderer. Run on the evaluator thread, without an input-channel roundtrip.
    pub fn discard(self, evaluator: &mut Context) -> bool {
        PreparedPresentationTicket {
            frame_id: FrameId(self.frame_placement.frame().get()),
            presentation: PresentationId::try_new(self.presentation().get())
                .expect("sealed presentations have nonzero revisions"),
        }
        .discard(evaluator)
    }
}

impl PreparedPresentationTicket {
    pub fn activate(
        self,
        evaluator: &mut Context,
    ) -> Result<Option<PresentationId>, PresentationActivateError> {
        evaluator
            .frame_manager_mut()
            .get_mut(self.frame_id)
            .ok_or(PresentationActivateError::UnknownPresentation(
                self.presentation,
            ))?
            .activate_display_presentation(self.presentation)
    }

    pub fn discard(self, evaluator: &mut Context) -> bool {
        evaluator.retire_interaction_presentation(self.presentation.get());
        evaluator
            .frame_manager_mut()
            .get_mut(self.frame_id)
            .is_some_and(|frame| frame.discard_display_presentation(self.presentation))
    }
}

#[must_use = "a prepared display must be submitted, activated, or discarded"]
pub struct PreparedFrameDisplay {
    ticket: PreparedPresentationTicket,
    state: SealedFramePresentation,
}

impl PreparedFrameDisplay {
    pub fn into_submission(self) -> (PreparedPresentationTicket, SealedFramePresentation) {
        (self.ticket, self.state)
    }

    pub fn activate(
        self,
        evaluator: &mut Context,
    ) -> Result<SealedFramePresentation, PresentationActivateError> {
        self.ticket.activate(evaluator)?;
        Ok(self.state)
    }

    pub fn discard(self, evaluator: &mut Context) -> FrameDisplayState {
        self.ticket.discard(evaluator);
        self.state.into_state()
    }
}

impl std::ops::Deref for PreparedFrameDisplay {
    type Target = FrameDisplayState;

    fn deref(&self) -> &Self::Target {
        self.state.state()
    }
}

/// Deep evaluator-thread owner for presentation and query layout state.
///
/// Leaf-local GNU callbacks execute while the presentation engine is borrowed.
/// A synchronous display query from such a callback uses the disjoint,
/// renderer-inert query engine below, so it never recursively borrows retained
/// presentation state.
pub struct RedisplayRuntime {
    engine: RefCell<LayoutEngine>,
    /// Reentrant, renderer-inert row producer used only while the presentation
    /// engine is inside a GNU redisplay callback. Keeping this ownership
    /// disjoint is what makes display queries ordinary nested operations
    /// instead of recursive borrows of retained presentation state.
    reentrant_query_engine: RefCell<Option<WindowLayoutQueryEngine>>,
    /// Font metrics for the installed font-shaping driver (issue #447): the
    /// driver is invoked DURING the layout walk (auto-composition inside a
    /// redisplay), when the engine's own font metrics are mutably borrowed —
    /// this disjoint instance shapes without touching that borrow.
    shaping_metrics: RefCell<FontMetricsService>,
    query_seed: RefCell<WindowLayoutQuerySeed>,
    cosmic_metrics_enabled: Cell<bool>,
    font_sizing: Cell<FontSizing>,
    /// One frontend connection shared by presentation, reentrant queries and
    /// shaping. The runtime is already boxed behind the existing TLS handle.
    display_identity: RefCell<Option<GraphicalDisplayIdentity>>,
}

impl RedisplayRuntime {
    pub fn resolved_scroll_preview(
        &self,
        evaluator: &Context,
        frame: FrameId,
        window: WindowId,
        inputs: Vec<neomacs_display_protocol::input_progress::InputReceipt>,
    ) -> Option<neomacs_display_protocol::scroll_coverage::ResolvedScrollIntent> {
        self.engine
            .try_borrow()
            .ok()?
            .resolved_scroll_preview(evaluator, frame, window, inputs)
    }

    pub fn maintain_scroll_coverage(
        &self,
        evaluator: &Context,
    ) -> (Option<std::time::Duration>, bool) {
        let Ok(mut engine) = self.engine.try_borrow_mut() else {
            return (None, false);
        };
        let started = std::time::Instant::now();
        let (progress, publish) = maintenance::run_budgeted(
            || {
                let progress = engine.maintain_scroll_coverage(evaluator);
                (progress, engine.take_scroll_coverage_publication())
            },
            || started.elapsed(),
        );
        (progress.map(|progress| progress.next_wake()), publish)
    }

    /// Construct a runtime without the expensive scalable-font database.
    /// GUI startup enables it explicitly; TTY keeps cell metrics.
    pub fn new_without_font_metrics() -> Self {
        let engine = LayoutEngine::new_without_font_metrics();
        let query_seed = engine.window_layout_query_seed();
        Self {
            engine: RefCell::new(engine),
            reentrant_query_engine: RefCell::new(None),
            query_seed: RefCell::new(query_seed),
            shaping_metrics: RefCell::new(FontMetricsService::new()),
            cosmic_metrics_enabled: Cell::new(false),
            font_sizing: Cell::new(FontSizing::native_gui()),
            display_identity: RefCell::new(None),
        }
    }

    /// Bind every evaluator-thread font opener to this native connection.
    /// Standalone and detached measurement services remain unbound.
    pub fn set_display_identity(&self, identity: GraphicalDisplayIdentity) {
        self.engine
            .borrow_mut()
            .set_display_identity(identity.clone());
        self.shaping_metrics
            .borrow_mut()
            .set_display_identity(identity.clone());
        if let Some(engine) = self.reentrant_query_engine.borrow_mut().as_mut() {
            engine.set_display_identity(identity.clone());
        }
        *self.display_identity.borrow_mut() = Some(identity);
    }

    pub fn enable_cosmic_metrics(&self) {
        self.engine.borrow_mut().enable_cosmic_metrics();
        self.cosmic_metrics_enabled.set(true);
        // Construct the disjoint query engine at GUI initialization, not from
        // inside the first scroll hook that calls `window-end`. Its independent
        // font service currently performs an eager system-font scan; paying
        // that bounded startup cost here avoids a user-visible callback stall.
        let mut query_engine = self.reentrant_query_engine.borrow_mut();
        let engine =
            query_engine.get_or_insert_with(WindowLayoutQueryEngine::new_without_font_metrics);
        if let Some(identity) = self.display_identity.borrow().as_ref() {
            engine.set_display_identity(identity.clone());
        }
        engine.enable_cosmic_metrics();
        engine.set_font_sizing(self.font_sizing.get());
        engine.synchronize(self.query_seed.borrow().clone());
    }

    pub fn disable_cosmic_metrics(&self) {
        self.engine.borrow_mut().disable_cosmic_metrics();
        self.cosmic_metrics_enabled.set(false);
        if let Some(engine) = self.reentrant_query_engine.borrow_mut().as_mut() {
            engine.disable_cosmic_metrics();
        }
    }

    pub fn set_font_sizing(&self, font_sizing: FontSizing) {
        self.engine.borrow_mut().set_font_sizing(font_sizing);
        self.font_sizing.set(font_sizing);
        if let Some(engine) = self.reentrant_query_engine.borrow_mut().as_mut() {
            engine.set_font_sizing(font_sizing);
        }
    }

    /// The presentation engine's layout statistics for its last accepted
    /// frame (relaid and reused rows, fast-path classes).
    pub fn last_layout_stats(&self) -> neomacs_layout_engine::incremental_layout::LayoutStats {
        self.engine.borrow().last_layout_stats().clone()
    }

    /// Issue #447: the installed font-shaping driver (GNU
    /// `font->driver->shape`). Shapes the gstring through the layout
    /// engine's font system — the same cosmic machinery the row walk uses —
    /// filling the gstring's glyph slots with the font's shaped glyphs
    /// (ligature glyphs included).
    pub fn shape_gstring(
        &self,
        mut gstring: neovm_core::Value,
        direction: neovm_core::Value,
    ) -> neovm_core::emacs_core::font::GstringShapeOutcome {
        // The driver fires DURING the layout walk (auto-composition inside a
        // redisplay), when the engine and its font metrics are already
        // mutably borrowed — shape through this disjoint instance instead.
        neomacs_layout_engine::font::metrics::FontMetricsService::shape_gstring_through_font(
            &mut self.shaping_metrics.borrow_mut(),
            &mut gstring,
            direction,
        )
    }

    /// The installed `font->driver->text_extents` seam (GNU
    /// `font_fill_lglyph_metrics`): encoded code and unshaped extents for
    /// one character of an opened font object.  Uses the same disjoint
    /// shaping-metrics instance the shaping driver uses, so a call from a
    /// gstring constructor during a layout/redisplay borrow cannot conflict
    /// with the engine.
    pub fn glyph_text_metrics(
        &self,
        font: neovm_core::Value,
        ch: u32,
    ) -> Option<neovm_core::display_vocab::GlyphTextMetrics> {
        let (family, pixel_size) =
            neovm_core::emacs_core::font::font_object_family_and_pixel_size(&font)?;
        let ch = char::from_u32(ch)?;
        let selection = neomacs_layout_engine::font::metrics::RealizedFaceFontSelection::new(
            &family,
            400,
            false,
            pixel_size as f32,
        );
        let metrics = self
            .shaping_metrics
            .borrow_mut()
            .gstring_glyph_metrics_for_realized_face_char(ch, selection)?;
        Some(neovm_core::display_vocab::GlyphTextMetrics {
            code: metrics.code,
            width: metrics.extents.advance_px as i64,
            lbearing: metrics.extents.lbearing_px as i64,
            rbearing: metrics.extents.rbearing_px as i64,
            ascent: metrics.extents.ascent_px as i64,
            descent: metrics.extents.descent_px as i64,
        })
    }

    /// Produce one logical frame through the presentation engine.
    pub fn prepare_frame(
        &self,
        evaluator: &mut Context,
        frame_id: FrameId,
        purpose: FrameLayoutPurpose,
    ) -> Option<PreparedFrameDisplay> {
        let (attempt, query_seed) = {
            let mut engine = self.engine.borrow_mut();
            let attempt = match purpose {
                FrameLayoutPurpose::Redisplay => {
                    engine.redisplay_frame_attempt(evaluator, frame_id)
                }
                FrameLayoutPurpose::Snapshot => engine.snapshot_frame_attempt(evaluator, frame_id),
            };
            (attempt, engine.window_layout_query_seed())
        };
        *self.query_seed.borrow_mut() = query_seed.clone();
        if let Some(engine) = self.reentrant_query_engine.borrow_mut().as_mut() {
            engine.synchronize(query_seed);
        }

        match attempt {
            FrameLayoutAttempt::Prepared(state) => Some(PreparedFrameDisplay {
                ticket: PreparedPresentationTicket {
                    frame_id,
                    presentation: PresentationId::new(state.presentation_id.get()),
                },
                state,
            }),
            FrameLayoutAttempt::Aborted => None,
        }
    }

    /// Answer a nested preparation using the geometry already committed in
    /// Context. The caller has stored the echo text; an active producer must
    /// keep its window bounds and starts stable until its walk finishes.
    ///
    /// Nil is the ordinary unchanged-height result. The next complete
    /// redisplay still requests mini geometry and runs the full producer.
    /// A whole-pass lease can use this answer without borrowing its engine.
    pub fn retain_committed_minibuffer_geometry() -> EvalResult {
        Ok(Value::NIL)
    }

    /// Prepare only mini/echo geometry before the owned hook pass. An idle
    /// engine runs the full producer; a nested request keeps committed
    /// geometry. Neither answer publishes a paint ticket. Producer Flow still
    /// belongs to the enclosing transaction.
    pub fn prepare_minibuffer_geometry(
        &self,
        evaluator: &mut Context,
        request: neovm_core::emacs_core::eval::RedisplayMiniGeometryRequest,
    ) -> Result<neovm_core::emacs_core::Value, neovm_core::emacs_core::error::Flow> {
        let Ok(mut engine) = self.engine.try_borrow_mut() else {
            // A fontification callback in a synchronous window-end query can
            // rebind inhibit-redisplay and call message. GNU accepts a
            // same-height message here; a Rust borrow conflict is not a Lisp
            // error and cannot authorize a second preparatory row walk.
            // A taller message remains a known divergence: GNU grows the mini
            // window synchronously, echo_area_display runs redisplay_internal,
            // and dsafe__call throws to top-level so window-end never returns.
            // This committed answer returns without growing. Follow-up
            // N-mini-taller-message pairs a two-line message with
            // resize-mini-windows=t, records mini height inside the callback,
            // and distinguishes top-level exit from normal query completion.
            return Self::retain_committed_minibuffer_geometry();
        };
        engine.prepare_minibuffer_geometry(evaluator, request)
    }

    /// Run the canonical row producer for one synchronous logical query.
    pub fn query_window(
        &self,
        evaluator: &mut Context,
        frame_id: FrameId,
        window_id: WindowId,
        scope: neovm_core::window::WindowLayoutQueryScope,
    ) -> WindowLayoutQueryOutcome {
        self.query_window_with_policy(
            evaluator,
            frame_id,
            window_id,
            scope,
            &neomacs_layout_engine::RowsQueryPolicy::disabled(),
        )
    }

    /// Immutable admission selection belongs to this query's boxed adapter.
    /// The disjoint reentrant engine retains its explicit disabled wrapper.
    pub fn query_window_with_policy(
        &self,
        evaluator: &mut Context,
        frame_id: FrameId,
        window_id: WindowId,
        scope: neovm_core::window::WindowLayoutQueryScope,
        policy: &neomacs_layout_engine::RowsQueryPolicy,
    ) -> WindowLayoutQueryOutcome {
        // The common path reuses the warm presentation engine while it is
        // idle. A nested callback takes the explicitly disjoint path below.
        if let Ok(mut engine) = self.engine.try_borrow_mut() {
            return match engine
                .query_window_layout_with_policy(evaluator, frame_id, window_id, scope, policy)
            {
                Ok(query) => WindowLayoutQueryOutcome::Ready(query),
                Err(failure) => WindowLayoutQueryOutcome::Failed(failure),
            };
        }

        // GNU's `Fwindow_end` owns a stack-local display iterator. Model that
        // ownership directly when a redisplay callback asks the question:
        // this engine has no renderer presentation state and therefore cannot
        // alias the already-borrowed frame transaction. It is lazy so normal
        // non-reentrant queries keep using the warm presentation engine.
        let Ok(mut query_engine) = self.reentrant_query_engine.try_borrow_mut() else {
            return WindowLayoutQueryOutcome::LayoutBusy;
        };
        let query_engine = query_engine.get_or_insert_with(|| {
            let mut engine = if self.cosmic_metrics_enabled.get() {
                WindowLayoutQueryEngine::new()
            } else {
                WindowLayoutQueryEngine::new_without_font_metrics()
            };
            if let Some(identity) = self.display_identity.borrow().as_ref() {
                engine.set_display_identity(identity.clone());
            }
            engine.set_font_sizing(self.font_sizing.get());
            engine.synchronize(self.query_seed.borrow().clone());
            engine
        });
        match query_engine.query_window_layout(evaluator, frame_id, window_id, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(failure) => WindowLayoutQueryOutcome::Failed(failure),
        }
    }
}
