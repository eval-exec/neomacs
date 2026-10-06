//! Focused invariant tests for GNU's indentation scan policy.
use super::super::*;

#[test]
fn gdm_tab_width_constructor_accepts_only_gnu_range() {
    for (input, expected) in [(-1, 8), (0, 8), (1, 1), (4, 4), (1000, 1000), (1001, 8)] {
        assert_eq!(TabWidth::from(Value::fixnum(input)).get(), expected);
    }
    assert_eq!(TabWidth::from(Value::NIL).get(), 8);
}

#[test]
fn gdm_column_glyphs_obey_tab_stops() {
    let glyphs = Value::vector(vec![
        Value::fixnum('中' as i64),
        Value::fixnum('\t' as i64),
        Value::fixnum('中' as i64),
    ]);
    assert_eq!(
        advance_column_glyphs(
            DisplayColumn::new(2),
            &glyphs,
            TabWidth::from(Value::fixnum(8)),
            LineEndPolicy::Newline
        ),
        Some(ColumnAdvance::Continue(DisplayColumn::new(9)))
    );
}

#[test]
fn gdm_column_glyphs_stop_at_selective_carriage_return() {
    let glyphs = Value::vector(vec![
        Value::fixnum('中' as i64),
        Value::fixnum('\r' as i64),
        Value::fixnum('中' as i64),
    ]);
    assert_eq!(
        advance_column_glyphs(
            DisplayColumn::ZERO,
            &glyphs,
            TabWidth::from(Value::fixnum(8)),
            LineEndPolicy::NewlineOrCarriageReturn
        ),
        Some(ColumnAdvance::EndOfLine(DisplayColumn::new(1)))
    );
    assert_eq!(
        advance_column_glyphs(
            DisplayColumn::ZERO,
            &glyphs,
            TabWidth::from(Value::fixnum(8)),
            LineEndPolicy::Newline
        ),
        Some(ColumnAdvance::Continue(DisplayColumn::new(3)))
    );
}
