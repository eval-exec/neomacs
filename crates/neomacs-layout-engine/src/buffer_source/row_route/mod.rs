//! Row acquisition routing for the buffer-source migration.
//!
//! GNU has ONE iterator (`struct it`, xdisp.c) and `next_element_from_buffer`
//! is simply a method on it; neomacs still has two row paths — the buffer
//! pipeline (this module's siblings) and the unified item renderer
//! (`display_row/`, driven by `DisplayItemSource`). This module is the seam
//! that migrates the simplest buffer row class onto the item renderer:
//!
//! * [`RowAcquisitionRoute`] + [`classify_row_acquisition`] decide, at the
//!   start of a buffer line, whether the row is plain enough for the item
//!   renderer. "Plain" mirrors what makes GNU `get_next_display_element`
//!   trivial: TAB plus printable characters the shared width table sizes at
//!   exactly 1 or 2 columns — ASCII since increment 1, and since increment 2b
//!   any printable non-ASCII char of unambiguous width, so CJK and Latin
//!   accents route while contextual-shaping scripts, regional indicators and
//!   nobreak chars refuse (see [`classify_routed_row_char`], which states the
//!   full ladder). Also required: no display/mouse-face/line-height
//!   properties in range and no display table. Direction is NOT a condition:
//!   RTL and mixed rows route, because bidi reordering is a row-level install
//!   step below this seam (`GlyphRowFinalizer::finalize` ->
//!   `reorder_row_bidi`), so both producers emit the same logical-order row
//!   and the same pure permutation makes it visual
//!   (`classifier_routes_hebrew_rtl_rows`). Either the whole line fits
//!   without
//!   continuation or truncation and ends in a real newline, or — phase 2f —
//!   the line overflows and the route covers only its maximal fitting
//!   prefix, handing the walk back to the pipeline at the first char that
//!   does not fit so the pipeline's own overflow machinery (truncation skip
//!   / continuation transition, row flags, carry-over bookkeeping) decides
//!   wrap-vs-truncate unchanged. Since P4.8 the route attempts EVERY walk
//!   position, line start or mid-line alike: a candidate classifies from
//!   its own charpos and the loop's live pen, so the continuation rows of a
//!   visually wrapped line, and the resumes after a display element, all
//!   route the same way a line start does. Composition refuses through the
//!   pipeline's OWN predicates (phase 2e): a char the shared writer would
//!   compose into the previous glyph (`composition::continues_cluster` /
//!   `continues_complex_run` over the scan's mirror of the row tail) and a
//!   static `composition` text property the pipeline's replacement
//!   predicate parses (`composition_display_text_for_property`) both keep
//!   the buffer pipeline; an inert composition prop renders literally and
//!   routes.
//!   FACE-affecting properties (`face`, `font-lock-face`, `fontified`
//!   boundaries) are allowed: they segment the row at property-change
//!   positions exactly like GNU `compute_stop_pos` bounds the iterator's
//!   text runs and `handle_face_prop` re-resolves the face at each stop.
//!   Overlays intersecting the row are allowed when they carry ONLY
//!   face-affecting properties ([`ROUTE_SAFE_OVERLAY_PROPS`]): their faces
//!   merge through the same checkpoint resolver seam (GNU
//!   `face_at_buffer_position`'s ascending-priority overlay loop) and their
//!   starts/ends segment the row like GNU `next_overlay_change` folded into
//!   `compute_stop_pos`. Overlay before/after-strings ROUTE since P4.6
//!   sub-step 3b (the increment 2i rung 4 refusal is retired). Its recorded
//!   reason — "overlay strings are INSERTIONS driven by the walk's own
//!   overlay machinery, and unlike the replacement session there is no
//!   single typed request the routed commit can drive without replicating
//!   that walk state" — stopped being true in two independent ways.
//!   Loading and GNU ordering moved OUT of the walk and into the producer
//!   (P4.6 sub-step 1), which surfaces them as one typed insertion element;

//!   and `render_produced_strings_at_text_row` takes exactly the loop state
//!   the routed commit already owns, `overlay_context` included. So the
//!   routed commit DELEGATES — the same call `render.rs`'s loop-level arm
//!   makes — and what stays refused is only what the routed row shape
//!   genuinely cannot express: an anchor at the row start (the loop's route
//!   attempt runs BEFORE the pipeline step that would emit it, so routing
//!   would drop the string), an anchor at the coverage end (the line end and
//!   the overflow handoff char belong to the pipeline's own lifecycle), a
//!   string outside the routable Lisp-string class, and any anchor on an
//!   overflow-prefix plan (the append session clips and breaks rows itself,
//!   so the scan's handoff cut is not the pipeline's overflow point — the
//!   same reason replacement rows never route as overflow prefixes).
//!   Plain-elision `invisible` text (phase 2d) is expressible: hidden spans
//!   simply drop chars, so the routed source emits visible-segment TextRuns
//!   whose charpos bookkeeping jumps the gap, exactly like the pipeline's
//!   invisible checkpoint `skip_chars_until` (GNU `handle_invisible_prop`
//!   advancing `IT_CHARPOS`). The inexpressible invisible sub-cases refuse:
//!   ellipsis (inserts `...` glyphs with their own face/provenance rules),
//!   runs covering the newline (line-structure change), row-start runs
//!   (consumed by the loop checkpoint before the route), overlay-sourced
//!   invisibility (2c allow-list). `display` replacements route since
//!   increment 2i for the narrow routable class — a plain property-less
//!   single-line string (rung 2) or a plain `(space :width N)` spec
//!   (rung 3) anchored strictly inside the line — by rendering through the
//!   pipeline's OWN replacement session at commit (typed string-index glyph
//!   provenance plus covered buffer range, string base-face policy, session
//!   walk bookkeeping); every
//!   other display shape refuses through [`routed_row_replacement_scan`].
//! * [`BufferPlainItemSource`] is the `DisplayItemSource` for such a row: it
//!   produces exactly the items `BufferTextSourceCursor` would — one plain
//!   `TextRun` per face segment (one for the whole line when no property
//!   changes in range), then the explicit-newline row break.
//!
//! Rows carrying point are deliberately excluded: cursor capture is a
//! documented buffer-pipeline responsibility (see the cursor-capture note in
//! `row_lifecycle.rs`), mirroring GNU `set_cursor_from_row` operating on
//! buffer positions only.

use crate::buffer_source::producer::frame::{
    DisplayReplacementExtentLookup, ReplacementCoveredSpan,
};

use crate::composition::{continues_cluster, continues_complex_run, needs_complex_shaping};

use crate::display_item::{
    DisplayItem, DisplayItemKind, DisplayItemLayout, DisplayLineHeightPolicy, DisplayRowBreak,
    DisplaySourcePosition, DisplayTextRun, RenderFaceRef, SourceSpan,
};

use crate::display_origin::DisplayOrigin;

use crate::display_row::builder::{DisplayRowPosition, DisplayTabPolicy};

use crate::display_row::face_state::stable_face_id_for_resolved;

use crate::display_source::{
    TextSourceCharClassification, classify_text_source_char, nonascii_hyphen_p, nonascii_space_p,
};

use crate::frame_face_arena::FrameFaceAttempt;

use crate::neovm_bridge::{
    CharPropertySource, FaceResolver, LayoutBufferView, LayoutCharPropertyLookup,
    OrderedFaceSources, OverlayDisplayString, ResolvedFace,
};

use crate::types::LineWrapMode;

use crate::unicode::{decode_utf8, is_regional_indicator};

use neomacs_display_protocol::types::FaceId;

use neovm_core::buffer::{BufferId, CharLen, CharPos0, EmacsBytePos, EmacsByteRange};

use neovm_core::emacs_core::Value;

use neovm_core::emacs_core::composite::composition_display_text_for_property;

mod execution;
mod item_source;
mod planning;
mod scan;
mod telemetry;

pub(crate) use self::execution::*;
pub(crate) use self::item_source::*;
pub(crate) use self::planning::*;
pub(crate) use self::scan::*;
pub(crate) use self::telemetry::*;

/// Which pipeline acquires and renders a buffer row.
///
/// This is the classifier's verdict named as a value, and it exists for the
/// tests: production never asks the yes/no question, it asks
/// [`plan_plain_row_classified`] for the plan or the refusal and acts on
/// whichever it gets. Gated to tests so it cannot drift back into a decision
/// production makes twice.
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowAcquisitionRoute {
    /// The full buffer pipeline (`loop_render` / `item_render` orchestration).
    BufferPipeline,
    /// The unified item renderer, fed by [`BufferPlainItemSource`].
    ItemRenderer,
}

#[cfg(test)]
impl RowAcquisitionRoute {
    /// The verdict a classifier result stands for.
    pub(crate) fn of(plan: &Result<PlainRowPlan, RouteRefusal>) -> Self {
        if plan.is_ok() {
            Self::ItemRenderer
        } else {
            Self::BufferPipeline
        }
    }
}

/// Per-window facts that disqualify the item-renderer route regardless of row
/// content. Each active feature has buffer-pipeline bookkeeping (hscroll skip,
/// selective display, word-wrap candidates, trailing-whitespace tracking) that
/// the routed render deliberately does not replicate.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RowRouteWindowPolicy {
    pub(crate) point_charpos: i64,
    pub(crate) hscroll_active: bool,
    pub(crate) selective_display: i32,
    pub(crate) word_wrap: bool,
    pub(crate) show_trailing_whitespace: bool,
    /// The window's effective wrap mode (GNU `it->line_wrap`, minus
    /// WORD_WRAP which the `word_wrap` flag above refuses outright). It does
    /// not change WHAT the route renders — an over-wide line always routes
    /// only its fitting prefix and hands the walk back BEFORE the first char
    /// that does not fit, so the pipeline's own truncation/continuation
    /// machinery makes the wrap-vs-truncate decision — but it labels the
    /// routed class for engagement accounting.
    pub(crate) wrap_mode: LineWrapMode,
    /// The window overlay strings are collected FOR (GNU's `window` overlay-
    /// property filter), or `None` when this row renders none at all. Taken
    /// from the loop's own `BufferOverlayStringTextRowRenderContext`, so the
    /// classifier and the commit's session agree by construction about which
    /// overlays apply.
    pub(crate) overlay_string_window: Option<u64>,
}

/// Why a candidate row stayed on the buffer pipeline. Every refusal point in
/// the classifier and the render probe maps to exactly one variant; the
/// route-coverage telemetry ([`route_stats_report_line`]) histograms them so
/// real workloads can show WHICH refusal dominates (the input that ranks the
/// next migration increment).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RouteRefusal {
    /// Window policy: horizontal scroll active.
    PolicyHscroll,
    /// Window policy: selective display active.
    PolicySelectiveDisplay,
    /// Window policy: word wrap enabled.
    PolicyWordWrap,
    /// Window policy: trailing-whitespace highlight enabled.
    PolicyTrailingWhitespace,
    /// The line's FIRST char already crosses the right edge (no routable
    /// fitting prefix exists).
    ScanNoFitFirstChar,
    /// The line exactly fills the row (the line-end/continuation edge stays
    /// on the buffer pipeline).
    ScanExactFill,
    /// A char outside the routable ladder (control/glyphless/shaped-script/
    /// nobreak/odd-width chars, malformed UTF-8).
    ScanChar,
    /// A composing char outside the routable composite class (joiners,
    /// extenders on wide/tab/row-start tails, shaped runs).
    ScanCompose,
    /// Defensive: the source text ended with ZERO scanned chars (an empty
    /// end-of-source tail — unreachable from the visible loop, which never
    /// attempts a row at `byte_idx == text.len()`). Since phase 2h the
    /// newline-less tail line itself ROUTES ([`RoutedRowLineEnd::EndOfSource`]);
    /// the bare-newline empty line routes RowBreak-only.
    ScanEob,
    /// Point sits inside the routed coverage (cursor capture stays on the
    /// buffer pipeline).
    PointInRow,
    /// An intersecting overlay carries a property outside the face-only
    /// allow-list (or its plist/boundaries are unmappable).
    Overlay,
    /// The buffer has an active display table.
    DisplayTable,
    /// Invisible text outside the plain-elision class (ellipsis,
    /// newline-spanning, row-start, non-advancing).
    Elision,
    /// An overflow-prefix plan intersected an elided span.
    OverflowElision,
    /// A hazard text property (display/mouse-face/line-height, or a
    /// replacing composition) in range.
    HazardProp,
    /// A `display` replacement outside the routed class (increment 2i):
    /// row-start anchor, newline/tab/props in the string, empty string,
    /// covered range reaching the newline, fit overflow, or a combination
    /// with elision/overflow the plan refuses conservatively.
    Replacement,
    /// A property-change boundary failed to convert to a row char offset.
    Boundary,
    /// A visible composed extender sits on a face-segment or elision seam.
    ComposedSeam,
    /// Probe: a multi-face row segment carries a box face.
    ProbeBoxFace,
    /// Probe: the per-run face chain diverges from the checkpoint chain.
    ProbeFaceDiverges,
    /// Probe: natural measurement refused or the measured end missed the
    /// classifier's fit.
    ProbeMeasure,
}

/// Walk positions the route has already PROVEN unroutable, so the classifier
/// is not re-run per position inside a line it has refused.
///
/// P4.8(a) made the route attempt EVERY walk position, which exposed how
/// often the walk re-classifies inside one line: 36429 of 77128 production
/// attempts are a repeat at a later position on a line an earlier position
/// already refused. Most of that repetition cannot simply be dropped — a
/// refusal is generally a fact about a POSITION in the line (a hazard
/// property, an unroutable char), and the walk positions past it route: 2920
/// rows in the corpus route after an earlier box-face refusal on their own
/// line. Only a refusal whose justification provably holds for a whole RANGE
/// of start positions may be recorded here, and it must name that range.
///
/// Today one refusal qualifies: [`RouteRefusal::PointInRow`] taken at the
/// pre-gate, which is a pure line/point test with no fit walk in it. Point on
/// this line at or after the start position means every start position from
/// here through point sits on the same line with point at or after it, so
/// each refuses identically. Positions PAST point are left to attempt (1703
/// corpus rows route there), which is why the window carries an end and not
/// just "the rest of the line".
#[derive(Debug, Default)]
pub(crate) struct RouteRefusalWindow {
    /// Inclusive absolute charpos range proven unroutable.
    refused: Option<(i64, i64)>,
}

impl RouteRefusalWindow {
    /// Whether `charpos` is inside a range an earlier attempt proved
    /// unroutable.
    pub(crate) fn covers(&self, charpos: i64) -> bool {
        self.refused
            .is_some_and(|(from, through)| charpos >= from && charpos <= through)
    }

    /// Record that every start position in `from..=through` refuses. Later
    /// records replace earlier ones: the walk moves forward, so an older
    /// window can only describe ground already covered.
    pub(crate) fn refuse_through(&mut self, from: i64, through: i64) {
        if through >= from {
            self.refused = Some((from, through));
        }
    }
}

/// The buffer-walk position at the start of a candidate row.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RowRouteRowStart<'a> {
    /// The visible buffer text the walk iterates (starts at `text_start_byte`).
    pub(crate) text: &'a [u8],
    /// Byte index of the row start within `text`.
    pub(crate) byte_idx: usize,
    /// 0-based char position of the row start.
    pub(crate) charpos: i64,
    /// Emacs byte position of `text[0]`.
    pub(crate) text_start_byte: usize,
}

/// Pixel-fit inputs. A whole-line plan must hold the line WITHOUT
/// continuation or truncation, applied strictly (a line exactly filling the
/// row is NOT eligible — its line end interacts with continuation policy);

/// an overflow-prefix plan (phase 2f) covers the maximal fitting prefix of
/// an over-wide line instead. Either way the routed render re-verifies with
/// the same natural measurement the buffer pipeline uses before committing.
/// The tab
/// policy is the append surface's (buffer `tab-width` / `tab-stop-list`), so
/// the classifier's tab expansion is the SAME `DisplayTabPolicy::advance_from`
/// the pipeline's per-char advance resolves (GNU `gui_produce_glyphs`
/// `next_tab_x`). `start_position` carries both the live screen-row pen and
/// the typed physical-line TAB coordinate space; keeping them atomic prevents
/// a continuation route from silently reconstructing a row-local TAB origin.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RowRouteFit<'a> {
    pub(crate) start_position: DisplayRowPosition,
    pub(crate) char_width_px: f32,
    pub(crate) right_edge_px: f32,
    pub(crate) tab_policy: &'a DisplayTabPolicy,
}

/// A classified plain row: `line_char_len` routable chars (`line_byte_len`
/// bytes — the row may be multibyte) followed by a real newline.
/// `face_boundaries` are the CHAR offsets strictly inside the line where text
/// properties change — each starts a new face segment, the neomacs mirror of
/// GNU `compute_stop_pos` stops re-resolved by `handle_face_prop`. Empty for
/// a property-constant line. `elided` are the CHAR-offset `[start, end)`
/// ranges hidden by plain (no-ellipsis) `invisible` text properties, in
/// ascending disjoint order — the routed render skips them entirely, exactly
/// as the pipeline's invisible checkpoint `skip_chars_until` does (GNU
/// `handle_invisible_prop` advancing `IT_CHARPOS` past the run). `composed`
/// are the CHAR offsets of zero-width extenders the shared writer merges
/// into their preceding base glyph (phase 2e rung 2) — they occupy no
/// column and produce no glyph of their own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlainRowPlan {
    line_byte_len: usize,
    line_char_len: usize,
    has_tab: bool,
    has_wide: bool,
    has_overlay: bool,
    face_boundaries: Vec<usize>,
    elided: Vec<(usize, usize)>,
    composed: Vec<usize>,
    replacements: Vec<RoutedRowReplacement>,
    /// The overlay-string anchors inside the routed coverage, ascending. Each
    /// is an INSERTION: it consumes no chars, so it is not a gap in
    /// [`PlainRowPlan::segment_ranges`] the way a replacement is.
    overlay_strings: Vec<RoutedRowOverlayStrings>,
    line_end: RoutedRowLineEnd,
}

/// A routed `display` replacement (increment 2i rung 2): the covered CHAR
/// range `[start, end)` of the line renders as the display value through the
/// pipeline's OWN replacement session (`display_property_render.rs` ->
/// `replacement.rs`). String text glyphs carry their GNU string indices plus
/// the exact covered buffer range; non-string display specs retain buffer
/// provenance. The routed class accepts only single-line property-less strings
/// whose chars have unambiguous column widths, so `advance_cols` is the exact
/// logical-cell advance the classifier's fit walk credits in place of the
/// covered chars.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RoutedRowReplacement {
    /// CHAR offset of the covered range start within the line.
    start: usize,
    /// The covered range in absolute positions, as the producer's rule
    /// derived it. The plan carries the span itself rather than a second end
    /// offset so the commit hands the session the range the SCAN resolved,
    /// with no chance of a third party re-deriving E.
    covered: ReplacementCoveredSpan,
    /// The full `display` property value (what the pipeline's walk consumes).
    value: Value,
    /// What the routed class recognized inside the display value.
    content: RoutedReplacementContent,
    /// The replacement's logical-cell width for the classifier's fit walk.
    advance_cols: usize,
}

impl RoutedRowReplacement {
    /// CHAR offset of the covered range end (exclusive) within the line.
    fn end(&self) -> usize {
        self.start + self.covered.covered_char_len()
    }
}

/// The routable display-replacement content kinds (increment 2i).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RoutedReplacementContent {
    /// Rung 2: a plain, property-less, single-line string.
    String { text: Box<str> },
    /// Rung 3: a plain `(space :width N)` spec, N a positive fixnum — one
    /// stretch glyph of N columns with covered-buffer provenance (GNU
    /// stamps the covered buffer position on stretch glyphs; xdisp.c
    /// handle_single_display_spec 6604 + append_stretch_glyph 32684).
    SpaceWidth,
}

/// How a routed row's coverage ends (phase 2f).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RoutedRowLineEnd {
    /// The line's real newline: the plan covers the whole line, strictly
    /// fitting inside the row; the buffer pipeline's line-break lifecycle
    /// consumes the newline afterwards.
    Newline,
    /// The line overflows the row (GNU `display_line`'s "glyph doesn't fit"
    /// branch, xdisp.c:26221): the plan covers only the MAXIMAL FITTING
    /// PREFIX — every covered char satisfies the pipeline's own fit rule
    /// (`DisplayRowTextOverflowDecision::for_char`: `x + advance <=
    /// right_edge`) — and the routed render hands the walk back to the
    /// buffer pipeline AT the first char that does not fit, BEFORE any
    /// wrap-vs-truncate decision. The pipeline's own overflow machinery
    /// (`overflow.rs` truncation skip / continuation transition, row flags,
    /// continuation rows, fringe indicators) then runs unchanged, which is
    /// what keeps the multi-row carry-over bookkeeping byte-identical.
    OverflowHandoff,
    /// Phase 2h rung 2: the line ends AT the end of the source text with no
    /// newline. The window read bound always cuts AFTER a complete line's
    /// newline (`find_nth_newline_after` returns newline+1) or at the
    /// accessible end, so a newline-less tail line is never a mid-line
    /// artifact of the bound — it is the buffer's (or narrowed region's)
    /// last line, GNU's `IT_EOB` exit (xdisp.c:26007, `row->ends_at_zv_p`).
    /// The plan covers the whole tail; the routed render leaves the walk at
    /// the source end and the visible loop exits, after which the pipeline's
    /// post-loop end-of-buffer machinery (EOB cursor/tail request, appended
    /// space, `ends_at_zv` marking in `finish_pending_text_window_row`, the
    /// trailing ZV placeholder row) runs unchanged on both modes. GNU has NO
    /// analogue of a bounded read (its iterator is lazy and stops only on
    /// pixels or ZV), so the faithful semantics here are the pipeline's own:
    /// route only WHO renders the tail's text, never the row's EOB
    /// finalization.
    EndOfSource,
}

impl PlainRowPlan {
    pub(crate) fn line_byte_len(&self) -> usize {
        self.line_byte_len
    }

    pub(crate) fn line_char_len(&self) -> usize {
        self.line_char_len
    }

    #[cfg(test)]
    pub(crate) fn has_tab(&self) -> bool {
        self.has_tab
    }

    #[cfg(test)]
    pub(crate) fn has_wide(&self) -> bool {
        self.has_wide
    }

    #[cfg(test)]
    pub(crate) fn face_boundaries(&self) -> &[usize] {
        &self.face_boundaries
    }

    #[cfg(test)]
    pub(crate) fn elided(&self) -> &[(usize, usize)] {
        &self.elided
    }

    /// Whether the row elides invisible spans.
    pub(crate) fn has_elision(&self) -> bool {
        !self.elided.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn composed(&self) -> &[usize] {
        &self.composed
    }

    /// Whether the row contains a composed grapheme cluster (a zero-width
    /// extender merged into its base glyph).
    pub(crate) fn has_composed(&self) -> bool {
        !self.composed.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn line_end(&self) -> RoutedRowLineEnd {
        self.line_end
    }

    /// Whether this plan covers only the fitting prefix of an over-wide line
    /// and hands the walk back to the pipeline at the first non-fitting char.
    pub(crate) fn is_overflow_handoff(&self) -> bool {
        self.line_end == RoutedRowLineEnd::OverflowHandoff
    }

    /// Phase 2h rung 1: a bare-newline empty line — zero covered chars, the
    /// production is RowBreak-only (the shared line-end plan consumes the
    /// newline).
    pub(crate) fn is_empty_line(&self) -> bool {
        self.line_char_len == 0
    }

    /// Phase 2h rung 2: the newline-less tail line ending at the source end.
    pub(crate) fn is_end_of_source(&self) -> bool {
        self.line_end == RoutedRowLineEnd::EndOfSource
    }

    /// Whether the row contains a routed `display` replacement.
    pub(crate) fn has_replacement(&self) -> bool {
        !self.replacements.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn replacement_ranges(&self) -> Vec<(usize, usize)> {
        self.replacements
            .iter()
            .map(|replacement| (replacement.start, replacement.end()))
            .collect()
    }

    pub(crate) fn replacements(&self) -> &[RoutedRowReplacement] {
        &self.replacements
    }

    /// Whether the row carries an overlay-string anchor.
    pub(crate) fn has_overlay_strings(&self) -> bool {
        !self.overlay_strings.is_empty()
    }

    pub(crate) fn overlay_strings(&self) -> &[RoutedRowOverlayStrings] {
        &self.overlay_strings
    }

    /// Whether the row renders as more than one run (face segments, elision
    /// gaps, or replacement spans splitting the line).
    pub(crate) fn is_segmented(&self) -> bool {
        !self.face_boundaries.is_empty() || !self.elided.is_empty() || !self.replacements.is_empty()
    }

    /// The `[start, end)` char ranges of the row's VISIBLE face segments, in
    /// row order: the line minus the elided spans, split at each face
    /// boundary that falls strictly inside a visible stretch (boundaries at
    /// an elided edge coincide with the gap and split nothing; boundaries
    /// inside a hidden span never render). A property-constant fully-visible
    /// line yields one range covering the line.
    pub(crate) fn segment_ranges(&self, start: CharPos0) -> Vec<(CharPos0, CharPos0)> {
        // Gaps the text segments skip: elided spans and replacement-covered
        // spans (mutually exclusive by the classifier's composition refusal;
        // each list is ascending and disjoint, so a simple merge sorts them).
        let mut gaps: Vec<(usize, usize)> =
            Vec::with_capacity(self.elided.len() + self.replacements.len());
        gaps.extend(self.elided.iter().copied());
        gaps.extend(
            self.replacements
                .iter()
                .map(|replacement| (replacement.start, replacement.end())),
        );
        gaps.sort_unstable();
        let mut visible: Vec<(usize, usize)> = Vec::with_capacity(gaps.len() + 1);
        let mut cursor = 0usize;
        for &(hidden_start, hidden_end) in &gaps {
            if hidden_start > cursor {
                visible.push((cursor, hidden_start));
            }
            cursor = cursor.max(hidden_end);
        }
        if cursor < self.line_char_len {
            visible.push((cursor, self.line_char_len));
        }

        let mut ranges = Vec::with_capacity(visible.len() + self.face_boundaries.len());
        for (visible_start, visible_end) in visible {
            let mut seg_start = visible_start;
            for &boundary in &self.face_boundaries {
                if boundary > seg_start && boundary < visible_end {
                    ranges.push((
                        start.add_len(CharLen::new(seg_start)),
                        start.add_len(CharLen::new(boundary)),
                    ));
                    seg_start = boundary;
                }
            }
            ranges.push((
                start.add_len(CharLen::new(seg_start)),
                start.add_len(CharLen::new(visible_end)),
            ));
        }
        ranges
    }
}

#[cfg(test)]
#[path = "row_route_test.rs"]
mod tests;
