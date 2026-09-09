//! Keyboard equivalents are resolved in the evaluator's active keymaps.
use super::{Context, Flow, MenuRoots, Value};

#[derive(Default)]
pub(super) struct Hints {
    pub explicit: Option<Value>,
    pub sequence: Option<Value>,
}

pub(super) fn resolve(
    ctx: &mut Context,
    command: Value,
    hints: Hints,
    roots: &MenuRoots,
) -> Result<String, Flow> {
    if hints.sequence.is_none()
        && let Some(text) = hints.explicit.filter(|value| value.is_string())
    {
        let text = ctx.funcall_general(Value::symbol("substitute-command-keys"), vec![text])?;
        return Ok(text.as_runtime_string_owned().unwrap_or_default());
    }
    let (command, affixes) = match hints.explicit.filter(|value| value.is_cons()) {
        Some(spec) => (spec.cons_car(), Some(spec.cons_cdr())),
        None => (command, None),
    };
    let mut keys = Value::NIL;
    if let Some(hint) = hints.sequence.filter(|value| !value.is_nil()) {
        let binding = crate::emacs_core::interactive::builtin_key_binding(ctx, vec![hint])?;
        let alias = command
            .as_symbol_name()
            .and_then(|name| ctx.obarray().symbol_function(name));
        if !binding.is_nil() && (binding == command || alias == Some(binding)) {
            keys = hint;
        }
    }
    if keys.is_nil() {
        keys = crate::emacs_core::interactive::builtin_where_is_internal(
            ctx,
            vec![command, Value::NIL, Value::T, Value::NIL, Value::NIL],
        )?;
    }
    if keys.is_nil() {
        return Ok(String::new());
    }
    roots.keep(keys);
    let description = ctx.funcall_general(Value::symbol("key-description"), vec![keys])?;
    let mut description = description.as_runtime_string_owned().unwrap_or_default();
    if let Some(affixes) = affixes.filter(|value| value.is_cons()) {
        if let Some(prefix) = affixes.cons_car().as_runtime_string_owned() {
            description.insert_str(0, &prefix);
        }
        if let Some(suffix) = affixes.cons_cdr().as_runtime_string_owned() {
            description.push_str(&suffix);
        }
    }
    Ok(description)
}
