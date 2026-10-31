//! Context-owned semantic registries with a thread-local active view.
//!
//! A Context and its installed registry views remain on their owning mutator
//! thread. Arc maintains registry lifetime across local Context activations;
//! it does not make the RefCell registries Send or Sync.

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

static_assertions::assert_not_impl_any!(HeapRegistryHandle<()>: Send, Sync);
static_assertions::assert_not_impl_any!(HeapRegistryWeak<()>: Send, Sync);

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
    /// `None` while a parked carrier holds no registry: the carrier keeps no
    /// heap-identified handle across its idle period. The option adds no TLS
    /// bytes (`Option<HeapRegistryHandle<T>>` is the same size as the
    /// handle). The next owner installs its handle before Lisp runs.
    active: RefCell<Option<HeapRegistryHandle<T>>>,
}

// The inner guard must drop before its owning outer handle guard.
pub(crate) struct HeapRegistryRef<'a, T> {
    value: Ref<'a, T>,
    _handle: Ref<'a, Option<HeapRegistryHandle<T>>>,
}

impl<T> Deref for HeapRegistryRef<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

pub(crate) struct HeapRegistryRefMut<'a, T> {
    value: RefMut<'a, T>,
    _handle: Ref<'a, Option<HeapRegistryHandle<T>>>,
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
            active: RefCell::new(Some(HeapRegistryHandle::new(value))),
        }
    }

    pub(crate) fn current(&self) -> HeapRegistryHandle<T> {
        self.active
            .borrow()
            .as_ref()
            .expect(
                "no registry is installed on this thread; an owner installs one before Lisp runs",
            )
            .clone()
    }

    /// The installed handle's heap identity, or `None` while a parked carrier
    /// holds no registry. Debug fingerprints use this instead of [`Self::current`]
    /// so they can sample an idle carrier.
    pub(crate) fn installed_heap_identity(&self) -> Option<usize> {
        self.active
            .borrow()
            .as_ref()
            .map(|handle| handle.heap_identity)
    }

    pub(crate) fn install(&self, handle: &HeapRegistryHandle<T>) {
        let mut active = self.active.borrow_mut();
        if active
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(&current.registry, &handle.registry))
        {
            return;
        }
        // This outer borrow refuses replacement while a local registry guard
        // is alive. Its Context and the installed registry are thread-confined.
        *active = Some(handle.clone());
    }

    /// Carrier park: forget the installed handle without allocating. The
    /// registry value stays alive through the holders that share it (the
    /// World's Contexts); the idle carrier only drops its clone.
    pub(crate) fn park(&self) {
        *self.active.borrow_mut() = None;
    }

    pub(crate) fn reset(&self, value: T) {
        let heap_identity = crate::tagged::gc::current_tagged_heap_identity().unwrap_or(0);
        let mut active = self.active.borrow_mut();
        if let Some(handle) = active.as_mut()
            && handle.heap_identity == heap_identity
        {
            // Ordinary clears in a live Context keep the owning handle.
            *handle.registry.borrow_mut() = value;
            return;
        }
        *active = Some(HeapRegistryHandle::new(value));
    }

    #[inline]
    pub(crate) fn borrow(&self) -> HeapRegistryRef<'_, T> {
        let handle = self.active.borrow();
        let current = handle.as_ref().expect(
            "no registry is installed on this thread; an owner installs one before Lisp runs",
        );
        // SAFETY: the outer Ref guard keeps the Arc installed and prevents
        // replacement for the entire inner Ref lifetime. The returned guard
        // drops the inner Ref first. Registry access follows the Context's
        // owning mutator thread; neither Context nor this handle is Send.
        let registry = unsafe { &*Arc::as_ptr(&current.registry) };
        HeapRegistryRef {
            value: registry.borrow(),
            _handle: handle,
        }
    }

    #[inline]
    pub(crate) fn borrow_mut(&self) -> HeapRegistryRefMut<'_, T> {
        let handle = self.active.borrow();
        let current = handle.as_ref().expect(
            "no registry is installed on this thread; an owner installs one before Lisp runs",
        );
        // SAFETY: same owning outer guard and drop order as borrow().
        let registry = unsafe { &*Arc::as_ptr(&current.registry) };
        HeapRegistryRefMut {
            value: registry.borrow_mut(),
            _handle: handle,
        }
    }
}

#[cfg(test)]
#[path = "tests/ownership_test.rs"]
mod ownership_tests;
