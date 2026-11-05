use super::*;

fn foreground_face(specification: &str, realized: Option<Color>) -> Face {
    let mut face = Face::default();
    assert!(face.set_attribute(
        LFaceAttr::Foreground,
        FaceAttrValue::SpecifiedColor {
            specification: SpecifiedColor::parse(specification),
            realized,
        },
    ));
    face
}

#[test]
fn color_specifications_retain_failed_lookup_and_exact_hex_input() {
    crate::test_utils::init_test_tracing();
    let missing = Face::from_plist(
        "missing",
        &[
            Value::keyword("foreground"),
            Value::string("lane-missing-color"),
        ],
    );
    assert_eq!(missing.foreground, None);
    assert_eq!(
        missing.color_specifications.foreground.as_deref(),
        Some(&SpecifiedColor::Named("lane-missing-color".to_owned()))
    );
    let wide = Face::from_plist(
        "wide",
        &[Value::keyword("background"), Value::string("#ffff33330000")],
    );
    assert_eq!(wide.background, Some(Color::rgb(255, 51, 0)));
    assert_eq!(
        wide.color_specifications.background.as_deref(),
        Some(&SpecifiedColor::Named("#ffff33330000".to_owned()))
    );
}

#[test]
fn color_specifications_follow_inheritance_and_failed_override() {
    crate::test_utils::init_test_tracing();
    let mut table = FaceTable::new();
    table.define(
        "retained-parent",
        foreground_face("red", Some(Color::rgb(255, 0, 0))),
    );
    let mut child = Face::default();
    child.inherit = Some(Value::symbol("retained-parent"));
    table.define("retained-child", child.clone());
    assert_eq!(
        table
            .resolve("retained-child")
            .color_specifications
            .foreground
            .as_deref(),
        Some(&SpecifiedColor::Named("red".to_owned()))
    );
    assert!(child.set_attribute(
        LFaceAttr::Foreground,
        FaceAttrValue::SpecifiedColor {
            specification: SpecifiedColor::parse("retry-after-palette-change"),
            realized: None,
        },
    ));
    table.define("retained-child", child);
    assert_eq!(
        table
            .resolve("retained-child")
            .color_specifications
            .foreground
            .as_deref(),
        Some(&SpecifiedColor::Named(
            "retry-after-palette-change".to_owned()
        ))
    );
}

#[test]
fn color_specifications_programmatic_override_drops_inherited_source() {
    crate::test_utils::init_test_tracing();
    let parent = foreground_face("red", Some(Color::rgb(255, 0, 0)));
    let mut overlay = Face::default();
    assert!(overlay.set_attribute(
        LFaceAttr::Foreground,
        FaceAttrValue::Color(Color::rgb(1, 2, 3))
    ));
    let merged = parent.merge(&overlay);
    assert_eq!(merged.foreground, Some(Color::rgb(1, 2, 3)));
    assert_eq!(merged.color_specifications.foreground, None);
    assert!(overlay.set_attribute(LFaceAttr::Foreground, FaceAttrValue::Unspecified));
    assert_eq!(parent.merge(&overlay), parent);
}

#[test]
fn color_specifications_underline_disable_and_uncolored_override_clear_source() {
    crate::test_utils::init_test_tracing();
    let parent = Face::from_plist(
        "underline-parent",
        &[
            Value::keyword("underline"),
            Value::string("lane-underline-color"),
        ],
    );
    assert_eq!(
        parent.color_specifications.underline.as_deref(),
        Some(&SpecifiedColor::Named("lane-underline-color".to_owned()))
    );
    let mut overlay = Face::default();
    assert!(overlay.set_attribute(LFaceAttr::Underline, FaceAttrValue::Bool(false)));
    assert_eq!(parent.merge(&overlay).underline, FaceDecoration::Disabled);
    assert_eq!(parent.merge(&overlay).color_specifications.underline, None);
    assert!(overlay.set_attribute(LFaceAttr::Underline, FaceAttrValue::Bool(true)));
    assert_eq!(parent.merge(&overlay).color_specifications.underline, None);
    assert!(overlay.set_attribute(LFaceAttr::Underline, FaceAttrValue::Unspecified));
    assert_eq!(parent.merge(&overlay), parent);
}

#[test]
fn color_specifications_underline_plist_retains_last_color_and_clones_share_input() {
    crate::test_utils::init_test_tracing();
    let underline = Value::list(vec![
        Value::keyword("color"),
        Value::string("red"),
        Value::keyword("color"),
        Value::string("#123456"),
        Value::keyword("style"),
        Value::symbol("wave"),
    ]);
    let face = Face::from_plist("underline", &[Value::keyword("underline"), underline]);
    assert_eq!(
        face.color_specifications.underline.as_deref(),
        Some(&SpecifiedColor::Named("#123456".to_owned()))
    );
    let cloned = face.clone();
    assert!(std::sync::Arc::ptr_eq(
        face.color_specifications.underline.as_ref().unwrap(),
        cloned.color_specifications.underline.as_ref().unwrap(),
    ));
}

#[test]
fn color_specifications_typed_underline_and_color_updates_are_atomic() {
    crate::test_utils::init_test_tracing();
    let mut face = foreground_face("old", None);
    assert!(face.set_attribute(
        LFaceAttr::Foreground,
        FaceAttrValue::SpecifiedColor {
            specification: SpecifiedColor::FrameForeground,
            realized: None,
        },
    ));
    assert_eq!(face.foreground, None);
    assert_eq!(
        face.color_specifications.foreground.as_deref(),
        Some(&SpecifiedColor::FrameForeground)
    );
    assert!(face.set_attribute(
        LFaceAttr::Underline,
        FaceAttrValue::SpecifiedUnderline {
            underline: Underline {
                style: UnderlineStyle::Line,
                color: Some(Color::rgb(4, 5, 6)),
                position: UnderlinePosition::FontMetric,
            },
            color: Some(SpecifiedColor::parse("retry-underline")),
        },
    ));
    assert_eq!(
        face.underline.enabled().unwrap().color,
        Some(Color::rgb(4, 5, 6))
    );
    assert_eq!(
        face.color_specifications.underline.as_deref(),
        Some(&SpecifiedColor::Named("retry-underline".to_owned()))
    );
    assert!(!face.set_attribute(LFaceAttr::Underline, FaceAttrValue::Text(Value::NIL)));
    assert!(face.color_specifications.underline.is_some());
}

#[test]
fn underline_operations_preserve_lookup_order_resets_and_shared_clone_storage() {
    crate::test_utils::init_test_tracing();
    let attribute = Value::list(vec![
        Value::keyword("color"),
        Value::string("#111111"),
        Value::keyword("color"),
        Value::symbol("foreground-color"),
        Value::keyword("color"),
        Value::string("#222222"),
    ]);
    let face = Face::from_plist("ordered", &[Value::keyword("underline"), attribute]);
    assert_eq!(
        face.color_specifications.underline_operations.as_deref(),
        Some(
            [
                UnderlineColorOperation::Lookup(SpecifiedColor::parse("#111111")),
                UnderlineColorOperation::Foreground,
                UnderlineColorOperation::Lookup(SpecifiedColor::parse("#222222")),
            ]
            .as_slice()
        )
    );
    assert_eq!(
        face.underline.enabled().unwrap().color,
        Some(Color::rgb(34, 34, 34))
    );
    let cloned = face.clone();
    assert!(std::sync::Arc::ptr_eq(
        face.color_specifications
            .underline_operations
            .as_ref()
            .unwrap(),
        cloned
            .color_specifications
            .underline_operations
            .as_ref()
            .unwrap(),
    ));
    assert_eq!(face.merge(&Face::default()), face);
    let mut overlay = Face::default();
    assert!(overlay.set_attribute(LFaceAttr::Underline, FaceAttrValue::Bool(true)));
    assert_eq!(
        face.merge(&overlay)
            .color_specifications
            .underline_operations,
        None
    );
}

#[test]
fn underline_operations_distinguish_equal_paint_with_different_callback_sequences() {
    crate::test_utils::init_test_tracing();
    let attribute = |colors: &[&str]| {
        Value::list(
            colors
                .iter()
                .flat_map(|color| [Value::keyword("color"), Value::string(*color)])
                .collect(),
        )
    };
    let first = Face::from_plist(
        "first",
        &[
            Value::keyword("underline"),
            attribute(&["#111111", "#222222"]),
        ],
    );
    let second = Face::from_plist(
        "second",
        &[
            Value::keyword("underline"),
            attribute(&["#222222", "#111111", "#222222"]),
        ],
    );
    assert_eq!(first.underline, second.underline);
    assert_eq!(
        first.color_specifications.underline,
        second.color_specifications.underline
    );
    assert_ne!(first, second);

    let plain = Face::from_plist(
        "plain",
        &[Value::keyword("underline"), Value::string("#222222")],
    );
    let plist = Face::from_plist(
        "plist",
        &[Value::keyword("underline"), attribute(&["#222222"])],
    );
    assert_eq!(plain.underline, plist.underline);
    assert_ne!(plain, plist);
}
