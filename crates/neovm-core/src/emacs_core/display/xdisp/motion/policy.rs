//! GNU window_scroll_pixel_based's viewport policy, independent of row production.

use super::{BufferId, Context, FrameId, Value, WindowId};
use crate::window::ScrollMargin;

/// Transient VM-owned goal, retained across consecutive scroll commands.
/// Not a Lisp object or persisted window configuration.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ScrollGoal {
    pub frame: FrameId,
    pub window: WindowId,
    pub buffer: BufferId,
    pub x: i64,
    pub y: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PreservePoint {
    KeepVisible,
    WhenOutside,
    Always,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ScrollPolicy {
    pub context_lines: i64,
    pub auto_vscroll: bool,
    pub preserve: PreservePoint,
    pub continues_scroll: bool,
    scroll_margin: ScrollMargin,
}

impl ScrollPolicy {
    pub fn capture(eval: &Context) -> Self {
        Self::from_values(eval, |name| eval.visible_variable_value_or_nil(name))
    }

    /// A captured window retains its own buffer-local viewport policy when
    /// layout Lisp switches the current buffer or selected window.
    pub fn capture_for_buffer(eval: &Context, buffer: BufferId) -> Self {
        Self::from_values(eval, |name| {
            eval.variables()
                .value_in_buffer_by_name(eval.buffers.get(buffer), name)
                .unwrap_or(Value::NIL)
        })
    }

    fn from_values(eval: &Context, value: impl Fn(&str) -> Value) -> Self {
        // GNU reads both values through `window_scroll_margin`
        // (src/window.c:6207): `scroll-margin` in lines and
        // `maximum-scroll-margin` only when it is a float.
        let scroll_margin = ScrollMargin::from_lisp_values(
            value("scroll-margin").as_fixnum().unwrap_or(0),
            value("maximum-scroll-margin").as_float(),
        );
        let continues_scroll = crate::emacs_core::builtins::symbol_property_get(
            eval,
            value("last-command"),
            Value::symbol("scroll-command"),
        )
        .ok()
        .and_then(|(_, property)| property)
        .is_some_and(|property| property.is_truthy());
        Self {
            context_lines: value("next-screen-context-lines")
                .as_fixnum()
                .unwrap_or(2)
                .clamp(0, 1_000_000),
            auto_vscroll: value("auto-window-vscroll").is_truthy(),
            preserve: match value("scroll-preserve-screen-position") {
                value if value.is_nil() => PreservePoint::KeepVisible,
                value if value == Value::T => PreservePoint::WhenOutside,
                _ => PreservePoint::Always,
            },
            continues_scroll,
            scroll_margin,
        }
    }

    pub fn margin_pixels(self, height: i64, line_height: i64) -> i64 {
        self.scroll_margin.pixels(height, line_height)
    }
}
