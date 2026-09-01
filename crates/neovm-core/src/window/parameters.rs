use super::{FrameManager, Window, WindowId};
use crate::emacs_core::value::{Value, eq_value};

fn alist_from_parameters(parameters: &[(Value, Value)]) -> Value {
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

impl FrameManager {
    fn live_window_parameters(&self, window_id: WindowId) -> Option<&[(Value, Value)]> {
        self.frames
            .values()
            .find_map(|frame| frame.find_window(window_id).map(Window::parameters))
            .map(Vec::as_slice)
    }

    fn live_window_parameters_mut(
        &mut self,
        window_id: WindowId,
    ) -> Option<&mut Vec<(Value, Value)>> {
        let frame_id = self.find_valid_window_frame_id(window_id)?;
        self.get_mut(frame_id)
            .and_then(|frame| frame.find_window_mut(window_id))
            .map(Window::parameters_mut)
    }

    /// Return window parameter KEY for WINDOW-ID, or nil when unset.
    pub fn window_parameter(&self, window_id: WindowId, key: &Value) -> Option<Value> {
        self.live_window_parameters(window_id)
            .or_else(|| {
                self.deleted_window_parameters
                    .get(&window_id)
                    .map(Vec::as_slice)
            })
            .and_then(|pairs| {
                pairs
                    .iter()
                    .find(|(existing_key, _)| eq_value(existing_key, key))
                    .map(|(_, value)| *value)
            })
    }

    /// Set window parameter KEY on WINDOW-ID to VALUE.
    pub fn set_window_parameter(&mut self, window_id: WindowId, key: Value, value: Value) {
        let parameters = if let Some(parameters) = self.live_window_parameters_mut(window_id) {
            parameters
        } else {
            self.deleted_window_parameters.entry(window_id).or_default()
        };

        if let Some((_, existing_value)) = parameters
            .iter_mut()
            .find(|(existing_key, _)| eq_value(existing_key, &key))
        {
            *existing_value = value;
        } else {
            parameters.insert(0, (key, value));
        }
    }

    /// Return window parameters alist for WINDOW-ID.
    pub fn window_parameters_alist(&self, window_id: WindowId) -> Value {
        self.live_window_parameters(window_id)
            .map(alist_from_parameters)
            .unwrap_or(Value::NIL)
    }

    /// Return WINDOW-ID's `(KEY . VALUE)` window-parameter pairs as an owned
    /// vec (empty when the window is unknown). The layout engine reads these to
    /// evaluate GNU `(:window PARAMETER VALUE)` `:filtered` face predicates,
    /// which it cannot express via the Lisp-alist accessor without re-parsing.
    pub fn window_parameters_pairs(&self, window_id: WindowId) -> Vec<(Value, Value)> {
        self.live_window_parameters(window_id)
            .map(<[(Value, Value)]>::to_vec)
            .unwrap_or_default()
    }
}
