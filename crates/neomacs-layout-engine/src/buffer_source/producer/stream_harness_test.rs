//! Phase-4 stream-equivalence harness: the shadow spine of the producer
//! inversion.
//!
//! For each corpus case the harness drives TWO legs over the same buffer and
//! asserts they produce the identical element stream on the assertion surface
//! from tmp/p4-test-inventory.md section 1 (char, provenance, scan before and
//! after, face-ref, class):
//!
//! * the PRODUCER leg — [`BufferElementProducer`] consumption straight through,
//!   runs yielded whole (as rungs land this becomes `next_element`);
//! * the PIPELINE-REPLAY leg — the same producer with the renderer's split
//!   feeders applied, using the REAL splitters
//!   (`DisplaySourceStepItem::split_text_run_items`, the per-char feeder at
//!   char_render.rs:111-117, and `split_text_run_at_charpos`, the fit split at
//!   item_render.rs:371-389) pushed back through the pending queue exactly as
//!   the renderer does.
//!
//! Runs are expanded to per-character observations before comparison, so the
//! legs agree only if splitting is STREAM-TRANSPARENT: same characters, same
//! provenance, same scan positions, same faces, same order. That is precisely
//! the invariant P4.3-P4.5 must preserve while they delete the feeders, which
//! is why this file lands before them.
//!
//! WHAT THE PRODUCER OWNS AT THIS RUNG — measured with a probe, not assumed:
//! it yields whole text runs, row breaks and replacement items with buffer
//! provenance, and it DOES terminate runs at a resolvable face boundary. It
//! does NOT elide invisible text, expand tabs, or emit truncation marks; those
//! are renderer-owned today (the invisible checkpoint at row_lifecycle.rs:694+,
//! `DisplayTabPolicy::advance_from`, special_glyphs.rs). Corpus cases whose
//! inventory expectation depends on
//! producer-owned stop state are landed `#[ignore]` naming the rung that will
//! un-ignore them, so the checklist lives in the code rather than a document.

use super::vocabulary::{
    BufferScanPos, ProducedElement, ProducedGlyphProvenance as GlyphProvenance,
};
use super::*;
use crate::buffer_source::consumption::BufferSourceConsumedItem;
use crate::display_item::RenderFaceRef;
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::display_source::DisplaySourceStepItem;
use crate::display_source::DisplaySourceTextPosition;
use crate::display_source_resolver::DisplaySourceFaceBasis;
use crate::frame_face_arena::FrameFaceAttempt;
use crate::neovm_bridge::{FaceResolver, LayoutBufferSnapshot, OverlayDisplayString, ResolvedFace};
use neomacs_display_protocol::types::FaceId;
use neovm_core::buffer::{BufferId, CharPos0, EmacsByteRange};
use neovm_core::emacs_core::{Context, Value};
use neovm_core::face::FaceTable;
use neovm_core::heap_types::OverlayDataInit;

const BASE_FACE: FaceId = FaceId::new(1);

/// The window the harness lays out for. Overlays carrying a `window` property
/// apply only in their own window (GNU `overlay_applies_to_window`), so the
/// corpus can state both "scoped to this window" and "scoped to another one".
const HARNESS_WINDOW_ID: u64 = 7;

/// Enough elements to drain every corpus case to end of text.
const DRAIN_LIMIT: usize = 128;

// ---------------------------------------------------------------------------
// Corpus rows
// ---------------------------------------------------------------------------

/// One text property applied to a half-open character range of a corpus
/// buffer. The Lisp value is built inside the fixture's `Context`, not here:
/// constructing a list allocates on the thread's Lisp heap, which only exists
/// while a `Context` does.
struct CaseProperty {
    start_char: usize,
    end_char: usize,
    kind: CasePropertyKind,
}

enum CasePropertyKind {
    /// An ANONYMOUS face (an attribute plist), not a named one: a named face
    /// only resolves when the face table happens to define it, which made this
    /// corpus depend on which Lisp state the build had dumped. A plist always
    /// resolves, so the seam is deterministic.
    Face {
        attribute: &'static str,
        value: &'static str,
    },
    Invisible,
}

impl CaseProperty {
    fn face(
        start_char: usize,
        end_char: usize,
        attribute: &'static str,
        value: &'static str,
    ) -> Self {
        Self {
            start_char,
            end_char,
            kind: CasePropertyKind::Face { attribute, value },
        }
    }

    fn invisible(start_char: usize, end_char: usize) -> Self {
        Self {
            start_char,
            end_char,
            kind: CasePropertyKind::Invisible,
        }
    }

    fn name(&self) -> &'static str {
        match self.kind {
            CasePropertyKind::Face { .. } => "face",
            CasePropertyKind::Invisible => "invisible",
        }
    }

    fn value(&self) -> Value {
        match self.kind {
            CasePropertyKind::Face { attribute, value } => {
                Value::list(vec![Value::symbol(attribute), Value::symbol(value)])
            }
            CasePropertyKind::Invisible => Value::t(),
        }
    }
}

/// An overlay applied to a half-open character range of a corpus buffer.
/// Overlays are a SECOND seam source next to text properties: GNU folds
/// `next_overlay_change` into `compute_stop_pos` (xdisp.c:4356-4365), and
/// overlay before/after-strings anchor exactly at an overlay's start and end.
///
/// One overlay carries a BAG of properties rather than a single kind, because
/// the ordering rules the O-pins state are about combinations: an overlay with
/// both strings (O2), a string plus the overlay's own face (O5), a string plus
/// `invisible` (O7), a string plus a `priority` (O3/O4).
struct CaseOverlay {
    start_char: usize,
    end_char: usize,
    before_string: Option<CaseOverlayString>,
    after_string: Option<CaseOverlayString>,
    face: Option<(&'static str, &'static str)>,
    /// An `invisible` property on the overlay itself, which makes GNU emit
    /// BOTH strings at BOTH endpoints (xdisp.c:7157-7173).
    invisible: bool,
    priority: Option<CaseOverlayPriority>,
    /// The overlay's `window` property. `Some(id)` scopes it to that window
    /// (GNU `overlay_applies_to_window`), so an overlay belonging to another
    /// window must be invisible to this harness's producer.
    window: Option<u64>,
}

/// The value of a `before-string` / `after-string` property. GNU's collection
/// guards are `STRINGP (str) && SCHARS (str)`, so a non-string and an empty
/// string are both dropped — and both must be constructible here to pin it.
enum CaseOverlayString {
    Text(&'static str),
    NonString(i64),
}

/// GNU reads overlay-string priority as a PLAIN fixnum
/// (`FIXNUMP (priority) ? XFIXNUM (priority) : 0`, xdisp.c:7132-7134); the
/// `(PRIORITY . SPRIORITY)` cons form that face merging understands
/// (buffer.c:3840-3858) degrades to 0 here.
enum CaseOverlayPriority {
    Plain(i64),
    Cons(i64, i64),
}

impl CaseOverlay {
    fn at(start_char: usize, end_char: usize) -> Self {
        Self {
            start_char,
            end_char,
            before_string: None,
            after_string: None,
            face: None,
            invisible: false,
            priority: None,
            window: None,
        }
    }

    fn face(
        start_char: usize,
        end_char: usize,
        attribute: &'static str,
        value: &'static str,
    ) -> Self {
        Self::at(start_char, end_char).with_face(attribute, value)
    }

    fn before_string(start_char: usize, end_char: usize, text: &'static str) -> Self {
        Self::at(start_char, end_char).with_before_string(text)
    }

    fn after_string(start_char: usize, end_char: usize, text: &'static str) -> Self {
        Self::at(start_char, end_char).with_after_string(text)
    }

    fn with_before_string(mut self, text: &'static str) -> Self {
        self.before_string = Some(CaseOverlayString::Text(text));
        self
    }

    fn with_after_string(mut self, text: &'static str) -> Self {
        self.after_string = Some(CaseOverlayString::Text(text));
        self
    }

    fn with_non_string_before_string(mut self, value: i64) -> Self {
        self.before_string = Some(CaseOverlayString::NonString(value));
        self
    }

    fn with_face(mut self, attribute: &'static str, value: &'static str) -> Self {
        self.face = Some((attribute, value));
        self
    }

    fn invisible(mut self) -> Self {
        self.invisible = true;
        self
    }

    fn with_priority(mut self, priority: i64) -> Self {
        self.priority = Some(CaseOverlayPriority::Plain(priority));
        self
    }

    fn with_cons_priority(mut self, priority: i64, secondary: i64) -> Self {
        self.priority = Some(CaseOverlayPriority::Cons(priority, secondary));
        self
    }

    fn in_window(mut self, window_id: u64) -> Self {
        self.window = Some(window_id);
        self
    }

    /// The overlay's property plist as (name, value) pairs. Values are built
    /// here so they allocate inside the fixture's `Context`.
    fn properties(&self) -> Vec<(&'static str, Value)> {
        let mut properties = Vec::new();
        if let Some((attribute, value)) = self.face {
            properties.push((
                "face",
                Value::list(vec![Value::symbol(attribute), Value::symbol(value)]),
            ));
        }
        for (name, string) in [
            ("before-string", self.before_string.as_ref()),
            ("after-string", self.after_string.as_ref()),
        ] {
            match string {
                Some(CaseOverlayString::Text(text)) => {
                    properties.push((name, Value::string(*text)))
                }
                Some(CaseOverlayString::NonString(value)) => {
                    properties.push((name, Value::fixnum(*value)))
                }
                None => {}
            }
        }
        if self.invisible {
            properties.push(("invisible", Value::t()));
        }
        match self.priority {
            Some(CaseOverlayPriority::Plain(priority)) => {
                properties.push(("priority", Value::fixnum(priority)))
            }
            Some(CaseOverlayPriority::Cons(priority, secondary)) => properties.push((
                "priority",
                Value::cons(Value::fixnum(priority), Value::fixnum(secondary)),
            )),
            None => {}
        }
        if let Some(window_id) = self.window {
            properties.push(("window", Value::make_window(window_id)));
        }
        properties
    }
}

/// A corpus row: buffer content plus the properties that create its seam.
struct StreamCase {
    name: &'static str,
    text: &'static str,
    properties: Vec<CaseProperty>,
    overlays: Vec<CaseOverlay>,
}

impl StreamCase {
    fn new(name: &'static str, text: &'static str) -> Self {
        Self {
            name,
            text,
            properties: Vec::new(),
            overlays: Vec::new(),
        }
    }

    fn with(mut self, property: CaseProperty) -> Self {
        self.properties.push(property);
        self
    }

    fn with_overlay(mut self, overlay: CaseOverlay) -> Self {
        self.overlays.push(overlay);
        self
    }

    /// A face property per character: the C3 shape that forces the pipeline to
    /// render a line character by character.
    fn per_char_faces(mut self, faces: &[(&'static str, &'static str)]) -> Self {
        for (index, (attribute, value)) in faces.iter().enumerate() {
            self.properties
                .push(CaseProperty::face(index, index + 1, attribute, value));
        }
        self
    }
}

// ---------------------------------------------------------------------------
// The assertion surface
// ---------------------------------------------------------------------------

/// The element classes the assertion surface distinguishes. Classes the
/// producer cannot yet compute (Wide, ComposedExtender) arrive with the rung
/// that gives `ProducedChar` its char class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ElementClass {
    PlainChar,
    Tab,
    RowBreak,
    Stretch,
    Replacement,
    /// The overlay strings anchored at a position (P4.6): produced with
    /// INSERTION semantics, so the scan track does not advance across it.
    OverlayStrings,
}

/// One character's worth of the element stream. A run contributes one of these
/// per character, so a split run and a whole run are directly comparable — the
/// property the whole harness rests on.
#[derive(Clone, Debug, PartialEq)]
struct CharObservation {
    class: ElementClass,
    ch: Option<char>,
    provenance: GlyphProvenance,
    scan_before: BufferScanPos,
    scan_after: BufferScanPos,
    face: RenderFaceRef,
}

impl CharObservation {
    fn chars(stream: &[Self]) -> String {
        stream
            .iter()
            .filter_map(|observation| observation.ch)
            .collect()
    }
}

fn char_class(ch: char) -> ElementClass {
    if ch == '\t' {
        ElementClass::Tab
    } else {
        ElementClass::PlainChar
    }
}

/// Expand one consumed item into per-character observations.
fn observe(item: &BufferSourceConsumedItem, scan_before: BufferScanPos) -> Vec<CharObservation> {
    if let BufferSourceConsumedItem::OverlayStrings(strings) = item {
        return vec![CharObservation {
            class: ElementClass::OverlayStrings,
            ch: None,
            provenance: GlyphProvenance::buffer(strings.anchor_charpos().get()),
            scan_before,
            scan_after: scan_before,
            face: RenderFaceRef::Inherit,
        }];
    }
    let BufferSourceConsumedItem::Renderable(step) = item else {
        // A display-property replacement is one opaque element at this rung;
        // decomposing its covered range is P4.7's business.
        return vec![CharObservation {
            class: ElementClass::Replacement,
            ch: None,
            provenance: GlyphProvenance::buffer(scan_before.charpos().max(0) as usize),
            scan_before,
            scan_after: scan_before,
            face: RenderFaceRef::Inherit,
        }];
    };
    let Some(element) = ProducedElement::from_step_item(step) else {
        // Kinds the P4.1 vocabulary does not model yet (glyphless, media).
        return Vec::new();
    };
    match &element {
        ProducedElement::Run(run) => {
            let mut observations = Vec::new();
            let mut scan = scan_before;
            for (offset, ch) in run.text().chars().enumerate() {
                let scan_after = BufferScanPos::new(
                    scan.byte_idx() + ch.len_utf8(),
                    scan.charpos().saturating_add(1),
                );
                observations.push(CharObservation {
                    class: char_class(ch),
                    ch: Some(ch),
                    provenance: run.glyph_provenance(offset),
                    scan_before: scan,
                    scan_after,
                    face: run.face(),
                });
                scan = scan_after;
            }
            observations
        }
        ProducedElement::Char(produced) => vec![CharObservation {
            class: char_class(produced.ch()),
            ch: Some(produced.ch()),
            provenance: produced.position().stamp(),
            scan_before,
            scan_after: BufferScanPos::new(
                scan_before.byte_idx() + produced.ch().len_utf8(),
                scan_before.charpos().saturating_add(1),
            ),
            face: produced.face(),
        }],
        ProducedElement::RowBreak(row_break) => vec![CharObservation {
            class: ElementClass::RowBreak,
            ch: Some('\n'),
            provenance: row_break.position().stamp(),
            scan_before,
            scan_after: BufferScanPos::new(
                scan_before.byte_idx() + 1,
                scan_before.charpos().saturating_add(1),
            ),
            face: RenderFaceRef::Inherit,
        }],
        ProducedElement::Stretch(stretch) => vec![CharObservation {
            class: ElementClass::Stretch,
            ch: None,
            provenance: stretch.position().stamp(),
            scan_before,
            scan_after: scan_before,
            face: stretch.face(),
        }],
        ProducedElement::EndOfText => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// The two legs
// ---------------------------------------------------------------------------

/// How a leg splits what the producer hands it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SplitPolicy {
    /// Producer leg: whole runs, no feeders.
    None,
    /// P4.5's replacement for the per-char feeder: the renderer DECLINES run
    /// batching from `charpos` on, and the producer yields single characters
    /// itself. Nothing is queued.
    CharGranularityFrom(i64),
    /// The per-char feeder (char_render.rs:111-117): every multi-char run is
    /// split, the first character is consumed now and the remainder goes back
    /// on the pending queue.
    PerChar,
    /// The fit split as it was before P4.3: a run crossing `charpos` is cut
    /// there and the tail is queued for a later iteration to pop.
    FitAt(i64),
    /// P4.3's replacement: cut the run at `charpos`, consume only the prefix,
    /// and reseat the producer there instead of queueing the tail.
    PrefixAt(i64),
}

/// Buffer plus face machinery for one corpus case. Owns its `FaceResolver`
/// (which copies the face table), so a driver can borrow it for a whole run.
struct Fixture {
    /// The evaluator is kept ALIVE for the fixture's whole lifetime: every Lisp
    /// value the corpus builds (face plists, overlay plists, overlay strings)
    /// lives on this context's heap, and an overlay's property plist is read
    /// back through its heap object at layout time. Dropping the context after
    /// snapshotting left those reads returning nothing, so a face-carrying
    /// overlay silently contributed no face.
    _eval: Context,
    buffer_id: BufferId,
    snapshot: LayoutBufferSnapshot,
    resolver: FaceResolver,
    base_face: ResolvedFace,
}

impl Fixture {
    fn new(case: &StreamCase) -> Self {
        let mut eval = Context::new();
        let buffer_id = eval
            .buffer_manager()
            .current_buffer()
            .expect("current buffer")
            .id();
        {
            let buffer = eval
                .buffer_manager_mut()
                .get_mut(buffer_id)
                .expect("buffer");
            buffer.insert(case.text);
            for property in &case.properties {
                let start =
                    buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(property.start_char));
                let end =
                    buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(property.end_char));
                buffer.text_props_put_property_in_emacs_byte_range(
                    EmacsByteRange::new(start, end),
                    Value::symbol(property.name()),
                    property.value(),
                );
            }
            for overlay in &case.overlays {
                let start =
                    buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(overlay.start_char));
                let end =
                    buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(overlay.end_char));
                let value = Value::make_overlay(OverlayDataInit {
                    serial: 0,
                    plist: Value::NIL,
                    buffer: Some(buffer_id),
                    start: start.get(),
                    end: end.get(),
                    front_advance: false,
                    rear_advance: false,
                });
                buffer.overlays_mut().insert_overlay(value);
                for (name, property) in overlay.properties() {
                    buffer
                        .overlays_mut()
                        .overlay_put(value, Value::symbol(name), property)
                        .expect("overlay property");
                }
            }
        }
        let buffer = eval.buffer_manager().get(buffer_id).expect("buffer");
        let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
        let table = FaceTable::new();
        let resolver = FaceResolver::new(&table, 0x00ff_ffff, 0x0000_0000, 14.0, None);
        let base_face = resolver.default_face().clone();
        Self {
            _eval: eval,
            buffer_id,
            snapshot,
            resolver,
            base_face,
        }
    }

    fn driver(&self, split: SplitPolicy) -> StreamDriver<'_> {
        StreamDriver {
            fixture: self,
            producer: BufferElementProducer::new_for_window(
                self.buffer_id,
                &self.snapshot,
                Some(HARNESS_WINDOW_ID),
                0,
                0,
            ),
            pending: std::collections::VecDeque::new(),
            position: DisplaySourceTextPosition::new(0, 0),
            face_ids: FrameFaceAttempt::for_test_with_next_id(BASE_FACE.get() + 1),
            split,
        }
    }
}

/// A producer seated where it was when the snapshot was taken, plus the walk
/// position of that moment — GNU SAVE_IT for the harness.
struct DriverSnapshot {
    producer: ProducerSnapshot,
    position: DisplaySourceTextPosition,
    pending: std::collections::VecDeque<DisplaySourceStepItem>,
}

struct StreamDriver<'a> {
    fixture: &'a Fixture,
    producer: BufferElementProducer<'a, LayoutBufferSnapshot>,
    position: DisplaySourceTextPosition,
    face_ids: FrameFaceAttempt,
    split: SplitPolicy,
    /// The renderer's pending queue, kept HERE and only here: P4.5 deleted it
    /// from the producer, and the replay legs still need it to reproduce the
    /// old mechanism they are the differential proof against. A test-only
    /// VecDeque is the honest home for a mechanism production no longer has.
    pending: std::collections::VecDeque<DisplaySourceStepItem>,
}

impl<'a> StreamDriver<'a> {
    /// Consume one element, applying this leg's split feeder, and return the
    /// per-character observations it contributes.
    fn next_observations(&mut self) -> Option<Vec<CharObservation>> {
        self.next_step().map(|(_item, observations)| observations)
    }

    /// Consume one element and return it alongside its observations. The
    /// overlay-string pins read the ELEMENT (its string list is not expressible
    /// as per-character observations); everything else reads the observations.
    fn next_step(&mut self) -> Option<(BufferSourceConsumedItem, Vec<CharObservation>)> {
        let scan_before = self.position;
        let basis = DisplaySourceFaceBasis::new(
            &self.fixture.resolver,
            BASE_FACE,
            &self.fixture.base_face,
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        );
        let item = match self.pending.pop_front() {
            Some(step) => BufferSourceConsumedItem::Renderable(step),
            None => self.producer.next_consumed_item_with_face_basis(
                &self.fixture.snapshot,
                basis,
                &mut self.face_ids,
                &mut self.position,
            )?,
        };
        let item = self.apply_split(item, scan_before);
        if let BufferSourceConsumedItem::Renderable(step) = &item {
            // Publish the position of the item actually consumed. A split leg
            // consumed only the prefix, so both tracks come from THAT item --
            // the renderer does the same with `into_render_parts` rather than
            // keeping the position the whole run advanced to.
            self.position = BufferScanPos::new(
                step.source_end_byte_idx()
                    .unwrap_or_else(|| self.position.byte_idx()),
                step.end_charpos(),
            );
        }
        let observations = observe(&item, scan_before);
        Some((item, observations))
    }

    fn apply_split(
        &mut self,
        item: BufferSourceConsumedItem,
        scan_before: BufferScanPos,
    ) -> BufferSourceConsumedItem {
        let BufferSourceConsumedItem::Renderable(step) = item else {
            return item;
        };
        match self.split {
            SplitPolicy::None => BufferSourceConsumedItem::Renderable(step),
            SplitPolicy::CharGranularityFrom(from_charpos) => {
                if scan_before.charpos() >= from_charpos
                    && let Some(end_charpos) = step.source_end_charpos()
                {
                    self.producer.request_char_granularity_until(end_charpos);
                }
                BufferSourceConsumedItem::Renderable(step)
            }
            SplitPolicy::PerChar => {
                let Some((first, pending)) = step
                    .is_multi_char_text_run()
                    .then(|| step.clone().split_text_run_items(0))
                    .flatten()
                else {
                    return BufferSourceConsumedItem::Renderable(step);
                };
                self.queue_pending(pending);
                BufferSourceConsumedItem::Renderable(first)
            }
            SplitPolicy::FitAt(at_charpos) => {
                let crosses = scan_before.charpos() < at_charpos
                    && step.end_charpos() > at_charpos
                    && step.is_multi_char_text_run();
                let Some((prefix, suffix)) = crosses
                    .then(|| step.clone().split_text_run_at_charpos(at_charpos, 0))
                    .flatten()
                else {
                    return BufferSourceConsumedItem::Renderable(step);
                };
                self.queue_pending(vec![suffix]);
                BufferSourceConsumedItem::Renderable(prefix)
            }
            SplitPolicy::PrefixAt(at_charpos) => {
                let crosses = scan_before.charpos() < at_charpos
                    && step.end_charpos() > at_charpos
                    && step.is_multi_char_text_run();
                let Some((prefix, _tail)) = crosses
                    .then(|| step.clone().split_text_run_at_charpos(at_charpos, 0))
                    .flatten()
                else {
                    return BufferSourceConsumedItem::Renderable(step);
                };
                self.producer.consume_prefix_to(at_charpos);
                BufferSourceConsumedItem::Renderable(prefix)
            }
        }
    }

    fn queue_pending<I: IntoIterator<Item = DisplaySourceStepItem>>(&mut self, items: I) {
        for item in items.into_iter().collect::<Vec<_>>().into_iter().rev() {
            self.pending.push_front(item);
        }
    }

    fn drain(&mut self, limit: usize) -> Vec<CharObservation> {
        let mut stream = Vec::new();
        for _ in 0..limit {
            let Some(observations) = self.next_observations() else {
                break;
            };
            stream.extend(observations);
        }
        stream
    }

    /// Drain until the next element would start at or past `charpos`.
    fn drain_until_charpos(&mut self, charpos: i64) -> Vec<CharObservation> {
        let mut stream = Vec::new();
        while self.position.charpos() < charpos {
            let Some(observations) = self.next_observations() else {
                break;
            };
            stream.extend(observations);
        }
        stream
    }

    fn snapshot(&self) -> DriverSnapshot {
        DriverSnapshot {
            producer: self.producer.snapshot(),
            position: self.position,
            pending: self.pending.clone(),
        }
    }

    fn restore(&mut self, snapshot: DriverSnapshot) {
        self.producer.restore(snapshot.producer);
        self.position = snapshot.position;
        self.pending = snapshot.pending;
    }
}

// ---------------------------------------------------------------------------
// Harness entry points
// ---------------------------------------------------------------------------

fn producer_stream(case: &StreamCase) -> Vec<CharObservation> {
    Fixture::new(case)
        .driver(SplitPolicy::None)
        .drain(DRAIN_LIMIT)
}

/// The harness assertion: a split feeder is invisible on the assertion surface.
fn assert_split_transparent(case: &StreamCase, split: SplitPolicy) {
    let fixture = Fixture::new(case);
    let producer_leg = fixture.driver(SplitPolicy::None).drain(DRAIN_LIMIT);
    let replay_leg = fixture.driver(split).drain(DRAIN_LIMIT);

    assert!(
        !producer_leg.is_empty(),
        "{}: the producer leg produced nothing",
        case.name
    );
    assert_eq!(
        producer_leg, replay_leg,
        "{}: the {:?} split feeder changed the element stream",
        case.name, split
    );
}

fn assert_streams_agree(case: &StreamCase) {
    assert_split_transparent(case, SplitPolicy::PerChar);
}

/// The snapshot/restore contract (C8, C9, C13): consume through a failed
/// overflow attempt past `candidate`, restore the seating saved AT the
/// candidate, and require the remainder stream to be byte-identical to a leg
/// that simply ran to the candidate and continued — INCLUDING re-production of
/// the candidate character consumed during the attempt (the bug class the
/// walk.rs rewind comment documents).
fn assert_restore_resumes_at_candidate(case: &StreamCase, candidate: i64, split: SplitPolicy) {
    let fixture = Fixture::new(case);

    let mut reference = fixture.driver(split);
    reference.drain_until_charpos(candidate);
    let expected = reference.drain(DRAIN_LIMIT);

    let mut attempted = fixture.driver(split);
    attempted.drain_until_charpos(candidate);
    let saved = attempted.snapshot();
    // The failed overflow attempt: consume past the candidate, then give up.
    attempted.next_observations();
    attempted.next_observations();
    attempted.restore(saved);
    let resumed = attempted.drain(DRAIN_LIMIT);

    assert!(
        !expected.is_empty(),
        "{}: nothing to resume at charpos {candidate}",
        case.name
    );
    assert_eq!(
        expected[0].scan_before.charpos(),
        candidate,
        "{}: the corpus candidate must fall on an element boundary",
        case.name
    );
    assert_eq!(
        resumed, expected,
        "{}: restore did not re-produce the stream from charpos {candidate}",
        case.name
    );
}

// ---------------------------------------------------------------------------
// Negative control: prove the harness can fail
// ---------------------------------------------------------------------------

#[test]
fn negative_control_the_harness_detects_a_perturbed_stream() {
    // Kept permanently: a harness that cannot fail proves nothing, and every
    // later rung reads these comparisons as evidence.
    let case = StreamCase::new("negative control", "hello\n");
    let stream = producer_stream(&case);

    let mut wrong_provenance = stream.clone();
    wrong_provenance[2].provenance = GlyphProvenance::buffer(99);
    assert_ne!(stream, wrong_provenance, "provenance must be compared");

    let mut wrong_scan = stream.clone();
    wrong_scan[2].scan_after = BufferScanPos::new(0, 0);
    assert_ne!(stream, wrong_scan, "scan positions must be compared");

    let mut wrong_class = stream.clone();
    wrong_class[2].class = ElementClass::Tab;
    assert_ne!(stream, wrong_class, "element class must be compared");

    let mut wrong_char = stream.clone();
    wrong_char[2].ch = Some('z');
    assert_ne!(stream, wrong_char, "characters must be compared");

    let mut dropped = stream.clone();
    dropped.remove(2);
    assert_ne!(stream, dropped, "stream length and order must be compared");
}

// ---------------------------------------------------------------------------
// C1 - C3: runs, faces, and the per-char feeder
// ---------------------------------------------------------------------------

#[test]
fn c1_plain_ascii_single_face() {
    let case = StreamCase::new("C1 plain ascii", "hello world\n");
    assert_streams_agree(&case);

    let stream = producer_stream(&case);
    assert_eq!(CharObservation::chars(&stream), "hello world\n");
    for (index, observation) in stream.iter().enumerate() {
        assert_eq!(
            observation.provenance,
            GlyphProvenance::buffer(index),
            "every buffer char stamps its own charpos"
        );
        assert_eq!(observation.scan_before.charpos(), index as i64);
    }
    assert_eq!(
        stream.last().expect("row break").class,
        ElementClass::RowBreak
    );
}

#[test]
fn c2_multi_face_line_keeps_one_continuous_scan_track() {
    // A face property cuts the producer's runs (see the boundary test below),
    // but the CHARACTER stream underneath must be untouched: same chars, same
    // provenance, same scan positions as the unfaced baseline. Only the
    // face-ref differs.
    let case = StreamCase::new("C2 multi-face", "abcdef\n")
        .with(CaseProperty::face(2, 4, ":weight", "bold"));
    assert_streams_agree(&case);

    let plain = producer_stream(&StreamCase::new("C2 baseline", "abcdef\n"));
    let faced = producer_stream(&case);
    assert_eq!(plain.len(), faced.len());
    for (plain, faced) in plain.iter().zip(faced.iter()) {
        assert_eq!(plain.ch, faced.ch);
        assert_eq!(plain.provenance, faced.provenance);
        assert_eq!(plain.scan_before, faced.scan_before);
        assert_eq!(plain.scan_after, faced.scan_after);
        assert_eq!(plain.class, faced.class);
    }
    // Where the run BOUNDARIES fall is pinned char-exactly by the boundary
    // test below; this case owns the continuity half.
}

#[test]
fn c2_multi_face_line_ends_runs_at_face_boundaries() {
    let case = StreamCase::new("C2 multi-face", "abcdef\n")
        .with(CaseProperty::face(2, 4, ":weight", "bold"));
    let fixture = Fixture::new(&case);
    let mut driver = fixture.driver(SplitPolicy::None);

    let first = driver.next_observations().expect("first run");
    assert_eq!(CharObservation::chars(&first), "ab");
    let second = driver.next_observations().expect("second run");
    assert_eq!(CharObservation::chars(&second), "cd");
}

#[test]
fn c3_per_char_face_line_is_stream_identical_under_the_per_char_feeder() {
    // THE case that must turn red the moment P4.5 changes consumption order or
    // provenance: the pipeline leg splits every run per character and drains
    // N-1 queued echoes, and the resulting stream must be indistinguishable
    // from the producer's whole-run stream.
    let case = StreamCase::new("C3 per-char faces", "abcdefgh\n").per_char_faces(&[
        (":weight", "bold"),
        (":slant", "italic"),
        (":underline", "t"),
        (":overline", "t"),
        (":strike-through", "t"),
        (":inverse-video", "t"),
        (":extend", "t"),
        (":weight", "ultra-bold"),
    ]);
    assert_streams_agree(&case);

    let stream = producer_stream(&case);
    assert_eq!(CharObservation::chars(&stream), "abcdefgh\n");
}

// ---------------------------------------------------------------------------
// C4: tabs
// ---------------------------------------------------------------------------

#[test]
fn c4a_tab_at_line_start() {
    let case = StreamCase::new("C4a tab at line start", "\tab\n");
    assert_streams_agree(&case);

    let stream = producer_stream(&case);
    assert_eq!(stream[0].class, ElementClass::Tab);
    assert_eq!(stream[0].scan_after.charpos(), 1);
}

#[test]
fn c4b_tab_mid_line() {
    let case = StreamCase::new("C4b tab mid line", "ab\tcd\n");
    assert_streams_agree(&case);

    let stream = producer_stream(&case);
    assert_eq!(stream[2].class, ElementClass::Tab);
    assert_eq!(stream[2].scan_before.charpos(), 2);
    assert_eq!(stream[3].scan_before.charpos(), 3);
}

#[test]
fn c4c_two_adjacent_tabs() {
    let case = StreamCase::new("C4c adjacent tabs", "a\t\tb\n");
    assert_streams_agree(&case);

    let stream = producer_stream(&case);
    assert_eq!(stream[1].class, ElementClass::Tab);
    assert_eq!(stream[2].class, ElementClass::Tab);
}

#[test]
fn c4d_tab_after_wrap_resumes_from_the_candidate() {
    // The tab lands on the continuation row; the producer contract is that a
    // restore at the wrap candidate re-produces it. Its EXPANSION from the
    // continuation row's own pen is renderer-side and stays pinned by the
    // shipped 2j shadow continuation_resume_shadow_matches_tab_after_wrap_row.
    let case = StreamCase::new("C4d tab after wrap", "aaaaaaaaaaaaaaaaaa\tZ\n");
    assert_restore_resumes_at_candidate(&case, 16, SplitPolicy::FitAt(16));
}

// ---------------------------------------------------------------------------
// C5 / C6: multibyte and cluster seams
// ---------------------------------------------------------------------------

#[test]
fn c5_wide_chars_track_multibyte_byte_deltas() {
    let case = StreamCase::new("C5 wide chars", "a漢字b\n");
    assert_streams_agree(&case);

    let stream = producer_stream(&case);
    assert_eq!(CharObservation::chars(&stream), "a漢字b\n");
    assert_eq!(stream[1].scan_before, BufferScanPos::new(1, 1));
    assert_eq!(
        stream[1].scan_after,
        BufferScanPos::new(4, 2),
        "a 3-byte char advances byte_idx by 3 and charpos by 1"
    );
    assert_eq!(stream[3].scan_before, BufferScanPos::new(7, 3));
}

#[test]
fn c6a_base_and_extender_stay_in_one_stream() {
    let case = StreamCase::new("C6a combining acute", "ae\u{301}z\n");
    assert_streams_agree(&case);

    let stream = producer_stream(&case);
    assert_eq!(CharObservation::chars(&stream), "ae\u{301}z\n");
    assert_eq!(
        stream[2].scan_before,
        BufferScanPos::new(2, 2),
        "the extender is its own scan step"
    );
}

#[test]
fn c6b_extender_at_a_face_seam_stays_in_one_stream() {
    let case = StreamCase::new("C6b extender at face seam", "ae\u{301}z\n")
        .with(CaseProperty::face(2, 3, ":weight", "bold"));
    assert_streams_agree(&case);
}

#[test]
#[ignore = "un-ignored by the rung that gives ProducedChar its char class: Wide / ComposedExtender classification and the never-split-a-cluster run seam rule (design section 4.10)"]
fn c6_cluster_classes_are_carried_on_the_element() {
    let case = StreamCase::new("C6 classes", "ae\u{301}z\n");
    let stream = producer_stream(&case);
    // Expected once the class is carried: the extender is a zero-advance
    // ComposedExtender and no run ever ends between a base and its extender.
    assert_ne!(stream[2].class, ElementClass::PlainChar);
}

// ---------------------------------------------------------------------------
// C7: invisible elision
// ---------------------------------------------------------------------------

#[test]
fn c7a_invisible_text_is_not_elided_by_the_producer_today() {
    // The honest current contract: elision is the renderer's invisible
    // checkpoint, so the producer streams the hidden characters and the split
    // feeder stays transparent over them.
    let case =
        StreamCase::new("C7a invisible mid-line", "abXXcd\n").with(CaseProperty::invisible(2, 4));
    assert_streams_agree(&case);

    let stream = producer_stream(&case);
    assert_eq!(CharObservation::chars(&stream), "abXXcd\n");
}

#[test]
#[ignore = "un-ignored by P4.8: the invisible checkpoint (row_lifecycle.rs) moves into producer stop state, and the scan track then jumps the elided span"]
fn c7a_invisible_span_jumps_the_scan_track() {
    let case =
        StreamCase::new("C7a invisible mid-line", "abXXcd\n").with(CaseProperty::invisible(2, 4));
    let stream = producer_stream(&case);
    assert_eq!(CharObservation::chars(&stream), "abcd\n");
    assert_eq!(stream[2].scan_before.charpos(), 4);
}

#[test]
fn c7b_trailing_elision_before_the_newline_keeps_the_row_break() {
    let case =
        StreamCase::new("C7b trailing elision", "abXX\n").with(CaseProperty::invisible(2, 4));
    assert_streams_agree(&case);

    let stream = producer_stream(&case);
    assert_eq!(
        stream.last().expect("row break").class,
        ElementClass::RowBreak
    );
}

#[test]
fn c7c_adjacent_invisible_runs_keep_one_continuous_scan_track() {
    let case = StreamCase::new("C7c adjacent invisible", "aXYb\n")
        .with(CaseProperty::invisible(1, 2))
        .with(CaseProperty::invisible(2, 3));
    assert_streams_agree(&case);

    let stream = producer_stream(&case);
    for (index, observation) in stream.iter().enumerate() {
        assert_eq!(observation.scan_before.charpos(), index as i64);
    }
}

// ---------------------------------------------------------------------------
// C8 / C9 / C13: the snapshot-restore contract
// ---------------------------------------------------------------------------

#[test]
fn c8a_single_wrap_restores_at_the_wrap_point() {
    let case = StreamCase::new("C8a single wrap", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n");
    assert_restore_resumes_at_candidate(&case, 20, SplitPolicy::FitAt(20));
}

#[test]
fn c8b_iterated_overflow_restores_at_every_row_edge() {
    let case = StreamCase::new(
        "C8b iterated overflow",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
    );
    for candidate in [20, 40, 60] {
        assert_restore_resumes_at_candidate(&case, candidate, SplitPolicy::FitAt(candidate));
    }
}

#[test]
fn c8c_wrap_at_a_wide_edge_char_restores_at_the_wide_char() {
    let case = StreamCase::new("C8c wide edge char", "aaaaaaaaaaaaaaaaaaa漢tail\n");
    assert_restore_resumes_at_candidate(&case, 19, SplitPolicy::FitAt(19));
}

#[test]
fn c9a_word_wrap_candidate_at_a_space() {
    // Break at the space; the continuation row resumes at `b` (charpos 5).
    let case = StreamCase::new("C9a candidate at space", "aaaa bbbbbb\n");
    assert_restore_resumes_at_candidate(&case, 5, SplitPolicy::FitAt(5));
}

#[test]
fn c9b_word_wrap_candidate_mid_run() {
    // The `b` word itself exceeds the row: after the space break the run
    // char-wraps with no candidate inside it.
    let case = StreamCase::new("C9b candidate mid run", "aaaa bbbbbbbbbbbb\n");
    assert_restore_resumes_at_candidate(&case, 5, SplitPolicy::FitAt(5));
    assert_restore_resumes_at_candidate(&case, 12, SplitPolicy::FitAt(12));
}

#[test]
fn c9c_word_wrap_with_no_candidate_falls_back_to_the_row_edge() {
    let case = StreamCase::new("C9c no candidate", "aaaaaaaaaaaa\n");
    assert_restore_resumes_at_candidate(&case, 10, SplitPolicy::FitAt(10));
}

#[test]
fn c9d_word_wrap_candidate_before_a_tab() {
    // Compound of C4d and C9a: the candidate space sits immediately before a
    // tab whose expansion overflows, so the tab is re-produced on the
    // continuation row.
    let case = StreamCase::new("C9d candidate before tab", "aaaa \tbb\n");
    assert_restore_resumes_at_candidate(&case, 5, SplitPolicy::FitAt(5));

    let fixture = Fixture::new(&case);
    let mut driver = fixture.driver(SplitPolicy::FitAt(5));
    driver.drain_until_charpos(5);
    let resumed = driver.drain(DRAIN_LIMIT);
    assert_eq!(resumed[0].class, ElementClass::Tab);
}

#[test]
fn c13_fit_split_isolation_resumes_at_the_split_point() {
    // The `consume_prefix(k)` contract in its P4.2 form: cut a 24-char run at
    // 20, and the next element starts at charpos 20 with the right byte_idx.
    let case = StreamCase::new("C13 fit split", "aaaaaaaaaaaaaaaaaaaaaaaa\n");
    let fixture = Fixture::new(&case);
    let mut driver = fixture.driver(SplitPolicy::FitAt(20));

    let prefix = driver.drain_until_charpos(20);
    assert_eq!(prefix.len(), 20);
    let remainder = driver.drain(DRAIN_LIMIT);
    assert_eq!(remainder[0].scan_before, BufferScanPos::new(20, 20));
    assert_eq!(CharObservation::chars(&remainder), "aaaa\n");

    assert_split_transparent(&case, SplitPolicy::FitAt(20));
}

// ---------------------------------------------------------------------------
// C10 / C11 / C14 / C15: renderer-owned glyphs are ABSENT from the stream
// ---------------------------------------------------------------------------

fn assert_no_redisplay_provenance(case: &StreamCase) {
    for observation in producer_stream(case) {
        assert!(
            matches!(observation.provenance, GlyphProvenance::Buffer { .. }),
            "{}: the producer stream must carry only buffer provenance; \
             redisplay's own glyphs are renderer-owned",
            case.name
        );
    }
}

#[test]
fn c10_truncation_marks_are_absent_from_the_producer_stream() {
    // Truncation marks are Redisplay(Mark) glyphs emitted by
    // special_glyphs.rs, never produced elements.
    let case = StreamCase::new("C10 truncation", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nZ\n");
    assert_no_redisplay_provenance(&case);
    assert_streams_agree(&case);
}

#[test]
fn c11_hscroll_left_markers_are_absent_and_the_stream_is_position_addressable() {
    // The hscroll skip is renderer-side; the producer is simply asked to start
    // at the post-skip position, and the left-truncation marker never appears.
    let case = StreamCase::new("C11 hscroll", "abcdefghij\n");
    assert_no_redisplay_provenance(&case);

    let fixture = Fixture::new(&case);
    let mut driver = fixture.driver(SplitPolicy::None);
    driver
        .producer
        .rewind_character_wrap_to(DisplaySourceTextPosition::new(5, 5));
    driver.position = DisplaySourceTextPosition::new(5, 5);
    let stream = driver.drain(DRAIN_LIMIT);

    assert_eq!(CharObservation::chars(&stream), "fghij\n");
    assert_eq!(stream[0].scan_before, BufferScanPos::new(5, 5));
}

#[test]
fn c14_eob_tail_without_a_trailing_newline_ends_without_a_row_break() {
    let case = StreamCase::new("C14 eob tail", "abc");
    assert_streams_agree(&case);

    let stream = producer_stream(&case);
    assert_eq!(CharObservation::chars(&stream), "abc");
    assert!(
        stream
            .iter()
            .all(|observation| observation.class != ElementClass::RowBreak),
        "a newline-less last line produces no RowBreak"
    );
}

#[test]
fn c15_empty_lines_produce_row_breaks_and_no_empty_line_glyph() {
    let case = StreamCase::new("C15 empty lines", "\n\n");
    assert_streams_agree(&case);

    let stream = producer_stream(&case);
    assert_eq!(stream.len(), 2);
    assert!(
        stream
            .iter()
            .all(|observation| observation.class == ElementClass::RowBreak)
    );
    assert_eq!(stream[0].provenance, GlyphProvenance::buffer(0));
    assert_eq!(stream[1].provenance, GlyphProvenance::buffer(1));
    // The empty-line cursor glyph is Redisplay(EmptyLineNewline) and is
    // renderer-owned (row-level in this engine, e772f82ed) — never a produced
    // element.
    assert_no_redisplay_provenance(&case);
}

// ---------------------------------------------------------------------------
// C12: overlay seams
// ---------------------------------------------------------------------------

/// The run boundaries a case produces, as the strings of each element: the
/// char-exact statement of where the producer's stop state fires.
fn producer_run_texts(case: &StreamCase) -> Vec<String> {
    let fixture = Fixture::new(case);
    let mut driver = fixture.driver(SplitPolicy::None);
    let mut runs = Vec::new();
    for _ in 0..DRAIN_LIMIT {
        let Some(observations) = driver.next_observations() else {
            break;
        };
        runs.push(CharObservation::chars(&observations));
    }
    runs
}

#[test]
fn c12_overlay_face_seam_terminates_runs() {
    // A face-only overlay over [3,6) is a stop boundary even though it carries
    // no string: GNU folds `next_overlay_change` into `compute_stop_pos`
    // (xdisp.c:4356-4365) and this engine mirrors it in
    // `BufferTextSourceCursor::next_property_change`. Runs are pinned as
    // EXPLICIT goldens, not derived from the stream under test.
    let case = StreamCase::new("C12 overlay face seam", "abcdefgh\n")
        .with_overlay(CaseOverlay::face(3, 6, ":weight", "bold"));
    assert_streams_agree(&case);
    assert_eq!(producer_run_texts(&case), vec!["abc", "def", "gh", "\n"]);

    // Same text with no overlay: one run, so the boundaries above are the
    // overlay's doing and not an artifact of run production.
    let plain = StreamCase::new("C12 baseline", "abcdefgh\n");
    assert_eq!(producer_run_texts(&plain), vec!["abcdefgh", "\n"]);
}

#[test]
fn c12_overlay_face_seam_keeps_the_char_stream_continuous() {
    // The seam changes where runs END and which face they carry; it must not
    // change the characters, provenance or scan track underneath.
    let case = StreamCase::new("C12 overlay face seam", "abcdefgh\n")
        .with_overlay(CaseOverlay::face(3, 6, ":weight", "bold"));
    let plain = producer_stream(&StreamCase::new("C12 baseline", "abcdefgh\n"));
    let overlaid = producer_stream(&case);

    assert_eq!(plain.len(), overlaid.len());
    for (plain, overlaid) in plain.iter().zip(overlaid.iter()) {
        assert_eq!(plain.ch, overlaid.ch);
        assert_eq!(plain.provenance, overlaid.provenance);
        assert_eq!(plain.scan_before, overlaid.scan_before);
        assert_eq!(plain.scan_after, overlaid.scan_after);
        assert_eq!(plain.class, overlaid.class);
    }
    // The overlaid span carries a DIFFERENT face than the text around it —
    // otherwise the seam above would be pinning nothing.
    assert_ne!(
        overlaid[3].face, overlaid[0].face,
        "the overlay face must actually reach the covered characters"
    );
    assert_eq!(overlaid[6].face, overlaid[0].face);
}

#[test]
fn c12_overlay_string_anchors_terminate_runs() {
    // The P4.4 contract: a produced run never CROSSES an overlay-string anchor,
    // so the renderer always meets an anchor at an element boundary. A
    // before-string anchors at the overlay's start (charpos 3), an after-string
    // at its end (charpos 6). Goldens, not derived.
    // Since P4.6 the anchor ALSO produces an overlay-strings element (empty
    // string here, since it carries no characters of its own), which is why an
    // empty entry appears at the anchor: the run boundaries around it are what
    // this case pins.
    let before = StreamCase::new("C12 before-string anchor", "abcdefgh\n")
        .with_overlay(CaseOverlay::before_string(3, 6, "B"));
    // The strings element (empty text, it carries no buffer characters) appears
    // at charpos 3 only — a before-string anchors at the overlay START, and the
    // boundary at 6 still cuts the run without anchoring anything.
    assert_eq!(
        producer_run_texts(&before),
        vec!["abc", "", "def", "gh", "\n"]
    );

    let after = StreamCase::new("C12 after-string anchor", "abcdefgh\n")
        .with_overlay(CaseOverlay::after_string(3, 6, "A"));
    // Mirror image: an after-string anchors at the overlay END, charpos 6.
    assert_eq!(
        producer_run_texts(&after),
        vec!["abc", "def", "", "gh", "\n"]
    );

    // An anchor at a zero-length overlay: start and end coincide, so the single
    // boundary still cuts the run there.
    let point = StreamCase::new("C12 zero-length anchor", "abcdefgh\n")
        .with_overlay(CaseOverlay::before_string(4, 4, "P"));
    assert_eq!(producer_run_texts(&point), vec!["abcd", "", "efgh", "\n"]);

    // At a row start the anchor coincides with the run start, so there is
    // nothing to cut: the row's first run is whole.
    let row_start = StreamCase::new("C12 anchor at row start", "abcdefgh\n")
        .with_overlay(CaseOverlay::before_string(0, 2, "S"));
    assert_eq!(
        producer_run_texts(&row_start),
        vec!["", "ab", "cdefgh", "\n"]
    );
}

#[test]
fn c12_overlay_scoped_to_another_window_is_not_a_seam() {
    // `window`-scoped overlays apply only in their own window
    // (GNU overlay_applies_to_window). The stop state is deliberately COARSER
    // than the string collection: it stops at every overlay boundary, windowed
    // or not, because a superset of stops is always safe and cheaper than
    // filtering the boundary index. What must NOT happen is a foreign window's
    // overlay reaching the glyphs.
    let foreign = StreamCase::new("C12 foreign window overlay", "abcdefgh\n")
        .with_overlay(CaseOverlay::face(3, 6, ":weight", "bold").in_window(HARNESS_WINDOW_ID + 1));
    let stream = producer_stream(&foreign);
    assert_eq!(CharObservation::chars(&stream), "abcdefgh\n");
    for observation in &stream {
        assert_eq!(
            observation.face, stream[0].face,
            "another window's overlay face must not reach this window's glyphs"
        );
    }

    // The same overlay scoped to THIS window does reach them.
    let local = StreamCase::new("C12 local window overlay", "abcdefgh\n")
        .with_overlay(CaseOverlay::face(3, 6, ":weight", "bold").in_window(HARNESS_WINDOW_ID));
    let stream = producer_stream(&local);
    assert_ne!(stream[3].face, stream[0].face);
}

// ---------------------------------------------------------------------------
// P4.3: the fit split is replaced by a prefix consume
// ---------------------------------------------------------------------------

#[test]
fn p43_prefix_consume_leaves_nothing_pending_and_resumes_at_the_prefix_end() {
    // The rung's contract: after the renderer takes a fitting prefix the
    // producer sits at the first unfitting character with an EMPTY pending
    // queue — no tail is pushed back for a later iteration to pop.
    let case = StreamCase::new("P4.3 fit consume", "aaaaaaaaaaaaaaaaaaaaaaaa\n");
    let fixture = Fixture::new(&case);
    let mut driver = fixture.driver(SplitPolicy::PrefixAt(20));

    let prefix = driver.drain_until_charpos(20);
    assert_eq!(prefix.len(), 20);
    assert_eq!(
        driver.pending.len(),
        0,
        "the prefix consume must not queue a remainder"
    );

    let remainder = driver.drain(DRAIN_LIMIT);
    assert_eq!(remainder[0].scan_before, BufferScanPos::new(20, 20));
    assert_eq!(CharObservation::chars(&remainder), "aaaa\n");
}

#[test]
fn p43_prefix_consume_is_stream_identical_to_the_queueing_fit_split() {
    // The deletion proof, including a property seam inside the discarded tail
    // and a multibyte boundary: the old mechanism (queue the tail) and the new
    // one (reseat the producer) yield the same element stream.
    for case in [
        StreamCase::new("P4.3 plain", "aaaaaaaaaaaaaaaaaaaaaaaa\n"),
        StreamCase::new("P4.3 multibyte", "aaaaaaaaaaaaaaaaaaa漢tail\n"),
        StreamCase::new("P4.3 tab in tail", "aaaaaaaaaaaaaaaaaa\tZ\n"),
        StreamCase::new("P4.3 face seam in tail", "aaaaaaaaaaaaaaaaaaaaaaaa\n")
            .with(CaseProperty::face(18, 21, ":weight", "bold")),
    ] {
        let fixture = Fixture::new(&case);
        let queued = fixture.driver(SplitPolicy::FitAt(16)).drain(DRAIN_LIMIT);
        let consumed = fixture.driver(SplitPolicy::PrefixAt(16)).drain(DRAIN_LIMIT);
        assert!(!queued.is_empty(), "{}: nothing produced", case.name);
        assert_eq!(
            queued, consumed,
            "{}: the prefix consume changed the element stream",
            case.name
        );
    }
}

// ---------------------------------------------------------------------------
// P4.5: the per-char feeder becomes producer-side char granularity
// ---------------------------------------------------------------------------

#[test]
fn p45_char_granularity_is_stream_identical_to_the_queueing_per_char_split() {
    // The deletion proof: asking the producer for single characters and
    // splitting a whole run into queued single-character items must yield the
    // same element stream, over corpora whose runs carry the seams the renderer
    // cares about (a face boundary, multibyte, a tab).
    for case in [
        StreamCase::new("P4.5 plain", "hello world\n"),
        StreamCase::new("P4.5 multibyte", "a漢字b tail\n"),
        StreamCase::new("P4.5 tab", "ab\tcd efgh\n"),
        StreamCase::new("P4.5 face seam", "abcdefgh\n")
            .with(CaseProperty::face(3, 6, ":weight", "bold")),
        StreamCase::new("P4.5 overlay seam", "abcdefgh\n")
            .with_overlay(CaseOverlay::face(3, 6, ":weight", "bold")),
    ] {
        let fixture = Fixture::new(&case);
        let queued = fixture.driver(SplitPolicy::PerChar).drain(DRAIN_LIMIT);
        let granular = fixture
            .driver(SplitPolicy::CharGranularityFrom(0))
            .drain(DRAIN_LIMIT);
        assert!(!queued.is_empty(), "{}: nothing produced", case.name);
        assert_eq!(
            queued, granular,
            "{}: producer-side char granularity changed the element stream",
            case.name
        );
    }
}

#[test]
fn p45_char_granularity_yields_single_characters_and_expires_by_position() {
    // The mechanism's two contracts: while the hint is live the producer stops
    // batching, and it expires at the requested end so the next run batches
    // again. Element texts are the observable, pinned explicitly.
    let case = StreamCase::new("P4.5 granularity extent", "abcdefgh\n")
        .with(CaseProperty::face(4, 8, ":weight", "bold"));
    let fixture = Fixture::new(&case);

    // Baseline: two runs, cut at the face boundary.
    assert_eq!(producer_run_texts(&case), vec!["abcd", "efgh", "\n"]);

    // Decline batching for the first run only: it arrives one character at a
    // time, and the run AFTER the hint's end is whole again.
    let mut driver = fixture.driver(SplitPolicy::None);
    driver.producer.request_char_granularity_until(4);
    let mut runs = Vec::new();
    for _ in 0..DRAIN_LIMIT {
        let Some(observations) = driver.next_observations() else {
            break;
        };
        runs.push(CharObservation::chars(&observations));
    }
    assert_eq!(runs, vec!["a", "b", "c", "d", "efgh", "\n"]);
}

// ---------------------------------------------------------------------------
// P4.6: the ORDER and CONTENT of the producer's overlay-strings element
//
// Since P4.6 sub-step 1 the DECISION, COLLECTION and GNU ORDERING of overlay
// strings are the producer's, so this is where the ordering rules from
// tmp/p4-test-inventory.md section 2 (O1-O8) belong. The glyph-level siblings in
// engine_test.rs (overlay_string_shadow_*) prove the same rules survive
// rendering; these prove the producer computes them, which is the half that can
// be pinned without a row.
//
// TWO THINGS ARE DELIBERATELY NOT PINNED (the inventory's do-not-pin list):
//
// * Equal-priority strings of the SAME kind from DIFFERENT overlays. GNU feeds
//   a comparator that returns 0 for that pair to plain `qsort`
//   (xdisp.c:7180) — unstable, so any order asserted here would invent a
//   contract GNU does not offer. Every case below gives its overlays DISTINCT
//   priorities where the order matters.
// * Anything observable about GNU's 16-string chunked re-collection
//   (OVERLAY_STRING_CHUNK_SIZE, dispextern.h:2559): GNU re-runs collection
//   against live buffer state at each chunk boundary, while this engine walks a
//   buffer SNAPSHOT and collects once. That is a deliberate, documented
//   divergence (design section 4.2), so the only thing worth pinning about a
//   many-string anchor is that ALL of them arrive in one correctly ordered list
//   — which o_many_strings_arrive_in_one_ordered_list does.
// ---------------------------------------------------------------------------

/// One overlay string as the producer surfaced it. The KIND is asserted next to
/// the text because a pin that reads text alone cannot tell a correctly ordered
/// list from one whose before/after discriminator is inverted.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ObservedOverlayString {
    text: String,
    kind: ObservedOverlayStringKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ObservedOverlayStringKind {
    Before,
    After,
}

fn before(text: &str) -> ObservedOverlayString {
    ObservedOverlayString {
        text: text.to_owned(),
        kind: ObservedOverlayStringKind::Before,
    }
}

fn after(text: &str) -> ObservedOverlayString {
    ObservedOverlayString {
        text: text.to_owned(),
        kind: ObservedOverlayStringKind::After,
    }
}

/// Every overlay-strings element the producer emits while draining `case`, as
/// (anchor charpos, the element's strings in production order).
fn producer_overlay_string_elements(case: &StreamCase) -> Vec<(i64, Vec<ObservedOverlayString>)> {
    let fixture = Fixture::new(case);
    let mut driver = fixture.driver(SplitPolicy::None);
    let mut elements = Vec::new();
    for _ in 0..DRAIN_LIMIT {
        let Some((item, _observations)) = driver.next_step() else {
            break;
        };
        let BufferSourceConsumedItem::OverlayStrings(strings) = item else {
            continue;
        };
        elements.push((
            strings.anchor_charpos().get() as i64,
            strings
                .strings()
                .iter()
                .copied()
                .map(observed_overlay_string)
                .collect(),
        ));
    }
    elements
}

/// The strings of the single element anchored at `anchor`, or an empty list if
/// the producer surfaced no element there. Both outcomes are assertable, which
/// is what O6 (nothing collected) needs.
fn producer_overlay_strings_at(case: &StreamCase, anchor: i64) -> Vec<ObservedOverlayString> {
    producer_overlay_string_elements(case)
        .into_iter()
        .find(|(charpos, _)| *charpos == anchor)
        .map(|(_, strings)| strings)
        .unwrap_or_default()
}

fn observed_overlay_string(string: OverlayDisplayString) -> ObservedOverlayString {
    ObservedOverlayString {
        text: String::from_utf8(
            string
                .bytes()
                .expect("an overlay string element holds Lisp strings")
                .to_vec(),
        )
        .expect("corpus overlay strings are utf-8"),
        kind: if string.after_string_p {
            ObservedOverlayStringKind::After
        } else {
            ObservedOverlayStringKind::Before
        },
    }
}

/// The corpus text every O-case uses: long enough that each anchor sits
/// mid-line, short enough to read.
const O_TEXT: &str = "abcdefgh\n";

#[test]
fn o1_after_strings_precede_before_strings_from_different_overlays() {
    // GNU compare_overlay_entries, xdisp.c:7020-7076: "Let after-strings appear
    // in front of before-strings if they come from different overlays."
    let case = StreamCase::new("O1 after before", O_TEXT)
        .with_overlay(CaseOverlay::after_string(1, 3, "A"))
        .with_overlay(CaseOverlay::before_string(3, 5, "B"));
    assert_eq!(
        producer_overlay_strings_at(&case, 3),
        vec![after("A"), before("B")]
    );
}

#[test]
fn o2_the_same_overlay_puts_its_before_string_first() {
    // The same-overlay exception (xdisp.c:7044-7051): within ONE overlay the
    // before-string comes first, which reverses O1's rule. A zero-length overlay
    // is the position where both of one overlay's strings are collected at once.
    let case = StreamCase::new("O2 same overlay", O_TEXT).with_overlay(
        CaseOverlay::at(4, 4)
            .with_before_string("B")
            .with_after_string("A"),
    );
    assert_eq!(
        producer_overlay_strings_at(&case, 4),
        vec![before("B"), after("A")]
    );

    // The contrast that makes the exception load-bearing: the same two strings
    // on two DIFFERENT zero-length overlays at the same position order the
    // other way round (O1's rule).
    let split = StreamCase::new("O2 different overlays", O_TEXT)
        .with_overlay(CaseOverlay::at(4, 4).with_before_string("B"))
        .with_overlay(CaseOverlay::at(4, 4).with_after_string("A"));
    assert_eq!(
        producer_overlay_strings_at(&split, 4),
        vec![after("A"), before("B")]
    );
}

#[test]
fn o3_priority_sorts_before_strings_up_and_after_strings_down() {
    // The direction REVERSES between the two groups (xdisp.c:7061-7072):
    // after-strings high to low, before-strings low to high. Asserted on one
    // anchor carrying all four so the combined order is pinned too.
    let case = StreamCase::new("O3 priority reversal", O_TEXT)
        .with_overlay(CaseOverlay::after_string(1, 3, "a1").with_priority(1))
        .with_overlay(CaseOverlay::after_string(1, 3, "a5").with_priority(5))
        .with_overlay(CaseOverlay::before_string(3, 5, "b1").with_priority(1))
        .with_overlay(CaseOverlay::before_string(3, 5, "b5").with_priority(5));
    assert_eq!(
        producer_overlay_strings_at(&case, 3),
        vec![after("a5"), after("a1"), before("b1"), before("b5")]
    );
}

#[test]
fn o4_a_cons_priority_orders_strings_as_priority_zero() {
    // load_overlay_strings reads the priority as a PLAIN fixnum
    // (FIXNUMP (priority) ? XFIXNUM (priority) : 0, xdisp.c:7132-7134), so the
    // cons form sorts as 0 and leads the ascending before-string group — even
    // though face merging DOES understand the cons (buffer.c:3840-3858, pinned
    // glyph-side by overlay_string_shadow_cons_priority_degrades_to_zero_for_
    // string_order and the face half below).
    let case = StreamCase::new("O4 cons priority", O_TEXT)
        .with_overlay(CaseOverlay::before_string(3, 5, "C").with_cons_priority(7, 1))
        .with_overlay(CaseOverlay::before_string(3, 5, "3").with_priority(3));
    assert_eq!(
        producer_overlay_strings_at(&case, 3),
        vec![before("C"), before("3")]
    );

    // Control: with the SAME overlay carrying a plain priority 7 the order
    // flips, so the case above is pinning the cons degradation and not the
    // collection order of the corpus.
    let plain = StreamCase::new("O4 plain priority", O_TEXT)
        .with_overlay(CaseOverlay::before_string(3, 5, "C").with_priority(7))
        .with_overlay(CaseOverlay::before_string(3, 5, "3").with_priority(3));
    assert_eq!(
        producer_overlay_strings_at(&plain, 3),
        vec![before("3"), before("C")]
    );
}

#[test]
fn o5_an_overlays_own_face_does_not_reach_its_own_strings() {
    // The producer half of O5: an overlay carrying BOTH a face and a string
    // contributes the string to the element and the face to the covered buffer
    // characters, and the two never meet — the element carries no face at all,
    // because GNU resolves an overlay string's base face through
    // face_for_overlay_string, which "simply disregards the `face' properties of
    // all overlays" (xfaces.c:7034-7092). The glyph-level half is
    // overlay_string_shadow_overlay_face_does_not_tint_its_own_string.
    let case = StreamCase::new("O5 face and string", O_TEXT).with_overlay(
        CaseOverlay::at(3, 6)
            .with_face(":weight", "bold")
            .with_before_string("S"),
    );
    assert_eq!(producer_overlay_strings_at(&case, 3), vec![before("S")]);

    // The face still reaches the buffer text it covers. Observations are
    // indexed by CHARPOS, not by position in the stream: the overlay-strings
    // element contributes a character-less observation of its own, so stream
    // indices past an anchor no longer equal charpos.
    let stream = producer_stream(&case);
    let face_at = |charpos: i64| {
        stream
            .iter()
            .find(|observation| {
                observation.ch.is_some() && observation.scan_before.charpos() == charpos
            })
            .expect("a buffer character at this charpos")
            .face
    };
    assert_ne!(
        face_at(3),
        face_at(0),
        "the overlay face must reach the characters it covers"
    );
    assert_eq!(face_at(6), face_at(0));
}

#[test]
fn o6_empty_and_non_string_values_are_dropped_at_collection() {
    // GNU's guards are STRINGP (str) && SCHARS (str) (xdisp.c:7171-7182): both
    // rejections happen at COLLECTION, so neither value may reach the element —
    // an element carrying an empty string would push an empty string frame the
    // renderer then has to no-op away.
    let case = StreamCase::new("O6 dropped values", O_TEXT)
        .with_overlay(CaseOverlay::before_string(3, 5, ""))
        .with_overlay(CaseOverlay::at(3, 5).with_non_string_before_string(42));
    assert_eq!(producer_overlay_string_elements(&case), Vec::new());

    // The same corpus with a real string does produce one, so the assertion
    // above is not passing because the anchors are wrong.
    let kept = StreamCase::new("O6 kept value", O_TEXT)
        .with_overlay(CaseOverlay::before_string(3, 5, "S"));
    assert_eq!(
        producer_overlay_string_elements(&kept),
        vec![(3, vec![before("S")])]
    );
}

#[test]
fn o7_an_invisible_overlay_contributes_both_strings_at_both_ends() {
    // "If the text ``under'' the overlay is invisible, both before- and
    // after-strings from this overlay are visible; start and end position are
    // indistinguishable" (xdisp.c:7157-7173). Same-overlay order applies at each
    // end, so both lists read before-then-after (O2).
    //
    // The producer's stream visits BOTH endpoints here because elision is still
    // renderer-owned at this rung (see c7a): the harness therefore sees the
    // collection rule twice, once per endpoint. In a real row the invisible
    // checkpoint skips the producer from the start endpoint to the end one
    // before the start is ever visited, so the strings render ONCE - which is
    // GNU's observable, and which
    // overlay_string_shadow_invisible_overlay_shows_both_strings_once pins at
    // glyph level. When P4.8 moves elision into producer stop state this case's
    // first element disappears with it.
    let case = StreamCase::new("O7 invisible overlay", O_TEXT).with_overlay(
        CaseOverlay::at(3, 6)
            .invisible()
            .with_before_string("B")
            .with_after_string("A"),
    );
    assert_eq!(
        producer_overlay_string_elements(&case),
        vec![
            (3, vec![before("B"), after("A")]),
            (6, vec![before("B"), after("A")]),
        ]
    );

    // Without `invisible` the same overlay contributes each string at its own
    // end only — the rule above is the invisibility's doing.
    let visible = StreamCase::new("O7 visible overlay", O_TEXT).with_overlay(
        CaseOverlay::at(3, 6)
            .with_before_string("B")
            .with_after_string("A"),
    );
    assert_eq!(
        producer_overlay_string_elements(&visible),
        vec![(3, vec![before("B")]), (6, vec![after("A")])]
    );

    // The zero-length invisible overlay: both strings fire at the single
    // position, which is the shape completion UIs build.
    let zero_length = StreamCase::new("O7 zero-length invisible", O_TEXT).with_overlay(
        CaseOverlay::at(4, 4)
            .invisible()
            .with_before_string("B")
            .with_after_string("A"),
    );
    assert_eq!(
        producer_overlay_string_elements(&zero_length),
        vec![(4, vec![before("B"), after("A")])]
    );
}

#[test]
fn o8_an_overlay_scoped_to_another_window_contributes_no_strings() {
    // GNU skips an overlay whose `window` property names a different window
    // (xdisp.c:7147-7156). The harness producer is seated in HARNESS_WINDOW_ID.
    let foreign = StreamCase::new("O8 foreign window", O_TEXT)
        .with_overlay(CaseOverlay::before_string(3, 5, "S").in_window(HARNESS_WINDOW_ID + 1));
    assert_eq!(producer_overlay_string_elements(&foreign), Vec::new());

    // The same overlay scoped to THIS window does contribute.
    let local = StreamCase::new("O8 local window", O_TEXT)
        .with_overlay(CaseOverlay::before_string(3, 5, "S").in_window(HARNESS_WINDOW_ID));
    assert_eq!(
        producer_overlay_string_elements(&local),
        vec![(3, vec![before("S")])]
    );
}

#[test]
fn o_many_strings_arrive_in_one_ordered_list() {
    // The only thing worth pinning about a position carrying more strings than
    // GNU's 16-string chunk: this engine collects once against a snapshot, so
    // ALL of them arrive in one element, ordered. Nothing about chunking is
    // asserted — see the module note above.
    let mut case = StreamCase::new("O many strings", O_TEXT);
    for priority in 0..20 {
        case = case.with_overlay(
            CaseOverlay::at(3, 5)
                .with_before_string(O_MANY_STRINGS[priority as usize])
                .with_priority(priority),
        );
    }
    let strings = producer_overlay_strings_at(&case, 3);
    assert_eq!(strings.len(), 20, "every string arrives in one element");
    assert_eq!(
        strings,
        O_MANY_STRINGS
            .iter()
            .map(|text| before(text))
            .collect::<Vec<_>>(),
        "ascending priority orders all 20 before-strings, chunk size or not"
    );
}

/// Twenty distinguishable one-character strings, more than GNU's 16-string
/// chunk, each given a distinct priority by its index.
const O_MANY_STRINGS: [&str; 20] = [
    "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s",
    "t",
];
