use super::DisplayWindowExt;
use super::{FrameManager, Window, WindowId};
use crate::emacs_core::value::{Value, eq_value};

pub(crate) fn alist_from_parameters(parameters: &[(Value, Value)]) -> Value {
    if parameters.is_empty() {
        Value::NIL
    } else {
        Value::list(
            parameters
                .iter()
                .map(|(key, value)| Value::cons(*key, *value))
                .collect(),
        )
    }
}
