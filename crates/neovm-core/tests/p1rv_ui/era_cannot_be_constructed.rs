use neovm_core::emacs_core::eval::WorldEra as ParallelHeapEra;

fn fabricate() -> ParallelHeapEra<'static> {
    ParallelHeapEra {}
}

fn main() {}
