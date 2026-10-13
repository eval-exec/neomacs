//! GNU-compatible Lisp boundary for the kqueue adapter.

use super::super::super::*;
use super::{KqueueRequest, action_from_lisp_name};
use crate::emacs_core::error::expect_args;

/// GNU kqueue descriptors are opaque bare fixnums, unlike inotify's conses.
/// GNU happens to expose its open fd; Neomacs uses a monotonic logical slot so
/// a queued event for a reused fd cannot target a newer watch.
fn extract_watch_id(value: Value) -> Option<WatchId> {
    let slot = value.as_fixnum()?;
    (slot >= 0).then(|| WatchId::new(slot, 0))
}

/// GNU `Fkqueue_add_watch` (`src/kqueue.c`) validates and normalizes FILE,
/// silently ignores unknown flag symbols, and returns an opaque fixnum.
pub(crate) fn kqueue_add_watch(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let symbol_mode = ctx.symbol_mode();
    expect_args("kqueue-add-watch", &args, 3)?;
    ctx.expect_lisp_string(args[0])?;
    let expanded =
        crate::emacs_core::fileio::builtin_expand_file_name(ctx, vec![args[0], Value::NIL])?;
    let normalized = crate::emacs_core::fileio::builtin_directory_file_name(ctx, vec![expanded])?;
    if crate::emacs_core::fileio::builtin_file_exists_p(ctx, vec![normalized])?.is_nil() {
        return Err(crate::emacs_core::error::signal(
            "file-missing",
            vec![
                Value::string("File does not exist"),
                Value::string("No such file or directory"),
                normalized,
            ],
        ));
    }
    let path =
        crate::emacs_core::fileio::lisp_file_name_to_path_buf(ctx.expect_lisp_string(normalized)?);
    let flags = list_to_vec(&args[1]).ok_or_else(|| {
        crate::emacs_core::error::signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), args[1]],
        )
    })?;
    if !crate::emacs_core::builtins::value_is_function(ctx, args[2]) {
        return Err(crate::emacs_core::error::signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("invalid-function"), args[2]],
        ));
    }
    let actions = flags
        .iter()
        .filter_map(|flag| flag.as_symbol_name(symbol_mode))
        .filter_map(action_from_lisp_name)
        .fold(enumflags2::BitFlags::empty(), |actions, action| {
            actions | action
        });
    let callback = args[2];
    let notifier = ctx.wait_notifier();

    FILE_NOTIFY_STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        let watch_id = state
            .backend
            .add_watch(&path, KqueueRequest::new(actions), notifier)?;
        state
            .registry
            .register(watch_id.clone(), callback, normalized);
        Ok(Value::fixnum(watch_id.slot()))
    })
}

pub(crate) fn kqueue_rm_watch(args: Vec<Value>) -> EvalResult {
    expect_args("kqueue-rm-watch", &args, 1)?;
    let not_a_watch_descriptor =
        || file_notify_error("Not a watch descriptor", None, Some(args[0]));
    let Some(watch_id) = extract_watch_id(args[0]) else {
        return Err(not_a_watch_descriptor());
    };
    FILE_NOTIFY_STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        match state.backend.remove_watch(&watch_id) {
            RemoveWatchOutcome::NotFound => Err(not_a_watch_descriptor()),
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

pub(crate) fn kqueue_valid_p(args: Vec<Value>) -> EvalResult {
    expect_args("kqueue-valid-p", &args, 1)?;
    let Some(watch_id) = extract_watch_id(args[0]) else {
        return Ok(Value::NIL);
    };
    FILE_NOTIFY_STATE.with(|slot| {
        let state = slot.borrow();
        Ok(Value::bool_val(state.backend.valid_p(&watch_id)))
    })
}
