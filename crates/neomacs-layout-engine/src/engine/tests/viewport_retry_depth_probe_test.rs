use std::cell::Cell;

thread_local! {
    static DEPTH: Cell<usize> = const { Cell::new(0) };
    static MAX_DEPTH: Cell<usize> = const { Cell::new(0) };
}

pub(super) struct Guard;

impl Guard {
    pub(super) fn enter() -> Self {
        DEPTH.with(|depth| {
            let next = depth.get() + 1;
            depth.set(next);
            MAX_DEPTH.with(|max| {
                if next > max.get() {
                    max.set(next);
                }
            });
        });
        Guard
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

pub(super) fn reset() {
    MAX_DEPTH.with(|max| max.set(0));
}

pub(super) fn max_depth() -> usize {
    MAX_DEPTH.with(|max| max.get())
}
