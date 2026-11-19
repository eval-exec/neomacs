//! Dormant InlineSerialMain driver for the batch selection entries.
//!
//! Hosted activation remains refused. Nothing in this module is installed on
//! the serial call path. [`builtin_gui_get_selection`] and
//! [`builtin_gui_set_selection`] stay the direct bodies, and each function
//! here is a one-arm call of that body. Both still return today's batch
//! result. There is no thread test, TLS read, env read, clipboard read, or
//! second algorithm. `builtin_handle_switch_frame` (R37) and the five
//! clipboard and primary-selection stubs stay direct.

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
        assert!(
            !stubs.contains("selection_step") && !stubs.contains("SelectionInstaller"),
            "clipboard stubs stay direct until slice 3"
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
}
