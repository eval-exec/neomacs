//! Inhibitor-lock file descriptors — GNU `dbus--fd-open` / `--fd-close` /
//! `--registered-fds`.

use std::cell::RefCell;

use crate::emacs_core::error::{Flow, LispCondition, expect_args, signal};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

thread_local! {
    static REGISTERED: RefCell<Vec<(i64, String)>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn fd_open(ctx: &mut Context, args: Vec<Value>) -> Result<Value, Flow> {
    let _ = ctx;
    expect_args("dbus--fd-open", &args, 1)?;
    let filename = args[0].as_utf8_str().map(str::to_owned).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )
    })?;
    let expanded = crate::emacs_core::fileio::expand_file_name(&filename, None);
    if let Some((fd, _)) = REGISTERED.with(|slot| {
        slot.borrow()
            .iter()
            .find(|(_, name)| name == &expanded)
            .cloned()
    }) {
        return Ok(Value::fixnum(fd));
    }
    open_and_register(&expanded)
}

pub(super) fn fd_close(args: Vec<Value>) -> Result<Value, Flow> {
    expect_args("dbus--fd-close", &args, 1)?;
    let Some(fd) = args[0].as_fixnum() else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integerp"), args[0]],
        ));
    };
    let found = REGISTERED.with(|slot| {
        let mut registered = slot.borrow_mut();
        registered
            .iter()
            .position(|(stored, _)| *stored == fd)
            .map(|index| registered.remove(index))
    });
    let Some((raw, _)) = found else {
        return Ok(Value::NIL);
    };
    Ok(Value::bool_val(close_fd(raw)))
}

pub(super) fn registered_fds(args: Vec<Value>) -> Result<Value, Flow> {
    expect_args("dbus--registered-fds", &args, 0)?;
    let entries = REGISTERED.with(|slot| slot.borrow().clone());
    Ok(Value::list(
        entries
            .into_iter()
            .map(|(fd, name)| Value::cons(Value::fixnum(fd), Value::string(name)))
            .collect(),
    ))
}

pub(super) fn reset() {
    REGISTERED.with(|slot| slot.borrow_mut().clear());
}

fn dbus_error(message: &str, extra: Value) -> Flow {
    signal(
        "dbus-error",
        vec![Value::string(message.to_owned()), extra],
    )
}

fn open_and_register(expanded: &str) -> Result<Value, Flow> {
    std::cfg_select! {
        unix => {
            use std::fs::File;
            use std::os::fd::IntoRawFd;

            let file = File::open(expanded).map_err(|_| {
                dbus_error("Cannot open file", Value::string(expanded.to_owned()))
            })?;
            let fd = file.into_raw_fd() as i64;
            REGISTERED.with(|slot| slot.borrow_mut().push((fd, expanded.to_owned())));
            Ok(Value::fixnum(fd))
        }
        windows => Err(dbus_error(
            "Cannot open file",
            Value::string(expanded.to_owned()),
        )),
        _ => Err(dbus_error(
            "Cannot open file",
            Value::string(expanded.to_owned()),
        )),
    }
}

fn close_fd(raw: i64) -> bool {
    std::cfg_select! {
        unix => unsafe { libc::close(raw as std::os::fd::RawFd) == 0 },
        windows => {
            let _ = raw;
            false
        }
        _ => {
            let _ = raw;
            false
        }
    }
}
