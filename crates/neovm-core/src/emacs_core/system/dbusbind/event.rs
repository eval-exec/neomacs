//! Incoming `dbus-event` construction — GNU `xd_read_message_1`.

use dbus::arg::ArgType;
use dbus::message::MessageType;

use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

use super::connection;
use super::types::retrieve_arg;

pub(super) fn drain(ctx: &mut Context) -> Result<usize, Flow> {
    let incoming = connection::pump_messages()?;
    let count = incoming.len();
    for (bus, message) in incoming {
        queue_event(ctx, bus.to_lisp(), message)?;
    }
    Ok(count)
}

fn queue_event(ctx: &mut Context, bus: Value, message: dbus::Message) -> Result<(), Flow> {
    let mtype = match message.msg_type() {
        MessageType::MethodCall => 1,
        MessageType::MethodReturn => 2,
        MessageType::Error => 3,
        MessageType::Signal => 4,
    };
    let serial = if matches!(
        message.msg_type(),
        MessageType::MethodReturn | MessageType::Error
    ) {
        message.get_reply_serial().unwrap_or(0)
    } else {
        message.get_serial().unwrap_or(0)
    };

    let mut args = Vec::new();
    let mut iter = message.iter_init();
    while iter.arg_type() != ArgType::Invalid {
        args.push(retrieve_arg(&mut iter)?);
        let _ = dbus::arg::Iter::next(&mut iter);
    }

    let member_or_error = optional_str(message.member());

    let mut event = vec![
        Value::symbol("dbus-event"),
        bus,
        Value::fixnum(mtype),
        Value::fixnum(serial as i64),
        optional_str(message.sender()),
        optional_str(message.destination()),
        optional_str(message.path()),
        optional_str(message.interface()),
        member_or_error,
    ];

    let handler = if matches!(
        message.msg_type(),
        MessageType::MethodReturn | MessageType::Error
    ) {
        lookup_handler(ctx, bus, serial)?
    } else {
        Value::NIL
    };
    event.push(handler);
    event.extend(args);
    ctx.queue_special_event(Value::list(event));
    Ok(())
}

fn lookup_handler(ctx: &mut Context, bus: Value, serial: u32) -> Result<Value, Flow> {
    let Some(table) = ctx.obarray.symbol_value("dbus-registered-objects-table") else {
        return Ok(Value::NIL);
    };
    let key = Value::list(vec![
        Value::keyword_by_name(":serial"),
        bus,
        Value::fixnum(serial as i64),
    ]);
    let handler = crate::emacs_core::builtins::builtin_gethash(vec![key, *table, Value::NIL])?;
    if !handler.is_nil() {
        let _ = crate::emacs_core::builtins::builtin_remhash(vec![key, *table]);
    }
    Ok(if handler.is_cons() {
        handler.cons_car()
    } else {
        handler
    })
}

fn optional_str(value: Option<impl ToString>) -> Value {
    value
        .map(|s| Value::string(s.to_string()))
        .unwrap_or(Value::NIL)
}
