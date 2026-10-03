//! Isolated native-validation work observation; tests only.
//!
//! Threading: these scalars belong to the compiling test thread and an explicit
//! capture scope. No Lisp values, IR, capabilities, pointers or cache are stored.
//! Nested captures restore the entire prior scalar state on success/unwind.
//! This module, its TLS and every call site are absent in non-test builds.

use std::cell::RefCell;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Calls {
    pub(crate) ordinary: usize,
    pub(crate) arrays: usize,
    pub(crate) recipes: usize,
}

#[derive(Clone, Copy)]
pub(crate) enum Checker {
    Ordinary,
    Arrays,
    Recipes,
}

#[derive(Clone, Copy, Default)]
struct State {
    enabled: bool,
    native_depth: usize,
    calls: Calls,
}
thread_local! {
    static ACTIVE: RefCell<State> = const { RefCell::new(State {
        enabled: false,
        native_depth: 0,
        calls: Calls { ordinary: 0, arrays: 0, recipes: 0 },
    }) };
}

struct Restore(State);
impl Drop for Restore {
    fn drop(&mut self) {
        ACTIVE.with(|state| *state.borrow_mut() = self.0);
    }
}

/// Observe actual checker entries ONLY inside a leaf's NativeRegion. Backend
/// final verification, pass validation and reference evaluation are excluded.
pub(crate) fn capture<T>(run: impl FnOnce() -> T) -> (T, Calls) {
    let previous = ACTIVE.with(|state| {
        std::mem::replace(
            &mut *state.borrow_mut(),
            State {
                enabled: true,
                ..State::default()
            },
        )
    });
    let restore = Restore(previous);
    let result = run();
    let calls = ACTIVE.with(|state| state.borrow().calls);
    drop(restore);
    (result, calls)
}

/// Explicit compile-only validation phase; never covers generated execution.
pub(crate) struct NativeRegion(bool);
impl NativeRegion {
    pub(crate) fn enter() -> Self {
        let active = ACTIVE.with(|state| {
            let mut state = state.borrow_mut();
            if state.enabled {
                state.native_depth += 1;
            }
            state.enabled
        });
        Self(active)
    }
}
impl Drop for NativeRegion {
    fn drop(&mut self) {
        if self.0 {
            ACTIVE.with(|state| {
                let mut state = state.borrow_mut();
                state.native_depth = state
                    .native_depth
                    .checked_sub(1)
                    .expect("balanced invocation-local native validation scope");
            });
        }
    }
}

/// Called at the actual independent/full checker entry, never inferred from
/// the requested pass or a successful capability's mere presence.
pub(crate) fn entered(checker: Checker) {
    ACTIVE.with(|state| {
        let mut state = state.borrow_mut();
        if !state.enabled || state.native_depth == 0 {
            return;
        }
        match checker {
            Checker::Ordinary => state.calls.ordinary += 1,
            Checker::Arrays => state.calls.arrays += 1,
            Checker::Recipes => state.calls.recipes += 1,
        }
    });
}
