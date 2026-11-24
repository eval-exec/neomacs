use neovm_core::tagged::gc::AllStopped;

fn complete_under_census(mut stopped: AllStopped<'static>) {
    let census = stopped.summaries();
    let _ = stopped.complete();
    drop(census);
}

fn main() {}
