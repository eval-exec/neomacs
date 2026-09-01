//! Shared Lisp decoder for named visual-effect operations.
//!
//! Global profiles and per-buffer cursor profiles intentionally share this
//! boundary parser.  Callers choose the allowed effect scope and whether
//! parse failures become Lisp conditions or an ignored buffer-local value.

use crate::emacs_core::{Value, value::list_to_vec};
use neomacs_display_protocol::{EffectOperation, EffectValue};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectProfileError {
    message: String,
}

impl EffectProfileError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for EffectProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for EffectProfileError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EffectScope {
    #[default]
    All,
    Cursor,
}

impl EffectScope {
    fn validate(self, name: &str) -> Result<(), EffectProfileError> {
        match self {
            Self::All => Ok(()),
            Self::Cursor if name.starts_with("cursor-") => Ok(()),
            Self::Cursor => Err(EffectProfileError::new(format!(
                "effect `{name}` is not valid in a cursor profile"
            ))),
        }
    }
}

pub fn effect_name_from_lisp(
    value: Value,
    scope: EffectScope,
) -> Result<String, EffectProfileError> {
    let name = value
        .as_symbol_name()
        .filter(|name| !name.starts_with(':'))
        .ok_or_else(|| {
            EffectProfileError::new(format!(
                "effect names must be non-keyword symbols, got {value}"
            ))
        })?;
    scope.validate(name)?;
    Ok(name.to_owned())
}

pub fn effect_set_operation_from_lisp(
    effect: Value,
    properties: &[Value],
    scope: EffectScope,
) -> Result<EffectOperation, EffectProfileError> {
    if !properties.len().is_multiple_of(2) {
        return Err(EffectProfileError::new(
            "effect properties must be keyword/value pairs",
        ));
    }
    let properties = properties
        .chunks_exact(2)
        .map(|pair| {
            Ok((
                property_name_from_lisp(pair[0])?,
                effect_value_from_lisp(pair[1])?,
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(EffectOperation::set(
        effect_name_from_lisp(effect, scope)?,
        properties,
    ))
}

pub fn effect_operation_from_lisp(
    form: Value,
    scope: EffectScope,
) -> Result<EffectOperation, EffectProfileError> {
    let values = if form.is_cons() {
        list_to_vec(&form).ok_or_else(|| {
            EffectProfileError::new(
                "each profile entry must be a proper (EFFECT :PROPERTY VALUE ...) list",
            )
        })?
    } else {
        vec![form]
    };
    let (effect, properties) = values
        .split_first()
        .ok_or_else(|| EffectProfileError::new("profile entries cannot be empty"))?;
    effect_set_operation_from_lisp(*effect, properties, scope)
}

pub fn effect_operations_from_lisp(
    profile: Value,
    scope: EffectScope,
) -> Result<Vec<EffectOperation>, EffectProfileError> {
    let entries = list_to_vec(&profile).ok_or_else(|| {
        EffectProfileError::new("profile must be a proper list of effect entries")
    })?;
    entries
        .into_iter()
        .map(|entry| effect_operation_from_lisp(entry, scope))
        .collect()
}

fn property_name_from_lisp(value: Value) -> Result<String, EffectProfileError> {
    value
        .as_symbol_name()
        .and_then(|name| name.strip_prefix(':'))
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            EffectProfileError::new(format!(
                "effect property names must be keywords, got {value}"
            ))
        })
}

fn effect_value_from_lisp(value: Value) -> Result<EffectValue, EffectProfileError> {
    if value.is_nil() {
        return Ok(EffectValue::Bool(false));
    }
    if value == Value::T {
        return Ok(EffectValue::Bool(true));
    }
    if let Some(integer) = value.as_int() {
        return Ok(EffectValue::Integer(integer));
    }
    if let Some(number) = value.as_float() {
        return Ok(EffectValue::Number(number));
    }
    if let Some(string) = value.as_utf8_str() {
        return Ok(EffectValue::String(string.to_owned()));
    }
    if let Some(symbol) = value.as_symbol_name().filter(|name| !name.starts_with(':')) {
        return Ok(EffectValue::Symbol(symbol.to_owned()));
    }
    if let Some(items) = list_to_vec(&value) {
        return items
            .into_iter()
            .map(effect_value_from_lisp)
            .collect::<Result<Vec<_>, _>>()
            .map(EffectValue::List);
    }
    Err(EffectProfileError::new(format!(
        "unsupported effect value {value}"
    )))
}
