use super::*;
use crate::display_item::{
    DisplayImageItem, DisplayLength, DisplayMediaReplacement, DisplayVideoItem, DisplayXwidgetItem,
};
use crate::display_property::DisplayPropertyObject;
use neomacs_display_protocol::VideoId;
use neovm_core::emacs_core::{Context, Value};

#[test]
fn rejected_nested_when_has_no_text_modifiers() {
    let mut eval = Context::new();
    let spec = eval
        .eval_str("'(when t when nil :raise 2 :height 3)")
        .expect("spec");
    let classified = classify_display_property(
        spec,
        &crate::display_when::DisplayWhenConditions::structural(),
        DisplayPropertyObject::LispString,
    );
    assert!(classified.replacement().is_none());
    assert_eq!(
        classified.modifiers,
        DisplayTextPropertyModifiers::default()
    );
}

#[test]
fn rejected_image_has_no_text_modifiers() {
    let mut eval = Context::new();
    let spec = eval.eval_str("'(image :raise 2 :height 3)").expect("spec");
    let classified = classify_display_property(
        spec,
        &crate::display_when::DisplayWhenConditions::structural(),
        DisplayPropertyObject::LispString,
    );
    assert!(classified.replacement().is_none());
    assert_eq!(
        classified.modifiers,
        DisplayTextPropertyModifiers::default()
    );
}

#[test]
fn classify_display_property_separates_replacements_from_text_modifiers() {
    let eval = Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let xwidget = Value::make_xwidget(
        Value::symbol("webkit"),
        Value::string("Title"),
        Value::make_buffer(buffer_id),
        96,
        54,
        1234,
        neomacs_display_protocol::WebViewId::new(5678),
    );

    assert_eq!(
        classify_display_property(
            Value::string("replacement"),
            &crate::display_when::DisplayWhenConditions::structural(),
            DisplayPropertyObject::Buffer
        )
        .replacement()
        .cloned(),
        Some(DisplayReplacementProperty::String)
    );
    let align_expr = Value::list(vec![
        Value::symbol("-"),
        Value::symbol("right"),
        Value::fixnum(2),
    ]);
    assert_eq!(
        classify_display_property(
            Value::list(vec![
                Value::symbol("space"),
                Value::keyword(":align-to"),
                align_expr,
            ]),
            &crate::display_when::DisplayWhenConditions::structural(),
            DisplayPropertyObject::Buffer
        )
        .replacement()
        .cloned(),
        Some(DisplayReplacementProperty::Stretch(ParsedDisplaySpace {
            width: ParsedDisplaySpaceWidth::AlignTo(align_expr),
            height: None,
            ascent: None,
        }))
    );
    assert_eq!(
        classify_display_property(
            Value::list(vec![
                Value::symbol("image"),
                Value::keyword(":type"),
                Value::symbol("png"),
                Value::keyword(":file"),
                Value::string("x.png")
            ]),
            &crate::display_when::DisplayWhenConditions::structural(),
            DisplayPropertyObject::Buffer
        )
        .replacement()
        .cloned(),
        Some(DisplayReplacementProperty::Media(
            DisplayMediaReplacementProperty::Image
        ))
    );
    assert_eq!(
        classify_display_property(
            Value::list(vec![Value::symbol("video")]),
            &crate::display_when::DisplayWhenConditions::structural(),
            DisplayPropertyObject::Buffer
        )
        .replacement()
        .cloned(),
        Some(DisplayReplacementProperty::Media(
            DisplayMediaReplacementProperty::Video
        ))
    );
    assert_eq!(
        classify_display_property(
            Value::list(vec![Value::symbol("webkit")]),
            &crate::display_when::DisplayWhenConditions::structural(),
            DisplayPropertyObject::Buffer
        )
        .replacement()
        .cloned(),
        Some(DisplayReplacementProperty::Media(
            DisplayMediaReplacementProperty::Webkit
        ))
    );
    assert_eq!(
        classify_display_property(
            Value::list(vec![
                Value::symbol("xwidget"),
                Value::keyword("xwidget"),
                xwidget,
            ]),
            &crate::display_when::DisplayWhenConditions::structural(),
            DisplayPropertyObject::Buffer
        )
        .replacement()
        .cloned(),
        Some(DisplayReplacementProperty::Media(
            DisplayMediaReplacementProperty::Xwidget(DisplayMediaReplacement::xwidget(
                DisplayXwidgetItem {
                    xwidget_id: neomacs_display_protocol::XwidgetId::new(1234),
                    webview_id: neomacs_display_protocol::WebViewId::new(5678),
                    width: 96.0,
                    height: 54.0,
                }
            ))
        ))
    );

    assert_eq!(
        classify_display_property(
            Value::list(vec![Value::symbol("raise"), Value::make_float(0.25),]),
            &crate::display_when::DisplayWhenConditions::structural(),
            DisplayPropertyObject::Buffer
        )
        .modifiers(),
        DisplayTextPropertyModifiers {
            raise: Some(0.25),
            height: None,
            space_width: None,
            break_after_row: false,
        }
    );
    assert_eq!(
        classify_display_property(
            Value::list(vec![
                Value::keyword(":raise"),
                Value::make_float(0.2),
                Value::keyword(":height"),
                Value::make_float(1.4),
            ]),
            &crate::display_when::DisplayWhenConditions::structural(),
            DisplayPropertyObject::Buffer
        )
        .modifiers(),
        DisplayTextPropertyModifiers {
            raise: Some(0.2),
            height: Some(1.4),
            space_width: None,
            break_after_row: false,
        }
    );
}

#[test]
fn display_property_classification_names_replacement_accessors() {
    let _eval = Context::new();
    let string = classify_display_property(
        Value::string("replacement"),
        &crate::display_when::DisplayWhenConditions::structural(),
        DisplayPropertyObject::Buffer,
    );
    let stretch = classify_display_property(
        Value::list(vec![
            Value::symbol("space"),
            Value::keyword(":width"),
            Value::fixnum(3),
        ]),
        &crate::display_when::DisplayWhenConditions::structural(),
        DisplayPropertyObject::Buffer,
    );
    let media = classify_display_property(
        Value::list(vec![
            Value::symbol("image"),
            Value::keyword(":type"),
            Value::symbol("png"),
            Value::keyword(":file"),
            Value::string("x.png"),
        ]),
        &crate::display_when::DisplayWhenConditions::structural(),
        DisplayPropertyObject::Buffer,
    );
    let modifier = classify_display_property(
        Value::list(vec![Value::symbol("raise"), Value::make_float(0.25)]),
        &crate::display_when::DisplayWhenConditions::structural(),
        DisplayPropertyObject::Buffer,
    );

    assert_eq!(
        string.replacement(),
        Some(&DisplayReplacementProperty::String)
    );

    assert!(matches!(
        stretch.replacement(),
        Some(DisplayReplacementProperty::Stretch(ParsedDisplaySpace {
            width: ParsedDisplaySpaceWidth::Length(DisplayLength::Em(3.0)),
            height: None,
            ascent: None,
        }))
    ));

    assert_eq!(
        media.replacement(),
        Some(&DisplayReplacementProperty::Media(
            DisplayMediaReplacementProperty::Image
        ))
    );

    assert!(modifier.replacement().is_none());
}

#[test]
fn classify_display_property_parses_space_width_height_and_ascent() {
    let _eval = Context::new();

    assert_eq!(
        classify_display_property(
            Value::list(vec![
                Value::symbol("space"),
                Value::keyword(":width"),
                Value::fixnum(3),
                Value::keyword(":height"),
                Value::fixnum(2),
                Value::keyword(":ascent"),
                Value::fixnum(50),
            ]),
            &crate::display_when::DisplayWhenConditions::structural(),
            DisplayPropertyObject::Buffer
        )
        .replacement()
        .cloned(),
        Some(DisplayReplacementProperty::Stretch(ParsedDisplaySpace {
            width: ParsedDisplaySpaceWidth::Length(DisplayLength::Em(3.0)),
            height: Some(DisplayLength::Em(2.0)),
            ascent: Some(DisplayLength::Em(50.0)),
        }))
    );
}

#[test]
fn classify_display_property_preserves_relative_width_source_basis() {
    let _eval = Context::new();

    assert_eq!(
        classify_display_property(
            Value::list(vec![
                Value::symbol("space"),
                Value::keyword(":relative-width"),
                Value::make_float(0.5),
            ]),
            &crate::display_when::DisplayWhenConditions::structural(),
            DisplayPropertyObject::Buffer
        )
        .replacement()
        .cloned(),
        Some(DisplayReplacementProperty::Stretch(ParsedDisplaySpace {
            width: ParsedDisplaySpaceWidth::RelativeToSource { factor: 0.5 },
            height: None,
            ascent: None,
        }))
    );
}

#[test]
fn classify_display_property_keeps_space_width_with_raise_modifier() {
    let _eval = Context::new();
    let classified = classify_display_property(
        Value::list(vec![
            Value::list(vec![Value::symbol("space-width"), Value::make_float(0.4)]),
            Value::list(vec![Value::symbol("raise"), Value::make_float(0.15)]),
        ]),
        &crate::display_when::DisplayWhenConditions::structural(),
        DisplayPropertyObject::Buffer,
    );

    assert!(classified.replacement().is_none());
    assert_eq!(classified.modifiers().space_width, Some(0.4));
    assert_eq!(classified.modifiers().raise, Some(0.15));
}

#[test]
fn classify_display_property_keeps_space_replacement_without_explicit_width() {
    let _eval = Context::new();

    assert_eq!(
        classify_display_property(
            Value::list(vec![
                Value::symbol("space"),
                Value::keyword(":height"),
                Value::fixnum(2),
            ]),
            &crate::display_when::DisplayWhenConditions::structural(),
            DisplayPropertyObject::Buffer
        )
        .replacement()
        .cloned(),
        Some(DisplayReplacementProperty::Stretch(ParsedDisplaySpace {
            width: ParsedDisplaySpaceWidth::Length(DisplayLength::Em(1.0)),
            height: Some(DisplayLength::Em(2.0)),
            ascent: None,
        }))
    );
}

#[test]
fn classify_display_property_recognizes_left_and_right_fringe_specs() {
    let _eval = Context::new();

    // `(left-fringe BITMAP FACE)` / `(right-fringe BITMAP FACE)` are replacement
    // specs that produce no inline output (magit's section-heading fold arrows).
    // The parsed layout carries the bitmap symbol, side, and optional face.
    let left = classify_display_property(
        Value::list(vec![
            Value::symbol("left-fringe"),
            Value::symbol("magit-fringe-bitmapv"),
            Value::symbol("fringe"),
        ]),
        &crate::display_when::DisplayWhenConditions::structural(),
        DisplayPropertyObject::Buffer,
    );
    match left.replacement() {
        Some(crate::display_property::DisplayReplacementProperty::Fringe(layout)) => {
            assert_eq!(layout.side, crate::display_spec::DisplayFringeSide::Left);
            assert!(layout.bitmap.is_symbol_named("magit-fringe-bitmapv"));
            assert!(
                layout
                    .face
                    .is_some_and(|face| face.is_symbol_named("fringe"))
            );
        }
        other => panic!("expected left fringe layout, got {other:?}"),
    }

    let right = classify_display_property(
        Value::list(vec![
            Value::symbol("right-fringe"),
            Value::symbol("right-arrow"),
        ]),
        &crate::display_when::DisplayWhenConditions::structural(),
        DisplayPropertyObject::Buffer,
    );
    match right.replacement() {
        Some(crate::display_property::DisplayReplacementProperty::Fringe(layout)) => {
            assert_eq!(layout.side, crate::display_spec::DisplayFringeSide::Right);
            assert!(layout.bitmap.is_symbol_named("right-arrow"));
            assert!(layout.face.is_none(), "no FACE provided");
        }
        other => panic!("expected right fringe layout, got {other:?}"),
    }
}

#[test]
fn classify_display_property_unwraps_list_wrapped_fringe_spec() {
    let _eval = Context::new();

    // diff-hl / flycheck / git-gutter attach the fringe marker via an overlay
    // before-string whose `display` value is LIST-WRAPPED:
    //   ((left-fringe diff-hl-bmp-middle diff-hl-change))
    // i.e. a list whose single element is the bare `(left-fringe …)` spec. GNU
    // `handle_display_spec` (src/xdisp.c) iterates such a list and handles each
    // element as a single spec, so the inner fringe spec must still classify as
    // a Fringe replacement (drawn in the fringe, no inline glyph).
    let bare = vec![
        Value::symbol("left-fringe"),
        Value::symbol("diff-hl-bmp-middle"),
        Value::symbol("diff-hl-change"),
    ];
    let wrapped = classify_display_property(
        Value::list(vec![Value::list(bare.clone())]),
        &crate::display_when::DisplayWhenConditions::structural(),
        DisplayPropertyObject::Buffer,
    );
    match wrapped.replacement() {
        Some(crate::display_property::DisplayReplacementProperty::Fringe(layout)) => {
            assert_eq!(layout.side, crate::display_spec::DisplayFringeSide::Left);
            assert!(layout.bitmap.is_symbol_named("diff-hl-bmp-middle"));
            assert!(
                layout
                    .face
                    .is_some_and(|face| face.is_symbol_named("diff-hl-change"))
            );
        }
        other => panic!("expected fringe layout from list-wrapped spec, got {other:?}"),
    }

    // The list-wrapped classification must match the bare spec exactly.
    assert_eq!(
        wrapped.replacement().cloned(),
        classify_display_property(
            Value::list(bare),
            &crate::display_when::DisplayWhenConditions::structural(),
            DisplayPropertyObject::Buffer
        )
        .replacement()
        .cloned(),
    );

    // A right-fringe list-wrapped spec resolves to the right side.
    let right = classify_display_property(
        Value::list(vec![Value::list(vec![
            Value::symbol("right-fringe"),
            Value::symbol("diff-hl-bmp-insert"),
        ])]),
        &crate::display_when::DisplayWhenConditions::structural(),
        DisplayPropertyObject::Buffer,
    );
    match right.replacement() {
        Some(crate::display_property::DisplayReplacementProperty::Fringe(layout)) => {
            assert_eq!(layout.side, crate::display_spec::DisplayFringeSide::Right);
            assert!(layout.bitmap.is_symbol_named("diff-hl-bmp-insert"));
        }
        other => panic!("expected right fringe layout from list-wrapped spec, got {other:?}"),
    }
}

#[test]
fn classify_display_property_keeps_fringe_length_units_in_space_specs() {
    let _eval = Context::new();

    // The `left-fringe` / `right-fringe` LENGTH UNITS appear inside a `space`
    // `:align-to` pixel expression and must keep resolving as length symbols,
    // NOT be mistaken for the `(left-fringe …)` fringe-bitmap replacement spec.
    assert_eq!(
        classify_display_property(
            Value::list(vec![
                Value::symbol("space"),
                Value::keyword(":align-to"),
                Value::symbol("left-fringe"),
            ]),
            &crate::display_when::DisplayWhenConditions::structural(),
            DisplayPropertyObject::Buffer
        )
        .replacement()
        .cloned(),
        Some(DisplayReplacementProperty::Stretch(ParsedDisplaySpace {
            width: ParsedDisplaySpaceWidth::AlignTo(Value::symbol("left-fringe")),
            height: None,
            ascent: None,
        }))
    );
}

/// Issue #204 — `(space :align-to (- center (0.5 . IMAGE-SPEC)))` centres an
/// image in GNU Emacs but left-aligned it in NEO Emacs.
///
/// GNU `calc_pixel_width_or_height` (xdisp.c:30506, :30551) evaluates an
/// `(image …)` operand to the image's pixel width and `(NUM . EXPR)` to
/// NUM × EXPR, so the spec resolves to `centre − half the image width`.
/// Note `(0.5 . (image …))` reads as the proper list
/// `(0.5 image :type png :file "…")`, which no `parse_display_length_expr`
/// arm matched — the operand parse failed, the whole `:align-to` was
/// dropped, and the stretch collapsed to zero width, flushing the image
/// left. Dashboard and `image-dired` centre banners with this exact form.
#[test]
fn align_to_keeps_fractional_image_width_operand() {
    let mut eval = Context::new();
    let expr = eval
        .eval_str(r#"(quote (- center (0.5 . (image :type png :file "x.png"))))"#)
        .expect("read :align-to expression");

    let spec = Value::list(vec![
        Value::symbol("space"),
        Value::keyword(":align-to"),
        expr,
    ]);

    assert_eq!(
        classify_display_property(
            spec,
            &crate::display_when::DisplayWhenConditions::structural(),
            DisplayPropertyObject::Buffer
        )
        .replacement()
        .cloned(),
        Some(DisplayReplacementProperty::Stretch(ParsedDisplaySpace {
            width: ParsedDisplaySpaceWidth::AlignTo(expr),
            height: None,
            ascent: None,
        })),
        "the `:align-to` operand must reach the evaluator verbatim; dropping it \
         collapsed the stretch to a 1-column space and flushed the image left"
    );
}

#[test]
fn display_replacement_property_accepts_only_matching_media_replacements() {
    let image = DisplayMediaReplacement::image(DisplayImageItem {
        image_id: 1,
        source_rect: neomacs_display_protocol::ImageSourceRect::FULL,
        width: 10.0,
        height: 20.0,
        ascent: 20.0,
        horizontal_margin: 0.0,
        vertical_margin: 0.0,
        opaque_background: None,
    });
    let video = DisplayMediaReplacement::video(DisplayVideoItem {
        video_id: VideoId::new(2),
        width: 30.0,
        height: 40.0,
        opacity: 1.0,
    });
    let xwidget = DisplayMediaReplacement::xwidget(DisplayXwidgetItem {
        xwidget_id: neomacs_display_protocol::XwidgetId::new(3),
        webview_id: neomacs_display_protocol::WebViewId::new(30),
        width: 50.0,
        height: 60.0,
    });

    assert!(DisplayMediaReplacementProperty::Image.accepts_media_replacement(&image));
    assert!(!DisplayMediaReplacementProperty::Image.accepts_media_replacement(&video));
    assert!(DisplayMediaReplacementProperty::Video.accepts_media_replacement(&video));
    assert!(!DisplayMediaReplacementProperty::Video.accepts_media_replacement(&image));
    assert!(DisplayMediaReplacementProperty::Webkit.accepts_media_replacement(&xwidget));
    assert!(!DisplayMediaReplacementProperty::Webkit.accepts_media_replacement(&image));
}

#[test]
fn display_replacement_property_describes_media_replacement_behavior() {
    let xwidget = DisplayXwidgetItem {
        xwidget_id: neomacs_display_protocol::XwidgetId::new(3),
        webview_id: neomacs_display_protocol::WebViewId::new(30),
        width: 50.0,
        height: 60.0,
    };
    let xwidget_replacement = DisplayMediaReplacement::xwidget(xwidget);

    assert_eq!(
        DisplayMediaReplacementProperty::Image.media_fallback_placeholder(),
        Some("[img]")
    );
    assert_eq!(
        DisplayMediaReplacementProperty::Video.media_fallback_placeholder(),
        Some("     ")
    );
    assert_eq!(
        DisplayMediaReplacementProperty::Webkit.media_fallback_placeholder(),
        Some("     ")
    );
    assert_eq!(
        DisplayMediaReplacementProperty::Xwidget(xwidget_replacement).media_fallback_placeholder(),
        None
    );
    assert_eq!(
        DisplayMediaReplacementProperty::Xwidget(xwidget_replacement).direct_replacement(),
        Some(xwidget_replacement)
    );
    assert_eq!(
        DisplayMediaReplacementProperty::Image.direct_replacement(),
        None
    );
}

#[test]
fn margin_display_spec_preserves_its_typed_side_and_content() {
    // magit's section visibility indicator is
    //   #("o" 0 1 (display ((margin right-margin) " ")))
    // The `((margin …) …)` spec must classify as a replacement so the covered
    // placeholder "o" is suppressed inline, not fall through to `None` (which
    // renders the "o" in the text flow). neomacs#188.
    let _eval = Context::new(); // initialize the tagged heap for Value allocation
    for side in ["left-margin", "right-margin"] {
        let spec = Value::list(vec![
            Value::list(vec![Value::symbol("margin"), Value::symbol(side)]),
            Value::string(" "),
        ]);
        let classified = classify_display_property(
            spec,
            &crate::display_when::DisplayWhenConditions::structural(),
            DisplayPropertyObject::Buffer,
        );
        let Some(DisplayReplacementProperty::Margin(margin)) = classified.replacement else {
            panic!("({side} …) should be a Margin replacement");
        };
        assert_eq!(
            margin.side(),
            if side == "left-margin" {
                DisplayMarginSide::Left
            } else {
                DisplayMarginSide::Right
            }
        );
        assert!(matches!(
            margin.content(),
            DisplayMarginContent::String(value) if value.as_utf8_str() == Some(" ")
        ));
    }
    // A non-margin cons-headed list is unaffected (still not a margin spec).
    assert!(!matches!(
        classify_display_property(
            Value::list(vec![
                Value::list(vec![Value::symbol("not-margin")]),
                Value::string("x"),
            ]),
            &crate::display_when::DisplayWhenConditions::structural(),
            DisplayPropertyObject::Buffer
        )
        .replacement,
        Some(DisplayReplacementProperty::Margin(_))
    ));
}

#[test]
fn margin_nil_routes_content_to_the_text_area_like_gnu() {
    let _eval = Context::new();
    let classified = classify_display_property(
        Value::list(vec![
            Value::list(vec![Value::symbol("margin"), Value::NIL]),
            Value::string("TEXT"),
        ]),
        &crate::display_when::DisplayWhenConditions::structural(),
        DisplayPropertyObject::Buffer,
    );
    assert_eq!(
        classified.replacement().cloned(),
        Some(DisplayReplacementProperty::String)
    );
    assert_eq!(classified.replacement_spec().as_utf8_str(), Some("TEXT"));
}

#[test]
fn a_vector_display_value_classifies_its_elements_like_gnu() {
    // GNU `handle_display_spec' iterates a VECTOR of specs (`VECTORP (spec)').
    // This classifier had no vector arm, so `'display ["VEC"]' rendered the
    // original text; GNU renders VEC.
    let _eval = Context::new();
    let classified = classify_display_property(
        Value::vector(vec![
            Value::list(vec![Value::symbol("raise"), Value::make_float(0.2)]),
            Value::string("VEC"),
        ]),
        &crate::display_when::DisplayWhenConditions::structural(),
        DisplayPropertyObject::Buffer,
    );
    assert_eq!(
        classified.replacement().cloned(),
        Some(DisplayReplacementProperty::String)
    );
    // The payload is the ELEMENT, not the vector: a consumer reading the whole
    // `display' value would try to display the vector as a string.
    assert_eq!(classified.replacement_spec().as_utf8_str(), Some("VEC"));
}

#[test]
fn a_list_wrapped_string_spec_carries_the_string_not_the_list() {
    // `("LIST")' is a LIST OF SPECS whose single element replaces the text.
    let _eval = Context::new();
    let classified = classify_display_property(
        Value::list(vec![Value::string("LIST")]),
        &crate::display_when::DisplayWhenConditions::structural(),
        DisplayPropertyObject::Buffer,
    );
    assert_eq!(
        classified.replacement().cloned(),
        Some(DisplayReplacementProperty::String)
    );
    assert_eq!(classified.replacement_spec().as_utf8_str(), Some("LIST"));
}

#[test]
fn a_when_spec_replaces_with_its_inner_spec_like_gnu() {
    // `(when FORM . SPEC)': GNU continues its single-spec arms on SPEC. Verified
    // against GNU 31: `(when t . "DOTTED")' replaces, `(when t "LISTED")' -- whose
    // SPEC is the LIST `("LISTED")' -- does not.
    let _eval = Context::new();
    let dotted = classify_display_property(
        Value::cons(
            Value::symbol("when"),
            Value::cons(Value::symbol("t"), Value::string("DOTTED")),
        ),
        &crate::display_when::DisplayWhenConditions::structural(),
        DisplayPropertyObject::Buffer,
    );
    assert_eq!(
        dotted.replacement().cloned(),
        Some(DisplayReplacementProperty::String)
    );
    assert_eq!(dotted.replacement_spec().as_utf8_str(), Some("DOTTED"));

    let listed = classify_display_property(
        Value::list(vec![
            Value::symbol("when"),
            Value::symbol("t"),
            Value::string("LISTED"),
        ]),
        &crate::display_when::DisplayWhenConditions::structural(),
        DisplayPropertyObject::Buffer,
    );
    assert!(listed.replacement().is_none());

    // A nil condition disables the spec.
    let disabled = classify_display_property(
        Value::cons(
            Value::symbol("when"),
            Value::cons(Value::NIL, Value::string("HIDDEN")),
        ),
        &crate::display_when::DisplayWhenConditions::structural(),
        DisplayPropertyObject::Buffer,
    );
    assert!(disabled.replacement().is_none());
}

#[test]
fn a_margin_spec_gnu_cannot_display_leaves_the_text_alone() {
    // Verified against GNU 31: `((margin bogus-area) "BADMARGIN")' displays the
    // covered text unchanged, because GNU's `location' stays unbound and the whole
    // cons then fails its `valid_p' test. Same for a valid area whose CONTENT is
    // not displayable.
    let _eval = Context::new();
    assert!(
        classify_display_property(
            Value::list(vec![
                Value::list(vec![Value::symbol("margin"), Value::symbol("bogus-area")]),
                Value::string("BADMARGIN"),
            ]),
            &crate::display_when::DisplayWhenConditions::structural(),
            DisplayPropertyObject::Buffer
        )
        .replacement()
        .is_none()
    );
    assert!(
        classify_display_property(
            Value::list(vec![
                Value::list(vec![Value::symbol("margin"), Value::NIL]),
                Value::fixnum(42),
            ]),
            &crate::display_when::DisplayWhenConditions::structural(),
            DisplayPropertyObject::Buffer
        )
        .replacement()
        .is_none()
    );
}

#[test]
fn disable_eval_classifies_the_wrapped_spec() {
    // `(disable-eval SPEC)' from enriched.el: GNU strips the wrapper and handles
    // SPEC (refusing to evaluate inside it).
    let _eval = Context::new();
    let classified = classify_display_property(
        Value::list(vec![Value::symbol("disable-eval"), Value::string("INNER")]),
        &crate::display_when::DisplayWhenConditions::structural(),
        DisplayPropertyObject::Buffer,
    );
    assert_eq!(
        classified.replacement().cloned(),
        Some(DisplayReplacementProperty::String)
    );
    assert_eq!(classified.replacement_spec().as_utf8_str(), Some("INNER"));
}

#[test]
fn an_evaluated_when_form_that_did_not_hold_disables_its_spec() {
    // GNU evaluates FORM (xdisp.c:6130-6160) and skips the spec on nil.  With
    // the walk's results in hand, `(display-graphic-p)`-style forms are no
    // longer taken as true just for being non-nil.
    let mut eval = Context::new();
    let call = eval.eval_str("'(some-predicate)").expect("read");
    let spec = Value::cons(
        Value::symbol("when"),
        Value::cons(call, Value::string("SHOWN")),
    );
    let mut results = rustc_hash::FxHashMap::default();
    results.insert(call, false);
    let held_false = crate::display_when::DisplayWhenConditions::evaluated(results);
    assert!(
        classify_display_property(spec, &held_false, DisplayPropertyObject::Buffer)
            .replacement()
            .is_none()
    );
    let mut results = rustc_hash::FxHashMap::default();
    results.insert(call, true);
    let held_true = crate::display_when::DisplayWhenConditions::evaluated(results);
    assert_eq!(
        classify_display_property(spec, &held_true, DisplayPropertyObject::Buffer)
            .replacement()
            .cloned(),
        Some(DisplayReplacementProperty::String)
    );
}

#[test]
fn a_string_object_takes_the_first_replacing_spec_and_a_buffer_the_last() {
    // GNU `handle_display_spec` breaks after the first element that replaced
    // text of a STRING (`if (!it || STRINGP (object)) break;`,
    // xdisp.c:6034-6040) and lets later elements override for buffer text.
    let _eval = Context::new();
    let specs = Value::list(vec![Value::string("FIRST"), Value::string("LAST")]);
    let structural = crate::display_when::DisplayWhenConditions::structural();
    assert_eq!(
        classify_display_property(specs, &structural, DisplayPropertyObject::LispString)
            .replacement_spec()
            .as_utf8_str(),
        Some("FIRST")
    );
    assert_eq!(
        classify_display_property(specs, &structural, DisplayPropertyObject::Buffer)
            .replacement_spec()
            .as_utf8_str(),
        Some("LAST")
    );
}
