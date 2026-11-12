//! Fresh-query transport, not a model of GNU's sticky live-frame entity cache.
use super::*;
use neovm_eval::emacs_core::eval::{FontScriptCoverage as Coverage, FontSpecListQuery};
type Requests = Rc<RefCell<Vec<(FontSpecListQuery, Option<FontSpecListQuery>)>>>;
struct RecordingHost(Requests);
impl DisplayHost for RecordingHost {
    fn realize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }
    fn resize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }
    fn list_fonts_for_spec(
        &mut self,
        query: FontSpecListQuery,
        prefer: Option<FontSpecListQuery>,
        _: Option<usize>,
        _: Option<&mut neovm_eval::emacs_core::eval::FontListFilter<'_>>,
    ) -> Result<
        Vec<neovm_eval::emacs_core::eval::ListedFontEntity>,
        neovm_eval::emacs_core::eval::FontListError,
    > {
        self.0.borrow_mut().push((query, prefer));
        Ok(Vec::new())
    }
}
fn context() -> (Context, Requests) {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    ensure_selected_gui_frame(&mut eval);
    let requests = Rc::new(RefCell::new(Vec::new()));
    eval.set_display_host(Box::new(RecordingHost(Rc::clone(&requests))));
    (eval, requests)
}
fn spec(script: Option<Value>) -> Value {
    let mut fields = vec![Value::keyword("family"), Value::string("Hack")];
    if let Some(script) = script {
        fields.extend([Value::keyword("script"), script]);
    }
    font_spec(fields).unwrap()
}
fn table(eval: &mut Context, script: Value, representatives: Value) {
    eval.set_variable(
        "script-representative-chars",
        Value::list(vec![Value::cons(script, representatives)]),
    );
}
fn capture(eval: &mut Context, requests: &Requests, spec: Value) -> Option<Coverage> {
    list_fonts(eval, vec![spec]).unwrap();
    requests.borrow().last().unwrap().0.script_coverage.clone()
}

#[test]
fn script_capture_preserves_list_and_vector_quantifiers_and_character_domains() {
    for case in 0..7 {
        let (mut eval, requests) = context();
        let script = Value::symbol("fresh-script");
        let (representatives, expected) = match case {
            0 => (
                Value::list(vec![Value::fixnum(65), Value::fixnum(0x4e00)]),
                Some(Coverage::All(vec![65, 0x4e00])),
            ),
            1 => (
                Value::vector(vec![Value::fixnum(65), Value::fixnum(0x4e00)]),
                Some(Coverage::Any(vec![65, 0x4e00])),
            ),
            2 => (Value::vector(Vec::new()), Some(Coverage::Any(Vec::new()))),
            3 => (Value::NIL, None),
            4 => (
                Value::list(vec![
                    Value::fixnum(0x400000),
                    Value::fixnum(65),
                    Value::symbol("ignored"),
                    Value::fixnum(0x3fffff),
                ]),
                Some(Coverage::All(vec![65, 0x3fffff])),
            ),
            5 => (
                Value::vector(vec![
                    Value::fixnum(-1),
                    Value::fixnum(u32::MAX as i64),
                    Value::fixnum(u32::MAX as i64 + 1),
                    Value::symbol("ignored"),
                    Value::fixnum(0x110000),
                    Value::fixnum(65),
                ]),
                Some(Coverage::Any(vec![u32::MAX, 0x110000, 65])),
            ),
            _ => (
                Value::cons(Value::fixnum(65), Value::symbol("improper-tail")),
                Some(Coverage::All(vec![65])),
            ),
        };
        table(&mut eval, script, representatives);
        assert_eq!(
            capture(&mut eval, &requests, spec(Some(script))),
            expected,
            "case {case}"
        );
    }
}

#[test]
fn script_capture_uses_association_identity_and_the_first_entry() {
    for same_identity in [false, true] {
        let (mut eval, requests) = context();
        let uninterned = Value::from_sym_id(neovm_value::intern::intern_uninterned("han"));
        let key = if same_identity {
            uninterned
        } else {
            Value::symbol("han")
        };
        table(&mut eval, key, Value::vector(vec![Value::fixnum(0x4e00)]));
        assert_eq!(
            capture(&mut eval, &requests, spec(Some(uninterned))),
            same_identity.then(|| Coverage::Any(vec![0x4e00]))
        );
    }
    let (mut eval, requests) = context();
    let script = Value::symbol("first-entry");
    eval.set_variable(
        "script-representative-chars",
        Value::list(vec![
            Value::cons(script, Value::NIL),
            Value::cons(script, Value::vector(vec![Value::fixnum(0x4e00)])),
        ]),
    );
    assert_eq!(capture(&mut eval, &requests, spec(Some(script))), None);
}

#[test]
fn script_capture_distinguishes_an_absent_property_from_explicit_nil() {
    for case in 0..3 {
        let (mut eval, requests) = context();
        let representatives = if case == 2 {
            Value::list(vec![Value::fixnum(0x4e00)])
        } else {
            Value::vector(vec![Value::fixnum(0x4e00)])
        };
        table(&mut eval, Value::NIL, representatives);
        let script = if case == 0 { None } else { Some(Value::NIL) };
        assert_eq!(
            capture(&mut eval, &requests, spec(script)),
            if case == 1 {
                Some(Coverage::Any(vec![0x4e00]))
            } else {
                None
            }
        );
    }
}

#[test]
fn script_capture_owns_codepoints_and_does_not_filter_by_preference() {
    let (mut eval, requests) = context();
    let script = Value::symbol("owned-vector");
    let representatives = Value::vector(vec![Value::fixnum(65)]);
    table(&mut eval, script, representatives);
    let primary = spec(Some(script));
    list_fonts(
        &mut eval,
        vec![primary, Value::NIL, Value::fixnum(1), primary],
    )
    .unwrap();
    assert_eq!(
        requests.borrow()[0].0.script_coverage,
        Some(Coverage::Any(vec![65]))
    );
    assert_eq!(
        requests.borrow()[0].1.as_ref().unwrap().script_coverage,
        None
    );
    assert!(representatives.replace_vector_data(vec![Value::fixnum(0x4e00)]));
    assert_eq!(
        requests.borrow()[0].0.script_coverage,
        Some(Coverage::Any(vec![65])),
        "host request owns its snapshot; this does not assert GNU cache invalidation"
    );
}

#[test]
fn script_capture_preserves_early_num_and_tty_validation_without_host_requests() {
    let (mut eval, requests) = context();
    let gui = eval.frame_manager().selected_frame().unwrap().id;
    let buffer = eval.buffer_manager_mut().create_buffer("*script-tty*");
    let tty = eval
        .frame_manager_mut()
        .create_frame_on_terminal("script-tty", 1, 80, 25, buffer);
    eval.frame_manager_mut()
        .get_mut(tty)
        .unwrap()
        .set_window_system(None);
    eval.set_variable("script-representative-chars", Value::fixnum(42));
    let primary = spec(Some(Value::symbol("malformed-table")));
    assert!(
        list_fonts(
            &mut eval,
            vec![
                primary,
                Value::make_frame(gui.0),
                Value::fixnum(0),
                Value::T
            ]
        )
        .unwrap()
        .is_nil()
    );
    assert!(
        list_fonts(&mut eval, vec![primary, Value::make_frame(tty.0)])
            .unwrap()
            .is_nil()
    );
    assert!(requests.borrow().is_empty());
}

#[test]
fn scalable_capture_preserves_absent_nil_and_truthy_property_values() {
    for case in 0..6 {
        let (mut eval, requests) = context();
        let mut fields = vec![Value::keyword("family"), Value::string("Hack")];
        let expected = if case == 0 { None } else { Some(case != 1) };
        if case != 0 {
            let value = match case {
                1 => Value::NIL,
                2 => Value::T,
                3 => Value::fixnum(0),
                4 => Value::vector(Vec::new()),
                _ => Value::string("false"),
            };
            fields.extend([Value::keyword("scalable"), value]);
        }
        let primary = font_spec(fields).unwrap();
        list_fonts(&mut eval, vec![primary]).unwrap();
        assert_eq!(requests.borrow()[0].0.scalable, expected, "case {case}");
    }
}

#[test]
fn scalable_capture_ignores_preferences_and_preserves_explicit_nil_with_avgwidth_zero() {
    let (mut eval, requests) = context();
    let primary = font_spec(vec![
        Value::keyword("family"),
        Value::string("Hack"),
        Value::keyword("avgwidth"),
        Value::fixnum(0),
        Value::keyword("scalable"),
        Value::NIL,
    ])
    .unwrap();
    let prefer = font_spec(vec![Value::keyword("scalable"), Value::T]).unwrap();
    list_fonts(
        &mut eval,
        vec![primary, Value::NIL, Value::fixnum(1), prefer],
    )
    .unwrap();
    assert_eq!(requests.borrow()[0].0.scalable, Some(false));
    assert_eq!(requests.borrow()[0].0.average_width, Some(0));
    assert_eq!(requests.borrow()[0].1.as_ref().unwrap().scalable, None);
}

#[test]
fn scalable_name_properties_preserve_gnu_boolean_spelling_rules() {
    for (word, expected) in [
        ("false", false),
        ("False", false),
        ("FALSE", false),
        ("FcFalse", false),
        ("off", false),
        ("OFF", false),
        ("Off", false),
        ("0", false),
        ("00", false),
        ("true", true),
        ("fAlSe", true),
        ("no", true),
        ("nil", true),
        ("0.0", true),
        ("-0", true),
        ("", true),
    ] {
        let (mut eval, requests) = context();
        let name = format!("Hack:scalable={word}");
        let primary = font_spec(vec![Value::keyword("name"), Value::string(name)]).unwrap();
        assert_eq!(
            font_get(vec![primary, Value::keyword("scalable")]).unwrap(),
            if expected { Value::T } else { Value::NIL },
            "{word}"
        );
        list_fonts(&mut eval, vec![primary]).unwrap();
        assert_eq!(requests.borrow()[0].0.scalable, Some(expected), "{word}");
    }
    // GNU font_parse_name dispatches any wildcard to the XLFD parser,
    // including a wildcard inside what otherwise resembles an extra property.
    let _eval = Context::new(crate::test_utils::unit_runtime());
    let flow = font_spec(vec![
        Value::keyword("name"),
        Value::string("Hack:scalable=*"),
    ])
    .unwrap_err();
    let neovm_value::error::FlowRef::Signal(condition) = flow.kind() else {
        panic!("expected invalid font signal: {flow:?}");
    };
    assert_eq!(condition.symbol, intern("error"));
    assert_eq!(
        condition.data[0].as_utf8_str(),
        Some("Invalid font name: Hack:scalable=*")
    );
}

#[test]
fn scalable_name_numeric_overflow_preserves_condition_and_original_digits() {
    let _eval = Context::new(crate::test_utils::unit_runtime());
    let maximum = Value::MOST_POSITIVE_FIXNUM.to_string();
    for digits in [
        maximum.clone(),
        format!("000000000000000000000{maximum}"),
        "0".repeat(40),
    ] {
        let name = format!("Hack:scalable={digits}");
        let spec = font_spec(vec![Value::keyword("name"), Value::string(name)]).unwrap();
        let expected = if digits.bytes().all(|byte| byte == b'0') {
            Value::NIL
        } else {
            Value::T
        };
        assert_eq!(
            font_get(vec![spec, Value::keyword("scalable")]).unwrap(),
            expected
        );
    }
    let excessive = (Value::MOST_POSITIVE_FIXNUM + 1).to_string();
    for digits in [
        excessive.clone(),
        format!("000000000000000000000{excessive}"),
        "9".repeat(40),
    ] {
        let name = format!("Hack:scalable={digits}");
        let flow = font_spec(vec![Value::keyword("name"), Value::string(name)])
            .expect_err("digit-only scalable values must fit a GNU fixnum");
        let neovm_value::error::FlowRef::Signal(condition) = flow.kind() else {
            panic!("expected overflow signal: {flow:?}");
        };
        assert_eq!(condition.symbol, intern("overflow-error"));
        assert_eq!(condition.data.len(), 1);
        assert_eq!(condition.data[0].as_utf8_str(), Some(digits.as_str()));
    }
}

fn publication_context(driver: SymId) -> Context {
    let (mut eval, _) = context();
    let requests = Rc::new(RefCell::new(Vec::new()));
    eval.set_display_host(Box::new(ListingFontDisplayHost {
        queries: requests,
        fonts: vec![neovm_eval::emacs_core::eval::ListedFontEntity {
            driver: Some(driver),
            adstyle: None,
            pixel_size: Some(0),
            average_width: Some(0),
            font: ResolvedFontSpecMatch {
                coverage_handle: None,
                family: LispString::from_utf8("Hack"),
                foundry: None,
                registry: Some(LispString::from_utf8("iso10646-1")),
                file: None,
                weight: Some(FontWeight::NORMAL),
                slant: Some(FontSlant::Normal),
                width: Some(FontWidth::Normal),
                spacing: Some(100),
                postscript_name: None,
            },
        }],
    }));
    eval
}

#[test]
fn scalable_publication_preserves_primary_values_only_for_copying_native_drivers() {
    for driver_name in ["ftcrhb", "ns", "directwrite", "uninterned-ftcrhb"] {
        for case in 0..3 {
            let driver = if driver_name == "uninterned-ftcrhb" {
                neovm_value::intern::intern_uninterned("ftcrhb")
            } else {
                intern(driver_name)
            };
            let mut eval = publication_context(driver);
            let value = match case {
                0 => Value::T,
                1 => Value::fixnum(0),
                _ => Value::vector(Vec::new()),
            };
            let primary = font_spec(vec![
                Value::keyword("family"),
                Value::string("Hack"),
                Value::keyword("scalable"),
                value,
            ])
            .unwrap();
            let fonts = list_fonts(&mut eval, vec![primary]).unwrap();
            let entity = list_to_vec(&fonts).unwrap()[0];
            let published = font_get(vec![entity, Value::keyword("scalable")]).unwrap();
            let expected = if matches!(driver_name, "ftcrhb" | "ns") {
                value
            } else {
                Value::NIL
            };
            assert!(eq_value(&published, &expected), "{driver_name} case {case}");
        }
    }
}

#[test]
fn scalable_publication_does_not_copy_preference_properties() {
    let mut eval = publication_context(intern("ftcrhb"));
    let primary = spec(None);
    let prefer = font_spec(vec![Value::keyword("scalable"), Value::T]).unwrap();
    let fonts = list_fonts(
        &mut eval,
        vec![primary, Value::NIL, Value::fixnum(1), prefer],
    )
    .unwrap();
    let entity = list_to_vec(&fonts).unwrap()[0];
    assert!(
        font_get(vec![entity, Value::keyword("scalable")])
            .unwrap()
            .is_nil()
    );
}
