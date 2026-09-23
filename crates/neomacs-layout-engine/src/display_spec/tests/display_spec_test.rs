use super::*;
use neovm_core::emacs_core::image_catalog::{ImageDataSource, ImageResolveSource};

fn image_spec(ascent: Option<Value>) -> Value {
    let mut items = vec![
        Value::symbol("image"),
        Value::keyword("type"),
        Value::symbol("svg"),
        Value::keyword("file"),
        Value::string("/tmp/icon.svg"),
    ];
    if let Some(ascent) = ascent {
        items.push(Value::keyword("ascent"));
        items.push(ascent);
    }
    Value::list(items)
}

fn parsed_image_ascent(value: Option<Value>) -> DisplayImageAscentPolicy {
    parse_display_image_layout(&image_spec(value), 0, 0)
        .expect("valid image spec")
        .ascent
}

#[test]
fn image_data_base_uri_survives_as_an_explicit_resource_capability() {
    let mut eval = neovm_core::emacs_core::Context::new();
    eval.setup_thread_locals();
    let image = Value::list(vec![
        Value::symbol("image"),
        Value::keyword("type"),
        Value::symbol("svg"),
        Value::keyword("data"),
        Value::string("<svg/>"),
        Value::keyword("base-uri"),
        Value::string("/tmp/telega/dummy"),
    ]);

    let parsed = parse_display_image_layout(&image, 0, 0).expect("valid data image");
    let ImageResolveSource::Data(ImageDataSource::WithBaseUri { data, base_uri }) =
        parsed.request.source
    else {
        panic!("data plus :base-uri must remain a capability-bearing source");
    };
    assert_eq!(data, b"<svg/>");
    assert_eq!(base_uri.as_utf8_str(), Some("/tmp/telega/dummy"));
}

#[test]
fn image_slice_resolves_fixnums_as_pixels_and_floats_as_fractions() {
    let mut eval = neovm_core::emacs_core::Context::new();
    eval.setup_thread_locals();
    let slice = parse_display_image_slice(Value::list(vec![
        Value::symbol("slice"),
        Value::fixnum(8),
        Value::make_float(0.25),
        Value::make_float(0.5),
        Value::fixnum(10),
    ]))
    .expect("slice spec");

    let resolved = slice.resolve(40.0, 80.0).expect("non-empty crop");
    assert_eq!((resolved.width, resolved.height), (20.0, 10.0));
    let tolerance = 2.0 / u16::MAX as f32;
    assert!((resolved.source_rect.x() - 0.2).abs() <= tolerance);
    assert!((resolved.source_rect.y() - 0.25).abs() <= tolerance);
    assert!((resolved.source_rect.width() - 0.5).abs() <= tolerance);
    assert!((resolved.source_rect.height() - 0.125).abs() <= tolerance);
}

#[test]
fn image_scale_default_survives_parsing_until_frame_realization() {
    let mut eval = neovm_core::emacs_core::Context::new();
    eval.setup_thread_locals();
    let image = Value::list(vec![
        Value::symbol("image"),
        Value::keyword("file"),
        Value::string("/tmp/icon.svg"),
        Value::keyword("height"),
        Value::fixnum(24),
        Value::keyword("scale"),
        Value::symbol("default"),
    ]);

    let parsed = parse_display_image_layout(&image, 0, 0).expect("valid image spec");
    assert_eq!(parsed.scale, ImageScalePolicy::Default);

    let request = parsed.into_resolve_request(
        ImageScaleEnvironment::new(
            7.2,
            1.75,
            neovm_core::emacs_core::image_catalog::ImageDefaultScale::Auto,
        ),
        DisplayImageDimensionEnvironment::new(14.0, 18.0, 7.2),
    );
    assert_eq!(request.realization.layout_dimension(24), 18);
    assert_eq!(request.realization.raster_dimension(18), 32);
}

#[test]
fn image_dimensions_resolve_gnu_face_relative_units_in_logical_pixels() {
    let mut eval = neovm_core::emacs_core::Context::new();
    eval.setup_thread_locals();
    let dimensions = DisplayImageDimensionEnvironment::new(14.0, 18.0, 7.2);
    let scale = ImageScaleEnvironment::default();

    let exact = Value::list(vec![
        Value::symbol("image"),
        Value::keyword("file"),
        Value::string("/tmp/icon.svg"),
        Value::keyword("width"),
        Value::cons(Value::make_float(1.5), Value::symbol("em")),
        Value::keyword("max-width"),
        Value::cons(Value::fixnum(99), Value::symbol("cw")),
        Value::keyword("height"),
        Value::cons(Value::make_float(0.5), Value::symbol("ch")),
    ]);
    let exact_request = parse_display_image_layout(&exact, 0, 0)
        .expect("valid image spec")
        .into_resolve_request(scale, dimensions);
    assert_eq!(
        exact_request.size,
        ImageSizeSpec::new(AxisSize::Exact(21), AxisSize::Exact(9)),
        ":width must override :max-width before resolving relative units"
    );

    let clamped = Value::list(vec![
        Value::symbol("image"),
        Value::keyword("file"),
        Value::string("/tmp/icon.svg"),
        Value::keyword("max-width"),
        Value::cons(Value::fixnum(2), Value::symbol("cw")),
        Value::keyword("max-height"),
        Value::fixnum(18),
    ]);
    let clamped_request = parse_display_image_layout(&clamped, 0, 0)
        .expect("valid image spec")
        .into_resolve_request(scale, dimensions);
    assert_eq!(
        clamped_request.size,
        ImageSizeSpec::new(AxisSize::AtMost(15), AxisSize::AtMost(18))
    );
}

#[test]
fn image_ascent_parses_gnu_domain_and_defaults_invalid_values() {
    let _eval = neovm_core::emacs_core::Context::new();

    assert_eq!(
        parsed_image_ascent(None),
        DisplayImageAscentPolicy::Percent(50.0)
    );
    assert_eq!(
        parsed_image_ascent(Some(Value::symbol("center"))),
        DisplayImageAscentPolicy::Center
    );
    assert_eq!(
        parsed_image_ascent(Some(Value::fixnum(0))),
        DisplayImageAscentPolicy::Percent(0.0)
    );
    assert_eq!(
        parsed_image_ascent(Some(Value::fixnum(100))),
        DisplayImageAscentPolicy::Percent(100.0)
    );
    assert_eq!(
        parsed_image_ascent(Some(Value::fixnum(101))),
        DisplayImageAscentPolicy::Percent(50.0)
    );
    assert_eq!(
        parsed_image_ascent(Some(Value::make_float(75.0))),
        DisplayImageAscentPolicy::Percent(50.0)
    );
}

#[test]
fn image_margin_preserves_gnu_scalar_and_pair_geometry() {
    let mut eval = neovm_core::emacs_core::Context::new();
    eval.setup_thread_locals();
    let scalar = Value::list(vec![
        Value::symbol("image"),
        Value::keyword("file"),
        Value::string("/tmp/icon.svg"),
        Value::keyword("margin"),
        Value::fixnum(2),
    ]);
    let pair = Value::list(vec![
        Value::symbol("image"),
        Value::keyword("file"),
        Value::string("/tmp/icon.svg"),
        Value::keyword("margin"),
        Value::cons(Value::fixnum(3), Value::fixnum(4)),
    ]);

    assert_eq!(
        parse_display_image_layout(&scalar, 0, 0).unwrap().margin,
        DisplayImageMargin {
            horizontal: 2.0,
            vertical: 2.0,
        }
    );
    assert_eq!(
        parse_display_image_layout(&pair, 0, 0).unwrap().margin,
        DisplayImageMargin {
            horizontal: 3.0,
            vertical: 4.0,
        }
    );
}

#[test]
fn image_background_is_only_decoder_input_not_opacity_evidence() {
    let mut eval = neovm_core::emacs_core::Context::new();
    eval.setup_thread_locals();
    let explicit = Value::list(vec![
        Value::symbol("image"),
        Value::keyword("file"),
        Value::string("/tmp/icon.svg"),
        Value::keyword("background"),
        Value::string("#123456"),
    ]);

    assert_eq!(
        parse_display_image_layout(&explicit, 0, 0)
            .unwrap()
            .request
            .colors
            .background()
            .rgb24(),
        0x12_34_56
    );
}

#[test]
fn video_opacity_is_clamped_at_the_typed_display_boundary() {
    let _eval = neovm_core::emacs_core::Context::new();
    let spec = |opacity| {
        Value::list(vec![
            Value::symbol("video"),
            Value::keyword("file"),
            Value::string("/tmp/movie.mp4"),
            Value::keyword("opacity"),
            Value::make_float(opacity),
        ])
    };

    assert_eq!(
        parse_display_video_layout(&spec(0.4), 10.0, 10.0)
            .unwrap()
            .opacity,
        0.4
    );
    assert_eq!(
        parse_display_video_layout(&spec(2.0), 10.0, 10.0)
            .unwrap()
            .opacity,
        1.0
    );
    assert_eq!(
        parse_display_video_layout(&spec(-1.0), 10.0, 10.0)
            .unwrap()
            .opacity,
        0.0
    );
}

#[test]
fn parse_display_fringe_layout_left_with_face() {
    let _eval = neovm_core::emacs_core::Context::new();
    let layout = parse_display_fringe_layout(&Value::list(vec![
        Value::symbol("left-fringe"),
        Value::symbol("magit-fringe-bitmap>"),
        Value::symbol("magit-section-heading"),
    ]))
    .expect("left fringe layout");
    assert_eq!(layout.side, DisplayFringeSide::Left);
    assert!(layout.bitmap.is_symbol_named("magit-fringe-bitmap>"));
    assert!(
        layout
            .face
            .is_some_and(|f| f.is_symbol_named("magit-section-heading"))
    );
}

#[test]
fn parse_display_fringe_layout_right_without_face() {
    let _eval = neovm_core::emacs_core::Context::new();
    let layout = parse_display_fringe_layout(&Value::list(vec![
        Value::symbol("right-fringe"),
        Value::symbol("right-arrow"),
    ]))
    .expect("right fringe layout");
    assert_eq!(layout.side, DisplayFringeSide::Right);
    assert!(layout.bitmap.is_symbol_named("right-arrow"));
    assert!(layout.face.is_none());
}

#[test]
fn parse_display_fringe_layout_rejects_non_fringe_and_missing_bitmap() {
    let _eval = neovm_core::emacs_core::Context::new();
    // Not a fringe head.
    assert!(
        parse_display_fringe_layout(&Value::list(vec![
            Value::symbol("space"),
            Value::keyword(":width"),
        ]))
        .is_none()
    );
    // Missing BITMAP.
    assert!(
        parse_display_fringe_layout(&Value::list(vec![Value::symbol("left-fringe")])).is_none()
    );
}

#[test]
fn display_space_keys_match_gnu_keyword_domain() {
    let keys = [
        (DisplaySpaceKey::Width, ":width"),
        (DisplaySpaceKey::RelativeWidth, ":relative-width"),
        (DisplaySpaceKey::AlignTo, ":align-to"),
        (DisplaySpaceKey::Height, ":height"),
        (DisplaySpaceKey::RelativeHeight, ":relative-height"),
        (DisplaySpaceKey::Ascent, ":ascent"),
    ];

    for (key, keyword) in keys {
        assert_eq!(key.keyword(), keyword);
        assert_eq!(DisplaySpaceKey::from_keyword(keyword), Some(key));
        assert_eq!(DisplaySpaceKey::from_lisp_value(key.value()), Some(key));
    }

    assert_eq!(DisplaySpaceKey::from_keyword("width"), None);
    assert_eq!(DisplaySpaceKey::from_keyword(":foreground"), None);
    assert_eq!(
        DisplaySpaceKey::from_lisp_value(Value::symbol("width")),
        None
    );
}

#[test]
fn display_media_keys_match_lisp_keyword_domain() {
    let keys = [
        (DisplayMediaKey::File, ":file"),
        (DisplayMediaKey::Uri, ":uri"),
        (DisplayMediaKey::Width, ":width"),
        (DisplayMediaKey::Height, ":height"),
        (DisplayMediaKey::Loop, ":loop"),
        (DisplayMediaKey::LoopCount, ":loop-count"),
        (DisplayMediaKey::Autoplay, ":autoplay"),
        (DisplayMediaKey::Xwidget, ":xwidget"),
    ];

    for (key, name) in keys {
        assert_eq!(
            DisplayMediaKey::from_lisp_value(Value::symbol(name)),
            Some(key)
        );
        let serialized: &'static str = key.into();
        assert_eq!(serialized, name);
    }

    assert_eq!(
        DisplayMediaKey::from_lisp_value(Value::symbol("width")),
        None
    );
    assert_eq!(
        DisplayMediaKey::from_lisp_value(Value::symbol(":foreground")),
        None
    );
}
