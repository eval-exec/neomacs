use neovm_core::emacs_core::{Context, Value};

fn completed_method_then_forget(context: &mut Context) {
    let mut writer = context.obarray_mut();
    writer.set_symbol_function("closed-writer-probe", Value::NIL);
    let _published_epoch = writer.function_epoch();
    std::mem::forget(writer);
    let _current = context.obarray().symbol_function("closed-writer-probe");
}

fn bounded_methods(context: &mut Context) {
    context.with_obarray_mut(|mut writer| writer.fmakunbound("closed-writer-probe"));
}

fn main() {}
