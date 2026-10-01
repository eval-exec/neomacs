//! Exact, bounded geometry reuse for synchronous queries. These entries are
//! observations, never presentations or a source of retained window markers.
use neovm_core::{
    emacs_core::Context,
    window::{FrameId, WindowId, WindowLayoutQuery, WindowLayoutQueryScope},
};
use std::collections::VecDeque;

mod placement;

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
    collections: neovm_core::tagged::collection_reads::CollectionReads,
    query: WindowLayoutQuery,
}

#[derive(Default)]
pub(super) struct QueryCache {
    entries: VecDeque<Entry>,
}

impl QueryCache {
    pub(super) fn clear(&mut self) {
        self.entries.clear();
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
        let source_point = evaluator
            .buffer_manager()
            .get(buffer)?
            .point_lisp_char_pos();
        self.entries.iter().rev().find_map(|entry| {
            tracing::trace!(target: "neomacs_layout_engine::query_cache", ?scope, entry_scope = ?entry.scope,
                point_matches = entry.source_point == source_point,
                collections_match = entry.collections.unchanged(),
                freshness_matches = entry.query.geometry().and_then(|g| g.layout_freshness.as_ref()) == Some(&current),
                "query cache lookup");
            let covers_scope = entry.scope == scope || matches!(
                (entry.scope, scope),
                (WindowLayoutQueryScope::Viewport, WindowLayoutQueryScope::Position { .. })
            ) || matches!(
                (entry.scope, scope),
                (WindowLayoutQueryScope::Pixels { start: cached_start, height: cached_height },
                 WindowLayoutQueryScope::Pixels { start, height })
                    if cached_start == start && cached_height >= height
            );
            if !(entry.frame == frame
                && entry.window == window
                && covers_scope
                && entry.source_point == source_point
                && entry.collections.unchanged_and_observe())
            {
                return None;
            }
            // A larger range does not certify arbitrary callbacks invoked
            // with a different fontification extent.
            if entry.scope != scope {
                let source = evaluator.buffer_manager().get(buffer)?;
                let fontification = source.buffer_local_value("fontification-functions")
                    .or_else(|| evaluator.obarray().symbol_value("fontification-functions").copied());
                if fontification.is_some_and(|value| !value.is_nil()) {
                    return None;
                }
            }
            if entry.query.geometry()?.layout_freshness.as_ref() == Some(&current) {
                return Some(entry.query.clone());
            }
            if let WindowLayoutQueryScope::Position { target } = scope {
                // A full observation keeps the existing viewport-edge proof.
                // A target prefix needs its complete final row, rather than
                // coverage of unrelated rows below the target.
                return if matches!(entry.scope, WindowLayoutQueryScope::Viewport) {
                    placement::reposition(&entry.query, &current, source_point)
                } else {
                    placement::reposition_position(&entry.query, &current, source_point, target)
                };
            }
            if matches!(scope, WindowLayoutQueryScope::Rows { .. } | WindowLayoutQueryScope::Pixels { .. }) {
                let source = evaluator.buffer_manager().get(buffer)?;
                let fontification = source.buffer_local_value("fontification-functions")
                    .or_else(|| evaluator.obarray().symbol_value("fontification-functions").copied());
                if fontification.is_some_and(|value| !value.is_nil()) {
                    return None;
                }
                return placement::absolute_rows(&entry.query, &current);
            }
            placement::reposition(&entry.query, &current, source_point)
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
    ) {
        let Some(snapshot) = query.geometry() else {
            return;
        };
        // A frontend cache is not an evaluator GC root. Never retain Lisp
        // chrome objects here, and do not turn unbounded measurement requests
        // into persistent geometry allocations.
        if snapshot.layout_freshness.is_none()
            || !snapshot.chrome_strings.is_empty()
            || snapshot.rows.len() > MAX_ROWS
            || snapshot.body_rows.len() > MAX_ROWS
            || snapshot.points.len() > MAX_POINTS
        {
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
        // Motion commands temporarily move point while querying the same
        // viewport. Retain those independent observations within the existing
        // global bound, so returning from save-excursion does not force a new
        // row walk. Lookup still validates every input and collection read.
        self.entries.retain(|entry| {
            entry.frame != frame
                || entry.window != window
                || entry.scope != scope
                || entry.source_point != source_point
                || entry
                    .query
                    .geometry()
                    .and_then(|g| g.layout_freshness.as_ref())
                    != snapshot.layout_freshness.as_ref()
        });
        self.entries.push_back(Entry {
            frame,
            window,
            scope,
            source_point,
            collections,
            query: query.clone(),
        });
        while self.entries.len() > MAX_ENTRIES
            || self
                .entries
                .iter()
                .filter_map(|entry| entry.query.geometry())
                .map(|s| s.rows.len())
                .sum::<usize>()
                > MAX_TOTAL_ROWS
            || self
                .entries
                .iter()
                .filter_map(|entry| entry.query.geometry())
                .map(|s| s.points.len())
                .sum::<usize>()
                > MAX_TOTAL_POINTS
        {
            self.entries.pop_front();
        }
    }
}
