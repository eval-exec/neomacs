use super::script_coverage_test::{context, publication_context};
use super::*;

fn pair() -> (Context, Value, Value) {
    let mut eval = publication_context(intern("ftcrhb"));
    let primary = font_spec(vec![
        Value::keyword("lang"),
        Value::symbol("en"),
        Value::keyword("custom-shared"),
        Value::fixnum(11),
    ])
    .unwrap();
    let listed = list_fonts(&mut eval, vec![primary]).unwrap();
    let entity = list_to_vec(&listed).unwrap()[0];
    (eval, primary, entity)
}

#[test]
fn font_extra_sharing_replaces_existing_cells_in_both_directions() {
    let (_eval, primary, entity) = pair();
    font_put(vec![primary, Value::keyword("lang"), Value::symbol("fr")]).unwrap();
    assert_eq!(
        font_get(vec![entity, Value::keyword("lang")]).unwrap(),
        Value::symbol("fr")
    );
    font_put(vec![
        primary,
        Value::keyword("custom-shared"),
        Value::fixnum(22),
    ])
    .unwrap();
    assert_eq!(
        font_get(vec![entity, Value::keyword("custom-shared")]).unwrap(),
        Value::fixnum(22)
    );
    font_put(vec![
        entity,
        Value::keyword("custom-shared"),
        Value::fixnum(44),
    ])
    .unwrap();
    assert_eq!(
        font_get(vec![primary, Value::keyword("custom-shared")]).unwrap(),
        Value::fixnum(44)
    );
    assert!(font_put(vec![entity, Value::keyword("lang"), Value::symbol("de")]).is_err());
}

#[test]
fn font_extra_sharing_does_not_copy_a_new_primary_key_to_an_old_ft_entity() {
    let (_eval, primary, entity) = pair();
    font_put(vec![
        primary,
        Value::keyword("new-shared"),
        Value::fixnum(33),
    ])
    .unwrap();
    assert_eq!(
        font_get(vec![primary, Value::keyword("new-shared")]).unwrap(),
        Value::fixnum(33)
    );
    assert!(
        font_get(vec![entity, Value::keyword("new-shared")])
            .unwrap()
            .is_nil()
    );
}

#[test]
fn font_extra_sharing_preserves_normalized_storage_and_original_put_return() {
    let (_eval, primary, _) = pair();
    let original = Value::string("fr");
    let returned = font_put(vec![primary, Value::keyword("lang"), original]).unwrap();
    assert!(eq_value(&returned, &original));
    assert_eq!(
        font_get(vec![primary, Value::keyword("lang")]).unwrap(),
        Value::symbol("fr")
    );
}

#[test]
fn font_extra_sharing_ignores_noncanonical_constraint_keys_without_normalizing_values() {
    for case in 0..2 {
        let (mut eval, requests) = context();
        let key = if case == 0 {
            Value::symbol("family")
        } else {
            Value::from_sym_id(neovm_value::intern::intern_uninterned(":family"))
        };
        let original = Value::string("Raw Family Extra");
        let primary = font_spec(vec![key, original]).unwrap();
        assert!(eq_value(&font_get(vec![primary, key]).unwrap(), &original));
        list_fonts(&mut eval, vec![primary]).unwrap();
        assert_eq!(requests.borrow()[0].0.specification.family, None);
    }
}

fn opened(capability: Value, explicit_nil: bool) -> Value {
    let mut fields = font_object_property_fields(&RuntimeFace::new("default"), Some(13));
    if explicit_nil {
        font_parse_set(&mut fields, "otf", Value::NIL);
    }
    finish_opened_font(
        fields,
        None,
        None,
        OpenedFontMetrics::fallback(13),
        capability,
        neomacs_display_protocol::font::ResolvedFontIdentity::from_memory(
            neomacs_display_protocol::font::FontBackendKind::Fontconfig,
            "extra-sharing-test".to_string(),
            0,
            None,
        ),
    )
}

#[test]
fn font_extra_sharing_opened_custom_slots_have_a_gc_visible_mutable_head() {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    eval.set_gc_threshold(usize::MAX);
    // Only the global variable roots the actual FontObject. Its fields must
    // retain the mutable extra head across a collection before the new slot.
    eval.set_variable("gc-opened-font", opened(Value::NIL, false));
    let before = eval.gc_count_for_test();
    eval.gc_collect_exact();
    assert_eq!(
        eval.eval_str("(fontp gc-opened-font 'font-object)")
            .unwrap(),
        Value::T
    );
    // This heap payload has no independent Lisp root after font-put returns.
    // A second full collection must trace FontObjectData.fields -> extra root
    // -> association cell -> payload, rather than just retain an immediate.
    eval.eval_str("(font-put gc-opened-font :custom-shared (vector 22))")
        .unwrap();
    eval.gc_collect_exact();
    assert!(eval.gc_count_for_test() >= before + 2);
    assert_eq!(
        eval.eval_str("(aref (font-get gc-opened-font :custom-shared) 0)")
            .unwrap(),
        Value::fixnum(22)
    );
    assert!(
        eval.eval_str("(font-put gc-opened-font :lang 'fr)")
            .is_err()
    );
}

#[test]
fn font_extra_sharing_opened_otf_fallback_requires_an_absent_extra_slot() {
    let _eval = Context::new(crate::test_utils::unit_runtime());
    let raw = Value::cons(Value::symbol("native-gsub"), Value::symbol("native-gpos"));
    let capability = Value::cons(Value::symbol("opentype"), raw);
    let inherited = opened(capability, false);
    assert!(eq_value(
        &font_get(vec![inherited, Value::keyword("otf")]).unwrap(),
        &raw
    ));
    let masked = opened(capability, true);
    assert!(
        font_get(vec![masked, Value::keyword("otf")])
            .unwrap()
            .is_nil()
    );
    assert!(eq_value(
        &masked.as_font_data().unwrap().capability,
        &capability
    ));
}

#[test]
fn font_extra_sharing_gc_retains_publicly_rooted_assoc_cells_and_replacements() {
    let mut eval = publication_context(intern("ftcrhb"));
    eval.set_gc_threshold(usize::MAX);
    eval.eval_str("(setq sharing-spec (font-spec :custom-shared (vector 11)) sharing-entity (car (list-fonts sharing-spec)))").unwrap();
    let before = eval.gc_count_for_test();
    eval.gc_collect_exact();
    eval.eval_str("(font-put sharing-spec :custom-shared (vector 22))")
        .unwrap();
    eval.gc_collect_exact();
    assert!(eval.gc_count_for_test() >= before + 2);
    assert_eq!(
        eval.eval_str("(aref (font-get sharing-entity :custom-shared) 0)")
            .unwrap(),
        Value::fixnum(22)
    );
}

#[test]
fn font_extra_sharing_pdump_preserves_the_rooted_spec_entity_cell_graph() {
    let mut eval = publication_context(intern("ftcrhb"));
    eval.eval_str("(setq sharing-spec (font-spec :custom-shared 11) sharing-entity (car (list-fonts sharing-spec)))").unwrap();
    let mut restored = neovm_eval::emacs_core::pdump::clone_evaluator(&eval).unwrap();
    restored
        .eval_str("(font-put sharing-spec :custom-shared 22)")
        .unwrap();
    assert_eq!(
        restored
            .eval_str("(font-get sharing-entity :custom-shared)")
            .unwrap(),
        Value::fixnum(22)
    );
    restored
        .eval_str("(font-put sharing-spec :new-shared 33)")
        .unwrap();
    assert!(
        restored
            .eval_str("(font-get sharing-entity :new-shared)")
            .unwrap()
            .is_nil()
    );
}

// NS shares the outer list too. Sorted middle insertion is visible through
// that old head; replacing a font's head is local to that font.
#[test]
fn font_extra_sharing_ns_head_and_middle_insertions_preserve_native_spine_semantics() {
    let mut eval = publication_context(intern("ns"));
    let primary = font_spec(vec![
        Value::keyword("bbb"),
        Value::fixnum(11),
        Value::keyword("ddd"),
        Value::fixnum(44),
    ])
    .unwrap();
    let entity = list_to_vec(&list_fonts(&mut eval, vec![primary]).unwrap()).unwrap()[0];
    font_put(vec![primary, Value::keyword("bbb"), Value::fixnum(22)]).unwrap();
    assert_eq!(
        font_get(vec![entity, Value::keyword("bbb")]).unwrap(),
        Value::fixnum(22)
    );
    font_put(vec![primary, Value::keyword("aaa"), Value::fixnum(1)]).unwrap();
    assert!(
        font_get(vec![entity, Value::keyword("aaa")])
            .unwrap()
            .is_nil()
    );
    font_put(vec![primary, Value::keyword("ccc"), Value::fixnum(3)]).unwrap();
    assert_eq!(
        font_get(vec![entity, Value::keyword("ccc")]).unwrap(),
        Value::fixnum(3)
    );
}

#[test]
fn font_extra_sharing_opened_builder_copies_cells_but_shares_payload_values() {
    let _eval = Context::new(crate::test_utils::unit_runtime());
    let payload = Value::vector(vec![Value::fixnum(11)]);
    let mut fields = font_object_property_fields(&RuntimeFace::new("default"), Some(13));
    font_parse_set(&mut fields, "custom-shared", payload);
    let original_root = properties::FontProperties::decode(&fields).root().unwrap();
    let font = finish_opened_font(
        fields,
        Some(&LispString::from_utf8("/native/font.ttf")),
        None,
        OpenedFontMetrics::fallback(13),
        Value::NIL,
        neomacs_display_protocol::font::ResolvedFontIdentity::from_memory(
            neomacs_display_protocol::font::FontBackendKind::Fontconfig,
            "copied-assoc".to_string(),
            0,
            None,
        ),
    );
    assert!(eq_value(
        &font_get(vec![font, Value::keyword("custom-shared")]).unwrap(),
        &payload
    ));
    font_put(vec![
        font,
        Value::keyword("custom-shared"),
        Value::fixnum(22),
    ])
    .unwrap();
    assert!(eq_value(
        &properties::assoc(
            original_root.cons_cdr(),
            Value::keyword("custom-shared"),
            neovm_value::tagged::SymbolMode::Bare
        )
        .unwrap()
        .cons_cdr(),
        &payload
    ));
    properties::set_extra(
        original_root,
        Value::keyword("custom-shared"),
        Value::fixnum(33),
        neovm_value::tagged::SymbolMode::Bare,
    );
    assert_eq!(
        font_get(vec![font, Value::keyword("custom-shared")]).unwrap(),
        Value::fixnum(22)
    );
    assert!(
        font_get(vec![font, Value::keyword("file")])
            .unwrap()
            .is_nil()
    );
    font_put(vec![font, Value::keyword("file"), Value::string("forged")]).unwrap();
    assert_eq!(
        font_get(vec![font, Value::keyword("file")])
            .unwrap()
            .as_utf8_str(),
        Some("forged")
    );
    assert_eq!(
        OpenedFont::decode(font)
            .unwrap()
            .property("file")
            .as_utf8_str(),
        Some("/native/font.ttf")
    );
}

#[test]
fn font_extra_sharing_native_handle_is_read_only_and_remains_the_matched_identity() {
    let (_eval, _primary, entity) = pair();
    let key = Value::keyword("neomacs-entity-handle");
    let actual = font_get(vec![entity, key]).unwrap();
    assert_eq!(actual, Value::fixnum(17));
    assert!(font_put(vec![entity, key, Value::fixnum(99)]).is_err());
    assert_eq!(font_get(vec![entity, key]).unwrap(), actual);
}

#[test]
fn font_extra_sharing_noncanonical_language_and_script_keys_do_not_constrain_native_queries() {
    for key_name in ["lang", "script", "otf"] {
        for uninterned in [false, true] {
            let (mut eval, requests) = context();
            let key = if uninterned {
                Value::from_sym_id(neovm_value::intern::intern_uninterned(&format!(
                    ":{key_name}"
                )))
            } else {
                Value::symbol(key_name)
            };
            let payload = Value::vector(vec![Value::fixnum(123)]);
            let primary = font_spec(vec![key, payload]).unwrap();
            list_fonts(&mut eval, vec![primary]).unwrap();
            let recorded = requests.borrow();
            assert_eq!(recorded[0].0.specification.lang, None);
            assert_eq!(recorded[0].0.script_coverage, None);
            assert_eq!(recorded[0].0.otf, None);
            assert!(eq_value(&font_get(vec![primary, key]).unwrap(), &payload));
        }
    }
}

#[test]
fn font_extra_sharing_native_opened_height_stays_distinct_from_pixel_size_for_internal_readers() {
    let _eval = Context::new(crate::test_utils::unit_runtime());
    let mut face = RuntimeFace::new("default");
    face.height = Some(FaceHeight::Absolute(102));
    let object = finish_opened_font(
        font_object_property_fields(&face, Some(13)),
        None,
        None,
        OpenedFontMetrics::fallback(13),
        Value::NIL,
        neomacs_display_protocol::font::ResolvedFontIdentity::from_memory(
            neomacs_display_protocol::font::FontBackendKind::Fontconfig,
            "native-height".to_string(),
            0,
            None,
        ),
    );
    assert!(
        font_get(vec![object, Value::keyword("height")])
            .unwrap()
            .is_nil()
    );
    let request = frame_font_request_from_value(&object).unwrap();
    assert_eq!(request.size(), FrameFontSize::points(10.2).unwrap());
    let attrs = crate::emacs_core::xfaces::derived_face_attrs_from_font_value(&object);
    assert_eq!(
        attrs
            .into_iter()
            .find(|(attr, _)| *attr == LFaceAttr::Height)
            .unwrap()
            .1,
        Value::fixnum(102)
    );
    let xlfd = font_xlfd_name(vec![object]).unwrap();
    assert!(xlfd.as_utf8_str().unwrap().ends_with("-0-iso10646-1"));
}

#[test]
fn font_extra_sharing_legacy_entity_writes_are_rejected_without_losing_old_flat_properties() {
    let _eval = Context::new(crate::test_utils::unit_runtime());
    let entity = Value::vector(vec![
        Value::keyword(FONT_ENTITY_TAG),
        Value::keyword("custom-old"),
        Value::fixnum(11),
    ]);
    assert!(
        font_put(vec![
            entity,
            Value::keyword("custom-new"),
            Value::fixnum(22)
        ])
        .is_err()
    );
    assert_eq!(
        font_get(vec![entity, Value::keyword("custom-old")]).unwrap(),
        Value::fixnum(11)
    );
    assert!(
        font_get(vec![entity, Value::keyword("custom-new")])
            .unwrap()
            .is_nil()
    );
}

#[test]
fn font_extra_sharing_positioned_extra_keys_use_context_symbol_mode_for_native_capture() {
    let mut observed = Vec::new();
    for property in ["lang", "script", "otf", "scalable"] {
        let (mut eval, requests) = context();
        eval.set_variable(
            "script-representative-chars",
            Value::list(vec![Value::cons(
                Value::symbol("latin"),
                Value::vector(vec![Value::fixnum(65)]),
            )]),
        );
        let value = match property {
            "lang" => "'en",
            "script" => "'latin",
            "otf" => "'(latn)",
            _ => "nil",
        };
        let form = format!(
            "(let ((symbols-with-pos-enabled t))
            (let ((spec (font-spec (position-symbol :{property} 1) {value})))
              (list-fonts spec)
              (font-get spec (position-symbol :{property} 2))))"
        );
        eval.eval_str(&form).unwrap();
        let query = &requests.borrow()[0].0;
        observed.push(match property {
            "lang" => query
                .specification
                .lang
                .as_ref()
                .is_some_and(|s| s.as_utf8_str() == Some("en")),
            "script" => {
                query.script_coverage
                    == Some(neovm_eval::emacs_core::eval::FontScriptCoverage::Any(vec![
                        65,
                    ]))
            }
            "otf" => query.otf.is_some(),
            _ => query.scalable == Some(false),
        });
    }
    assert_eq!(observed, vec![true; 4]);
}

#[test]
fn font_extra_sharing_missing_native_handle_cannot_be_supplied_by_a_user_extra() {
    let (mut eval, requests) = context();
    eval.set_display_host(Box::new(ListingFontDisplayHost {
        queries: Rc::new(RefCell::new(Vec::new())),
        fonts: vec![neovm_eval::emacs_core::eval::ListedFontEntity {
            native_index: None,
            driver: Some(intern("ftcrhb")),
            adstyle: None,
            pixel_size: Some(0),
            average_width: Some(0),
            font: ResolvedFontSpecMatch {
                coverage_handle: None,
                family: LispString::from_utf8("Hack"),
                foundry: None,
                registry: None,
                file: None,
                weight: None,
                slant: None,
                width: None,
                spacing: None,
                postscript_name: None,
            },
        }],
    }));
    let key = Value::keyword("neomacs-entity-handle");
    let primary = font_spec(vec![key, Value::fixnum(99)]).unwrap();
    assert_eq!(font_get(vec![primary, key]).unwrap(), Value::fixnum(99));
    let entity = list_to_vec(&list_fonts(&mut eval, vec![primary]).unwrap()).unwrap()[0];
    assert!(font_get(vec![entity, key]).unwrap().is_nil());
    assert_eq!(
        font_vector_get_flexible(font_value_fields(&entity).unwrap(), "neomacs-entity-handle"),
        None
    );
    assert!(requests.borrow().is_empty());
    struct CoverageHost(Rc<RefCell<usize>>);
    impl DisplayHost for CoverageHost {
        fn realize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
            Ok(())
        }
        fn resize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
            Ok(())
        }
        fn font_character_coverage(
            &mut self,
            _: crate::emacs_core::display_host::FontCoverageRequest,
        ) -> Result<crate::emacs_core::display_host::FontCoverage, String> {
            *self.0.borrow_mut() += 1;
            Ok(crate::emacs_core::display_host::FontCoverage::Present)
        }
    }
    let calls = Rc::new(RefCell::new(0));
    eval.set_display_host(Box::new(CoverageHost(Rc::clone(&calls))));
    for font in [primary, entity] {
        assert!(
            font_has_char_p(&mut eval, vec![font, Value::fixnum(65)])
                .unwrap()
                .is_nil()
        );
    }
    assert_eq!(
        *calls.borrow(),
        0,
        "user metadata must never reach native capability lookup"
    );
}

#[test]
fn font_extra_sharing_positioned_symbol_values_are_preserved_and_projected_in_context() {
    for (property, symbol) in [
        ("family", "MissingModeFamily"),
        ("lang", "en"),
        ("type", "neomacs-missing-driver"),
    ] {
        let (mut eval, requests) = context();
        let form = format!(
            "(let* ((symbols-with-pos-enabled t)
            (value (position-symbol '{symbol} 1))
            (spec (font-spec :{property} value)))
            (list-fonts spec)
            (eq (font-get spec :{property}) value))"
        );
        assert_eq!(eval.eval_str(&form).unwrap(), Value::T);
        let requests = requests.borrow();
        if property == "type" {
            assert_eq!(
                requests[0].0.driver,
                Some(intern(symbol)),
                "a wrapped driver remains a constraint"
            );
        } else {
            let field = if property == "family" {
                &requests[0].0.specification.family
            } else {
                &requests[0].0.specification.lang
            };
            assert_eq!(
                field.as_ref().and_then(LispString::as_utf8_str),
                Some(symbol)
            );
        }
    }
}

#[test]
fn font_extra_sharing_opened_otf_getter_returns_driver_data_without_info_prefix() {
    let _eval = Context::new(crate::test_utils::unit_runtime());
    let native = FontOtfCapability {
        gsub: Vec::new(),
        gpos: Vec::new(),
    };
    let info = otf_capability_to_lisp(&native);
    assert_eq!(info.cons_car(), Value::symbol("opentype"));
    let object = opened(info, false);
    let getter = font_get(vec![object, Value::keyword("otf")]).unwrap();
    assert!(eq_value(&getter, &info.cons_cdr()));
    assert!(eq_value(&object.as_font_data().unwrap().capability, &info));
}

#[test]
fn font_extra_sharing_positioned_extra_key_lookup_observes_the_query_time_mode() {
    for enabled in [false, true] {
        let (mut eval, requests) = context();
        let source = format!(
            "(progn
            (let ((symbols-with-pos-enabled t))
              (setq positioned-key-spec (font-spec (position-symbol :otf 1) '(zzzz))))
            (let ((symbols-with-pos-enabled {})) (list-fonts positioned-key-spec)))",
            if enabled { "t" } else { "nil" }
        );
        eval.eval_str(&source).unwrap();
        assert_eq!(requests.borrow()[0].0.otf.is_some(), enabled);
    }
}

#[test]
fn font_extra_sharing_disabled_positioned_driver_never_becomes_an_unrestricted_query() {
    #[derive(Default)]
    struct Counts {
        lists: usize,
        matches: usize,
    }
    struct Host(Rc<RefCell<Counts>>);
    impl DisplayHost for Host {
        fn realize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
            Ok(())
        }
        fn resize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
            Ok(())
        }
        fn list_fonts_for_spec(
            &mut self,
            _: neovm_eval::emacs_core::eval::FontSpecListQuery,
            _: Option<neovm_eval::emacs_core::eval::FontSpecListQuery>,
            _: Option<usize>,
            _: Option<&mut neovm_eval::emacs_core::eval::FontListFilter<'_>>,
        ) -> Result<
            Vec<neovm_eval::emacs_core::eval::ListedFontEntity>,
            neovm_eval::emacs_core::eval::FontListError,
        > {
            self.0.borrow_mut().lists += 1;
            Ok(Vec::new())
        }
        fn resolve_font_for_spec(
            &mut self,
            _: FontSpecResolveRequest,
        ) -> Result<Option<ResolvedFontSpecMatch>, String> {
            self.0.borrow_mut().matches += 1;
            Ok(None)
        }
    }
    for symbol in ["ftcrhb", "nil"] {
        let (mut eval, _) = context();
        let counts = Rc::new(RefCell::new(Counts::default()));
        eval.set_display_host(Box::new(Host(Rc::clone(&counts))));
        let primary = eval
            .eval_str(&format!(
                "(let ((symbols-with-pos-enabled t))
            (font-spec :type (position-symbol '{symbol} 1)))"
            ))
            .unwrap();
        assert!(list_fonts(&mut eval, vec![primary]).unwrap().is_nil());
        assert!(find_font(&mut eval, vec![primary]).unwrap().is_nil());
        assert!(font_info(&mut eval, vec![primary]).unwrap().is_nil());
        assert_eq!(
            (counts.borrow().lists, counts.borrow().matches),
            (0, 0),
            "{symbol}"
        );
        assert!(list_fonts(&mut eval, vec![primary, Value::fixnum(42)]).is_err());
        assert!(find_font(&mut eval, vec![primary, Value::fixnum(42)]).is_err());
        assert!(
            list_fonts(
                &mut eval,
                vec![primary, Value::NIL, Value::fixnum(0), Value::T]
            )
            .unwrap()
            .is_nil()
        );
        assert!(list_fonts(&mut eval, vec![primary, Value::NIL, Value::NIL, Value::T]).is_err());
        // Driver constraints on PREFER are ignored, including an otherwise
        // unmatchable raw wrapper. Only the primary spec gates enumeration.
        let unrestricted = font_spec(Vec::new()).unwrap();
        list_fonts(
            &mut eval,
            vec![unrestricted, Value::NIL, Value::NIL, primary],
        )
        .unwrap();
        assert_eq!((counts.borrow().lists, counts.borrow().matches), (1, 0));
    }
}

#[test]
fn font_extra_sharing_rejects_forged_extra_entries_before_mutating_them() {
    let _eval = Context::new(crate::test_utils::unit_runtime());
    let root = Value::cons(Value::NIL, Value::list(vec![Value::fixnum(42)]));
    let forged = Value::vector(vec![Value::keyword(FONT_SPEC_TAG), root]);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        font_put(vec![
            forged,
            Value::keyword("custom-new"),
            Value::fixnum(22),
        ])
    }));
    assert!(
        result.is_ok(),
        "a user-visible forged vector must not panic in the property walker"
    );
    assert!(result.unwrap().is_err());
}

#[test]
fn font_extra_sharing_rejects_forged_extra_entries_before_native_key_copying() {
    let mut eval = publication_context(intern("ftcrhb"));
    let root = Value::cons(Value::NIL, Value::list(vec![Value::fixnum(42)]));
    let forged = Value::vector(vec![Value::keyword(FONT_SPEC_TAG), root]);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        list_fonts(&mut eval, vec![forged])
    }));
    assert!(
        result.is_ok(),
        "a user-visible forged vector must not panic while copying extras"
    );
    assert!(result.unwrap().is_err());
}

#[test]
fn font_extra_sharing_xlfd_renderer_interprets_positioned_symbol_values_in_context() {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let rendered = eval
        .eval_str(
            "(let ((symbols-with-pos-enabled t))
        (font-xlfd-name (font-spec :family (position-symbol 'Hack 1))))",
        )
        .unwrap();
    assert_eq!(
        rendered.as_utf8_str(),
        Some("-*-Hack-*-*-*-*-*-*-*-*-*-*-*-*")
    );
}

#[test]
fn font_extra_sharing_hostile_extra_spines_are_finite_and_rejected_before_mutation() {
    for case in 0..3 {
        let (mut eval, requests) = context();
        let pair = Value::cons(Value::keyword("known"), Value::fixnum(11));
        let head = match case {
            0 => Value::list(vec![Value::fixnum(42), pair]),
            1 => Value::cons(pair, Value::fixnum(42)),
            _ => {
                let head = Value::cons(pair, Value::NIL);
                head.set_cdr(head);
                head
            }
        };
        let root = Value::cons(Value::NIL, head);
        let forged = Value::vector(vec![Value::keyword(FONT_SPEC_TAG), root]);
        // Read-only walks skip malformed associations and terminate on an
        // improper or circular spine; mutation/listing require valid storage.
        assert_eq!(
            font_get(vec![forged, Value::keyword("known")]).unwrap(),
            Value::fixnum(11)
        );
        assert!(
            font_get(vec![forged, Value::keyword("missing")])
                .unwrap()
                .is_nil()
        );
        assert!(font_put(vec![forged, Value::keyword("new"), Value::fixnum(22)]).is_err());
        assert!(list_fonts(&mut eval, vec![forged]).is_err());
        assert!(
            eq_value(&root.cons_cdr(), &head),
            "rejected mutation changed the extra head"
        );
        assert!(
            requests.borrow().is_empty(),
            "malformed storage reached the native host"
        );
        for copy in [properties::copy_outer(head), properties::copy_cells(head)] {
            let pairs = list_to_vec(&copy).expect("copy must have a finite proper outer list");
            assert!(!pairs.is_empty());
            assert!(pairs.iter().all(|pair| pair.is_cons()));
        }
        properties::remove_extra(
            root,
            Value::keyword("missing"),
            neovm_value::tagged::SymbolMode::Bare,
        );
        properties::set_extra(
            root,
            Value::keyword("zzz"),
            Value::fixnum(22),
            neovm_value::tagged::SymbolMode::Bare,
        );
        // The private primitives must remain memory-safe and finite too;
        // their handling of forged storage is not a GNU font representation.
    }
}
