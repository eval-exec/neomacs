use super::*;
use crate::buffer::BufferTextBackendKind;
use crate::buffer::text::ImplementedBufferTextBackendKind;
use crate::emacs_core::error::expect_args;

fn value_to_buffer_text_backend_kind(
    value: Value,
) -> Result<ImplementedBufferTextBackendKind, Flow> {
    let symbol = super::symbols::expect_symbol_id(&value)?;
    let name = resolve_sym(symbol);
    let kind = name.parse::<BufferTextBackendKind>().map_err(|_| {
        signal(
            "error",
            vec![Value::string(format!(
                "Unknown buffer text backend: {name}"
            ))],
        )
    })?;
    kind.implemented().ok_or_else(|| {
        signal(
            "error",
            vec![Value::string(format!(
                "Unimplemented buffer text backend: {}",
                kind.symbol_name()
            ))],
        )
    })
}

fn buffer_text_backend_kind_value(kind: BufferTextBackendKind) -> Value {
    Value::symbol(kind.symbol_name())
}

pub(crate) fn builtin_neomacs_buffer_text_backend(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("neomacs-buffer-text-backend", &args, 0)?;
    let buffer = ctx
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    Ok(buffer_text_backend_kind_value(buffer.text_backend_kind()))
}

pub(crate) fn builtin_neomacs_default_buffer_text_backend(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("neomacs-default-buffer-text-backend", &args, 0)?;
    Ok(buffer_text_backend_kind_value(
        ctx.buffers.default_text_backend_kind(),
    ))
}

pub(crate) fn builtin_neomacs_set_default_buffer_text_backend(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("neomacs-set-default-buffer-text-backend", &args, 1)?;
    let kind = value_to_buffer_text_backend_kind(args[0])?;
    ctx.buffers.set_default_text_backend_kind(kind);
    Ok(buffer_text_backend_kind_value(kind.public_kind()))
}

pub(crate) fn builtin_neomacs_set_buffer_text_backend(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("neomacs-set-buffer-text-backend", &args, 1)?;
    let kind = value_to_buffer_text_backend_kind(args[0])?;
    let buffer = ctx
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buffer.convert_text_backend_kind(kind);
    Ok(buffer_text_backend_kind_value(kind.public_kind()))
}
