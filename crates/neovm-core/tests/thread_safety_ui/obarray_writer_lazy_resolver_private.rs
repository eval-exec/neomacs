use neovm_core::emacs_core::{Context, SymId};

fn name_private_canonical_resolver(context: &mut Context, id: SymId) {
    let mut writer = context.obarray_mut();
    let _descriptor = (*writer).debug_on_next_call_bool_fwd(id);
}

fn main() {}
