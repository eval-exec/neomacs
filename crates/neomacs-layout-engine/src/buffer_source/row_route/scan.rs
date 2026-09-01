//! Classification scans: the per-character routability ladder and the line
//! scans that answer, in ONE pass over the row's text, properties and
//! overlays, what the planner is allowed to assume.
//!
//! Everything here REPORTS (offsets, extents, hazards) and refuses nothing
//! that depends on where the routed coverage ends -- that decision belongs to
//! the planner, which knows the fit.

use super::*;

/// How a routed row char advances the pen. Only chars this classification
/// accepts may appear in a routed row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RoutedRowCharAdvance {
    /// TAB: expands to the next stop of the append surface's tab policy.
    Tab,
    /// A plain char occupying `1` or `2` unambiguous columns.
    Cols(u8),
}

/// Classify one STANDALONE char for the routed row class (the scan resolves
/// composition first: a char the pipeline would compose into the previous
/// glyph never reaches this ladder). Accepted: TAB, printable ASCII, and
/// printable non-ASCII chars whose display width is unambiguously 1 or 2
/// columns per the SAME width source the buffer pipeline advances by
/// (`neovm_core::encoding::char_width`, the GNU default `char-width-table`,
/// via `base_width_cols`). Refused (each has pipeline machinery the routed
/// render does not replicate):
/// * control chars other than TAB, and every non-Text classification
///   (`^X` caret runs, `\`+octal escapes, glyphless boxes — this arm also
///   catches the zero-width chars the glyphless policy does NOT preserve
///   for composition, e.g. ZWSP);

/// * regional indicators — the shared writer's width model
///   (`composition::base_width_cols`) forces them to 2 columns in
///   anticipation of flag-pair composition, diverging from the plain
///   `char_width` cell model this classifier fits with;

/// * contextual-shaping script chars (Arabic, Indic, …) — every such char
///   can START a run the pipeline shapes as a unit
///   (`composition::continues_complex_run` decides membership only at the
///   NEXT char), so run entry refuses here;

/// * nobreak spaces/hyphens — their display consults the
///   `nobreak-char-display` setting per char (GNU xdisp.c:8594);

/// * anything the shared width table does not size at exactly 1 or 2 cols
///   (which also refuses zero-width cluster extenders defensively — the
///   scan's compose branch normally intercepts them first).
pub(super) fn classify_routed_row_char(ch: char) -> Option<RoutedRowCharAdvance> {
    if ch == '\t' {
        return Some(RoutedRowCharAdvance::Tab);
    }
    if matches!(ch, '\x20'..='\x7E') {
        return Some(RoutedRowCharAdvance::Cols(1));
    }
    if ch.is_ascii() {
        return None;
    }
    if classify_text_source_char(ch) != TextSourceCharClassification::Text {
        return None;
    }
    if is_regional_indicator(ch as u32)
        || needs_complex_shaping(ch)
        || nonascii_space_p(ch)
        || nonascii_hyphen_p(ch)
    {
        return None;
    }
    match neovm_core::encoding::char_width(ch) {
        1 => Some(RoutedRowCharAdvance::Cols(1)),
        2 => Some(RoutedRowCharAdvance::Cols(2)),
        _ => None,
    }
}

/// Whether the pipeline's shared writer would COMPOSE `ch` into the
/// previously produced glyph instead of appending a standalone one. This is
/// the actual seam predicate, not a parallel heuristic: the writer's advance
/// ladder (`DisplayRowTextNaturalAdvanceKind::for_tail`, display_row/
/// append_context.rs) routes a text char to `ClusterContinuation` /
/// `ComplexRunMember` — merging it into a `Composite` glyph — on exactly
/// these two checks, fed by the row's `last_text_cluster_tail_in_glyphs`
/// view, which the scan mirrors as `tail`.
pub(super) fn routed_char_would_compose(ch: char, tail: Option<(char, bool)>) -> bool {
    continues_cluster(ch, tail) || continues_complex_run(ch, tail)
}

/// Whether `ch` is an extender the routed composite class accepts: a
/// zero-width cluster extender (combining marks, variation selectors, the
/// enclosing keycap) that the shared writer merges into the previous glyph
/// WITHOUT advancing the pen. Grounded in the same width source the writer's
/// composed-cluster metric sums (`composed_cluster_cols` = `string_width`,
/// GNU `cmp->width`): a zero-width extender leaves the cluster at its base's
/// columns, so the scan's fit walk and the writer agree exactly. ZWJ/ZWNJ
/// are excluded — a joiner makes the FOLLOWING char compose too
/// (`continues_cluster`'s prev-is-ZWJ arm), an open-ended sequence shape
/// (emoji ZWJ sequences) that stays refused.
pub(super) fn routed_composable_extender(ch: char) -> bool {
    !matches!(ch as u32, 0x200C | 0x200D) && neovm_core::encoding::char_width(ch) == 0
}

/// What the last scanned char left in the row for a following extender to
/// merge into, mirroring the writer's merge targets: `Simple` is a 1-column
/// non-padding Char glyph (or a Composite already grown from one) — the ONLY
/// shape the routed class lets an extender merge into. A tab's stretch glyph
/// and a wide char's base+padding pair are not routable merge targets (the
/// writer would push an orphan glyph or merge into the padding cell), and at
/// the row start there is nothing to merge into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RoutedScanMergeTarget {
    None,
    Simple,
    Wide,
}

/// A scanned routable line: `byte_len` bytes / `char_len` chars of routed
/// chars terminated by a real `\n` inside `text`, fitting strictly inside
/// the row. `composed` are the CHAR offsets of extenders the shared writer
/// merges into their preceding base glyph (ascending).
#[derive(Clone, Debug)]
pub(super) struct RoutedLineScan {
    pub(super) byte_len: usize,
    pub(super) char_len: usize,
    pub(super) has_tab: bool,
    pub(super) has_wide: bool,
    pub(super) composed: Vec<usize>,
    pub(super) line_end: RoutedRowLineEnd,
}

/// Scan for a routable line at `byte_idx`: at least one char accepted by
/// the routed ladder, either terminated by a real `\n` inside `text` with
/// the pen walk ending STRICTLY inside the right edge
/// ([`RoutedRowLineEnd::Newline`]), or — phase 2f — overflowing the row, in
/// which case the scan covers the maximal fitting prefix and stops at the
/// first char whose advance would cross the right edge
/// ([`RoutedRowLineEnd::OverflowHandoff`]). The prefix cut mirrors the
/// pipeline's fit rule (`x + advance <= right_edge` fits), with one
/// deliberate conservatism: a TAB whose expansion crosses the edge ends the
/// prefix too (the pipeline treats a tab as always fitting and clips it;

/// handing the tab back keeps that clip on the pipeline's own append).
///
/// The walk advances the pen exactly as the pipeline's natural advance does
/// for uniform `char_width_px` cells — tabs through the append surface's
/// `DisplayTabPolicy::advance_from` (GNU `next_tab_x`), wide chars by two
/// cells — and mirrors the writer's composition ladder in decision order:
/// a Text-class char is first tested against the SAME compose predicate the
/// writer applies ([`routed_char_would_compose`] over the running `tail`,
/// the scan's mirror of `last_text_cluster_tail_in_glyphs`). A composing
/// char is accepted only in the rung-2 routed class — a zero-width extender
/// ([`routed_composable_extender`]) merging into a simple 1-col base in this
/// row ([`RoutedScanMergeTarget::Simple`]); it advances the pen by ZERO
/// (the writer's merge appends no glyph and the composed-cluster metric
/// `composed_cluster_cols` counts its width as 0). Every other composing
/// shape — joiners, 2-col extenders, shaped-script runs, extenders on
/// wide/tab/row-start tails — refuses, keeping those clusters on the buffer
/// pipeline deliberately. A line exactly filling the row keeps the buffer
/// pipeline too (continuation/truncation policy owns that edge); the routed
/// render re-verifies with the pipeline's own per-face natural measurement
/// before committing. `tail` evolves as the writer's row view would: a
/// pushed char becomes `(ch, lone-regional-indicator)`, a merged extender
/// becomes the cluster's last char, a tab's stretch glyph clears it.
pub(super) fn routed_line_scan(
    text: &[u8],
    byte_idx: usize,
    fit: RowRouteFit<'_>,
    replacements: &[RoutedRowReplacement],
    overlay_strings: &[RoutedRowOverlayStrings],
) -> Result<RoutedLineScan, RouteRefusal> {
    let mut idx = byte_idx;
    let mut char_len = 0usize;
    let mut has_tab = false;
    let mut has_wide = false;
    let mut composed = Vec::new();
    let mut x_px = fit.start_position.x_px();
    let mut col = fit.start_position.col();
    let mut tail: Option<(char, bool)> = None;
    let mut merge_target = RoutedScanMergeTarget::None;
    let mut next_replacement = replacements.iter().peekable();
    let mut next_anchor = overlay_strings.iter().peekable();
    // The maximal-fitting-prefix cut for an over-wide line: every scanned
    // char so far fits; the char at `idx` would cross the right edge, so the
    // routed coverage ends here and the pipeline resumes at `idx`.
    let overflow_prefix =
        |idx: usize, char_len: usize, has_tab: bool, has_wide: bool, composed: Vec<usize>| {
            if char_len == 0 {
                return Err(RouteRefusal::ScanNoFitFirstChar);
            }
            Ok(RoutedLineScan {
                byte_len: idx - byte_idx,
                char_len,
                has_tab,
                has_wide,
                composed,
                line_end: RoutedRowLineEnd::OverflowHandoff,
            })
        };
    while idx < text.len() {
        // A replacement-covered span (increment 2i rung 2): the pen advances
        // by the REPLACEMENT string's predicted columns, not the covered
        // chars', and the covered chars are consumed without classification
        // (they are never rendered — any well-formed UTF-8 content is fine,
        // exactly like the pipeline's skip_chars_until over the covered
        // range). The session renders into the row, so a following extender
        // finds no routable merge target (tail = None) and refuses through
        // the ordinary ladder. A replacement whose advance crosses the right
        // edge refuses outright: replacement rows never route as overflow
        // prefixes (the handoff cut would not be the pipeline's overflow
        // point mid-replacement).
        if let Some(replacement) = next_replacement.peek()
            && char_len == replacement.start
        {
            let advance_px = replacement.advance_cols as f32 * fit.char_width_px;
            if x_px + advance_px > fit.right_edge_px {
                return Err(RouteRefusal::Replacement);
            }
            x_px += advance_px;
            col += replacement.advance_cols;
            tail = None;
            merge_target = RoutedScanMergeTarget::None;
            while char_len < replacement.end() {
                if idx >= text.len() || text[idx] == b'\n' {
                    return Err(RouteRefusal::Replacement);
                }
                let (ch, consumed) = decode_utf8(&text[idx..]);
                if consumed == 0 || ch.len_utf8() != consumed {
                    return Err(RouteRefusal::ScanChar);
                }
                char_len += 1;
                idx += consumed;
            }
            next_replacement.next();
            continue;
        }
        if text[idx] == b'\n' {
            // A bare-newline empty line routes with ZERO covered chars
            // (phase 2h rung 1): the production is RowBreak-only, driving
            // the shared line-end plan. A non-empty line exactly filling the
            // row keeps the pipeline (the line end interacts with
            // continuation policy); an empty line's pen never moved.
            if char_len > 0 && x_px >= fit.right_edge_px {
                return Err(RouteRefusal::ScanExactFill);
            }
            return Ok(RoutedLineScan {
                byte_len: idx - byte_idx,
                char_len,
                has_tab,
                has_wide,
                composed,
                line_end: RoutedRowLineEnd::Newline,
            });
        }
        // An overlay-string ANCHOR (P4.6): the producer surfaces its strings
        // BEFORE the buffer char at this position and does not advance (GNU
        // `push_it (it, NULL)` insertion semantics), so the pen gains the
        // strings' columns and no char is consumed. Strings that would cross
        // the right edge refuse outright rather than cutting a prefix here:
        // the append session clips and breaks rows on its own, so a cut
        // taken mid-anchor is not the pipeline's overflow point. The strings
        // append real glyphs, so a following extender finds no routable
        // merge target.
        if let Some(anchor) = next_anchor.peek()
            && char_len == anchor.at
        {
            let advance_px = anchor.advance_cols as f32 * fit.char_width_px;
            if x_px + advance_px > fit.right_edge_px {
                return Err(RouteRefusal::Overlay);
            }
            x_px += advance_px;
            col += anchor.advance_cols;
            tail = None;
            merge_target = RoutedScanMergeTarget::None;
            next_anchor.next();
            continue;
        }
        let (ch, consumed) = decode_utf8(&text[idx..]);
        // Reject malformed UTF-8 (decode yields U+FFFD over fewer bytes than
        // the char re-encodes to): raw bytes have their own display path.
        if consumed == 0 || ch.len_utf8() != consumed {
            return Err(RouteRefusal::ScanChar);
        }
        // Pipeline decision order: non-Text chars break the text run into
        // their own items BEFORE any composition (the classify arm below
        // refuses those), while a Text-class char consults the writer's
        // compose ladder first.
        if classify_text_source_char(ch) == TextSourceCharClassification::Text
            && routed_char_would_compose(ch, tail)
        {
            if !(routed_composable_extender(ch) && merge_target == RoutedScanMergeTarget::Simple) {
                return Err(RouteRefusal::ScanCompose);
            }
            // The merge appends no glyph and advances nothing; the cluster's
            // tail becomes the extender (writer: the Composite's last char).
            composed.push(char_len);
            tail = Some((ch, false));
            char_len += 1;
            idx += consumed;
            continue;
        }
        match classify_routed_row_char(ch).ok_or(RouteRefusal::ScanChar)? {
            RoutedRowCharAdvance::Tab => {
                let tab = fit.tab_policy.advance_from(
                    fit.start_position.at_screen_position(x_px, col),
                    fit.char_width_px,
                );
                // A tab crossing the right edge is clipped in place by the
                // pipeline (GNU xdisp.c:26390, tab never split): end the
                // routed prefix BEFORE it and let the pipeline append it.
                if x_px + tab.pixel_width > fit.right_edge_px {
                    return overflow_prefix(idx, char_len, has_tab, has_wide, composed);
                }
                has_tab = true;
                x_px += tab.pixel_width;
                col += tab.width_cols;
                // A tab renders a Stretch glyph: the writer's cluster-tail
                // view yields None over it.
                tail = None;
                merge_target = RoutedScanMergeTarget::None;
            }
            RoutedRowCharAdvance::Cols(cols) => {
                // The pipeline's fit rule: a char fits when its END lands at
                // or inside the right edge (`x + advance <= right_edge`,
                // DisplayRowTextOverflowDecision::for_char). The first char
                // crossing the edge — including a 2-col char straddling it —
                // ends the routed prefix; the pipeline's overflow machinery
                // consumes it (truncation skip / continuation transition).
                if x_px + f32::from(cols) * fit.char_width_px > fit.right_edge_px {
                    return overflow_prefix(idx, char_len, has_tab, has_wide, composed);
                }
                has_wide |= cols == 2;
                x_px += f32::from(cols) * fit.char_width_px;
                col += usize::from(cols);
                tail = Some((ch, is_regional_indicator(ch as u32)));
                merge_target = if cols == 1 {
                    RoutedScanMergeTarget::Simple
                } else {
                    RoutedScanMergeTarget::Wide
                };
            }
        }
        char_len += 1;
        idx += consumed;
    }
    // End of the source text without a newline (phase 2h rung 2): the tail
    // line ends at the accessible end — the read bound never cuts mid-line —
    // and routes as [`RoutedRowLineEnd::EndOfSource`]. Its end-of-buffer
    // finalization (appended default-face space, ends_at_zv, the ZV
    // placeholder) is post-loop pipeline machinery on both modes. The pen
    // may end exactly AT the right edge: with no following char there is no
    // continuation/truncation edge to interact with (every scanned char
    // individually satisfied the fit rule).
    if char_len == 0 {
        return Err(RouteRefusal::ScanEob);
    }
    Ok(RoutedLineScan {
        byte_len: idx - byte_idx,
        char_len,
        has_tab,
        has_wide,
        composed,
        line_end: RoutedRowLineEnd::EndOfSource,
    })
}

/// Overlay properties the routed row class accepts on an intersecting
/// overlay. `face` merges through the SAME resolver seam the pipeline's
/// checkpoint uses (GNU `face_at_buffer_position`'s ascending-priority
/// overlay loop), `priority` orders that merge, and `evaporate` is
/// buffer-maintenance-only. `before-string`/`after-string` joined the list at
/// P4.6 sub-step 3b: the producer owns their collection and GNU ordering, and
/// the routed commit delegates the append to the pipeline's OWN session
/// through [`RoutedRowOverlayStrings`] — the routable-shape and anchor-
/// position conditions are enforced by [`routed_row_overlay_string_scan`],
/// not by keeping the properties off this list. EVERYTHING else refuses the
/// route: `display`/`invisible` rewrite content, `mouse-face`/`line-prefix`/
/// `line-height` and friends have pipeline machinery, `window` restricts
/// applicability per window, and `category` indirects to arbitrary props.
/// Unknown properties are conservatively refused (allow-list, not deny-list).
pub(super) const ROUTE_SAFE_OVERLAY_PROPS: [&str; 5] = [
    "face",
    "priority",
    "evaporate",
    "before-string",
    "after-string",
];

/// The overlay properties that anchor a Lisp-string INSERTION at an overlay
/// endpoint (before-string at its start, after-string at its end).
pub(super) const OVERLAY_STRING_PROPS: [&str; 2] = ["before-string", "after-string"];

/// The overlay facts of a candidate row: whether any overlay intersects it
/// and the overlay start/end CHAR boundaries strictly inside the line.
pub(super) struct RoutedRowOverlayScan {
    pub(super) has_overlay: bool,
    pub(super) boundaries: Vec<usize>,
}

/// One overlay-string ANCHOR inside a routed row: the CHAR offset where the
/// producer surfaces its typed insertion element, and the strings collected
/// there in GNU `compare_overlay_entries` order.
///
/// An insertion consumes no buffer characters (GNU `push_it (it, NULL)`
/// resumes at the same position), so `at` is both the start and the end of
/// the part this becomes — which is exactly why the parts of a routed row
/// cannot be ordered by position alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RoutedRowOverlayStrings {
    /// CHAR offset of the anchor within the line.
    at: usize,
    /// The producer's collection at `at`, already GNU-ordered. The routed
    /// commit hands this slice to the SAME session `render.rs` drives, so
    /// content and order are the producer's, not a second derivation.
    strings: Vec<OverlayDisplayString>,
    /// The strings' combined logical-cell advance for the classifier's fit
    /// walk. The probe re-verifies it with the session's own base face.
    advance_cols: usize,
}

/// What [`routed_row_overlay_string_scan`] found on the line: the routable
/// anchors, and the offsets of the anchors whose strings are not routable.
/// Both are reported rather than decided, because whether either matters
/// depends on where the routed coverage ends.
#[derive(Default)]
pub(super) struct RoutedRowOverlayStringScan {
    pub(super) anchors: Vec<RoutedRowOverlayStrings>,
    /// CHAR offsets of anchors outside the routable string class. The caller
    /// refuses only for the ones the coverage reaches, mirroring how
    /// [`RoutedRowDisplayScan`] treats an unroutable `display` value.
    pub(super) hazards: Vec<usize>,
}

impl RoutedRowOverlayStrings {
    pub(crate) fn at(&self) -> usize {
        self.at
    }

    pub(crate) fn strings(&self) -> &[OverlayDisplayString] {
        &self.strings
    }

    pub(crate) fn advance_cols(&self) -> usize {
        self.advance_cols
    }
}

/// Scan the overlays intersecting `[start_byte, coverage_end_byte]` (touching
/// endpoints included: an overlay ending at the row start or starting at the
/// coverage end can anchor strings there). Returns `None` — refusing the
/// route — when any intersecting overlay carries a property outside
/// [`ROUTE_SAFE_OVERLAY_PROPS`]. Boundary positions mirror GNU
/// `next_overlay_change` feeding `compute_stop_pos`: every overlay start or
/// end strictly inside the coverage becomes a face-segment boundary (an empty
/// overlay contributes its single position).
///
/// `coverage_end_byte` is the ROUTED coverage end — the newline for a
/// whole-line plan, the handoff char for an overflow-prefix plan — so an
/// overlay living entirely in an over-wide line's unreached tail neither
/// refuses the route nor segments the row.
pub(super) fn routed_row_overlay_scan<B: LayoutBufferView + ?Sized>(
    buffer: &B,
    row_charpos: i64,
    start_byte: usize,
    coverage_end_byte: usize,
) -> Option<RoutedRowOverlayScan> {
    let overlays = buffer.layout_overlays();
    let mut scan = RoutedRowOverlayScan {
        has_overlay: false,
        boundaries: Vec::new(),
    };
    if overlays.is_empty() {
        return Some(scan);
    }
    for overlay in overlays.overlays_in_gnu_lists_order() {
        let (Some(ov_start), Some(ov_end)) = (
            overlays.overlay_start_emacs_byte_pos(overlay),
            overlays.overlay_end_emacs_byte_pos(overlay),
        ) else {
            continue;
        };
        let (ov_start, ov_end) = (ov_start.get(), ov_end.get());
        if ov_start > coverage_end_byte || ov_end < start_byte {
            continue;
        }
        // Every property of an intersecting overlay must be on the
        // allow-list; a non-symbol key or malformed plist refuses too.
        let plist = overlays.overlay_plist(overlay)?;
        let mut tail = plist;
        while tail.is_cons() {
            let prop = tail.cons_car();
            let rest = tail.cons_cdr();
            if !rest.is_cons() {
                return None;
            }
            let name = prop.as_symbol_name()?;
            if !ROUTE_SAFE_OVERLAY_PROPS
                .iter()
                .any(|allowed| *allowed == name)
            {
                return None;
            }
            tail = rest.cons_cdr();
        }
        scan.has_overlay = true;
        for boundary in [ov_start, ov_end] {
            if boundary > start_byte && boundary < coverage_end_byte {
                let char_offset = buffer
                    .layout_emacs_byte_pos_to_char_pos(EmacsBytePos::new(boundary))
                    .get()
                    .checked_sub(row_charpos.max(0) as usize)?;
                scan.boundaries.push(char_offset);
            }
        }
    }
    Some(scan)
}

/// The routable Lisp-string class, shared by `display` replacements and
/// overlay strings: a plain, property-less string whose every char has an
/// unambiguous column width. Returns its total advance in columns.
///
/// A newline would end the row, a tab expands pen-dependently in the
/// session's own frame, and text properties re-face (or re-shape) the string
/// mid-flight — none of which the classifier's logical-cell fit walk can
/// predict, so all three refuse. This is ONE predicate deliberately: the two
/// callers must agree about what "plain enough to route" means, and a second
/// copy would drift.
pub(super) fn routed_lisp_string_advance_cols(value: Value) -> Option<usize> {
    let text = value.as_utf8_str()?;
    if text.is_empty() {
        return None;
    }
    if neovm_core::emacs_core::value::get_string_text_properties_table_for_value(value).is_some() {
        return None;
    }
    let mut advance_cols = 0usize;
    for ch in text.chars() {
        match classify_routed_row_char(ch) {
            Some(RoutedRowCharAdvance::Cols(cols)) => advance_cols += usize::from(cols),
            Some(RoutedRowCharAdvance::Tab) | None => return None,
        }
    }
    Some(advance_cols)
}

/// Find the overlay-string anchors on the line `[start_byte, line_end_byte]`
/// and collect the strings at each, refusing the route for anything the
/// routed row shape cannot express.
///
/// Anchor positions are the endpoints of the intersecting overlays that carry
/// a string property (before-string at the start, after-string at the end);

/// the CONTENT at each position comes from `overlay_strings_at`, the same
/// producer-side collection the pipeline renders, so the two can never
/// disagree about which strings are there, in which order, or whether a
/// position is an anchor at all (GNU drops empty strings at collection —
/// `STRINGP && SCHARS`, xdisp.c:7171-7182 — so an overlay whose only string
/// is `""` anchors nothing and must not refuse).
///
/// Refuses outright only for an anchor ON the row start: the visible loop
/// attempts the route BEFORE the pipeline step that would emit the strings
/// (loop_render.rs), so a routed row start would DROP them. That is a
/// correctness bound, and it needs no coverage knowledge — offset 0 is inside
/// every coverage.
///
/// Everything else this scan finds is REPORTED, not decided, because both
/// remaining questions turn on where the routed coverage ends and the fit
/// walk has not run yet. An anchor whose strings are outside the routable
/// class is recorded as a HAZARD at its offset, exactly as
/// [`routed_row_replacement_scan`] records an unroutable `display` value, and
/// the caller refuses only if the coverage reaches it. Deciding either here
/// instead cost real routing: refusing a line-end anchor lost every prefix
/// row of a long wrapped line, and refusing an unroutable string lost the
/// prefix rows of a line whose unroutable string sits in the unreached tail.
///
/// Anchors before `start_byte` are simply not this row's: they were emitted
/// on an earlier row.
pub(super) fn routed_row_overlay_string_scan<B: LayoutBufferView>(
    buffer: &B,
    window_id: Option<u64>,
    row_charpos: i64,
    start_byte: usize,
    line_end_byte: usize,
) -> Result<RoutedRowOverlayStringScan, RouteRefusal> {
    let mut scan = RoutedRowOverlayStringScan::default();
    // No window means this row renders no overlay strings at all, so no
    // position on it is an anchor (the append is a no-op either way).
    let Some(window_id) = window_id else {
        return Ok(scan);
    };
    let overlays = buffer.layout_overlays();
    if overlays.is_empty() {
        return Ok(scan);
    }

    // Candidate positions first, content second: reading the plists of the
    // overlays that intersect the line is cheap, and it keeps the per-anchor
    // collection (which walks a byte range and sorts) off every row that has
    // face-only overlays.
    let mut anchor_bytes: Vec<usize> = Vec::new();
    for overlay in overlays.overlays_in_gnu_lists_order() {
        let (Some(ov_start), Some(ov_end)) = (
            overlays.overlay_start_emacs_byte_pos(overlay),
            overlays.overlay_end_emacs_byte_pos(overlay),
        ) else {
            continue;
        };
        let (ov_start, ov_end) = (ov_start.get(), ov_end.get());
        if ov_start > line_end_byte || ov_end < start_byte {
            continue;
        }
        if OVERLAY_STRING_PROPS.iter().any(|prop| {
            overlays
                .overlay_get_named(overlay, Value::symbol(prop))
                .is_some()
        }) {
            anchor_bytes.push(ov_start);
            anchor_bytes.push(ov_end);
        }
    }
    anchor_bytes.sort_unstable();
    anchor_bytes.dedup();

    let props = crate::neovm_bridge::RustTextPropAccess::new_for_window(buffer, window_id);
    for anchor_byte in anchor_bytes {
        if anchor_byte < start_byte || anchor_byte > line_end_byte {
            continue;
        }
        let anchor_charpos = buffer
            .layout_emacs_byte_pos_to_char_pos(EmacsBytePos::new(anchor_byte))
            .get();
        let strings = props.overlay_strings_at(anchor_charpos as i64);
        if strings.is_empty() {
            continue;
        }
        if anchor_byte == start_byte {
            return Err(RouteRefusal::Overlay);
        }
        let at = anchor_charpos
            .checked_sub(row_charpos.max(0) as usize)
            .ok_or(RouteRefusal::Boundary)?;
        let mut advance_cols = 0usize;
        let routable =
            strings.iter().all(
                |string| match routed_lisp_string_advance_cols(string.string) {
                    Some(cols) => {
                        advance_cols += cols;
                        true
                    }
                    None => false,
                },
            );
        if !routable {
            scan.hazards.push(at);
            continue;
        }
        scan.anchors.push(RoutedRowOverlayStrings {
            at,
            strings,
            advance_cols,
        });
    }
    Ok(scan)
}

/// Scan the line `[start_byte, line_end_byte]` for `display` text properties
/// (increment 2i rung 2), walking property-change positions exactly like the
/// hazard walk. Every `display` value found must be a routable replacement
/// (a plain, property-less, single-line string of unambiguous-width chars,
/// anchored strictly inside the line and covering chars strictly before the
/// newline) or the row refuses. The covered extent is not mirrored from the
/// pipeline — it IS the pipeline's, taken from
/// [`ReplacementCoveredSpan::for_property_source`] through
/// [`RoutedScanExtentLookup`] (GNU `display_prop_end` /
/// `next_single_char_property_change(pos, Qdisplay)`: the range over which the
/// resolved value stays the SAME object). `line_end_byte` is the newline's
/// byte (or the source end for the tail line); a value covering it would
/// replace the line end and refuses.
///
/// Overlay-supplied `display` never reaches this scan: the overlay allow-list
/// refuses any intersecting overlay carrying `display`, so the text-property
/// read here resolves the same winner as the pipeline's
/// `get_char_property`-style overlay-or-text read.
/// Parse the rung-3 routable space shape: exactly `(space :width N)` with a
/// positive fixnum N, nothing else. Every other `(space …)` form —
/// `:align-to` (targets a column), `:relative-width` (consults the covered
/// char's font), extra vertical keys, float widths, expression operands
/// riding `calc_pixel_width_or_height` — keeps the buffer pipeline: their
/// widths are pen/metric-dependent in ways the classifier's logical-cell
/// pre-filter cannot predict. For the plain form GNU's width is N times the
/// canonical column width (xdisp.c calc_pixel_width_or_height, bare numbers
/// scale by FRAME_COLUMN_WIDTH on the horizontal axis), which is exactly N
/// advance columns for the fit walk; the probe re-verifies with the
/// session's own resolved stretch width.
pub(super) fn routed_space_width_cols(spec: Value) -> Option<usize> {
    use crate::display_spec::DisplaySpaceKey;
    if !crate::display_spec::is_display_space_spec(&spec) {
        return None;
    }
    let rest = spec.cons_cdr();
    if !rest.is_cons() {
        return None;
    }
    if DisplaySpaceKey::from_lisp_value(rest.cons_car()) != Some(DisplaySpaceKey::Width) {
        return None;
    }
    let tail = rest.cons_cdr();
    if !tail.is_cons() || !tail.cons_cdr().is_nil() {
        return None;
    }
    let cols = tail.cons_car().as_fixnum()?;
    (1..=512).contains(&cols).then_some(cols as usize)
}

/// Outcome of the display-property line scan: the routable replacement
/// candidates in ascending order, plus the CHAR offsets (with refusal
/// reasons) of unroutable `display` props. The classifier refuses only when
/// an unroutable prop falls inside (or at the end of) the ROUTED coverage —
/// a prop in the unreached tail of an overflow-prefix plan stays with the
/// pipeline at resume, preserving the phase-2f class exactly.
pub(super) struct RoutedRowDisplayScan {
    pub(super) replacements: Vec<RoutedRowReplacement>,
    pub(super) hazards: Vec<(usize, RouteRefusal)>,
}

/// The four lookups [`ReplacementCoveredSpan::for_property_source`] needs, as
/// the routed line scan can answer them: over the buffer view's TEXT
/// properties, bounded by the line end.
///
/// The scan reads only text properties, and that is sound rather than lucky:
/// `display` is not in [`ROUTE_SAFE_OVERLAY_PROPS`], so an overlay carrying one
/// refuses the route outright, well before this scan runs. The source reaching
/// the constructor from here is therefore always `overlay: None`.
pub(super) struct RoutedScanExtentLookup<'a, B: ?Sized> {
    buffer: &'a B,
    /// The line end (the newline's position, or the source end for the tail
    /// line). Clipping the extent here rather than letting it run past is what
    /// lets the caller ask its one routability question — does the covered
    /// range reach the line end? — as a comparison instead of a second walk.
    line_end: CharPos0,
}

impl<B: LayoutBufferView + ?Sized> DisplayReplacementExtentLookup
    for RoutedScanExtentLookup<'_, B>
{
    fn extent_scan_end(&self) -> CharPos0 {
        self.line_end
    }

    fn extent_overlay_end(&self, _overlay: Value) -> Option<CharPos0> {
        // Unreachable by construction (see the struct doc): an overlay-sourced
        // `display` refuses the route before the scan runs. `None` is the
        // conservative answer if that ever stops holding — the constructor
        // falls back to the property run's own end, which cannot over-cover.
        debug_assert!(false, "the routed scan never resolves an overlay source");
        None
    }

    fn extent_display_prop_at(&self, at: CharPos0) -> Option<Value> {
        self.buffer.layout_text_prop_at_emacs_byte_pos(
            self.buffer.layout_char_pos_to_emacs_byte_pos(at),
            Value::symbol("display"),
        )
    }

    fn extent_next_property_change(&self, at: CharPos0) -> CharPos0 {
        let byte = self.buffer.layout_char_pos_to_emacs_byte_pos(at);
        self.buffer
            .layout_next_text_prop_change_after_emacs_byte_pos(byte)
            .map(|change| change.get())
            .filter(|&change| change > byte.get())
            .map(|change| {
                self.buffer
                    .layout_emacs_byte_pos_to_char_pos(EmacsBytePos::new(change))
            })
            // No further change means the properties hold to the end of the
            // buffer, so the covered range reaches the line end and past it.
            // Reporting the bound says exactly that, and the caller's line-end
            // test then refuses — which is what the inline walk did directly.
            .unwrap_or(self.line_end)
    }
}

pub(super) fn routed_row_replacement_scan<B: LayoutBufferView>(
    buffer: &B,
    row_charpos: i64,
    start_byte: usize,
    line_end_byte: usize,
) -> Result<RoutedRowDisplayScan, RouteRefusal> {
    use crate::display_property::{DisplayReplacementProperty, classify_display_property};

    let display_prop_at = |byte: usize| {
        buffer.layout_text_prop_at_emacs_byte_pos(EmacsBytePos::new(byte), Value::symbol("display"))
    };
    let char_offset_at = |byte: usize| -> Result<usize, RouteRefusal> {
        buffer
            .layout_emacs_byte_pos_to_char_pos(EmacsBytePos::new(byte))
            .get()
            .checked_sub(row_charpos.max(0) as usize)
            .ok_or(RouteRefusal::Boundary)
    };
    let next_change_after = |byte: usize| {
        buffer
            .layout_next_text_prop_change_after_emacs_byte_pos(EmacsBytePos::new(byte))
            .map(|change| change.get())
            .filter(|&change| change > byte)
    };

    let line_end = buffer.layout_emacs_byte_pos_to_char_pos(EmacsBytePos::new(line_end_byte));

    let mut scan = RoutedRowDisplayScan {
        replacements: Vec::new(),
        hazards: Vec::new(),
    };
    let mut probe_byte = start_byte;
    loop {
        if let Some(value) = display_prop_at(probe_byte) {
            let probe_offset = char_offset_at(probe_byte)?;
            // The routable class: a plain, property-less, single-line string
            // of unambiguous-width chars, anchored strictly inside the line
            // (a row-start anchor replays into the loop's segment-0
            // checkpoint; a line-end anchor replaces the newline), covering
            // chars strictly before the line end. Everything else records a
            // hazard at its position: non-string display shapes keep the
            // historical HazardProp refusal, unroutable string shapes the
            // Replacement refusal.
            let classification = classify_display_property(value);
            let spec = classification.replacement_spec();
            // Parse the routable content shape, independent of anchoring:
            // rung 2 — a plain, property-less string whose chars are all
            // single-line unambiguous-width (a newline emits a row break, a
            // tab expands pen-dependently in the session's full-text-width
            // frame); rung 3 — a plain `(space :width N)` spec.
            let content: Option<(RoutedReplacementContent, usize)> = match classification
                .replacement()
            {
                Some(DisplayReplacementProperty::String) => routed_lisp_string_advance_cols(spec)
                    .and_then(|advance_cols| {
                        spec.as_utf8_str().map(|text| {
                            (
                                RoutedReplacementContent::String { text: text.into() },
                                advance_cols,
                            )
                        })
                    }),
                Some(DisplayReplacementProperty::Stretch(_)) => routed_space_width_cols(spec)
                    .map(|cols| (RoutedReplacementContent::SpaceWidth, cols)),
                _ => None,
            };
            // Hazard reasons keep their historic split: string display
            // values (and recognized space shapes) report Replacement;
            // everything else keeps HazardProp.
            let hazard_reason = if content.is_some()
                || matches!(
                    classification.replacement(),
                    Some(DisplayReplacementProperty::String)
                ) {
                RouteRefusal::Replacement
            } else {
                RouteRefusal::HazardProp
            };
            let candidate =
                content.filter(|_| probe_byte > start_byte && probe_byte < line_end_byte);
            let Some((content, advance_cols)) = candidate else {
                scan.hazards.push((probe_offset, hazard_reason));
                let Some(change) = next_change_after(probe_byte) else {
                    break;
                };
                probe_byte = change;
                continue;
            };
            // Covered extent: the producer's rule, not a second copy of it.
            // `ReplacementCoveredSpan` owns "how far does one display property
            // reach", and it takes the SOURCE so the overlay-vs-text branch
            // cannot be skipped; here the source is always a text property
            // (see `RoutedScanExtentLookup`), which is GNU's
            // `display_prop_end` — the run over which the value stays the same
            // object.
            let span = ReplacementCoveredSpan::for_property_source(
                CharPropertySource {
                    value,
                    overlay: None,
                },
                buffer.layout_emacs_byte_pos_to_char_pos(EmacsBytePos::new(probe_byte)),
                next_change_after(probe_byte)
                    .map(|change| {
                        buffer.layout_emacs_byte_pos_to_char_pos(EmacsBytePos::new(change))
                    })
                    .unwrap_or(line_end),
                &RoutedScanExtentLookup { buffer, line_end },
            );
            // The one question the shared rule does not answer: routability.
            // The covered range must end strictly BEFORE the line end, since
            // an extent reaching the newline hides it — a line-structure
            // change. Reaching the bound is not itself disqualifying: a range
            // that merely ends there while some other value holds at the line
            // end covers no newline.
            let covers_line_end = span.resume() >= line_end
                && display_prop_at(line_end_byte).is_some_and(|next| next.bits() == value.bits());
            if covers_line_end {
                scan.hazards.push((probe_offset, RouteRefusal::Replacement));
                break;
            }
            scan.replacements.push(RoutedRowReplacement {
                start: probe_offset,
                covered: span,
                value,
                content,
                advance_cols,
            });
            probe_byte = buffer
                .layout_char_pos_to_emacs_byte_pos(span.resume())
                .get();
            continue;
        }
        let Some(change) = next_change_after(probe_byte) else {
            break;
        };
        if change > line_end_byte {
            break;
        }
        probe_byte = change;
    }
    Ok(scan)
}

/// Text properties that influence acquisition or the line end beyond faces.
/// Any of these present anywhere on the line (or its newline) sends the row
/// to the buffer pipeline. Face-affecting properties (`face`,
/// `font-lock-face`, `fontified` boundaries) are NOT hazards: they only
/// segment the row and are handled by the routed face resolution. Properties
/// are constant between change positions, so probing each segment start (and
/// the newline, when a change lands on it) covers the whole row. `invisible`
/// is not on this list since phase 2d: the plain-elision sub-case is routed
/// and the inexpressible sub-cases (ellipsis, newline-spanning, row-start,
/// overlay-sourced) refuse through [`routed_row_elision_scan`]. `composition` is
/// not on this list since phase 2e: its refusal is grounded in the
/// pipeline's own replacement predicate ([`routed_composition_prop_replaces`]),
/// so an inert (unparseable) prop no longer refuses.
/// `display` is NOT probed here since increment 2i: the dedicated
/// [`routed_row_replacement_scan`] owns every display-prop decision (routable
/// string replacements become plan spans; everything else records a
/// positioned hazard the classifier applies against the routed coverage).
pub(super) const ROUTE_HAZARD_TEXT_PROPS: [&str; 2] = ["mouse-face", "line-height"];

/// Whether a static `composition` text property at `probe` would REPLACE its
/// covered chars in the pipeline. This is the same predicate the pipeline's
/// item production applies (`BufferTextSourceCursor::next_text_item_with_layout`
/// -> `composition_display_text_for_property`, the neomacs stand-in for GNU
/// `handle_composition_prop`'s `composition_valid_p` gate): a prop that
/// parses to display text composes — the row refuses; a prop the predicate
/// rejects renders its chars literally through the ordinary text run and
/// stays routable. Refusal here is deliberately extent-agnostic (the
/// pipeline additionally requires the composition to fit inside the run and
/// walk bounds); refusing the superset is always safe.
pub(super) fn routed_composition_prop_replaces<B: LayoutBufferView>(
    buffer: &B,
    probe: EmacsBytePos,
) -> bool {
    buffer
        .layout_text_prop_at_emacs_byte_pos(probe, Value::symbol("composition"))
        .is_some_and(|prop| composition_display_text_for_property(prop).is_some())
}

/// Scan the `[row_charpos, newline_charpos)` line for invisible text through
/// the SAME semantics the pipeline's invisible checkpoint consumes
/// (`RustTextPropAccess::check_invisible`: overlay value shadows the text
/// property, values judged against `buffer-invisibility-spec`, adjacent
/// hidden runs collapsed with the ENTRY run's ellipsis flag). Returns the
/// hidden CHAR-offset ranges — the expressible plain-elision class — or
/// `None`, refusing the route, when a hidden run:
/// * shows an ellipsis (the pipeline appends `...` glyphs with their own
///   face/provenance rules, GNU `setup_for_ellipsis`);

/// * starts AT the row start (the visible loop's invisible checkpoint
///   consumes it BEFORE the route attempt; the walk then resumes mid-line —
///   classifying it here keeps direct classification aligned with the
///   production ordering);

/// * covers the newline (hiding the line end joins buffer lines into one
///   display row — a line-structure change; a run ending exactly AT the
///   newline keeps it visible and is fine);

/// * fails to advance (defensive: a skip that does not move would loop).
///
/// The scan walks exactly the checkpoint cadence: probe, jump to the
/// returned `next_visible`, re-probe — the same positions the pipeline's
/// `InvisibleTextScanCheckpoint` re-checks at.
pub(super) fn routed_row_elision_scan<B: LayoutBufferView>(
    buffer: &B,
    row_charpos: i64,
    newline_charpos: i64,
) -> Option<Vec<(usize, usize)>> {
    let text_props = crate::neovm_bridge::RustTextPropAccess::new(buffer);
    let mut elided = Vec::new();
    let mut pos = row_charpos;
    // Probe through the newline INCLUSIVE: a hidden run starting at the
    // newline itself covers the line end just as one running into it does.
    while pos <= newline_charpos {
        let (status, next_visible) = text_props.check_invisible(pos);
        if status.hidden {
            if status.ellipsis
                || pos == row_charpos
                || pos >= newline_charpos
                || next_visible > newline_charpos
                || next_visible <= pos
            {
                return None;
            }
            elided.push((
                (pos - row_charpos) as usize,
                (next_visible - row_charpos) as usize,
            ));
        }
        if next_visible <= pos {
            break;
        }
        pos = next_visible;
    }
    Some(elided)
}
