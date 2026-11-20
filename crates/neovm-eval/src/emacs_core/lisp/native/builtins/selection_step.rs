//! Lowering driver for the five clipboard and primary stubs.
//!
//! The owner's installed [`SelectionPhase`](crate::emacs_core::eval::activation::SelectionPhase)
//! selects the arm. `Uninstalled` is the serial direct call; `Local` is the
//! synchronous local owner turn, which is the same direct body; `Routed` is
//! the C4b worker boundary and is not installed by this slice. Each public
//! stub unwraps [`SelectionStep::Done`]. There is no TLS read or env test in
//! this module. The direct bodies keep the cache they already had.

use neovm_value::error::EvalResult;
use neovm_value::value::Value;

use super::{
    direct_neomacs_clipboard_get, direct_neomacs_clipboard_set,
    direct_neomacs_primary_selection_get, direct_neomacs_primary_selection_owner,
    direct_neomacs_primary_selection_set,
};

#[path = "selection_installer.rs"]
mod selection_installer;

pub(super) use selection_installer::SelectionInstaller;

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

/// Which lowering the owner installed. The installed phase is the single
/// source of truth, so the phase and the lowering cannot disagree.
#[inline]
pub(super) fn installed(ctx: &crate::emacs_core::eval::Context) -> SelectionInstaller {
    use crate::emacs_core::eval::activation::SelectionPhase;
    match ctx.selection_phase() {
        SelectionPhase::Uninstalled => SelectionInstaller::InlineSerialMain,
        SelectionPhase::Local => SelectionInstaller::OwnedLocalMain,
        // Only a routed owner reaches these. The routed arm is the C4b worker
        // boundary; until it lands, the driver refuses cold rather than
        // running a host call on the wrong thread.
        SelectionPhase::Routed | SelectionPhase::AwaitingRollback => {
            SelectionInstaller::OwnedRoutedWorker
        }
    }
}

/// The routed arm is installed by the C4b worker boundary, not by cut-4a.
/// Reaching it means a routed owner ran the direct path; that is a service
/// bug, and the existing service code aborts on the same class of state.
#[cold]
#[inline(never)]
fn routed_worker_boundary_not_installed() -> ! {
    std::process::abort()
}

#[inline(always)]
pub(super) fn clipboard_set(
    installer: SelectionInstaller,
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> SelectionStep<EvalResult> {
    match installer {
        // The serial and local lowerings both call the direct body: the local
        // owner turn is synchronous, so suspension is Infallible (Lead1).
        SelectionInstaller::InlineSerialMain | SelectionInstaller::OwnedLocalMain => {
            SelectionStep::Done(direct_neomacs_clipboard_set(ctx, args))
        }
        SelectionInstaller::OwnedRoutedWorker => routed_worker_boundary_not_installed(),
    }
}

#[inline(always)]
pub(super) fn clipboard_get(
    installer: SelectionInstaller,
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> SelectionStep<EvalResult> {
    match installer {
        // The serial and local lowerings both call the direct body: the local
        // owner turn is synchronous, so suspension is Infallible (Lead1).
        SelectionInstaller::InlineSerialMain | SelectionInstaller::OwnedLocalMain => {
            SelectionStep::Done(direct_neomacs_clipboard_get(ctx, args))
        }
        SelectionInstaller::OwnedRoutedWorker => routed_worker_boundary_not_installed(),
    }
}

#[inline(always)]
pub(super) fn primary_selection_set(
    installer: SelectionInstaller,
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> SelectionStep<EvalResult> {
    match installer {
        // The serial and local lowerings both call the direct body: the local
        // owner turn is synchronous, so suspension is Infallible (Lead1).
        SelectionInstaller::InlineSerialMain | SelectionInstaller::OwnedLocalMain => {
            SelectionStep::Done(direct_neomacs_primary_selection_set(ctx, args))
        }
        SelectionInstaller::OwnedRoutedWorker => routed_worker_boundary_not_installed(),
    }
}

#[inline(always)]
pub(super) fn primary_selection_get(
    installer: SelectionInstaller,
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> SelectionStep<EvalResult> {
    match installer {
        // The serial and local lowerings both call the direct body: the local
        // owner turn is synchronous, so suspension is Infallible (Lead1).
        SelectionInstaller::InlineSerialMain | SelectionInstaller::OwnedLocalMain => {
            SelectionStep::Done(direct_neomacs_primary_selection_get(ctx, args))
        }
        SelectionInstaller::OwnedRoutedWorker => routed_worker_boundary_not_installed(),
    }
}

#[inline(always)]
pub(super) fn primary_selection_owner(
    installer: SelectionInstaller,
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> SelectionStep<EvalResult> {
    match installer {
        // The serial and local lowerings both call the direct body: the local
        // owner turn is synchronous, so suspension is Infallible (Lead1).
        SelectionInstaller::InlineSerialMain | SelectionInstaller::OwnedLocalMain => {
            SelectionStep::Done(direct_neomacs_primary_selection_owner(ctx, args))
        }
        SelectionInstaller::OwnedRoutedWorker => routed_worker_boundary_not_installed(),
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
        SERIAL_CLIPBOARD_ENTRIES, SelectionInstaller, clipboard_get, clipboard_set, installed,
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
    fn installer_selector_is_derived_from_the_owner_phase() {
        use crate::emacs_core::eval::activation::ActivationAuthority;

        let fresh = Context::new(crate::test_utils::unit_runtime());
        assert_eq!(installed(&fresh), SelectionInstaller::InlineSerialMain);

        let mut local = Context::new(crate::test_utils::unit_runtime());
        local
            .activate_selection_local(&ActivationAuthority::mint_for_test())
            .unwrap();
        assert_eq!(installed(&local), SelectionInstaller::OwnedLocalMain);

        let mut routed = Context::new(crate::test_utils::unit_runtime());
        routed
            .activate_selection_routed(&ActivationAuthority::mint_for_test())
            .unwrap();
        assert_eq!(installed(&routed), SelectionInstaller::OwnedRoutedWorker);
    }

    #[test]
    fn stubs_lower_through_the_installed_selector() {
        let stubs = include_str!("stubs.rs");
        let step = include_str!("selection_step.rs");
        let driver = step.split("#[cfg(test)]").next().expect("driver");
        assert!(
            !driver.contains("std::env")
                && !driver.contains("NEOVM_THREADS")
                && !driver.contains("thread_local")
                && !driver.contains(".with("),
            "the step driver adds no TLS read or env test"
        );
        for name in SERIAL_CLIPBOARD_ENTRIES {
            let marker = format!("pub fn {name}(");
            let start = stubs.find(&marker).expect(name);
            let rest = &stubs[start..];
            let end = rest[marker.len()..]
                .find("\nfn direct_")
                .expect("direct body follows the stub");
            let stub = &rest[..marker.len() + end];
            assert!(
                stub.contains("selection_step::installed(ctx)")
                    && stub.contains(".into_direct()")
                    && !stub.contains(".with(")
                    && !stub.contains("display_host")
                    && !stub.contains("SelectionInstaller::"),
                "{name} must read the installed lowering, not a constant or a second body"
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
            // Two arms: serial|local lowers to the direct call, routed aborts
            // cold until the C4b worker boundary installs it.
            assert_eq!(function.matches("=>").count(), 2, "{signature}");
            assert!(
                function.contains(call)
                    && function
                        .contains("OwnedRoutedWorker => routed_worker_boundary_not_installed()")
                    && !function.contains("if "),
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
                    && stub.contains("selection_step::installed(ctx)")
                    && !stub.contains("SelectionInstaller::")
                    && !stub.contains("expect_args"),
                "{name} lowers through the installed step"
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

    /// One selection program. The installed serial arm stays the direct call.
    /// This lowering is the other one, and only the harness below constructs it.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum SelectionOp {
        ClipboardSet,
        ClipboardGet,
        PrimarySet,
        PrimaryGet,
        PrimaryOwner,
    }

    enum OwnedLowering {
        NeedOwner {
            op: SelectionOp,
            args: Vec<Value>,
        },
        NativeWait {
            op: SelectionOp,
            pending: EvalResult,
        },
        Done(EvalResult),
    }

    fn owned_begin(op: SelectionOp, args: Vec<Value>) -> OwnedLowering {
        OwnedLowering::NeedOwner { op, args }
    }

    /// NeedOwner handoff. `pending` is the harness owner's result.
    /// Replacing `result` here is the red mutation for this lowering.
    fn owned_accept_owner_result(op: SelectionOp, result: EvalResult) -> OwnedLowering {
        OwnedLowering::NativeWait {
            op,
            pending: result,
        }
    }

    fn owned_resume(step: OwnedLowering) -> OwnedLowering {
        match step {
            OwnedLowering::NativeWait { pending, .. } => OwnedLowering::Done(pending),
            other => other,
        }
    }

    fn perform_on_host(op: SelectionOp, ctx: &mut Context, args: Vec<Value>) -> EvalResult {
        match op {
            SelectionOp::ClipboardSet => super::super::direct_neomacs_clipboard_set(ctx, args),
            SelectionOp::ClipboardGet => super::super::direct_neomacs_clipboard_get(ctx, args),
            SelectionOp::PrimarySet => {
                super::super::direct_neomacs_primary_selection_set(ctx, args)
            }
            SelectionOp::PrimaryGet => {
                super::super::direct_neomacs_primary_selection_get(ctx, args)
            }
            SelectionOp::PrimaryOwner => {
                super::super::direct_neomacs_primary_selection_owner(ctx, args)
            }
        }
    }

    fn serial_lower(op: SelectionOp, ctx: &mut Context, args: Vec<Value>) -> EvalResult {
        match op {
            SelectionOp::ClipboardSet => {
                clipboard_set(SelectionInstaller::InlineSerialMain, ctx, args).into_direct()
            }
            SelectionOp::ClipboardGet => {
                clipboard_get(SelectionInstaller::InlineSerialMain, ctx, args).into_direct()
            }
            SelectionOp::PrimarySet => {
                primary_selection_set(SelectionInstaller::InlineSerialMain, ctx, args).into_direct()
            }
            SelectionOp::PrimaryGet => {
                primary_selection_get(SelectionInstaller::InlineSerialMain, ctx, args).into_direct()
            }
            SelectionOp::PrimaryOwner => {
                primary_selection_owner(SelectionInstaller::InlineSerialMain, ctx, args)
                    .into_direct()
            }
        }
    }

    fn drive_owned(op: SelectionOp, ctx: &mut Context, args: Vec<Value>) -> EvalResult {
        let (op, args) = match owned_begin(op, args) {
            OwnedLowering::NeedOwner { op, args } => (op, args),
            OwnedLowering::NativeWait { .. } | OwnedLowering::Done(_) => {
                panic!("owned program begins at NeedOwner");
            }
        };
        let result = perform_on_host(op, ctx, args);
        let pending = match owned_accept_owner_result(op, result) {
            OwnedLowering::NativeWait {
                op: waited,
                pending,
            } => {
                assert_eq!(waited, op);
                pending
            }
            OwnedLowering::NeedOwner { .. } | OwnedLowering::Done(_) => {
                panic!("NeedOwner handoff suspends in NativeWait");
            }
        };
        match owned_resume(OwnedLowering::NativeWait { op, pending }) {
            OwnedLowering::Done(done) => done,
            OwnedLowering::NeedOwner { .. } | OwnedLowering::NativeWait { .. } => {
                panic!("NativeWait resumes to Done");
            }
        }
    }

    fn prime_clipboard(ctx: &mut Context) {
        match super::super::direct_neomacs_clipboard_set(ctx, vec![Value::symbol("clip")]) {
            Ok(_) => {}
            Err(_) => panic!("prime clipboard"),
        }
    }

    fn prime_primary(ctx: &mut Context) {
        match super::super::direct_neomacs_primary_selection_set(ctx, vec![Value::symbol("prim")]) {
            Ok(_) => {}
            Err(_) => panic!("prime primary"),
        }
    }

    fn run_owned_against_serial(op: SelectionOp, args: Vec<Value>, prime: fn(&mut Context)) {
        let mut serial_ctx = Context::new(crate::test_utils::unit_runtime());
        let mut owned_ctx = Context::new(crate::test_utils::unit_runtime());
        super::super::reset_stubs_thread_locals();
        prime(&mut serial_ctx);
        let serial = serial_lower(op, &mut serial_ctx, args.clone());
        super::super::reset_stubs_thread_locals();
        prime(&mut owned_ctx);
        let owned = drive_owned(op, &mut owned_ctx, args);
        match &serial {
            Ok(value) => assert_ne!(value, &Value::T, "{op:?} serial result must not be t"),
            Err(_) => panic!("{op:?} serial lowering returns a value"),
        }
        assert_same_batch(serial, owned);
    }

    fn prime_nothing(_ctx: &mut Context) {}

    #[test]
    fn owned_lowering_matches_the_serial_lowering() {
        let heap = neovm_value::tagged::gc::TestHeap::new();
        let clear_heap = heap.root();
        run_owned_against_serial(
            SelectionOp::ClipboardSet,
            vec![Value::symbol("clip")],
            prime_nothing,
        );
        run_owned_against_serial(SelectionOp::ClipboardGet, vec![], prime_clipboard);
        run_owned_against_serial(
            SelectionOp::PrimarySet,
            vec![Value::symbol("prim")],
            prime_nothing,
        );
        run_owned_against_serial(SelectionOp::PrimaryGet, vec![], prime_primary);
        run_owned_against_serial(SelectionOp::PrimaryOwner, vec![], prime_primary);
        neovm_value::tagged::gc::clear_tagged_heap_root_if_installed(clear_heap);
    }
}
