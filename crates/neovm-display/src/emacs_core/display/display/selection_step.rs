//! Dormant InlineSerialMain driver for the batch selection entries.
//!
//! Hosted activation remains refused. Nothing in this module is installed on
//! the serial call path. [`builtin_gui_get_selection`] and
//! [`builtin_gui_set_selection`] stay the direct bodies, and each function
//! here is a one-arm call of that body. Both still return today's batch
//! result. There is no thread test, TLS read, env read, clipboard read, or
//! second algorithm. `builtin_handle_switch_frame` (R37) stays direct.
//! The clipboard stubs read the owner's installed phase: `Uninstalled` is this
//! inline serial arm unless a hosted Green worker routes it, `Local` lowers to
//! the same direct body, and a hosted Green worker's routed phase executes the
//! C4b host call.

use neovm_value::error::EvalResult;
use neovm_value::value::Value;

use super::{builtin_gui_get_selection, builtin_gui_set_selection};

/// Serial installer. Owned drivers are not constructible in this slice.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "dormant until hosted activation (S2 step 2)")
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelectionInstaller {
    InlineSerialMain,
}

/// Serial step. Suspension is uninhabited: the only arm is the direct result.
#[derive(Debug)]
pub(crate) enum SelectionStep<T> {
    Done(T),
}

impl<T> SelectionStep<T> {
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "dormant until hosted activation (S2 step 2)")
    )]
    pub(crate) fn into_direct(self) -> T {
        let SelectionStep::Done(value) = self;
        value
    }
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "dormant until hosted activation (S2 step 2)")
)]
pub(crate) fn gui_get_selection(
    installer: SelectionInstaller,
    args: Vec<Value>,
) -> SelectionStep<EvalResult> {
    match installer {
        SelectionInstaller::InlineSerialMain => {
            SelectionStep::Done(builtin_gui_get_selection(args))
        }
    }
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "dormant until hosted activation (S2 step 2)")
)]
pub(crate) fn gui_set_selection(
    installer: SelectionInstaller,
    args: Vec<Value>,
) -> SelectionStep<EvalResult> {
    match installer {
        SelectionInstaller::InlineSerialMain => {
            SelectionStep::Done(builtin_gui_set_selection(args))
        }
    }
}

/// Batch selection entries whose lowering must match the parent.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "dormant until hosted activation (S2 step 2)")
)]
pub(crate) const SERIAL_SELECTION_ENTRIES: &[&str] =
    &["builtin_gui_get_selection", "builtin_gui_set_selection"];

#[cfg(test)]
mod tests {
    use super::{
        SERIAL_SELECTION_ENTRIES, SelectionInstaller, gui_get_selection, gui_set_selection,
    };
    use neovm_value::error::EvalResult;
    use neovm_value::value::Value;

    fn assert_same_batch(direct: EvalResult, via: EvalResult) {
        match (direct, via) {
            (Ok(left), Ok(right)) => assert_eq!(left, right),
            (Err(left), Err(right)) => {
                let left = left.as_signal().expect("direct batch error is a signal");
                let right = right.as_signal().expect("step batch error is a signal");
                assert_eq!(left.symbol_name(), right.symbol_name());
                assert_eq!(left.data, right.data);
                assert_eq!(left.raw_data, right.raw_data);
            }
            _ => panic!("direct body and selection step disagree"),
        }
    }

    #[test]
    fn serial_bodies_do_not_call_the_executor() {
        let source = include_str!("mod.rs");
        assert!(
            !source.contains("selection_step::") && !source.contains("SelectionInstaller"),
            "serial selection bodies must stay the direct functions"
        );
        for name in SERIAL_SELECTION_ENTRIES {
            assert!(
                source.contains(&format!("fn {name}(")),
                "missing serial entry {name}"
            );
        }
        let frame = include_str!("../frame/selection.rs");
        assert!(
            frame.contains("fn builtin_handle_switch_frame(")
                && !frame.contains("selection_step")
                && !frame.contains("SelectionInstaller"),
            "R37 stays direct"
        );
        let stubs =
            include_str!("../../../../../neovm-eval/src/emacs_core/lisp/native/builtins/stubs.rs");
        let clipboard_step = include_str!(
            "../../../../../neovm-eval/src/emacs_core/lisp/native/builtins/selection_step.rs"
        );
        for name in [
            "builtin_neomacs_clipboard_set",
            "builtin_neomacs_clipboard_get",
            "builtin_neomacs_primary_selection_set",
            "builtin_neomacs_primary_selection_get",
            "builtin_neomacs_primary_selection_owner",
        ] {
            assert!(
                stubs.contains(&format!("fn {name}(")),
                "missing clipboard stub {name}"
            );
        }
        let clipboard_driver = clipboard_step.split("#[cfg(test)]").next().expect("driver");
        assert!(
            stubs.contains("mod selection_step;")
                && stubs.matches("selection_step::installed(ctx)").count() >= 5,
            "clipboard stubs must ask the installed phase, not hard-code an arm"
        );
        assert!(
            clipboard_driver.contains("SelectionPhase::Uninstalled")
                && clipboard_driver.contains("SelectionInstaller::InlineSerialMain")
                && clipboard_driver
                    .contains("SelectionPhase::Local => SelectionInstaller::OwnedLocalMain")
                && clipboard_driver
                    .contains("SelectionPhase::Routed | SelectionPhase::AwaitingRollback")
                && clipboard_driver.contains("SelectionInstaller::OwnedRoutedWorker")
                && clipboard_driver
                    .contains("ctx.is_green_worker_owner() && ctx.in_hosted_green_session()")
                && clipboard_driver
                    .matches("OwnedRoutedWorker => SelectionStep::Done(routed_")
                    .count()
                    == 5
                && !clipboard_driver.contains("std::env"),
            "clipboard stubs keep the direct body for the serial/local arms; a \
             hosted Green worker's routed phase maps to OwnedRoutedWorker, and \
             each of the five entries executes its routed host call (C4b)"
        );
    }

    #[test]
    fn inline_serial_get_matches_the_direct_body() {
        let samples = [
            vec![],
            vec![Value::symbol("PRIMARY")],
            vec![Value::symbol("PRIMARY"), Value::symbol("STRING")],
            vec![Value::NIL, Value::NIL, Value::NIL],
        ];
        for args in samples {
            let direct = super::super::builtin_gui_get_selection(args.clone());
            let via = gui_get_selection(SelectionInstaller::InlineSerialMain, args).into_direct();
            assert_same_batch(direct, via);
        }
    }

    #[test]
    fn inline_serial_set_matches_the_direct_body() {
        let samples = [
            vec![],
            vec![Value::symbol("PRIMARY")],
            vec![Value::symbol("PRIMARY"), Value::symbol("x")],
            vec![Value::NIL, Value::NIL, Value::NIL],
        ];
        for args in samples {
            let direct = super::super::builtin_gui_set_selection(args.clone());
            let via = gui_set_selection(SelectionInstaller::InlineSerialMain, args).into_direct();
            assert_same_batch(direct, via);
        }
    }

    #[test]
    fn serial_lowering_is_only_the_direct_call() {
        let step = include_str!("selection_step.rs");
        let driver = step.split("#[cfg(test)]").next().expect("driver");
        let arms = [
            (
                "fn gui_get_selection(",
                "SelectionStep::Done(builtin_gui_get_selection(args))",
            ),
            (
                "fn gui_set_selection(",
                "SelectionStep::Done(builtin_gui_set_selection(args))",
            ),
        ];
        assert_eq!(driver.matches("match installer").count(), arms.len());
        assert!(
            !driver.contains("std::env")
                && !driver.contains("NEOVM_THREADS")
                && !driver.contains("thread_local")
                && !driver.contains(".with(")
                && !driver.contains("OwnedRouted")
                && !driver.contains("OwnedLocal"),
            "the selection step adds no route, TLS read, or env test"
        );
        for (signature, call) in arms {
            let start = driver.find(signature).expect(signature);
            let rest = &driver[start..];
            let close = rest.find("\n}").expect("function end");
            let function = &rest[..=close + 1];
            assert_eq!(function.matches("=>").count(), 1, "{signature}");
            assert!(
                function.contains(call) && !function.contains("if "),
                "{signature} must lower to {call}"
            );
        }
        let source = include_str!("mod.rs");
        for name in SERIAL_SELECTION_ENTRIES {
            let marker = format!("fn {name}(");
            let start = source.find(&marker).expect(name);
            let rest = &source[start..];
            let close = rest.find("\n}").expect("entry end");
            let function = &rest[..=close + 1];
            assert!(
                !function.contains("selection_step") && !function.contains("SelectionInstaller"),
                "{name} stays the direct function"
            );
        }
    }
}
