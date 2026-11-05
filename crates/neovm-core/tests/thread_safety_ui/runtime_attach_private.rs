use neovm_value::tagged::bytecode::chunk::ByteCodeFunction;

fn attach(function: &mut ByteCodeFunction) {
    function.runtime = None;
}

fn main() {
    let _ = attach;
}
