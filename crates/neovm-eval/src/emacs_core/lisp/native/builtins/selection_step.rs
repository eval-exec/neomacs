//! Dormant InlineSerialMain driver for the five clipboard and primary stubs.
//!
//! Hosted activation remains refused. Each public stub calls one of these
//! functions with [`SelectionInstaller::InlineSerialMain`] and unwraps
//! [`SelectionStep::Done`]. The arm calls the existing direct body. There is
//! no second installer, route branch, TLS read, or env test in this module.
//! The direct bodies keep the cache they already had.

use neovm_value::error::EvalResult;
use neovm_value::value::Value;

use super::{
    direct_neomacs_clipboard_get, direct_neomacs_clipboard_set,
    direct_neomacs_primary_selection_get, direct_neomacs_primary_selection_owner,
    direct_neomacs_primary_selection_set,
};

/// Serial installer. Owned drivers are not constructible in this slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SelectionInstaller {
    InlineSerialMain,
}

/// Serial step. Suspension is uninhabited: the only arm is the direct result.
#[derive(Debug)]
pub(super) enum SelectionStep<T> {
    Done(T),
}

impl<T> SelectionStep<T> {
    #[inline(always)]
    pub(super) fn into_direct(self) -> T {
        let SelectionStep::Done(value) = self;
        value
    }
}

#[inline(always)]
pub(super) fn clipboard_set(
    installer: SelectionInstaller,
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> SelectionStep<EvalResult> {
    match installer {
        SelectionInstaller::InlineSerialMain => {
            SelectionStep::Done(direct_neomacs_clipboard_set(ctx, args))
        }
    }
}

#[inline(always)]
pub(super) fn clipboard_get(
    installer: SelectionInstaller,
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> SelectionStep<EvalResult> {
    match installer {
        SelectionInstaller::InlineSerialMain => {
            SelectionStep::Done(direct_neomacs_clipboard_get(ctx, args))
        }
    }
}

#[inline(always)]
pub(super) fn primary_selection_set(
    installer: SelectionInstaller,
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> SelectionStep<EvalResult> {
    match installer {
        SelectionInstaller::InlineSerialMain => {
            SelectionStep::Done(direct_neomacs_primary_selection_set(ctx, args))
        }
    }
}

#[inline(always)]
pub(super) fn primary_selection_get(
    installer: SelectionInstaller,
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> SelectionStep<EvalResult> {
    match installer {
        SelectionInstaller::InlineSerialMain => {
            SelectionStep::Done(direct_neomacs_primary_selection_get(ctx, args))
        }
    }
}

#[inline(always)]
pub(super) fn primary_selection_owner(
    installer: SelectionInstaller,
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> SelectionStep<EvalResult> {
    match installer {
        SelectionInstaller::InlineSerialMain => {
            SelectionStep::Done(direct_neomacs_primary_selection_owner(ctx, args))
        }
    }
}

/// Clipboard entries whose serial path is this one installer.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "dormant until hosted activation (S2 step 2)")
)]
pub(super) const SERIAL_CLIPBOARD_ENTRIES: &[&str] = &[
    "builtin_neomacs_clipboard_set",
    "builtin_neomacs_clipboard_get",
    "builtin_neomacs_primary_selection_set",
    "builtin_neomacs_primary_selection_get",
    "builtin_neomacs_primary_selection_owner",
];

#[cfg(test)]
mod tests {
    use super::{
        SERIAL_CLIPBOARD_ENTRIES, SelectionInstaller, clipboard_get, clipboard_set,
        primary_selection_get, primary_selection_owner, primary_selection_set,
    };
    use crate::emacs_core::eval::Context;
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
            _ => panic!("direct body and clipboard step disagree"),
        }
    }

    #[test]
    fn serial_stubs_call_only_the_inline_arm() {
        let stubs = include_str!("stubs.rs");
        let step = include_str!("selection_step.rs");
        let driver = step.split("#[cfg(test)]").next().expect("driver");
        assert!(
            !driver.contains("std::env")
                && !driver.contains("NEOVM_THREADS")
                && !driver.contains("thread_local")
                && !driver.contains(".with(")
                && !driver.contains("OwnedRouted")
                && !driver.contains("OwnedLocal"),
            "the step driver adds no route, TLS read, or env test"
        );
        let arms = driver
            .matches("SelectionInstaller::InlineSerialMain =>")
            .count();
        assert_eq!(arms, SERIAL_CLIPBOARD_ENTRIES.len());
        for name in SERIAL_CLIPBOARD_ENTRIES {
            let marker = format!("pub fn {name}(");
            let start = stubs.find(&marker).expect(name);
            let rest = &stubs[start..];
            let end = rest[marker.len()..]
                .find("\nfn direct_")
                .expect("direct body follows the stub");
            let stub = &rest[..marker.len() + end];
            assert!(
                stub.contains("SelectionInstaller::InlineSerialMain")
                    && stub.contains(".into_direct()")
                    && !stub.contains(".with(")
                    && !stub.contains("display_host"),
                "{name} must be the inline step, not a second body"
            );
        }
    }

    #[test]
    fn inline_serial_clipboard_matches_the_direct_body() {
        let mut direct_ctx = Context::new(crate::test_utils::unit_runtime());
        let mut step_ctx = Context::new(crate::test_utils::unit_runtime());
        super::super::reset_stubs_thread_locals();
        let direct = super::super::direct_neomacs_clipboard_set(&mut direct_ctx, vec![Value::NIL]);
        super::super::reset_stubs_thread_locals();
        let via = clipboard_set(
            SelectionInstaller::InlineSerialMain,
            &mut step_ctx,
            vec![Value::NIL],
        )
        .into_direct();
        assert_same_batch(direct, via);

        super::super::reset_stubs_thread_locals();
        let direct = super::super::direct_neomacs_clipboard_get(&mut direct_ctx, vec![]);
        super::super::reset_stubs_thread_locals();
        let via = clipboard_get(SelectionInstaller::InlineSerialMain, &mut step_ctx, vec![])
            .into_direct();
        assert_same_batch(direct, via);

        super::super::reset_stubs_thread_locals();
        let direct =
            super::super::direct_neomacs_primary_selection_set(&mut direct_ctx, vec![Value::NIL]);
        super::super::reset_stubs_thread_locals();
        let via = primary_selection_set(
            SelectionInstaller::InlineSerialMain,
            &mut step_ctx,
            vec![Value::NIL],
        )
        .into_direct();
        assert_same_batch(direct, via);

        super::super::reset_stubs_thread_locals();
        let direct = super::super::direct_neomacs_primary_selection_get(&mut direct_ctx, vec![]);
        super::super::reset_stubs_thread_locals();
        let via =
            primary_selection_get(SelectionInstaller::InlineSerialMain, &mut step_ctx, vec![])
                .into_direct();
        assert_same_batch(direct, via);

        super::super::reset_stubs_thread_locals();
        let direct = super::super::direct_neomacs_primary_selection_owner(&mut direct_ctx, vec![]);
        super::super::reset_stubs_thread_locals();
        let via =
            primary_selection_owner(SelectionInstaller::InlineSerialMain, &mut step_ctx, vec![])
                .into_direct();
        assert_same_batch(direct, via);

        super::super::reset_stubs_thread_locals();
        let direct = super::super::direct_neomacs_clipboard_set(&mut direct_ctx, vec![]);
        super::super::reset_stubs_thread_locals();
        let via = clipboard_set(SelectionInstaller::InlineSerialMain, &mut step_ctx, vec![])
            .into_direct();
        assert_same_batch(direct, via);
    }

    #[test]
    fn serial_lowering_is_only_the_direct_call() {
        let step = include_str!("selection_step.rs");
        let driver = step.split("#[cfg(test)]").next().expect("driver");
        let arms = [
            (
                "fn clipboard_set(",
                "SelectionStep::Done(direct_neomacs_clipboard_set(ctx, args))",
            ),
            (
                "fn clipboard_get(",
                "SelectionStep::Done(direct_neomacs_clipboard_get(ctx, args))",
            ),
            (
                "fn primary_selection_set(",
                "SelectionStep::Done(direct_neomacs_primary_selection_set(ctx, args))",
            ),
            (
                "fn primary_selection_get(",
                "SelectionStep::Done(direct_neomacs_primary_selection_get(ctx, args))",
            ),
            (
                "fn primary_selection_owner(",
                "SelectionStep::Done(direct_neomacs_primary_selection_owner(ctx, args))",
            ),
        ];
        assert_eq!(driver.matches("match installer").count(), arms.len());
        assert_eq!(arms.len(), SERIAL_CLIPBOARD_ENTRIES.len());
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
        let stubs = include_str!("stubs.rs");
        for name in SERIAL_CLIPBOARD_ENTRIES {
            let marker = format!("pub fn {name}(");
            let start = stubs.find(&marker).expect(name);
            let rest = &stubs[start..];
            let direct_at = rest.find("\nfn direct_").expect("direct body");
            let stub = &rest[..direct_at];
            let direct_rest = &rest[direct_at..];
            let direct_close = direct_rest.find("\n}").expect("direct end");
            let direct = &direct_rest[..=direct_close + 1];
            assert!(
                stub.contains(".into_direct()")
                    && stub.contains("SelectionInstaller::InlineSerialMain")
                    && !stub.contains("expect_args"),
                "{name} lowers through the inline step"
            );
            assert!(
                direct.contains("expect_args") && !direct.contains("SelectionInstaller"),
                "{name} direct body stays the previous function"
            );
        }
    }

    #[test]
    fn inline_serial_populated_cache_matches_the_direct_body() {
        // The unit runtime has no display host, so these calls take the cache
        // branch. A symbol argument stores Some(text); NIL stores None.
        let heap = neovm_value::tagged::gc::TestHeap::new();
        let clear_heap = heap.root();
        let mut direct_ctx = Context::new(crate::test_utils::unit_runtime());
        let mut step_ctx = Context::new(crate::test_utils::unit_runtime());

        super::super::reset_stubs_thread_locals();
        let direct_set = super::super::direct_neomacs_clipboard_set(
            &mut direct_ctx,
            vec![Value::symbol("clip")],
        );
        let direct_cached = super::super::cached_clipboard_text();
        let direct_get = super::super::direct_neomacs_clipboard_get(&mut direct_ctx, vec![]);
        super::super::reset_stubs_thread_locals();
        let via_set = clipboard_set(
            SelectionInstaller::InlineSerialMain,
            &mut step_ctx,
            vec![Value::symbol("clip")],
        )
        .into_direct();
        let via_cached = super::super::cached_clipboard_text();
        let via_get = clipboard_get(SelectionInstaller::InlineSerialMain, &mut step_ctx, vec![])
            .into_direct();
        assert!(direct_cached.is_some());
        assert_eq!(direct_cached, via_cached);
        assert_same_batch(direct_set, via_set);
        assert_same_batch(direct_get, via_get);

        super::super::reset_stubs_thread_locals();
        let direct_set = super::super::direct_neomacs_primary_selection_set(
            &mut direct_ctx,
            vec![Value::symbol("prim")],
        );
        let direct_owner =
            super::super::direct_neomacs_primary_selection_owner(&mut direct_ctx, vec![]);
        let direct_cached = super::super::cached_primary_selection_text();
        let direct_get =
            super::super::direct_neomacs_primary_selection_get(&mut direct_ctx, vec![]);
        super::super::reset_stubs_thread_locals();
        let via_set = primary_selection_set(
            SelectionInstaller::InlineSerialMain,
            &mut step_ctx,
            vec![Value::symbol("prim")],
        )
        .into_direct();
        let via_owner =
            primary_selection_owner(SelectionInstaller::InlineSerialMain, &mut step_ctx, vec![])
                .into_direct();
        let via_cached = super::super::cached_primary_selection_text();
        let via_get =
            primary_selection_get(SelectionInstaller::InlineSerialMain, &mut step_ctx, vec![])
                .into_direct();
        assert!(direct_cached.is_some());
        assert_eq!(direct_cached, via_cached);
        match &via_owner {
            Ok(value) => assert_eq!(value, &Value::symbol("this-process")),
            Err(_) => panic!("populated primary cache reports this-process"),
        }
        assert_same_batch(direct_set, via_set);
        assert_same_batch(direct_owner, via_owner);
        assert_same_batch(direct_get, via_get);

        super::super::reset_stubs_thread_locals();
        let direct_set = super::super::direct_neomacs_primary_selection_set(
            &mut direct_ctx,
            vec![Value::symbol("prim")],
        );
        let direct_clear =
            super::super::direct_neomacs_primary_selection_set(&mut direct_ctx, vec![Value::NIL]);
        let direct_owner =
            super::super::direct_neomacs_primary_selection_owner(&mut direct_ctx, vec![]);
        let direct_cached = super::super::cached_primary_selection_text();
        super::super::reset_stubs_thread_locals();
        let via_set = primary_selection_set(
            SelectionInstaller::InlineSerialMain,
            &mut step_ctx,
            vec![Value::symbol("prim")],
        )
        .into_direct();
        let via_clear = primary_selection_set(
            SelectionInstaller::InlineSerialMain,
            &mut step_ctx,
            vec![Value::NIL],
        )
        .into_direct();
        let via_owner =
            primary_selection_owner(SelectionInstaller::InlineSerialMain, &mut step_ctx, vec![])
                .into_direct();
        let via_cached = super::super::cached_primary_selection_text();
        assert_eq!(direct_cached, None);
        assert_eq!(direct_cached, via_cached);
        match &via_owner {
            Ok(value) => assert_eq!(value, &Value::symbol("none")),
            Err(_) => panic!("cleared primary cache reports none"),
        }
        assert_same_batch(direct_set, via_set);
        assert_same_batch(direct_clear, via_clear);
        assert_same_batch(direct_owner, via_owner);

        neovm_value::tagged::gc::clear_tagged_heap_root_if_installed(clear_heap);
    }
}
