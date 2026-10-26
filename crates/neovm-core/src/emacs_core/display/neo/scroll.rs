//! Evaluator-published permission for predictable native pixel scrolling.
//! Read-only: keymap filters, autoloads and arbitrary Lisp are never executed
//! to speculate about an input. Unknown bindings retain ordinary delivery.

use crate::emacs_core::intern::{EpochCachedSymbol, SymId};
use crate::emacs_core::keymap::{
    command_remapping_lookup_in_keymaps, current_active_maps_for_position_read_only,
    list_keymap_lookup_one_unresolved, lookup_key_in_obarray,
};
use crate::emacs_core::{Context, Value};

/// A fixed spelling consumed by the raw-input scroll predictor.
///
/// GNU reads these as DEFVAR/DEFUN statics (`Vpixel_scroll_precision_mode`,
/// `Qpre_command_hook`, ...). The predictor runs on every input event, and each
/// probe previously built a symbol from the printed name, re-interning it.
/// Each handle resolves once per registry epoch; no variable or function value
/// is cached, and every read keeps its original buffer/owner boundary.
struct ScrollSymbol {
    name: &'static str,
    handle: EpochCachedSymbol,
}

impl ScrollSymbol {
    const fn new(name: &'static str) -> Self {
        Self {
            name,
            handle: EpochCachedSymbol::new(name),
        }
    }

    #[inline]
    fn id(&self) -> SymId {
        self.handle.get()
    }

    /// The symbol Value this probe used to construct, with the same
    /// constructor accounting.
    #[inline]
    fn value(&self) -> Value {
        Value::symbol(&self.handle)
    }
}

const NEOMACS_COMPOSITOR_SCROLLING: ScrollSymbol =
    ScrollSymbol::new("neomacs-compositor-scrolling");
const PIXEL_SCROLL_PRECISION_MODE: ScrollSymbol = ScrollSymbol::new("pixel-scroll-precision-mode");
const PRE_COMMAND_HOOK: ScrollSymbol = ScrollSymbol::new("pre-command-hook");
const ELDOC_LAST_MESSAGE: ScrollSymbol = ScrollSymbol::new("eldoc-last-message");
const NEOMACS_SCROLL_DEFINITION: ScrollSymbol = ScrollSymbol::new("neomacs--scroll-definition");
const WINDOW_SCROLL_FUNCTIONS: ScrollSymbol = ScrollSymbol::new("window-scroll-functions");
const ISEARCH_MODE: ScrollSymbol = ScrollSymbol::new("isearch-mode");
const PREFIX_ARG: ScrollSymbol = ScrollSymbol::new("prefix-arg");
const CURRENT_PREFIX_ARG: ScrollSymbol = ScrollSymbol::new("current-prefix-arg");
const OVERRIDING_LOCAL_MAP: ScrollSymbol = ScrollSymbol::new("overriding-local-map");
const OVERRIDING_TERMINAL_LOCAL_MAP: ScrollSymbol =
    ScrollSymbol::new("overriding-terminal-local-map");
const PIXEL_SCROLL_PRECISION_LARGE_SCROLL_HEIGHT: ScrollSymbol =
    ScrollSymbol::new("pixel-scroll-precision-large-scroll-height");
const PIXEL_SCROLL_PRECISION_USE_MOMENTUM: ScrollSymbol =
    ScrollSymbol::new("pixel-scroll-precision-use-momentum");
const PIXEL_SCROLL_PRECISION: ScrollSymbol = ScrollSymbol::new("pixel-scroll-precision");
const PIXEL_SCROLL_PRECISION_SCROLL_DOWN: ScrollSymbol =
    ScrollSymbol::new("pixel-scroll-precision-scroll-down");
const PIXEL_SCROLL_PRECISION_SCROLL_DOWN_PAGE: ScrollSymbol =
    ScrollSymbol::new("pixel-scroll-precision-scroll-down-page");
const PIXEL_SCROLL_PRECISION_SCROLL_UP: ScrollSymbol =
    ScrollSymbol::new("pixel-scroll-precision-scroll-up");
const PIXEL_SCROLL_PRECISION_SCROLL_UP_PAGE: ScrollSymbol =
    ScrollSymbol::new("pixel-scroll-precision-scroll-up-page");
const PIXEL_SCROLL_PRECISION_INTERPOLATE: ScrollSymbol =
    ScrollSymbol::new("pixel-scroll-precision-interpolate");
const SET_WINDOW_START: ScrollSymbol = ScrollSymbol::new("set-window-start");
const SET_WINDOW_VSCROLL: ScrollSymbol = ScrollSymbol::new("set-window-vscroll");
const WINDOW_TEXT_PIXEL_SIZE: ScrollSymbol = ScrollSymbol::new("window-text-pixel-size");
const POSN_AT_X_Y: ScrollSymbol = ScrollSymbol::new("posn-at-x-y");
const POSN_AT_POINT: ScrollSymbol = ScrollSymbol::new("posn-at-point");
const POS_VISIBLE_IN_WINDOW_P: ScrollSymbol = ScrollSymbol::new("pos-visible-in-window-p");
const VERTICAL_MOTION: ScrollSymbol = ScrollSymbol::new("vertical-motion");
const KEY_TRANSLATION_MAP: ScrollSymbol = ScrollSymbol::new("key-translation-map");
const INPUT_DECODE_MAP: ScrollSymbol = ScrollSymbol::new("input-decode-map");
const LOCAL_FUNCTION_KEY_MAP: ScrollSymbol = ScrollSymbol::new("local-function-key-map");
const SPECIAL_EVENT_MAP: ScrollSymbol = ScrollSymbol::new("special-event-map");
const WHEEL_UP: ScrollSymbol = ScrollSymbol::new("wheel-up");
const WHEEL_DOWN: ScrollSymbol = ScrollSymbol::new("wheel-down");

impl Context {
    pub(crate) fn publish_committed_scroll_preview(
        &mut self,
        frame: crate::window::FrameId,
        window: crate::window::WindowId,
    ) {
        if self.scroll_preview_fn.is_none() {
            return;
        }
        let inputs = self.input_progress.current_command_receipts();
        if inputs.is_empty() {
            return;
        }
        let Some(mut observer) = self.scroll_preview_fn.take() else {
            return;
        };
        observer(self, frame, window, inputs);
        self.scroll_preview_fn = Some(observer);
    }

    fn scroll_variable(
        &self,
        window: crate::window::WindowId,
        variable: &ScrollSymbol,
    ) -> Option<Value> {
        let symbol_mode = self.symbol_mode();
        let frame = self.frame_manager().selected_frame()?;
        let buffer = frame
            .find_window(window)
            .and_then(|window| window.buffer_id())
            .and_then(|id| self.buffer_manager().get(id))?;
        let symbol = crate::emacs_core::builtins::symbols::resolve_variable_alias_id_in_obarray(
            self.obarray(),
            variable.value().as_symbol_id(symbol_mode)?,
        )
        .ok()?;
        let value = self
            .obarray()
            .read_localized_in_buffer(symbol, buffer)
            .or_else(|| buffer.buffer_local_value_id(symbol))
            .or_else(|| self.variable_value(symbol))
            .unwrap_or(Value::NIL);
        (!value.is_unbound()).then_some(value)
    }

    pub fn compositor_scrolling_enabled(&self, window: crate::window::WindowId) -> bool {
        self.scroll_variable(window, &NEOMACS_COMPOSITOR_SCROLLING)
            .is_some_and(|value| !value.is_nil())
    }

    pub fn permits_compositor_pixel_scroll(&self, window: crate::window::WindowId) -> bool {
        let symbol_mode = self.symbol_mode();
        // Raw input prediction relies on the selected window's keymaps and
        // command context. A resolved preview already has its destination
        // from canonical dispatch and only needs the target buffer's opt-in.
        if self
            .frame_manager()
            .selected_frame()
            .is_none_or(|frame| frame.selected_window != window)
        {
            return false;
        }
        // Timer redisplay may run with another current buffer. Policy belongs
        // to the displayed buffer and must not use that callback's locals.
        let variable = |symbol: &ScrollSymbol| self.scroll_variable(window, symbol);
        if variable(&NEOMACS_COMPOSITOR_SCROLLING).is_none_or(|value| value.is_nil())
            || variable(&PIXEL_SCROLL_PRECISION_MODE).is_none_or(|value| value.is_nil())
        {
            return false;
        }
        let mut hooks = variable(&PRE_COMMAND_HOOK);
        let mut global = false;
        // A local hook's `t` includes the global hook. Only the verified ElDoc
        // no-op and standard non-scrolling hooks are predictable. Bound the
        // traversal to reject cycles.
        for _ in 0..32 {
            let Some(tail) = hooks else { return false };
            if tail.is_nil() {
                break;
            }
            if !tail.is_cons() {
                return false;
            }
            let hook = tail.cons_car();
            hooks = Some(tail.cons_cdr());
            if hook == Value::T && !global && tail.cons_cdr().is_nil() {
                global = true;
                hooks = Some(
                    self.variables()
                        .default_value(PRE_COMMAND_HOOK.value().as_symbol_id(symbol_mode).unwrap())
                        .unwrap_or(Value::NIL),
                );
                continue;
            }
            let Some(name) = hook.as_symbol_name(symbol_mode) else {
                return false;
            };
            if !matches!(
                name,
                "eldoc-pre-command-refresh-echo-area"
                    | "neomacs--release-startup-gc-ceiling"
                    | "tooltip-hide"
            ) {
                return false;
            }
            if name == "eldoc-pre-command-refresh-echo-area"
                && variable(&ELDOC_LAST_MESSAGE).is_none_or(|value| !value.is_nil())
            {
                return false;
            }
            let definition = self.obarray().symbol_function(name);
            if definition.is_none_or(|definition| {
                definition.is_nil()
                    || self
                        .obarray()
                        .get_property(name, "neomacs--scroll-definition")
                        != Some(definition)
            }) {
                return false;
            }
        }
        if hooks.is_none_or(|hooks| !hooks.is_nil()) {
            return false;
        }
        for symbol in [
            &WINDOW_SCROLL_FUNCTIONS,
            &ISEARCH_MODE,
            &PREFIX_ARG,
            &CURRENT_PREFIX_ARG,
            &OVERRIDING_LOCAL_MAP,
            &OVERRIDING_TERMINAL_LOCAL_MAP,
            &PIXEL_SCROLL_PRECISION_LARGE_SCROLL_HEIGHT,
            &PIXEL_SCROLL_PRECISION_USE_MOMENTUM,
        ] {
            if variable(symbol).is_none_or(|value| !value.is_nil()) {
                tracing::debug!(target: "neovm_core::scroll_prediction", variable = symbol.name, value = ?variable(symbol), "prediction requires evaluator dispatch");
                return false;
            }
        }
        for symbol in [
            &PIXEL_SCROLL_PRECISION,
            &PIXEL_SCROLL_PRECISION_SCROLL_DOWN,
            &PIXEL_SCROLL_PRECISION_SCROLL_DOWN_PAGE,
            &PIXEL_SCROLL_PRECISION_SCROLL_UP,
            &PIXEL_SCROLL_PRECISION_SCROLL_UP_PAGE,
            &PIXEL_SCROLL_PRECISION_INTERPOLATE,
        ] {
            let Some(definition) = self.obarray().symbol_function_id(symbol.id()) else {
                return false;
            };
            if definition.is_nil()
                || self
                    .obarray()
                    .get_property_id(symbol.id(), NEOMACS_SCROLL_DEFINITION.id())
                    != Some(definition)
            {
                tracing::debug!(target: "neovm_core::scroll_prediction", function = symbol.name, "scroll definition changed or is unverified");
                return false;
            }
        }
        for symbol in [
            &SET_WINDOW_START,
            &SET_WINDOW_VSCROLL,
            &WINDOW_TEXT_PIXEL_SIZE,
            &POSN_AT_X_Y,
            &POSN_AT_POINT,
            &POS_VISIBLE_IN_WINDOW_P,
            &VERTICAL_MOTION,
        ] {
            if self
                .obarray()
                .symbol_function_id(symbol.id())
                .and_then(|value| value.as_subr_id())
                != symbol.value().as_symbol_id(symbol_mode)
            {
                tracing::debug!(target: "neovm_core::scroll_prediction", function = symbol.name, "scroll primitive is customized");
                return false;
            }
        }
        let Ok(maps) = current_active_maps_for_position_read_only(
            self,
            true,
            Some(&Value::make_window(window.0)),
        ) else {
            return false;
        };
        let command = PIXEL_SCROLL_PRECISION.value();
        if command_remapping_lookup_in_keymaps(
            &maps,
            command.as_symbol_id(symbol_mode).unwrap(),
            symbol_mode,
        )
        .is_some()
        {
            return false;
        }
        [&WHEEL_UP, &WHEEL_DOWN].iter().all(|symbol| {
            let event = symbol.value();
            for map_symbol in [
                &KEY_TRANSLATION_MAP,
                &INPUT_DECODE_MAP,
                &LOCAL_FUNCTION_KEY_MAP,
                &SPECIAL_EVENT_MAP,
            ] {
                let Some(map) = variable(map_symbol) else {
                    return false;
                };
                if !map.is_nil()
                    && !list_keymap_lookup_one_unresolved(&map, &event, symbol_mode).is_nil()
                {
                    return false;
                }
            }
            maps.iter()
                .find_map(|map| {
                    let binding =
                        lookup_key_in_obarray(self.obarray(), map, &[event], true, symbol_mode);
                    (!binding.is_nil()).then(|| {
                        binding == command
                            && list_keymap_lookup_one_unresolved(map, &event, symbol_mode)
                                == command
                    })
                })
                .unwrap_or(false)
        })
    }
}

#[cfg(test)]
#[path = "tests/scroll_test.rs"]
mod tests;
