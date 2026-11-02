use neovm_core::emacs_core::bytecode::ByteCodeFunction;

fn attach(function: &mut ByteCodeFunction) {
    function.runtime = None;
}

fn main() {
    let _ = attach;
}
