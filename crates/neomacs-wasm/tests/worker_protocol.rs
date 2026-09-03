use neomacs_app::frontend_event::{
    FrontendEvent, FrontendFrameId, FrontendKeyState, FrontendKeySymbol, FrontendPresentationId,
};
use neomacs_wasm::worker_protocol::{
    BrowserInputBatch, BrowserInputEvent, BrowserKeyState, BrowserModifiers, InputBatchSequence,
    InvalidBrowserInputBatch,
};

#[test]
fn browser_batch_becomes_one_ordered_editor_input_batch() {
    let sequence = InputBatchSequence::new(7).expect("positive sequence");
    let batch = BrowserInputBatch::new(
        sequence,
        vec![
            BrowserInputEvent::key(
                0xff51,
                BrowserModifiers::new(false, true, true, false),
                BrowserKeyState::Pressed,
                41,
            ),
            BrowserInputEvent::text_committed("λ🙂", 41),
            BrowserInputEvent::presentation_retired(99),
        ],
    )
    .expect("nonempty browser batch");

    let batch = batch
        .try_into_frontend_batch()
        .expect("valid browser observations");

    assert_eq!(batch.sequence(), sequence);
    assert_eq!(
        batch.events(),
        &[
            FrontendEvent::Key(neomacs_app::frontend_event::FrontendKeyEvent::new(
                FrontendKeySymbol::new(0xff51),
                neomacs_app::frontend_event::FrontendModifiers::new(false, true, true, false),
                FrontendKeyState::Pressed,
                FrontendFrameId::new(41),
            )),
            FrontendEvent::text_committed("λ🙂", FrontendFrameId::new(41)),
            FrontendEvent::PresentationRetired {
                presentation: FrontendPresentationId::new(99),
            },
        ]
    );
}

#[test]
fn browser_batch_rejects_invalid_viewport_before_exposing_any_editor_events() {
    let batch: BrowserInputBatch = serde_json::from_str(
        r#"{
            "sequence": 8,
            "events": [
                {"type":"focus-changed", "focused":true, "target":1},
                {"type":"viewport-changed", "width":800, "height":600,
                 "scale_factor":0.0, "target":1}
            ]
        }"#,
    )
    .expect("wire shape is syntactically valid");

    assert_eq!(
        batch.try_into_frontend_batch(),
        Err(InvalidBrowserInputBatch::InvalidScaleFactor { event_index: 1 })
    );
}

#[test]
fn browser_batch_requires_a_nonzero_sequence_and_at_least_one_event() {
    assert!(InputBatchSequence::new(0).is_err());
    assert_eq!(
        BrowserInputBatch::new(
            InputBatchSequence::new(1).expect("positive sequence"),
            Vec::new(),
        ),
        Err(InvalidBrowserInputBatch::Empty)
    );
}
