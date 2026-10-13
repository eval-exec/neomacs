use super::*;

#[test]
fn same_buffer_excursion_finish_does_not_allocate_a_vm_root_frame() {
    use crate::buffer::EmacsBytePos;

    let mut ctx = Context::new();
    let buffer = ctx.buffers.current_buffer_id().expect("current buffer");
    ctx.buffers.replace_buffer_contents(buffer, "abcd").unwrap();
    ctx.buffers
        .goto_buffer_emacs_byte_pos(buffer, EmacsBytePos::new(1))
        .unwrap();
    let count = ctx.specpdl.len();
    ctx.vm_root_frames.clear();
    ctx.vm_root_frames.shrink_to_fit();
    let mut scope = ExcursionScope::enter(&mut ctx);
    scope
        .context()
        .buffers
        .goto_buffer_emacs_byte_pos(buffer, EmacsBytePos::new(3))
        .unwrap();
    scope.finish(Ok(Value::NIL)).expect("scope finish");
    assert_eq!(ctx.specpdl.len(), count);
    assert_eq!(
        ctx.buffers.get(buffer).unwrap().point_emacs_byte_pos(),
        EmacsBytePos::new(1)
    );
    assert!(ctx.vm_root_frames.is_empty());
    assert_eq!(
        ctx.vm_root_frames.capacity(),
        0,
        "restoring an existing point without switching buffer or window needs no root allocation"
    );
}

/// GNU's same-buffer restore is a no-op. A native saved-buffer scope needs no
/// temporary VM root frame when its child suffix consists only of stores.
#[test]
fn same_buffer_scope_finish_does_not_allocate_a_vm_root_frame() {
    use crate::emacs_core::eval::CurrentBufferScope;
    use crate::emacs_core::intern::intern;

    let mut ctx = Context::new();
    let symbol = intern("same-buffer-scope-child");
    ctx.obarray.set_symbol_value_id(symbol, Value::fixnum(10));
    let result = Value::string("retained scope result");
    let count = ctx.specpdl.len();
    ctx.vm_root_frames.clear();
    ctx.vm_root_frames.shrink_to_fit();
    assert_eq!(ctx.vm_root_frames.capacity(), 0);

    let mut scope = CurrentBufferScope::enter(&mut ctx);
    scope
        .context()
        .try_specbind(symbol, Value::fixnum(20))
        .expect("plain child binding");
    assert_eq!(scope.finish(Ok(result)).expect("scope finish"), result);
    assert_eq!(
        ctx.obarray.symbol_value_id_copied(symbol),
        Some(Value::fixnum(10))
    );
    assert_eq!(ctx.specpdl.len(), count);
    assert!(ctx.vm_root_frames.is_empty());
    assert_eq!(
        ctx.vm_root_frames.capacity(),
        0,
        "a same-buffer restore and plain child binding need no allocation or rooting"
    );
}
