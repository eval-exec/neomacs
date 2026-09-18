//! Session/system/private bus table — GNU `xd_registered_buses`.

use std::cell::RefCell;
use std::collections::HashMap;

use dbus::channel::{BusType, Channel};

use crate::emacs_core::error::{Flow, LispCondition, expect_args, expect_args_range, signal};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

use super::watch;

thread_local! {
    static BUSES: RefCell<HashMap<BusKey, BusConnection>> = RefCell::new(HashMap::new());
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) enum BusKey {
    Session,
    System,
    SessionPrivate,
    SystemPrivate,
    Address(String),
}

struct BusConnection {
    channel: Channel,
}

impl BusKey {
    pub(super) fn to_lisp(&self) -> Value {
        match self {
            Self::Session => Value::keyword_by_name(":session"),
            Self::System => Value::keyword_by_name(":system"),
            Self::SessionPrivate => Value::keyword_by_name(":session-private"),
            Self::SystemPrivate => Value::keyword_by_name(":system-private"),
            Self::Address(address) => Value::string(address.clone()),
        }
    }

    pub(super) fn from_lisp(bus: Value, private: bool) -> Result<Self, Flow> {
        if bus.is_string() {
            let address = string_arg(bus)?;
            if std::env::var("DBUS_SESSION_BUS_ADDRESS")
                .ok()
                .is_some_and(|session| session == address)
            {
                return Ok(if private {
                    Self::SessionPrivate
                } else {
                    Self::Session
                });
            }
            return Ok(Self::Address(address));
        }
        if !bus.is_symbol() {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), bus],
            ));
        }
        let key = if bus == Value::keyword_by_name(":session") {
            Self::Session
        } else if bus == Value::keyword_by_name(":system") {
            Self::System
        } else if bus == Value::keyword_by_name(":session-private") {
            Self::SessionPrivate
        } else if bus == Value::keyword_by_name(":system-private") {
            Self::SystemPrivate
        } else {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("keywordp"), bus],
            ));
        };
        Ok(match (key, private) {
            (Self::Session, true) => Self::SessionPrivate,
            (Self::System, true) => Self::SystemPrivate,
            (other, _) => other,
        })
    }
}

pub(super) fn init_bus(ctx: &mut Context, args: Vec<Value>) -> Result<Value, Flow> {
    let _ = ctx;
    expect_args_range("dbus--init-bus", &args, 1, 2)?;
    let private = args.get(1).copied().is_some_and(Value::is_truthy);
    let key = BusKey::from_lisp(args[0], private)?;
    let channel = open_channel(&key)?;
    let refs = BUSES.with(|slot| {
        let mut buses = slot.borrow_mut();
        buses.insert(key, BusConnection { channel });
        1
    });
    Ok(Value::fixnum(refs))
}

pub(super) fn get_unique_name(args: Vec<Value>) -> Result<Value, Flow> {
    expect_args("dbus-get-unique-name", &args, 1)?;
    let key = BusKey::from_lisp(args[0], false)?;
    BUSES.with(|slot| {
        let buses = slot.borrow();
        let Some(conn) = buses.get(&key) else {
            return Err(dbus_error("No connection to bus"));
        };
        let Some(name) = conn.channel.unique_name() else {
            return Err(dbus_error("No unique name available"));
        };
        Ok(Value::string(name.to_owned()))
    })
}

pub(super) fn with_channel<T>(
    key: &BusKey,
    f: impl FnOnce(&Channel) -> Result<T, Flow>,
) -> Result<T, Flow> {
    BUSES.with(|slot| {
        let buses = slot.borrow();
        let Some(conn) = buses.get(key) else {
            return Err(dbus_error("No connection to bus"));
        };
        f(&conn.channel)
    })
}

pub(super) fn has_buses() -> bool {
    BUSES.with(|slot| !slot.borrow().is_empty())
}

pub(super) fn pump_messages() -> Result<Vec<(BusKey, dbus::Message)>, Flow> {
    BUSES.with(|slot| {
        let buses = slot.borrow();
        let mut out = Vec::new();
        for (key, conn) in buses.iter() {
            watch::pump(&conn.channel)?;
            while let Some(message) = conn.channel.pop_message() {
                out.push((key.clone(), message));
            }
        }
        Ok(out)
    })
}

pub(super) fn reset() {
    BUSES.with(|slot| slot.borrow_mut().clear());
}

fn open_channel(key: &BusKey) -> Result<Channel, Flow> {
    let result = match key {
        BusKey::Session | BusKey::SessionPrivate => Channel::get_private(BusType::Session),
        BusKey::System | BusKey::SystemPrivate => Channel::get_private(BusType::System),
        BusKey::Address(address) => Channel::open_private(address).and_then(|mut channel| {
            channel.register()?;
            Ok(channel)
        }),
    };
    result.map_err(|err| dbus_error(&err.to_string()))
}

pub(super) fn dbus_error(message: &str) -> Flow {
    signal("dbus-error", vec![Value::string(message.to_owned())])
}

fn string_arg(value: Value) -> Result<String, Flow> {
    value.as_utf8_str().map(str::to_owned).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), value],
        )
    })
}
