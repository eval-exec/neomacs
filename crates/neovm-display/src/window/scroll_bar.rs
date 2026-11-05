use neovm_value::value::Value;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use strum::{EnumString, IntoStaticStr};

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
pub use neovm_eval::emacs_core::eval::data::window_state::{
    HorizontalScrollBarType, VerticalScrollBarType,
};
