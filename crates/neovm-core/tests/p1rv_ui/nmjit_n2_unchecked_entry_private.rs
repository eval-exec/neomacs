use neovm_core::emacs_core::eval::WorldEra as ParallelHeapEra;

fn unchecked_entry() {
    let _ = ParallelHeapEra::admit;
}

fn main() {}
