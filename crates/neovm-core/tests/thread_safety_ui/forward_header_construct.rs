use neovm_core::emacs_core::forward::{LispFwd, LispFwdType};

fn main() {
    let _header = LispFwd {
        ty: LispFwdType::Int,
        _thread_confined: Default::default(),
    };
}
