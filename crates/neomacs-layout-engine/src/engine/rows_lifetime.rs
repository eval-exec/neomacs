//! Actual GEN1 numeric Rows producer and private original-capture admission.
use super::{LayoutEngine, QueryRowCoverage, rows_face_services::DiagnosticEffects};
use neovm_core::{
    buffer::{BufferId, BufferSourceAuthority, BufferSourceReadLease, LispCharPos1},
    emacs_core::{
        Context,
        eval::{RowsDependencyMetadata, RowsObservationEpoch, RowsObservationRefusal},
        xdisp::RedisplayFontificationWitness,
    },
    tagged::collection_reads::CollectionReads,
    window::{
        FrameId, Window, WindowDisplaySnapshot, WindowId, WindowLayoutAttemptFreshness,
        WindowLayoutQuery, WindowLayoutQueryScope,
    },
};
use std::marker::PhantomData;

#[path = "rows_lifetime/owned.rs"]
mod owned;
use owned::OriginalRowsFacts;
pub(super) use owned::{CertifiedRowsHit, PublicationCandidate, StoredRows};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(super) enum Refusal {
    #[error("not an exact synchronous Rows query")]
    Scope,
    #[error("native font services can enter external code")]
    FontServices,
    #[error("the source window is missing or a minibuffer")]
    Window,
    #[error("the source text authority is missing or changed")]
    Source,
    #[error("the canonical producer did not prove complete no-demand fontification")]
    Fontification,
    #[error("the TTY producer entered cold face realization or lacks its actual witness")]
    ColdFaceEntry,
    #[error("the accepted producer logged face diagnostics or lacks its actual drain witness")]
    DiagnosticEffects,
    #[error("the producer evaluated a dynamic display form or retried")]
    DynamicOrRetry,
    #[error("the completed query lacks complete numeric geometry")]
    Geometry,
    #[error("the source or window point changed")]
    Point,
    #[error("the original complete attempt inputs or current buffer changed")]
    Inputs,
    #[error("collection capture overflowed or was incoherent")]
    Collections,
    #[error(transparent)]
    Core(#[from] RowsObservationRefusal),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Report {
    Certified { rows: usize, points: usize },
    Refused(Refusal),
}

#[derive(Debug)]
enum FaceEntrySeal {
    Unseen,
    WarmOnly,
    Cold,
}

#[derive(Debug)]
enum FontificationSeal {
    Unseen,
    NoDemand,
    Refused,
}

#[derive(Debug)]
struct AdmissionInputs {
    current_buffer: Option<BufferId>,
    attempt: WindowLayoutAttemptFreshness,
}

/// Pending clock/source facts, never a lifetime proof on their own.
#[derive(Debug)]
pub(super) struct Producer {
    frame: FrameId,
    window: WindowId,
    source: BufferId,
    scope: WindowLayoutQueryScope,
    source_point: LispCharPos1,
    window_point: LispCharPos1,
    source_authority: BufferSourceAuthority,
    epoch: RowsObservationEpoch,
    fontification: FontificationSeal,
    face_entry: FaceEntrySeal,
    diagnostic_effects: DiagnosticEffects,
    tty: bool,
    admission: Option<AdmissionInputs>,
    _local: PhantomData<*const ()>,
}

/// The only physical lifetime constructor is finish below, after actual
/// producer seals. Its sole production caller receives exactly the read set
/// from the canonical capture which began AFTER Producer::begin, including
/// normalization. Core's public metadata alone does not prove that ordering.
/// Borrows the exact read set and numeric geometry; never retains Lisp Values.
/// No Debug implementation can format the geometry while its lease is active.
struct Completed<'a> {
    dependencies: RowsDependencyMetadata<'a>,
    geometry: &'a WindowDisplaySnapshot,
    _source: BufferSourceReadLease<'a>,
    origin: CompletedOrigin,
    _local: PhantomData<*const ()>,
}

struct CompletedOrigin {
    frame: FrameId,
    window: WindowId,
    source: BufferId,
    scope: WindowLayoutQueryScope,
    source_point: LispCharPos1,
    window_point: LispCharPos1,
    source_authority: BufferSourceAuthority,
    admission: Option<AdmissionInputs>,
}

pub(super) struct AdmissionCompletion {
    pub(super) canonical_query: WindowLayoutQuery,
    pub(super) report: Report,
    pub(super) candidate: Option<PublicationCandidate>,
}
static_assertions::assert_not_impl_any!(Producer: Send, Sync);
static_assertions::assert_not_impl_any!(Completed<'static>: Send, Sync);

fn saved_window_point(window: &Window) -> Option<LispCharPos1> {
    match window {
        Window::Leaf { point, .. } => Some(*point),
        Window::Internal { .. } => None,
    }
}

impl Producer {
    pub(super) fn begin(
        engine: &LayoutEngine,
        context: &Context,
        frame: FrameId,
        window: WindowId,
        scope: WindowLayoutQueryScope,
    ) -> Result<Self, Refusal> {
        if !matches!(scope, WindowLayoutQueryScope::Rows { .. }) {
            return Err(Refusal::Scope);
        }
        if engine.font_metrics.is_some() {
            return Err(Refusal::FontServices);
        }
        let epoch = context.begin_static_rows_observation(frame)?;
        Self::begin_with_epoch(context, frame, window, scope, epoch)
    }

    /// Explicit TTY metadata policy; the actual render must still supply its
    /// independent warm-only face-entry seal before completion.
    pub(super) fn begin_tty(
        engine: &LayoutEngine,
        context: &Context,
        frame: FrameId,
        window: WindowId,
        scope: WindowLayoutQueryScope,
        host: Option<&neovm_core::emacs_core::display_host::InertRowsHostAuthority>,
    ) -> Result<Self, Refusal> {
        if !matches!(scope, WindowLayoutQueryScope::Rows { .. }) {
            return Err(Refusal::Scope);
        }
        if engine.font_metrics.is_some() {
            return Err(Refusal::FontServices);
        }
        let epoch = context.begin_tty_rows_observation(frame, host)?;
        Self::begin_with_epoch(context, frame, window, scope, epoch)
    }

    fn begin_with_epoch(
        context: &Context,
        frame: FrameId,
        window: WindowId,
        scope: WindowLayoutQueryScope,
        epoch: RowsObservationEpoch,
    ) -> Result<Self, Refusal> {
        let frame_owner = context.frame_manager().get(frame).ok_or(Refusal::Window)?;
        if frame_owner.minibuffer_window == Some(window) {
            return Err(Refusal::Window);
        }
        let window_owner = frame_owner.find_window(window).ok_or(Refusal::Window)?;
        let source = window_owner.buffer_id().ok_or(Refusal::Window)?;
        let buffer = context
            .buffer_manager()
            .get(source)
            .ok_or(Refusal::Source)?;
        let source_authority = buffer
            .capture_motion_source_authority()
            .ok_or(Refusal::Source)?;
        Ok(Self {
            frame,
            window,
            source,
            scope,
            source_point: buffer.point_lisp_char_pos(),
            window_point: saved_window_point(window_owner).ok_or(Refusal::Window)?,
            source_authority,
            epoch,
            fontification: FontificationSeal::Unseen,
            face_entry: FaceEntrySeal::Unseen,
            diagnostic_effects: DiagnosticEffects::Unseen,
            tty: frame_owner.effective_window_system().is_none(),
            admission: None,
            _local: PhantomData,
        })
    }

    /// Select only the real TTY admission route before the WHOLE canonical
    /// capture. OFF observation never captures this additional complete token.
    pub(super) fn for_admission(mut self, context: &Context) -> Result<Self, Refusal> {
        if !self.tty {
            return Err(Refusal::Scope);
        }
        self.admission = Some(AdmissionInputs {
            current_buffer: context.buffer_manager().current_buffer_id(),
            attempt: context
                .window_layout_attempt_freshness(self.frame, self.window, self.source)
                .ok_or(Refusal::Inputs)?,
        });
        Ok(self)
    }

    /// Supplied only by the actual canonical Rows render's sticky witness.
    /// Repeated notes cannot turn an incomplete or cold observation warm.
    pub(super) fn note_face_entry(&mut self, entry: Option<&std::cell::Cell<bool>>) {
        self.face_entry = match (&self.face_entry, entry) {
            (FaceEntrySeal::Unseen, Some(entry)) if !entry.get() => FaceEntrySeal::WarmOnly,
            (FaceEntrySeal::Unseen, Some(_))
            | (FaceEntrySeal::Unseen, None)
            | (FaceEntrySeal::WarmOnly, _)
            | (FaceEntrySeal::Cold, _) => FaceEntrySeal::Cold,
        };
    }

    /// The request-local factory observed the actual accepted-query Vec
    /// before canonical logging. Repeated or missing notes cannot certify it.
    pub(super) fn note_diagnostic_effects(&mut self, effects: DiagnosticEffects) {
        self.diagnostic_effects = match (self.diagnostic_effects, effects) {
            (DiagnosticEffects::Unseen, DiagnosticEffects::Clear) => DiagnosticEffects::Clear,
            (
                DiagnosticEffects::Unseen,
                DiagnosticEffects::Unseen | DiagnosticEffects::Observed,
            )
            | (
                DiagnosticEffects::Clear | DiagnosticEffects::Observed,
                DiagnosticEffects::Unseen | DiagnosticEffects::Clear | DiagnosticEffects::Observed,
            ) => DiagnosticEffects::Observed,
        };
    }

    /// Called only at the real canonical Rows return, using stage1's actual
    /// attempt ledger. No second fontifier/preflight can manufacture this.
    pub(super) fn note_fontification(&mut self, witness: Option<&RedisplayFontificationWitness>) {
        self.fontification = match &self.fontification {
            FontificationSeal::Unseen => match witness {
                Some(RedisplayFontificationWitness::NoDemand(_coverage)) => {
                    FontificationSeal::NoDemand
                }
                Some(RedisplayFontificationWitness::CallbacksRan)
                | Some(RedisplayFontificationWitness::Unknown)
                | Some(RedisplayFontificationWitness::Aborted)
                | None => FontificationSeal::Refused,
            },
            FontificationSeal::NoDemand | FontificationSeal::Refused => FontificationSeal::Refused,
        };
    }

    fn finish<'a>(
        self,
        engine: &LayoutEngine,
        context: &'a Context,
        query: &'a WindowLayoutQuery,
        reads: Option<&'a CollectionReads>,
    ) -> Result<Completed<'a>, Refusal> {
        if !matches!(self.fontification, FontificationSeal::NoDemand) {
            return Err(Refusal::Fontification);
        }
        if self.tty && !matches!(self.face_entry, FaceEntrySeal::WarmOnly) {
            return Err(Refusal::ColdFaceEntry);
        }
        if self.diagnostic_effects != DiagnosticEffects::Clear {
            return Err(Refusal::DiagnosticEffects);
        }
        // These retain the deliberately narrow producer scope. Lifetime no
        // longer depends on inferring that native callbacks did not execute.
        if engine.query_body_cache_state != super::QueryBodyCacheState::Produced
            || engine.query_row_coverage != QueryRowCoverage::Complete
        {
            return Err(Refusal::DynamicOrRetry);
        }
        if engine.font_metrics.is_some() {
            return Err(Refusal::FontServices);
        }
        // Classify the actual producer seals before an incoherent capture
        // can mask their refusal. Missing reads still forbid completion.
        let reads = reads.ok_or(Refusal::Collections)?;
        // Every native owner/driver/idle/count check precedes saved-key
        // classification AND the first short source lease/header read.
        context.static_rows_observation_current(&self.epoch)?;
        let dependencies = context.finish_static_rows_observation(self.epoch, reads)?;
        let window = context
            .frame_manager()
            .get(self.frame)
            .and_then(|frame| frame.find_window(self.window))
            .ok_or(Refusal::Window)?;
        if window.buffer_id() != Some(self.source)
            || saved_window_point(window) != Some(self.window_point)
        {
            return Err(Refusal::Point);
        }
        let source = context
            .buffer_manager()
            .get(self.source)
            .ok_or(Refusal::Source)?;
        if source.point_lisp_char_pos() != self.source_point {
            return Err(Refusal::Point);
        }
        if let Some(admission) = &self.admission
            && (context.buffer_manager().current_buffer_id() != admission.current_buffer
                || context
                    .window_layout_attempt_freshness(self.frame, self.window, self.source)
                    .as_ref()
                    != Some(&admission.attempt))
        {
            return Err(Refusal::Inputs);
        }
        // SAFETY: this sole production consumer is inside the closed native
        // completion phase. The actual Producer snapshot preceded the entire
        // canonical capture_normalized, and this completion rechecked installed
        // Context/heap/GEN1/driver/full idle/count before numeric domain matching
        // or any source reference. All producer seals and source/window/point
        // checks above are native. From this call through Completed's explicit
        // drop there is no Lisp, callback/reentry, collection, safepoint, source
        // mutation, tracing, service or saved-Value formatting. The opaque Ref
        // never spans canonical normalization or a fontification callback.
        let source_read =
            unsafe { self.source_authority.validate(source) }.ok_or(Refusal::Source)?;
        let geometry = query.geometry().ok_or(Refusal::Geometry)?;
        if geometry.window_id != self.window
            || geometry.rows.is_empty()
            || geometry.point_count() == 0
            || !geometry.chrome_strings.is_empty()
        {
            return Err(Refusal::Geometry);
        }
        if self.admission.is_some() && !super::query_cache::geometry_fits(query) {
            return Err(Refusal::Geometry);
        }
        if !matches!(self.scope, WindowLayoutQueryScope::Rows { .. }) {
            return Err(Refusal::Scope);
        }
        // Numeric origin membership used the pre-read allocation domain,
        // not a post-callback heap ownership guess. This final native phase
        // executes no trace, callback, service, Lisp or allocation safepoint.
        if !dependencies.current_and_observe(context) {
            return Err(Refusal::Collections);
        }
        Ok(Completed {
            dependencies,
            geometry,
            _source: source_read,
            origin: CompletedOrigin {
                frame: self.frame,
                window: self.window,
                source: self.source,
                scope: self.scope,
                source_point: self.source_point,
                window_point: self.window_point,
                source_authority: self.source_authority,
                admission: self.admission,
            },
            _local: PhantomData,
        })
    }
}

/// Sole production call site supplies the owned query/read set from the same
/// lexical capture that began after Producer. No public metadata can bind an
/// arbitrary geometry/read pair to this interval. All fields of the retained
/// bundle remain private after this consuming transition.
pub(super) fn finish_for_admission(
    pending: Result<Producer, Refusal>,
    engine: &LayoutEngine,
    context: &Context,
    query: WindowLayoutQuery,
    reads: Option<CollectionReads>,
) -> AdmissionCompletion {
    let (report, facts) = {
        let result =
            pending.and_then(|producer| producer.finish(engine, context, &query, reads.as_ref()));
        match result {
            Ok(proof) => {
                let report = Report::Certified {
                    rows: proof.geometry.rows.len(),
                    points: proof.geometry.point_count(),
                };
                let Completed {
                    dependencies,
                    geometry: _,
                    _source: source_lease,
                    origin,
                    _local: _,
                } = proof;
                let epoch = dependencies.into_original_epoch();
                drop(source_lease);
                // The lease and every borrow of query/reads end before any trace,
                // sink or owned publication candidate can escape this function.
                match origin.admission {
                    Some(admission) => (
                        report,
                        Some(OriginalRowsFacts {
                            epoch,
                            source_authority: origin.source_authority,
                            frame: origin.frame,
                            window: origin.window,
                            source: origin.source,
                            current_buffer: admission.current_buffer,
                            scope: origin.scope,
                            source_point: origin.source_point,
                            window_point: origin.window_point,
                            attempt: admission.attempt,
                        }),
                    ),
                    None => (Report::Refused(Refusal::Scope), None),
                }
            }
            Err(refusal) => (Report::Refused(refusal), None),
        }
    };
    match (facts, reads) {
        (Some(facts), Some(reads)) => {
            // finish checked the unchanged existing cache budget before this
            // sole numeric clone, replacing the previous remember-time clone.
            let canonical_query = query.clone();
            AdmissionCompletion {
                canonical_query,
                report,
                candidate: Some(PublicationCandidate::from_finished(facts, query, reads)),
            }
        }
        (Some(_), None) => AdmissionCompletion {
            canonical_query: query,
            report: Report::Refused(Refusal::Collections),
            candidate: None,
        },
        (None, Some(_) | None) => AdmissionCompletion {
            canonical_query: query,
            report,
            candidate: None,
        },
    }
}

pub(super) fn trace_report(report: Report) {
    tracing::trace!(target: "neomacs_layout_engine::query_cache", ?report,
        "GEN1 pre-read Rows owner-domain observation; no hit admission");
}

pub(super) fn finish_and_drop(
    pending: Result<Producer, Refusal>,
    engine: &LayoutEngine,
    context: &Context,
    query: &WindowLayoutQuery,
    reads: Option<&CollectionReads>,
) -> Report {
    let result = pending.and_then(|producer| producer.finish(engine, context, query, reads));
    let report = match result {
        Ok(proof) => {
            // The proof is consumed only for this observation's numeric report.
            // No QueryCache field, get condition or query answer changes.
            let report = Report::Certified {
                rows: proof.geometry.rows.len(),
                points: proof.geometry.point_count(),
            };
            debug_assert!(proof.dependencies.current_and_observe(context));
            drop(proof); // Source Ref and dependency receipt end before any trace/sink.
            report
        }
        Err(refusal) => Report::Refused(refusal),
    };
    trace_report(report);
    report
}

#[cfg(test)]
#[path = "rows_lifetime_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tty_rows_lifetime_test.rs"]
mod tty_tests;
