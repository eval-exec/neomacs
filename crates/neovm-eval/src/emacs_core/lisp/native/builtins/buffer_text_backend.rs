use super::*;
use neovm_edit::buffer::BufferTextBackendKind;
use neovm_edit::buffer::text::ImplementedBufferTextBackendKind;
use neovm_value::error::expect_args;

fn value_to_buffer_text_backend_kind(
    value: Value,
    mode: neovm_value::tagged::SymbolMode,
) -> Result<ImplementedBufferTextBackendKind, Flow> {
    let symbol = super::symbols::expect_symbol_id(&value, mode)?;
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

#[doc(hidden)]
pub fn builtin_neomacs_buffer_text_backend(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("neomacs-buffer-text-backend", &args, 0)?;
    let buffer = ctx
        .edit
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    Ok(buffer_text_backend_kind_value(buffer.text_backend_kind()))
}

#[doc(hidden)]
pub fn builtin_neomacs_default_buffer_text_backend(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("neomacs-default-buffer-text-backend", &args, 0)?;
    Ok(buffer_text_backend_kind_value(
        ctx.edit.buffers.default_text_backend_kind(),
    ))
}

#[doc(hidden)]
pub fn builtin_neomacs_set_default_buffer_text_backend(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("neomacs-set-default-buffer-text-backend", &args, 1)?;
    let kind = value_to_buffer_text_backend_kind(args[0], ctx.symbol_mode())?;
    ctx.edit.buffers.set_default_text_backend_kind(kind);
    Ok(buffer_text_backend_kind_value(kind.public_kind()))
}

#[doc(hidden)]
pub fn builtin_neomacs_set_buffer_text_backend(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("neomacs-set-buffer-text-backend", &args, 1)?;
    let kind = value_to_buffer_text_backend_kind(args[0], ctx.symbol_mode())?;
    let buffer = ctx
        .edit
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buffer.convert_text_backend_kind(kind);
    Ok(buffer_text_backend_kind_value(kind.public_kind()))
}
