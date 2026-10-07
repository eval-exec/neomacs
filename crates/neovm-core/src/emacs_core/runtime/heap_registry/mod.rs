//! Context-owned semantic registries with a thread-local active view.
//!
//! Handles use Arc so a worker may move the exclusively accessed Context
//! while the originating thread drops its installed alias. The registry's
//! RefCell remains subject to the Context's exclusive-mutator contract.

use std::cell::{Ref, RefCell, RefMut};
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Weak};

pub(crate) struct HeapRegistryHandle<T> {
    heap_identity: usize,
    registry: Arc<RefCell<T>>,
}

pub(crate) struct HeapRegistryWeak<T> {
    heap_identity: usize,
    registry: Weak<RefCell<T>>,
}

impl<T> Clone for HeapRegistryWeak<T> {
    fn clone(&self) -> Self {
        Self {
            heap_identity: self.heap_identity,
            registry: Weak::clone(&self.registry),
        }
    }
}

impl<T> HeapRegistryWeak<T> {
    pub(crate) fn upgrade(&self) -> Option<HeapRegistryHandle<T>> {
        Some(HeapRegistryHandle {
            heap_identity: self.heap_identity,
            registry: self.registry.upgrade()?,
        })
    }
}

impl<T> Clone for HeapRegistryHandle<T> {
    fn clone(&self) -> Self {
        Self {
            heap_identity: self.heap_identity,
            registry: Arc::clone(&self.registry),
        }
    }
}

impl<T> HeapRegistryHandle<T> {
    pub(crate) fn new(value: T) -> Self {
        Self {
            heap_identity: crate::tagged::gc::current_tagged_heap_identity().unwrap_or(0),
            registry: Arc::new(RefCell::new(value)),
        }
    }

    pub(crate) fn heap_identity(&self) -> usize {
        self.heap_identity
    }

    pub(crate) fn downgrade(&self) -> HeapRegistryWeak<T> {
        HeapRegistryWeak {
            heap_identity: self.heap_identity,
            registry: Arc::downgrade(&self.registry),
        }
    }

    #[inline]
    pub(crate) fn borrow(&self) -> Ref<'_, T> {
        self.registry.borrow()
    }

    #[inline]
    pub(crate) fn borrow_mut(&self) -> RefMut<'_, T> {
        self.registry.borrow_mut()
    }
}

pub(crate) struct HeapRegistrySlot<T> {
    active: RefCell<HeapRegistryHandle<T>>,
}

// The inner guard must drop before its owning outer handle guard.
pub(crate) struct HeapRegistryRef<'a, T> {
    value: Ref<'a, T>,
    _handle: Ref<'a, HeapRegistryHandle<T>>,
}

impl<T> Deref for HeapRegistryRef<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

pub(crate) struct HeapRegistryRefMut<'a, T> {
    value: RefMut<'a, T>,
    _handle: Ref<'a, HeapRegistryHandle<T>>,
}

impl<T> Deref for HeapRegistryRefMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> DerefMut for HeapRegistryRefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

impl<T> HeapRegistrySlot<T> {
    pub(crate) fn new(value: T) -> Self {
        Self {
            active: RefCell::new(HeapRegistryHandle::new(value)),
        }
    }

    pub(crate) fn current(&self) -> HeapRegistryHandle<T> {
        self.active.borrow().clone()
    }

    pub(crate) fn install(&self, handle: &HeapRegistryHandle<T>) {
        if Arc::ptr_eq(&self.active.borrow().registry, &handle.registry) {
            return;
        }
        // This outer borrow refuses replacement while a local registry guard
        // is alive. Never inspect the old inner RefCell: its Context may have
        // moved to a worker thread that is currently using it.
        *self.active.borrow_mut() = handle.clone();
    }

    pub(crate) fn reset(&self, value: T) {
        let heap_identity = crate::tagged::gc::current_tagged_heap_identity().unwrap_or(0);
        let active = self.active.borrow();
        if active.heap_identity == heap_identity {
            // Ordinary clears in a live Context keep the owning handle.
            *active.borrow_mut() = value;
        } else {
            drop(active);
            self.install(&HeapRegistryHandle::new(value));
        }
    }

    #[inline]
    pub(crate) fn borrow(&self) -> HeapRegistryRef<'_, T> {
        let handle = self.active.borrow();
        // SAFETY: the outer Ref guard keeps the Arc installed and prevents
        // replacement for the entire inner Ref lifetime. The returned guard
        // drops the inner Ref first. Registry access follows the Context's
        // existing exclusive-mutator contract, including thread transfers.
        let registry = unsafe { &*Arc::as_ptr(&handle.registry) };
        HeapRegistryRef {
            value: registry.borrow(),
            _handle: handle,
        }
    }

    #[inline]
    pub(crate) fn borrow_mut(&self) -> HeapRegistryRefMut<'_, T> {
        let handle = self.active.borrow();
        // SAFETY: same owning outer guard and drop order as borrow().
        let registry = unsafe { &*Arc::as_ptr(&handle.registry) };
        HeapRegistryRefMut {
            value: registry.borrow_mut(),
            _handle: handle,
        }
    }
}

#[cfg(test)]
#[path = "tests/ownership_test.rs"]
mod ownership_tests;
