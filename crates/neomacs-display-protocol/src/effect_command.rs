//! Typed, name-based configuration protocol for renderer effects.
//!
//! `VisualConfig` is the complete control-plane snapshot while
//! `EffectsConfig` remains the renderer's strongly typed shader storage.
//! This module derives both registries from their Serde shapes, so a new
//! config or property does not require a second match table or positional
//! argument decoder.

use crate::{EffectsConfig, VisualConfig, types::Color};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Number, Value as JsonValue};
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum EffectValue {
    Bool(bool),
    Integer(i64),
    Number(f64),
    Symbol(String),
    String(String),
    List(Vec<EffectValue>),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EffectName(String);

impl EffectName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for EffectName {
    fn from(name: String) -> Self {
        Self(name)
    }
}

impl From<&str> for EffectName {
    fn from(name: &str) -> Self {
        Self(name.to_owned())
    }
}

impl fmt::Display for EffectName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EffectPropertyName(String);

impl EffectPropertyName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for EffectPropertyName {
    fn from(name: String) -> Self {
        Self(name)
    }
}

impl From<&str> for EffectPropertyName {
    fn from(name: &str) -> Self {
        Self(name.to_owned())
    }
}

impl fmt::Display for EffectPropertyName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum EffectOperation {
    Set {
        effect: EffectName,
        properties: Vec<(EffectPropertyName, EffectValue)>,
    },
    Reset {
        effect: EffectName,
    },
}

impl EffectOperation {
    pub fn set<E, P, I>(effect: E, properties: I) -> Self
    where
        E: Into<EffectName>,
        P: Into<EffectPropertyName>,
        I: IntoIterator<Item = (P, EffectValue)>,
    {
        Self::Set {
            effect: effect.into(),
            properties: properties
                .into_iter()
                .map(|(name, value)| (name.into(), value))
                .collect(),
        }
    }

    pub fn reset(effect: impl Into<EffectName>) -> Self {
        Self::Reset {
            effect: effect.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectConfigError {
    message: String,
}

impl EffectConfigError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for EffectConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for EffectConfigError {}

macro_rules! impl_effect_registry {
    ($config:ty) => {
        impl $config {
            /// Apply all operations to a new snapshot.  The receiver is never
            /// mutated, so callers publish only after every operation validates.
            pub fn apply_effects(
                &self,
                operations: &[EffectOperation],
            ) -> Result<Self, EffectConfigError> {
                apply_effects(self, operations)
            }

            pub fn effect_names(&self) -> Vec<String> {
                effect_names(self)
            }

            pub fn effect_values(
                &self,
                effect: &str,
            ) -> Result<Vec<(String, EffectValue)>, EffectConfigError> {
                effect_values(self, effect)
            }
        }
    };
}

impl_effect_registry!(EffectsConfig);
impl_effect_registry!(VisualConfig);

impl EffectsConfig {
    /// Baseline for a buffer/window-local cursor profile: every cursor effect
    /// with an `enabled` property starts disabled, independent of its global
    /// Rust default.  The selected profile then enables only named entries.
    pub fn cursor_profile_baseline() -> Self {
        let defaults = Self::default();
        let disable = defaults
            .effect_names()
            .into_iter()
            .filter(|name| name.starts_with("cursor-"))
            .filter(|name| {
                defaults.effect_values(name).is_ok_and(|properties| {
                    properties.iter().any(|(property, _)| property == "enabled")
                })
            })
            .map(|name| EffectOperation::set(name, [("enabled", EffectValue::Bool(false))]))
            .collect::<Vec<_>>();
        defaults
            .apply_effects(&disable)
            .expect("cursor profile baseline is generated from the effect registry")
    }
}

fn apply_effects<T>(config: &T, operations: &[EffectOperation]) -> Result<T, EffectConfigError>
where
    T: Clone + Default + Serialize + DeserializeOwned,
{
    let mut current = config_json(config)?;
    let defaults = config_json(&T::default())?;

    for operation in operations {
        match operation {
            EffectOperation::Set { effect, properties } => {
                let effect_key = rust_name(effect.as_str());
                let config = effect_object_mut(&mut current, effect.as_str(), &effect_key)?;
                for (property, value) in properties {
                    let property_key = rust_name(property.as_str());
                    let Some(target) = config.get_mut(&property_key) else {
                        return Err(EffectConfigError::new(format!(
                            "effect `{effect}` has no property `{property}`"
                        )));
                    };
                    *target =
                        value_for_target(&property_key, value, target).map_err(|expected| {
                            EffectConfigError::new(format!(
                                "effect `{effect}` property `{property}` expects {expected}"
                            ))
                        })?;
                }
            }
            EffectOperation::Reset { effect } => {
                let effect_key = rust_name(effect.as_str());
                let default = effect_object(&defaults, effect.as_str(), &effect_key)?.clone();
                let current_map = current.as_object_mut().expect("visual config is an object");
                current_map.insert(effect_key, JsonValue::Object(default));
            }
        }
    }

    serde_json::from_value(current)
        .map_err(|error| EffectConfigError::new(format!("invalid effect configuration: {error}")))
}

fn effect_names(config: &impl Serialize) -> Vec<String> {
    config_json(config)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .map(|effects| effects.keys().map(|name| lisp_name(name)).collect())
        .unwrap_or_default()
}

fn effect_values(
    config: &impl Serialize,
    effect: &str,
) -> Result<Vec<(String, EffectValue)>, EffectConfigError> {
    let effects = config_json(config)?;
    let effect_key = rust_name(effect);
    let values = effect_object(&effects, effect, &effect_key)?;
    values
        .iter()
        .map(|(name, value)| {
            Ok((
                lisp_name(name),
                effect_value_from_json(value).map_err(|kind| {
                    EffectConfigError::new(format!(
                        "effect `{effect}` property `{}` has unsupported {kind} storage",
                        lisp_name(name)
                    ))
                })?,
            ))
        })
        .collect()
}

fn config_json(config: &impl Serialize) -> Result<JsonValue, EffectConfigError> {
    serde_json::to_value(config).map_err(|error| {
        EffectConfigError::new(format!("cannot encode effect configuration: {error}"))
    })
}

fn effect_object<'a>(
    effects: &'a JsonValue,
    public_name: &str,
    storage_name: &str,
) -> Result<&'a Map<String, JsonValue>, EffectConfigError> {
    effects
        .as_object()
        .and_then(|all| all.get(storage_name))
        .and_then(JsonValue::as_object)
        .ok_or_else(|| EffectConfigError::new(format!("unknown effect `{public_name}`")))
}

fn effect_object_mut<'a>(
    effects: &'a mut JsonValue,
    public_name: &str,
    storage_name: &str,
) -> Result<&'a mut Map<String, JsonValue>, EffectConfigError> {
    effects
        .as_object_mut()
        .and_then(|all| all.get_mut(storage_name))
        .and_then(JsonValue::as_object_mut)
        .ok_or_else(|| EffectConfigError::new(format!("unknown effect `{public_name}`")))
}

fn rust_name(name: &str) -> String {
    name.replace('-', "_")
}

fn lisp_name(name: &str) -> String {
    name.replace('_', "-")
}

fn value_for_target(
    property: &str,
    value: &EffectValue,
    target: &JsonValue,
) -> Result<JsonValue, &'static str> {
    match target {
        JsonValue::Bool(_) => match value {
            EffectValue::Bool(value) => Ok(JsonValue::Bool(*value)),
            _ => Err("a boolean"),
        },
        JsonValue::Number(number) => number_for_target(property, value, number),
        JsonValue::String(_) => match value {
            EffectValue::Symbol(value) => Ok(JsonValue::String(value.clone())),
            _ => Err("a symbol"),
        },
        JsonValue::Array(items) if is_color(items) => match value {
            EffectValue::String(value) => parse_color(value, items.len())
                .map(JsonValue::Array)
                .ok_or("a #RRGGBB or #RRGGBBAA color"),
            _ => Err("a color"),
        },
        JsonValue::Array(_) if property == "rainbow_colors" => match value {
            EffectValue::List(values) => values
                .iter()
                .map(|value| match value {
                    EffectValue::String(color) => parse_color(color, 4)
                        .map(JsonValue::Array)
                        .ok_or("a list of #RRGGBB or #RRGGBBAA colors"),
                    _ => Err("a list of #RRGGBB or #RRGGBBAA colors"),
                })
                .collect::<Result<Vec<_>, _>>()
                .map(JsonValue::Array),
            _ => Err("a list of colors"),
        },
        JsonValue::Array(items) => match value {
            EffectValue::List(values) => {
                list_for_target(property, values, items).map(JsonValue::Array)
            }
            _ => Err("a list"),
        },
        JsonValue::Object(object) if is_duration(object) => match value {
            EffectValue::Number(seconds) if seconds.is_finite() && *seconds >= 0.0 => {
                let whole = seconds.trunc() as u64;
                let nanos = ((seconds.fract()) * 1_000_000_000.0).round() as u64;
                Ok(
                    serde_json::json!({ "secs": whole + nanos / 1_000_000_000, "nanos": nanos % 1_000_000_000 }),
                )
            }
            EffectValue::Integer(seconds) if *seconds >= 0 => Ok(serde_json::json!({
                "secs": *seconds as u64,
                "nanos": 0
            })),
            _ => Err("a non-negative number of seconds"),
        },
        JsonValue::Object(_) => Err("an object"),
        JsonValue::Null => Err("a non-null value"),
    }
}

fn number_for_target(
    property: &str,
    value: &EffectValue,
    target: &Number,
) -> Result<JsonValue, &'static str> {
    let value = match value {
        EffectValue::Number(value) => *value,
        EffectValue::Integer(value) => *value as f64,
        _ => {
            return Err(if target.is_f64() {
                "a number"
            } else {
                "an integer"
            });
        }
    };
    if !value.is_finite() {
        return Err("a finite number");
    }
    if target.is_f64() && value.abs() > f64::from(f32::MAX) {
        return Err("a number representable by the renderer");
    }
    if is_unit_interval(property) && !(0.0..=1.0).contains(&value) {
        return Err("a number between 0 and 1");
    }
    if property == "fps" && (value <= 0.0 || value.fract() != 0.0) {
        return Err("a positive integer");
    }
    if value < 0.0 && !allows_negative(property) {
        return Err("a non-negative number");
    }
    if property.ends_with("_pct") && value > 100.0 {
        return Err("a percentage between 0 and 100");
    }
    if target.is_u64() {
        if value.fract() != 0.0 || value < 0.0 || value > u64::MAX as f64 {
            return Err("a non-negative integer");
        }
        return Ok(JsonValue::Number(Number::from(value as u64)));
    }
    if target.is_i64() {
        if value.fract() != 0.0 || value < i64::MIN as f64 || value > i64::MAX as f64 {
            return Err("an integer");
        }
        return Ok(JsonValue::Number(Number::from(value as i64)));
    }
    Number::from_f64(value)
        .map(JsonValue::Number)
        .ok_or("a finite number")
}

fn allows_negative(property: &str) -> bool {
    matches!(
        property,
        "angle" | "angle_offset" | "offset" | "offset_x" | "offset_y" | "rotation" | "row_offset"
    )
}

fn is_unit_interval(property: &str) -> bool {
    property.contains("opacity")
        || matches!(
            property,
            "flicker" | "lightness" | "saturation" | "thumb_radius" | "trail_size"
        )
}

fn list_for_target(
    property: &str,
    values: &[EffectValue],
    targets: &[JsonValue],
) -> Result<Vec<JsonValue>, &'static str> {
    if values.len() != targets.len() {
        return Err("a list with the configured number of elements");
    }
    values
        .iter()
        .zip(targets)
        .map(|(value, target)| value_for_target(property, value, target))
        .collect()
}

fn is_color(items: &[JsonValue]) -> bool {
    matches!(items.len(), 3 | 4) && items.iter().all(JsonValue::is_number)
}

fn is_duration(object: &Map<String, JsonValue>) -> bool {
    object.len() == 2 && object.contains_key("secs") && object.contains_key("nanos")
}

fn parse_color(text: &str, components: usize) -> Option<Vec<JsonValue>> {
    let hex = text.strip_prefix('#')?;
    let requested_components = match hex.len() {
        6 => 3,
        8 => 4,
        _ => return None,
    };
    if requested_components > components {
        return None;
    }
    let bytes = (0..requested_components)
        .map(|index| u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).ok())
        .collect::<Option<Vec<_>>>()?;
    let linear = Color::from_u8(
        bytes[0],
        bytes[1],
        bytes[2],
        bytes.get(3).copied().unwrap_or(u8::MAX),
    )
    .srgb_to_linear();
    [linear.r, linear.g, linear.b, linear.a]
        .into_iter()
        .take(components)
        .map(|component| Number::from_f64(f64::from(component)).map(JsonValue::Number))
        .collect()
}

fn effect_value_from_json(value: &JsonValue) -> Result<EffectValue, &'static str> {
    match value {
        JsonValue::Bool(value) => Ok(EffectValue::Bool(*value)),
        JsonValue::Number(value) if value.is_i64() => {
            value.as_i64().map(EffectValue::Integer).ok_or("integer")
        }
        JsonValue::Number(value) if value.is_u64() => value
            .as_u64()
            .and_then(|value| i64::try_from(value).ok())
            .map(EffectValue::Integer)
            .ok_or("integer"),
        JsonValue::Number(value) => value.as_f64().map(EffectValue::Number).ok_or("number"),
        JsonValue::String(value) => Ok(EffectValue::Symbol(value.clone())),
        JsonValue::Array(items) if is_color(items) => {
            let components = items
                .iter()
                .map(|item| item.as_f64().map(|value| value as f32).ok_or("color"))
                .collect::<Result<Vec<_>, _>>()?;
            let srgb = Color::new(
                components[0],
                components[1],
                components[2],
                components.get(3).copied().unwrap_or(1.0),
            )
            .linear_to_srgb();
            let mut text = String::from("#");
            for component in [srgb.r, srgb.g, srgb.b, srgb.a]
                .into_iter()
                .take(items.len())
            {
                text.push_str(&format!(
                    "{:02X}",
                    (component.clamp(0.0, 1.0) * 255.0).round() as u8
                ));
            }
            Ok(EffectValue::String(text))
        }
        JsonValue::Array(items) => items
            .iter()
            .map(effect_value_from_json)
            .collect::<Result<Vec<_>, _>>()
            .map(EffectValue::List),
        JsonValue::Object(object) if is_duration(object) => {
            let secs = object
                .get("secs")
                .and_then(JsonValue::as_f64)
                .ok_or("duration")?;
            let nanos = object
                .get("nanos")
                .and_then(JsonValue::as_f64)
                .ok_or("duration")?;
            Ok(EffectValue::Number(secs + nanos / 1_000_000_000.0))
        }
        JsonValue::Object(_) => Err("object"),
        JsonValue::Null => Err("null"),
    }
}
