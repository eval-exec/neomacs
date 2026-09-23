use super::*;

#[test]
fn scroll_bar_domains_match_gnu_symbols() {
    assert_eq!(
        VerticalScrollBarType::from_symbol_value(&Value::symbol("left")),
        Some(VerticalScrollBarType::Left)
    );
    assert_eq!(
        VerticalScrollBarType::from_symbol_value(&Value::symbol("right")),
        Some(VerticalScrollBarType::Right)
    );
    assert_eq!(VerticalScrollBarType::from_symbol_name("bottom"), None);
    assert_eq!(VerticalScrollBarType::Left.gnu_code(), 1);
    assert_eq!(VerticalScrollBarType::Right.gnu_code(), 2);
    assert_eq!(VerticalScrollBarType::from_gnu_code(0), None);
    assert_eq!(
        VerticalScrollBarType::from_gnu_code(1),
        Some(VerticalScrollBarType::Left)
    );
    assert_eq!(
        VerticalScrollBarType::from_gnu_code(2),
        Some(VerticalScrollBarType::Right)
    );
    assert_eq!(VerticalScrollBarType::from_gnu_code(3), None);
    assert!(is_valid_vertical_scroll_bar_value(Value::NIL));
    assert!(is_valid_vertical_scroll_bar_value(Value::T));
    assert!(!is_valid_vertical_scroll_bar_value(Value::symbol("bottom")));

    assert_eq!(
        HorizontalScrollBarType::from_symbol_value(&Value::symbol("bottom")),
        Some(HorizontalScrollBarType::Bottom)
    );
    assert_eq!(HorizontalScrollBarType::from_symbol_name("top"), None);
    assert!(is_valid_horizontal_scroll_bar_value(Value::NIL));
    assert!(is_valid_horizontal_scroll_bar_value(Value::T));
    assert!(!is_valid_horizontal_scroll_bar_value(Value::symbol("left")));
}
