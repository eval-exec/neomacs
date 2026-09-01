use super::*;

#[test]
fn color_from_hex() {
    crate::test_utils::init_test_tracing();
    assert_eq!(Color::from_hex("#ff0000"), Some(Color::rgb(255, 0, 0)));
    assert_eq!(Color::from_hex("#00ff00"), Some(Color::rgb(0, 255, 0)));
    assert_eq!(Color::from_hex("#abc"), Some(Color::rgb(170, 187, 204)));
    assert_eq!(Color::from_hex("invalid"), None);
    // Wide GNU/X11 forms: 12 hex digits (16-bit channels, what `color-values`
    // and blend math emit — e.g. indent-bars' computed bar colors). Each
    // channel downscales to its most-significant 8 bits.
    assert_eq!(
        Color::from_hex("#ffff33333333"),
        Some(Color::rgb(0xff, 0x33, 0x33))
    );
    assert_eq!(
        Color::from_hex("#ffffffffffff"),
        Some(Color::rgb(255, 255, 255))
    );
    assert_eq!(Color::from_hex("#000000000000"), Some(Color::rgb(0, 0, 0)));
    // 9 hex digits (12-bit channels).
    assert_eq!(
        Color::from_hex("#fff333000"),
        Some(Color::rgb(0xff, 0x33, 0x00))
    );
    // Bad lengths still reject.
    assert_eq!(Color::from_hex("#abcd"), None);
    assert_eq!(Color::from_hex("#fffffffffffff"), None);
}

#[test]
fn color_to_hex() {
    crate::test_utils::init_test_tracing();
    assert_eq!(Color::rgb(255, 0, 128).to_hex(), "#ff0080");
}

#[test]
fn color_from_name() {
    crate::test_utils::init_test_tracing();
    assert_eq!(Color::from_name("red"), Some(Color::rgb(255, 0, 0)));
    assert_eq!(Color::from_name("RED"), Some(Color::rgb(255, 0, 0)));
    assert_eq!(Color::from_name("nonexistent"), None);
}

#[test]
fn named_colors_follow_gnu_x11_rgb_database() {
    crate::test_utils::init_test_tracing();
    for (name, expected) in [
        ("green", Color::rgb(0, 255, 0)),
        ("gray", Color::rgb(190, 190, 190)),
        ("purple", Color::rgb(160, 32, 240)),
        ("maroon", Color::rgb(176, 48, 96)),
    ] {
        assert_eq!(Color::from_name(name), Some(expected), "{name}");
    }
}

#[test]
fn lface_attr_keyword_mapping_matches_gnu_slots() {
    crate::test_utils::init_test_tracing();
    let attrs = [
        (LFaceAttr::Family, 1, ":family"),
        (LFaceAttr::Foundry, 2, ":foundry"),
        (LFaceAttr::Width, 3, ":width"),
        (LFaceAttr::Height, 4, ":height"),
        (LFaceAttr::Weight, 5, ":weight"),
        (LFaceAttr::Slant, 6, ":slant"),
        (LFaceAttr::Underline, 7, ":underline"),
        (LFaceAttr::InverseVideo, 8, ":inverse-video"),
        (LFaceAttr::Foreground, 9, ":foreground"),
        (LFaceAttr::Background, 10, ":background"),
        (LFaceAttr::Stipple, 11, ":stipple"),
        (LFaceAttr::Overline, 12, ":overline"),
        (LFaceAttr::StrikeThrough, 13, ":strike-through"),
        (LFaceAttr::Box, 14, ":box"),
        (LFaceAttr::Font, 15, ":font"),
        (LFaceAttr::Inherit, 16, ":inherit"),
        (LFaceAttr::Fontset, 17, ":fontset"),
        (LFaceAttr::DistantForeground, 18, ":distant-foreground"),
        (LFaceAttr::Extend, 19, ":extend"),
    ];

    assert_eq!(LFACE_VECTOR_SIZE, 20);
    assert_eq!(
        LFACE_ATTRS,
        attrs.map(|(attr, _, _)| attr),
        "LFACE_ATTRS must stay in GNU xfaces.c face_attr_sym slot order"
    );

    for (attr, index, keyword) in attrs {
        assert_eq!(attr.index(), index);
        assert_eq!(LFaceAttr::from_index(index), Some(attr));
        assert_eq!(attr.keyword(), keyword);
        assert_eq!(LFaceAttr::from_keyword(keyword), Some(attr));
    }

    assert_eq!(LFaceAttr::from_index(0), None);
    assert_eq!(LFaceAttr::from_index(20), None);
    assert_eq!(LFaceAttr::from_keyword("foreground"), None);
    assert_eq!(LFaceAttr::from_keyword(":bold"), None);
    assert_eq!(LFaceAttr::from_keyword(":italic"), None);
}

#[test]
fn underline_style_codes_match_gnu_face_underline_type() {
    crate::test_utils::init_test_tracing();
    let styles = [
        (UnderlineStyle::Line, 1, "line"),
        (UnderlineStyle::DoubleLine, 2, "double-line"),
        (UnderlineStyle::Wave, 3, "wave"),
        (UnderlineStyle::Dots, 4, "dots"),
        (UnderlineStyle::Dashes, 5, "dashes"),
    ];

    for (style, code, symbol) in styles {
        assert_eq!(style.gnu_code(), code);
        assert_eq!(UnderlineStyle::from_gnu_code(code), Some(style));
        assert_eq!(style.symbol_name(), symbol);
        assert_eq!(UnderlineStyle::from_symbol(symbol), Some(style));
    }

    assert_eq!(UnderlineStyle::from_gnu_code(0), None);
    assert_eq!(UnderlineStyle::from_gnu_code(6), None);
    assert_eq!(UnderlineStyle::from_symbol("dash"), None);
    assert_eq!(UnderlineStyle::from_symbol("double"), None);
}

#[test]
fn box_style_codes_match_gnu_face_box_type() {
    crate::test_utils::init_test_tracing();
    let styles = [
        (BoxStyle::Flat, 1, "flat-button"),
        (BoxStyle::Raised, 2, "released-button"),
        (BoxStyle::Pressed, 3, "pressed-button"),
    ];

    for (style, code, symbol) in styles {
        assert_eq!(style.gnu_code(), code);
        assert_eq!(BoxStyle::from_gnu_code(code), Some(style));
        assert_eq!(style.symbol_name(), symbol);
        assert_eq!(BoxStyle::from_symbol(symbol), Some(style));
    }

    assert_eq!(BoxStyle::from_gnu_code(0), None);
    assert_eq!(BoxStyle::from_gnu_code(4), None);
    assert_eq!(BoxStyle::from_symbol("flat"), None);
    assert_eq!(BoxStyle::from_symbol("raised"), None);
}

#[test]
fn face_merge() {
    crate::test_utils::init_test_tracing();
    let base = Face {
        foreground: Some(Color::rgb(0, 0, 0)),
        background: Some(Color::rgb(255, 255, 255)),
        ..Default::default()
    };
    let overlay = Face {
        foreground: Some(Color::rgb(255, 0, 0)),
        ..Default::default()
    };

    let merged = base.merge(&overlay);
    assert_eq!(merged.foreground, Some(Color::rgb(255, 0, 0))); // overlay wins
    assert_eq!(merged.background, Some(Color::rgb(255, 255, 255))); // base preserved
}

#[test]
fn face_inverse_video() {
    crate::test_utils::init_test_tracing();
    let face = Face {
        foreground: Some(Color::rgb(255, 255, 255)),
        background: Some(Color::rgb(0, 0, 0)),
        inverse_video: Some(true),
        ..Default::default()
    };

    assert_eq!(face.effective_foreground(), Some(Color::rgb(0, 0, 0)));
    assert_eq!(face.effective_background(), Some(Color::rgb(255, 255, 255)));
}

#[test]
fn face_table_standard_faces() {
    crate::test_utils::init_test_tracing();
    let table = FaceTable::new();
    assert!(table.get("default").is_some());
    assert!(table.get("bold").is_some());
    assert!(table.get("italic").is_some());
    assert!(table.get("mode-line").is_some());
    assert!(table.get("mode-line-active").is_some());
    assert!(table.get("tool-bar").is_some());
    assert!(table.get("tab-bar").is_some());
    assert!(table.get("tab-line").is_some());
    assert!(table.get("font-lock-keyword-face").is_some());
    assert!(table.len() > 30);
}

#[test]
fn face_table_pdump_uses_symbol_identity() {
    crate::test_utils::init_test_tracing();
    let eval = crate::emacs_core::Context::new();
    let dump = crate::emacs_core::pdump::convert::dump_evaluator(&eval);
    assert!(dump.face_table.faces.is_empty());
    assert!(!dump.face_table.face_ids.is_empty());
}

#[test]
fn face_table_pdump_preserves_lisp_owned_attrs() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let mut face = Face::new("pdump-face");
    face.family = Some(Value::symbol("unspecified"));
    face.foundry = Some(Value::string("OpenAI"));
    face.stipple = Some(Value::symbol("unspecified"));
    face.doc = Some(Value::string("Face doc"));
    eval.face_table.define("pdump-face", face);

    let dump = crate::emacs_core::pdump::convert::dump_evaluator(&eval);
    assert!(dump.face_table.faces.is_empty());

    let mut decoder = crate::emacs_core::pdump::convert::LoadDecoder::new(&dump.tagged_heap);
    crate::emacs_core::pdump::convert::load_symbol_table(&dump.symbol_table).expect("remap");
    let restored =
        crate::emacs_core::pdump::convert::load_face_table(&mut decoder, &dump.face_table);
    crate::emacs_core::pdump::convert::finish_load_interner();
    let restored_face = restored.get("pdump-face").expect("restored face");
    assert!(
        restored_face
            .family
            .as_ref()
            .is_some_and(|value| value.is_symbol_named("unspecified"))
    );
    assert_eq!(
        restored_face
            .foundry
            .as_ref()
            .and_then(|value| value.as_runtime_string_owned())
            .as_deref(),
        Some("OpenAI")
    );
    assert!(
        restored_face
            .stipple
            .as_ref()
            .is_some_and(|value| value.is_symbol_named("unspecified"))
    );
    assert_eq!(
        restored_face
            .doc
            .as_ref()
            .and_then(|value| value.as_runtime_string_owned())
            .as_deref(),
        Some("Face doc")
    );
}

#[test]
fn face_table_pdump_keeps_inherit_as_symbols() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let mut face = Face::new("pdump-inherit-face");
    face.inherit = Some(Value::list(vec![
        Value::symbol("font-lock-keyword-face"),
        Value::symbol("warning"),
    ]));
    eval.face_table.define("pdump-inherit-face", face);

    let dump = crate::emacs_core::pdump::convert::dump_evaluator(&eval);
    let dumped = dump
        .face_table
        .face_ids
        .iter()
        .find(|(sym_id, _)| sym_id.0 == crate::emacs_core::intern::intern("pdump-inherit-face").0)
        .map(|(_, face)| face)
        .expect("dumped pdump-inherit-face");

    assert_eq!(dumped.inherit.len(), 0);
    assert_eq!(dumped.inherit_syms.len(), 2);
}

#[test]
fn default_face_does_not_seed_font_family_or_height() {
    crate::test_utils::init_test_tracing();
    let table = FaceTable::new();
    let default = table.get("default").expect("default face");
    assert!(default.family.is_none());
    assert!(default.height.is_none());
}

#[test]
fn default_face_does_not_seed_tty_default_colors() {
    crate::test_utils::init_test_tracing();
    let table = FaceTable::new();
    let default = table.get("default").expect("default face");
    assert!(default.foreground.is_none());
    assert!(default.background.is_none());
}

#[test]
fn face_table_resolve_inheritance() {
    crate::test_utils::init_test_tracing();
    let table = FaceTable::new();
    let bold = table.resolve("bold");
    assert_eq!(bold.weight, Some(FontWeight::BOLD));
    // GNU TTY default colors remain sentinel values when inherited.
    assert!(bold.foreground.is_none());
    assert!(bold.background.is_none());
}

#[test]
fn face_table_merge_faces() {
    crate::test_utils::init_test_tracing();
    let table = FaceTable::new();
    let merged = table.merge_faces(&["bold", "italic"]);
    assert_eq!(merged.weight, Some(FontWeight::BOLD));
    assert_eq!(merged.slant, Some(FontSlant::Italic));
}

#[test]
fn face_from_plist() {
    crate::test_utils::init_test_tracing();
    let plist = vec![
        Value::keyword("foreground"),
        Value::string("#ff0000"),
        Value::keyword("weight"),
        Value::symbol("bold"),
        Value::keyword("height"),
        Value::make_float(1.5),
    ];
    let face = Face::from_plist("test", &plist);
    assert_eq!(face.foreground, Some(Color::rgb(255, 0, 0)));
    assert_eq!(face.weight, Some(FontWeight::BOLD));
    assert_eq!(face.height, Some(FaceHeight::Relative(1.5)));
}

#[test]
fn face_from_plist_stores_stipple_spec() {
    crate::test_utils::init_test_tracing();
    // `indent-bars` supplies an inline `(WIDTH HEIGHT DATA)` bitmap spec; it
    // must be preserved on the face so the layout bridge can realize it.
    let spec = Value::list(vec![
        Value::fixnum(8),
        Value::fixnum(2),
        Value::string("AB"),
    ]);
    let face = Face::from_plist("test", &[Value::keyword("stipple"), spec]);
    let stored = face.stipple.expect("stipple spec preserved on the face");
    let items = crate::emacs_core::value::list_to_vec(&stored).expect("stipple is a list");
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].as_fixnum(), Some(8));
    assert_eq!(items[1].as_fixnum(), Some(2));

    // Explicit nil resets it, matching GNU's `unspecified`/nil handling.
    let nil_face = Face::from_plist("test", &[Value::keyword("stipple"), Value::NIL]);
    assert!(nil_face.stipple.is_none());
}

#[test]
fn face_from_plist_accepts_source_style_keywords() {
    crate::test_utils::init_test_tracing();
    let plist = vec![
        Value::symbol(":family"),
        Value::string("JetBrains Mono"),
        Value::symbol(":foreground"),
        Value::string("gold"),
        Value::symbol(":underline"),
        Value::list(vec![
            Value::symbol(":style"),
            Value::symbol("wave"),
            Value::symbol(":color"),
            Value::string("cyan"),
        ]),
        Value::symbol(":box"),
        Value::list(vec![
            Value::symbol(":line-width"),
            Value::fixnum(2),
            Value::symbol(":color"),
            Value::string("#336699"),
            Value::symbol(":style"),
            Value::symbol("pressed-button"),
        ]),
        Value::symbol(":width"),
        Value::symbol("expanded"),
    ];

    let face = Face::from_plist("test", &plist);
    assert_eq!(
        face.family_runtime_string_owned().as_deref(),
        Some("JetBrains Mono")
    );
    assert_eq!(face.foreground, Some(Color::rgb(255, 215, 0)));
    assert_eq!(face.width, Some(FontWidth::Expanded));
    assert_eq!(
        face.underline.enabled().map(|underline| &underline.style),
        Some(&UnderlineStyle::Wave)
    );
    assert_eq!(
        face.underline
            .enabled()
            .and_then(|underline| underline.color),
        Some(Color::rgb(0, 255, 255))
    );
    assert_eq!(
        face.box_border.enabled().map(|border| border.width),
        Some(2)
    );
    assert_eq!(
        face.box_border.enabled().and_then(|border| border.color),
        Some(Color::rgb(51, 102, 153))
    );
    assert_eq!(
        face.box_border.enabled().map(|border| border.style),
        Some(BoxStyle::Pressed)
    );
}

#[test]
fn font_weight_from_symbol() {
    crate::test_utils::init_test_tracing();
    assert_eq!(FontWeight::from_symbol("bold"), Some(FontWeight::BOLD));
    assert_eq!(FontWeight::from_symbol("normal"), Some(FontWeight::NORMAL));
    assert!(FontWeight::BOLD.is_bold());
    assert!(!FontWeight::NORMAL.is_bold());
}

#[test]
fn face_table_custom_face() {
    crate::test_utils::init_test_tracing();
    let mut table = FaceTable::new();
    let mut custom = Face::new("my-face");
    custom.foreground = Some(Color::rgb(100, 200, 50));
    custom.inherit = Some(face_symbol_value("bold"));
    table.define("my-face", custom);

    let resolved = table.resolve("my-face");
    assert_eq!(resolved.foreground, Some(Color::rgb(100, 200, 50)));
    assert_eq!(resolved.weight, Some(FontWeight::BOLD)); // inherited
}

// --- Color::parse (unified hex + named) ---

#[test]
fn color_parse_hex_and_named() {
    crate::test_utils::init_test_tracing();
    // Hex path
    assert_eq!(Color::parse("#ff0000"), Some(Color::rgb(255, 0, 0)));
    assert_eq!(Color::parse("#abc"), Some(Color::rgb(170, 187, 204)));
    // Named color path
    assert_eq!(Color::parse("blue"), Some(Color::rgb(0, 0, 255)));
    assert_eq!(Color::parse("gold"), Some(Color::rgb(255, 215, 0)));
    // Unknown
    assert_eq!(Color::parse("nonexistent"), None);
    assert_eq!(Color::parse("#xyz"), None);
}

// --- SpecifiedColor (lface-vector spec layer, GNU xfaces staging) ---

#[test]
fn specified_color_parse_special_tokens() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        SpecifiedColor::parse("unspecified"),
        SpecifiedColor::Unspecified
    );
    assert_eq!(
        SpecifiedColor::parse("unspecified-fg"),
        SpecifiedColor::FrameForeground
    );
    assert_eq!(
        SpecifiedColor::parse("unspecified-bg"),
        SpecifiedColor::FrameBackground
    );
}

#[test]
fn specified_color_parse_keeps_strings_as_specs() {
    crate::test_utils::init_test_tracing();
    // Names stay names: "white" means X11 white on GUI but the terminal
    // palette's white on a TTY — realization decides, not parsing.
    assert_eq!(
        SpecifiedColor::parse("white"),
        SpecifiedColor::Named("white".to_owned())
    );
    // Hex strings ALSO stay specs. GNU routes every lface color string —
    // hex included — through the frame-class realization step
    // (map_tty_color -> tty-color-desc approximates "#ff0000" through the
    // terminal palette when the tty lacks 24-bit color), so hex is not
    // frame-independent and must not collapse to RGB at parse time.
    assert_eq!(
        SpecifiedColor::parse("#ff0000"),
        SpecifiedColor::Named("#ff0000".to_owned())
    );
    // Unknown junk is still a spec; realization is where it fails.
    assert_eq!(
        SpecifiedColor::parse("no-such-color"),
        SpecifiedColor::Named("no-such-color".to_owned())
    );
}

#[test]
fn specified_color_spec_string_round_trip() {
    crate::test_utils::init_test_tracing();
    for spec in ["unspecified", "unspecified-fg", "unspecified-bg", "gold"] {
        assert_eq!(
            SpecifiedColor::parse(spec).spec_string().as_deref(),
            Some(spec),
            "{spec}"
        );
    }
    assert_eq!(SpecifiedColor::Rgb(1, 2, 3).spec_string(), None);
}

#[test]
fn realized_color_is_the_render_layer_color() {
    crate::test_utils::init_test_tracing();
    // The render-layer type carries alpha, exactly as Color does today.
    let c: RealizedColor = Color::rgb(1, 2, 3);
    assert_eq!((c.r, c.g, c.b, c.a), (1, 2, 3, 255));
}

#[test]
fn color_from_name_case_insensitive() {
    crate::test_utils::init_test_tracing();
    assert_eq!(Color::from_name("Black"), Some(Color::rgb(0, 0, 0)));
    assert_eq!(Color::from_name("CYAN"), Some(Color::rgb(0, 255, 255)));
    assert_eq!(Color::from_name("Gray"), Some(Color::rgb(190, 190, 190)));
    assert_eq!(Color::from_name("grey"), Some(Color::rgb(190, 190, 190)));
}

#[test]
fn color_from_name_full_palette() {
    crate::test_utils::init_test_tracing();
    // Spot-check a wide range of named colors
    let names_and_expected = [
        ("orange", Color::rgb(255, 165, 0)),
        ("pink", Color::rgb(255, 192, 203)),
        ("navy", Color::rgb(0, 0, 128)),
        ("coral", Color::rgb(255, 127, 80)),
        ("ivory", Color::rgb(255, 255, 240)),
        ("wheat", Color::rgb(245, 222, 179)),
        ("lavender", Color::rgb(230, 230, 250)),
        ("snow", Color::rgb(255, 250, 250)),
    ];
    for (name, expected) in names_and_expected {
        assert_eq!(
            Color::from_name(name),
            Some(expected),
            "failed for color: {name}"
        );
    }
}

// --- Font weight/slant from_symbol ---

#[test]
fn font_weight_from_symbol_all_names() {
    crate::test_utils::init_test_tracing();
    let cases = [
        ("thin", FontWeight::Thin, 0, 100),
        ("ultra-light", FontWeight::UltraLight, 40, 200),
        ("ultralight", FontWeight::Ultralight, 40, 200),
        ("extra-light", FontWeight::ExtraLight, 40, 200),
        ("extralight", FontWeight::Extralight, 40, 200),
        ("light", FontWeight::Light, 50, 300),
        ("semi-light", FontWeight::SemiLight, 55, 350),
        ("semilight", FontWeight::Semilight, 55, 350),
        ("demilight", FontWeight::Demilight, 55, 350),
        ("regular", FontWeight::Regular, 80, 400),
        ("normal", FontWeight::Normal, 80, 400),
        ("unspecified", FontWeight::Unspecified, 80, 400),
        ("book", FontWeight::Book, 80, 400),
        ("medium", FontWeight::Medium, 100, 500),
        ("semi-bold", FontWeight::SemiBold, 180, 600),
        ("semibold", FontWeight::Semibold, 180, 600),
        ("demibold", FontWeight::Demibold, 180, 600),
        ("demi-bold", FontWeight::DemiBold, 180, 600),
        ("demi", FontWeight::Demi, 180, 600),
        ("bold", FontWeight::Bold, 200, 700),
        ("extra-bold", FontWeight::ExtraBold, 205, 800),
        ("extrabold", FontWeight::Extrabold, 205, 800),
        ("ultra-bold", FontWeight::UltraBold, 205, 800),
        ("ultrabold", FontWeight::Ultrabold, 205, 800),
        ("black", FontWeight::Black, 210, 900),
        ("heavy", FontWeight::Heavy, 210, 900),
        ("ultra-heavy", FontWeight::UltraHeavy, 250, 950),
        ("ultraheavy", FontWeight::Ultraheavy, 250, 950),
    ];
    for (name, weight, gnu_numeric, css_weight) in cases {
        assert_eq!(FontWeight::from_symbol(name), Some(weight), "parse {name}");
        assert_eq!(weight.symbol_name(), name);
        assert_eq!(weight.gnu_numeric(), gnu_numeric);
        assert_eq!(weight.css_weight(), css_weight);
        assert_eq!(FontWeight::from_dump_code(weight.dump_code()), weight);
    }
    assert_eq!(FontWeight::from_symbol("BOLD"), Some(FontWeight::Bold));
    assert_eq!(
        FontWeight::from_symbol("EXTRA-BOLD"),
        Some(FontWeight::ExtraBold)
    );
    assert_eq!(FontWeight::from_symbol("unknown"), None);
}

#[test]
fn font_slant_from_symbol_all() {
    crate::test_utils::init_test_tracing();
    let cases = [
        ("normal", FontSlant::Normal, 100),
        ("r", FontSlant::Normal, 100),
        ("unspecified", FontSlant::Normal, 100),
        ("italic", FontSlant::Italic, 200),
        ("i", FontSlant::Italic, 200),
        ("ot", FontSlant::Italic, 200),
        ("oblique", FontSlant::Oblique, 210),
        ("o", FontSlant::Oblique, 210),
        ("reverse-italic", FontSlant::ReverseItalic, 10),
        ("ri", FontSlant::ReverseItalic, 10),
        ("reverse-oblique", FontSlant::ReverseOblique, 0),
        ("ro", FontSlant::ReverseOblique, 0),
    ];
    for (name, slant, gnu_numeric) in cases {
        assert_eq!(FontSlant::from_symbol(name), Some(slant));
        assert_eq!(slant.gnu_numeric(), gnu_numeric);
        assert_eq!(FontSlant::from_gnu_numeric(gnu_numeric), Some(slant));
    }
    assert_eq!(
        FontSlant::from_gnu_numeric(205),
        None,
        "GNU slant table has no 205 entry"
    );
    assert_eq!(FontSlant::from_symbol("unknown"), None);
    assert_eq!(FontSlant::from_symbol("roman"), None);
    assert_eq!(FontSlant::from_symbol("ITALIC"), Some(FontSlant::Italic));
    assert!(FontSlant::Italic.is_italic());
    assert!(FontSlant::Oblique.is_italic());
    assert!(!FontSlant::Normal.is_italic());
    assert_eq!(FontSlant::Normal.symbol_name(), "normal");
    assert_eq!(FontSlant::ReverseOblique.symbol_name(), "reverse-oblique");
}

#[test]
fn font_width_from_symbol_all() {
    crate::test_utils::init_test_tracing();
    let cases = [
        ("ultra-condensed", FontWidth::UltraCondensed, 50),
        ("ultracondensed", FontWidth::UltraCondensed, 50),
        ("extra-condensed", FontWidth::ExtraCondensed, 63),
        ("extracondensed", FontWidth::ExtraCondensed, 63),
        ("condensed", FontWidth::Condensed, 75),
        ("compressed", FontWidth::Condensed, 75),
        ("narrow", FontWidth::Condensed, 75),
        ("semi-condensed", FontWidth::SemiCondensed, 87),
        ("semicondensed", FontWidth::SemiCondensed, 87),
        ("demicondensed", FontWidth::SemiCondensed, 87),
        ("normal", FontWidth::Normal, 100),
        ("medium", FontWidth::Normal, 100),
        ("regular", FontWidth::Normal, 100),
        ("unspecified", FontWidth::Normal, 100),
        ("semi-expanded", FontWidth::SemiExpanded, 113),
        ("semiexpanded", FontWidth::SemiExpanded, 113),
        ("demiexpanded", FontWidth::SemiExpanded, 113),
        ("expanded", FontWidth::Expanded, 125),
        ("extra-expanded", FontWidth::ExtraExpanded, 150),
        ("extraexpanded", FontWidth::ExtraExpanded, 150),
        ("ultra-expanded", FontWidth::UltraExpanded, 200),
        ("ultraexpanded", FontWidth::UltraExpanded, 200),
        ("wide", FontWidth::UltraExpanded, 200),
    ];
    for (name, width, gnu_numeric) in cases {
        assert_eq!(FontWidth::from_symbol(name), Some(width), "parse {name}");
        assert_eq!(width.gnu_numeric(), gnu_numeric);
        assert_eq!(FontWidth::from_gnu_numeric(gnu_numeric), Some(width));
    }
    assert_eq!(
        FontWidth::from_symbol("EXTRA-EXPANDED"),
        Some(FontWidth::ExtraExpanded)
    );
    assert_eq!(
        FontWidth::from_gnu_numeric(101),
        None,
        "GNU width table has no 101 entry"
    );
    assert_eq!(FontWidth::from_symbol("unknown"), None);
    assert_eq!(FontWidth::Condensed.symbol_name(), "condensed");
    assert_eq!(FontWidth::Normal.symbol_name(), "normal");
    assert_eq!(FontWidth::ExtraExpanded.symbol_name(), "extra-expanded");
}

// --- Face::to_plist round-trip ---

#[test]
fn face_to_plist_contains_set_attrs() {
    crate::test_utils::init_test_tracing();
    let mut face = Face::new("test");
    face.foreground = Some(Color::rgb(255, 0, 0));
    face.weight = Some(FontWeight::BOLD);
    face.slant = Some(FontSlant::Italic);
    face.height = Some(FaceHeight::Absolute(120));

    let plist = face.to_plist();
    let items = crate::emacs_core::value::list_to_vec(&plist).unwrap();
    // Should have keyword-value pairs
    assert!(items.len() >= 8); // 4 attrs * 2
}

// --- Merge with underline/box/overline/strike-through ---

#[test]
fn face_merge_underline_and_box() {
    crate::test_utils::init_test_tracing();
    let base = Face {
        underline: FaceDecoration::Enabled(Underline {
            style: UnderlineStyle::Line,
            color: None,
            position: UnderlinePosition::FontMetric,
        }),
        ..Default::default()
    };
    let overlay = Face {
        box_border: FaceDecoration::Enabled(BoxBorder {
            color: Some(Color::rgb(255, 0, 0)),
            width: 2,
            style: BoxStyle::Flat,
        }),
        overline: Some(true),
        strike_through: Some(true),
        ..Default::default()
    };
    let merged = base.merge(&overlay);
    // base's underline preserved
    assert!(merged.underline.enabled().is_some());
    // overlay's box, overline, strike-through applied
    assert_eq!(merged.box_border.enabled().unwrap().width, 2);
    assert_eq!(merged.overline, Some(true));
    assert_eq!(merged.strike_through, Some(true));
}

#[test]
fn face_merge_preserves_explicit_box_disable() {
    let base = Face {
        box_border: FaceDecoration::Enabled(BoxBorder {
            color: None,
            width: 1,
            style: BoxStyle::Flat,
        }),
        ..Default::default()
    };
    let overlay = Face {
        box_border: FaceDecoration::Disabled,
        ..Default::default()
    };

    assert!(matches!(
        base.merge(&overlay).box_border,
        FaceDecoration::Disabled
    ));
}

#[test]
fn face_merge_relative_height_over_absolute_becomes_absolute() {
    crate::test_utils::init_test_tracing();
    let mut base = Face::new("base");
    base.height = Some(FaceHeight::Absolute(120));

    let mut overlay = Face::new("overlay");
    overlay.height = Some(FaceHeight::Relative(1.5));

    let merged = base.merge(&overlay);
    assert_eq!(merged.height, Some(FaceHeight::Absolute(180)));
}

#[test]
fn face_merge_relative_height_over_relative_multiplies() {
    crate::test_utils::init_test_tracing();
    let mut base = Face::new("base");
    base.height = Some(FaceHeight::Relative(1.2));

    let mut overlay = Face::new("overlay");
    overlay.height = Some(FaceHeight::Relative(1.5));

    let merged = base.merge(&overlay);
    match merged.height {
        Some(FaceHeight::Relative(value)) => assert!((value - 1.8).abs() < 1e-9),
        other => panic!("expected relative height, got {other:?}"),
    }
}

// --- Multi-level inheritance ---

#[test]
fn face_table_multi_level_inheritance() {
    crate::test_utils::init_test_tracing();
    let mut table = FaceTable::new();

    // grandparent: sets foreground
    let mut gp = Face::new("grandparent");
    gp.foreground = Some(Color::rgb(100, 100, 100));
    gp.slant = Some(FontSlant::Italic);
    table.define("grandparent", gp);

    // parent: inherits grandparent, sets weight
    let mut parent = Face::new("parent");
    parent.weight = Some(FontWeight::BOLD);
    parent.inherit = Some(face_symbol_value("grandparent"));
    table.define("parent", parent);

    // child: inherits parent, sets background
    let mut child = Face::new("child");
    child.background = Some(Color::rgb(200, 200, 200));
    child.inherit = Some(face_symbol_value("parent"));
    table.define("child", child);

    let resolved = table.resolve("child");
    assert_eq!(resolved.background, Some(Color::rgb(200, 200, 200))); // own
    assert_eq!(resolved.weight, Some(FontWeight::BOLD)); // from parent
    assert_eq!(resolved.foreground, Some(Color::rgb(100, 100, 100))); // from grandparent
    assert_eq!(resolved.slant, Some(FontSlant::Italic)); // from grandparent
}

// --- from_plist with underline/overline/extend/inherit ---

#[test]
fn face_from_plist_underline_and_flags() {
    crate::test_utils::init_test_tracing();
    let plist = vec![
        Value::keyword("underline"),
        Value::T,
        Value::keyword("overline"),
        Value::T,
        Value::keyword("strike-through"),
        Value::T,
        Value::keyword("inverse-video"),
        Value::T,
        Value::keyword("extend"),
        Value::T,
        Value::keyword("inherit"),
        Value::symbol("bold"),
    ];
    let face = Face::from_plist("test", &plist);
    assert!(face.underline.enabled().is_some());
    assert_eq!(
        face.underline.enabled().unwrap().style,
        UnderlineStyle::Line
    );
    assert_eq!(face.overline, Some(true));
    assert_eq!(face.strike_through, Some(true));
    assert_eq!(face.inverse_video, Some(true));
    assert_eq!(face.extend, Some(true));
    assert_eq!(face.inherit, Some(face_symbol_value("bold")));
}

#[test]
fn face_from_plist_accepts_raw_unibyte_underline_and_box_strings() {
    crate::test_utils::init_test_tracing();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let plist = vec![Value::keyword("underline"), raw, Value::keyword("box"), raw];
    let face = Face::from_plist("test", &plist);
    assert!(face.underline.enabled().is_some());
    assert_eq!(
        face.underline.enabled().unwrap().style,
        UnderlineStyle::Line
    );
    assert_eq!(face.underline.enabled().unwrap().color, None);
    assert!(face.box_border.enabled().is_some());
    assert_eq!(face.box_border.enabled().unwrap().width, 1);
    assert_eq!(face.box_border.enabled().unwrap().color, None);
}

// --- Resolve unknown face returns empty ---

#[test]
fn face_table_resolve_unknown_face() {
    crate::test_utils::init_test_tracing();
    let table = FaceTable::new();
    let resolved = table.resolve("nonexistent");
    assert!(resolved.foreground.is_none());
}

// --- face_list and len ---

#[test]
fn face_table_face_list() {
    crate::test_utils::init_test_tracing();
    let table = FaceTable::new();
    let list = table.face_list();
    assert!(list.contains(&"default".to_string()));
    assert!(list.contains(&"bold".to_string()));
    assert_eq!(list.len(), table.len());
    assert!(!table.is_empty());
}

#[test]
fn face_table_gc_traces_lisp_owned_face_text_fields() {
    crate::test_utils::init_test_tracing();
    let mut table = FaceTable::new();
    let mut face = Face::new("gc-face");
    face.family = Some(Value::string("Iosevka"));
    face.foundry = Some(Value::string("OpenAI"));
    face.stipple = Some(Value::string("gray3"));
    face.doc = Some(Value::string("Face doc"));
    face.inherit = Some(Value::symbol("default"));
    table.define("gc-face", face);

    let mut roots = Vec::new();
    table.trace_roots(&mut roots);

    assert!(roots.contains(&Value::symbol("gc-face")));
    assert!(roots.contains(&Value::string("Iosevka")));
    assert!(roots.contains(&Value::string("OpenAI")));
    assert!(roots.contains(&Value::string("gray3")));
    assert!(roots.contains(&Value::string("Face doc")));
    assert!(roots.contains(&Value::symbol("default")));
}

#[test]
fn face_remapping_from_lisp_interns_string_names_to_symbols() {
    crate::test_utils::init_test_tracing();
    let remapping = FaceRemapping::from_lisp(&Value::list(vec![Value::cons(
        Value::string("mode-line"),
        Value::string("bold"),
    )]));

    let entries = remapping.get("mode-line").expect("remapping");
    assert_eq!(entries.len(), 1);
    match &entries[0] {
        FaceRemapEntry::RemapFace(value) => assert_eq!(*value, face_symbol_value("bold")),
        other => panic!("expected face remap, got {other:?}"),
    }
}

/// The realized underline colour of an ANONYMOUS attribute plist.
///
/// GNU has one realization path: `merge_face_ref` folds an inline plist into
/// the same lface vector, and `realize_tty_face` then maps `:underline`'s
/// colour through `map_tty_color` (src/xfaces.c:6748) exactly as it maps
/// `:foreground` and `:background` (:6800-6803).  The writer emits the result
/// through `TF_set_underline_color` (src/term.c:2119-2126).
///
/// This path used to call `Color::parse` for the underline while calling
/// `realize_color_spec` for the foreground, so an inline
/// `(:underline (:color "red"))` reached the terminal with a pixel and no
/// index -- a colour the writer is right to refuse to emit, and therefore an
/// underline that silently lost its colour on every terminal frame.
#[test]
fn an_anonymous_plist_realizes_its_underline_color_through_the_palette() {
    use neomacs_display_protocol::terminal_color::TerminalColor;
    use neomacs_display_protocol::{TtyPalette, TtyPaletteEntry};

    let palette = TtyPalette::new(
        vec![
            TtyPaletteEntry {
                name: "black".to_string(),
                index: 0,
                rgb: Some((0, 0, 0)),
            },
            TtyPaletteEntry {
                name: "red".to_string(),
                index: 1,
                rgb: Some((205, 0, 0)),
            },
        ],
        256,
    );

    // The string form, GNU's `(:underline "red")`.
    let plist = vec![Value::keyword("underline"), Value::string("red")];
    let face = Face::from_plist_realized("t", &plist, Some(&palette));
    let underline = face.underline.enabled().expect("underline enabled");
    assert_eq!(
        underline.color.and_then(|color| color.terminal),
        Some(TerminalColor::Indexed(1)),
        "a string :underline realizes through the palette"
    );

    // The plist form, GNU's `(:underline (:color "red" :style wave))`.
    let inner = Value::list(vec![
        Value::keyword("color"),
        Value::string("red"),
        Value::keyword("style"),
        Value::symbol("wave"),
    ]);
    let plist = vec![Value::keyword("underline"), inner];
    let face = Face::from_plist_realized("t", &plist, Some(&palette));
    let underline = face.underline.enabled().expect("underline enabled");
    assert_eq!(underline.style, UnderlineStyle::Wave);
    assert_eq!(
        underline.color.and_then(|color| color.terminal),
        Some(TerminalColor::Indexed(1)),
        "a plist :color realizes through the palette"
    );

    // No palette is a GUI frame: the realized colour IS the pixel and carries
    // no terminal index, which is what keeps a GUI underline out of the
    // terminal writer's slot.
    let face = Face::from_plist_realized(
        "t",
        &[Value::keyword("underline"), Value::string("red")],
        None,
    );
    let underline = face.underline.enabled().expect("underline enabled");
    assert_eq!(underline.color.and_then(|color| color.terminal), None);
}
