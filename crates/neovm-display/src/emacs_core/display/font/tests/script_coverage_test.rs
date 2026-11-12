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
pub(super) fn context() -> (Context, Requests) {
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

pub(super) fn publication_context(driver: SymId) -> Context {
    let (mut eval, _) = context();
    let requests = Rc::new(RefCell::new(Vec::new()));
    eval.set_display_host(Box::new(ListingFontDisplayHost {
        queries: requests,
        fonts: vec![neovm_eval::emacs_core::eval::ListedFontEntity {
            native_index: (driver == intern("ftcrhb")).then_some(0x0007_0003),
            driver: Some(driver),
            adstyle: None,
            pixel_size: Some(0),
            average_width: Some(0),
            font: ResolvedFontSpecMatch {
                coverage_handle: neovm_value::display_vocab::FontEntityHandle::new(17),
                family: LispString::from_utf8("Hack"),
                foundry: None,
                registry: Some(LispString::from_utf8("iso10646-1")),
                file: Some(LispString::from_utf8("/native/é.ttf")),
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

#[test]
fn otf_font_spec_and_font_put_validate_shape_and_preserve_error_data() {
    let _eval = Context::new(crate::test_utils::unit_runtime());
    let invalid = [
        Value::vector(Vec::new()),
        Value::fixnum(42),
        Value::list(vec![Value::fixnum(42)]),
        Value::list(vec![
            Value::symbol("latn"),
            Value::NIL,
            Value::list(vec![Value::fixnum(42)]),
        ]),
        Value::list(vec![Value::symbol("latn"), Value::NIL, Value::fixnum(42)]),
    ];
    for value in invalid {
        for put in [false, true] {
            let result = if put {
                let spec = font_spec(Vec::new()).unwrap();
                font_put(vec![spec, Value::keyword("otf"), value])
            } else {
                font_spec(vec![Value::keyword("otf"), value])
            };
            let flow = result.expect_err("GNU validates :otf at font-spec/font-put");
            let neovm_value::error::FlowRef::Signal(condition) = flow.kind() else {
                panic!("expected font property error: {flow:?}");
            };
            assert_eq!(condition.symbol, intern("error"));
            let expected = neovm_value::error::signal_error_with_arg(
                "invalid font property",
                Value::cons(Value::keyword("otf"), value),
            );
            let neovm_value::error::FlowRef::Signal(expected) = expected.kind() else {
                unreachable!()
            };
            assert_eq!(condition.data, expected.data);
        }
    }
    for value in [
        Value::NIL,
        Value::list(vec![Value::NIL]),
        Value::list(vec![Value::symbol("latn")]),
    ] {
        assert!(font_spec(vec![Value::keyword("otf"), value]).is_ok());
    }
}

#[test]
fn otf_capture_owns_tags_and_required_and_forbidden_features_only_from_primary() {
    use neovm_eval::emacs_core::eval::{FontOtfFeature as Feature, FontOtfRequirement};
    let (mut eval, requests) = context();
    let value = Value::list(vec![
        Value::symbol("latn"),
        Value::NIL,
        Value::list(vec![
            Value::symbol("ccmp"),
            Value::NIL,
            Value::symbol("zzzz"),
        ]),
        Value::list(vec![Value::symbol("kern")]),
    ]);
    let primary = font_spec(vec![Value::keyword("otf"), value]).unwrap();
    let prefer = font_spec(vec![
        Value::keyword("otf"),
        Value::list(vec![Value::symbol("zzzz")]),
    ])
    .unwrap();
    list_fonts(
        &mut eval,
        vec![primary, Value::NIL, Value::fixnum(1), prefer],
    )
    .unwrap();
    assert_eq!(
        requests.borrow()[0].0.otf,
        Some(FontOtfRequirement {
            script: *b"latn",
            language: None,
            gsub: vec![Feature::Required(*b"ccmp"), Feature::Forbidden(*b"zzzz")],
            gpos: vec![Feature::Required(*b"kern")],
        })
    );
    assert_eq!(requests.borrow()[0].1.as_ref().unwrap().otf, None);
}

#[test]
fn otf_capture_uses_default_script_and_preserves_named_language_spaces() {
    for named_language in [false, true] {
        let (mut eval, requests) = context();
        let value = if named_language {
            Value::list(vec![Value::symbol("latn"), Value::symbol("ISM ")])
        } else {
            Value::list(vec![Value::NIL])
        };
        let primary = font_spec(vec![Value::keyword("otf"), value]).unwrap();
        list_fonts(&mut eval, vec![primary]).unwrap();
        let recorded = requests.borrow();
        let requirement = recorded[0].0.otf.as_ref().expect("cons OTF constraint");
        assert_eq!(
            requirement.script,
            if named_language { *b"latn" } else { *b"DFLT" }
        );
        assert_eq!(requirement.language, named_language.then_some(*b"ISM "));
    }
}

#[test]
fn otf_derived_script_uses_identity_alist_and_explicit_script_overrides_it() {
    let (mut eval, requests) = context();
    let mapped = Value::symbol("otf-derived-script");
    eval.set_variable(
        "otf-script-alist",
        Value::list(vec![Value::cons(Value::symbol("latn"), mapped)]),
    );
    table(&mut eval, mapped, Value::list(vec![Value::fixnum(65)]));
    let otf = Value::list(vec![Value::symbol("latn")]);
    let primary = font_spec(vec![Value::keyword("otf"), otf]).unwrap();
    assert_eq!(
        capture(&mut eval, &requests, primary),
        Some(Coverage::All(vec![65]))
    );
    let overridden = font_spec(vec![
        Value::keyword("script"),
        Value::NIL,
        Value::keyword("otf"),
        otf,
    ])
    .unwrap();
    assert_eq!(capture(&mut eval, &requests, overridden), None);
    table(&mut eval, mapped, Value::vector(vec![Value::fixnum(65)]));
    assert_eq!(
        capture(&mut eval, &requests, primary),
        None,
        "only explicit :script enables vector alternatives"
    );
}

#[test]
fn otf_publication_retains_primary_extra_only_for_copying_native_drivers() {
    for driver in ["ftcrhb", "ns", "directwrite"] {
        let mut eval = publication_context(intern(driver));
        let value = Value::list(vec![Value::symbol("latn")]);
        let primary = font_spec(vec![Value::keyword("otf"), value]).unwrap();
        let fonts = list_fonts(&mut eval, vec![primary]).unwrap();
        let entity = list_to_vec(&fonts).unwrap()[0];
        let published = font_get(vec![entity, Value::keyword("otf")]).unwrap();
        assert!(
            eq_value(
                &published,
                &if driver == "directwrite" {
                    Value::NIL
                } else {
                    value
                }
            ),
            "{driver}"
        );
    }
}

#[test]
fn otf_public_subrs_observe_positioned_symbol_mode_and_keep_raw_property_values() {
    for put in [false, true] {
        let (mut eval, requests) = context();
        let construct = if put {
            "(let ((spec (font-spec :family \"Hack\"))) (font-put spec :otf otf) spec)"
        } else {
            "(font-spec :family \"Hack\" :otf otf)"
        };
        let source = format!(
            "(let* ((symbols-with-pos-enabled t)
            (otf (list (position-symbol 'latn 1) nil (list (position-symbol 'ccmp 2))))
            (spec {construct}))
            (list-fonts spec)
            (and (symbol-with-pos-p (car (font-get spec :otf)))
                 (symbol-with-pos-p (car (car (cdr (cdr (font-get spec :otf))))))))"
        );
        assert_eq!(eval.eval_str(&source).unwrap(), Value::T, "font-put={put}");
        let recorded = requests.borrow();
        let otf = recorded[0]
            .0
            .otf
            .as_ref()
            .expect("positioned tags must reach the native query");
        assert_eq!(otf.script, *b"latn");
        assert_eq!(
            otf.gsub,
            vec![neovm_eval::emacs_core::eval::FontOtfFeature::Required(
                *b"ccmp"
            )]
        );
    }
}

#[test]
fn otf_capture_preserves_raw_unibyte_tag_bytes_and_the_native_exclusion_bit() {
    let (mut eval, requests) = context();
    let feature = Value::from_sym_id(neovm_value::intern::intern_lisp_string(
        &LispString::from_unibyte(vec![200, 65, 66, 67]),
    ));
    let value = Value::list(vec![
        Value::symbol("latn"),
        Value::NIL,
        Value::list(vec![feature]),
    ]);
    let primary = font_spec(vec![Value::keyword("otf"), value]).unwrap();
    list_fonts(&mut eval, vec![primary]).unwrap();
    let recorded = requests.borrow();
    let otf = recorded[0].0.otf.as_ref().unwrap();
    assert_eq!(
        otf.gsub,
        vec![neovm_eval::emacs_core::eval::FontOtfFeature::Forbidden([
            72, 65, 66, 67
        ])]
    );
}

#[test]
fn otf_positioned_nil_keeps_raw_nilp_distinctions_and_disabled_mode_rejects_wrappers() {
    for role in 0..3 {
        let (mut eval, requests) = context();
        let value = match role {
            0 => "(list (position-symbol nil 1))",
            1 => "(list 'latn (position-symbol nil 1))",
            _ => "(list 'latn nil (list (position-symbol nil 1)))",
        };
        eval.eval_str(&format!(
            "(let ((symbols-with-pos-enabled t)) (list-fonts (font-spec :otf {value})))"
        ))
        .unwrap();
        let recorded = requests.borrow();
        let otf = recorded[0].0.otf.as_ref().unwrap();
        assert_eq!(otf.script, if role == 0 { *b"nil\0" } else { *b"latn" });
        assert_eq!(otf.language, (role == 1).then_some(*b"nil\0"));
        assert_eq!(
            otf.gsub,
            if role == 2 {
                vec![neovm_eval::emacs_core::eval::FontOtfFeature::Required(
                    *b"nil\0",
                )]
            } else {
                Vec::new()
            }
        );
    }
    let (mut eval, requests) = context();
    assert!(eval.eval_str("(let ((symbols-with-pos-enabled nil)) (font-spec :otf (list (position-symbol 'latn 1))))").is_err());
    assert!(requests.borrow().is_empty());
}

#[test]
fn generic_primary_extras_preserve_exact_keys_and_values_without_replacing_basic_fields() {
    for driver in ["ftcrhb", "ns"] {
        let mut eval = publication_context(intern(driver));
        let alias = Value::from_sym_id(neovm_value::intern::intern_uninterned(":family"));
        let payload = Value::vector(vec![Value::symbol("owned-payload")]);
        let extras = [
            (Value::keyword("lang"), Value::symbol("en")),
            (Value::keyword("script"), Value::symbol("latin")),
            (Value::keyword("custom-publication"), payload),
            (Value::symbol("family"), Value::symbol("bare-extra")),
            (alias, Value::symbol("uninterned-extra")),
            (Value::keyword("average-width"), Value::fixnum(120)),
            (Value::keyword("file"), Value::symbol("custom-file")),
        ];
        let mut properties = vec![
            Value::keyword("family"),
            Value::string("Requested Family"),
            Value::keyword("size"),
            Value::fixnum(20),
            Value::keyword("dpi"),
            Value::fixnum(72),
            Value::keyword("avgwidth"),
            Value::fixnum(120),
        ];
        for (key, value) in extras {
            properties.extend([key, value]);
        }
        let primary = font_spec(properties).unwrap();
        let listed = list_fonts(&mut eval, vec![primary]).unwrap();
        let entity = list_to_vec(&listed).unwrap()[0];
        for (key, value) in extras {
            assert!(
                eq_value(&font_get(vec![entity, key]).unwrap(), &value),
                "{driver} key {key:?}"
            );
        }
        assert_eq!(
            font_get(vec![entity, Value::keyword("family")]).unwrap(),
            Value::symbol("Hack")
        );
        assert_eq!(
            font_get(vec![entity, Value::keyword("size")]).unwrap(),
            Value::fixnum(0)
        );
        assert!(
            font_get(vec![entity, Value::keyword("dpi")])
                .unwrap()
                .is_nil()
        );
        assert_eq!(
            font_get(vec![entity, Value::keyword("avgwidth")]).unwrap(),
            Value::fixnum(0)
        );
    }
}

#[test]
fn generic_name_extra_preserves_original_string_identity() {
    let mut eval = publication_context(intern("ftcrhb"));
    let name = Value::string("Hack-13");
    let primary = font_spec(vec![Value::keyword("name"), name]).unwrap();
    let listed = list_fonts(&mut eval, vec![primary]).unwrap();
    let entity = list_to_vec(&listed).unwrap()[0];
    assert!(eq_value(
        &font_get(vec![entity, Value::keyword("name")]).unwrap(),
        &name
    ));
    assert_eq!(
        font_get(vec![entity, Value::keyword("size")]).unwrap(),
        Value::fixnum(0)
    );
}

#[test]
fn generic_extras_do_not_copy_preferences_or_noncopying_driver_properties() {
    for driver in [
        intern("ftcrhb"),
        intern("directwrite"),
        neovm_value::intern::intern_uninterned("ftcrhb"),
    ] {
        let mut eval = publication_context(driver);
        let primary = if driver == intern("ftcrhb") {
            font_spec(Vec::new()).unwrap()
        } else {
            font_spec(vec![Value::keyword("custom-publication"), Value::T]).unwrap()
        };
        let prefer = font_spec(vec![Value::keyword("custom-publication"), Value::T]).unwrap();
        let listed = list_fonts(
            &mut eval,
            vec![primary, Value::NIL, Value::fixnum(1), prefer],
        )
        .unwrap();
        let entity = list_to_vec(&listed).unwrap()[0];
        assert!(
            font_get(vec![entity, Value::keyword("custom-publication")])
                .unwrap()
                .is_nil()
        );
    }
}

#[test]
fn generic_extras_preserve_the_backend_owned_entity_handle() {
    let mut eval = publication_context(intern("ftcrhb"));
    let primary = font_spec(vec![
        Value::keyword("neomacs-entity-handle"),
        Value::fixnum(999),
    ])
    .unwrap();
    let listed = list_fonts(&mut eval, vec![primary]).unwrap();
    let entity = list_to_vec(&listed).unwrap()[0];
    assert_eq!(
        font_get(vec![entity, Value::keyword("neomacs-entity-handle")]).unwrap(),
        Value::fixnum(17)
    );
}

#[test]
fn generic_extras_publish_the_actual_freetype_key_after_public_file_and_key_extras() {
    for driver in ["ftcrhb", "ns"] {
        let mut eval = publication_context(intern(driver));
        let fake = Value::cons(Value::symbol("fake-file"), Value::fixnum(999));
        let primary = font_spec(vec![
            Value::keyword("font-entity"),
            fake,
            Value::keyword("file"),
            Value::symbol("custom-file"),
        ])
        .unwrap();
        let listed = list_fonts(&mut eval, vec![primary]).unwrap();
        let entity = list_to_vec(&listed).unwrap()[0];
        let key = font_get(vec![entity, Value::keyword("font-entity")]).unwrap();
        if driver == "ftcrhb" {
            assert!(key.is_cons(), "FreeType must replace the primary fake key");
            let file = key.cons_car();
            let file = file.as_lisp_string().unwrap();
            assert!(!file.is_multibyte());
            assert_eq!(file.as_bytes(), "/native/é.ttf".as_bytes());
            assert_eq!(
                key.cons_cdr(),
                Value::fixnum(0x0007_0003),
                "preserve the native instance selector"
            );
        } else {
            assert!(eq_value(&key, &fake), "NS copies the original extra");
        }
        assert_eq!(
            font_get(vec![entity, Value::keyword("file")]).unwrap(),
            Value::symbol("custom-file")
        );
    }
}

fn reserved_key_context() -> (Context, Rc<RefCell<Vec<FontSpecListQuery>>>) {
    let (mut eval, _) = context();
    let queries = Rc::new(RefCell::new(Vec::new()));
    let listed =
        |file: &str, index, handle, weight| neovm_eval::emacs_core::eval::ListedFontEntity {
            native_index: Some(index),
            driver: Some(intern("ftcrhb")),
            adstyle: None,
            pixel_size: Some(0),
            average_width: Some(0),
            font: ResolvedFontSpecMatch {
                coverage_handle: neovm_value::display_vocab::FontEntityHandle::new(handle),
                family: LispString::from_utf8("Fixture Sans"),
                foundry: None,
                registry: Some(LispString::from_utf8("iso10646-1")),
                file: Some(LispString::from_utf8(file)),
                weight: Some(weight),
                slant: Some(FontSlant::Normal),
                width: Some(FontWidth::Normal),
                spacing: Some(100),
                postscript_name: None,
            },
        };
    eval.set_display_host(Box::new(ListingFontDisplayHost {
        queries: Rc::clone(&queries),
        fonts: vec![
            listed("/first.ttf", 17, 1, FontWeight::BOLD),
            listed("/last.ttf", 23, 2, FontWeight::NORMAL),
        ],
    }));
    (eval, queries)
}

#[test]
fn reserved_key_mutates_primary_and_aliases_all_results_even_when_maximum_hides_last_candidate() {
    for maximum in [None, Some(1)] {
        let (mut eval, queries) = reserved_key_context();
        let fake = Value::cons(Value::symbol("fake"), Value::fixnum(123));
        let primary = font_spec(vec![
            Value::keyword("family"),
            Value::string("Fixture Sans"),
            Value::keyword("font-entity"),
            fake,
        ])
        .unwrap();
        let listed = list_fonts(
            &mut eval,
            vec![
                primary,
                Value::NIL,
                maximum
                    .map(|limit| Value::fixnum(limit))
                    .unwrap_or(Value::NIL),
            ],
        )
        .unwrap();
        let primary_key = font_get(vec![primary, Value::keyword("font-entity")]).unwrap();
        assert!(!eq_value(&primary_key, &fake));
        assert_eq!(primary_key.cons_car().as_utf8_str(), Some("/last.ttf"));
        assert_eq!(primary_key.cons_cdr(), Value::fixnum(23));
        let entities = list_to_vec(&listed).unwrap();
        assert_eq!(
            entities.len(),
            maximum.map(|limit| limit as usize).unwrap_or(2)
        );
        for (index, entity) in entities.into_iter().enumerate() {
            assert!(eq_value(
                &font_get(vec![entity, Value::keyword("font-entity")]).unwrap(),
                &primary_key
            ));
            assert_eq!(
                font_get(vec![entity, Value::keyword("neomacs-entity-handle")]).unwrap(),
                Value::fixnum(index as i64 + 1)
            );
        }
        assert_eq!(fake.cons_cdr(), Value::fixnum(123));
        assert_eq!(queries.borrow().len(), 2);
    }
}

#[test]
fn reserved_key_observation_clears_only_postfilter_constraints() {
    let (mut eval, queries) = reserved_key_context();
    let primary = font_spec(vec![
        Value::keyword("family"),
        Value::string("Fixture Sans"),
        Value::keyword("weight"),
        Value::symbol("bold"),
        Value::keyword("size"),
        Value::fixnum(20),
        Value::keyword("dpi"),
        Value::fixnum(72),
        Value::keyword("avgwidth"),
        Value::fixnum(120),
        Value::keyword("scalable"),
        Value::T,
        Value::keyword("font-entity"),
        Value::NIL,
    ])
    .unwrap();
    list_fonts(&mut eval, vec![primary, Value::NIL, Value::fixnum(1)]).unwrap();
    let queries = queries.borrow();
    assert_eq!(queries.len(), 2);
    assert_eq!(queries[0].specification.weight, None);
    assert_eq!(queries[0].size, None);
    assert_eq!(queries[0].dpi, None);
    assert_eq!(queries[0].average_width, None);
    assert_eq!(queries[0].scalable, Some(true));
    assert_eq!(queries[0].driver, Some(intern("ftcrhb")));
    assert_eq!(queries[1].specification.weight, Some(FontWeight::BOLD));
    assert_eq!(queries[1].dpi, Some(72));
    assert_eq!(queries[1].average_width, Some(120));
}

#[test]
fn reserved_key_mutation_precedes_ignored_font_regexp_errors() {
    let (mut eval, _) = reserved_key_context();
    eval.set_variable("face-ignored-fonts", Value::list(vec![Value::string("[")]));
    let fake = Value::cons(Value::symbol("fake"), Value::fixnum(123));
    let primary = font_spec(vec![Value::keyword("font-entity"), fake]).unwrap();
    let flow = list_fonts(&mut eval, vec![primary]).unwrap_err();
    let neovm_value::error::FlowRef::Signal(condition) = flow.kind() else {
        panic!("expected signal")
    };
    assert_eq!(condition.symbol, intern("invalid-regexp"));
    let key = font_get(vec![primary, Value::keyword("font-entity")]).unwrap();
    assert!(!eq_value(&key, &fake));
    assert_eq!(key.cons_car().as_utf8_str(), Some("/last.ttf"));
}

#[test]
fn reserved_key_zero_limit_and_lookalike_or_nonfreetype_drivers_do_not_observe() {
    let (mut eval, queries) = reserved_key_context();
    let fake = Value::cons(Value::symbol("fake"), Value::fixnum(123));
    let primary = font_spec(vec![Value::keyword("font-entity"), fake]).unwrap();
    assert!(
        list_fonts(&mut eval, vec![primary, Value::NIL, Value::fixnum(0)])
            .unwrap()
            .is_nil()
    );
    assert!(queries.borrow().is_empty());
    assert!(eq_value(
        &font_get(vec![primary, Value::keyword("font-entity")]).unwrap(),
        &fake
    ));
    let lookalike = Value::from_sym_id(neovm_value::intern::intern_uninterned(":font-entity"));
    let primary = font_spec(vec![lookalike, fake]).unwrap();
    list_fonts(&mut eval, vec![primary, Value::NIL, Value::fixnum(1)]).unwrap();
    assert_eq!(queries.borrow().len(), 1);
    assert!(eq_value(
        &font_get(vec![primary, lookalike]).unwrap(),
        &fake
    ));
    for driver in [
        intern("ns"),
        neovm_value::intern::intern_uninterned("ftcrhb"),
    ] {
        let (mut eval, requests) = context();
        let primary = font_spec(vec![
            Value::keyword("type"),
            Value::from_sym_id(driver),
            Value::keyword("font-entity"),
            Value::NIL,
        ])
        .unwrap();
        list_fonts(&mut eval, vec![primary]).unwrap();
        assert_eq!(requests.borrow().len(), 1);
    }
}
