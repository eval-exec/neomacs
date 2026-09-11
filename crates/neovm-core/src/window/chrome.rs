//! GNU window.c's chrome-presence rules, shared by queries and layout.
//!
//! Presence is live semantic state, not a cached height. The three format
//! variables are built-in buffer slots, so their values include defaults.

use super::{Frame, Window, WindowChromeLine, WindowLayoutVariable};
use crate::buffer::Buffer;

#[derive(Clone, Copy, Debug, Default)]
pub struct WindowChromePresence {
    mode: bool,
    header: bool,
    tab: bool,
}

impl WindowChromePresence {
    pub fn resolve(window: &Window, buffer: &Buffer, frame: &Frame, minibuffer: bool) -> Self {
        if !window.is_leaf() || minibuffer {
            return Self::default();
        }
        let requested = |variable: WindowLayoutVariable| {
            let symbol = variable.sym_id();
            let parameter = window
                .parameters()
                .iter()
                .find(|(key, _)| key.as_symbol_id() == Some(symbol))
                .map(|(_, value)| *value);
            parameter.is_none_or(|v| v.as_symbol_name() != Some("none"))
                && (parameter.is_some_and(|v| v.is_truthy())
                    || buffer
                        .buffer_local_value_id(symbol)
                        .is_some_and(|v| v.is_truthy()))
        };
        let rows = window.bounds().height / frame.char_height.max(1.0);
        let mode = requested(WindowLayoutVariable::ModeLineFormat) && rows > 1.0;
        let header =
            requested(WindowLayoutVariable::HeaderLineFormat) && rows > 1.0 + u8::from(mode) as f32;
        let tab = requested(WindowLayoutVariable::TabLineFormat)
            && rows > 1.0 + u8::from(mode) as f32 + u8::from(header) as f32;
        Self { mode, header, tab }
    }

    pub const fn contains(self, line: WindowChromeLine) -> bool {
        match line {
            WindowChromeLine::ModeLine => self.mode,
            WindowChromeLine::HeaderLine => self.header,
            WindowChromeLine::TabLine => self.tab,
        }
    }
}
