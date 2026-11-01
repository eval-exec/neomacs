use neovm_core::emacs_core::eval::WorldEra as ParallelHeapEra;

#[path = "../thread_safety_ui/support/thread_bounds.rs"]
mod thread_bounds;
use thread_bounds::ThreadSharing;

fn main() {
    ThreadSharing::<ParallelHeapEra<'static>>::check();
}
