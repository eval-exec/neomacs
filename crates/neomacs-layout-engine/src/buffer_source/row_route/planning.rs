//! Planning: turn a candidate position plus the scans' findings into a
//! [`PlainRowPlan`], or a typed [`RouteRefusal`], and resolve the face
//! segments a planned row renders with.

use super::*;

/// Classify the row starting at `row` for acquisition routing. Returns
/// Plan the routed body of the row starting at `row`, or say why the row
/// cannot route. This is the classifier: production calls it through
/// `try_render_plain_row_via_item_renderer`, and the tests call it directly.
pub(crate) fn plan_plain_row_classified<B: LayoutBufferView>(
    buffer: &B,
    row: RowRouteRowStart<'_>,
    fit: RowRouteFit<'_>,
    policy: RowRouteWindowPolicy,
) -> Result<PlainRowPlan, RouteRefusal> {
    if policy.hscroll_active {
        return Err(RouteRefusal::PolicyHscroll);
    }
    if policy.selective_display != 0 {
        return Err(RouteRefusal::PolicySelectiveDisplay);
    }
    if policy.word_wrap {
        return Err(RouteRefusal::PolicyWordWrap);
    }
    if policy.show_trailing_whitespace {
        return Err(RouteRefusal::PolicyTrailingWhitespace);
    }
    // P4.8(a): there is no entry taxonomy left. A candidate is just "the
    // walk standing at a position", and everything position-relative the
    // classifier computes (face boundaries, elision spans, replacement
    // anchors, overlay boundaries, the fit walk's tab expansion) already
    // derives from the row's own charpos and the live pen, so a mid-line
    // start is classified exactly like a line start.
    //
    // The gate that used to stand here refused every mid-line position that
    // was not an attested continuation-row resume. Its stated hazards all
    // survive as ordinary refusals a few lines down, which is why they are
    // safe to drop: a display element or overlay string in range refuses on
    // the hazard/overlay scans, and the overflow handoff char on the FIRST
    // row of a continued line refuses on the fit walk, whose pen already
    // stands at the right edge, so no fitting prefix exists. That last one
    // was a prediction and is now pinned, not assumed.
    // Cheap pre-gate BEFORE the pen walk: the refusal the steady-state edit
    // path hits every keystroke — the cursor row, including the common
    // typing-at-EOB shape where the cursor row IS the newline-less tail row
    // — is decidable from a newline search plus arithmetic, so the per-row
    // probe cost on it is one memchr (plus, inside the ambiguous byte range,
    // one branch-light count), not a full classifier walk.
    // * Line extent: up to the newline, or — phase 2h rung 2 — the whole
    //   remaining text when no newline exists (the end-of-source tail line;
    //   the read bound never cuts mid-line).
    // * Point on this line (through its newline, or through the tail's
    //   end-of-buffer position): refuse. Byte length bounds char length from
    //   above, so `point > charpos + line_byte_len` proves point is past
    //   this line with no further work; inside that ambiguous byte range,
    //   one branch-light non-continuation-byte count gives the EXACT char
    //   length, deciding line membership without the pen walk. This
    //   deliberately refuses point in the unrouted tail of an over-wide line
    //   too (which phase 2f used to route): the cursor row is the row the
    //   steady-state edit path re-lays every keystroke, and its refusal must
    //   cost a memchr, not a classifier walk. For the end-of-source tail the
    //   inclusive upper bound is one past the last char — GNU places the EOB
    //   cursor on that row (xdisp.c:26811, first ends_at_zv row wins).
    let line_byte_len =
        memchr::memchr(b'\n', &row.text[row.byte_idx..]).unwrap_or(row.text.len() - row.byte_idx);
    let line_char_len = row.text[row.byte_idx..row.byte_idx + line_byte_len]
        .iter()
        .filter(|&&byte| (byte & 0xC0) != 0x80)
        .count();
    if policy.point_charpos >= row.charpos
        && policy.point_charpos <= row.charpos + line_byte_len as i64
    {
        if policy.point_charpos <= row.charpos + line_char_len as i64 {
            return Err(RouteRefusal::PointInRow);
        }
    }
    let row_char_start = CharPos0::new(row.charpos.max(0) as usize);
    let row_char_end = row_char_start.add_len(neovm_core::buffer::CharLen::new(line_char_len));
    if buffer
        .layout_next_automatic_composition_start(row_char_start, row_char_end)
        .is_some()
    {
        return Err(RouteRefusal::ScanCompose);
    }
    // P4.8(b): an active display table can remap any char (including the
    // newline), and it is a property of the BUFFER, not of this row — so it
    // is decided HERE, ahead of every row scan, alongside the window-policy
    // gates and the line-scoped point gate. It used to sit past the display,
    // overlay, fit and elision scans, which meant a display-table buffer paid
    // a full classifier walk at every walk position to reach a verdict that
    // never consulted any of it: 14718 of 75490 production attempts after
    // P4.8(a), the largest capability refusal in the corpus.
    if crate::neovm_bridge::buffer_has_active_display_table(buffer) {
        return Err(RouteRefusal::DisplayTable);
    }

    // Display-property scan over the whole line FIRST (increment 2i rung 2):
    // routable string replacements become plan spans the fit walk credits
    // with the STRING's width; unroutable display shapes are recorded with
    // their positions and refuse below only when they fall inside the routed
    // coverage (a prop in an over-wide line's unreached tail stays with the
    // pipeline at resume, preserving the phase-2f class).
    let start_byte = row.text_start_byte + row.byte_idx;
    let display_scan =
        routed_row_replacement_scan(buffer, row.charpos, start_byte, start_byte + line_byte_len)?;

    // Overlay-string anchors over the whole line, for the same reason the
    // display scan runs first: their strings widen the pen, so the fit walk
    // below cannot find the coverage end without them.
    let overlay_scan_strings = routed_row_overlay_string_scan(
        buffer,
        policy.overlay_string_window,
        row.charpos,
        start_byte,
        start_byte + line_byte_len,
    )?;
    let mut overlay_strings = overlay_scan_strings.anchors;
    // A row carries EITHER anchors OR replacements, never both: their pen
    // bookkeeping would have to interleave (an anchor exactly at a covered
    // range's start has no defined order against it), and neither the plan's
    // parts nor the fit walk encode that precedence.
    if !overlay_strings.is_empty() && !display_scan.replacements.is_empty() {
        return Err(RouteRefusal::Overlay);
    }

    // One pass scans the chars (refusing anything the pipeline would
    // compose) AND applies the strict logical-cell fit: a line exactly
    // filling the row keeps the buffer pipeline (continuation/truncation
    // policy owns that edge).
    let scan = routed_line_scan(
        buffer,
        row.text,
        row.byte_idx,
        fit,
        &display_scan.replacements,
        &overlay_strings,
    )?;

    // An anchor whose strings are not routable refuses only when the routed
    // coverage reaches it (inclusive of the end position, like the display
    // hazard walk below). One in an over-wide line's unreached tail stays
    // with the pipeline at resume, exactly as an unroutable display prop
    // there does.
    if overlay_scan_strings
        .hazards
        .iter()
        .any(|&offset| offset <= scan.char_len)
    {
        return Err(RouteRefusal::Overlay);
    }

    // Anchor POSITION against the routed coverage, now that the fit walk has
    // found where that coverage ends.
    if scan.line_end == RoutedRowLineEnd::OverflowHandoff {
        // An overflow-prefix plan refuses ANY anchor it reaches: the append
        // session clips at the right edge and can break the row on its own,
        // so a handoff cut taken mid-anchor is not the pipeline's overflow
        // point. Anchors BEYOND the cut are unrouted remainder the pipeline
        // emits at resume, and DROPPING those rather than refusing the row is
        // what lets the continuation rows of a long wrapped line keep routing
        // when the line carries an anchor further down it. It is also what
        // keeps the producer's once-per-anchor marker honest: the routed
        // prefix never emits a string the resumed walk would emit again.
        if overlay_strings
            .iter()
            .any(|anchor| anchor.at() <= scan.char_len)
        {
            return Err(RouteRefusal::Overlay);
        }
        overlay_strings.clear();
    } else if overlay_strings
        .iter()
        .any(|anchor| anchor.at() >= scan.char_len)
    {
        // A whole-line or end-of-source plan covers the line, so its coverage
        // end IS the line end (or the end-of-buffer position). An anchor
        // there precedes a line end the pipeline's own line-break lifecycle
        // owns, or sits at the EOB position whose strings the post-loop tail
        // collects for itself.
        return Err(RouteRefusal::Overlay);
    }

    // Unroutable display props refuse when they touch the routed coverage
    // (inclusive of the end position, mirroring the historical hazard walk
    // which probed the newline / handoff char too).
    if let Some(&(_, reason)) = display_scan
        .hazards
        .iter()
        .find(|&&(offset, _)| offset <= scan.char_len)
    {
        return Err(reason);
    }
    // Keep only the replacements the routed coverage actually consumed; a
    // candidate at or beyond an overflow handoff is unrouted remainder.
    let mut replacements = display_scan.replacements;
    replacements.retain(|replacement| replacement.end() <= scan.char_len);
    // A replacement row never routes as an overflow prefix: the scan's
    // handoff cut is not the pipeline's overflow point once the covered
    // span's width substitution is in play.
    if scan.line_end == RoutedRowLineEnd::OverflowHandoff && !replacements.is_empty() {
        return Err(RouteRefusal::Replacement);
    }

    // Cursor capture stays on the buffer pipeline: exclude any row whose
    // ROUTED coverage contains point. The pre-gate above already refused the
    // byte-superset (point anywhere on the line), so this precise check is
    // defense-in-depth for the coverage interval itself.
    let routed_end_charpos = row.charpos + scan.char_len as i64;
    if policy.point_charpos >= row.charpos && policy.point_charpos <= routed_end_charpos {
        return Err(RouteRefusal::PointInRow);
    }

    // Overlays intersecting the row (touching endpoints included) may carry
    // ONLY face-affecting properties; their in-line boundaries become face
    // segment boundaries below. Anything else — strings, display, invisible,
    // window restriction, category indirection — keeps the buffer pipeline.
    let routed_end_byte = start_byte + scan.byte_len;
    let overlay_scan = routed_row_overlay_scan(buffer, row.charpos, start_byte, routed_end_byte)
        .ok_or(RouteRefusal::Overlay)?;

    // Invisible text: accept only the plain-elision class (hidden spans that
    // simply drop chars from the row); ellipsis, newline-spanning folds,
    // row-start runs, and non-advancing skips refuse. Overlay-sourced
    // invisibility never reaches this scan — any intersecting overlay
    // carrying `invisible` already refused through the overlay allow-list.
    let elided = routed_row_elision_scan(buffer, row.charpos, routed_end_charpos)
        .ok_or(RouteRefusal::Elision)?;

    // Conservative composition refusal: a routed row carries EITHER plain
    // elision OR a replacement, never both — their skip bookkeeping would
    // interleave (and GNU's handler order makes a replacing display beat
    // invisible inside the covered range, a precedence the plan's disjoint
    // gap model does not encode).
    if !replacements.is_empty() && !elided.is_empty() {
        return Err(RouteRefusal::Replacement);
    }

    // Same conservatism for an anchor meeting a hidden span: an insertion at
    // a gap edge has no defined order against the skip, and GNU's handler
    // order (invisible before overlay strings at the same stop) is not
    // something the plan's disjoint-gap model encodes.
    if !overlay_strings.is_empty() && !elided.is_empty() {
        return Err(RouteRefusal::Overlay);
    }

    // An overflow-prefix plan refuses ANY elision inside its coverage: the
    // scan's fit walk advanced the pen for every char including hidden ones,
    // so its handoff cut would not be the pipeline's overflow point. (A
    // hidden run beyond the handoff is unrouted remainder — the elision scan
    // above never sees it, and the pipeline handles it at resume.)
    if scan.line_end == RoutedRowLineEnd::OverflowHandoff && !elided.is_empty() {
        return Err(RouteRefusal::OverflowElision);
    }

    // Walk the property-change positions over the routed coverage AND its
    // end position (the newline for a whole-line plan — a display/invisible
    // property on the newline would replace it; the handoff char for an
    // overflow-prefix plan — probing it too is conservative, the pipeline
    // could handle a hazard there). Hazard properties anywhere in that range
    // refuse the route; changes strictly inside the coverage become
    // face-segment boundaries. The row may be multibyte, so boundary BYTE
    // positions convert to CHAR offsets through the buffer's own mapping.
    let mut face_boundaries = Vec::new();
    let mut probe_byte = start_byte;
    loop {
        let probe = EmacsBytePos::new(probe_byte);
        for prop in ROUTE_HAZARD_TEXT_PROPS {
            if buffer
                .layout_text_prop_at_emacs_byte_pos(probe, Value::symbol(prop))
                .is_some()
            {
                return Err(RouteRefusal::HazardProp);
            }
        }
        // Static composition: refuse exactly when the pipeline's replacement
        // predicate would fire (an inert prop still segments below, like any
        // other property change).
        if routed_composition_prop_replaces(buffer, probe) {
            return Err(RouteRefusal::HazardProp);
        }
        let Some(change) = buffer.layout_next_text_prop_change_after_emacs_byte_pos(probe) else {
            break;
        };
        let change = change.get();
        if change <= probe_byte || change > routed_end_byte {
            break;
        }
        if change < routed_end_byte {
            let change_charpos = buffer
                .layout_emacs_byte_pos_to_char_pos(EmacsBytePos::new(change))
                .get();
            let char_offset = change_charpos
                .checked_sub(row.charpos.max(0) as usize)
                .ok_or(RouteRefusal::Boundary)?;
            debug_assert!(
                char_offset > 0 && char_offset < scan.char_len,
                "a mid-line property change must land strictly inside the line"
            );
            face_boundaries.push(char_offset);
        }
        probe_byte = change;
    }

    // Overlay starts/ends are face-change stops exactly like text-property
    // changes (GNU compute_stop_pos takes the MIN of the two); merge, sort,
    // and dedupe into one ascending boundary list.
    for char_offset in overlay_scan.boundaries {
        debug_assert!(
            char_offset > 0 && char_offset < scan.char_len,
            "an in-line overlay boundary must land strictly inside the line"
        );
        face_boundaries.push(char_offset);
    }
    face_boundaries.sort_unstable();
    face_boundaries.dedup();
    // An anchor IS an overlay endpoint, so the boundary merge above already
    // cut a text segment there. The routed commit relies on it: an insertion
    // part is ordered ahead of the text segment that STARTS at its position,
    // which only exists because of this cut.
    debug_assert!(
        overlay_strings
            .iter()
            .all(|anchor| face_boundaries.binary_search(&anchor.at()).is_ok()),
        "every overlay-string anchor must have cut a face segment"
    );

    // A VISIBLE composed extender must merge into a base rendered
    // immediately before it in the SAME routed segment. If a face boundary
    // lands ON the extender, or its base is hidden (the extender sits
    // exactly at a hidden span's end), the pipeline's writer still merges it
    // across that seam — into the previous segment's glyph, keeping that
    // glyph's face — a cross-segment shape the per-segment routed render
    // does not replicate. An extender INSIDE a hidden span is fine: it is
    // simply dropped (its property-change boundaries coincide with the gap
    // and split nothing).
    for &offset in &scan.composed {
        let hidden = elided
            .iter()
            .any(|&(hidden_start, hidden_end)| offset >= hidden_start && offset < hidden_end);
        if hidden {
            continue;
        }
        if face_boundaries.binary_search(&offset).is_ok()
            || elided.iter().any(|&(_, hidden_end)| offset == hidden_end)
        {
            return Err(RouteRefusal::ComposedSeam);
        }
    }

    Ok(PlainRowPlan {
        line_byte_len: scan.byte_len,
        line_char_len: scan.char_len,
        has_tab: scan.has_tab,
        has_wide: scan.has_wide,
        has_overlay: overlay_scan.has_overlay,
        face_boundaries,
        elided,
        composed: scan.composed,
        replacements,
        overlay_strings,
        line_end: scan.line_end,
    })
}

/// The realized face of a routed row position, resolved through the SAME
/// seam the buffer pipeline's face checkpoint uses
/// ([`crate::buffer_source::face_resolution::BufferSourceFaceResolutionContext::resolve_at_checkpoint`]
/// drives `FaceResolver::default_base_face_for_origin`, GNU `face_at_pos` in
/// `handle_face_prop`), stamped with the same content-addressed stable id the
/// checkpoint would produce.
pub(crate) fn resolve_routed_position_face<B: LayoutBufferView>(
    buffer: &B,
    face_resolver: &FaceResolver,
    face_ids: &mut FrameFaceAttempt,
    pos: CharPos0,
) -> (FaceId, ResolvedFace) {
    let mut next_check = 0usize;
    let resolved = face_resolver.default_base_face_for_origin(
        Some(buffer),
        &DisplayOrigin::BufferText { charpos: pos },
        &mut next_check,
    );
    let face_id = stable_face_id_for_resolved(face_ids, &resolved);
    (face_id, resolved)
}

/// A routed row face segment: `[start, end)` rendered with `face_id`.
#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct RoutedRowFaceSegment {
    pub(crate) start: CharPos0,
    pub(crate) end: CharPos0,
    pub(crate) face_id: FaceId,
    pub(crate) resolved: ResolvedFace,
}

/// Resolve the face segments of a classified row via
/// [`resolve_routed_position_face`] — one segment per property-change stretch,
/// each carrying the realized face id the buffer pipeline's checkpoint
/// resolution produces for that span.
#[cfg(test)]
pub(crate) fn plan_row_face_segments<B: LayoutBufferView>(
    buffer: &B,
    face_resolver: &FaceResolver,
    face_ids: &mut FrameFaceAttempt,
    start: CharPos0,
    plan: &PlainRowPlan,
) -> Vec<RoutedRowFaceSegment> {
    plan.segment_ranges(start)
        .into_iter()
        .map(|(seg_start, seg_end)| {
            let (face_id, resolved) =
                resolve_routed_position_face(buffer, face_resolver, face_ids, seg_start);
            RoutedRowFaceSegment {
                start: seg_start,
                end: seg_end,
                face_id,
                resolved,
            }
        })
        .collect()
}

/// Whether the buffer pipeline's PER-RUN face chain would stamp a different
/// face id on glyphs at `pos` than the checkpoint chain (`expected_face_id`).
///
/// The pipeline resolves each run's face twice: once while producing the item
/// and once at the loop checkpoint.  Both paths are required to use the same
/// buffer-aware logical resolver, including remapping inherited named faces,
/// and therefore content-address to the same stable id.  Keep this explicit
/// comparison at the route boundary so a future resolver split surfaces as a
/// route refusal instead of silently stamping different faces on routed rows.
pub(crate) fn routed_segment_item_face_diverges<B: LayoutBufferView>(
    buffer: &B,
    face_resolver: &FaceResolver,
    face_ids: &mut FrameFaceAttempt,
    default_resolved: &ResolvedFace,
    default_face_id: FaceId,
    pos: CharPos0,
    expected_face_id: FaceId,
) -> bool {
    let bytepos = buffer.layout_char_pos_to_emacs_byte_pos(pos);
    let text_face =
        LayoutCharPropertyLookup::new(buffer, Value::symbol("face")).text_value_at(buffer, bytepos);
    // Overlay faces merge AFTER the text face, ascending priority, via the
    // SAME shared collector the run resolution uses
    // (`BufferTextSourceCursor::face_at` -> `overlay_faces_at`).
    let overlay_faces =
        crate::neovm_bridge::overlay_faces_at(buffer, bytepos, face_resolver.current_window_id())
            .faces;
    let sources = OrderedFaceSources::from_text_and_overlays(text_face, overlay_faces);
    if sources.is_empty() {
        // No face sources: the run resolves `Inherit` -> the active
        // (checkpoint) face id, which IS `expected_face_id`.
        return false;
    }
    let Some(resolved) =
        face_resolver.resolve_buffer_face_sources_over(buffer, default_resolved, &sources)
    else {
        // Every source contributed nothing: the ref stays `Inherit` ->
        // active (checkpoint) face id.
        return false;
    };
    let current_id =
        if crate::display_source_resolver::same_resolved_face(&resolved, default_resolved) {
            default_face_id
        } else {
            stable_face_id_for_resolved(face_ids, &resolved)
        };
    current_id != expected_face_id
}
