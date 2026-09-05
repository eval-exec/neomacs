//! Execution: the render loop's routed-row attempt. Plans the row, probes and
//! commits it through the item renderer, and hands the line end back to the
//! buffer pipeline's own lifecycle.

use super::*;

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
    /// The renderer reported a stop; the visible loop must end (mirrors the
    /// buffer pipeline mapping a failed append to Stop).
    Stopped,
}

impl<'rows, 'emit, 'surface>
    crate::buffer_source::loop_state::BufferSourceLoopMutableState<'rows, 'emit, 'surface>
{
    /// Attempt to acquire and render the row starting at the current walk
    /// position through the whole-row fast path. Only rows
    /// [`classify_row_acquisition`] approves are
    /// taken; every candidate face segment is probed (checkpoint face,
    /// per-run face-chain agreement, box-free, natural-measurement fit — the
    /// same measurement the buffer pipeline's whole-run decision uses)
    /// BEFORE any loop state is mutated; everything else falls back to the
    /// buffer pipeline. The only probe-side effects are content-addressed
    /// stable-id mints, which the pipeline performs identically for the same
    /// row.
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

        if route_refusals.covers(self.progress.charpos()) {
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

        // Phase 2h rung 1: a bare-newline empty line renders RowBreak-only —
        // no text probe/commit; the row break drives the shared line-end
        // plan and row transition directly.
        if plan.is_empty_line() {
            return self.render_routed_empty_row_break(
                loop_context,
                source_walk,
                text,
                active_face_state,
                buffer,
                row,
                &plan,
                policy.wrap_mode,
                mid_line_start,
            );
        }

        let start = CharPos0::new(row.charpos.max(0) as usize);
        // The routed row renders as an ordered sequence of PARTS: visible
        // text segments and (increment 2i) display-replacement spans, merged
        // by char position. A replacement part renders through the
        // pipeline's OWN replacement session at commit; the probe phase
        // predicts its advance with the session's base-face resolution.
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
        let ranges: Vec<(CharPos0, CharPos0)> =
            parts.iter().map(|part| (part.start, part.end)).collect();
        let _ = &ranges;
        // The walk resumes at the end of the ROUTED COVERAGE — the newline
        // for a whole-line plan (not the last visible segment's end: with a
        // trailing elision they differ, the hidden span sits between them
        // and produces nothing, exactly like the pipeline's invisible skip),
        // or the first non-fitting char for an overflow-prefix plan (the
        // pipeline's own overflow machinery consumes it and everything
        // after).
        let line_end = start.add_len(CharLen::new(plan.line_char_len()));

        // ---- Probe phase: no loop-state mutation. Resolve every segment's
        // checkpoint face (the loop already resolved segment 0's), refuse
        // divergent per-run face chains and box faces on the NEW multi-face
        // class, and verify the strict natural-measurement fit segment by
        // segment at its running position.
        struct ProbedSegment<'plan> {
            start: CharPos0,
            end: CharPos0,
            kind: RoutedRowPartKind<'plan>,
            active: crate::display_row::face_state::DisplayRowActiveFaceState,
        }
        let mut probed: Vec<ProbedSegment> = Vec::with_capacity(ranges.len());
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
                let (face_id, resolved) =
                    face_resolution_context.resolve_routed_position_face(self.face_ids, *seg_start);
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
                && face_resolution_context.routed_segment_item_face_diverges(
                    self.face_ids,
                    *seg_start,
                    active.face_id(),
                )
            {
                note_route_refusal(RouteRefusal::ProbeFaceDiverges);
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

        let geometry = *self.row_build.row_geometry;
        let mut probe_position = position;
        for segment in &probed {
            // Advance-based measurement: tab expansion depends on the pen x
            // and 2-col chars advance two cells, so the measured END position
            // (x AND col) seeds the next segment's probe exactly as the
            // pipeline's own natural walk would. A replacement part measures
            // the SESSION's shape: the string's chars as one covered
            // SourceMappedText run in the session's base face (the same
            // resolution `DisplayPropertyReplacementAppendPlanItemRequest`
            // performs at commit; content-addressed mints only).
            let measured = match &segment.kind {
                // P4.6: the strings the retained session will append at this
                // anchor, measured one after another from the running pen.
                // The base face is resolved through
                // `DisplayOrigin::OverlayString` — GNU
                // `face_for_overlay_string` (xfaces.c:7034-7092) "simply
                // disregards the `face' properties of all overlays", so
                // resolving through `DisplayPropertyString` here would tint
                // the string with the overlay's own face and mis-measure a
                // box or a different font. Resolution only (no pending-face
                // install): the session performs its own at commit.
                RoutedRowPartKind::OverlayStrings(anchor) => {
                    let mut position = Some(probe_position);
                    for overlay_string in anchor.strings() {
                        let Some(at) = position else {
                            break;
                        };
                        let Some(text) = overlay_string.string.as_utf8_str() else {
                            position = None;
                            break;
                        };
                        let origin = DisplayOrigin::OverlayString {
                            overlay_id: overlay_string.overlay_id,
                            anchor_charpos: segment.start,
                            kind: if overlay_string.after_string_p {
                                crate::display_origin::OverlayStringKind::After
                            } else {
                                crate::display_origin::OverlayStringKind::Before
                            },
                        };
                        let base_face = face_resolution_context.resolve_display_string_base_face(
                            origin,
                            origin.default_base_face_policy(),
                            None,
                            crate::display_source_resolver::DisplayDefaultFaceInstallPolicy::ReuseInstalledDefaultFace,
                            self.face_ids,
                        );
                        // The insertion has its own GNU
                        // `face_for_overlay_string` base. It can be boxed even
                        // when an overlay makes the surrounding buffer run
                        // unboxed, so the earlier carried-face probe is not a
                        // sufficient admission check. Keep every boxed string
                        // on the canonical affine-topology source path.
                        if base_face.face().box_type != 0 {
                            note_route_refusal(RouteRefusal::ProbeBoxFace);
                            return PlainRowRouteOutcome::NotRouted;
                        }
                        // The session appends the string's chars one by one
                        // through the Lisp-string source; measuring them as
                        // one text item in the same face gives the same pen
                        // advance for the routable class (no tabs, no
                        // newlines, no per-char property changes), and the
                        // commit re-reads the session's OWN end position, so
                        // this prediction only has to keep the fit check
                        // honest.
                        let item = DisplayItem::new(
                            SourceSpan::new(
                                DisplaySourcePosition::buffer(
                                    loop_context.buffer_id(),
                                    segment.start,
                                    buffer.layout_char_pos_to_emacs_byte_pos(segment.start),
                                ),
                                DisplaySourcePosition::buffer(
                                    loop_context.buffer_id(),
                                    segment.start,
                                    buffer.layout_char_pos_to_emacs_byte_pos(segment.start),
                                ),
                            ),
                            RenderFaceRef::FaceId(base_face.face_id()),
                            DisplayItemKind::SourceMappedText(
                                crate::display_item::DisplaySourceMappedText::new(text),
                            ),
                        );
                        let append_context = BufferSourceRowAppendContext::from_active_face_row(
                            buffer,
                            loop_context.buffer_id(),
                            self.surface.append_surface,
                            &segment.active,
                            0.0,
                            loop_context.char_height(),
                            self.face_ids.clone(),
                        )
                        .with_resolved_item_face(base_face.face_id(), base_face.face().clone());
                        let mut measure = self.source_render.measure_state();
                        position = append_context.measure_source_display_item_advance_naturally(
                            &geometry,
                            &mut measure,
                            &item,
                            at,
                            DisplayRowAppendKind::SourceText,
                        );
                    }
                    position
                }
                RoutedRowPartKind::Text => {
                    let source = BufferPlainItemSource::text_only(
                        loop_context.buffer_id(),
                        buffer,
                        segment.start,
                        segment.end,
                        RenderFaceRef::FaceId(segment.active.face_id()),
                    );
                    let Some(text_item) = source.text_item().cloned() else {
                        note_route_refusal(RouteRefusal::ProbeMeasure);
                        return PlainRowRouteOutcome::NotRouted;
                    };
                    let append_context = BufferSourceRowAppendContext::from_active_face_row(
                        buffer,
                        loop_context.buffer_id(),
                        self.surface.append_surface,
                        &segment.active,
                        0.0,
                        loop_context.char_height(),
                        self.face_ids.clone(),
                    );
                    let mut measure = self.source_render.measure_state();
                    append_context.measure_source_display_item_advance_naturally(
                        &geometry,
                        &mut measure,
                        &text_item,
                        probe_position,
                        DisplayRowAppendKind::SourceText,
                    )
                }
                RoutedRowPartKind::Replacement(replacement)
                    if matches!(replacement.content, RoutedReplacementContent::SpaceWidth) =>
                {
                    // Rung 3 probe: resolve the spec through the SAME request
                    // the session renders (resolution only — metric queries,
                    // no append) and take its stretch width; the commit's
                    // append advances by exactly this width, with the column
                    // advance mirroring the builder's rounding.
                    let start_byte_pos = buffer.layout_char_pos_to_emacs_byte_pos(segment.start);
                    let end_byte_pos = buffer.layout_char_pos_to_emacs_byte_pos(segment.end);
                    let replacement_item =
                        crate::display_item::BufferDisplayPropertyReplacementItem::new(
                            replacement.value,
                            crate::display_property::classify_display_property(replacement.value),
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
                        );
                    let fallback_metrics =
                        crate::buffer_source::item_append::BufferSourceActiveFaceRowMetrics::from_active_face_row(
                            &segment.active,
                            loop_context.char_height(),
                        )
                        .fallback_metrics();
                    replacement_item
                        .source_text(loop_context.text_start_byte(), text)
                        .and_then(|source_text| {
                            self.source_render
                                .resolve_display_property_replacement_row_request(
                                    replacement_item.descriptor(),
                                    source_text,
                                    &segment.active,
                                    probe_position.x_px(),
                                    loop_context.content_x(),
                                    params,
                                    0.0,
                                    fallback_metrics,
                                    probe_position,
                                )
                        })
                        .and_then(|request| request.stretch_width_px())
                        .map(|width_px| {
                            if width_px <= 0.0 {
                                // A non-positive stretch appends nothing
                                // (the session's from_stretch Empty arm).
                                probe_position
                            } else {
                                probe_position.at_screen_position(
                                    probe_position.x_px() + width_px,
                                    probe_position.col()
                                        + (width_px / params.char_width.max(1.0)).round() as usize,
                                )
                            }
                        })
                }
                RoutedRowPartKind::Replacement(replacement) => {
                    let base_face = face_resolution_context.resolve_display_string_base_face(
                        DisplayOrigin::DisplayPropertyString {
                            anchor_charpos: segment.start,
                            source: crate::display_origin::DisplayPropertySource::TextProperty,
                        },
                        DisplayOrigin::DisplayPropertyString {
                            anchor_charpos: segment.start,
                            source: crate::display_origin::DisplayPropertySource::TextProperty,
                        }
                        .default_base_face_policy(),
                        Some(crate::display_source_resolver::ActiveDisplayStringBaseFace::new(
                            segment.active.face_id(),
                            segment.active.resolved_face(),
                        )),
                        crate::display_source_resolver::DisplayDefaultFaceInstallPolicy::ReuseInstalledDefaultFace,
                        self.face_ids,
                    );
                    let item = DisplayItem::new(
                        SourceSpan::new(
                            DisplaySourcePosition::buffer(
                                loop_context.buffer_id(),
                                segment.start,
                                buffer.layout_char_pos_to_emacs_byte_pos(segment.start),
                            ),
                            DisplaySourcePosition::buffer(
                                loop_context.buffer_id(),
                                segment.end,
                                buffer.layout_char_pos_to_emacs_byte_pos(segment.end),
                            ),
                        ),
                        RenderFaceRef::FaceId(base_face.face_id()),
                        DisplayItemKind::SourceMappedText(
                            crate::display_item::DisplaySourceMappedText::new(
                                match &replacement.content {
                                    RoutedReplacementContent::String { text } => text.as_ref(),
                                    RoutedReplacementContent::SpaceWidth => unreachable!(
                                        "space replacements probe through the resolved request"
                                    ),
                                },
                            ),
                        ),
                    );
                    let append_context = BufferSourceRowAppendContext::from_active_face_row(
                        buffer,
                        loop_context.buffer_id(),
                        self.surface.append_surface,
                        &segment.active,
                        0.0,
                        loop_context.char_height(),
                        self.face_ids.clone(),
                    )
                    .with_resolved_item_face(base_face.face_id(), base_face.face().clone());
                    let mut measure = self.source_render.measure_state();
                    append_context.measure_source_display_item_advance_naturally(
                        &geometry,
                        &mut measure,
                        &item,
                        probe_position,
                        DisplayRowAppendKind::SourceText,
                    )
                }
            };
            let Some(end_position) = measured else {
                note_route_refusal(RouteRefusal::ProbeMeasure);
                return PlainRowRouteOutcome::NotRouted;
            };
            probe_position = end_position;
            // Fit re-verification with the pipeline's OWN natural
            // measurement. Whole-line plans stay strict (any borderline row
            // — exact fill — keeps the buffer pipeline). Overflow-prefix
            // plans allow the prefix to end exactly AT the right edge: the
            // pipeline's fit rule is `x + advance <= right_edge`, and pen x
            // is monotonic over the run, so a measured end at or inside the
            // edge proves every routed char individually fits — the same
            // chars the flag-off pipeline would append before its overflow
            // decision fires at the handoff char. End-of-source tail plans
            // (phase 2h rung 2) share the `<=` bound: with no char after
            // the tail there is no continuation/truncation edge, the walk
            // simply ends at the source end.
            let fits = if plan.is_overflow_handoff() || plan.is_end_of_source() {
                probe_position.x_px() <= self.surface.append_surface.right_edge()
            } else {
                probe_position.x_px() < self.surface.append_surface.right_edge()
            };
            if !fits {
                note_route_refusal(RouteRefusal::ProbeMeasure);
                return PlainRowRouteOutcome::NotRouted;
            }
        }

        // ---- Commit phase: render segment by segment, replaying the
        // pipeline's per-iteration bookkeeping. Segment 0's face checkpoint
        // already ran in the visible loop; each later segment start IS the
        // next property change, so `resolve_at_checkpoint` fires there
        // exactly as the pipeline's next iteration would (installing the
        // measured face, including row extents, scoping row-extend/box).
        let mut render_position = position;
        for (index, segment) in probed.iter().enumerate() {
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
                let (x, col) = self.progress.row_progress_mut().coordinates_mut();
                let continuation = self
                    .surface
                    .overlay_context
                    .render_produced_strings_at_text_row(
                        buffer,
                        positions,
                        anchor.strings(),
                        crate::display_item::DisplayStringBoxBoundaries::known(false, false),
                        self.source_render.reborrow(),
                        x,
                        col,
                        self.row_build.row_geometry,
                        self.cursor_info,
                        self.row_source_start,
                        self.row_y_positions,
                        self.face_ids,
                        self.row_carryover.line_numbers,
                        self.face_scan,
                    );
                // A routable overlay string holds no newline and the plan
                // refused every anchor an overflow prefix reaches, so the
                // session has neither a row break nor a clip to report.
                debug_assert!(
                    !continuation.should_break(),
                    "routed overlay strings are single-line and fit the row"
                );
                if continuation.should_break() {
                    return note_route_stopped(PlainRowRouteOutcome::Stopped);
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
                        crate::display_property::classify_display_property(replacement.value),
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
                let replacement_context = BufferDisplayPropertyTextReplacementRenderContext::new(
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
                match replacement_context.render_and_apply(
                    buffer,
                    BufferDisplayPropertyTextReplacementRenderState::new(
                        self.source_render.reborrow(),
                        self.face_ids,
                        self.surface.append_surface,
                        self.row_build.row_geometry,
                        active_face_state,
                    ),
                    &mut self.progress,
                    self.cursor_info,
                    loop_context.point_charpos(),
                ) {
                    BufferDisplayPropertyTextReplacementApplyOutcome::Applied => {
                        render_position = self.progress.row_position();
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
                            return note_route_stopped(PlainRowRouteOutcome::Stopped);
                        };
                        self.progress.apply_row_position(outcome.end_position());
                        if !matches!(
                            outcome.stop(),
                            DisplayReplacementStringRowStop::SourceExhausted
                        ) {
                            debug_assert!(
                                false,
                                "the routed-row fit proof must exclude clipped or multiline display strings"
                            );
                            return note_route_stopped(PlainRowRouteOutcome::Stopped);
                        }
                        replacement_context.apply_completed_string(
                            session.finish(outcome.end_position()),
                            &mut self.progress,
                        );
                        render_position = self.progress.row_position();
                    }
                    BufferDisplayPropertyTextReplacementApplyOutcome::Fallback(_) => {
                        // Unreachable for a classified plain string (the
                        // resolver only falls back when the spec is not a
                        // utf8 string). Render the covered text literally —
                        // the same glyphs the pipeline's fallback appends.
                        debug_assert!(false, "a classified replacement string must resolve");
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
                        let geometry = *self.row_build.row_geometry;
                        let mut render_policy = DisplaySourceAppendRenderPolicy::natural();
                        let mut source_state =
                            crate::display_row::source_state::DisplayRowSourceState::frame_local();
                        let Some(append_progress) = append_context
                            .render_display_item_source_to_text_row(
                                &geometry,
                                &mut self.source_render.reborrow(),
                                &mut source,
                                &mut source_state,
                                render_position,
                                DisplayRowAppendKind::SourceText,
                                &mut render_policy,
                            )
                        else {
                            return note_route_stopped(PlainRowRouteOutcome::Stopped);
                        };
                        render_position = append_progress.end();
                        self.progress.apply_row_position(render_position);
                    }
                    BufferDisplayPropertyTextReplacementApplyOutcome::Stop => {
                        return note_route_stopped(PlainRowRouteOutcome::Stopped);
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
            let geometry = *self.row_build.row_geometry;
            let mut render_policy = DisplaySourceAppendRenderPolicy::natural();
            let mut source_state =
                crate::display_row::source_state::DisplayRowSourceState::frame_local();
            let Some(append_progress) = append_context.render_display_item_source_to_text_row(
                &geometry,
                &mut self.source_render.reborrow(),
                &mut source,
                &mut source_state,
                render_position,
                DisplayRowAppendKind::SourceText,
                &mut render_policy,
            ) else {
                return note_route_stopped(PlainRowRouteOutcome::Stopped);
            };
            render_position = append_progress.end();
            self.progress.apply_row_position(render_position);
        }

        self.progress.max_charpos(line_end.get() as i64);
        self.progress
            .set_byte_idx(row.byte_idx + plan.line_byte_len());
        note_routed_row(&plan, policy.wrap_mode, mid_line_start);
        PlainRowRouteOutcome::Rendered
    }

    /// Phase 2h rung 1 production: render a classified EMPTY line (a bare
    /// newline) through the item vocabulary's RowBreak-only shape. The
    /// [`BufferPlainItemSource`] yields exactly one explicit-newline
    /// `RowBreak` at the newline's charpos (shadow-proven glyph-identical to
    /// the pipeline's empty row in engine_test), and that break drives the
    /// SAME shared line-end plan + row-transition lifecycle the pipeline's
    /// newline dispatch uses (`BufferSourceLineBreakRenderRequest` ->
    /// `LineEndContext` -> `line_end::plan` -> `emit_line_break_then_row_start`),
    /// so the finished row carries the pinned empty-row semantics unchanged:
    /// start == end == the newline's charpos, `displays_text` false, the
    /// appended newline space in the line's own face (GNU display_line's
    /// at_end_of_line branch, xdisp.c:26517, with `default_face_p = false`).
    ///
    /// The per-char consumption bookkeeping the pipeline would run before
    /// its dispatch is provably idle for a classified empty row: the
    /// selective-display tail probe (policy refused selective display),
    /// cursor capture (point-on-newline refused by the pre-gate), overlay
    /// strings at eol (the overlay allow-list refused string-bearing
    /// overlays touching the newline; face-only overlays merge through the
    /// shared eol collector inside the line-break render), and pending
    /// source-face installation (the loop's face checkpoint already resolved
    /// and installed the face AT the newline's charpos this iteration).
    #[allow(clippy::too_many_arguments)]
    fn render_routed_empty_row_break<B: LayoutBufferView>(
        &mut self,
        loop_context: crate::buffer_source::loop_context::BufferSourceLoopRequestContext,
        source_walk: &mut crate::buffer_source::walk::BufferSourceWalk<'_, B>,
        text: &[u8],
        active_face_state: &crate::display_row::face_state::DisplayRowActiveFaceState,
        buffer: &B,
        row: RowRouteRowStart<'_>,
        plan: &PlainRowPlan,
        wrap_mode: LineWrapMode,
        mid_line_start: bool,
    ) -> PlainRowRouteOutcome {
        use crate::display_source::DisplayItemSource as _;

        debug_assert_eq!(plan.line_char_len(), 0);
        debug_assert_eq!(text.get(row.byte_idx), Some(&b'\n'));

        let line_end = CharPos0::new(row.charpos.max(0) as usize);
        let mut source = BufferPlainItemSource::with_row_break_segments(
            loop_context.buffer_id(),
            buffer,
            &[],
            line_end,
            RenderFaceRef::FaceId(active_face_state.face_id()),
        );
        let mut item_context = crate::display_source::DisplaySourceContext::empty();
        let row_break_item = source
            .next_item(&mut item_context)
            .expect("RowBreak-only source yields exactly the row break");
        debug_assert!(
            matches!(
                row_break_item.kind,
                DisplayItemKind::RowBreak(row_break)
                    if row_break == DisplayRowBreak::explicit_newline()
                        .with_line_height(DisplayLineHeightPolicy::from_property(None))
            ),
            "empty-row production must be the explicit-newline row break"
        );
        debug_assert!(source.next_item(&mut item_context).is_none());

        // Mirror the pipeline's explicit-line-break dispatch
        // (item_render.rs): byte_idx advances past the newline BEFORE the
        // line-break render; charpos is advanced/re-synced inside it.
        let source_char =
            crate::display_source::DisplaySourceStepChar::new('\n', row.byte_idx, row.charpos);
        self.progress.set_byte_idx(row.byte_idx + 1);
        let continuation = loop_context
            .line_break_request(
                source_char,
                text,
                self.surface.append_surface,
                active_face_state,
            )
            .render_and_apply(source_walk, buffer, self.reborrow());
        if continuation.should_break() {
            return note_route_stopped(PlainRowRouteOutcome::Stopped);
        }
        note_routed_row(plan, wrap_mode, mid_line_start);
        PlainRowRouteOutcome::Rendered
    }
}
