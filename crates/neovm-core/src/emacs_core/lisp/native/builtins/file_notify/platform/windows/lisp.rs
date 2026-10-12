//! GNU-compatible Lisp boundary for the ReadDirectoryChangesW adapter.

use super::super::super::*;
use super::{W32Filter, W32Request};
use crate::emacs_core::error::expect_args;

/// GNU's w32 backend exposes an opaque pointer integer. Neomacs keeps the
/// platform handle private and uses this fixnum only as a compatibility token.
fn extract_watch_id(value: Value) -> Option<WatchId> {
    let slot = value.as_fixnum()?;
    (slot >= 0).then(|| WatchId::new(slot, 0))
}

pub(crate) fn w32notify_add_watch(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let symbol_mode = ctx.symbol_mode();
    expect_args("w32notify-add-watch", &args, 3)?;
    let filters = list_to_vec(&args[1]).ok_or_else(|| {
        crate::emacs_core::error::signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), args[1]],
        )
    })?;
    ctx.expect_lisp_string(args[0])?;
    let expanded =
        crate::emacs_core::fileio::builtin_expand_file_name(ctx, vec![args[0], Value::NIL])?;
    let normalized = crate::emacs_core::fileio::builtin_directory_file_name(ctx, vec![expanded])?;
    let path =
        crate::emacs_core::fileio::lisp_file_name_to_path_buf(ctx.expect_lisp_string(normalized)?);
    let filters = filters
        .iter()
        .filter_map(|filter| filter.as_symbol_name(symbol_mode))
        .filter_map(W32Filter::from_lisp_name)
        .fold(enumflags2::BitFlags::empty(), |filters, filter| {
            filters | filter
        });
    let callback = args[2];
    let notifier = ctx.wait_notifier();

    FILE_NOTIFY_STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        let watch_id = state
            .backend
            .add_watch(&path, W32Request::new(filters), notifier)?;
        state
            .registry
            .register(watch_id.clone(), callback, normalized);
        Ok(Value::fixnum(watch_id.slot()))
    })
}

pub(crate) fn w32notify_rm_watch(args: Vec<Value>) -> EvalResult {
    expect_args("w32notify-rm-watch", &args, 1)?;
    let invalid = || file_notify_error("Invalid watch descriptor", None, Some(args[0]));
    let Some(watch_id) = extract_watch_id(args[0]) else {
        return Err(invalid());
    };
    FILE_NOTIFY_STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        match state.backend.remove_watch(&watch_id) {
            RemoveWatchOutcome::NotFound => Err(invalid()),
            RemoveWatchOutcome::Removed => {
                state.registry.unregister(&watch_id);
                Ok(Value::NIL)
            }
            RemoveWatchOutcome::RemovedWithError(error) => {
                state.registry.unregister(&watch_id);
                Err(error)
            }
        }
    })
}

pub(crate) fn w32notify_valid_p(args: Vec<Value>) -> EvalResult {
    expect_args("w32notify-valid-p", &args, 1)?;
    let Some(watch_id) = extract_watch_id(args[0]) else {
        return Ok(Value::NIL);
    };
    FILE_NOTIFY_STATE.with(|slot| Ok(Value::bool_val(slot.borrow().backend.valid_p(&watch_id))))
}
