use neovm_core::emacs_core::eval::WorldEra as ParallelHeapEra;
use neovm_core::tagged::gc::MutatorPermit;

fn escape(era: ParallelHeapEra<'static>) -> MutatorPermit<'static> {
    match era.register(1, 0) {
        Ok(permit) => permit,
        Err(_) => panic!("fixture registration failed"),
    }
}

fn main() {}
