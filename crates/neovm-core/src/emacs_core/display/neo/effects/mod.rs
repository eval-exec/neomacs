//! Lisp interface to the typed renderer-effect registry.

mod subrs;
#[cfg(test)]
pub(crate) use subrs::SUBRS;
pub(crate) use subrs::register_subrs;

use crate::emacs_core::effect_profile::{
    EffectScope, effect_name_from_lisp, effect_operations_from_lisp, effect_set_operation_from_lisp,
};
use crate::emacs_core::error::{
    EvalResult, Flow, expect_args, expect_args_range, expect_min_args, signal,
};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;
use neomacs_display_protocol::{EffectOperation, EffectValue, VisualConfig};

fn effect_error(function: &str, message: impl std::fmt::Display) -> Flow {
    signal(
        "error",
        vec![Value::string(format!("{function}: {message}"))],
    )
}

fn publish_visual_config(
    eval: &mut crate::emacs_core::eval::Context,
    function: &str,
    updated: VisualConfig,
) -> EvalResult {
    if let Some(host) = eval.display_host.as_mut() {
        host.set_visual_config(updated.clone())
            .map_err(|error| effect_error(function, error))?;
    }
    eval.visual_config = updated;
    Ok(Value::NIL)
}

fn apply_operations(
    eval: &mut crate::emacs_core::eval::Context,
    function: &str,
    base: &VisualConfig,
    operations: &[EffectOperation],
) -> EvalResult {
    let updated = base
        .apply_effects(operations)
        .map_err(|error| effect_error(function, error))?;
    publish_visual_config(eval, function, updated)
}

fn set(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("neomacs-effect-set", &args, 1)?;
    let operation = effect_set_operation_from_lisp(args[0], &args[1..], EffectScope::All)
        .map_err(|error| effect_error("neomacs-effect-set", error))?;
    let base = eval.visual_config.clone();
    apply_operations(eval, "neomacs-effect-set", &base, &[operation])
}

fn get(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("neomacs-effect-get", &args, 1)?;
    let effect = effect_name_from_lisp(args[0], EffectScope::All)
        .map_err(|error| effect_error("neomacs-effect-get", error))?;
    let properties = eval
        .visual_config
        .effect_values(&effect)
        .map_err(|error| effect_error("neomacs-effect-get", error))?;
    let mut result = Vec::with_capacity(properties.len() * 2);
    for (name, value) in properties {
        result.push(Value::keyword(format!(":{name}")));
        result.push(value_to_lisp(value));
    }
    Ok(Value::list(result))
}

/// `(neomacs-effect-schema EFFECT)` — what each property of EFFECT accepts.
///
/// Returns one entry per property, `(:PROPERTY KIND . ALLOWED)`, where ALLOWED
/// is present only for a symbol-valued property:
///
///     (neomacs-effect-schema 'window-resize)
///     => ((:enabled boolean)
///         (:kind symbol easing spring)
///         (:duration seconds)
///         (:stiffness integer)
///         (:damping-ratio number)
///         ...)
///
/// This exists so Lisp can build a real customization widget. `effect-get`
/// answers with values, from which only the current shape can be guessed — a
/// float that happens to be 1.0 reads as an integer, and nothing says that
/// `opacity` is clamped to the unit interval or which symbols `easing` accepts.
/// That is why the effect profile has been a `sexp` field rather than a typed
/// option.
fn schema(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    use neomacs_display_protocol::effect_config::PropertyKind;
    expect_args("neomacs-effect-schema", &args, 1)?;
    let effect = effect_name_from_lisp(args[0], EffectScope::All)
        .map_err(|error| effect_error("neomacs-effect-schema", error))?;
    // Ask the registry first, so an effect that exists but publishes no schema
    // is a loud error rather than an empty list a caller would read as "no
    // properties".
    eval.visual_config
        .effect_values(&effect)
        .map_err(|error| effect_error("neomacs-effect-schema", error))?;
    let Some(schema) = neomacs_display_protocol::effect_config::schema_for(&effect) else {
        return Err(effect_error(
            "neomacs-effect-schema",
            format!("effect `{effect}` publishes no schema"),
        ));
    };
    let entries = schema
        .iter()
        .map(|property| {
            let refined = property.refined();
            let mut entry = vec![
                Value::keyword(format!(":{}", property.property.replace('_', "-"))),
                Value::symbol(match refined.kind {
                    PropertyKind::Boolean => "boolean",
                    PropertyKind::Integer => "integer",
                    PropertyKind::Number => "number",
                    PropertyKind::UnitInterval => "unit-interval",
                    PropertyKind::Percentage => "percentage",
                    PropertyKind::Color => "color",
                    PropertyKind::ColorList => "color-list",
                    PropertyKind::Seconds => "seconds",
                    PropertyKind::Symbol(_) => "symbol",
                }),
            ];
            if let PropertyKind::Symbol(allowed) = refined.kind {
                entries_push_allowed(&mut entry, allowed);
            }
            Value::list(entry)
        })
        .collect::<Vec<_>>();
    Ok(Value::list(entries))
}

fn entries_push_allowed(entry: &mut Vec<Value>, allowed: &[&str]) {
    for name in allowed {
        entry.push(Value::symbol(name));
    }
}

fn reset(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("neomacs-effect-reset", &args, 1)?;
    let effect = effect_name_from_lisp(args[0], EffectScope::All)
        .map_err(|error| effect_error("neomacs-effect-reset", error))?;
    let base = eval.visual_config.clone();
    apply_operations(
        eval,
        "neomacs-effect-reset",
        &base,
        &[EffectOperation::reset(effect)],
    )
}

fn apply(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("neomacs-effects-apply", &args, 1)?;
    let operations = effect_operations_from_lisp(args[0], EffectScope::All)
        .map_err(|error| effect_error("neomacs-effects-apply", error))?;
    apply_operations(
        eval,
        "neomacs-effects-apply",
        &VisualConfig::default(),
        &operations,
    )
}

fn names(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("neomacs-effect-names", &args, 0, 1)?;
    let names = match args.first() {
        None => eval.visual_config.effect_names(),
        Some(value) if value.is_nil() => eval.visual_config.effect_names(),
        Some(value) => match value.as_symbol_name() {
            Some("shader") => eval.visual_config.effects.effect_names(),
            Some("cursor") => eval
                .visual_config
                .effects
                .effect_names()
                .into_iter()
                .filter(|name| name.starts_with("cursor-"))
                .collect(),
            Some("behavior") => {
                let shader_names = eval.visual_config.effects.effect_names();
                eval.visual_config
                    .effect_names()
                    .into_iter()
                    .filter(|name| !shader_names.contains(name))
                    .collect()
            }
            _ => {
                return Err(effect_error(
                    "neomacs-effect-names",
                    "scope must be nil, `shader`, `cursor`, or `behavior`",
                ));
            }
        },
    };
    Ok(Value::list(names.into_iter().map(Value::symbol).collect()))
}

fn value_to_lisp(value: EffectValue) -> Value {
    match value {
        EffectValue::Bool(false) => Value::NIL,
        EffectValue::Bool(true) => Value::T,
        EffectValue::Integer(value) => Value::fixnum(value),
        EffectValue::Number(value) => Value::make_float(value),
        EffectValue::Symbol(value) => Value::symbol(value),
        EffectValue::String(value) => Value::string(value),
        EffectValue::List(values) => Value::list(values.into_iter().map(value_to_lisp).collect()),
    }
}
