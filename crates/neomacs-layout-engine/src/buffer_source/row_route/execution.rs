//! Execution: the render loop's routed-row attempt. Plans the row, renders it
//! ONCE through the item renderer behind a rewindable checkpoint, and hands
//! the line end back to the buffer pipeline's own lifecycle.

use super::*;

use crate::display_cursor::CursorCaptureState;
use crate::display_row::face_state::DisplayRowActiveFaceState;
use crate::display_row::geometry::{DisplayRowExtendState, DisplayRowGeometryState};
use crate::display_row::walk_state::{
    BoxFaceRowState, DisplayRowSourceStart, FaceScanCheckpoint, LineNumberRenderState,
};
use neomacs_display_protocol::glyph_matrix::GlyphRow;

/// Everything a routed row acquisition reads that does not change during the
/// walk, bundled because the render loop passes exactly these four together on
/// every attempt.
///
/// The buffer is deliberately NOT a field: the face-resolution context already
/// carries the one the faces were resolved against, so there is no way to hand
/// the route a buffer that disagrees with its faces.
#[derive(Clone, Copy)]
pub(crate) struct PlainRowRouteRequest<'a, B: LayoutBufferView> {
    loop_context: crate::buffer_source::loop_context::BufferSourceLoopRequestContext,
    face_resolution_context:
        crate::buffer_source::face_resolution::BufferSourceFaceResolutionContext<'a, B>,
    text: &'a [u8],
    params: &'a crate::types::WindowParams,
}

impl<'a, B: LayoutBufferView> PlainRowRouteRequest<'a, B> {
    pub(crate) fn new(
        loop_context: crate::buffer_source::loop_context::BufferSourceLoopRequestContext,
        face_resolution_context: crate::buffer_source::face_resolution::BufferSourceFaceResolutionContext<'a, B>,
        text: &'a [u8],
        params: &'a crate::types::WindowParams,
    ) -> Self {
        Self {
            loop_context,
            face_resolution_context,
            text,
            params,
        }
    }
}

/// Outcome of an attempted item-renderer row acquisition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlainRowRouteOutcome {
    /// The row is not eligible (or measurement rejected it); the buffer
    /// pipeline proceeds unchanged.
    NotRouted,
    /// The row's text was rendered through the unified item renderer; the
    /// walk resumes at the end of the routed coverage — the line's newline
    /// (consumed by the buffer pipeline's own line-break lifecycle) for a
    /// whole-line plan, or the first non-fitting char (consumed by the
    /// pipeline's own overflow machinery: truncation skip or continuation
    /// transition) for an overflow-prefix plan.
    Rendered,
}

/// Everything one routed-row attempt mutates between standing at the row's
/// start and PROVING the row fits.
///
/// GNU keeps the display iterator saveable around exactly this decision —
/// `SAVE_IT` / `RESTORE_IT` (src/xdisp.c:840-861) — so a row that turns out
/// not to fit is produced again from the saved iterator. The route uses the
/// same shape for the row producer: it appends every planned part ONCE
/// through the real writer, and the first refusal rewinds to this snapshot so
/// the buffer pipeline produces the row, instead of predicting each part's
/// advance with a throwaway render and then rendering it again.
///
/// The snapshot is TOTAL by construction: the output row is replaced
/// wholesale from its captured copy (glyphs, side tables and every scalar
/// field), the emitted display points are truncated back to their captured
/// count, and the walk-side state a rendered part can touch is captured by
/// value. Adding a new piece of loop state that the commit mutates means
/// adding it here — the fields below are the complete set the render pass
/// writes.
struct RoutedRowCheckpoint {
    byte_idx: usize,
    charpos: i64,
    row_x: f32,
    row_col: usize,
    row_geometry: DisplayRowGeometryState,
    row_extend: DisplayRowExtendState,
    box_face: BoxFaceRowState,
    active_face: DisplayRowActiveFaceState,
    row_source_start: DisplayRowSourceStart,
    face_scan: FaceScanCheckpoint,
    line_numbers: LineNumberRenderState,
    cursor_info: CursorCaptureState,
    row: GlyphRow,
    /// Display points emitted into the window's geometry so far. The routed
    /// render emits them per part (the same `..._and_emit` seam the buffer
    /// pipeline uses), so a refused attempt must truncate back to this count —
    /// the producer half of the pipeline's own word-wrap rewind
    /// (`truncate_display_points` + restored row display positions/pen,
    /// overflow.rs `restore_row_output_progress`).
    display_point_len: usize,
    row_display_positions: (
        Option<neovm_core::buffer::LispCharPos1>,
        Option<neovm_core::buffer::LispCharPos1>,
    ),
    /// The face-cache speculation window the attempt opened before its
    /// first probe (`arm_face_cache_speculation`): probes realize faces and
    /// the render pass resolves more, so both journal their cache inserts.
    /// A rewind restores every memo the attempt touched, and an accepted row
    /// keeps what it earned — the buffer pipeline re-renders a refused row
    /// from a cache state identical to the one the attempt started from.
    face_cache_mark: crate::neovm_bridge::FaceCacheSpeculationMark,
}

impl RoutedRowCheckpoint {
    /// Capture the walk and row state an attempt mutates. `None` when no
    /// output row is open — the route has nothing to render into and the
    /// buffer pipeline owns the row. `face_cache_mark` is the speculation
    /// window the caller armed before its first face probe; ownership moves
    /// here so the checkpoint is the single rewind authority for the attempt.
    fn capture<FS: crate::neovm_bridge::FaceServices>(
        state: &mut crate::buffer_source::loop_state::BufferSourceLoopMutableState<'_, '_, '_, FS>,
        active_face: &DisplayRowActiveFaceState,
        face_cache_mark: crate::neovm_bridge::FaceCacheSpeculationMark,
    ) -> Option<Self> {
        let row = state.source_render.current_row_snapshot()?;
        let position = state.progress.row_position();
        let emitter = state.source_render.output_emitter_ref();
        Some(Self {
            byte_idx: state.progress.byte_idx(),
            charpos: state.progress.charpos(),
            row_x: position.x_px(),
            row_col: position.col(),
            row_geometry: *state.row_build.row_geometry,
            row_extend: state.row_build.row_extend.clone(),
            box_face: *state.row_build.box_face,
            active_face: active_face.clone(),
            row_source_start: *state.row_source_start,
            face_scan: *state.face_scan,
            line_numbers: *state.row_carryover.line_numbers,
            cursor_info: state.cursor_info.clone(),
            row,
            display_point_len: emitter.display_point_len(),
            row_display_positions: emitter.current_row_display_positions(),
            face_cache_mark,
        })
    }

    /// Rewind every mutation the attempt made. Consuming the checkpoint
    /// keeps the rewind single-shot: the next attempt captures a fresh one.
    fn restore<FS: crate::neovm_bridge::FaceServices>(
        self,
        state: &mut crate::buffer_source::loop_state::BufferSourceLoopMutableState<'_, '_, '_, FS>,
        active_face: &mut DisplayRowActiveFaceState,
    ) {
        assert!(
            state.source_render.restore_current_row_snapshot(self.row),
            "a routed-row checkpoint always belongs to an open output row"
        );
        {
            let emitter = state.source_render.output_emitter();
            emitter.truncate_display_points(self.display_point_len);
            emitter.restore_current_row_display_positions(
                self.row_display_positions.0,
                self.row_display_positions.1,
            );
            emitter.restore_current_row_pen(DisplayRowPosition::new(self.row_x, self.row_col));
        }
        *state.row_build.row_geometry = self.row_geometry;
        *state.row_build.row_extend = self.row_extend;
        *state.row_build.box_face = self.box_face;
        *state.row_source_start = self.row_source_start;
        *state.face_scan = self.face_scan;
        *state.row_carryover.line_numbers = self.line_numbers;
        *state.cursor_info = self.cursor_info;
        *active_face = self.active_face;
        state.progress.set_charpos(self.charpos);
        state.progress.set_byte_idx(self.byte_idx);
        state
            .progress
            .apply_row_position(DisplayRowPosition::new(self.row_x, self.row_col));
        // The face caches are the one mutation the row snapshot cannot carry:
        // they live on the shared resolver, not the loop state. Rewind them
        // here so a refused row leaves no face-cache side effects visible.
        state
            .source_render
            .rewind_face_cache_speculation(self.face_cache_mark);
    }

    /// Close the attempt's face-cache speculation window for a row that
    /// routed: its cache entries were earned and stay.
    fn accept<FS: crate::neovm_bridge::FaceServices>(
        self,
        state: &mut crate::buffer_source::loop_state::BufferSourceLoopMutableState<'_, '_, '_, FS>,
    ) {
        state
            .source_render
            .accept_face_cache_speculation(self.face_cache_mark);
    }
}

impl<'rows, 'emit, 'surface, FS: crate::neovm_bridge::FaceServices>
    crate::buffer_source::loop_state::BufferSourceLoopMutableState<'rows, 'emit, 'surface, FS>
{
    /// Attempt to acquire and render the row starting at the current walk
    /// position through the whole-row fast path. Only rows
    /// [`classify_row_acquisition`] approves are
    /// taken; every candidate face segment is probed (checkpoint face,
    /// per-run face-chain agreement, box-free) BEFORE any loop state is
    /// mutated, and the row's fit is then decided by the real append itself:
    /// every part is produced ONCE and a refusal rewinds the attempt. The
    /// only probe-side effects are content-addressed stable-id mints, which
    /// the pipeline performs identically for the same row.
    ///
    /// The bookkeeping the buffer pipeline performs per item is either
    /// replicated (per-segment face checkpoint via `resolve_at_checkpoint`,
    /// active-face row-extend scope, resolved-face memo) or provably idle
    /// for a classified row: cursor capture (point excluded),
    /// trailing-whitespace tracking and word-wrap candidates (both disabled
    /// by the classifier), overlay-string splits (no intersecting overlay
    /// carries a before/after-string — the classifier's overlay allow-list
    /// admits only face-affecting properties).
    pub(crate) fn try_render_plain_row_via_item_renderer<B: LayoutBufferView>(
        &mut self,
        request: PlainRowRouteRequest<'_, B>,
        source_walk: &mut crate::buffer_source::walk::BufferSourceWalk<'_, B>,
        active_face_state: &mut crate::display_row::face_state::DisplayRowActiveFaceState,
        route_refusals: &mut RouteRefusalWindow,
    ) -> PlainRowRouteOutcome {
        use crate::buffer_source::item_append::BufferSourceRowAppendContext;
        use crate::display_row::append_context::DisplayRowAppendKind;
        use crate::display_source_append_plan::DisplaySourceAppendRenderPolicy;

        let PlainRowRouteRequest {
            loop_context,
            face_resolution_context,
            text,
            params,
        } = request;
        let buffer = face_resolution_context.buffer();

        // The whole-row route cannot certify a stop inside an ordinary
        // glyph. Let the canonical element arm capture a truncated query's
        // target before it reads the physical line's invisible tail.
        if params.query_target.is_some() && params.wrap_mode == crate::types::LineWrapMode::Truncate
        {
            return PlainRowRouteOutcome::NotRouted;
        }

        // Once whole-run admission declined, the canonical producer renders
        // scalar items until the next row or source boundary. Re-entering
        // bulk measurement here changes grapheme advances mid-row.
        if source_walk.produces_single_chars_at(self.progress.charpos())
            || route_refusals.covers(self.progress.charpos())
        {
            note_route_skipped();
            return PlainRowRouteOutcome::NotRouted;
        }
        note_route_attempt();
        let position = self.progress.row_position();
        let row = RowRouteRowStart {
            text,
            byte_idx: self.progress.byte_idx(),
            charpos: self.progress.charpos(),
            text_start_byte: loop_context.text_start_byte(),
        };
        let fit = RowRouteFit {
            start_position: position,
            char_width_px: params.char_width,
            right_edge_px: self.surface.append_surface.right_edge(),
            tab_policy: self.surface.append_surface.tab_policy(),
        };
        let policy = RowRouteWindowPolicy {
            point_charpos: loop_context.point_charpos(),
            hscroll_active: params.hscroll != 0 || self.row_carryover.hscroll_skip.should_skip(),
            selective_display: loop_context.selective_display(),
            word_wrap: params.word_wrap || self.row_carryover.word_wrap.is_enabled(),
            show_trailing_whitespace: params.show_trailing_whitespace
                || self.row_carryover.trailing_whitespace.is_enabled(),
            wrap_mode: params.wrap_mode,
            overlay_string_window: self.surface.overlay_context.string_window_id(),
            scan_window_chars: params.narrowed_width_chars(),
            source_end: if loop_context.source_is_character_horizon() {
                RowRouteSourceEnd::CharacterHorizon
            } else {
                RowRouteSourceEnd::LineEnd
            },
        };
        // P4.8(a): the entry taxonomy is gone, but continuation routing still
        // needs this positional fact for its other admission checks.  It is
        // deliberately not a pen test (start_x_px > 0): a genuine line start
        // already has a non-zero pen whenever line numbers or a line prefix
        // rendered.
        let mid_line_start = row.byte_idx > 0 && row.text.get(row.byte_idx - 1) != Some(&b'\n');
        // Box terminals are an affine source property: they depend on the
        // realized faces immediately before and after each emitted range.
        // This fast route intentionally does not duplicate that policy.
        // Refuse before classification when the live face is boxed so the
        // canonical BufferTextSourceCursor computes GNU
        // start_of_box_run_p/end_of_box_run_p, including empty rows.
        if active_face_state.resolved_face().box_type != 0 {
            note_route_refusal(RouteRefusal::ProbeBoxFace);
            return PlainRowRouteOutcome::NotRouted;
        }
        let plan = match plan_plain_row_classified(buffer, row, fit, policy) {
            Ok(plan) => plan,
            Err(reason) => {
                if reason == RouteRefusal::PointInRow {
                    // Point is on this line at or after the start position, so
                    // every start position from here through point refuses the
                    // same way — see RouteRefusalWindow. The walk resumes
                    // classifying at the first position past point.
                    route_refusals.refuse_through(row.charpos, policy.point_charpos);
                }
                note_route_refusal(reason);
                return PlainRowRouteOutcome::NotRouted;
            }
        };

        // Batching owns text, never newline semantics. Even a bare newline
        // carries properties (including defaults and overlay values). Leave
        // it to BufferElementProducer just like a newline under point/region.
        if plan.is_empty_line() {
            return PlainRowRouteOutcome::NotRouted;
        }

        let start = CharPos0::new(row.charpos.max(0) as usize);
        // The routed row renders as an ordered sequence of PARTS: visible
        // text segments, (increment 2i) display-replacement spans and (P4.6)
        // overlay-string insertions, merged by char position. Every part is
        // appended by its own pipeline session during the single render pass.
        enum RoutedRowPartKind<'plan> {
            Text,
            Replacement(&'plan RoutedRowReplacement),
            /// P4.6: an INSERTION — `start == end`, no chars consumed.
            OverlayStrings(&'plan RoutedRowOverlayStrings),
        }
        impl RoutedRowPartKind<'_> {
            /// Tie-break for parts sharing a start position. An insertion
            /// belongs BEFORE the text segment that starts where it sits —
            /// GNU's `handle_stop` order, and the same insertion semantics
            /// the producer gives the element. Sorting by position alone
            /// would land every string one character late.
            fn order_rank(&self) -> u8 {
                match self {
                    Self::OverlayStrings(_) => 0,
                    Self::Text | Self::Replacement(_) => 1,
                }
            }

            fn is_insertion(&self) -> bool {
                matches!(self, Self::OverlayStrings(_))
            }
        }
        struct RoutedRowPart<'plan> {
            start: CharPos0,
            end: CharPos0,
            kind: RoutedRowPartKind<'plan>,
        }
        let mut parts: Vec<RoutedRowPart> = plan
            .segment_ranges(start)
            .into_iter()
            .map(|(seg_start, seg_end)| RoutedRowPart {
                start: seg_start,
                end: seg_end,
                kind: RoutedRowPartKind::Text,
            })
            .collect();
        parts.extend(plan.replacements().iter().map(|replacement| RoutedRowPart {
            start: start.add_len(CharLen::new(replacement.start)),
            end: start.add_len(CharLen::new(replacement.end())),
            kind: RoutedRowPartKind::Replacement(replacement),
        }));
        parts.extend(plan.overlay_strings().iter().map(|anchor| {
            let at = start.add_len(CharLen::new(anchor.at()));
            RoutedRowPart {
                start: at,
                end: at,
                kind: RoutedRowPartKind::OverlayStrings(anchor),
            }
        }));
        parts.sort_by_key(|part| (part.start.get(), part.kind.order_rank()));
        debug_assert!(
            parts
                .first()
                .is_none_or(|part| matches!(part.kind, RoutedRowPartKind::Text)),
            "a routed row never starts with a replacement or an insertion \
             (row-start anchors refuse)"
        );
        // The walk resumes at the end of the ROUTED COVERAGE — the newline
        // for a whole-line plan (not the last visible segment's end: with a
        // trailing elision they differ, the hidden span sits between them
        // and produces nothing, exactly like the pipeline's invisible skip),
        // or the first non-fitting char for an overflow-prefix plan (the
        // pipeline's own overflow machinery consumes it and everything
        // after).
        let line_end = start.add_len(CharLen::new(plan.line_char_len()));

        // ---- Face probe phase: no loop-state mutation. Resolve every
        // segment's checkpoint face (the loop already resolved segment 0's)
        // and refuse divergent per-run face chains and box faces on the NEW
        // multi-face class. The row's ADVANCE is not predicted here: the
        // single render pass below produces every part once through the real
        // append writer, which stops at the row's right edge and reports it
        // (GNU `move_it_in_display_line_to`, src/xdisp.c:10422-10451), and a
        // refused row rewinds to the checkpoint.
        //
        // Probes realize faces from cache, and both they and the render pass
        // write resolver-side memos (paint beside composed sources, folded
        // named specifications). The attempt journals all of it from before
        // its first probe: a refusal rewinds to this mark, so the caches look
        // exactly as they did before the attempt and the buffer pipeline
        // re-renders the row from the same starting state. Only the
        // content-addressed stable-id mints stay unjournaled — the pipeline
        // performs them identically for the same row.
        let face_cache_mark = self.source_render.arm_face_cache_speculation();
        struct ProbedSegment<'plan> {
            start: CharPos0,
            end: CharPos0,
            kind: RoutedRowPartKind<'plan>,
            active: crate::display_row::face_state::DisplayRowActiveFaceState,
        }
        let part_count = parts.len();
        let mut probed: Vec<ProbedSegment> = Vec::with_capacity(part_count);
        let mut carried_active: Option<crate::display_row::face_state::DisplayRowActiveFaceState> =
            None;
        for (index, part) in parts.into_iter().enumerate() {
            let (seg_start, seg_end) = (&part.start, &part.end);
            let active = if index == 0 {
                active_face_state.clone()
            } else if part.kind.is_insertion() {
                // An insertion resolves its OWN base face per string (GNU
                // face_for_overlay_string ignores the surrounding run), so
                // resolving a segment face here would mint an id nothing
                // uses. Carry the previous part's face through instead — the
                // box-face check below still sees the row's real faces.
                carried_active
                    .clone()
                    .unwrap_or_else(|| active_face_state.clone())
            } else {
                let (_, mut resolved) =
                    face_resolution_context.resolve_routed_position_face(self.face_ids, *seg_start);
                if !self
                    .source_render
                    .realize_cached_face_for_display(&mut resolved)
                {
                    note_route_refusal(RouteRefusal::UnrealizedFace);
                    self.source_render
                        .rewind_face_cache_speculation(face_cache_mark);
                    return PlainRowRouteOutcome::NotRouted;
                }
                let face_id = crate::display_row::face_state::stable_face_id_for_resolved(
                    self.face_ids,
                    &resolved,
                );
                face_resolution_context.probe_measured_active_face(
                    &mut self.source_render.reborrow(),
                    face_id,
                    resolved,
                )
            };
            // Keep every fast-route segment box-free.  A later segment can
            // acquire a boxed face even when the row-start face was plain;
            // the canonical cursor must own that row's source topology too.
            if active.resolved_face().box_type != 0 {
                note_route_refusal(RouteRefusal::ProbeBoxFace);
                self.source_render
                    .rewind_face_cache_speculation(face_cache_mark);
                return PlainRowRouteOutcome::NotRouted;
            }
            // The pipeline stamps glyphs with the PER-RUN face chain; refuse
            // the row if it ever diverges from the checkpoint chain.  Both
            // currently share one buffer-aware resolver (including inherited
            // default remapping), so divergence is an invariant violation we
            // surface at this boundary. A replacement part's glyphs take the
            // SESSION's base-face resolution instead — the run chain never
            // applies there.
            if matches!(part.kind, RoutedRowPartKind::Text)
                && self.source_render.with_cached_face_realizer(|realize| {
                    face_resolution_context
                        .with_face_realizer(realize)
                        .routed_segment_item_face_diverges(
                            self.face_ids,
                            *seg_start,
                            active.face_id(),
                        )
                })
            {
                note_route_refusal(RouteRefusal::ProbeFaceDiverges);
                self.source_render
                    .rewind_face_cache_speculation(face_cache_mark);
                return PlainRowRouteOutcome::NotRouted;
            }
            carried_active = Some(active.clone());
            probed.push(ProbedSegment {
                start: *seg_start,
                end: *seg_end,
                kind: part.kind,
                active,
            });
        }

        // ---- Rewindable checkpoint: everything the render pass below can
        // mutate, captured before its first write. GNU keeps its display
        // iterator saveable for exactly this decision (SAVE_IT / RESTORE_IT,
        // src/xdisp.c:840-861). The face-cache mark moves in with it: from
        // here the checkpoint is the attempt's single rewind authority.
        let Some(checkpoint) =
            RoutedRowCheckpoint::capture(self, active_face_state, face_cache_mark)
        else {
            // No open output row: the buffer pipeline renders this row.
            note_route_refusal(RouteRefusal::CommitRefused);
            self.source_render
                .rewind_face_cache_speculation(face_cache_mark);
            return PlainRowRouteOutcome::NotRouted;
        };

        // ---- Single render pass: every planned part is appended ONCE
        // through the real writer, replaying the pipeline's per-iteration
        // bookkeeping. Segment 0's face checkpoint already ran in the visible
        // loop; each later segment start IS the next property change, so
        // `resolve_at_checkpoint` fires there exactly as the pipeline's next
        // iteration would (installing the measured face, including row
        // extents, scoping row-extend/box).
        //
        // The writer is the fit authority: it stops at the row's right edge
        // and reports the stop, exactly as GNU's `move_it_in_display_line_to`
        // keeps a character only while
        // `it->current_x + pixel_width <= it->last_visible_x`
        // (src/xdisp.c:10422-10451). A part the row cannot hold — a clipped
        // append, a replacement the session refuses whole, or a row end that
        // misses the plan's fit bound — rewinds the whole attempt and leaves
        // the row to the buffer pipeline, which produces it from its own
        // source. Nothing here predicts an advance with a throwaway render.
        let rendered: Result<(), ()> = (|| -> Result<(), ()> {
            let mut render_position = position;
            for (index, segment) in probed.into_iter().enumerate() {
                // Test-only fault injection: refuse with the previous part's
                // glyphs already in the row, so the rewind is exercised
                // against a PARTIAL row. `RoutedRowRefusalInjection::arm`
                // never exists in production builds.
                #[cfg(test)]
                if index > 0 && take_forced_commit_refusal() {
                    return Err(());
                }
                // P4.6: DELEGATE the anchor's strings to the pipeline's own
                // session — the identical call `render.rs`'s loop-level element
                // arm makes, with the loop state this commit already owns. The
                // producer decided WHERE and in WHICH order (its collection is
                // what the plan carries); this side owns only the append.
                //
                // No face checkpoint runs first: an insertion consumes no chars,
                // so the position's checkpoint belongs to the text segment that
                // starts here, and it fires on the next iteration. The strings
                // resolve their own base face regardless.
                if let RoutedRowPartKind::OverlayStrings(anchor) = &segment.kind {
                    self.progress.apply_row_position(render_position);
                    let positions = crate::display_row::overlay_string::OverlayStringRenderPositions::from_attachment_and_layout_point(
                            segment.start,
                            loop_context.point_charpos(),
                        );
                    let continuation = self
                        .surface
                        .overlay_context
                        .without_row_transitions()
                        .render_produced_strings_at_text_row(
                            buffer,
                            positions,
                            anchor.strings(),
                            crate::display_item::DisplayStringBoxBoundaries::known(false, false),
                            self.source_render.reborrow(),
                            self.progress.visual_row_progress(),
                            self.row_build.row_geometry,
                            self.row_build.row_flags,
                            self.cursor_info,
                            self.row_source_start,
                            self.row_y_positions,
                            self.face_ids,
                            self.row_carryover.reborrow(),
                            self.face_scan,
                            active_face_state,
                        );
                    // A routable overlay string holds no newline and the plan
                    // refused every anchor an overflow prefix reaches, so the
                    // session should report neither a row break nor a clip.
                    // If it does, the row does not fit as planned: rewind.
                    if continuation.should_break() {
                        return Err(());
                    }
                    let committed = self.progress.row_position();
                    debug_assert_eq!(
                        committed.col(),
                        render_position.col() + anchor.advance_cols(),
                        "the classifier's fit walk credited a different advance than \
                         the session appended"
                    );
                    render_position = committed;
                    continue;
                }

                if index > 0 {
                    face_resolution_context.resolve_at_checkpoint_with_source_state(
                        &mut self.source_render.reborrow(),
                        self.face_scan,
                        self.face_ids,
                        active_face_state,
                        self.row_build.row_geometry,
                        self.row_build.row_extend,
                        self.row_build.box_face,
                        render_position.x_px(),
                        segment.start.get() as i64,
                    );
                    debug_assert_eq!(
                        active_face_state.face_id(),
                        segment.active.face_id(),
                        "probe and checkpoint face resolution must agree"
                    );
                }

                // A replacement part (increment 2i): render through the
                // pipeline's OWN replacement session — the same context
                // `render.rs` consume_replacement builds — so glyph provenance
                // (covered-start charpos), string base-face policy, and the
                // walk/progress bookkeeping are the session's, verbatim. The
                // pipeline performs no remember-face/row-extend bookkeeping for
                // a consumed replacement, so neither does the routed commit.
                if let RoutedRowPartKind::Replacement(replacement) = &segment.kind {
                    use crate::buffer_source::display_property_render::{
                        BufferDisplayPropertyTextReplacementApplyOutcome,
                        BufferDisplayPropertyTextReplacementRenderContext,
                        BufferDisplayPropertyTextReplacementRenderState,
                    };
                    use crate::display_row::replacement::DisplayReplacementStringRowStop;
                    let start_byte_pos = buffer.layout_char_pos_to_emacs_byte_pos(segment.start);
                    let end_byte_pos = buffer.layout_char_pos_to_emacs_byte_pos(segment.end);
                    let replacement_item =
                        crate::display_item::BufferDisplayPropertyReplacementItem::new(
                            replacement.value,
                            crate::display_property::classify_display_property_at(
                                replacement.value,
                                &buffer.layout_display_when_conditions(),
                                crate::display_property::DisplayPropertyObject::Buffer,
                                buffer.layout_display_target(),
                                crate::display_when::DisplayWhenLookup::buffer_at_char_pos(
                                    segment.start.get() as i64,
                                ),
                            ),
                            crate::display_item::BufferDisplayReplacementSource::spanning(
                                loop_context.buffer_id(),
                                segment.start,
                                start_byte_pos,
                                segment.end,
                                end_byte_pos,
                            ),
                            start_byte_pos,
                            end_byte_pos,
                            replacement.covered,
                        )
                        // Every admitted fast-route face is proven box-free. The
                        // canonical cursor owns boxed replacement topology.
                        .with_box_vertical_edges(
                            neomacs_display_protocol::face::BoxVerticalEdges::Neither,
                        );
                    self.progress.apply_row_position(render_position);
                    let replacement_context =
                        BufferDisplayPropertyTextReplacementRenderContext::new(
                            replacement_item,
                            loop_context.text_start_byte(),
                            text,
                            loop_context.content_x(),
                            params,
                            0.0,
                            loop_context.char_height(),
                            active_face_state,
                            self.progress.row_progress().x(),
                            self.progress.row_position(),
                        );
                    // `render_and_report_refusal`: a replacement the row
                    // refuses whole is REPORTED (nothing drawn, covered text
                    // unconsumed) instead of silently consumed, which is what
                    // lets this attempt rewind and the pipeline produce the
                    // row.
                    match replacement_context.render_and_report_refusal(
                        buffer,
                        BufferDisplayPropertyTextReplacementRenderState::new(
                            self.source_render.reborrow(),
                            self.face_ids,
                            self.surface.append_surface,
                            self.row_build.row_geometry,
                            active_face_state,
                            self.row_carryover.line_numbers,
                        ),
                        &mut self.progress,
                        self.cursor_info,
                        loop_context.point_charpos(),
                    ) {
                        BufferDisplayPropertyTextReplacementApplyOutcome::Applied => {
                            render_position = self.progress.row_position();
                        }
                        BufferDisplayPropertyTextReplacementApplyOutcome::RefusedWhole => {
                            // The row refused the replacement whole (GNU
                            // display_line's "doesn't fit" branch for a
                            // display element, src/xdisp.c:26448-26475):
                            // nothing was drawn, and the covered text must
                            // not be consumed. Rewind; the pipeline produces
                            // this row.
                            return Err(());
                        }
                        BufferDisplayPropertyTextReplacementApplyOutcome::String(mut session) => {
                            let Some(outcome) = session.render_next_row(
                                &mut self.source_render.reborrow(),
                                self.face_ids,
                                self.surface.append_surface,
                                self.row_build.row_geometry,
                                active_face_state,
                                self.progress.row_position(),
                            ) else {
                                return Err(());
                            };
                            self.progress.apply_row_position(outcome.end_position());
                            if !matches!(
                                outcome.stop(),
                                DisplayReplacementStringRowStop::SourceExhausted
                            ) {
                                // A clipped or multiline display string ends
                                // its own row; this row cannot carry it.
                                return Err(());
                            }
                            replacement_context.apply_completed_string(
                                session.finish(outcome.end_position()),
                                &mut self.progress,
                                self.row_carryover.line_numbers,
                            );
                            render_position = self.progress.row_position();
                        }
                        BufferDisplayPropertyTextReplacementApplyOutcome::Fallback(_) => {
                            // Unreachable for a classified plain string (the
                            // resolver only falls back when the spec is not a
                            // utf8 string). Render the covered text literally —
                            // the same glyphs the pipeline's fallback appends —
                            // and let the writer's own clip decision below
                            // refuse a row that cannot hold it.
                            let mut source = BufferPlainItemSource::text_only(
                                loop_context.buffer_id(),
                                buffer,
                                segment.start,
                                segment.end,
                                RenderFaceRef::FaceId(active_face_state.face_id()),
                            );
                            let append_context = BufferSourceRowAppendContext::from_active_face_row(
                                buffer,
                                loop_context.buffer_id(),
                                self.surface.append_surface,
                                active_face_state,
                                0.0,
                                loop_context.char_height(),
                                self.face_ids.clone(),
                            );
                            let mut render_policy = DisplaySourceAppendRenderPolicy::natural();
                            let mut source_state =
                                crate::display_row::source_state::DisplayRowSourceState::frame_local();
                            let Some(append_progress) = append_context
                                .render_display_item_source_to_text_row(
                                    self.row_build.row_geometry,
                                    &mut self.source_render.reborrow(),
                                    &mut source,
                                    &mut source_state,
                                    render_position,
                                    DisplayRowAppendKind::SourceText,
                                    &mut render_policy,
                                )
                            else {
                                return Err(());
                            };
                            if append_progress.status()
                                != crate::display_row::builder::DisplayRowAppendStatus::Complete
                            {
                                return Err(());
                            }
                            render_position = append_progress.end();
                            self.progress.apply_row_position(render_position);
                        }
                        BufferDisplayPropertyTextReplacementApplyOutcome::Stop => {
                            return Err(());
                        }
                    }
                    continue;
                }

                // Per-item bookkeeping the buffer pipeline would perform for
                // this run (item_render.rs): remember the resolved active face
                // for later splits, and scope the row-extend fill to the row.
                source_walk.remember_resolved_source_face_if_absent(
                    active_face_state.face_id(),
                    active_face_state.resolved_face(),
                );
                if let Some(fill) = active_face_state.row_extend_fill() {
                    self.row_build
                        .row_extend
                        .activate(self.row_build.row_geometry.current_row_marker(), fill);
                } else {
                    self.row_build.row_extend.clear();
                }

                // The segment's text becomes ONE `TextRun` item and is pushed
                // through the shared writer once: no separate measurement
                // pass consumed the same captured text before this render.
                let mut source = BufferPlainItemSource::text_only(
                    loop_context.buffer_id(),
                    buffer,
                    segment.start,
                    segment.end,
                    RenderFaceRef::FaceId(segment.active.face_id()),
                );
                let append_context = BufferSourceRowAppendContext::from_active_face_row(
                    buffer,
                    loop_context.buffer_id(),
                    self.surface.append_surface,
                    active_face_state,
                    0.0,
                    loop_context.char_height(),
                    self.face_ids.clone(),
                );
                let mut render_policy = DisplaySourceAppendRenderPolicy::natural();
                let mut source_state =
                    crate::display_row::source_state::DisplayRowSourceState::frame_local();
                let Some(append_progress) = append_context.render_display_item_source_to_text_row(
                    self.row_build.row_geometry,
                    &mut self.source_render.reborrow(),
                    &mut source,
                    &mut source_state,
                    render_position,
                    DisplayRowAppendKind::SourceText,
                    &mut render_policy,
                ) else {
                    return Err(());
                };
                // `Complete` is the writer's own statement that the WHOLE run
                // reached the row; a Clipped append stopped at the right edge
                // and left this segment's tail unrendered, so the row is not
                // the one the plan promised. `RowBreak` cannot come from a
                // text-only source.
                if append_progress.status()
                    != crate::display_row::builder::DisplayRowAppendStatus::Complete
                {
                    return Err(());
                }
                render_position = append_progress.end();
                self.progress.apply_row_position(render_position);
            }
            // Test-only fault injection for the LAST appended part (the
            // single-part row shape, where the mid-loop hook never fires).
            #[cfg(test)]
            if take_forced_commit_refusal() {
                return Err(());
            }
            // The pen only ever advances, so the row's END position decides
            // every part's fit: a whole-line plan must end strictly inside
            // the right edge (an exact fill belongs to the pipeline's
            // continuation/truncation policy), while an overflow-prefix or
            // end-of-source plan may end exactly at it.
            let fits = if plan.is_overflow_handoff() || plan.is_end_of_source() {
                render_position.x_px() <= self.surface.append_surface.right_edge()
            } else {
                render_position.x_px() < self.surface.append_surface.right_edge()
            };
            if fits { Ok(()) } else { Err(()) }
        })();
        if rendered.is_err() {
            checkpoint.restore(self, active_face_state);
            #[cfg(test)]
            ROUTE_REWIND_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            note_route_refusal(RouteRefusal::CommitRefused);
            return PlainRowRouteOutcome::NotRouted;
        }
        // The row routed: its cache entries were earned and stay.
        checkpoint.accept(self);

        self.progress.max_charpos(line_end.get() as i64);
        self.progress
            .set_byte_idx(row.byte_idx + plan.line_byte_len());
        note_routed_row(&plan, policy.wrap_mode, mid_line_start);
        PlainRowRouteOutcome::Rendered
    }
}
