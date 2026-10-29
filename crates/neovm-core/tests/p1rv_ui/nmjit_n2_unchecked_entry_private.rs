use neovm_core::tagged::gc::ParallelHeapEra;

fn unchecked_entry() {
    let _ = ParallelHeapEra::admit;
}

fn main() {}
