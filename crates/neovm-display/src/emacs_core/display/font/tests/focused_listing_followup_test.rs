use super::*;
use neovm_eval::emacs_core::eval::ResolvedFrameFont;

#[test]
fn focused_listing_explicit_zero_dpi_preserves_zero_point_conversion() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let frame = ensure_selected_gui_frame(&mut eval);
    for (size, dpi, expected) in [
        (
            Value::fixnum(0),
            None,
            Some(FontOpeningSize::SmallestUsable),
        ),
        (
            Value::make_float(0.0),
            None,
            Some(FontOpeningSize::SmallestUsable),
        ),
        (
            Value::make_float(50.0),
            Some(0),
            Some(FontOpeningSize::SmallestUsable),
        ),
        (
            Value::fixnum(50),
            Some(0),
            Some(FontOpeningSize::Pixels(
                std::num::NonZeroU32::new(50).unwrap(),
            )),
        ),
        (
            Value::make_float(50.0),
            None,
            Some(FontOpeningSize::Points {
                size: PositiveFontScalar::new(50.0).unwrap(),
                dpi: None,
            }),
        ),
    ] {
        let mut arguments = vec![
            Value::keyword("family"),
            Value::string("Hack"),
            Value::keyword("size"),
            size,
        ];
        if let Some(dpi) = dpi {
            arguments.extend([Value::keyword("dpi"), Value::fixnum(dpi)]);
        }
        let spec = font_spec(arguments).unwrap();
        let query = font_list_query(&mut eval, &spec, frame).unwrap();
        assert_eq!(query.size, expected);
        assert_eq!(query.dpi, dpi.map(|dpi| dpi as u32));
    }
}

#[test]
fn focused_listing_unknown_width_does_not_leave_a_synthetic_legacy_zero() {
    crate::test_utils::init_test_tracing();
    let _eval = Context::new(crate::test_utils::unit_runtime());
    let font = ResolvedFontSpecMatch {
        coverage_handle: None,
        family: LispString::from_utf8("Fixed Fixture"),
        foundry: None,
        registry: Some(LispString::from_utf8("iso10646-1")),
        file: None,
        weight: Some(FontWeight::NORMAL),
        slant: Some(FontSlant::Normal),
        width: Some(FontWidth::Normal),
        spacing: Some(100),
        postscript_name: None,
    };
    let entity = listed_font_entity(&neovm_eval::emacs_core::eval::ListedFontEntity {
        font: font.clone(),
        driver: Some(intern("ftcrhb")),
        pixel_size: Some(16),
        average_width: None,
    });
    assert!(
        font_get(vec![entity, Value::keyword("avgwidth")])
            .unwrap()
            .is_nil()
    );
    assert!(
        font_get(vec![entity, Value::keyword("avg-width")])
            .unwrap()
            .is_nil()
    );
    let known = listed_font_entity(&neovm_eval::emacs_core::eval::ListedFontEntity {
        font,
        driver: Some(intern("ftcrhb")),
        pixel_size: Some(0),
        average_width: Some(0),
    });
    assert_eq!(
        font_get(vec![known, Value::keyword("avgwidth")]).unwrap(),
        Value::fixnum(0)
    );
    // The similarly spelled unknown property remains an independent extra.
    let spec = font_spec(vec![
        Value::keyword("avgwidth"),
        Value::fixnum(0),
        Value::keyword("avg-width"),
        Value::fixnum(120),
    ])
    .unwrap();
    assert_eq!(
        font_get(vec![spec, Value::keyword("avg-width")]).unwrap(),
        Value::fixnum(120)
    );
}

struct ListingFaceMetricsHost(Rc<RefCell<Vec<FrameFontSize>>>);
impl DisplayHost for ListingFaceMetricsHost {
    fn realize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }
    fn resize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }
    fn resolve_frame_font(
        &mut self,
        _: FrameId,
        request: FrameFontRequest,
    ) -> Result<Option<ResolvedFrameFont>, String> {
        self.0.borrow_mut().push(request.size());
        Ok(None)
    }
}

#[test]
fn focused_listing_face_metrics_follow_current_buffer_remapping_and_restore() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let frame = ensure_selected_gui_frame(&mut eval);
    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword("height"),
            Value::fixnum(130),
            Value::make_frame(frame.0),
        ],
    )
    .unwrap();
    let requests = Rc::new(RefCell::new(Vec::new()));
    eval.set_display_host(Box::new(ListingFaceMetricsHost(Rc::clone(&requests))));
    let query = |eval: &mut Context, face| {
        x_list_fonts(
            eval,
            vec![Value::string("Hack-13"), face, Value::NIL, Value::fixnum(1)],
        )
        .unwrap()
    };
    query(&mut eval, Value::symbol("default"));
    let remapping = Value::list(vec![Value::list(vec![
        Value::symbol("default"),
        Value::list(vec![Value::keyword("height"), Value::make_float(2.0)]),
        Value::symbol("default"),
    ])]);
    eval.buffer_manager_mut()
        .current_buffer_mut()
        .unwrap()
        .set_buffer_local("face-remapping-alist", remapping);
    query(&mut eval, Value::symbol("default"));
    query(&mut eval, Value::vector(Vec::new()));
    query(&mut eval, Value::NIL);
    eval.buffer_manager_mut()
        .current_buffer_mut()
        .unwrap()
        .set_buffer_local("face-remapping-alist", Value::NIL);
    query(&mut eval, Value::symbol("default"));
    assert_eq!(
        *requests.borrow(),
        vec![
            FrameFontSize::points(13.0).unwrap(),
            FrameFontSize::points(26.0).unwrap(),
            FrameFontSize::points(13.0).unwrap(),
            FrameFontSize::points(13.0).unwrap()
        ]
    );
}
