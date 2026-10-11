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

/// Prepare the complete subtree before opening any native container. Native
/// iterator callbacks cannot return Lisp errors, so they only emit owned,
/// validated arguments and never inspect Lisp objects.
pub(super) fn append_arg(
    iter: &mut IterAppend<'_>,
    arg_type: ArgType,
    object: Value,
) -> Result<(), Flow> {
    PreparedArgument::prepare(arg_type, object, 0)?.append(iter);
    Ok(())
}

struct PreparedArgument {
    signature: Signature<'static>,
    body: PreparedBody,
}

enum PreparedBody {
    Basic(PreparedBasic),
    Array {
        element_signature: Signature<'static>,
        elements: Vec<PreparedArgument>,
    },
    Dictionary {
        key_signature: Signature<'static>,
        value_signature: Signature<'static>,
        entries: Vec<PreparedEntry>,
    },
    Struct {
        first: Box<PreparedArgument>,
        rest: Vec<PreparedArgument>,
    },
    Variant(Box<PreparedArgument>),
}

struct PreparedEntry {
    key: PreparedBasic,
    value: PreparedArgument,
}

enum PreparedBasic {
    Byte(u8),
    Boolean(bool),
    Int16(i16),
    UInt16(u16),
    Int32(i32),
    UInt32(u32),
    Int64(i64),
    UInt64(u64),
    Double(f64),
    String(String),
    ObjectPath(DbusPath<'static>),
    Signature(Signature<'static>),
    UnixFd(std::fs::File),
}

fn container_type_error(predicate: &str, value: Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol(predicate), value],
    )
}

fn checked_signature(signature: String) -> Result<Signature<'static>, Flow> {
    if signature.contains('\0') {
        return Err(dbus_error("D-Bus signature contains NUL"));
    }
    Signature::new(signature).map_err(|err| dbus_error(&err))
}

/// Normalize GNU's optional keyword/value pairs without accepting improper or
/// cyclic lists, or a type keyword with no corresponding value.
fn container_fields(object: Value) -> Result<Vec<(ArgType, Value)>, Flow> {
    let values = crate::emacs_core::value::list_to_vec(&object)
        .ok_or_else(|| dbus_error("D-Bus container must be a proper list"))?;
    let mut fields = Vec::new();
    let mut cursor = values.into_iter();
    while let Some(value) = cursor.next() {
        if let Some(dtype) = symbol_to_arg_type(value) {
            let value = cursor
                .next()
                .ok_or_else(|| dbus_error("Missing D-Bus value"))?;
            fields.push((dtype, value));
        } else {
            fields.push((object_to_arg_type(value)?, value));
        }
    }
    Ok(fields)
}

impl PreparedArgument {
    fn prepare(dtype: ArgType, object: Value, depth: usize) -> Result<Self, Flow> {
        // D-Bus limits total container nesting to 64. Check before traversing
        // Lisp data, including variants which do not grow their wire signature.
        if depth > 64 || (depth == 64 && !is_basic(dtype)) {
            return Err(dbus_error("D-Bus container nesting exceeds 64"));
        }
        if is_basic(dtype) {
            let (signature, basic) = PreparedBasic::prepare(dtype, object)?;
            return Ok(Self {
                signature,
                body: PreparedBody::Basic(basic),
            });
        }
        let source = object;
        let object = if object.is_cons()
            && symbol_to_arg_type(object.cons_car()).is_some_and(|kind| !is_basic(kind))
        {
            object.cons_cdr()
        } else {
            object
        };
        if dtype == ArgType::Array {
            let raw = crate::emacs_core::value::list_to_vec(&object)
                .ok_or_else(|| dbus_error("D-Bus array must be a proper list"))?;
            if let [keyword, value] = raw.as_slice() {
                if symbol_to_arg_type(*keyword) == Some(ArgType::Signature) {
                    let element = string_arg(*value)?;
                    // Validating the whole array permits a dictionary entry
                    // only in its legal position as an array element.
                    let signature = checked_signature(format!("a{element}"))?;
                    let body = if element.starts_with('{') {
                        PreparedBody::Dictionary {
                            key_signature: checked_signature(element[1..2].to_owned())?,
                            value_signature: checked_signature(
                                element[2..element.len() - 1].to_owned(),
                            )?,
                            entries: Vec::new(),
                        }
                    } else {
                        PreparedBody::Array {
                            element_signature: checked_signature(element)?,
                            elements: Vec::new(),
                        }
                    };
                    return Ok(Self { signature, body });
                }
            }
        }
        let fields = container_fields(object)?;
        match dtype {
            ArgType::Array
                if fields
                    .first()
                    .is_some_and(|(kind, _)| *kind == ArgType::DictEntry) =>
            {
                let mut entries = Vec::new();
                let mut signatures: Option<(Signature<'static>, Signature<'static>)> = None;
                for (kind, value) in fields {
                    if kind != ArgType::DictEntry {
                        return Err(container_type_error("D-Bus", value));
                    }
                    let entry_source = value;
                    let value =
                        if value.is_cons() && symbol_to_arg_type(value.cons_car()) == Some(kind) {
                            value.cons_cdr()
                        } else {
                            value
                        };
                    let fields = container_fields(value)?;
                    let [(key_type, key), (value_type, value)] = fields.as_slice() else {
                        return Err(match fields.get(2) {
                            Some((_, value)) => container_type_error("D-Bus", *value),
                            None => container_type_error("consp", Value::NIL),
                        });
                    };
                    if !is_basic(*key_type) {
                        return Err(container_type_error("D-Bus", *key));
                    }
                    let (key_signature, key) = PreparedBasic::prepare(*key_type, *key)?;
                    let value = Self::prepare(*value_type, *value, depth + 2)?;
                    if let Some((expected_key, expected_value)) = &signatures {
                        if *expected_key != key_signature || *expected_value != value.signature {
                            return Err(container_type_error("D-Bus", entry_source));
                        }
                    } else {
                        signatures = Some((key_signature, value.signature.clone()));
                    }
                    entries.push(PreparedEntry { key, value });
                }
                let (key_signature, value_signature) =
                    signatures.ok_or_else(|| dbus_error("Missing D-Bus dictionary signature"))?;
                Ok(Self {
                    signature: checked_signature(format!("a{{{key_signature}{value_signature}}}"))?,
                    body: PreparedBody::Dictionary {
                        key_signature,
                        value_signature,
                        entries,
                    },
                })
            }
            ArgType::Array => {
                let mut elements = Vec::new();
                for (kind, value) in fields {
                    let element_source = value;
                    let value = Self::prepare(kind, value, depth + 1)?;
                    if elements
                        .first()
                        .is_some_and(|first: &Self| first.signature != value.signature)
                    {
                        return Err(container_type_error("D-Bus", element_source));
                    }
                    elements.push(value);
                }
                let element_signature = match elements.first() {
                    Some(first) => first.signature.clone(),
                    None => checked_signature("s".to_owned())?,
                };
                Ok(Self {
                    signature: checked_signature(format!("a{element_signature}"))?,
                    body: PreparedBody::Array {
                        element_signature,
                        elements,
                    },
                })
            }
            ArgType::Variant => {
                let [(kind, value)] = fields.as_slice() else {
                    return Err(match fields.get(1) {
                        Some((_, value)) => container_type_error("D-Bus", *value),
                        None => container_type_error("consp", object),
                    });
                };
                Ok(Self {
                    signature: checked_signature("v".to_owned())?,
                    body: PreparedBody::Variant(Box::new(Self::prepare(*kind, *value, depth + 1)?)),
                })
            }
            ArgType::Struct => {
                let mut fields = fields.into_iter();
                let (kind, value) = fields
                    .next()
                    .ok_or_else(|| container_type_error("consp", object))?;
                let first = Box::new(Self::prepare(kind, value, depth + 1)?);
                let rest: Vec<Self> = fields
                    .map(|(kind, value)| Self::prepare(kind, value, depth + 1))
                    .collect::<Result<_, _>>()?;
                let mut signature = format!("({}", first.signature);
                for field in &rest {
                    signature.push_str(&field.signature);
                }
                signature.push(')');
                Ok(Self {
                    signature: checked_signature(signature)?,
                    body: PreparedBody::Struct { first, rest },
                })
            }
            ArgType::DictEntry => Err(container_type_error("D-Bus", source)),
            _ => Err(dbus_error("Invalid D-Bus type")),
        }
    }

    fn append(&self, iter: &mut IterAppend<'_>) {
        match &self.body {
            PreparedBody::Basic(value) => value.append(iter),
            PreparedBody::Array {
                element_signature,
                elements,
            } => iter.append_array(element_signature, |sub| {
                for element in elements {
                    element.append(sub);
                }
            }),
            PreparedBody::Dictionary {
                key_signature,
                value_signature,
                entries,
            } => iter.append_dict(key_signature, value_signature, |sub| {
                for entry in entries {
                    sub.append_dict_entry(|sub| {
                        entry.key.append(sub);
                        entry.value.append(sub);
                    });
                }
            }),
            PreparedBody::Struct { first, rest } => iter.append_struct(|sub| {
                first.append(sub);
                for field in rest {
                    field.append(sub);
                }
            }),
            PreparedBody::Variant(value) => {
                iter.append_variant(&value.signature, |sub| value.append(sub))
            }
        }
    }
}

impl PreparedBasic {
    fn prepare(dtype: ArgType, object: Value) -> Result<(Signature<'static>, Self), Flow> {
        let (signature, value) = match dtype {
            ArgType::Byte => ("y", Self::Byte(unsigned(object, u8::MAX as u64)? as u8)),
            ArgType::Boolean => ("b", Self::Boolean(object.is_truthy())),
            ArgType::Int16 => (
                "n",
                Self::Int16(signed(object, i16::MIN as i64, i16::MAX as i64)? as i16),
            ),
            ArgType::UInt16 => ("q", Self::UInt16(unsigned(object, u16::MAX as u64)? as u16)),
            ArgType::Int32 => (
                "i",
                Self::Int32(signed(object, i32::MIN as i64, i32::MAX as i64)? as i32),
            ),
            ArgType::UInt32 => ("u", Self::UInt32(unsigned(object, u32::MAX as u64)? as u32)),
            ArgType::Int64 => ("x", Self::Int64(signed(object, i64::MIN, i64::MAX)?)),
            ArgType::UInt64 => ("t", Self::UInt64(unsigned(object, u64::MAX)?)),
            ArgType::Double => (
                "d",
                Self::Double(object.as_float().ok_or_else(|| {
                    signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("numberp"), object],
                    )
                })?),
            ),
            ArgType::String => ("s", Self::String(string_arg(object)?)),
            ArgType::ObjectPath => (
                "o",
                Self::ObjectPath(
                    DbusPath::new(string_arg(object)?).map_err(|err| dbus_error(&err))?,
                ),
            ),
            ArgType::Signature => (
                "g",
                Self::Signature(checked_signature(string_arg(object)?)?),
            ),
            ArgType::UnixFd => {
                use std::os::fd::FromRawFd;
                let fd = unsigned(object, i32::MAX as u64)? as i32;
                // A successful duplication establishes ownership without taking
                // ownership of the Lisp caller's descriptor.
                let duplicate = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0) };
                if duplicate < 0 {
                    return Err(dbus_error("Invalid D-Bus file descriptor"));
                }
                (
                    "h",
                    Self::UnixFd(unsafe { std::fs::File::from_raw_fd(duplicate) }),
                )
            }
            _ => return Err(dbus_error("D-Bus value is not basic")),
        };
        Ok((checked_signature(signature.to_owned())?, value))
    }

    fn append(&self, iter: &mut IterAppend<'_>) {
        match self {
            Self::Byte(value) => iter.append(*value),
            Self::Boolean(value) => iter.append(*value),
            Self::Int16(value) => iter.append(*value),
            Self::UInt16(value) => iter.append(*value),
            Self::Int32(value) => iter.append(*value),
            Self::UInt32(value) => iter.append(*value),
            Self::Int64(value) => iter.append(*value),
            Self::UInt64(value) => iter.append(*value),
            Self::Double(value) => iter.append(*value),
            Self::String(value) => iter.append(value.as_str()),
            Self::ObjectPath(value) => iter.append(value),
            Self::Signature(value) => iter.append(value),
            Self::UnixFd(value) => iter.append(value),
        }
    }
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
            let mut inner = iter
                .recurse(arg_type)
                .ok_or_else(|| dbus_error("Cannot read container"))?;
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
    let string = value.as_utf8_str().map(str::to_owned).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), value],
        )
    })?;
    if string.contains('\0') {
        return Err(dbus_error("D-Bus string contains NUL"));
    }
    Ok(string)
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
