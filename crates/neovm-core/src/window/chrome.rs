//! GNU window.c's chrome-presence rules, shared by queries and layout.
//!
//! Presence is live semantic state, not a cached height. The three format
//! variables are built-in buffer slots, so their values include defaults.

use super::{Frame, Window, WindowChromeLine, WindowLayoutVariable};
use crate::buffer::Buffer;
use crate::emacs_core::value::{Value, borrow_string_text_properties_table_for_value};

/// Proof required before a position query may discard a point change.
/// Unknown or executable chrome must be measured by the layout adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PointGeometryReuse {
    CertifiedIndependent,
    RequiresMeasurement,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WindowChromePresence {
    mode: bool,
    header: bool,
    tab: bool,
}

impl WindowChromePresence {
    pub const fn is_empty(self) -> bool {
        !(self.mode || self.header || self.tab)
    }

    pub fn resolve(window: &Window, buffer: &Buffer, frame: &Frame, minibuffer: bool) -> Self {
        if !window.is_leaf() || minibuffer {
            return Self::default();
        }
        let requested = |variable: WindowLayoutVariable| {
            let symbol = variable.sym_id();
            let parameter = window
                .parameters()
                .iter()
                .find(|(key, _)| key.as_bare_symbol_id() == Some(symbol))
                .map(|(_, value)| *value);
            parameter.is_none_or(|v| v.as_bare_symbol_name() != Some("none"))
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

    pub(super) fn point_geometry_reuse(
        self,
        window: &Window,
        buffer: &Buffer,
    ) -> PointGeometryReuse {
        for (line, variable) in [
            (
                WindowChromeLine::ModeLine,
                WindowLayoutVariable::ModeLineFormat,
            ),
            (
                WindowChromeLine::HeaderLine,
                WindowLayoutVariable::HeaderLineFormat,
            ),
            (
                WindowChromeLine::TabLine,
                WindowLayoutVariable::TabLineFormat,
            ),
        ] {
            if !self.contains(line) {
                continue;
            }
            let symbol = variable.sym_id();
            let format = window
                .parameters()
                .iter()
                .find(|(key, _)| key.as_bare_symbol_id() == Some(symbol))
                .map(|(_, value)| *value)
                .filter(|value| !value.is_nil())
                .or_else(|| buffer.buffer_local_value_id(symbol))
                .unwrap_or(Value::NIL);
            // A literal, property-free string has no executable display forms.
            // Admit only literal text, escaped percent signs and dash fill;
            // other percent substitutions can depend on point or Lisp values.
            let Some(text) = format.as_utf8_str() else {
                return PointGeometryReuse::RequiresMeasurement;
            };
            if borrow_string_text_properties_table_for_value(format).is_some() {
                return PointGeometryReuse::RequiresMeasurement;
            }
            let mut chars = text.chars();
            while let Some(ch) = chars.next() {
                if ch == '%' && !matches!(chars.next(), Some('%' | '-')) {
                    return PointGeometryReuse::RequiresMeasurement;
                }
            }
        }
        PointGeometryReuse::CertifiedIndependent
    }

    pub const fn contains(self, line: WindowChromeLine) -> bool {
        match line {
            WindowChromeLine::ModeLine => self.mode,
            WindowChromeLine::HeaderLine => self.header,
            WindowChromeLine::TabLine => self.tab,
        }
    }
}
