use neomacs_app::frontend_event::FrontendLogicalExtent;
use neomacs_app::frontend_event::{
    FrontendEvent, FrontendFrameId, FrontendKeyState, FrontendKeySymbol, FrontendPresentationId,
};
use neomacs_wasm_protocol::{
    BrowserColorScheme, BrowserEditorStartup, BrowserInputBatch, BrowserInputEvent,
    BrowserKeyState, BrowserModifiers, InputBatchSequence, InvalidBrowserEditorStartup,
    InvalidBrowserInputBatch, WORKER_PROTOCOL_VERSION,
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
            "sequence": "8",
            "events": [
                {"type":"focus-changed", "focused":true, "target":"1"},
                {"type":"viewport-changed", "width":800, "height":600,
                 "scale_factor":0.0, "target":"1"}
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

#[test]
fn browser_editor_startup_validates_the_complete_initial_surface() {
    let startup = BrowserEditorStartup::new(
        FrontendLogicalExtent::new(1280, 720),
        2.0,
        8.0,
        16.0,
        16.0,
        BrowserColorScheme::Dark,
    )
    .expect("valid browser startup");

    assert_eq!(startup.protocol_version(), WORKER_PROTOCOL_VERSION);
    assert_eq!(
        startup.logical_extent(),
        FrontendLogicalExtent::new(1280, 720)
    );
    assert_eq!(startup.scale_factor(), 2.0);
    assert_eq!(startup.character_size(), (8.0, 16.0));
    assert_eq!(startup.font_pixel_size(), 16.0);
    assert_eq!(startup.color_scheme(), BrowserColorScheme::Dark);
}

#[test]
fn browser_editor_startup_rejects_invalid_geometry_before_worker_boot() {
    assert_eq!(
        BrowserEditorStartup::new(
            FrontendLogicalExtent::new(1280, 0),
            2.0,
            8.0,
            16.0,
            16.0,
            BrowserColorScheme::Light,
        ),
        Err(InvalidBrowserEditorStartup::EmptyExtent),
    );
    assert_eq!(
        BrowserEditorStartup::new(
            FrontendLogicalExtent::new(1280, 720),
            f64::NAN,
            8.0,
            16.0,
            16.0,
            BrowserColorScheme::Light,
        ),
        Err(InvalidBrowserEditorStartup::ScaleFactor),
    );
}

#[test]
fn browser_wire_preserves_identities_beyond_javascript_safe_integers() {
    const BEYOND_JS_SAFE_INTEGER: u64 = 9_007_199_254_740_993;
    let batch = BrowserInputBatch::new(
        InputBatchSequence::new(BEYOND_JS_SAFE_INTEGER).expect("positive sequence"),
        vec![
            BrowserInputEvent::key(
                0xff51,
                BrowserModifiers::default(),
                BrowserKeyState::Pressed,
                BEYOND_JS_SAFE_INTEGER + 1,
            ),
            BrowserInputEvent::presentation_retired(BEYOND_JS_SAFE_INTEGER + 2),
        ],
    )
    .expect("nonempty browser batch");

    let json = serde_json::to_string(&batch).expect("serializable browser batch");
    assert!(json.contains(r#""sequence":"9007199254740993""#));
    assert!(json.contains(r#""target":"9007199254740994""#));
    assert!(json.contains(r#""presentation":"9007199254740995""#));

    let restored: BrowserInputBatch =
        serde_json::from_str(&json).expect("lossless browser batch round trip");
    assert_eq!(restored, batch);
}

#[test]
fn browser_wire_rejects_numeric_identifiers_that_javascript_could_round() {
    assert!(
        serde_json::from_str::<BrowserInputBatch>(
            r#"{"sequence":1,"events":[{"type":"close-requested","target":"1"}]}"#,
        )
        .is_err()
    );
    assert!(
        serde_json::from_str::<BrowserInputBatch>(
            r#"{"sequence":"1","events":[{"type":"close-requested","target":1}]}"#,
        )
        .is_err()
    );
}
