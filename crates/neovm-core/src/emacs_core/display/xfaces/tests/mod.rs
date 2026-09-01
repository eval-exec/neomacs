use super::*;

// --- FaceColorResolver::realize (spec -> realized color bridge) ---

#[test]
fn realize_standard_named_and_hex_specs() {
    crate::test_utils::init_test_tracing();
    use crate::face::{Color, SpecifiedColor};
    let r = FaceColorResolver::Standard;
    assert_eq!(
        r.realize(&SpecifiedColor::parse("red")),
        Some(Color::rgb(255, 0, 0))
    );
    assert_eq!(
        r.realize(&SpecifiedColor::parse("#abc")),
        Some(Color::rgb(170, 187, 204))
    );
    assert_eq!(r.realize(&SpecifiedColor::parse("no-such-color")), None);
    assert_eq!(
        r.realize(&SpecifiedColor::Rgb(9, 8, 7)),
        Some(Color::rgb(9, 8, 7))
    );
}

#[test]
fn realize_unspecified_and_frame_default_specs_stay_unrealized() {
    crate::test_utils::init_test_tracing();
    use crate::face::SpecifiedColor;
    // GNU realize_tty_face maps unspecified-fg/-bg to the frame defaults at
    // realization; in neomacs the frame default substitution happens earlier
    // (realize_default_lisp_face_for_frame rewrites the default face vector),
    // so at this boundary the specs realize to None and downstream frame
    // defaults apply — identical to the pre-split string behavior where
    // "unspecified-fg" failed the name lookup.
    for spec in [
        SpecifiedColor::Unspecified,
        SpecifiedColor::FrameForeground,
        SpecifiedColor::FrameBackground,
    ] {
        assert_eq!(FaceColorResolver::Standard.realize(&spec), None);
        assert_eq!(
            FaceColorResolver::TtyPalette(&TtyColorMap::default()).realize(&spec),
            None
        );
    }
}

#[test]
fn realize_tty_palette_wins_over_standard_parse() {
    crate::test_utils::init_test_tracing();
    use crate::face::{Color, SpecifiedColor};
    let mut palette = TtyColorMap::default();
    // xterm registers "white" as 229,229,229 — the palette must beat rgb.txt.
    palette.insert("white".to_owned(), Color::rgb(229, 229, 229));
    // A tty without 24-bit color approximates hex through the palette too
    // (GNU tty-color-desc -> tty-color-approximate), keyed by the exact
    // lface string.
    palette.insert("#ff0000".to_owned(), Color::rgb(205, 0, 0));
    let r = FaceColorResolver::TtyPalette(&palette);
    assert_eq!(
        r.realize(&SpecifiedColor::parse("white")),
        Some(Color::rgb(229, 229, 229))
    );
    assert_eq!(
        r.realize(&SpecifiedColor::parse("#ff0000")),
        Some(Color::rgb(205, 0, 0))
    );
    // Not in the palette: fall back to the standard parse, GNU's
    // failed-tty_lookup_color fallback.
    assert_eq!(
        r.realize(&SpecifiedColor::parse("gold")),
        Some(Color::rgb(255, 215, 0))
    );
    assert_eq!(r.realize(&SpecifiedColor::parse("no-such-color")), None);
}

#[test]
fn lisp_value_to_face_attr_realizes_at_the_boundary() {
    crate::test_utils::init_test_tracing();
    use crate::face::{Color, FaceAttrValue};
    let mut palette = TtyColorMap::default();
    palette.insert("white".to_owned(), Color::rgb(229, 229, 229));
    let tty = FaceColorResolver::TtyPalette(&palette);

    // fg/bg/distant-fg and underline colors realize through the stated
    // frame-class policy (GNU map_tty_color covers exactly fg, bg, and
    // underline).
    let expect_color = |got: Option<FaceAttrValue>, want: Color| match got {
        Some(FaceAttrValue::Color(c)) => assert_eq!(c, want),
        other => panic!("expected color, got {other:?}"),
    };
    for attr in [
        LFaceAttr::Foreground,
        LFaceAttr::Background,
        LFaceAttr::DistantForeground,
    ] {
        expect_color(
            lisp_value_to_face_attr_resolved(attr, Value::string("white"), tty),
            Color::rgb(229, 229, 229),
        );
    }
    match lisp_value_to_face_attr_resolved(LFaceAttr::Underline, Value::string("white"), tty) {
        Some(FaceAttrValue::Underline(u)) => {
            assert_eq!(u.color, Some(Color::rgb(229, 229, 229)))
        }
        other => panic!("expected underline, got {other:?}"),
    }
    // Overline/strike-through/box color shorthands are NOT tty-mapped in
    // GNU; they keep the context-free standard parse.
    for attr in [LFaceAttr::Overline, LFaceAttr::StrikeThrough] {
        expect_color(
            lisp_value_to_face_attr_resolved(attr, Value::string("white"), tty),
            Color::rgb(255, 255, 255),
        );
    }
    match lisp_value_to_face_attr_resolved(LFaceAttr::Box, Value::string("white"), tty) {
        Some(FaceAttrValue::Box(b)) => assert_eq!(b.color, Some(Color::rgb(255, 255, 255))),
        other => panic!("expected box, got {other:?}"),
    }
    // The unspecified-fg/-bg frame-default tokens stay unrealized at this
    // boundary (the attribute is skipped; frame defaults apply downstream).
    assert!(
        lisp_value_to_face_attr_resolved(
            LFaceAttr::Foreground,
            Value::string("unspecified-fg"),
            tty
        )
        .is_none()
    );
    assert!(
        lisp_value_to_face_attr_resolved(
            LFaceAttr::Background,
            Value::string("unspecified-bg"),
            FaceColorResolver::Standard
        )
        .is_none()
    );
}

#[test]
fn underline_position_t_preserves_gnu_descent_line_semantics() {
    crate::test_utils::init_test_tracing();
    use crate::face::{FaceAttrValue, UnderlinePosition};

    let underline = Value::list(vec![
        Value::keyword(":color"),
        Value::string("white"),
        Value::keyword(":position"),
        Value::T,
    ]);

    match lisp_value_to_face_attr_resolved(
        LFaceAttr::Underline,
        underline,
        FaceColorResolver::Standard,
    ) {
        Some(FaceAttrValue::Underline(underline)) => {
            assert_eq!(
                underline.position,
                UnderlinePosition::DescentLine { pixels_above: 0 },
                "GNU :position t places the underline on the descent line"
            );
        }
        other => panic!("expected underline, got {other:?}"),
    }
}

/// The palette entry a TTY frame realizes to carries the INDEX
/// `tty-color-desc` returned, and it survives realization -- that number is
/// GNU's whole realized colour on a terminal (`map_tty_color` stores it in
/// `face->foreground`, src/xfaces.c:6640-6648) and the only thing
/// `turn_on_face` writes (src/term.c:2093-2117).
///
/// The two rows are read out of GNU 31.0.90 on a real pty with COLORTERM unset
/// and TERM=xterm-256color:
///
/// ```text
/// (tty-color-desc "white")   => ("white" 7 58853 58853 58853)
/// (tty-color-desc "#afafaf") => ("color-145" 145 44975 44975 44975)
/// ```
#[test]
fn realized_tty_colour_keeps_the_index_tty_color_desc_returned() {
    crate::test_utils::init_test_tracing();
    use crate::face::{Color, SpecifiedColor};
    use neomacs_display_protocol::TerminalColor;
    let mut palette = TtyColorMap::default();
    palette.insert(
        "white".to_owned(),
        Color::rgb(229, 229, 229).with_terminal(TerminalColor::Indexed(7)),
    );
    palette.insert(
        "#afafaf".to_owned(),
        Color::rgb(175, 175, 175).with_terminal(TerminalColor::Indexed(145)),
    );
    let r = FaceColorResolver::TtyPalette(&palette);
    assert_eq!(
        r.realize(&SpecifiedColor::parse("white"))
            .and_then(|c| c.terminal),
        Some(TerminalColor::Indexed(7))
    );
    assert_eq!(
        r.realize(&SpecifiedColor::parse("#afafaf"))
            .and_then(|c| c.terminal),
        Some(TerminalColor::Indexed(145))
    );
    // A name the palette could not resolve keeps the context-free parse and
    // therefore has NO terminal colour: GNU leaves the pixel at
    // FACE_TTY_DEFAULT_COLOR there, and `face_tty_specified_color`
    // (src/dispextern.h:1933-1936) makes `turn_on_face` emit nothing rather
    // than a colour nobody computed.
    assert_eq!(
        r.realize(&SpecifiedColor::parse("gold"))
            .and_then(|c| c.terminal),
        None
    );
    // A GUI frame realizes the same spec with no terminal colour at all.
    assert_eq!(
        FaceColorResolver::Standard
            .realize(&SpecifiedColor::parse("white"))
            .and_then(|c| c.terminal),
        None
    );
}

/// Two colours that LOOK the same but write different bytes are different
/// realized colours, so a face cache keyed by colour cannot serve one for the
/// other.
#[test]
fn a_realized_colour_is_not_equal_to_the_same_rgb_without_its_index() {
    crate::test_utils::init_test_tracing();
    use crate::face::Color;
    use neomacs_display_protocol::TerminalColor;
    let plain = Color::rgb(255, 0, 0);
    assert_ne!(plain, plain.with_terminal(TerminalColor::Indexed(9)));
    assert_ne!(
        plain.with_terminal(TerminalColor::Indexed(1)),
        plain.with_terminal(TerminalColor::Indexed(9))
    );
}

#[test]
fn register_bootstrap_vars_matches_gnu_defaults() {
    crate::test_utils::init_test_tracing();
    let mut obarray = Obarray::new();
    register_bootstrap_vars(&mut obarray);

    assert_eq!(
        obarray.symbol_value("face-default-stipple").copied(),
        Some(Value::string("gray3"))
    );
    assert_eq!(
        obarray
            .symbol_value("face-near-same-color-threshold")
            .copied(),
        Some(Value::fixnum(30_000))
    );
    assert_eq!(
        obarray
            .symbol_value("face-font-lax-matched-attributes")
            .copied(),
        Some(Value::T)
    );

    let table = obarray
        .symbol_value("face--new-frame-defaults")
        .copied()
        .expect("face--new-frame-defaults");
    if !table.is_hash_table() {
        panic!("face--new-frame-defaults must be a hash table");
    };
    let test = table.as_hash_table().unwrap().test.clone();
    assert_eq!(test, HashTableTest::Eq);
}

#[test]
fn frame_face_hash_table_eval_has_initialized_default_face() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let out = builtin_frame_face_hash_table(&mut eval, vec![Value::NIL])
        .expect("live frame face hash table");
    if !out.is_hash_table() {
        panic!("expected hash table");
    };
    let default = lookup_frame_face_hash_entry(out, Value::symbol("default"))
        .expect("selected frame should have a default Lisp face vector");
    assert!(default.is_vector());
}

#[test]
fn frame_face_hash_table_eval_returns_stable_frame_owned_table() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let first =
        builtin_frame_face_hash_table(&mut eval, vec![Value::NIL]).expect("first face hash table");
    let second =
        builtin_frame_face_hash_table(&mut eval, vec![Value::NIL]).expect("second face hash table");
    assert_eq!(first, second);
}

#[test]
fn ensure_startup_compat_variables_backfills_missing_xfaces_state() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    for name in [
        "face-filters-always-match",
        "face--new-frame-defaults",
        "face-default-stipple",
        "scalable-fonts-allowed",
        "face-ignored-fonts",
        "face-remapping-alist",
        "face-font-rescale-alist",
        "face-near-same-color-threshold",
        "face-font-lax-matched-attributes",
    ] {
        eval.obarray_mut().makunbound(name);
    }

    ensure_startup_compat_variables(&mut eval);

    assert_eq!(
        eval.obarray().symbol_value("face-default-stipple").copied(),
        Some(Value::string("gray3"))
    );
    let table = eval
        .obarray()
        .symbol_value("face--new-frame-defaults")
        .copied()
        .expect("face hash table backfilled");
    if !table.is_hash_table() {
        panic!("face--new-frame-defaults must be a hash table");
    };
    let has_seeded_faces =
        {
            let hash_table = table.as_hash_table().unwrap();
            hash_table
                .data
                .contains_key(&HashKey::Symbol(crate::emacs_core::intern::intern(
                    "default",
                )))
                && hash_table.data.contains_key(&HashKey::Symbol(
                    crate::emacs_core::intern::intern("mode-line"),
                ))
        };
    assert!(
        has_seeded_faces,
        "face--new-frame-defaults should be preseeded with GNU face entries"
    );
}

#[test]
fn ensure_startup_compat_variables_reseeds_existing_face_defaults_table() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let table = Value::hash_table(HashTableTest::Eq);
    eval.set_variable("face--new-frame-defaults", table);

    ensure_startup_compat_variables(&mut eval);

    let table = eval
        .obarray()
        .symbol_value("face--new-frame-defaults")
        .copied()
        .expect("face hash table should stay bound");
    let hash_table = table
        .as_hash_table()
        .expect("face--new-frame-defaults must remain a hash table");
    assert!(
        hash_table
            .data
            .contains_key(&HashKey::Symbol(crate::emacs_core::intern::intern(
                "mode-line",
            ))),
        "existing face--new-frame-defaults tables must be reseeded after dump load"
    );
}
