use std::cell::Cell;

thread_local! {
    static DEPTH: Cell<usize> = const { Cell::new(0) };
    static MAX_DEPTH: Cell<usize> = const { Cell::new(0) };
    static ATTEMPTS: Cell<usize> = const { Cell::new(0) };
}

pub(super) struct Guard;

impl Guard {
    pub(super) fn enter() -> Self {
        ATTEMPTS.with(|attempts| attempts.set(attempts.get() + 1));
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
    ATTEMPTS.with(|attempts| attempts.set(0));
}

pub(super) fn max_depth() -> usize {
    MAX_DEPTH.with(|max| max.get())
}

/// Leaf layout attempts (window walks) since the last [`reset`].
pub(super) fn attempts() -> usize {
    ATTEMPTS.with(|attempts| attempts.get())
}
