use neovm_core::emacs_core::Context;

fn raw_symbol(context: &mut Context) {
    let mut writer = context.obarray_mut();
    let _symbol = writer.get_mut("debug-on-next-call");
}

fn main() {}
