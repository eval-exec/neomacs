//! Demand observation for one synchronous Rows producer. This grants no hit.
//!
//! The ledger is confined to the owning serialized mutator's stack. It never
//! retains a Context, a Lisp value, a source snapshot, or a collector root.
use super::{LayoutEngine, LayoutPurpose};
use neovm_core::window::DisplayFrameExt;
use neovm_core::window::DisplayFrameManagerExt;
use neovm_core::{
    buffer::{BufferId, CharLen, CharPos0, CharRange},
    emacs_core::{
        Context,
        xdisp::{self, RedisplayFontificationDemand, RedisplayFontificationOutcome},
    },
    window::{FrameId, WindowDisplaySnapshot, WindowId, WindowLayoutQueryScope},
};

#[derive(Default)]
pub(super) enum RowsFontificationDemand {
    #[default]
    Inactive,
    Observing {
        window: WindowId,
        source: BufferId,
        attempt: Attempt,
        demand: RedisplayFontificationDemand,
    },
}

#[derive(Default)]
pub(super) enum Attempt {
    #[default]
    NotStarted,
    Started,
}

impl RowsFontificationDemand {
    pub(super) fn for_purpose(ctx: &Context, frame: FrameId, purpose: LayoutPurpose) -> Self {
        let LayoutPurpose::SynchronousQuery {
            window_id,
            scope: WindowLayoutQueryScope::Rows { .. },
        } = purpose
        else {
            return Self::Inactive;
        };
        let Some(source) = ctx
            .frame_manager()
            .get(frame)
            .and_then(|frame| frame.find_window(window_id))
            .and_then(|window| window.buffer_id())
        else {
            return Self::Inactive;
        };
        Self::Observing {
            window: window_id,
            source,
            attempt: Attempt::NotStarted,
            demand: RedisplayFontificationDemand::new(),
        }
    }

    /// A retry is not a completed single-source observation. Continue the
    /// canonical walk, but never publish its combined provisional footprint.
    pub(super) fn begin_leaf(&mut self, window_id: WindowId, buffer: BufferId) {
        if let Self::Observing {
            window,
            source,
            attempt,
            demand,
        } = self
        {
            if *window != window_id || *source != buffer || matches!(attempt, Attempt::Started) {
                demand.abort();
            }
            *attempt = Attempt::Started;
        }
    }

    pub(super) fn ensure(
        &mut self,
        ctx: &mut Context,
        source: BufferId,
        from: i64,
        to: i64,
    ) -> RedisplayFontificationOutcome {
        let Self::Observing { demand, .. } = self else {
            return LayoutEngine::ensure_fontified_rust(ctx, source, from, to);
        };
        match xdisp::ensure_fontified_for_redisplay_with_demand(ctx, source, from, to, demand) {
            Ok(outcome) => outcome,
            Err(error) => {
                // Identical error policy to the legacy layout bridge. The
                // observer independently rejects this incomplete observation.
                tracing::debug!("ensure_fontified_rust: fontification error: {:?}", error);
                RedisplayFontificationOutcome::CallbacksUnchanged
            }
        }
    }

    pub(super) fn finish(
        self,
        ctx: &Context,
        snapshot: Option<&WindowDisplaySnapshot>,
    ) -> Option<xdisp::RedisplayFontificationWitness> {
        if let Self::Observing {
            window,
            source,
            demand,
            ..
        } = self
        {
            let witness = match demand.finish() {
                xdisp::RedisplayFontificationWitness::NoDemand(coverage) => {
                    // Complete sparse inspection means no callback was needed,
                    // not that its already-fontified stops entered the demand
                    // ledger. Require positive coverage of the actual emitted
                    // source footprint before naming the whole Rows producer.
                    let covered = match (ctx.buffer_manager().get(source), snapshot) {
                        (Some(buffer), Some(snapshot)) if snapshot.window_id == window => {
                            let begv = buffer.point_min_char_pos();
                            let zv = buffer.point_max_char_pos();
                            let mut visited_source = false;
                            let all_covered = snapshot.iter_points().all(|point| {
                                let Some(charpos) = point
                                    .buffer_pos
                                    .as_i64()
                                    .checked_sub(1)
                                    .and_then(|position| usize::try_from(position).ok())
                                    .map(CharPos0::new)
                                else {
                                    return false;
                                };
                                if charpos == zv {
                                    // EOB is an insertion stop, not source text.
                                    return true;
                                }
                                if charpos < begv || charpos > zv {
                                    return false;
                                }
                                visited_source = true;
                                coverage.covers(
                                    source,
                                    CharRange::from_start_len(charpos, CharLen::new(1)),
                                )
                            });
                            all_covered && visited_source
                        }
                        _ => false,
                    };
                    if covered {
                        xdisp::RedisplayFontificationWitness::NoDemand(coverage)
                    } else {
                        xdisp::RedisplayFontificationWitness::Unknown
                    }
                }
                witness @ (xdisp::RedisplayFontificationWitness::CallbacksRan
                | xdisp::RedisplayFontificationWitness::Unknown
                | xdisp::RedisplayFontificationWitness::Aborted) => witness,
            };
            tracing::trace!(target: "neomacs_layout_engine::query_cache", ?witness,
                "canonical Rows fontification demand; no hit admission");
            // Stage 1 observes only. No query cache lookup consumes a witness.
            Some(witness)
        } else {
            None
        }
    }
}

#[cfg(test)]
#[path = "rows_fontification_test.rs"]
mod tests;
