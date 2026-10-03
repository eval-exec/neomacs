//! Evaluator-published permission for predictable native pixel scrolling.
//! Read-only: keymap filters, autoloads and arbitrary Lisp are never executed
//! to speculate about an input. Unknown bindings retain ordinary delivery.

use crate::emacs_core::keymap::{
    command_remapping_lookup_in_keymaps, current_active_maps_for_position_read_only,
    list_keymap_lookup_one_unresolved, lookup_key_in_obarray,
};
use crate::emacs_core::{Context, Value};

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

    fn scroll_variable(&self, window: crate::window::WindowId, name: &str) -> Option<Value> {
        let frame = self.frame_manager().selected_frame()?;
        let buffer = frame
            .find_window(window)
            .and_then(|window| window.buffer_id())
            .and_then(|id| self.buffer_manager().get(id))?;
        let symbol = crate::emacs_core::builtins::symbols::resolve_variable_alias_id_in_obarray(
            self.obarray(),
            Value::symbol(name).as_symbol_id()?,
        )
        .ok()?;
        let value = self
            .obarray()
            .read_localized_in_buffer(symbol, buffer)
            .or_else(|| buffer.buffer_local_value_id(symbol))
            .or_else(|| self.obarray().symbol_value_id(symbol).copied())
            .unwrap_or(Value::NIL);
        (!value.is_unbound()).then_some(value)
    }

    pub fn compositor_scrolling_enabled(&self, window: crate::window::WindowId) -> bool {
        self.scroll_variable(window, "neomacs-compositor-scrolling")
            .is_some_and(|value| !value.is_nil())
    }

    pub fn permits_compositor_pixel_scroll(&self, window: crate::window::WindowId) -> bool {
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
        let variable = |name: &str| self.scroll_variable(window, name);
        if variable("neomacs-compositor-scrolling").is_none_or(|value| value.is_nil())
            || variable("pixel-scroll-precision-mode").is_none_or(|value| value.is_nil())
        {
            return false;
        }
        let mut hooks = variable("pre-command-hook");
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
                    self.obarray()
                        .default_value_id(Value::symbol("pre-command-hook").as_symbol_id().unwrap())
                        .copied()
                        .unwrap_or(Value::NIL),
                );
                continue;
            }
            let Some(name) = hook.as_symbol_name() else {
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
                && variable("eldoc-last-message").is_none_or(|value| !value.is_nil())
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
        for name in [
            "window-scroll-functions",
            "isearch-mode",
            "prefix-arg",
            "current-prefix-arg",
            "overriding-local-map",
            "overriding-terminal-local-map",
            "pixel-scroll-precision-large-scroll-height",
            "pixel-scroll-precision-use-momentum",
        ] {
            if variable(name).is_none_or(|value| !value.is_nil()) {
                tracing::debug!(target: "neovm_core::scroll_prediction", variable = name, value = ?variable(name), "prediction requires evaluator dispatch");
                return false;
            }
        }
        for name in [
            "pixel-scroll-precision",
            "pixel-scroll-precision-scroll-down",
            "pixel-scroll-precision-scroll-down-page",
            "pixel-scroll-precision-scroll-up",
            "pixel-scroll-precision-scroll-up-page",
            "pixel-scroll-precision-interpolate",
        ] {
            let Some(definition) = self.obarray().symbol_function(name) else {
                return false;
            };
            if definition.is_nil()
                || self
                    .obarray()
                    .get_property(name, "neomacs--scroll-definition")
                    != Some(definition)
            {
                tracing::debug!(target: "neovm_core::scroll_prediction", function = name, "scroll definition changed or is unverified");
                return false;
            }
        }
        for name in [
            "set-window-start",
            "set-window-vscroll",
            "window-text-pixel-size",
            "posn-at-x-y",
            "posn-at-point",
            "pos-visible-in-window-p",
            "vertical-motion",
        ] {
            if self
                .obarray()
                .symbol_function(name)
                .and_then(|value| value.as_subr_id())
                != Value::symbol(name).as_symbol_id()
            {
                tracing::debug!(target: "neovm_core::scroll_prediction", function = name, "scroll primitive is customized");
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
        let command = Value::symbol("pixel-scroll-precision");
        if command_remapping_lookup_in_keymaps(&maps, command.as_symbol_id().unwrap()).is_some() {
            return false;
        }
        ["wheel-up", "wheel-down"].iter().all(|name| {
            let event = Value::symbol(name);
            for name in [
                "key-translation-map",
                "input-decode-map",
                "local-function-key-map",
                "special-event-map",
            ] {
                let Some(map) = variable(name) else {
                    return false;
                };
                if !map.is_nil() && !list_keymap_lookup_one_unresolved(&map, &event).is_nil() {
                    return false;
                }
            }
            maps.iter()
                .find_map(|map| {
                    let binding = lookup_key_in_obarray(self.obarray(), map, &[event], true);
                    (!binding.is_nil()).then(|| {
                        binding == command
                            && list_keymap_lookup_one_unresolved(map, &event) == command
                    })
                })
                .unwrap_or(false)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolved_scroll_observer_sees_complete_commit_and_current_command_only() {
        let mut eval = Context::new();
        let buffer = eval.buffer_manager_mut().create_buffer("preview");
        eval.buffer_manager_mut()
            .get_mut(buffer)
            .unwrap()
            .insert("one\ntwo\nthree\n");
        let frame = eval
            .frame_manager_mut()
            .create_frame("preview", 800, 600, buffer);
        let window = eval.frame_manager().get(frame).unwrap().selected_window;
        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
        let observed = calls.clone();
        eval.scroll_preview_fn = Some(Box::new(move |eval, frame, window, inputs| {
            let state = eval
                .frame_manager()
                .get(frame)
                .unwrap()
                .find_window(window)
                .unwrap()
                .redisplay_state()
                .unwrap();
            assert_eq!(state.window_start.as_i64(), 5);
            assert_eq!(state.point.as_i64(), 5);
            assert_eq!(state.vscroll, -4);
            assert_eq!(inputs.len(), 1);
            observed.set(observed.get() + 1);
        }));
        let update = || crate::window::WindowScrollUpdate {
            frame,
            window,
            buffer,
            start: crate::buffer::LispCharPos1::new(5),
            point: crate::buffer::LispCharPos1::new(5),
            hidden_top_pixels: 4,
        };
        update().commit(&mut eval).unwrap();
        assert_eq!(calls.get(), 0);
        let stream = neomacs_display_protocol::input_progress::InputStream::default();
        let command = eval.input_progress.begin_command();
        eval.input_progress.consumed(stream.issue().unwrap());
        update().commit(&mut eval).unwrap();
        assert_eq!(calls.get(), 1);
        drop(command);
        update().commit(&mut eval).unwrap();
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn compositor_pixel_scroll_policy_requires_definition_and_binding_witnesses() {
        let mut eval = Context::new();
        let buffer = eval.buffer_manager_mut().create_buffer("scroll-policy");
        let frame = eval
            .frame_manager_mut()
            .create_frame("scroll-policy", 800, 600, buffer);
        let window = eval.frame_manager().get(frame).unwrap().selected_window;
        assert!(!eval.permits_compositor_pixel_scroll(window));
        eval.eval_str("(use-global-map (make-sparse-keymap)) (setq neomacs-compositor-scrolling t pixel-scroll-precision-mode t)").unwrap();
        for name in [
            "pixel-scroll-precision",
            "pixel-scroll-precision-scroll-down",
            "pixel-scroll-precision-scroll-down-page",
            "pixel-scroll-precision-scroll-up",
            "pixel-scroll-precision-scroll-up-page",
            "pixel-scroll-precision-interpolate",
        ] {
            eval.eval_str(&format!("(fset '{name} (lambda () nil)) (put '{name} 'neomacs--scroll-definition (symbol-function '{name}))")).unwrap();
        }
        eval.eval_str("(define-key (current-global-map) [wheel-up] 'pixel-scroll-precision) (define-key (current-global-map) [wheel-down] 'pixel-scroll-precision)").unwrap();
        assert!(eval.permits_compositor_pixel_scroll(window));
        let other_buffer = eval
            .buffer_manager_mut()
            .create_buffer("other-scroll-policy");
        let other_window = eval
            .frame_manager_mut()
            .split_window(
                frame,
                window,
                crate::window::SplitDirection::Horizontal,
                other_buffer,
                None,
                crate::window::SplitPlacement::AfterTarget,
            )
            .unwrap();
        assert!(eval.compositor_scrolling_enabled(other_window));
        assert!(!eval.permits_compositor_pixel_scroll(other_window));
        eval.buffer_manager_mut()
            .get_mut(other_buffer)
            .unwrap()
            .set_buffer_local("neomacs-compositor-scrolling", Value::NIL);
        assert!(!eval.compositor_scrolling_enabled(other_window));
        assert!(eval.compositor_scrolling_enabled(window));
        let publications = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let observed = publications.clone();
        eval.redisplay_fn = Some(Box::new(move |eval| {
            observed
                .borrow_mut()
                .push(eval.permits_compositor_pixel_scroll(window));
            crate::test_utils::mock_redisplay::accept_all_frames(eval);
        }));
        eval.redisplay().expect("redisplay");
        eval.redisplay().expect("redisplay");
        assert_eq!(&*publications.borrow(), &[true]);
        eval.eval_str("(setq neomacs-compositor-scrolling nil)")
            .unwrap();
        eval.redisplay().expect("redisplay");
        assert_eq!(
            &*publications.borrow(),
            &[true, false],
            "policy-only changes must publish revocation"
        );
        eval.eval_str("(setq neomacs-compositor-scrolling t)")
            .unwrap();
        eval.redisplay().expect("redisplay");
        assert_eq!(&*publications.borrow(), &[true, false, true]);
        eval.eval_str("(define-key (current-global-map) [wheel-down] '(menu-item \"scroll\" pixel-scroll-precision :filter ignore))").unwrap();
        assert!(
            !eval.permits_compositor_pixel_scroll(window),
            "menu filters require actual dispatch"
        );
        eval.eval_str("(define-key (current-global-map) [wheel-down] 'ignore)")
            .unwrap();
        assert!(!eval.permits_compositor_pixel_scroll(window));
        eval.eval_str("(define-key (current-global-map) [wheel-down] 'pixel-scroll-precision) (setq pre-command-hook '(ignore))").unwrap();
        assert!(!eval.permits_compositor_pixel_scroll(window));
        eval.eval_str("(setq pre-command-hook nil)").unwrap();
        let local = |eval: &mut Context, name: &str, value| {
            eval.buffer_manager_mut()
                .get_mut(buffer)
                .unwrap()
                .set_buffer_local(name, value);
        };
        local(
            &mut eval,
            "pre-command-hook",
            Value::list(vec![Value::symbol("ignore")]),
        );
        assert!(
            !eval.permits_compositor_pixel_scroll(window),
            "displayed buffer hooks veto prediction"
        );
        local(&mut eval, "pre-command-hook", Value::NIL);
        eval.buffer_manager_mut()
            .current_buffer_mut()
            .unwrap()
            .set_buffer_local(
                "pre-command-hook",
                Value::list(vec![Value::symbol("ignore")]),
            );
        assert!(
            eval.permits_compositor_pixel_scroll(window),
            "timer buffer hooks do not belong to the displayed buffer"
        );
        eval.eval_str("(fset 'eldoc-pre-command-refresh-echo-area (lambda () nil)) (put 'eldoc-pre-command-refresh-echo-area 'neomacs--scroll-definition (symbol-function 'eldoc-pre-command-refresh-echo-area))").unwrap();
        local(
            &mut eval,
            "pre-command-hook",
            Value::list(vec![
                Value::symbol("eldoc-pre-command-refresh-echo-area"),
                Value::T,
            ]),
        );
        assert!(eval.permits_compositor_pixel_scroll(window));
        local(
            &mut eval,
            "eldoc-last-message",
            Value::string("documentation"),
        );
        assert!(!eval.permits_compositor_pixel_scroll(window));
        local(&mut eval, "eldoc-last-message", Value::NIL);
        eval.eval_str("(fset 'eldoc-pre-command-refresh-echo-area (lambda () t))")
            .unwrap();
        assert!(!eval.permits_compositor_pixel_scroll(window));
        local(&mut eval, "pre-command-hook", Value::NIL);
        eval.eval_str("(setq pre-command-hook nil) (fset 'pixel-scroll-precision (lambda () t))")
            .unwrap();
        assert!(!eval.permits_compositor_pixel_scroll(window));
    }
}
