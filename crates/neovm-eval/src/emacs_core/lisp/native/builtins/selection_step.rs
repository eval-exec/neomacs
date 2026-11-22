//! Lowering driver for the five clipboard and primary stubs.
//!
//! The owner's installed [`SelectionPhase`](crate::emacs_core::eval::activation::SelectionPhase)
//! selects the arm. `Uninstalled` is the serial direct call, except that a
//! non-main Green owner in a hosted session routes; `Local` is the
//! synchronous local owner turn, which is the same direct body; `Routed` is
//! the C4b worker boundary, where arity, conversion and the Lisp result stay
//! on the caller and only a typed `SelectionRequest` crosses. Each public
//! stub unwraps [`SelectionStep::Done`]. There is no TLS read or env test in
//! this module. The direct bodies keep the cache they already had.

use neovm_value::error::{EvalResult, Flow, expect_args, signal};
use neovm_value::value::{Value, ValueKind};

use crate::host_effect::selection::{SelectionOwnerReply, SelectionReply, SelectionRequest};

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

/// Which lowering the owner installed. The phase is the per-owner source of
/// truth; a non-main Green owner never installs one, so a hosted session is
/// the World-level fact that routes it. Phase and lowering cannot disagree:
/// both are derived from the same session state.
#[inline]
pub(super) fn installed(ctx: &crate::emacs_core::eval::Context) -> SelectionInstaller {
    use crate::emacs_core::eval::activation::SelectionPhase;
    match ctx.selection_phase() {
        SelectionPhase::Local => SelectionInstaller::OwnedLocalMain,
        SelectionPhase::Routed | SelectionPhase::AwaitingRollback => {
            SelectionInstaller::OwnedRoutedWorker
        }
        // A worker has no per-owner activation; only a hosted World routes
        // it. Unhosted Green keeps the thread-local cache, exactly as before.
        SelectionPhase::Uninstalled => {
            if ctx.is_green_worker_owner() && ctx.in_hosted_green_session() {
                SelectionInstaller::OwnedRoutedWorker
            } else {
                SelectionInstaller::InlineSerialMain
            }
        }
    }
}

/// A host error crosses back as the direct body's exact signal.
#[cold]
fn host_error(message: String) -> Flow {
    signal("error", vec![Value::string(message)])
}

/// A reply of the wrong family is a service invariant violation, never a
/// Lisp-visible result.
#[cold]
fn reply_mismatch(reply: &SelectionReply) -> Flow {
    debug_assert!(
        false,
        "selection reply does not match its request: {reply:?}"
    );
    tracing::error!(?reply, "selection reply does not match its request");
    signal(
        "error",
        vec![Value::string(
            "neovm internal error: selection reply does not match its request",
        )],
    )
}

/// Set-argument semantics, identical to `direct_neomacs_*_set`: nil clears,
/// a string is converted lossily, anything else is printed on this caller.
fn selection_text(name: &'static str, args: &[Value]) -> Result<Option<String>, Flow> {
    expect_args(name, args, 1)?;
    Ok(match args[0].kind() {
        ValueKind::Nil => None,
        ValueKind::String => args[0]
            .as_lisp_string()
            .map(|ls| neovm_value::emacs_char::to_utf8_lossy(ls.as_bytes())),
        _ => Some(format!("{}", args[0])),
    })
}

#[cold]
fn routed_set(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
    name: &'static str,
    request: impl FnOnce(Option<String>) -> SelectionRequest,
) -> EvalResult {
    let text = selection_text(name, &args)?;
    match ctx.green_host_selection(request(text)) {
        Ok(SelectionReply::Set) => Ok(Value::NIL),
        Ok(SelectionReply::HostError(message)) => Err(host_error(message)),
        Ok(other) => Err(reply_mismatch(&other)),
        Err(flow) => Err(flow),
    }
}

#[cold]
fn routed_text(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
    name: &'static str,
    clipboard: bool,
    request: SelectionRequest,
) -> EvalResult {
    expect_args(name, &args, 0)?;
    match ctx.green_host_selection(request) {
        Ok(SelectionReply::Clipboard(text)) if clipboard => {
            Ok(text.map(Value::string).unwrap_or(Value::NIL))
        }
        Ok(SelectionReply::Primary(text)) if !clipboard => {
            Ok(text.map(Value::string).unwrap_or(Value::NIL))
        }
        Ok(SelectionReply::HostError(message)) => Err(host_error(message)),
        Ok(other) => Err(reply_mismatch(&other)),
        Err(flow) => Err(flow),
    }
}

#[cold]
fn routed_owner(ctx: &mut crate::emacs_core::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("neomacs-primary-selection-owner", &args, 0)?;
    match ctx.green_host_selection(SelectionRequest::PrimaryOwner) {
        Ok(SelectionReply::Owner(owner)) => Ok(Value::symbol(match owner {
            SelectionOwnerReply::ThisProcess => "this-process",
            SelectionOwnerReply::OtherProcess => "other-process",
            SelectionOwnerReply::None => "none",
            SelectionOwnerReply::Unknown => "unknown",
        })),
        Ok(SelectionReply::HostError(message)) => Err(host_error(message)),
        Ok(other) => Err(reply_mismatch(&other)),
        Err(flow) => Err(flow),
    }
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
        SelectionInstaller::OwnedRoutedWorker => SelectionStep::Done(routed_set(
            ctx,
            args,
            "neomacs-clipboard-set",
            SelectionRequest::ClipboardSet,
        )),
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
        SelectionInstaller::OwnedRoutedWorker => SelectionStep::Done(routed_text(
            ctx,
            args,
            "neomacs-clipboard-get",
            true,
            SelectionRequest::ClipboardGet,
        )),
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
        SelectionInstaller::OwnedRoutedWorker => SelectionStep::Done(routed_set(
            ctx,
            args,
            "neomacs-primary-selection-set",
            SelectionRequest::PrimarySet,
        )),
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
        SelectionInstaller::OwnedRoutedWorker => SelectionStep::Done(routed_text(
            ctx,
            args,
            "neomacs-primary-selection-get",
            false,
            SelectionRequest::PrimaryGet,
        )),
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
        SelectionInstaller::OwnedRoutedWorker => SelectionStep::Done(routed_owner(ctx, args)),
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
                "SelectionStep::Done(routed_set(",
            ),
            (
                "fn clipboard_get(",
                "SelectionStep::Done(direct_neomacs_clipboard_get(ctx, args))",
                "SelectionStep::Done(routed_text(",
            ),
            (
                "fn primary_selection_set(",
                "SelectionStep::Done(direct_neomacs_primary_selection_set(ctx, args))",
                "SelectionStep::Done(routed_set(",
            ),
            (
                "fn primary_selection_get(",
                "SelectionStep::Done(direct_neomacs_primary_selection_get(ctx, args))",
                "SelectionStep::Done(routed_text(",
            ),
            (
                "fn primary_selection_owner(",
                "SelectionStep::Done(direct_neomacs_primary_selection_owner(ctx, args))",
                "SelectionStep::Done(routed_owner(",
            ),
        ];
        assert_eq!(driver.matches("match installer").count(), arms.len());
        assert_eq!(arms.len(), SERIAL_CLIPBOARD_ENTRIES.len());
        for (signature, call, routed) in arms {
            let start = driver.find(signature).expect(signature);
            let rest = &driver[start..];
            let close = rest.find("\n}").expect("function end");
            let function = &rest[..=close + 1];
            // Two arms: serial|local lowers to the direct call, routed to the
            // typed host-request helper. No cold abort remains.
            assert_eq!(function.matches("=>").count(), 2, "{signature}");
            assert!(
                function.contains(call)
                    && function.contains(routed)
                    && !function.contains("routed_worker_boundary_not_installed")
                    && !function.contains("if "),
                "{signature} must lower to {call} and route through {routed}"
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

    #[derive(Default)]
    struct RecordingClipboardHost {
        clipboard: Option<String>,
        primary: Option<String>,
    }

    impl crate::emacs_core::eval::protocol::DisplayHost for RecordingClipboardHost {
        fn realize_gui_frame(
            &mut self,
            _request: crate::emacs_core::eval::GuiFrameHostRequest,
        ) -> Result<(), String> {
            Ok(())
        }

        fn resize_gui_frame(
            &mut self,
            _request: crate::emacs_core::eval::GuiFrameHostRequest,
        ) -> Result<(), String> {
            Ok(())
        }

        fn set_clipboard_text(&mut self, text: Option<&str>) -> Result<(), String> {
            self.clipboard = text.map(str::to_owned);
            Ok(())
        }

        fn clipboard_text(&mut self) -> Result<Option<String>, String> {
            Ok(self.clipboard.clone())
        }

        fn set_primary_selection_text(&mut self, text: Option<&str>) -> Result<(), String> {
            self.primary = text.map(str::to_owned);
            Ok(())
        }

        fn primary_selection_text(&mut self) -> Result<Option<String>, String> {
            Ok(self.primary.clone())
        }

        fn primary_selection_owner(
            &mut self,
        ) -> Result<neomacs_display_protocol::SelectionOwner, String> {
            Ok(if self.primary.is_some() {
                neomacs_display_protocol::SelectionOwner::ThisProcess
            } else {
                neomacs_display_protocol::SelectionOwner::None
            })
        }
    }

    /// The flip's production wiring: a Green hosted Context activates Local
    /// from `set_display_host`, and the stubs then reach the attached host.
    /// Removing the `activate_host_session` call from `set_display_host` is
    /// the mutation that makes this test red.
    #[test]
    fn set_display_host_activates_the_local_arm_and_reaches_the_host() {
        use crate::emacs_core::eval::activation::SelectionPhase;
        use crate::emacs_core::eval::protocol::DisplayHost;

        // Green hosted Context: set_display_host is the flip's production
        // entry point. Removing its activate_host_session call makes this red.
        let mut ctx = Context::new(crate::test_utils::unit_runtime());
        ctx.set_green_threading_mode_at_construction_for_test();
        assert_eq!(ctx.selection_phase(), SelectionPhase::Uninstalled);
        ctx.set_display_host(Box::new(RecordingClipboardHost::default()));
        assert_eq!(ctx.selection_phase(), SelectionPhase::Local);
        assert_eq!(installed(&ctx), SelectionInstaller::OwnedLocalMain);

        let set =
            super::super::builtin_neomacs_clipboard_set(&mut ctx, vec![Value::string("wire")]);
        let got = super::super::builtin_neomacs_clipboard_get(&mut ctx, vec![]);
        assert!(set.is_ok(), "the installed arm performs the clipboard set");
        match &got {
            Ok(value) => assert_eq!(value, &Value::string("wire"), "the host holds the text"),
            Err(_) => panic!("the installed arm reads the attached host"),
        }

        // The same calls on a serial Context with an identical host return the
        // same values; the direct bodies are the serial lowering.
        let mut serial = Context::new(crate::test_utils::unit_runtime());
        serial.set_display_host(Box::new(RecordingClipboardHost::default()));
        let serial_set =
            super::super::builtin_neomacs_clipboard_set(&mut serial, vec![Value::string("wire")]);
        let serial_get = super::super::builtin_neomacs_clipboard_get(&mut serial, vec![]);
        assert_same_batch(set, serial_set);
        assert_same_batch(got, serial_get);

        // Default mode must not activate. The one-authority-stays-untaken
        // assertion lives in activation_owner::tests::
        // default_mode_hosted_stays_uninstalled, where the eval-scoped take
        // accessor is nameable.
        let mut default_mode = Context::new(crate::test_utils::unit_runtime());
        default_mode.set_display_host(Box::new(RecordingClipboardHost::default()));
        assert_eq!(default_mode.selection_phase(), SelectionPhase::Uninstalled);
    }

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

/// Cut-4b red rows. A hosted Green worker must reach main's display host;
/// today `installed` returns `InlineSerialMain` for every worker (its
/// per-owner phase is `Uninstalled`), the worker finds no `display_host`, and
/// the direct body writes the worker's TLS cache instead. These rows fail on
/// that tip and pass once the routed boundary is installed.
#[cfg(test)]
mod cut4b {
    use super::super::{
        builtin_neomacs_clipboard_get, builtin_neomacs_clipboard_set,
        builtin_neomacs_primary_selection_get, builtin_neomacs_primary_selection_owner,
        builtin_neomacs_primary_selection_set,
    };
    use crate::emacs_core::eval::{Context, ThreadingMode};
    use crate::emacs_core::print::print_value;
    use neomacs_display_protocol::SelectionOwner;
    use neovm_value::value::Value;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Default)]
    struct ClipboardState {
        clipboard: Option<String>,
        primary: Option<String>,
        owner: Option<SelectionOwner>,
        fail: Option<String>,
    }

    #[derive(Clone, Default)]
    struct SharedClipboardHost {
        state: Rc<RefCell<ClipboardState>>,
    }

    impl crate::emacs_core::eval::protocol::DisplayHost for SharedClipboardHost {
        fn realize_gui_frame(
            &mut self,
            _request: crate::emacs_core::eval::GuiFrameHostRequest,
        ) -> Result<(), String> {
            Ok(())
        }

        fn resize_gui_frame(
            &mut self,
            _request: crate::emacs_core::eval::GuiFrameHostRequest,
        ) -> Result<(), String> {
            Ok(())
        }

        fn set_clipboard_text(&mut self, text: Option<&str>) -> Result<(), String> {
            if let Some(error) = self.state.borrow().fail.clone() {
                return Err(error);
            }
            self.state.borrow_mut().clipboard = text.map(str::to_owned);
            Ok(())
        }

        fn clipboard_text(&mut self) -> Result<Option<String>, String> {
            if let Some(error) = self.state.borrow().fail.clone() {
                return Err(error);
            }
            Ok(self.state.borrow().clipboard.clone())
        }

        fn set_primary_selection_text(&mut self, text: Option<&str>) -> Result<(), String> {
            if let Some(error) = self.state.borrow().fail.clone() {
                return Err(error);
            }
            self.state.borrow_mut().primary = text.map(str::to_owned);
            Ok(())
        }

        fn primary_selection_text(&mut self) -> Result<Option<String>, String> {
            if let Some(error) = self.state.borrow().fail.clone() {
                return Err(error);
            }
            Ok(self.state.borrow().primary.clone())
        }

        fn primary_selection_owner(&mut self) -> Result<SelectionOwner, String> {
            if let Some(error) = self.state.borrow().fail.clone() {
                return Err(error);
            }
            Ok(self.state.borrow().owner.unwrap_or(SelectionOwner::None))
        }
    }

    /// A real Green runtime, then a host attached (so the C4c refusal is not
    /// hit), then Lisp runs as main. The worker is a real owner carrier.
    fn hosted_green_context() -> (Context, Rc<RefCell<ClipboardState>>) {
        let mut ctx =
            Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
        ctx.start_green_threads().expect("green runtime starts");
        let state = Rc::new(RefCell::new(ClipboardState::default()));
        ctx.set_display_host(Box::new(SharedClipboardHost {
            state: Rc::clone(&state),
        }));
        (ctx, state)
    }

    fn worker(form: &str) -> String {
        format!("(thread-join (make-thread (lambda () {form})))")
    }

    fn serial_context() -> (Context, Rc<RefCell<ClipboardState>>) {
        let mut ctx = Context::new(crate::test_utils::unit_runtime());
        let state = Rc::new(RefCell::new(ClipboardState::default()));
        ctx.set_display_host(Box::new(SharedClipboardHost {
            state: Rc::clone(&state),
        }));
        (ctx, state)
    }

    /// Print one case's outcome and the host state it left, so the serial and
    /// routed lowerings are compared on both the Lisp answer and the effect.
    fn case_result(
        ctx: &mut Context,
        state: &Rc<RefCell<ClipboardState>>,
        expr: &str,
        fail: Option<&str>,
        owner: Option<SelectionOwner>,
        routed: bool,
    ) -> String {
        *state.borrow_mut() = ClipboardState {
            fail: fail.map(str::to_owned),
            owner,
            ..ClipboardState::default()
        };
        let guarded = format!("(condition-case e {expr} (error (list 'caught (car e) (cdr e))))");
        let form = if routed { worker(&guarded) } else { guarded };
        let out = ctx.eval_str(&form).expect("selection case evaluates");
        let state = state.borrow();
        format!(
            "{}|{:?}|{:?}|{:?}",
            print_value(&out),
            state.clipboard,
            state.primary,
            state.owner.map(|owner| format!("{owner:?}")),
        )
    }

    #[test]
    fn routed_worker_set_reaches_the_main_host() {
        let (mut ctx, state) = hosted_green_context();
        let out = ctx
            .eval_str(&worker(r#"(neomacs-clipboard-set "routed")"#))
            .expect("worker set");
        assert_eq!(out, Value::NIL);
        assert_eq!(
            state.borrow().clipboard.as_deref(),
            Some("routed"),
            "the main host must receive the worker's clipboard set"
        );
    }

    #[test]
    fn routed_worker_get_reads_the_main_host() {
        let (mut ctx, state) = hosted_green_context();
        ctx.eval_str(r#"(neomacs-clipboard-set "p")"#)
            .expect("prime the host");
        assert_eq!(state.borrow().clipboard.as_deref(), Some("p"));
        let out = ctx
            .eval_str(&worker("(neomacs-clipboard-get)"))
            .expect("worker get");
        assert_eq!(out, Value::string("p"));
    }

    #[test]
    fn routed_worker_primary_reaches_the_main_host() {
        let (mut ctx, state) = hosted_green_context();
        ctx.eval_str(r#"(neomacs-primary-selection-set "prim")"#)
            .expect("prime primary");
        assert_eq!(state.borrow().primary.as_deref(), Some("prim"));
        let out = ctx
            .eval_str(&worker("(neomacs-primary-selection-get)"))
            .expect("worker primary get");
        assert_eq!(out, Value::string("prim"));
    }

    #[test]
    fn routed_worker_owner_reports_the_main_host_owner() {
        let (mut ctx, state) = hosted_green_context();
        for (owner, symbol) in [
            (SelectionOwner::ThisProcess, "this-process"),
            (SelectionOwner::OtherProcess, "other-process"),
            (SelectionOwner::None, "none"),
            (SelectionOwner::Unknown, "unknown"),
        ] {
            state.borrow_mut().owner = Some(owner);
            let out = ctx
                .eval_str(&worker("(neomacs-primary-selection-owner)"))
                .expect("worker owner");
            assert_eq!(out, Value::symbol(symbol));
        }
    }

    #[test]
    fn routed_worker_host_error_is_the_direct_body_signal() {
        let (mut ctx, state) = hosted_green_context();
        state.borrow_mut().fail = Some("boom".to_string());
        let form = worker(
            r#"(condition-case e (neomacs-clipboard-set "x") (error (list 'caught (car e) (cdr e))))"#,
        );
        let out = ctx.eval_str(&form).expect("worker error");
        assert_eq!(print_value(&out), "(caught error (\"boom\"))");
    }

    #[test]
    fn serial_main_host_calls_still_work_in_the_same_session() {
        let (mut ctx, state) = hosted_green_context();
        builtin_neomacs_clipboard_set(&mut ctx, vec![Value::string("main")]).unwrap();
        assert_eq!(state.borrow().clipboard.as_deref(), Some("main"));
        let got = builtin_neomacs_clipboard_get(&mut ctx, vec![]).unwrap();
        assert_eq!(got, Value::string("main"));
        builtin_neomacs_primary_selection_set(&mut ctx, vec![Value::string("pmain")]).unwrap();
        assert_eq!(state.borrow().primary.as_deref(), Some("pmain"));
        let got = builtin_neomacs_primary_selection_get(&mut ctx, vec![]).unwrap();
        assert_eq!(got, Value::string("pmain"));
        let owner = builtin_neomacs_primary_selection_owner(&mut ctx, vec![]).unwrap();
        assert_eq!(print_value(&owner), "none");
    }

    #[test]
    fn selector_matrix_on_real_owners() {
        use super::{SelectionInstaller, installed};
        use crate::emacs_core::eval::activation::ActivationAuthority;

        // Build every non-Green Context before the Green runtime starts: a
        // Context constructed afterwards would register subrs after the
        // runtime captured the tables (bootstrap's debug assertion).
        let serial = Context::new(crate::test_utils::unit_runtime());
        assert_eq!(installed(&serial), SelectionInstaller::InlineSerialMain);

        let mut routed = Context::new(crate::test_utils::unit_runtime());
        routed
            .activate_selection_routed(&ActivationAuthority::mint_for_test())
            .unwrap();
        assert_eq!(installed(&routed), SelectionInstaller::OwnedRoutedWorker);

        let (hosted, _state) = hosted_green_context();
        assert_eq!(installed(&hosted), SelectionInstaller::OwnedLocalMain);
    }

    #[test]
    fn unhosted_green_worker_keeps_the_thread_local_cache() {
        let mut ctx =
            Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
        ctx.start_green_threads().expect("green runtime starts");
        let out = ctx
            .eval_str(&worker(
                r#"(progn (neomacs-clipboard-set "cache") (neomacs-clipboard-get))"#,
            ))
            .expect("worker cache round-trip");
        assert_eq!(out, Value::string("cache"));
        // No host was attached: the worker used its own cache and the World
        // is not marked hosted. The serial default is unchanged.
        assert!(!ctx.in_hosted_green_session());
    }

    /// GNU's x-own-selection-internal does not release the global lock, so no
    /// other Lisp thread may run between the routed request and its reply.
    /// Worker B is runnable from creation; if the service hop yields to it,
    /// A reads 2 instead of 1. Run many interleavings.
    #[test]
    fn routed_selection_adds_no_worker_scheduling_point() {
        let (mut ctx, _state) = hosted_green_context();
        let form = r#"
(progn
  (setq c4b-atomic-x 0)
  ;; A is queued first, so it runs first while B waits in the ready queue.
  ;; If the routed hop yields, B runs between A's setq and A's read.
  (let ((a (make-thread (lambda ()
                          (setq c4b-atomic-x 1)
                          (neomacs-clipboard-set "atomic")
                          c4b-atomic-x))))
    (let ((b (make-thread (lambda () (setq c4b-atomic-x 2)))))
      (let ((result (thread-join a)))
        (thread-join b)
        result))))
"#;
        for iteration in 0..50 {
            let out = ctx.eval_str(form).expect("atomicity iteration");
            assert_eq!(
                out,
                Value::fixnum(1),
                "another Lisp thread ran inside the routed set (iteration {iteration})"
            );
        }
    }

    #[test]
    fn routed_and_serial_lowerings_agree_on_every_entry() {
        use crate::host_effect::selection::SelectionRequest;

        let (mut serial, serial_state) = serial_context();
        let (mut routed, routed_state) = hosted_green_context();

        let mut cases: Vec<(String, Option<&str>, Option<SelectionOwner>)> = Vec::new();
        for input in ["nil", "\"ascii\"", "\"λμ\"", "'sym", "42", "'(a b)"] {
            cases.push((format!("(neomacs-clipboard-set {input})"), None, None));
            cases.push((
                format!("(neomacs-primary-selection-set {input})"),
                None,
                None,
            ));
        }
        for expr in [
            "(neomacs-clipboard-get)",
            "(neomacs-primary-selection-get)",
            "(progn (neomacs-clipboard-set \"seeded\") (neomacs-clipboard-get))",
            "(progn (neomacs-primary-selection-set \"pseeded\") (neomacs-primary-selection-get))",
            "(neomacs-clipboard-get 1)",
            "(neomacs-primary-selection-get 1)",
            "(neomacs-primary-selection-owner)",
            "(neomacs-primary-selection-owner 1)",
        ] {
            cases.push((expr.to_string(), None, Some(SelectionOwner::ThisProcess)));
        }
        for expr in [
            "(neomacs-clipboard-set \"x\")",
            "(neomacs-primary-selection-set \"x\")",
            "(neomacs-clipboard-get)",
            "(neomacs-primary-selection-get)",
            "(neomacs-primary-selection-owner)",
        ] {
            cases.push((
                expr.to_string(),
                Some("boom"),
                Some(SelectionOwner::ThisProcess),
            ));
        }

        for (expr, fail, owner) in cases {
            let left = case_result(&mut serial, &serial_state, &expr, fail, owner, false);
            let right = case_result(&mut routed, &routed_state, &expr, fail, owner, true);
            assert_eq!(left, right, "serial and routed disagree on {expr}");
        }

        // The variant list is total: a sixth entry fails to compile here.
        fn describe(request: SelectionRequest) -> &'static str {
            match request {
                SelectionRequest::ClipboardSet(_) => "clipboard-set",
                SelectionRequest::ClipboardGet => "clipboard-get",
                SelectionRequest::PrimarySet(_) => "primary-set",
                SelectionRequest::PrimaryGet => "primary-get",
                SelectionRequest::PrimaryOwner => "primary-owner",
            }
        }
        let all = [
            SelectionRequest::ClipboardSet(None),
            SelectionRequest::ClipboardGet,
            SelectionRequest::PrimarySet(None),
            SelectionRequest::PrimaryGet,
            SelectionRequest::PrimaryOwner,
        ];
        let described: Vec<_> = all.into_iter().map(describe).collect();
        assert_eq!(
            described,
            vec![
                "clipboard-set",
                "clipboard-get",
                "primary-set",
                "primary-get",
                "primary-owner"
            ]
        );
        assert_eq!(super::SERIAL_CLIPBOARD_ENTRIES.len(), described.len());
    }
}
