use neovm_core::tagged::gc::AllStopped;

fn trace_under_census(mut stopped: AllStopped<'static>) {
    let census = stopped.summaries();
    let _ = stopped.trace(|_stopped, _proof| Ok(()));
    drop(census);
}

fn main() {}
