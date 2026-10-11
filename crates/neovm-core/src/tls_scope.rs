//! Thread-confined restoration of temporary thread-local state.
//!
//! Restoration during teardown is best effort: a destroyed TLS key or an
//! outstanding `RefCell` borrow must not cause a second panic while unwinding.
//! The payloads used here have non-panicking destructors. This module does not
//! invoke user callbacks during restoration.

use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::rc::Rc;
use std::thread::LocalKey;

mod sealed {
    pub trait Sealed {}
    impl<T> Sealed for std::cell::Cell<T> {}
    impl<T> Sealed for std::cell::RefCell<T> {}
}

/// The two supported TLS storage kinds; callers cannot supply restoration code.
pub(crate) trait TlsSlot<T>: sealed::Sealed {
    fn replace(&self, value: T) -> T;
    fn try_replace(&self, value: T) -> Result<T, T>;
}

impl<T> TlsSlot<T> for Cell<T> {
    #[inline]
    fn replace(&self, value: T) -> T {
        Cell::replace(self, value)
    }

    #[inline]
    fn try_replace(&self, value: T) -> Result<T, T> {
        Ok(Cell::replace(self, value))
    }
}

impl<T> TlsSlot<T> for RefCell<T> {
    #[inline]
    fn replace(&self, value: T) -> T {
        std::mem::replace(&mut *self.borrow_mut(), value)
    }

    #[inline]
    fn try_replace(&self, value: T) -> Result<T, T> {
        match self.try_borrow_mut() {
            Ok(mut slot) => Ok(std::mem::replace(&mut *slot, value)),
            Err(_) => Err(value),
        }
    }
}

/// Owns restoration on the thread that installed a temporary value.
#[must_use = "thread-local state is restored when the scope drops"]
pub(crate) struct TlsScope<T: 'static, Slot: TlsSlot<T> + 'static> {
    key: &'static LocalKey<Slot>,
    previous: Option<T>,
    _thread: PhantomData<Rc<()>>,
}

static_assertions::assert_not_impl_any!(TlsScope<u32, Cell<u32>>: Send, Sync);
static_assertions::assert_not_impl_any!(TlsScope<Vec<u32>, RefCell<Vec<u32>>>: Send, Sync);

impl<T: 'static, Slot: TlsSlot<T> + 'static> std::fmt::Debug for TlsScope<T, Slot> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsScope")
            .field("active", &self.previous.is_some())
            .finish_non_exhaustive()
    }
}

impl<T: 'static, Slot: TlsSlot<T> + 'static> TlsScope<T, Slot> {
    #[inline]
    pub(crate) fn new(key: &'static LocalKey<Slot>, value: T) -> Self {
        Self::restore(key, key.with(|slot| slot.replace(value)))
    }

    /// Restore state saved by a specialized entry operation.
    #[inline]
    pub(crate) fn restore(key: &'static LocalKey<Slot>, previous: T) -> Self {
        Self {
            key,
            previous: Some(previous),
            _thread: PhantomData,
        }
    }

    /// Return an owned value without constructing a temporary scope.
    /// An unavailable key or borrowed slot drops the returned value instead.
    #[inline]
    pub(crate) fn restore_now(key: &'static LocalKey<Slot>, previous: T) {
        let _ = key.try_with(|slot| slot.try_replace(previous));
    }

    /// Restore the enclosing value and return the temporary value once.
    pub(crate) fn finish(mut self) -> Option<T> {
        self.previous
            .take()
            .map(|previous| self.key.with(|slot| slot.replace(previous)))
    }
}

impl<T: 'static, Slot: TlsSlot<T> + 'static> Drop for TlsScope<T, Slot> {
    #[inline]
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            Self::restore_now(self.key, previous);
        }
    }
}

/// A saved stack boundary, distinct from the stack's entries.
#[derive(Clone, Copy, Debug)]
struct StackDepth(usize);

/// Restores a thread-local stack to its entry depth without popping/asserting.
#[must_use = "the stack entry lasts until its scope drops"]
pub(crate) struct TlsStackScope<T: 'static> {
    key: &'static LocalKey<RefCell<Vec<T>>>,
    depth: StackDepth,
    _thread: PhantomData<Rc<()>>,
}

static_assertions::assert_not_impl_any!(TlsStackScope<u32>: Send, Sync);

impl<T: 'static> std::fmt::Debug for TlsStackScope<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsStackScope")
            .field("depth", &self.depth)
            .finish_non_exhaustive()
    }
}

impl<T: 'static> TlsStackScope<T> {
    pub(crate) fn push(key: &'static LocalKey<RefCell<Vec<T>>>, value: T) -> Self {
        let depth = key.with(|stack| {
            let mut stack = stack.borrow_mut();
            let depth = StackDepth(stack.len());
            stack.push(value);
            depth
        });
        Self {
            key,
            depth,
            _thread: PhantomData,
        }
    }
}

impl<T: 'static> Drop for TlsStackScope<T> {
    fn drop(&mut self) {
        let _ = self.key.try_with(|stack| {
            if let Ok(mut stack) = stack.try_borrow_mut() {
                stack.truncate(self.depth.0);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    thread_local! {
        static SCALAR: Cell<u32> = const { Cell::new(0) };
        static VALUES: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
        static STACK: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
    }

    #[test]
    fn tls_scope_nested_cell_restores_outer() {
        let outer = TlsScope::new(&SCALAR, 1);
        {
            let _inner = TlsScope::new(&SCALAR, 2);
            assert_eq!(SCALAR.with(Cell::get), 2);
        }
        assert_eq!(SCALAR.with(Cell::get), 1);
        drop(outer);
        assert_eq!(SCALAR.with(Cell::get), 0);
    }

    #[test]
    fn tls_scope_nested_refcell_restores_outer() {
        let outer = TlsScope::new(&VALUES, vec![1]);
        {
            let _inner = TlsScope::new(&VALUES, vec![2]);
            assert_eq!(VALUES.with(|values| values.borrow().clone()), vec![2]);
        }
        assert_eq!(VALUES.with(|values| values.borrow().clone()), vec![1]);
        drop(outer);
        assert!(VALUES.with(|values| values.borrow().is_empty()));
    }

    #[test]
    fn tls_scope_cell_restores_during_unwind() {
        assert!(
            catch_unwind(|| {
                let _scope = TlsScope::new(&SCALAR, 3);
                panic!("exercise unwind restoration");
            })
            .is_err()
        );
        assert_eq!(SCALAR.with(Cell::get), 0);
    }

    #[test]
    fn tls_scope_refcell_restores_during_unwind() {
        assert!(
            catch_unwind(|| {
                let _scope = TlsScope::new(&VALUES, vec![3]);
                panic!("exercise unwind restoration");
            })
            .is_err()
        );
        assert!(VALUES.with(|values| values.borrow().is_empty()));
    }

    #[test]
    fn tls_scope_borrowed_refcell_drop_does_not_panic() {
        let scope = TlsScope::new(&VALUES, vec![4]);
        VALUES.with(|values| {
            let borrowed = values.borrow();
            assert!(catch_unwind(AssertUnwindSafe(|| drop(scope))).is_ok());
            assert_eq!(&*borrowed, &[4]);
        });
        VALUES.with(|values| values.borrow_mut().clear());
    }

    #[test]
    fn tls_scope_finish_returns_current_and_restores_once() {
        let scope = TlsScope::new(&VALUES, vec![5]);
        VALUES.with(|values| values.borrow_mut().push(6));
        assert_eq!(scope.finish(), Some(vec![5, 6]));
        assert!(VALUES.with(|values| values.borrow().is_empty()));
    }

    #[test]
    fn tls_scope_restore_now_drops_owned_values_once() {
        #[derive(Debug)]
        struct DropCount(Rc<Cell<usize>>);
        impl Drop for DropCount {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        thread_local! {
            static OWNED: RefCell<Option<DropCount>> = const { RefCell::new(None) };
        }
        let replaced = Rc::new(Cell::new(0));
        let restored = Rc::new(Cell::new(0));
        let rejected = Rc::new(Cell::new(0));
        OWNED.with(|slot| *slot.borrow_mut() = Some(DropCount(Rc::clone(&replaced))));
        TlsScope::restore_now(&OWNED, Some(DropCount(Rc::clone(&restored))));
        assert_eq!(replaced.get(), 1);
        assert_eq!(restored.get(), 0);
        OWNED.with(|slot| {
            let borrowed = slot.borrow();
            TlsScope::restore_now(&OWNED, Some(DropCount(Rc::clone(&rejected))));
            assert_eq!(rejected.get(), 1);
            assert_eq!(restored.get(), 0);
            assert!(borrowed.is_some());
        });
        TlsScope::restore_now(&OWNED, None);
        assert_eq!(replaced.get(), 1);
        assert_eq!(restored.get(), 1);
        assert_eq!(rejected.get(), 1);
    }

    #[test]
    fn tls_stack_scope_tolerates_already_cleared_stack() {
        let outer = TlsStackScope::push(&STACK, 1);
        let inner = TlsStackScope::push(&STACK, 2);
        STACK.with(|stack| stack.borrow_mut().clear());
        drop(inner);
        drop(outer);
        assert!(STACK.with(|stack| stack.borrow().is_empty()));
    }

    #[test]
    fn tls_stack_scope_nested_unwind_restores_outer() {
        let outer = TlsStackScope::push(&STACK, 1);
        assert!(
            catch_unwind(|| {
                let _inner = TlsStackScope::push(&STACK, 2);
                panic!("exercise stack unwind restoration");
            })
            .is_err()
        );
        assert_eq!(STACK.with(|stack| stack.borrow().clone()), vec![1]);
        drop(outer);
        assert!(STACK.with(|stack| stack.borrow().is_empty()));
    }

    #[test]
    fn tls_scope_destroyed_key_drop_does_not_panic() {
        thread_local! {
            static SURVIVOR: RefCell<Option<TlsScope<Vec<u32>, RefCell<Vec<u32>>>>> =
                const { RefCell::new(None) };
            static DESTROYED_FIRST: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
        }
        std::thread::spawn(|| {
            // TLS destruction is reverse initialization order: the target key
            // disappears before the survivor drops the scope pointing to it.
            SURVIVOR.with(|_| {});
            let scope = TlsScope::new(&DESTROYED_FIRST, vec![7]);
            SURVIVOR.with(|survivor| *survivor.borrow_mut() = Some(scope));
        })
        .join()
        .expect("TLS teardown must complete");
    }
}
