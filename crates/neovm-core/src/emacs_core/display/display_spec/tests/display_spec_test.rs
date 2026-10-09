//! Tests for the `display` property lens: every GNU spec shape and head.

use super::*;
use crate::emacs_core::value::Value;

fn list(items: Vec<Value>) -> Value {
    Value::list(items)
}

fn symbol(name: &str) -> Value {
    Value::symbol(name)
}

fn string(text: &str) -> Value {
    Value::string(text)
}

fn collect(value: Value) -> Vec<Value> {
    let mut seen = Vec::new();
    DisplayPropertySpecs::of(value).for_each(|spec| {
        seen.push(spec);
        ControlFlow::Continue(())
    });
    seen
}

#[test]
fn display_spec_kind_classifies_every_gnu_head() {
    // GNU handle_single_display_spec's arms, in its own order.
    let cases: Vec<(Value, DisplaySpecKind)> = vec![
        (
            list(vec![symbol("when"), symbol("t"), string("x")]),
            DisplaySpecKind::When,
        ),
        (
            list(vec![symbol("height"), Value::fixnum(2)]),
            DisplaySpecKind::Height,
        ),
        (
            list(vec![symbol("space-width"), Value::fixnum(2)]),
            DisplaySpecKind::SpaceWidth,
        ),
        (
            list(vec![symbol("min-width"), list(vec![Value::fixnum(5)])]),
            DisplaySpecKind::MinWidth,
        ),
        (
            list(vec![symbol("slice"), Value::fixnum(0), Value::fixnum(0)]),
            DisplaySpecKind::Slice,
        ),
        (
            list(vec![symbol("raise"), Value::fixnum(1)]),
            DisplaySpecKind::Raise,
        ),
        (
            list(vec![symbol("left-fringe"), symbol("question-mark")]),
            DisplaySpecKind::LeftFringe,
        ),
        (
            list(vec![symbol("right-fringe"), symbol("question-mark")]),
            DisplaySpecKind::RightFringe,
        ),
        (
            list(vec![symbol("space"), Value::keyword("width")]),
            DisplaySpecKind::Space,
        ),
        (
            list(vec![symbol("image"), Value::keyword("file")]),
            DisplaySpecKind::Image,
        ),
        (
            list(vec![symbol("xwidget"), Value::keyword("id")]),
            DisplaySpecKind::Xwidget,
        ),
        (
            list(vec![
                list(vec![symbol("margin"), symbol("left-margin")]),
                string("m"),
            ]),
            DisplaySpecKind::Margin,
        ),
        (string("replacement"), DisplaySpecKind::Text),
        // neomacs supersets, deliberately classified as single specs.
        (
            list(vec![symbol("video"), Value::keyword("file")]),
            DisplaySpecKind::Media(DisplayMediaSpecKind::Video),
        ),
        (
            list(vec![symbol("webkit"), Value::keyword("uri")]),
            DisplaySpecKind::Media(DisplayMediaSpecKind::Webkit),
        ),
        (
            list(vec![symbol("surface"), Value::keyword("id")]),
            DisplaySpecKind::Media(DisplayMediaSpecKind::Surface),
        ),
        (
            list(vec![Value::keyword("raise"), Value::fixnum(1)]),
            DisplaySpecKind::KeywordPlist,
        ),
        // Unrecognized: a cons of this kind is a LIST of specs at the shape level.
        (
            list(vec![symbol("unknown-head"), Value::fixnum(1)]),
            DisplaySpecKind::Other,
        ),
        (Value::fixnum(7), DisplaySpecKind::Other),
    ];
    for (spec, expected) in cases {
        assert_eq!(
            display_spec_kind(spec),
            expected,
            "spec {spec:?} classified wrongly"
        );
    }
}

#[test]
fn a_margin_head_stays_a_single_spec_even_with_an_area_gnu_rejects() {
    // GNU's shape test looks at the `margin' head only, so this is ONE spec --
    // iterating it as a list would let the inner string replace the text, which
    // GNU never does (its `location' stays unbound and nothing is displayed).
    let spec = list(vec![
        list(vec![symbol("margin"), symbol("bogus-area")]),
        string("m"),
    ]);
    assert_eq!(display_spec_kind(spec), DisplaySpecKind::Margin);
    assert!(!DisplayPropertySpecs::of(spec).is_spec_list());
    assert_eq!(display_spec_margin_value(spec), None);

    let accepted = list(vec![
        list(vec![symbol("margin"), symbol("right-margin")]),
        string("m"),
    ]);
    let typed = display_margin_spec(accepted).expect("valid right margin spec");
    assert_eq!(typed.location(), DisplayMarginLocation::Right);
    assert_eq!(typed.content().as_utf8_str(), Some("m"));
    assert_eq!(
        display_spec_margin_value(accepted).and_then(|value| value.as_utf8_str()),
        Some("m")
    );

    let text_area = list(vec![
        list(vec![symbol("margin"), Value::NIL]),
        string("text"),
    ]);
    assert_eq!(
        display_margin_spec(text_area).map(DisplayMarginSpec::location),
        Some(DisplayMarginLocation::Text)
    );
}

#[test]
fn a_vector_display_value_iterates_its_elements_like_gnu() {
    // GNU handle_display_spec's `VECTORP (spec)' arm. Missing it made
    // `(put-text-property … 'display ["X"])' render nothing at all.
    let value = Value::vector(vec![
        list(vec![symbol("raise"), Value::fixnum(1)]),
        string("VEC"),
    ]);
    let seen = collect(value);
    assert_eq!(seen.len(), 2);
    assert_eq!(display_spec_kind(seen[0]), DisplaySpecKind::Raise);
    assert_eq!(display_spec_kind(seen[1]), DisplaySpecKind::Text);
}

#[test]
fn a_list_of_specs_iterates_but_a_single_spec_does_not() {
    // `("LIST")' — car is not a recognized head, so the cons is a LIST of specs.
    let spec_list = list(vec![string("LIST")]);
    assert!(DisplayPropertySpecs::of(spec_list).is_spec_list());
    let seen = collect(spec_list);
    assert_eq!(seen.len(), 1);
    assert_eq!(display_spec_kind(seen[0]), DisplaySpecKind::Text);

    // `(space :width 2)' — a recognized head, so ONE spec, not three.
    let single = list(vec![
        symbol("space"),
        Value::keyword("width"),
        Value::fixnum(2),
    ]);
    assert!(!DisplayPropertySpecs::of(single).is_spec_list());
    assert_eq!(collect(single).len(), 1);
}

#[test]
fn disable_eval_unwraps_to_the_inner_spec_and_forbids_eval() {
    // `(disable-eval SPEC)' from enriched.el.
    let value = list(vec![symbol("disable-eval"), string("INNER")]);
    let specs = DisplayPropertySpecs::of(value);
    assert!(!specs.eval_enabled);
    let seen = collect(value);
    assert_eq!(seen.len(), 1);
    assert_eq!(display_spec_kind(seen[0]), DisplaySpecKind::Text);

    assert!(DisplayPropertySpecs::of(string("plain")).eval_enabled);
}

#[test]
fn a_nil_car_keeps_the_value_a_single_spec() {
    // GNU's shape test excludes a nil car explicitly (`!NILP (XCAR (spec))').
    let value = list(vec![Value::NIL, string("x")]);
    assert!(!DisplayPropertySpecs::of(value).is_spec_list());
    assert_eq!(collect(value).len(), 1);
}

#[test]
fn when_parts_split_form_from_the_inner_spec() {
    // `(when FORM . VALUE)': the spec continues with the cdr AFTER the form.
    let spec = list(vec![symbol("when"), symbol("t"), string("x")]);
    let (form, inner) = display_spec_when_parts(spec).expect("when spec");
    assert!(form.is_truthy());
    // VALUE is the tail `("x")', which is itself a list of specs.
    assert!(DisplayPropertySpecs::of(inner).is_spec_list());

    assert_eq!(display_spec_when_parts(string("x")), None);
}

#[test]
fn replaces_text_matches_gnu_validity_including_tty_frames() {
    // Fringe and `(space …)' replace text on a tty too; image-class specs are
    // `valid_image_p' only on a window frame.
    assert_eq!(DisplaySpecKind::Space.replaces_text(false), Some(true));
    assert_eq!(DisplaySpecKind::LeftFringe.replaces_text(false), Some(true));
    assert_eq!(DisplaySpecKind::Text.replaces_text(false), Some(true));
    assert_eq!(DisplaySpecKind::Image.replaces_text(false), Some(false));
    assert_eq!(DisplaySpecKind::Image.replaces_text(true), Some(true));
    assert_eq!(DisplaySpecKind::Xwidget.replaces_text(false), Some(false));
    assert_eq!(DisplaySpecKind::Raise.replaces_text(true), Some(false));
    assert_eq!(
        DisplaySpecKind::KeywordPlist.replaces_text(true),
        Some(false)
    );
    // `when'/`margin' defer to their inner spec.
    assert_eq!(DisplaySpecKind::When.replaces_text(true), None);
    assert_eq!(DisplaySpecKind::Margin.replaces_text(true), None);
}
