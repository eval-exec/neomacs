use super::*;
use neovm_core::emacs_core::Context;
use neovm_core::emacs_core::emacs_char::MAX_5_BYTE_CHAR;
use neovm_core::emacs_core::value::StringTextPropertyRun;
use neovm_core::heap_types::LispString;

fn frame(value: Value) -> LispStringSourceFrame {
    LispStringSourceFrame::new(
        7,
        value,
        RenderFaceRef::Inherit,
        LispStringSourceOrigin::Normal,
    )
    .expect("native string fixture")
}

fn compare_run(
    source: &LispStringSourceFrame,
    bounds: CharRange,
    tty: TtyGlyphlessCharDisplay,
) -> LispStringOrdinaryRun {
    let decoded = source.ordinary_text_run(bounds, tty, LispStringRunTextPolicy::Decode);
    let copied = source.ordinary_text_run(bounds, tty, LispStringRunTextPolicy::ValidatedBytes);
    assert_eq!(copied.end, decoded.end, "bounds {bounds:?}");
    assert_eq!(copied.text, decoded.text, "bounds {bounds:?}");
    assert_eq!(
        source.span(bounds.start().get(), copied.end.get()),
        source.span(bounds.start().get(), decoded.end.get()),
        "source coordinates for bounds {bounds:?}"
    );
    copied
}

#[test]
fn ordinary_run_text_policies_agree_for_every_ordinary_subrange() {
    let _eval = Context::new();
    for value in [
        Value::string(""),
        Value::string("plain ASCII text"),
        Value::string("aéλ文🙂z"),
        Value::heap_string(LispString::from_unibyte(b"ASCII bytes".to_vec())),
    ] {
        let source = frame(value);
        for start in 0..=source.char_count() {
            for end in start..=source.char_count() {
                let bounds = CharRange::new(CharPos0::new(start), CharPos0::new(end));
                let run = compare_run(&source, bounds, TtyGlyphlessCharDisplay::default());
                assert_eq!(run.end, bounds.end());
                assert_eq!(run.text, source.text_slice(start, end));
            }
        }
    }
}

#[test]
fn ordinary_run_text_preserves_prefixes_before_raw_and_extended_characters() {
    let _eval = Context::new();
    let characters = [
        EmacsChar::from_char('a'),
        EmacsChar::from_char('é'),
        EmacsChar::from_byte8(0x80),
        EmacsChar::from_char('b'),
        EmacsChar::from_code(0x11_0000).expect("extended four-byte code"),
        EmacsChar::from_char('λ'),
        EmacsChar::from_code(MAX_5_BYTE_CHAR).expect("extended five-byte code"),
        EmacsChar::from_char('z'),
    ];
    let bytes = characters
        .iter()
        .flat_map(|ch| ch.to_emacs_bytes())
        .collect();
    let multibyte = frame(Value::heap_string(LispString::from_emacs_bytes(bytes)));
    for start in [0, 1, 3, 5, 7] {
        compare_run(
            &multibyte,
            CharRange::new(CharPos0::new(start), CharPos0::new(characters.len())),
            TtyGlyphlessCharDisplay::default(),
        );
    }
    // C3 A9 is UTF-8 for é, but these unibyte bytes remain two raw bytes.
    let unibyte = frame(Value::heap_string(LispString::from_unibyte(vec![
        b'a', 0xc3, 0xa9, b'z',
    ])));
    let prefix = compare_run(
        &unibyte,
        CharRange::new(CharPos0::ZERO, CharPos0::new(4)),
        TtyGlyphlessCharDisplay::default(),
    );
    assert_eq!(prefix.end, CharPos0::new(1));
    compare_run(
        &unibyte,
        CharRange::new(CharPos0::new(3), CharPos0::new(4)),
        TtyGlyphlessCharDisplay::default(),
    );
}

#[test]
fn ordinary_run_text_noncanonical_bytes_keep_the_scalar_fallback() {
    let _eval = Context::new();
    // The existing permissive Emacs decoder accepts these overlong A forms.
    // Mutate only the frame's owned native fixture, not Lisp heap storage.
    for encoded in [
        &[0xe0, 0x81, 0x81][..],
        &[0xf0, 0x80, 0x81, 0x81][..],
        &[0xf8, 0x80, 0x80, 0x81, 0x81][..],
    ] {
        let mut source = frame(Value::string("aAz"));
        source.storage = EmacsTextStorage::Multibyte;
        source.text = [b"a".as_slice(), encoded, b"z".as_slice()].concat();
        source.char_byte_offsets = vec![0, 1, 1 + encoded.len(), 2 + encoded.len()];
        assert!(std::str::from_utf8(&source.text).is_err());
        let run = compare_run(
            &source,
            CharRange::new(CharPos0::ZERO, CharPos0::new(3)),
            TtyGlyphlessCharDisplay::default(),
        );
        assert_eq!(run.end, CharPos0::new(3));
        assert_eq!(
            source.span(0, run.end.get()).end,
            DisplaySourcePosition::lisp_string(7, 3, source.text.len())
        );
    }
}

#[test]
fn ordinary_run_text_obeys_property_and_automatic_composition_limits() {
    let _eval = Context::new();
    let value = Value::string_with_text_properties(
        "aéx́λz",
        vec![StringTextPropertyRun {
            start: 4,
            end: 5,
            plist: Value::list(vec![Value::symbol("face"), Value::symbol("bold")]),
        }],
    );
    let mut source = frame(value);
    source.automatic_compositions =
        LispStringCompositions::Resolved(vec![AutomaticCompositionSpan::new(2, 4)]);
    let property_end = source.next_property_change(0);
    let text_end = source
        .next_automatic_composition_start(0)
        .unwrap_or(property_end)
        .min(property_end);
    let run = compare_run(
        &source,
        CharRange::new(CharPos0::ZERO, CharPos0::new(text_end)),
        TtyGlyphlessCharDisplay::default(),
    );
    assert_eq!(run.end, CharPos0::new(2));
    assert_eq!(
        source.span(0, run.end.get()).end,
        DisplaySourcePosition::lisp_string(7, 2, 3)
    );
    for (start, end) in [(0, 1), (1, 2), (4, 5), (5, 6)] {
        compare_run(
            &source,
            CharRange::new(CharPos0::new(start), CharPos0::new(end)),
            TtyGlyphlessCharDisplay::default(),
        );
    }
}

#[test]
fn ordinary_run_text_stops_before_a_captured_terminal_mapping() {
    let mut eval = Context::new();
    let buffer_id = eval.buffer_manager().current_buffer().expect("buffer").id();
    let table = Value::make_char_table(Value::symbol("glyphless-char-display"), Value::NIL, 1);
    neovm_core::emacs_core::chartable::ct_set_single(
        &table,
        '▼' as i64,
        Value::cons(Value::NIL, Value::string("v")),
    );
    eval.buffer_manager_mut()
        .get_mut(buffer_id)
        .expect("buffer")
        .set_buffer_local("glyphless-char-display", table);
    let tty =
        TtyGlyphlessCharDisplay::capture(eval.buffer_manager().get(buffer_id).expect("buffer"));
    assert!(tty.method_for(EmacsChar::from_char('▼')).is_some());
    let source = frame(Value::string("ab▼cd"));
    let prefix = compare_run(
        &source,
        CharRange::new(CharPos0::ZERO, CharPos0::new(5)),
        tty,
    );
    assert_eq!(prefix.end, CharPos0::new(2));
    // Starting at the mapped ordinary scalar takes the legacy forced-one
    // branch. It must keep the old helper's advance and materialized text.
    let legacy = compare_run(
        &source,
        CharRange::new(CharPos0::new(2), CharPos0::new(5)),
        tty,
    );
    assert_eq!(legacy.end, CharPos0::new(3));
    compare_run(
        &source,
        CharRange::new(CharPos0::new(3), CharPos0::new(5)),
        tty,
    );
}

#[test]
fn ordinary_run_text_full_cursor_keeps_special_item_coordinates() {
    let _eval = Context::new();
    let value = Value::string("aé\nλ\u{7}文\u{feff}z");
    let source = frame(value);
    let mut cursor = LispStringSourceCursor::new(
        7,
        value,
        RenderFaceRef::Inherit,
        LispStringSourceOrigin::Normal,
    )
    .expect("string cursor");
    let mut context = DisplaySourceContext::empty();
    let mut previous_end = DisplaySourcePosition::lisp_string(7, 0, 0);
    let mut ordinary_items = 0;
    let mut special_items = 0;
    while let Some(item) = cursor.next_item(&mut context) {
        assert_eq!(item.span.start, previous_end);
        previous_end = item.span.end.clone();
        if let DisplayItemKind::TextRun(run) = &item.kind {
            ordinary_items += 1;
            let start = item
                .span
                .start
                .lisp_string_char_index()
                .expect("string start");
            let end = item.span.end.lisp_string_char_index().expect("string end");
            let decoded = source.ordinary_text_run(
                CharRange::new(CharPos0::new(start), CharPos0::new(end)),
                TtyGlyphlessCharDisplay::default(),
                LispStringRunTextPolicy::Decode,
            );
            assert_eq!(run.text.as_ref(), decoded.text);
            assert_eq!(item.span, source.span(start, decoded.end.get()));
        } else {
            special_items += 1;
        }
    }
    assert!(ordinary_items > 1);
    assert!(special_items > 1);
    assert_eq!(previous_end, source.span(0, source.char_count()).end);
}

#[test]
fn ordinary_run_text_special_legacy_branch_keeps_existing_outcome() {
    let _eval = Context::new();
    // The low-level scalar helper has an existing debug assertion for an
    // impossible special start. Compare its outcome rather than introducing
    // a new promise about malformed low-level calls in either build mode.
    for text in ["\n", "\u{7}", "\u{feff}"] {
        let source = frame(Value::string(text));
        let bounds = CharRange::new(CharPos0::ZERO, CharPos0::new(1));
        let call = |policy| {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let run =
                    source.ordinary_text_run(bounds, TtyGlyphlessCharDisplay::default(), policy);
                (run.end, run.text)
            }))
        };
        match (
            call(LispStringRunTextPolicy::Decode),
            call(LispStringRunTextPolicy::ValidatedBytes),
        ) {
            (Ok(decoded), Ok(copied)) => assert_eq!(copied, decoded),
            (Err(_), Err(_)) => {}
            _ => panic!("materialization policy changed the legacy special-start outcome"),
        }
    }
}
