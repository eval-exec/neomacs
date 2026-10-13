//! GNU-compatible Lisp boundary for the inotify adapter.

use super::super::super::*;
use super::InotifyRequest;
use crate::emacs_core::errno::Errno;
use crate::emacs_core::error::expect_args;
use crate::tagged::SymbolMode;

// Both helpers below feed GNU's `report_file_notify_error` (`src/inotify.c`),
// which reads the *global* `errno` at the time it runs rather than taking one
// as an argument.  The errnos here are named rather than their text being
// spelled out, because these are exactly the values GNU's argument shapes are
// observed to leave behind (see the oracle expectation for `inotify-rm-watch`).
fn unknown_aspect_error(aspect: Value) -> Flow {
    file_notify_error(
        "Unknown aspect",
        Some(ErrorDetail::Os(Errno::new(libc::EINVAL))),
        Some(aspect),
    )
}

/// `src/inotify.c:502`.  A cons is rejected after GNU has already set `EINVAL`
/// probing the descriptor's shape; a non-cons leaves `ENOENT`.
fn invalid_descriptor_error(descriptor: Value, detail: Errno) -> Flow {
    file_notify_error(
        "Invalid descriptor ",
        Some(ErrorDetail::Os(detail)),
        Some(descriptor),
    )
}

fn aspect_symbol_valid(name: &str) -> bool {
    matches!(
        name,
        "access"
            | "attrib"
            | "close-write"
            | "close-nowrite"
            | "create"
            | "delete"
            | "delete-self"
            | "modify"
            | "move-self"
            | "moved-from"
            | "moved-to"
            | "open"
            | "move"
            | "close"
            | "dont-follow"
            | "onlydir"
            | "ignored"
            | "unmount"
            | "all-events"
            | "t"
    )
}

fn parse_aspects(aspect: Value, symbol_mode: SymbolMode) -> Result<Vec<String>, Flow> {
    if aspect.is_nil() {
        return Ok(Vec::new());
    }
    if let Some(name) = aspect.as_symbol_name(symbol_mode) {
        return if aspect_symbol_valid(name) {
            Ok(vec![name.to_owned()])
        } else {
            Err(unknown_aspect_error(aspect))
        };
    }
    if !aspect.is_cons() {
        return Err(unknown_aspect_error(aspect));
    }

    let mut names = Vec::new();
    let mut rest = aspect;
    while rest.is_cons() {
        let item = rest.cons_car();
        let Some(name) = item.as_symbol_name(symbol_mode) else {
            return Err(unknown_aspect_error(item));
        };
        if !aspect_symbol_valid(name) {
            return Err(unknown_aspect_error(item));
        }
        names.push(name.to_owned());
        rest = rest.cons_cdr();
    }
    if !rest.is_nil() {
        return Err(unknown_aspect_error(rest));
    }
    Ok(names)
}

fn extract_watch_id(value: Value) -> Option<WatchId> {
    if !value.is_cons() {
        return None;
    }
    let slot = value.cons_car().as_int()?;
    let generation = value.cons_cdr().as_int()?;
    (slot >= 0 && generation >= 0).then(|| WatchId::new(slot, generation))
}

pub(crate) fn inotify_valid_p(args: Vec<Value>) -> EvalResult {
    expect_args("inotify-valid-p", &args, 1)?;
    let Some(watch_id) = extract_watch_id(args[0]) else {
        return Ok(Value::NIL);
    };
    FILE_NOTIFY_STATE.with(|slot| {
        let state = slot.borrow();
        Ok(Value::bool_val(state.backend.valid_p(&watch_id)))
    })
}

pub(crate) fn inotify_add_watch(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("inotify-add-watch", &args, 3)?;
    let registered_file_name = args[0];
    let path = crate::emacs_core::fileio::lisp_file_name_to_path_buf(
        ctx.expect_lisp_string(registered_file_name)?,
    );
    let aspects = parse_aspects(args[1], ctx.symbol_mode())?;
    let callback = args[2];
    let notifier = ctx.wait_notifier();

    FILE_NOTIFY_STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        let watch_id = state
            .backend
            .add_watch(&path, InotifyRequest::new(aspects), notifier)?;
        state
            .registry
            .register(watch_id.clone(), callback, registered_file_name);
        Ok(watch_id.to_inotify_lisp())
    })
}

pub(crate) fn inotify_rm_watch(args: Vec<Value>) -> EvalResult {
    expect_args("inotify-rm-watch", &args, 1)?;
    let detail = if args[0].is_cons() {
        Errno::new(libc::EINVAL)
    } else {
        Errno::new(libc::ENOENT)
    };
    let Some(watch_id) = extract_watch_id(args[0]) else {
        return Err(invalid_descriptor_error(args[0], detail));
    };

    FILE_NOTIFY_STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        match state.backend.remove_watch(&watch_id) {
            RemoveWatchOutcome::NotFound => Ok(Value::T),
            RemoveWatchOutcome::Removed => {
                state.registry.unregister(&watch_id);
                Ok(Value::T)
            }
            RemoveWatchOutcome::RemovedWithError(error) => {
                state.registry.unregister(&watch_id);
                Err(error)
            }
        }
    })
}

#[cfg(test)]
#[path = "tests/positioned_aspects_test.rs"]
mod positioned_aspects_tests;
