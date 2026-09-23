use super::*;

#[test]
fn clean_top_layer_covers_bar_hbar_hollow_only() {
    // The retained-static cursor fast path depends on this mapping: only
    // the filled box (inverse video) forces the full render.
    assert!(CursorStyle::Bar(2.0).is_clean_top_layer());
    assert!(CursorStyle::Hbar(2.0).is_clean_top_layer());
    assert!(CursorStyle::Hollow.is_clean_top_layer());
    assert!(!CursorStyle::FilledBox.is_clean_top_layer());
}

#[test]
fn cursor_kind_matches_gnu_text_cursor_codes() {
    let cases = [
        (-2, CursorKind::Default),
        (-1, CursorKind::NoCursor),
        (0, CursorKind::FilledBox),
        (1, CursorKind::HollowBox),
        (2, CursorKind::Bar),
        (3, CursorKind::Hbar),
    ];

    for (code, kind) in cases {
        assert_eq!(CursorKind::from_gnu_code(code), Some(kind));
        assert_eq!(kind.gnu_code(), code);
    }

    assert_eq!(CursorKind::from_gnu_code(i8::MIN), None);
    assert_eq!(CursorKind::from_gnu_code(4), None);
}

#[test]
fn lisp_bar_width_accepts_gnu_nonnegative_int_range() {
    assert_eq!(
        CursorBarWidth::from_lisp_fixnum(0),
        Some(CursorBarWidth::new(0))
    );
    assert_eq!(
        CursorBarWidth::from_lisp_fixnum(i64::from(i32::MAX)),
        Some(CursorBarWidth::new(i32::MAX as u32))
    );
    assert_eq!(CursorBarWidth::from_lisp_fixnum(-1), None);
    assert_eq!(
        CursorBarWidth::from_lisp_fixnum(i64::from(i32::MAX) + 1),
        None
    );
}

#[test]
fn renderer_clamps_zero_width_without_changing_lisp_value() {
    let width = CursorBarWidth::new(0);

    assert_eq!(width.raw(), 0);
    assert_eq!(width.render_px(), 1.0);
}

#[test]
fn non_selected_bar_narrowing_preserves_zero_and_one() {
    assert_eq!(
        CursorBarWidth::new(0).narrowed_for_non_selected_bar(),
        CursorBarWidth::new(0)
    );
    assert_eq!(
        CursorBarWidth::new(1).narrowed_for_non_selected_bar(),
        CursorBarWidth::new(1)
    );
    assert_eq!(
        CursorBarWidth::new(3).narrowed_for_non_selected_bar(),
        CursorBarWidth::new(2)
    );
}
