//! Incoming `dbus-event` construction — GNU `xd_read_message_1`.
//!
//! GNU stores one `dbus-event` per registration that matches the message, and
//! stores nothing at all when nothing matches (`src/dbusbind.c:1864-1918`).
//! The handler slot is therefore never nil, and it must not be:
//! `dbus-check-event` (`lisp/net/dbus.el`) requires `functionp` on it, and
//! signals the whole message back as "Not a valid D-Bus event" otherwise.
//! A message nobody registered for -- Avahi's `CacheExhausted` on the
//! service-type browser `zeroconf-init` opens, say -- is dropped.

use dbus::arg::ArgType;
use dbus::message::MessageType;

use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::{Value, eq_value};

use super::connection;
use super::types::retrieve_arg;

pub(super) fn drain(ctx: &mut Context) -> Result<usize, Flow> {
    let incoming = connection::pump_messages()?;
    let count = incoming.len();
    for (bus, message) in incoming {
        queue_events(ctx, bus.to_lisp(), message)?;
    }
    Ok(count)
}

/// Build the `dbus-event`s one incoming message produces.
///
/// `bus` is the Lisp value (`:system`, `:session`) the message arrived on.
pub(super) fn queue_events(
    ctx: &mut Context,
    bus: Value,
    mut message: dbus::Message,
) -> Result<(), Flow> {
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

    let sender = message.sender().map(|sender| sender.to_string());
    let destination = message.destination().map(|name| name.to_string());
    let path = message.path().map(|path| path.to_string());
    let interface = message.interface().map(|name| name.to_string());
    // GNU's member slot is the error name for `DBUS_MESSAGE_TYPE_ERROR`
    // (`dbus_message_get_error_name`), which lives in the D-Bus ERROR_NAME
    // header field -- `Message::member` reads MEMBER and is empty there.
    // `dbus-check-event` requires a string in that slot for every type but
    // method-return, so an error reply has to be named.
    let member = match message.msg_type() {
        MessageType::Error => message
            .as_result()
            .err()
            .and_then(|error| error.name().map(str::to_owned)),
        _ => message.member().map(|name| name.to_string()),
    };

    // `(dbus-event BUS TYPE SERIAL SERVICE DESTINATION PATH INTERFACE MEMBER
    //  HANDLER . ARGS)`.  GNU appends the handler to this prefix in
    // `xd_store_event`, once per matching registration.
    let prefix = vec![
        Value::symbol("dbus-event"),
        bus,
        Value::fixnum(mtype),
        Value::fixnum(serial as i64),
        opt_string(&sender),
        opt_string(&destination),
        opt_string(&path),
        opt_string(&interface),
        opt_string(&member),
    ];

    match message.msg_type() {
        // Answered from the `(:serial BUS SERIAL)` reply registry, which the
        // caller filled with `dbus-message-internal`'s HANDLER argument.
        MessageType::MethodReturn | MessageType::Error => {
            let handler = take_serial_handler(ctx, bus, serial)?;
            if !handler.is_nil() {
                store_event(ctx, &prefix, handler, &args);
            }
        }
        // Every registration that matches the message is called; one event
        // per handler, each handler at most once.
        MessageType::MethodCall | MessageType::Signal => {
            dispatch_registered(
                ctx,
                bus,
                mtype,
                &prefix,
                &sender,
                &path,
                opt_string(&interface),
                opt_string(&member),
                &args,
            )?;
        }
    }

    // `monitor:` (`src/dbusbind.c:1921`) — every valid message also reaches a
    // registered monitor, in addition to the handlers above.
    if let Some(handler) = monitor_handler(ctx, bus)? {
        store_event(ctx, &prefix, handler, &args);
    }
    Ok(())
}

/// The method-call/signal arm of `xd_read_message_1`.
#[allow(clippy::too_many_arguments)]
fn dispatch_registered(
    ctx: &mut Context,
    bus: Value,
    mtype: i64,
    prefix: &[Value],
    sender: &Option<String>,
    path: &Option<String>,
    interface: Value,
    member: Value,
    args: &[Value],
) -> Result<(), Flow> {
    // "Vdbus_registered_objects_table requires non-nil interface and member."
    if interface.is_nil() || member.is_nil() {
        return Ok(());
    }

    let mut called: Vec<Value> = Vec::new();
    for entry in registered_entries(ctx, bus, mtype, interface, member)? {
        // An entry is `(UNAME SERVICE PATH HANDLER RULE)`.  GNU matches the
        // sender against UNAME and the object path against PATH, and ignores
        // SERVICE -- the daemon already routed the message to this bus name.
        let key_uname = car(entry);
        if let Some(sender) = sender.as_deref()
            && !key_uname.is_nil()
            && key_uname.as_utf8_str() != Some(sender)
        {
            continue;
        }
        let key_path = car(cdr(cdr(entry)));
        if let Some(path) = path.as_deref()
            && !key_path.is_nil()
            && key_path.as_utf8_str() != Some(path)
        {
            continue;
        }
        let handler = car(cdr(cdr(cdr(entry))));
        if handler.is_nil() {
            continue;
        }
        if called.iter().any(|seen| eq_value(seen, &handler)) {
            continue;
        }
        called.push(handler);
        store_event(ctx, prefix, handler, args);
    }
    Ok(())
}

/// The registration lists one message is dispatched against.
///
/// The exact `(KIND BUS INTERFACE MEMBER)` key first, then -- for signals --
/// the three wildcard keys `dbus-register-signal` also fills when INTERFACE
/// or SIGNAL is nil (`src/dbusbind.c:1872-1890`).
fn registered_entries(
    ctx: &mut Context,
    bus: Value,
    mtype: i64,
    interface: Value,
    member: Value,
) -> Result<Vec<Value>, Flow> {
    let Some(table) = ctx.obarray.symbol_value("dbus-registered-objects-table") else {
        return Ok(Vec::new());
    };
    let kind = Value::keyword_by_name(if mtype == 1 { ":method" } else { ":signal" });
    let mut keys = vec![Value::list(vec![kind, bus, interface, member])];
    if mtype != 1 {
        keys.push(Value::list(vec![kind, bus, Value::NIL, member]));
        keys.push(Value::list(vec![kind, bus, interface, Value::NIL]));
        keys.push(Value::list(vec![kind, bus, Value::NIL, Value::NIL]));
    }

    let mut entries = Vec::new();
    for key in keys {
        let value = crate::emacs_core::builtins::builtin_gethash(vec![key, *table, Value::NIL])?;
        let mut rest = value;
        while rest.is_cons() {
            entries.push(rest.cons_car());
            rest = rest.cons_cdr();
        }
    }
    Ok(entries)
}

/// A registered monitor's handler: the first entry's HANDLER.
fn monitor_handler(ctx: &mut Context, bus: Value) -> Result<Option<Value>, Flow> {
    let Some(table) = ctx.obarray.symbol_value("dbus-registered-objects-table") else {
        return Ok(None);
    };
    let key = Value::list(vec![Value::keyword_by_name(":monitor"), bus]);
    let value = crate::emacs_core::builtins::builtin_gethash(vec![key, *table, Value::NIL])?;
    if !value.is_cons() {
        return Ok(None);
    }
    // An entry is `(UNAME SERVICE PATH HANDLER RULE)`; `BecomeMonitor` gives
    // the monitor the whole bus, so only its handler is used.
    let handler = car(cdr(cdr(cdr(value.cons_car()))));
    Ok(if handler.is_nil() { None } else { Some(handler) })
}

/// Take the `dbus-message-internal` handler registered for a reply serial.
fn take_serial_handler(ctx: &mut Context, bus: Value, serial: u32) -> Result<Value, Flow> {
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

/// GNU `xd_store_event`: queue `(HANDLER . EVENT-PREFIX) ++ ARGS`.
fn store_event(ctx: &mut Context, prefix: &[Value], handler: Value, args: &[Value]) {
    let mut event = prefix.to_vec();
    event.push(handler);
    event.extend_from_slice(args);
    ctx.queue_special_event(Value::list(event));
}

/// GNU `CAR_SAFE`.
fn car(value: Value) -> Value {
    if value.is_cons() {
        value.cons_car()
    } else {
        Value::NIL
    }
}

/// GNU `CDR_SAFE`.
fn cdr(value: Value) -> Value {
    if value.is_cons() {
        value.cons_cdr()
    } else {
        Value::NIL
    }
}

fn opt_string(value: &Option<String>) -> Value {
    match value {
        Some(text) => Value::string(text.clone()),
        None => Value::NIL,
    }
}
