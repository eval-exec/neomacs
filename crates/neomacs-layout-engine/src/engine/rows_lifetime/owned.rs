//! Private owned original Rows captures. No public geometry proof or lease API.
//! Construction belongs only to the actual Terminal producer completion.
use super::saved_window_point;
use neovm_core::window::DisplayFrameExt;
use neovm_core::window::DisplayFrameManagerExt;
use neovm_core::window::display::WindowDisplayContextExt;
use neovm_core::{
    buffer::{BufferId, BufferSourceAuthority, LispCharPos1},
    emacs_core::{Context, eval::RowsObservationEpoch},
    tagged::collection_reads::CollectionReads,
    window::{
        FrameId, WindowDisplaySnapshot, WindowDisplaySnapshotFreshness, WindowId,
        WindowLayoutAttemptFreshness, WindowLayoutQuery, WindowLayoutQueryScope,
    },
};

// Visible only to the enclosing rows_lifetime implementation, not engine/cache
// clients. The actual completion moves these original facts into the bundle.
pub(super) struct OriginalRowsFacts {
    pub(super) epoch: RowsObservationEpoch,
    pub(super) source_authority: BufferSourceAuthority,
    pub(super) frame: FrameId,
    pub(super) window: WindowId,
    pub(super) source: BufferId,
    pub(super) current_buffer: Option<BufferId>,
    pub(super) scope: WindowLayoutQueryScope,
    pub(super) source_point: LispCharPos1,
    pub(super) window_point: LispCharPos1,
    pub(super) attempt: WindowLayoutAttemptFreshness,
}

struct OriginalRowsCapture {
    facts: OriginalRowsFacts,
    query: WindowLayoutQuery,
    reads: CollectionReads,
}

pub(in crate::engine) struct PublicationCandidate(OriginalRowsCapture);
pub(in crate::engine) struct StoredRows(OriginalRowsCapture);
pub(in crate::engine) struct CertifiedRowsHit(WindowLayoutQuery);

/// Same-call projections borrow the actual live Context and cannot own roots.
/// They must stay inside one closed lookup, with no interposed trace/callback.
pub(in crate::engine) struct CurrentInputs<'context> {
    context: &'context Context,
    frame: FrameId,
    window: WindowId,
    source: BufferId,
    current_buffer: Option<BufferId>,
    source_point: LispCharPos1,
    window_point: LispCharPos1,
    attempt: WindowLayoutAttemptFreshness,
    snapshot: WindowDisplaySnapshotFreshness,
}

static_assertions::assert_not_impl_any!(PublicationCandidate: Send, Sync, Clone);
static_assertions::assert_not_impl_any!(StoredRows: Send, Sync, Clone);
static_assertions::assert_not_impl_any!(CurrentInputs<'static>: Send, Sync);

impl PublicationCandidate {
    /// Sole caller: enclosing actual Terminal producer completion, after ALL
    /// canonical seals, original domain/read checks and explicit lease drop.
    /// query/reads are moved from that same lexical canonical capture. Reject
    /// per-entry size limits before calling or cloning the canonical return.
    /// There is no detached proof or unpack/rebind API outside this module.
    pub(super) fn from_finished(
        facts: OriginalRowsFacts,
        query: WindowLayoutQuery,
        reads: CollectionReads,
    ) -> Self {
        Self(OriginalRowsCapture {
            facts,
            query,
            reads,
        })
    }

    /// Run only after every completion trace and optional test sink. Retain
    /// the same epoch, query freshness and immutable read revision on success.
    pub(in crate::engine) fn publish_if_current(self, context: &Context) -> Option<StoredRows> {
        let current = self.0.current_inputs(context)?;
        if !self.0.facts_current(&current) {
            return None;
        }
        let source = context.buffer_manager().get(self.0.facts.source)?;
        {
            // SAFETY: current_inputs checked the ORIGINAL physical owners,
            // Context/GEN1/driver/idle/GC/face/cache/host before any saved key.
            // Native exact facts/full attempt checks precede this phase. No
            // Lisp, callback, tracing, reentry, collection/safepoint, service,
            // source mutation or saved-Value formatting occurs while leased.
            let lease = unsafe { self.0.facts.source_authority.validate(source) }?;
            if !self.0.reads.unchanged_and_observe() {
                return None;
            }
            let snapshot = self.0.query.geometry()?;
            if !snapshot.chrome_strings.is_empty()
                || !snapshot
                    .layout_freshness
                    .as_ref()?
                    .same_query_row_content(&current.snapshot)
            {
                return None;
            }
            drop(lease);
        }
        Some(StoredRows(self.0))
    }
}

impl OriginalRowsCapture {
    fn owners_current(&self, context: &Context) -> bool {
        context
            .static_rows_observation_current(&self.facts.epoch)
            .is_ok()
    }

    fn current_inputs<'context>(
        &self,
        context: &'context Context,
    ) -> Option<CurrentInputs<'context>> {
        // The original epoch always precedes saved-key projection and source
        // leasing; there is no begin/rebase or fresh allocation-domain capture.
        if !self.owners_current(context) {
            return None;
        }
        let frame = context.frame_manager().get(self.facts.frame)?;
        let window = frame.find_window(self.facts.window)?;
        let source = window.buffer_id()?;
        let buffer = context.buffer_manager().get(source)?;
        Some(CurrentInputs {
            context,
            frame: self.facts.frame,
            window: self.facts.window,
            source,
            current_buffer: context.buffer_manager().current_buffer_id(),
            source_point: buffer.point_lisp_char_pos(),
            window_point: saved_window_point(window)?,
            attempt: context.window_layout_attempt_freshness(
                self.facts.frame,
                self.facts.window,
                source,
            )?,
            snapshot: context.window_display_snapshot_freshness(
                self.facts.frame,
                self.facts.window,
                source,
            )?,
        })
    }

    fn facts_current(&self, current: &CurrentInputs<'_>) -> bool {
        self.facts.frame == current.frame
            && self.facts.window == current.window
            && self.facts.source == current.source
            && self.facts.current_buffer == current.current_buffer
            && self.facts.source_point == current.source_point
            && self.facts.window_point == current.window_point
            && self.facts.attempt == current.attempt
    }
}

impl StoredRows {
    // Metadata/geometry accessors cannot export or rebind proof/dependencies.
    pub(in crate::engine) fn frame(&self) -> FrameId {
        self.0.facts.frame
    }
    pub(in crate::engine) fn window(&self) -> WindowId {
        self.0.facts.window
    }
    pub(in crate::engine) fn scope(&self) -> WindowLayoutQueryScope {
        self.0.facts.scope
    }
    pub(in crate::engine) fn source_point(&self) -> LispCharPos1 {
        self.0.facts.source_point
    }
    pub(in crate::engine) fn geometry(&self) -> Option<&WindowDisplaySnapshot> {
        self.0.query.geometry()
    }

    /// Capture once after scalar prefiltering in this inert lookup. The
    /// borrow ties all projections to the Context used by later try_hit.
    pub(in crate::engine) fn current_inputs<'context>(
        &self,
        context: &'context Context,
    ) -> Option<CurrentInputs<'context>> {
        self.0.current_inputs(context)
    }

    /// Caller selects Terminal+ExactProven Rows and requires absent external
    /// font metrics. Never call a legacy/prepared path after this refuses.
    pub(in crate::engine) fn try_hit(
        &self,
        frame: FrameId,
        window: WindowId,
        scope: WindowLayoutQueryScope,
        current: &CurrentInputs<'_>,
    ) -> Option<CertifiedRowsHit> {
        if !matches!(scope, WindowLayoutQueryScope::Rows { .. })
            || self.0.facts.frame != frame
            || self.0.facts.window != window
            || self.0.facts.scope != scope
            || !self.0.owners_current(current.context)
            || !self.0.facts_current(current)
        {
            return None;
        }
        let source = current.context.buffer_manager().get(self.0.facts.source)?;
        let answer = {
            // SAFETY: this is the final closed native phase. ORIGINAL epoch
            // currentness precedes saved keys; exact logical source/window,
            // both points/current-buffer/full attempt match. No callback,
            // Lisp, reentry, GC/safepoint, trace, service, source mutation or
            // saved-Value formatting occurs before the explicit lease drop.
            let lease = unsafe { self.0.facts.source_authority.validate(source) }?;
            if !self.0.reads.unchanged_and_observe() {
                return None;
            }
            let answer =
                super::super::query_cache::absolute_rows(&self.0.query, &current.snapshot)?;
            drop(lease);
            answer
        };
        Some(CertifiedRowsHit(answer))
    }
}

impl CertifiedRowsHit {
    /// The sole engine success arm immediately returns this native move.
    /// Emitted main-thread calls are success evidence, not candidate traces.
    /// Keep its complete call cost in the qualified production binary.
    #[inline(never)]
    pub(in crate::engine) fn into_query(self) -> WindowLayoutQuery {
        self.0
    }
}
