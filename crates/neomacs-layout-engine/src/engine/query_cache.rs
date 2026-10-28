//! Exact, bounded geometry reuse for synchronous queries. These entries are
//! observations, never presentations or a source of retained window markers.
use super::rows_lifetime::{CertifiedRowsHit, PublicationCandidate, StoredRows};
use crate::buffer_source::render_attempt::QueryRowCoverage;
use neovm_core::{
    emacs_core::Context,
    window::{FrameId, WindowDisplaySnapshot, WindowId, WindowLayoutQuery, WindowLayoutQueryScope},
};
use std::collections::VecDeque;

mod placement;
mod slicing;

pub(super) use placement::absolute_rows;

// A page plan uses several row budgets at both ends. Bound payload as well
// as entry count so these small observations can coexist without increasing
// the previous worst-case point allocation (4 * 16,384).
const MAX_ENTRIES: usize = 16;
const MAX_TOTAL_ROWS: usize = 512;
const MAX_TOTAL_POINTS: usize = 65_536;
const MAX_ROWS: usize = 256;
const MAX_POINTS: usize = 16_384;

struct Entry {
    frame: FrameId,
    window: WindowId,
    scope: WindowLayoutQueryScope,
    source_point: neovm_core::buffer::LispCharPos1,
    payload: Payload,
    restart_rows: Vec<(neovm_core::buffer::LispCharPos1, i64)>,
    row_coverage: QueryRowCoverage,
    target_prefix_policy: bool,
}

enum Payload {
    Legacy {
        query: WindowLayoutQuery,
        collections: neovm_core::tagged::collection_reads::CollectionReads,
    },
    Certified(Box<StoredRows>),
}

impl Entry {
    fn geometry(&self) -> Option<&WindowDisplaySnapshot> {
        match &self.payload {
            Payload::Legacy { query, .. } => query.geometry(),
            Payload::Certified(stored) => stored.geometry(),
        }
    }
}

fn snapshot_fits(snapshot: &WindowDisplaySnapshot) -> bool {
    snapshot.layout_freshness.is_some()
        && snapshot.chrome_strings.is_empty()
        && snapshot.rows.len() <= MAX_ROWS
        && snapshot.body_rows.len() <= MAX_ROWS
        && snapshot.point_count() <= MAX_POINTS
}

/// Check the existing entry payload limits before any retained query clone.
/// A successful bound check supplies no producer or lifetime authority.
pub(super) fn geometry_fits(query: &WindowLayoutQuery) -> bool {
    query.geometry().is_some_and(snapshot_fits)
}

#[derive(Default)]
pub(super) struct QueryCache {
    entries: VecDeque<Entry>,
}

impl QueryCache {
    pub(super) fn clear(&mut self) {
        self.entries.clear();
    }

    /// Only the Terminal+ExactProven Rows route calls this lookup. Refusal
    /// must go straight to canonical production, not legacy/prepared reuse.
    pub(super) fn get_certified(
        &self,
        evaluator: &Context,
        frame: FrameId,
        window: WindowId,
        scope: WindowLayoutQueryScope,
    ) -> Option<CertifiedRowsHit> {
        if !matches!(scope, WindowLayoutQueryScope::Rows { .. }) {
            return None;
        }
        // This is prospective evidence only. A subscriber may invalidate any
        // candidate; all input/owner checks and leases occur AFTER this trace.
        tracing::trace!(target: "neomacs_layout_engine::query_cache", ?scope,
            entries = self.entries.len(), "certified Rows candidates before validation");
        let source = evaluator
            .frame_manager()
            .get(frame)?
            .find_window(window)?
            .buffer_id()?;
        let source_point = evaluator
            .buffer_manager()
            .get(source)?
            .point_lisp_char_pos();
        let mut current = None;
        for entry in self.entries.iter().rev() {
            if entry.frame != frame
                || entry.window != window
                || entry.scope != scope
                || entry.source_point != source_point
                || entry.row_coverage != QueryRowCoverage::Complete
            {
                continue;
            }
            let stored = match &entry.payload {
                Payload::Certified(stored) => stored,
                Payload::Legacy { .. } => continue,
            };
            // The first relevant original owner must be current before any
            // full projection. No callback/trace follows this capture. Each
            // try_hit still checks its OWN original epoch before saved keys.
            if current.is_none() {
                current = stored.current_inputs(evaluator);
            }
            let Some(current) = current.as_ref() else {
                continue;
            };
            if let Some(hit) = stored.try_hit(frame, window, scope, current) {
                return Some(hit);
            }
        }
        None
    }

    pub(super) fn get(
        &self,
        evaluator: &Context,
        frame: FrameId,
        window: WindowId,
        scope: WindowLayoutQueryScope,
    ) -> Option<WindowLayoutQuery> {
        let buffer = evaluator
            .frame_manager()
            .get(frame)?
            .find_window(window)?
            .buffer_id()?;
        // Scalar symbol values are not captured collection reads. A hook
        // installed after a callback-free prefix must run the full viewport,
        // even when all of that prefix's geometry inputs still match.
        if matches!(scope, WindowLayoutQueryScope::Position { .. })
            && super::window_source_has_fontification_callbacks(evaluator, buffer.0)
        {
            return None;
        }
        let current = evaluator.window_display_snapshot_freshness(frame, window, buffer)?;
        let chrome_absent = {
            let frame = evaluator.frame_manager().get(frame)?;
            neovm_core::window::WindowChromePresence::resolve(
                frame.find_window(window)?,
                evaluator.buffer_manager().get(buffer)?,
                frame,
                frame.minibuffer_window == Some(window),
            )
            .is_empty()
        };
        let source_point = evaluator
            .buffer_manager()
            .get(buffer)?
            .point_lisp_char_pos();
        tracing::trace!(target: "neomacs_layout_engine::query_cache", ?scope,
            entries = self.entries.len(),
            invisible_presence = ?evaluator.buffer_manager().get(buffer).map(|source|
                source.text_props_property_name_presence(
                    neovm_core::emacs_core::value::Value::symbol("invisible"))),
            "query cache candidates");
        self.entries.iter().rev().find_map(|entry| {
            // Certified payloads have their own closed owner/source/read
            // consumer. The generic legacy transformations cannot use them.
            let (query, collections) = match &entry.payload {
                Payload::Legacy { query, collections } => (query, collections),
                Payload::Certified(_) => return None,
            };
            // Target-prefix metrics can differ from a completed row's metrics
            // when a later display element increases its height. Until the
            // cache carries a metric certificate for each stop, another scope
            // cannot answer this target merely because it emitted its point.
            if entry.target_prefix_policy
                && matches!(scope, WindowLayoutQueryScope::Position { .. }) && entry.scope != scope {
                return None;
            }
            // A partial final row certifies only the original target under
            // identical inputs. Its metrics cannot answer an earlier target
            // or prove placement after a viewport change.
            if entry.row_coverage == QueryRowCoverage::TargetPrefix
                && (entry.scope != scope
                    || query.geometry()?.layout_freshness.as_ref() != Some(&current))
            {
                return None;
            }
            tracing::trace!(target: "neomacs_layout_engine::query_cache", ?scope, entry_scope = ?entry.scope,
                point_matches = entry.source_point == source_point,
                collections_match = collections.unchanged(),
                freshness_matches = query.geometry().and_then(|g| g.layout_freshness.as_ref()) == Some(&current),
                "query cache lookup");
            // A later target completes all earlier exact source points. A
            // hidden target resolved through a neighbor is insufficient: the
            // canonical earlier stop might not yet have emitted that neighbor.
            // A viewport/window-end observation preserves accepted chrome.
            // Position visibility requires live chrome measurement, so that
            // policy can be upgraded only when no chrome exists to measure.
            let covers_scope = entry.scope == scope || (chrome_absent && matches!(
                (entry.scope, scope),
                (WindowLayoutQueryScope::Viewport, WindowLayoutQueryScope::Position { .. })
            )) || matches!(
                (entry.scope, scope),
                (WindowLayoutQueryScope::Position { target: cached_target },
                 WindowLayoutQueryScope::Position { target })
                    if cached_target >= target && query.geometry()
                        .and_then(|geometry| geometry.point_for_buffer_pos(target))
                        .is_some_and(|point| point.buffer_pos == target)
            ) || matches!(
                (entry.scope, scope),
                (WindowLayoutQueryScope::Pixels { start: cached_start, height: cached_height },
                 WindowLayoutQueryScope::Pixels { start, height })
                    if cached_start == start && cached_height >= height
            ) || matches!(
                (entry.scope, scope),
                (WindowLayoutQueryScope::Pixels { start: cached_start, .. },
                 WindowLayoutQueryScope::Pixels { start, .. })
                    if cached_start < start && entry.restart_rows.iter()
                        .any(|(anchor, _)| *anchor == start)
            );
            tracing::trace!(target: "neomacs_layout_engine::query_cache", ?scope,
                entry_scope = ?entry.scope, covers_scope,
                owner_matches = entry.frame == frame && entry.window == window,
                "query cache coverage");
            if !(entry.frame == frame
                && entry.window == window
                && covers_scope
                && entry.source_point == source_point
                && collections.unchanged_and_observe())
            {
                return None;
            }
            // A larger range does not certify arbitrary callbacks invoked
            // with a different fontification extent.
            if entry.scope != scope {
                let source = evaluator.buffer_manager().get(buffer)?;
                let fontification = source.buffer_local_value("fontification-functions")
                    .or_else(|| evaluator.variable_value_by_name("fontification-functions"));
                if fontification.is_some_and(|value| !value.is_nil()) {
                    return None;
                }
            }
            if let (
                WindowLayoutQueryScope::Pixels { start: cached_start, .. },
                WindowLayoutQueryScope::Pixels { start, height },
            ) = (entry.scope, scope)
                && cached_start < start
            {
                return slicing::pixels_suffix(
                    query,
                    &current,
                    source_point,
                    start,
                    height,
                    &entry.restart_rows,
                );
            }
            if query.geometry()?.layout_freshness.as_ref() == Some(&current) {
                return Some(query.clone());
            }
            if let WindowLayoutQueryScope::Position { target } = scope {
                // A full observation keeps the existing viewport-edge proof.
                // A target prefix needs its complete final row, rather than
                // coverage of unrelated rows below the target.
                return if matches!(entry.scope, WindowLayoutQueryScope::Viewport) {
                    placement::reposition(query, &current, source_point)
                } else {
                    // Re-place the complete certified prefix, including its
                    // original final row. An earlier requested point cannot
                    // turn that coverage proof into a shorter row-set proof.
                    let completed_target = match entry.scope {
                        WindowLayoutQueryScope::Position { target } => target,
                        _ => target,
                    };
                    placement::reposition_position(
                        query, &current, source_point, completed_target,
                    )
                };
            }
            if matches!(scope, WindowLayoutQueryScope::Rows { .. } | WindowLayoutQueryScope::Pixels { .. }) {
                let source = evaluator.buffer_manager().get(buffer)?;
                let fontification = source.buffer_local_value("fontification-functions")
                    .or_else(|| evaluator.variable_value_by_name("fontification-functions"));
                if fontification.is_some_and(|value| !value.is_nil()) {
                    return None;
                }
                return placement::absolute_rows(query, &current);
            }
            placement::reposition(query, &current, source_point)
        })
    }

    pub(super) fn remember(
        &mut self,
        evaluator: &Context,
        frame: FrameId,
        window: WindowId,
        scope: WindowLayoutQueryScope,
        query: &WindowLayoutQuery,
        collections: neovm_core::tagged::collection_reads::CollectionReads,
        mut restart_rows: Vec<(neovm_core::buffer::LispCharPos1, i64)>,
        row_coverage: QueryRowCoverage,
        target_prefix_policy: bool,
    ) {
        let Some(snapshot) = query.geometry() else {
            return;
        };
        // A frontend cache is not an evaluator GC root. Never retain Lisp
        // chrome objects here, and do not turn unbounded measurement requests
        // into persistent geometry allocations.
        if !snapshot_fits(snapshot) {
            return;
        }
        // Direct query clients need not have synchronized the selected
        // window's saved point yet. The producer also reads the source point.
        let Some(buffer) = evaluator
            .frame_manager()
            .get(frame)
            .and_then(|frame| frame.find_window(window))
            .and_then(|window| window.buffer_id())
            .and_then(|buffer| evaluator.buffer_manager().get(buffer))
        else {
            return;
        };
        let source_point = buffer.point_lisp_char_pos();
        restart_rows.retain(|(anchor, index)| {
            snapshot
                .rows
                .iter()
                .any(|row| row.row == *index && row.start_buffer_pos == Some(*anchor))
        });
        restart_rows.sort_unstable_by_key(|(_, index)| *index);
        restart_rows.dedup_by_key(|(_, index)| *index);
        restart_rows.truncate(MAX_ROWS);
        // Preserve the old allocation order: release matching observations
        // before cloning the next numeric query payload.
        self.forget_matching(frame, window, scope, source_point, snapshot);
        self.push_bounded(Entry {
            frame,
            window,
            scope,
            source_point,
            payload: Payload::Legacy {
                query: query.clone(),
                collections,
            },
            restart_rows,
            row_coverage,
            target_prefix_policy,
        });
    }

    /// Consume only the opaque actual producer candidate after all report and
    /// test callbacks. Publication revalidates its ORIGINAL facts/read journal
    /// and drops the short source lease before any native cache insertion.
    pub(super) fn remember_certified(
        &mut self,
        evaluator: &Context,
        candidate: PublicationCandidate,
    ) {
        let Some(stored) = candidate.publish_if_current(evaluator) else {
            return;
        };
        let Some(snapshot) = stored.geometry() else {
            return;
        };
        if !snapshot_fits(snapshot) {
            return;
        }
        self.forget_matching(
            stored.frame(),
            stored.window(),
            stored.scope(),
            stored.source_point(),
            snapshot,
        );
        self.push_bounded(Entry {
            frame: stored.frame(),
            window: stored.window(),
            scope: stored.scope(),
            source_point: stored.source_point(),
            payload: Payload::Certified(Box::new(stored)),
            restart_rows: Vec::new(),
            row_coverage: QueryRowCoverage::Complete,
            target_prefix_policy: false,
        });
    }

    // Dedup remains before the payload clone/box in both native constructors.
    // Keep the previous exact key and release matching entries immediately.
    fn forget_matching(
        &mut self,
        frame: FrameId,
        window: WindowId,
        scope: WindowLayoutQueryScope,
        source_point: neovm_core::buffer::LispCharPos1,
        snapshot: &WindowDisplaySnapshot,
    ) {
        // Motion commands temporarily move point while querying the same
        // viewport. Preserve independent observations within the old bound.
        self.entries.retain(|previous| {
            previous.frame != frame
                || previous.window != window
                || previous.scope != scope
                || previous.source_point != source_point
                || previous
                    .geometry()
                    .and_then(|g| g.layout_freshness.as_ref())
                    != snapshot.layout_freshness.as_ref()
        });
    }

    // Both constructors checked the same per-entry bounds before cloning or
    // boxing. Keep the old global bounds and oldest-first eviction.
    fn push_bounded(&mut self, entry: Entry) {
        self.entries.push_back(entry);
        while self.entries.len() > MAX_ENTRIES
            || self
                .entries
                .iter()
                .filter_map(|entry| entry.geometry())
                .map(|s| s.rows.len())
                .sum::<usize>()
                > MAX_TOTAL_ROWS
            || self
                .entries
                .iter()
                .filter_map(|entry| entry.geometry())
                .map(|s| s.point_count())
                .sum::<usize>()
                > MAX_TOTAL_POINTS
        {
            self.entries.pop_front();
        }
    }
}
