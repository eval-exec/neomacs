use crate::emacs_core::value::Value;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use strum::{EnumString, IntoStaticStr};

/// GNU vertical scroll-bar type symbols accepted by window and frame code.
///
/// Mirrors concrete values from GNU `enum vertical_scroll_bar_type`:
/// `none = 0`, `left = 1`, `right = 2`.  `none` is represented by
/// `Option<VerticalScrollBarType>::None` in Neomacs because there is no Lisp
/// symbol for the disabled state.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr, IntoPrimitive, TryFromPrimitive,
)]
#[repr(u8)]
#[strum(serialize_all = "kebab-case")]
pub enum VerticalScrollBarType {
    Left = 1,
    Right = 2,
}

impl VerticalScrollBarType {
    pub fn from_gnu_code(code: u8) -> Option<Self> {
        Self::try_from(code).ok()
    }

    pub fn gnu_code(self) -> u8 {
        self.into()
    }

    pub fn from_symbol_name(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    pub fn from_symbol_value(value: &Value) -> Option<Self> {
        Self::from_symbol_name(value.as_symbol_name()?)
    }

    pub fn name(self) -> &'static str {
        self.into()
    }

    pub fn symbol(self) -> Value {
        Value::symbol(self.name())
    }
}

/// GNU horizontal scroll-bar type symbols accepted by window code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub enum HorizontalScrollBarType {
    Bottom,
}

impl HorizontalScrollBarType {
    pub fn from_symbol_name(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    pub fn from_symbol_value(value: &Value) -> Option<Self> {
        Self::from_symbol_name(value.as_symbol_name()?)
    }

    pub fn name(self) -> &'static str {
        self.into()
    }

    pub fn symbol(self) -> Value {
        Value::symbol(self.name())
    }
}

pub fn is_valid_vertical_scroll_bar_value(value: Value) -> bool {
    value.is_nil()
        || value == Value::T
        || VerticalScrollBarType::from_symbol_value(&value).is_some()
}

pub fn is_valid_horizontal_scroll_bar_value(value: Value) -> bool {
    value.is_nil()
        || value == Value::T
        || HorizontalScrollBarType::from_symbol_value(&value).is_some()
}

#[cfg(test)]
mod tests {
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
}
