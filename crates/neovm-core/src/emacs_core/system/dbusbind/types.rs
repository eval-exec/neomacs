//! GNU Lisp ↔ D-Bus type conversion (`xd_symbol_to_dbus_type`, `xd_append_arg`,
//! `xd_retrieve_arg` in `src/dbusbind.c`).

use dbus::arg::{ArgType, Iter, IterAppend};
use dbus::strings::{Path as DbusPath, Signature};

use crate::emacs_core::error::{Flow, LispCondition, signal};
use crate::emacs_core::value::Value;

use super::connection::dbus_error;

pub(super) fn keyword_for(arg_type: ArgType) -> Value {
    Value::keyword_by_name(match arg_type {
        ArgType::Byte => ":byte",
        ArgType::Boolean => ":boolean",
        ArgType::Int16 => ":int16",
        ArgType::UInt16 => ":uint16",
        ArgType::Int32 => ":int32",
        ArgType::UInt32 => ":uint32",
        ArgType::Int64 => ":int64",
        ArgType::UInt64 => ":uint64",
        ArgType::Double => ":double",
        ArgType::String => ":string",
        ArgType::ObjectPath => ":object-path",
        ArgType::Signature => ":signature",
        ArgType::UnixFd => ":unix-fd",
        ArgType::Array => ":array",
        ArgType::Variant => ":variant",
        ArgType::Struct => ":struct",
        ArgType::DictEntry => ":dict-entry",
        ArgType::Invalid => ":invalid",
    })
}

pub(super) fn is_type_keyword(value: Value) -> bool {
    symbol_to_arg_type(value).is_some()
}

pub(super) fn symbol_to_arg_type(value: Value) -> Option<ArgType> {
    if !value.is_keyword() {
        return None;
    }
    if value == Value::keyword_by_name(":byte") {
        Some(ArgType::Byte)
    } else if value == Value::keyword_by_name(":boolean") {
        Some(ArgType::Boolean)
    } else if value == Value::keyword_by_name(":int16") {
        Some(ArgType::Int16)
    } else if value == Value::keyword_by_name(":uint16") {
        Some(ArgType::UInt16)
    } else if value == Value::keyword_by_name(":int32") {
        Some(ArgType::Int32)
    } else if value == Value::keyword_by_name(":uint32") {
        Some(ArgType::UInt32)
    } else if value == Value::keyword_by_name(":int64") {
        Some(ArgType::Int64)
    } else if value == Value::keyword_by_name(":uint64") {
        Some(ArgType::UInt64)
    } else if value == Value::keyword_by_name(":double") {
        Some(ArgType::Double)
    } else if value == Value::keyword_by_name(":string") {
        Some(ArgType::String)
    } else if value == Value::keyword_by_name(":object-path") {
        Some(ArgType::ObjectPath)
    } else if value == Value::keyword_by_name(":signature") {
        Some(ArgType::Signature)
    } else if value == Value::keyword_by_name(":unix-fd") {
        Some(ArgType::UnixFd)
    } else if value == Value::keyword_by_name(":array") {
        Some(ArgType::Array)
    } else if value == Value::keyword_by_name(":variant") {
        Some(ArgType::Variant)
    } else if value == Value::keyword_by_name(":struct") {
        Some(ArgType::Struct)
    } else if value == Value::keyword_by_name(":dict-entry") {
        Some(ArgType::DictEntry)
    } else {
        None
    }
}

pub(super) fn is_basic(arg_type: ArgType) -> bool {
    !matches!(
        arg_type,
        ArgType::Array | ArgType::Variant | ArgType::Struct | ArgType::DictEntry | ArgType::Invalid
    )
}

/// GNU `XD_OBJECT_TO_DBUS_TYPE`.
pub(super) fn object_to_arg_type(object: Value) -> Result<ArgType, Flow> {
    if object == Value::T || object.is_nil() {
        return Ok(ArgType::Boolean);
    }
    if let Some(n) = object.as_fixnum() {
        return Ok(if n >= 0 {
            ArgType::UInt32
        } else {
            ArgType::Int32
        });
    }
    if object.is_float() {
        return Ok(ArgType::Double);
    }
    if object.is_string() {
        return Ok(ArgType::String);
    }
    if let Some(arg_type) = symbol_to_arg_type(object) {
        return Ok(arg_type);
    }
    if object.is_cons() {
        let car = object.cons_car();
        if let Some(inner) = symbol_to_arg_type(car) {
            return Ok(if is_basic(inner) {
                ArgType::Array
            } else {
                inner
            });
        }
        return Ok(ArgType::Array);
    }
    Err(dbus_error("Unable to determine D-Bus type"))
}

/// Skip a leading type keyword, GNU `XD_NEXT_VALUE`.
pub(super) fn next_value(object: Value) -> Value {
    if object.is_cons() && is_type_keyword(object.cons_car()) {
        object.cons_cdr()
    } else {
        object
    }
}

pub(super) fn append_arg(
    iter: &mut IterAppend<'_>,
    arg_type: ArgType,
    object: Value,
) -> Result<(), Flow> {
    match arg_type {
        ArgType::Boolean => iter.append(object.is_truthy()),
        ArgType::Byte => iter.append(unsigned(object, u8::MAX as u64)? as u8),
        ArgType::Int16 => iter.append(signed(object, i16::MIN as i64, i16::MAX as i64)? as i16),
        ArgType::UInt16 => iter.append(unsigned(object, u16::MAX as u64)? as u16),
        ArgType::Int32 => iter.append(signed(object, i32::MIN as i64, i32::MAX as i64)? as i32),
        ArgType::UInt32 | ArgType::UnixFd => {
            iter.append(unsigned(object, u32::MAX as u64)? as u32)
        }
        ArgType::Int64 => iter.append(signed(object, i64::MIN, i64::MAX)?),
        ArgType::UInt64 => iter.append(unsigned(object, u64::MAX)?),
        ArgType::Double => {
            let value = object.as_float().ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("numberp"), object],
                )
            })?;
            iter.append(value);
        }
        ArgType::String => iter.append(string_arg(object)?.as_str()),
        ArgType::ObjectPath => {
            let path = string_arg(object)?;
            let typed: DbusPath<'_> = path.as_str().into();
            iter.append(typed);
        }
        ArgType::Signature => {
            let signature = string_arg(object)?;
            let typed: Signature<'_> = signature.as_str().into();
            iter.append(typed);
        }
        ArgType::Array | ArgType::Variant | ArgType::Struct | ArgType::DictEntry => {
            append_container(iter, arg_type, object)?;
        }
        ArgType::Invalid => return Err(dbus_error("Invalid D-Bus type")),
    }
    Ok(())
}

fn append_container(
    iter: &mut IterAppend<'_>,
    arg_type: ArgType,
    mut object: Value,
) -> Result<(), Flow> {
    if object.is_cons() && !is_basic(object_to_arg_type(object.cons_car())?) {
        object = next_value(object);
    }
    match arg_type {
        ArgType::Array => {
            let signature = array_signature(object)?;
            iter.append_array(&signature, |sub| {
                append_elements(sub, object).unwrap_or_else(|err| {
                    panic_flow(err);
                })
            });
        }
        ArgType::Variant => {
            let inner_type = if object.is_cons() {
                object_to_arg_type(object.cons_car())?
            } else {
                object_to_arg_type(object)?
            };
            let signature = dbus_sig(inner_type);
            iter.append_variant(&signature, |sub| {
                let value = if object.is_cons() {
                    next_value(object).cons_car()
                } else {
                    object
                };
                append_arg(sub, inner_type, value).unwrap_or_else(|err| panic_flow(err));
            });
        }
        ArgType::Struct => {
            iter.append_struct(|sub| {
                append_elements(sub, object).unwrap_or_else(|err| panic_flow(err));
            });
        }
        ArgType::DictEntry => {
            iter.append_dict_entry(|sub| {
                append_elements(sub, object).unwrap_or_else(|err| panic_flow(err));
            });
        }
        _ => return Err(dbus_error("Invalid D-Bus container")),
    }
    Ok(())
}

fn append_elements(iter: &mut IterAppend<'_>, mut object: Value) -> Result<(), Flow> {
    while object.is_cons() {
        let dtype = object_to_arg_type(object.cons_car())?;
        object = next_value(object);
        if !object.is_cons() && !object.is_nil() {
            append_arg(iter, dtype, object)?;
            break;
        }
        let value = if object.is_cons() {
            object.cons_car()
        } else {
            Value::NIL
        };
        append_arg(iter, dtype, value)?;
        object = if object.is_cons() {
            object.cons_cdr()
        } else {
            Value::NIL
        };
    }
    Ok(())
}

fn array_signature(object: Value) -> Result<Signature<'static>, Flow> {
    if object.is_nil() {
        return Ok(dbus_sig(ArgType::String));
    }
    let first = if object.is_cons() {
        object.cons_car()
    } else {
        object
    };
    let dtype = object_to_arg_type(first)?;
    Ok(dbus_sig(dtype))
}

fn dbus_sig(arg_type: ArgType) -> Signature<'static> {
    Signature::from((arg_type as u8 as char).to_string())
}

pub(super) fn retrieve_arg(iter: &mut Iter<'_>) -> Result<Value, Flow> {
    let arg_type = iter.arg_type();
    let typed = match arg_type {
        ArgType::Byte => Value::fixnum(iter.get::<u8>().unwrap_or(0) as i64),
        ArgType::Boolean => Value::bool_val(iter.get::<bool>().unwrap_or(false)),
        ArgType::Int16 => Value::fixnum(iter.get::<i16>().unwrap_or(0) as i64),
        ArgType::UInt16 => Value::fixnum(iter.get::<u16>().unwrap_or(0) as i64),
        ArgType::Int32 => Value::fixnum(iter.get::<i32>().unwrap_or(0) as i64),
        ArgType::UInt32 | ArgType::UnixFd => Value::fixnum(iter.get::<u32>().unwrap_or(0) as i64),
        ArgType::Int64 => Value::fixnum(iter.get::<i64>().unwrap_or(0)),
        ArgType::UInt64 => {
            let n = iter.get::<u64>().unwrap_or(0);
            if n <= i64::MAX as u64 {
                Value::fixnum(n as i64)
            } else {
                Value::string(n.to_string())
            }
        }
        ArgType::Double => Value::make_float(iter.get::<f64>().unwrap_or(0.0)),
        ArgType::String => Value::string(iter.get::<String>().unwrap_or_default()),
        ArgType::ObjectPath => Value::string(
            iter.get::<DbusPath<'_>>()
                .map(|path| path.to_string())
                .unwrap_or_default(),
        ),
        ArgType::Signature => Value::string(
            iter.get::<Signature<'_>>()
                .map(|signature| signature.to_string())
                .unwrap_or_default(),
        ),
        ArgType::Array | ArgType::Variant | ArgType::Struct | ArgType::DictEntry => {
            let mut inner = iter.recurse(arg_type).ok_or_else(|| dbus_error("Cannot read container"))?;
            let mut items = Vec::new();
            while inner.arg_type() != ArgType::Invalid {
                items.push(retrieve_arg(&mut inner)?);
                let _ = Iter::next(&mut inner);
            }
            return Ok(Value::cons(keyword_for(arg_type), Value::list(items)));
        }
        ArgType::Invalid => return Ok(Value::NIL),
    };
    Ok(Value::list(vec![keyword_for(arg_type), typed]))
}

fn string_arg(value: Value) -> Result<String, Flow> {
    value.as_utf8_str().map(str::to_owned).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), value],
        )
    })
}

fn signed(value: Value, min: i64, max: i64) -> Result<i64, Flow> {
    let n = value.as_fixnum().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integerp"), value],
        )
    })?;
    if n < min || n > max {
        return Err(dbus_error("Integer out of range"));
    }
    Ok(n)
}

fn unsigned(value: Value, max: u64) -> Result<u64, Flow> {
    let n = value.as_fixnum().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integerp"), value],
        )
    })?;
    if n < 0 || n as u64 > max {
        return Err(dbus_error("Integer out of range"));
    }
    Ok(n as u64)
}

fn panic_flow(err: Flow) -> ! {
    panic!("D-Bus container callback cannot return Flow: {err:?}")
}
