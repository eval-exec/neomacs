//! Evaluator-thread redisplay ownership and reentrant display queries.
//!
//! Renderer presentation and GNU's stack-local display queries are different
//! ownership domains. This module owns both engines and routes nested queries
//! away from an in-progress presentation without weakening either engine's
//! exclusive mutable state.

use std::cell::{Cell, RefCell};

use neomacs_display_protocol::SealedFramePresentation;
use neomacs_display_protocol::glyph_matrix::FrameDisplayState;
use neomacs_layout_engine::LayoutEngine;
use neomacs_layout_engine::engine::{
    FrameLayoutAttempt, WindowLayoutQueryEngine, WindowLayoutQuerySeed,
};
use neomacs_layout_engine::font::sizing::FontSizing;
use neovm_core::emacs_core::eval::Context;
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

impl FrameLayoutPurpose {
    const fn consumes_pending_input(self) -> bool {
        matches!(self, Self::Redisplay)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreparedPresentationTicket {
    frame_id: FrameId,
    presentation: PresentationId,
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
    query_seed: RefCell<WindowLayoutQuerySeed>,
    cosmic_metrics_enabled: Cell<bool>,
    font_sizing: Cell<FontSizing>,
}

impl RedisplayRuntime {
    /// Construct a runtime without the expensive scalable-font database.
    /// GUI startup enables it explicitly; TTY keeps cell metrics.
    pub fn new_without_font_metrics() -> Self {
        let engine = LayoutEngine::new_without_font_metrics();
        let query_seed = engine.window_layout_query_seed();
        Self {
            engine: RefCell::new(engine),
            reentrant_query_engine: RefCell::new(None),
            query_seed: RefCell::new(query_seed),
            cosmic_metrics_enabled: Cell::new(false),
            font_sizing: Cell::new(FontSizing::native_gui()),
        }
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

    /// Produce one logical frame through the presentation engine.
    pub fn prepare_frame(
        &self,
        evaluator: &mut Context,
        frame_id: FrameId,
        purpose: FrameLayoutPurpose,
    ) -> Option<PreparedFrameDisplay> {
        let (attempt, query_seed) = {
            let mut engine = self.engine.borrow_mut();
            self.apply_pending_input(&mut engine, evaluator, frame_id, purpose);
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

    /// Run the canonical row producer for one synchronous logical query.
    pub fn query_window(
        &self,
        evaluator: &mut Context,
        frame_id: FrameId,
        window_id: WindowId,
    ) -> WindowLayoutQueryOutcome {
        // The common path reuses the warm presentation engine while it is
        // idle. A nested callback takes the explicitly disjoint path below.
        if let Ok(mut engine) = self.engine.try_borrow_mut() {
            return match engine.query_window_layout(evaluator, frame_id, window_id) {
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
            engine.set_font_sizing(self.font_sizing.get());
            engine.synchronize(self.query_seed.borrow().clone());
            engine
        });
        match query_engine.query_window_layout(evaluator, frame_id, window_id) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(failure) => WindowLayoutQueryOutcome::Failed(failure),
        }
    }

    fn apply_pending_input(
        &self,
        engine: &mut LayoutEngine,
        evaluator: &mut Context,
        frame_id: FrameId,
        purpose: FrameLayoutPurpose,
    ) {
        if !purpose.consumes_pending_input() {
            return;
        }
        let Some(delta) = evaluator.take_pending_pixel_scroll_for_frame(frame_id) else {
            return;
        };
        let Some(window_id) = evaluator
            .frame_manager()
            .get(frame_id)
            .map(|frame| frame.selected_window)
        else {
            return;
        };
        // SIGN: trackpad delta_y vs scroll direction is verified on-screen.
        let delta_px = (-delta).round() as i32;
        let _ = engine.pixel_scroll_window(evaluator, window_id, delta_px);
    }
}
