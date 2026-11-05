use super::{FrameManager, Window, WindowId};
use neovm_value::value::Value;

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
