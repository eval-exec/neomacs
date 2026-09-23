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
#[path = "scroll_bar/tests/scroll_bar_test.rs"]
mod tests;
