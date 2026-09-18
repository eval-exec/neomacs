//! `dbus-message-internal` — GNU `Fdbus_message_internal`.

use dbus::Message;
use dbus::arg::IterAppend;

use crate::emacs_core::error::{Flow, LispCondition, expect_min_args, signal};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

use super::connection::{self, BusKey, dbus_error};
use super::types::{append_arg, is_type_keyword, object_to_arg_type};

pub(super) fn message_internal(ctx: &mut Context, args: Vec<Value>) -> Result<Value, Flow> {
    expect_min_args("dbus-message-internal", &args, 3)?;
    let mtype = args[0].as_fixnum().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("natnump"), args[0]],
        )
    })?;
    if !(0..=4).contains(&mtype) {
        return Err(dbus_error("Invalid message type"));
    }
    let bus = args[1];
    let key = BusKey::from_lisp(bus, false)?;
    let service = args[2];

    let (path, interface, member, serial, error_name, handler, mut count) = match mtype {
        1 | 4 => {
            if args.len() < 6 {
                return Err(signal(
                    LispCondition::WrongNumberOfArguments,
                    vec![Value::symbol("dbus-message-internal"), Value::fixnum(args.len() as i64)],
                ));
            }
            let handler = if mtype == 1 {
                if args.len() < 7 {
                    return Err(signal(
                        LispCondition::WrongNumberOfArguments,
                        vec![
                            Value::symbol("dbus-message-internal"),
                            Value::fixnum(args.len() as i64),
                        ],
                    ));
                }
                args[6]
            } else {
                Value::NIL
            };
            (
                args[3],
                args[4],
                args[5],
                0u32,
                Value::NIL,
                handler,
                if mtype == 1 { 7 } else { 6 },
            )
        }
        2 | 3 => {
            let serial = args[3].as_fixnum().unwrap_or(0) as u32;
            let error_name = if mtype == 3 {
                args.get(4).copied().unwrap_or(Value::NIL)
            } else {
                Value::NIL
            };
            (
                Value::NIL,
                Value::NIL,
                Value::NIL,
                serial,
                error_name,
                Value::NIL,
                if mtype == 3 { 5 } else { 4 },
            )
        }
        _ => (
            Value::NIL,
            Value::NIL,
            Value::NIL,
            0,
            Value::NIL,
            Value::NIL,
            3,
        ),
    };

    if mtype == 0 {
        while count < args.len() {
            let dtype = object_to_arg_type(args[count])?;
            if count + 1 < args.len() && is_type_keyword(args[count]) {
                count += 1;
            }
            let _ = dtype;
            count += 1;
        }
        return Ok(Value::T);
    }

    let mut message = build_message(mtype, service, path, interface, member, serial, error_name)?;

    while count + 1 < args.len()
        && (args[count] == Value::keyword_by_name(":timeout")
            || args[count] == Value::keyword_by_name(":authorizable")
            || args[count] == Value::keyword_by_name(":keep-fd"))
    {
        if args[count] == Value::keyword_by_name(":keep-fd") {
            count += 1;
        } else {
            count += 2;
        }
    }

    {
        let mut iter = IterAppend::new(&mut message);
        while count < args.len() {
            let dtype = object_to_arg_type(args[count])?;
            let value = if count + 1 < args.len() && is_type_keyword(args[count]) {
                count += 1;
                args[count]
            } else {
                args[count]
            };
            append_arg(&mut iter, dtype, value)?;
            count += 1;
        }
    }

    if mtype == 0 {
        return Ok(Value::T);
    }

    let serial = connection::with_channel(&key, |channel| {
        let serial = dbus::channel::Channel::send(channel, message)
            .map_err(|()| dbus_error("Cannot send message"))?;
        channel.flush();
        Ok(serial)
    })?;

    if handler.is_nil() {
        return Ok(Value::NIL);
    }

    let key_lisp = Value::list(vec![
        Value::keyword_by_name(":serial"),
        key.to_lisp(),
        Value::fixnum(serial as i64),
    ]);
    if let Some(table) = ctx.obarray.symbol_value("dbus-registered-objects-table") {
        crate::emacs_core::builtins::builtin_puthash(vec![key_lisp, handler, *table])?;
    }
    Ok(key_lisp)
}

fn build_message(
    mtype: i64,
    service: Value,
    path: Value,
    interface: Value,
    member: Value,
    serial: u32,
    error_name: Value,
) -> Result<Message, Flow> {
    let dest = optional_string(service)?;
    let message = match mtype {
        1 => Message::new_method_call(
            dest.as_deref().unwrap_or("org.freedesktop.DBus"),
            require_string(path)?,
            require_string(interface)?,
            require_string(member)?,
        )
        .map_err(|err| dbus_error(&err))?,
        4 => {
            let mut signal = Message::new_signal(
                require_string(path)?,
                require_string(interface)?,
                require_string(member)?,
            )
            .map_err(|err| dbus_error(&err))?;
            if let Some(dest) = dest.as_deref() {
                signal.set_destination(Some(dest.into()));
            }
            signal
        }
        2 | 3 => {
            // The dbus crate only builds replies from an original method-call
            // message. GNU sets reply serial on a fresh buffer. Method-return
            // / error from Lisp is used when Emacs is a D-Bus service; the
            // client path (fcitx5, notifications) is method-call + signal.
            let _ = (serial, error_name, dest);
            return Err(dbus_error("Unable to create a return message"));
        }
        _ => return Err(dbus_error("Invalid message type")),
    };
    Ok(message)
}

fn require_string(value: Value) -> Result<String, Flow> {
    value.as_utf8_str().map(str::to_owned).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), value],
        )
    })
}

fn optional_string(value: Value) -> Result<Option<String>, Flow> {
    if value.is_nil() {
        Ok(None)
    } else {
        require_string(value).map(Some)
    }
}
