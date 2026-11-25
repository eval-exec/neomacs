//! FrameSync (DESIGN §6.10): a receiver rebuilds exactly what the sender
//! sealed, whatever sequence of updates it saw.

use std::collections::BTreeSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use neomacs_display_protocol::input_progress::{FrontierSnapshot, InputStreamId};
use neomacs_display_protocol::xwidget_extent::XwidgetContentExtent;
use neomacs_display_protocol::{GlyphType, SealedFramePresentation, WebViewId, XwidgetId};
use neomacs_display_wire::codec::WireCodec;
use neomacs_display_wire::payload::Payload;
use neomacs_display_wire::present::{
    Changed, FrameSyncDecoder, FrameSyncEncoder, PresentOp, RejectReason, RowEdit, RowSlot,
    RowsUpdate, SlotRef, StoreLimits,
};
use neomacs_display_wire::rows::{RowKey, RowRuns};
use neomacs_display_wire::testing::{
    KEYWORD_FACE, TestFrame, assert_same_materialized, round_trip, transmit,
};
use proptest::prelude::*;

use crate::corpus::frame;

fn fresh() -> (FrameSyncEncoder, FrameSyncDecoder) {
    (
        FrameSyncEncoder::new(StoreLimits::default()),
        FrameSyncDecoder::new(StoreLimits::default()),
    )
}

fn source_text(lines: usize, width: usize) -> String {
    (0..lines)
        .map(|line| {
            let words = ["fn", "Let", "x", "MATCH", "value", "=", "1;", "// note"];
            let mut text = String::new();
            let mut word = line;
            while text.chars().count() < width.saturating_sub(8) + line % 9 {
                text.push_str(words[word % words.len()]);
                text.push(' ');
                word += 3;
            }
            text
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn check(
    encoder: &mut FrameSyncEncoder,
    decoder: &mut FrameSyncDecoder,
    presentation: &SealedFramePresentation,
    context: &str,
) -> usize {
    let (size, decoded) = round_trip(encoder, decoder, frame(), presentation);
    let decoded = decoded.unwrap_or_else(|reason| panic!("{context}: rejected: {reason}"));
    assert_same_materialized(&presentation.materialize(), &decoded.materialize(), context);
    size.bytes
}

#[derive(Clone, Copy, Debug)]
enum Step {
    Edit { line: usize, col: usize, ch: char },
    NewLine { line: usize },
    Scroll { window: usize, lines: isize },
    Resize { cols: usize, rows: usize },
    Split,
    Nothing,
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        6 => (0usize..40, 0usize..60, prop::char::range('a', 'z'))
            .prop_map(|(line, col, ch)| Step::Edit { line, col, ch }),
        2 => (0usize..40).prop_map(|line| Step::NewLine { line }),
        3 => (0usize..3, -3isize..4).prop_map(|(window, lines)| Step::Scroll { window, lines }),
        1 => (20usize..90, 6usize..30).prop_map(|(cols, rows)| Step::Resize { cols, rows }),
        1 => Just(Step::Split),
        1 => Just(Step::Nothing),
    ]
}

fn apply(source: &mut TestFrame, step: Step) {
    match step {
        Step::Edit { line, col, ch } => {
            let line = line % source.lines.len();
            let col = col.min(source.lines[line].chars().count());
            source.insert_char(line, col, ch);
        }
        Step::NewLine { line } => {
            let line = line % (source.lines.len() + 1);
            source.insert_line(line, "inserted LINE");
        }
        Step::Scroll { window, lines } => {
            let window = window % source.windows.len();
            source.scroll(window, lines);
        }
        Step::Resize { cols, rows } => source.resize(cols, rows),
        Step::Split => {
            if source.windows.len() < 3 && source.windows[0].height >= 6 {
                source.split();
            }
        }
        Step::Nothing => {}
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// Random edit, scroll, resize and split sequences: after every
    /// presentation the receiver materializes exactly what the sender does.
    #[test]
    fn framesync_sequence_reconstructs_canonically(steps in prop::collection::vec(step(), 1..24)) {
        let mut source = TestFrame::new(60, 18, &source_text(40, 50));
        let (mut encoder, mut decoder) = fresh();
        let first = source.present();
        check(&mut encoder, &mut decoder, &first, "keyframe");
        for (index, step) in steps.into_iter().enumerate() {
            apply(&mut source, step);
            let presentation = source.present();
            check(&mut encoder, &mut decoder, &presentation, &format!("step {index}: {step:?}"));
        }
    }
}

#[test]
fn discarded_updates_still_update_the_mirror() {
    let mut source = TestFrame::new(40, 10, &source_text(20, 30));
    let (mut encoder, mut decoder) = fresh();
    check(&mut encoder, &mut decoder, &source.present(), "keyframe");

    // The receiver applies a presentation and then discards it (superseded):
    // its rows still entered the store, as the sender's mirror assumes.
    source.insert_line(2, "A ROW ONLY THE DISCARDED PRESENTATION DEFINED");
    let discarded = source.present();
    let ops = encoder.encode(frame(), &discarded).unwrap();
    let defined: Vec<RowSlot> = ops
        .iter()
        .filter_map(|op| match op {
            PresentOp::DefineRow { slot, .. } => Some(*slot),
            _ => None,
        })
        .collect();
    assert!(!defined.is_empty());
    let (_, outcome) = transmit(&WireCodec::default(), ops, &mut decoder);
    drop(outcome.expect("accepted, then discarded by the receiver"));

    // The next presentation reuses that row by slot and defines nothing new
    // for it; the receiver resolves it from its store.
    source.insert_char(0, 0, 'q');
    let next = source.present();
    let ops = encoder.encode(frame(), &next).unwrap();
    let redefined = ops
        .iter()
        .filter(|op| matches!(op, PresentOp::DefineRow { slot, .. } if defined.contains(slot)))
        .count();
    assert_eq!(
        redefined, 0,
        "rows the receiver holds are referenced, not resent"
    );
    let (_, decoded) = transmit(&WireCodec::default(), ops, &mut decoder);
    let decoded = decoded.unwrap().expect("committed").presentation;
    assert_same_materialized(
        &next.materialize(),
        &decoded.materialize(),
        "after a discard",
    );
}

fn keyframe_ops() -> Vec<PresentOp> {
    let mut source = TestFrame::new(40, 8, &source_text(10, 30));
    FrameSyncEncoder::new(StoreLimits::default())
        .encode(frame(), &source.present())
        .unwrap()
}

/// A keyframe whose first window exports scroll coverage with four
/// prefetched lines below its seven text rows.
fn coverage_keyframe() -> SealedFramePresentation {
    TestFrame::new(40, 8, &source_text(20, 30))
        .with_scroll_coverage(4)
        .present()
}

fn coverage_keyframe_ops() -> Vec<PresentOp> {
    FrameSyncEncoder::new(StoreLimits::default())
        .encode(frame(), &coverage_keyframe())
        .unwrap()
}

/// A slot only the scroll coverage references (a prefetched line).
fn coverage_only_slot(ops: &[PresentOp]) -> Option<RowSlot> {
    let window_slots: BTreeSet<RowSlot> = ops
        .iter()
        .filter_map(|op| match op {
            PresentOp::Windows(update) => Some(update),
            _ => None,
        })
        .flat_map(|update| &update.changed)
        .flat_map(|layout| match &layout.rows {
            RowsUpdate::Script(script) => script
                .iter()
                .filter_map(|edit| match edit {
                    RowEdit::Place(slot_ref) => Some(slot_ref.slot),
                    RowEdit::Copy { .. } => None,
                })
                .collect(),
            RowsUpdate::Same => Vec::new(),
        })
        .collect();
    ops.iter()
        .find_map(|op| match op {
            PresentOp::Facts(facts) => match &facts.coverage {
                Changed::Set(coverage) => coverage.first(),
                Changed::Same => None,
            },
            _ => None,
        })?
        .rows
        .iter()
        .map(|row| row.slot)
        .find(|slot| !window_slots.contains(slot))
}

fn apply_all(ops: Vec<PresentOp>) -> Result<(), RejectReason> {
    let mut decoder = FrameSyncDecoder::new(StoreLimits::default());
    for op in ops {
        decoder.apply(op)?;
    }
    Ok(())
}

fn edit_windows(ops: &mut [PresentOp], edit: impl FnOnce(&mut Vec<RowEdit>)) {
    for op in ops {
        if let PresentOp::Windows(update) = op
            && let RowsUpdate::Script(script) = &mut update.changed[0].rows
        {
            edit(script);
            return;
        }
    }
    panic!("a keyframe has a window layout");
}

#[test]
fn bad_slot_ref_rejected() {
    // A reference to a slot that holds nothing.
    let mut ops = keyframe_ops();
    edit_windows(&mut ops, |script| {
        if let RowEdit::Place(slot_ref) = &mut script[0] {
            slot_ref.slot = RowSlot(999);
        }
    });
    assert_eq!(apply_all(ops), Err(RejectReason::UnknownSlot(999)));

    // A slot beyond the store bound.
    let mut ops = keyframe_ops();
    edit_windows(&mut ops, |script| {
        if let RowEdit::Place(slot_ref) = &mut script[0] {
            slot_ref.slot = RowSlot(u32::MAX);
        }
    });
    assert_eq!(apply_all(ops), Err(RejectReason::SlotOutOfRange(u32::MAX)));

    // A copy of rows a keyframe's (empty) previous layout does not have.
    let mut ops = keyframe_ops();
    edit_windows(&mut ops, |script| {
        script.insert(0, RowEdit::Copy { from: 0, len: 1 })
    });
    assert_eq!(apply_all(ops), Err(RejectReason::CopyOutOfRange));

    // A row whose bytes do not match its key.
    let mut ops = keyframe_ops();
    for op in &mut ops {
        if let PresentOp::DefineRow { key, .. } = op {
            *key = RowKey([0; 16]);
            break;
        }
    }
    assert_eq!(apply_all(ops), Err(RejectReason::KeyMismatch));
}

/// M2 review r1 (correctness m1): a refused window edit used to remove the
/// window from the frame while the frame's window order still named it, so
/// the `Commit` that followed panicked looking it up. A caller that keeps
/// applying after a refusal gets typed outcomes only.
#[test]
fn a_refused_window_edit_leaves_the_frame_whole() {
    let corruptions: [fn(SlotRef) -> RowEdit; 2] = [
        |_| RowEdit::Copy {
            from: u32::MAX - 1,
            len: 1,
        },
        |place| {
            RowEdit::Place(SlotRef {
                slot: RowSlot(u32::MAX),
                ..place
            })
        },
    ];
    for corrupt in corruptions {
        let mut source = TestFrame::new(40, 10, &source_text(20, 30));
        let (mut encoder, mut decoder) = fresh();
        check(&mut encoder, &mut decoder, &source.present(), "keyframe");
        source.insert_char(0, 0, 'q');
        let mut ops = encoder.encode(frame(), &source.present()).unwrap();
        let edit = corrupt(first_slot_ref(&ops));
        edit_windows(&mut ops, |script| script.insert(0, edit));
        let outcomes: Vec<_> = ops.into_iter().map(|op| decoder.apply(op)).collect();
        assert!(
            outcomes.iter().any(|outcome| matches!(
                outcome,
                Err(RejectReason::CopyOutOfRange | RejectReason::SlotOutOfRange(_))
            )),
            "{edit:?}: {outcomes:?}"
        );
    }
}

fn first_slot_ref(ops: &[PresentOp]) -> SlotRef {
    ops.iter()
        .find_map(|op| match op {
            PresentOp::Windows(update) => match &update.changed[0].rows {
                RowsUpdate::Script(script) => script.iter().find_map(|edit| match edit {
                    RowEdit::Place(slot_ref) => Some(*slot_ref),
                    RowEdit::Copy { .. } => None,
                }),
                RowsUpdate::Same => None,
            },
            _ => None,
        })
        .expect("a delta places a row")
}

/// M1 follow-up 3: a presentation's input frontier names each input stream
/// once; a peer's frontier that repeats one is refused.
#[test]
fn a_frontier_repeating_a_stream_is_refused() {
    let snapshot = FrontierSnapshot::Indeterminate {
        stream: InputStreamId::FIRST,
        reflected_through: 3,
    };
    let mut ops = keyframe_ops();
    for op in &mut ops {
        if let PresentOp::Facts(facts) = op {
            facts.frontier = vec![snapshot.clone(), snapshot.clone()];
        }
    }
    assert_eq!(
        apply_all(ops),
        Err(RejectReason::FrontierRepeatsStream(
            InputStreamId::FIRST.get()
        ))
    );
    // One snapshot per stream is accepted.
    let mut ops = keyframe_ops();
    for op in &mut ops {
        if let PresentOp::Facts(facts) = op {
            facts.frontier = vec![snapshot.clone()];
        }
    }
    assert_eq!(apply_all(ops), Ok(()));
}

#[test]
fn missing_face_rejected() {
    let mut ops = keyframe_ops();
    for op in &mut ops {
        if let PresentOp::Tables(tables) = op {
            tables.faces.upsert.retain(|(id, _)| *id != KEYWORD_FACE);
        }
    }
    assert_eq!(
        apply_all(ops),
        Err(RejectReason::MissingFace(KEYWORD_FACE.get()))
    );
}

/// One structural mutation of a decoded presentation.
#[derive(Clone, Debug)]
enum Mutation {
    /// Replace a float in the row content with arbitrary bits.
    RowFloat { field: u8, bits: u32 },
    /// Turn a character glyph into an xwidget with arbitrary geometry.
    Xwidget { width: u32, offset: u32 },
    /// The same, in a row only the scroll coverage references.
    CoverageXwidget { width: u32, offset: u32 },
    /// Arbitrary frame scalars.
    Scalars { char_width: u32, height: u32 },
    /// Arbitrary window geometry.
    Geometry { x: u32, width: u32 },
    /// An arbitrary row placement top.
    Top { bits: u32 },
}

/// The bits of an `f32`: arbitrary, or often one of the values geometry
/// checks trip on (a uniform draw is almost never NaN or infinite).
fn float_bits() -> impl Strategy<Value = u32> {
    prop_oneof![
        3 => any::<u32>(),
        2 => prop::sample::select(vec![
            f32::NAN.to_bits(),
            f32::INFINITY.to_bits(),
            f32::NEG_INFINITY.to_bits(),
            0.0f32.to_bits(),
            (-0.0f32).to_bits(),
            f32::MAX.to_bits(),
            f32::MIN_POSITIVE.to_bits(),
            8.0f32.to_bits(),
        ]),
    ]
}

fn mutation() -> impl Strategy<Value = Mutation> {
    prop_oneof![
        (0u8..6, float_bits()).prop_map(|(field, bits)| Mutation::RowFloat { field, bits }),
        (float_bits(), float_bits())
            .prop_map(|(width, offset)| Mutation::Xwidget { width, offset }),
        (float_bits(), float_bits())
            .prop_map(|(width, offset)| Mutation::CoverageXwidget { width, offset }),
        (float_bits(), float_bits())
            .prop_map(|(char_width, height)| Mutation::Scalars { char_width, height }),
        (float_bits(), float_bits()).prop_map(|(x, width)| Mutation::Geometry { x, width }),
        float_bits().prop_map(|bits| Mutation::Top { bits }),
    ]
}

/// Turn the first text-area glyph of a row into an xwidget with the given
/// advance and vertical offset bits; `false` when the row has no glyph.
fn xwidget_first_glyph(
    key: &mut RowKey,
    row: &mut Payload<RowRuns>,
    width: u32,
    offset: u32,
) -> bool {
    let mut runs = (*row.value().unwrap()).clone();
    let area = &mut runs.areas[1];
    if area.len == 0 || area.text.is_empty() {
        return false;
    }
    area.text.remove(0);
    area.kinds[0].len -= 1;
    let rest = std::mem::take(&mut area.kinds);
    area.kinds.push(neomacs_display_wire::rows::Run {
        len: 1,
        value: neomacs_display_wire::rows::GlyphKind::Other,
    });
    area.kinds
        .extend(rest.into_iter().filter(|run| run.len > 0));
    area.others.insert(
        0,
        GlyphType::Xwidget {
            xwidget_id: XwidgetId::new(1),
            webview_id: WebViewId::new(1),
            width_cols: 1,
            content: XwidgetContentExtent::new(8.0, 8.0).unwrap(),
        },
    );
    area.advance_palette[0] = f32::from_bits(width);
    area.voffsets[0].value = f32::from_bits(offset);
    let (new_key, _) = runs.keyed_bytes().unwrap();
    *key = new_key;
    *row = Payload::Shared(Arc::new(runs));
    true
}

fn mutate(ops: &mut [PresentOp], mutation: &Mutation) {
    let coverage_slot = coverage_only_slot(ops);
    for op in ops.iter_mut() {
        match (op, mutation) {
            (PresentOp::DefineRow { key, row, .. }, Mutation::RowFloat { field, bits }) => {
                let mut runs = (*row.value().unwrap()).clone();
                let value = f32::from_bits(*bits);
                let area = &mut runs.areas[1];
                match field {
                    0 => runs.pixel_x = value,
                    1 => runs.height_px = value,
                    2 => runs.ascent_px = value,
                    3 => area
                        .advance_palette
                        .iter_mut()
                        .for_each(|width| *width = value),
                    4 => area.voffsets.iter_mut().for_each(|run| run.value = value),
                    _ => area.heights.iter_mut().for_each(|run| run.value = value),
                }
                let (new_key, _) = runs.keyed_bytes().unwrap();
                *key = new_key;
                *row = Payload::Shared(Arc::new(runs));
                return;
            }
            (PresentOp::DefineRow { key, row, .. }, Mutation::Xwidget { width, offset }) => {
                if xwidget_first_glyph(key, row, *width, *offset) {
                    return;
                }
            }
            (
                PresentOp::DefineRow { slot, key, row },
                Mutation::CoverageXwidget { width, offset },
            ) if Some(*slot) == coverage_slot => {
                xwidget_first_glyph(key, row, *width, *offset);
                return;
            }
            (PresentOp::Facts(facts), Mutation::Scalars { char_width, height }) => {
                if let Changed::Set(scalars) = &mut facts.scalars {
                    scalars.char_width = f32::from_bits(*char_width);
                    scalars.frame_pixel_height = f32::from_bits(*height);
                }
                return;
            }
            (PresentOp::Windows(update), Mutation::Geometry { x, width }) => {
                if let Changed::Set(geometry) = &mut update.changed[0].geometry {
                    geometry.pixel_bounds.x = f32::from_bits(*x);
                    geometry.text_pixel_bounds.width = f32::from_bits(*width);
                }
                return;
            }
            (PresentOp::Windows(update), Mutation::Top { bits }) => {
                if let RowsUpdate::Script(script) = &mut update.changed[0].rows
                    && let Some(RowEdit::Place(slot_ref)) = script.get_mut(1)
                {
                    slot_ref.y = neomacs_display_wire::present::YRef::At(
                        neomacs_display_wire::present::F32Bits(*bits),
                    );
                }
                return;
            }
            _ => {}
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Whatever a receiver accepts, it can materialize: no peer frame can
    /// panic the frontend, with or without scroll coverage.
    #[test]
    fn materialize_never_panics_on_accepted_state(
        with_coverage in any::<bool>(),
        mutations in prop::collection::vec(mutation(), 1..4),
    ) {
        let mut ops = if with_coverage { coverage_keyframe_ops() } else { keyframe_ops() };
        for mutation in &mutations {
            mutate(&mut ops, mutation);
        }
        let mut decoder = FrameSyncDecoder::new(StoreLimits::default());
        let (_, outcome) = transmit(&WireCodec::default(), ops, &mut decoder);
        if let Ok(Some(decoded)) = outcome {
            let materialized = catch_unwind(AssertUnwindSafe(|| decoded.presentation.materialize()));
            prop_assert!(materialized.is_ok(), "an accepted frame panicked in materialize: {:?}", mutations);
        }
    }
}

#[test]
fn size_budget_200x60() {
    // u1's scenario: a 200×60 frame of about 7,200 body glyphs.
    let mut source = TestFrame::new(200, 60, &source_text(80, 120));
    let (mut encoder, mut decoder) = fresh();
    let keyframe = check(&mut encoder, &mut decoder, &source.present(), "keyframe");
    source.insert_char(10, 7, 'x');
    let edit = check(
        &mut encoder,
        &mut decoder,
        &source.present(),
        "one character",
    );
    source.scroll(0, 1);
    let scroll = check(
        &mut encoder,
        &mut decoder,
        &source.present(),
        "one line of scroll",
    );
    println!(
        "200x60: keyframe {keyframe} B, one-character edit {edit} B, one-line scroll {scroll} B"
    );
    assert!(keyframe <= 64 * 1024, "keyframe {keyframe} B > 64 KiB");
    assert!(edit <= 1024, "one-character edit {edit} B > 1 KiB");
}

/// The property above is not vacuous: frames with non-finite values in
/// most fields are accepted (and materialize), and the ones the glyph walk
/// cannot place are refused with a typed reason.
#[test]
fn accepted_and_refused_mutations_are_typed() {
    let accepted = [
        Mutation::RowFloat {
            field: 0,
            bits: 1.5f32.to_bits(),
        },
        Mutation::RowFloat {
            field: 3,
            bits: f32::NAN.to_bits(),
        },
        Mutation::RowFloat {
            field: 1,
            bits: f32::INFINITY.to_bits(),
        },
        Mutation::Xwidget {
            width: 8.0f32.to_bits(),
            offset: 0.0f32.to_bits(),
        },
        Mutation::Scalars {
            char_width: 0.0f32.to_bits(),
            height: 100.0f32.to_bits(),
        },
        Mutation::Geometry {
            x: f32::NAN.to_bits(),
            width: 1.0f32.to_bits(),
        },
        Mutation::Top {
            bits: f32::NEG_INFINITY.to_bits(),
        },
    ];
    for mutation in accepted {
        let mut ops = keyframe_ops();
        mutate(&mut ops, &mutation);
        let mut decoder = FrameSyncDecoder::new(StoreLimits::default());
        let (_, outcome) = transmit(&WireCodec::default(), ops, &mut decoder);
        let decoded = outcome.unwrap_or_else(|reason| panic!("{mutation:?}: {reason}"));
        let presentation = decoded.expect("committed").presentation;
        assert!(catch_unwind(AssertUnwindSafe(|| presentation.materialize())).is_ok());
    }
    let mut ops = keyframe_ops();
    mutate(
        &mut ops,
        &Mutation::Xwidget {
            width: 8.0f32.to_bits(),
            offset: f32::INFINITY.to_bits(),
        },
    );
    let mut decoder = FrameSyncDecoder::new(StoreLimits::default());
    let (_, outcome) = transmit(&WireCodec::default(), ops, &mut decoder);
    assert!(
        matches!(outcome, Err(RejectReason::Materialize(_))),
        "{:?}",
        outcome.map(|decoded| decoded.is_some())
    );
}

#[test]
fn framesync_carries_scroll_coverage() {
    let mut source = TestFrame::new(40, 8, &source_text(20, 30)).with_scroll_coverage(4);
    let (mut encoder, mut decoder) = fresh();
    let keyframe = source.present();
    assert_eq!(keyframe.materialize().scroll_surfaces.len(), 1);
    check(&mut encoder, &mut decoder, &keyframe, "coverage keyframe");
    source.scroll(0, 2);
    let scrolled = source.present();
    assert_eq!(scrolled.materialize().scroll_surfaces.len(), 1);
    check(
        &mut encoder,
        &mut decoder,
        &scrolled,
        "coverage after a scroll",
    );
}

/// M2 review r1 (compat 1): a peer's coverage row whose glyph the walk
/// cannot place used to panic `validate_for_materialize` at `Commit`
/// ("an xwidget origin ... is not finite"). It refuses the coverage; the
/// frame itself is accepted and materializes without it.
#[test]
fn coverage_glyph_the_walk_cannot_place_is_refused_not_a_panic() {
    for (width, offset) in [(8.0f32, f32::NAN), (f32::INFINITY, 0.0)] {
        let mut ops = coverage_keyframe_ops();
        assert!(coverage_only_slot(&ops).is_some());
        mutate(
            &mut ops,
            &Mutation::CoverageXwidget {
                width: width.to_bits(),
                offset: offset.to_bits(),
            },
        );
        let mut decoder = FrameSyncDecoder::new(StoreLimits::default());
        let (_, outcome) = transmit(&WireCodec::default(), ops, &mut decoder);
        let decoded = outcome
            .unwrap_or_else(|reason| panic!("{width} {offset}: {reason}"))
            .expect("committed");
        let materialized = decoded.presentation.materialize();
        assert!(materialized.scroll_surfaces.is_empty(), "{width} {offset}");
        assert!(!materialized.glyphs.is_empty());
    }
}
