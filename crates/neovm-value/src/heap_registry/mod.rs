//! Context-owned semantic registries with a thread-local active view.
//!
//! Moved verbatim from `emacs_core::runtime::heap_registry` (peer ruling
//! STAGE3B T3); core re-exports it through its facade.
//!
//! A Context and its installed registry views remain on their owning mutator
//! thread. Arc maintains registry lifetime across local Context activations;
//! it does not make the RefCell registries Send or Sync.

use std::cell::{Ref, RefCell, RefMut};
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Weak};

#[doc(hidden)]
pub struct HeapRegistryHandle<T> {
    heap_identity: usize,
    registry: Arc<RefCell<T>>,
}

#[doc(hidden)]
pub struct HeapRegistryWeak<T> {
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
    #[doc(hidden)]
    pub fn upgrade(&self) -> Option<HeapRegistryHandle<T>> {
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
    #[doc(hidden)]
    pub fn new(value: T) -> Self {
        Self {
            heap_identity: crate::tagged::gc::current_tagged_heap_identity().unwrap_or(0),
            registry: Arc::new(RefCell::new(value)),
        }
    }

    #[doc(hidden)]
    pub fn heap_identity(&self) -> usize {
        self.heap_identity
    }

    #[doc(hidden)]
    pub fn downgrade(&self) -> HeapRegistryWeak<T> {
        HeapRegistryWeak {
            heap_identity: self.heap_identity,
            registry: Arc::downgrade(&self.registry),
        }
    }

    #[inline]
    #[doc(hidden)]
    pub fn borrow(&self) -> Ref<'_, T> {
        self.registry.borrow()
    }

    #[inline]
    #[doc(hidden)]
    pub fn borrow_mut(&self) -> RefMut<'_, T> {
        self.registry.borrow_mut()
    }
}

#[doc(hidden)]
pub struct HeapRegistrySlot<T> {
    /// `None` while a parked carrier holds no registry: the carrier keeps no
    /// heap-identified handle across its idle period. The option adds no TLS
    /// bytes (`Option<HeapRegistryHandle<T>>` is the same size as the
    /// handle). The next owner installs its handle before Lisp runs.
    active: RefCell<Option<HeapRegistryHandle<T>>>,
}

// The inner guard must drop before its owning outer handle guard.
#[doc(hidden)]
pub struct HeapRegistryRef<'a, T> {
    value: Ref<'a, T>,
    _handle: Ref<'a, Option<HeapRegistryHandle<T>>>,
}

impl<T> Deref for HeapRegistryRef<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

#[doc(hidden)]
pub struct HeapRegistryRefMut<'a, T> {
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
    #[doc(hidden)]
    pub fn new(value: T) -> Self {
        Self {
            active: RefCell::new(Some(HeapRegistryHandle::new(value))),
        }
    }

    #[doc(hidden)]
    pub fn current(&self) -> HeapRegistryHandle<T> {
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
    #[doc(hidden)]
    pub fn installed_heap_identity(&self) -> Option<usize> {
        self.active
            .borrow()
            .as_ref()
            .map(|handle| handle.heap_identity)
    }

    /// Signed exact-caller row (STAGE3B T3): each registry's own install
    /// entry point and the carrier activation code.
    #[doc(hidden)]
    pub fn install(&self, handle: &HeapRegistryHandle<T>) {
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
    /// Signed exact-caller row (STAGE3B T3): the carrier park and each
    /// registry's own park entry point.
    #[doc(hidden)]
    pub fn park(&self) {
        *self.active.borrow_mut() = None;
    }

    /// Signed exact-caller row (STAGE3B T3): each registry's own reset
    /// entry point and the carrier activation code.
    #[doc(hidden)]
    pub fn reset(&self, value: T) {
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
    #[doc(hidden)]
    pub fn borrow(&self) -> HeapRegistryRef<'_, T> {
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
    #[doc(hidden)]
    pub fn borrow_mut(&self) -> HeapRegistryRefMut<'_, T> {
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

/// Whether two handles share one registry allocation: the cross-heap
/// reset rows (kept core-side, where `Context` lives) assert on this.
#[cfg(any(test, feature = "test-support"))]
impl<T> HeapRegistryHandle<T> {
    pub fn registry_ptr_eq_for_test(a: &Self, b: &Self) -> bool {
        Arc::ptr_eq(&a.registry, &b.registry)
    }
}

#[cfg(any(test, feature = "test-support"))]
#[path = "tests/ownership_test.rs"]
mod ownership_tests;
