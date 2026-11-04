//! Dynamic-module registry state owned by the evaluator layer (stage-4
//! W2/E6d STATE-DOWN).
//!
//! The registry is runtime state: the Context holds its value-backed handle,
//! construction installs that handle from this thread-local slot, the green
//! carrier parks it across owner handoff, the GC root walk enumerates its
//! entries (both the Context registry and the thread-local sink), pdump
//! reset clears it, and teardown retires it.  All of those touches come from
//! eval, so the slot, the handle and its payload types live here.
//!
//! The module host (loading, per-env arenas, global-ref allocation and the
//! emacs_env function implementations) stays in
//! `emacs_core::system::dynamic_module` and reaches this state downward
//! through the accessors.  The payload moves verbatim: `GlobalRefEntry`
//! keeps the exact `HeapRegistryHandle<Vec<Option<GlobalRefEntry>>>`
//! instantiation, its raw tag pointer and Drop, so the Context field's type,
//! size and offset are unchanged.
//!
//! The GC collector below stays a direct function (K3): the GC walkers call
//! it inside the eval crate; no installed indirection is introduced.

#![allow(non_camel_case_types)]

use neovm_value::heap_registry::{HeapRegistryHandle, HeapRegistrySlot, HeapRegistryWeak};
use neovm_value::value::Value;

/// C-compatible value cell matching GNU `emacs-module.h`.
#[repr(C)]
pub struct emacs_value_tag {
    pub v: Value,
    // Null for scoped local arena tags.  Native global tags keep their owning
    // registry weakly and can therefore be released after Context teardown.
    global_reference: *mut GlobalRefMetadata,
}

pub type emacs_value = *mut emacs_value_tag;

impl emacs_value_tag {
    /// A scoped (non-global) tag cell.
    pub fn new(v: Value) -> Self {
        Self {
            v,
            global_reference: std::ptr::null_mut(),
        }
    }

    /// The global-reference metadata this tag belongs to, when it is a
    /// native global tag.
    pub fn metadata(&self) -> *mut GlobalRefMetadata {
        self.global_reference
    }

    pub fn set_metadata(&mut self, metadata: *mut GlobalRefMetadata) {
        self.global_reference = metadata;
    }
}

/// One native global reference registered by a loaded module.
pub struct GlobalRefEntry {
    value: Value,
    heap_identity: usize,
    tag_ptr: *mut emacs_value_tag,
}

impl GlobalRefEntry {
    pub fn new(value: Value, heap_identity: usize, tag_ptr: *mut emacs_value_tag) -> Self {
        Self {
            value,
            heap_identity,
            tag_ptr,
        }
    }

    pub fn value(&self) -> Value {
        self.value
    }

    pub fn heap_identity(&self) -> usize {
        self.heap_identity
    }

    pub fn tag_ptr(&self) -> *mut emacs_value_tag {
        self.tag_ptr
    }
}

/// Refcount and weak owner for one native global reference.
pub struct GlobalRefMetadata {
    owner: HeapRegistryWeak<Vec<Option<GlobalRefEntry>>>,
    refcount: isize,
}

impl GlobalRefMetadata {
    pub fn new(owner: HeapRegistryWeak<Vec<Option<GlobalRefEntry>>>) -> Self {
        Self { owner, refcount: 1 }
    }

    pub fn owner(&self) -> &HeapRegistryWeak<Vec<Option<GlobalRefEntry>>> {
        &self.owner
    }

    pub fn refcount_add(&mut self, delta: isize) {
        self.refcount += delta;
    }

    pub fn refcount(&self) -> isize {
        self.refcount
    }
}

impl Drop for GlobalRefEntry {
    fn drop(&mut self) {
        // Outstanding native references may be freed after their world ends.
        // Invalidate the Lisp word without freeing the independently owned tag.
        unsafe { (*self.tag_ptr).v = Value::NIL };
    }
}

pub type DynamicModuleRegistryHandle = HeapRegistryHandle<Vec<Option<GlobalRefEntry>>>;

thread_local! {
    static GLOBAL_REFS: HeapRegistrySlot<Vec<Option<GlobalRefEntry>>> =
        HeapRegistrySlot::new(Vec::new());
}

pub fn current_dynamic_module_registry_handle() -> DynamicModuleRegistryHandle {
    GLOBAL_REFS.with(HeapRegistrySlot::current)
}

pub fn install_dynamic_module_registry_handle(handle: &DynamicModuleRegistryHandle) {
    GLOBAL_REFS.with(|slot| slot.install(handle));
}

pub fn reset_dynamic_module_registry() {
    GLOBAL_REFS.with(|slot| slot.reset(Vec::new()));
}

/// Carrier park: forget the installed global-ref registry handle (a plain
/// store). The World's registry stays alive through its Contexts.
pub fn park_dynamic_module_registry() {
    GLOBAL_REFS.with(HeapRegistrySlot::park);
}

/// The installed handle's heap identity, for the carrier TLS fingerprint.
pub fn dynamic_module_registry_heap_identity() -> Option<usize> {
    GLOBAL_REFS.with(|slot| slot.installed_heap_identity())
}

/// Access to the thread-local registry slot for the module host's
/// global-ref allocation path.
pub fn with_global_refs<R>(
    f: impl FnOnce(&HeapRegistrySlot<Vec<Option<GlobalRefEntry>>>) -> R,
) -> R {
    GLOBAL_REFS.with(f)
}

pub fn retire_dynamic_module_registry(registry: &DynamicModuleRegistryHandle) {
    registry.borrow_mut().clear();
}

pub fn collect_dynamic_module_registry_gc_roots(
    registry: &DynamicModuleRegistryHandle,
    roots: &mut Vec<Value>,
) {
    for entry in registry.borrow().iter().flatten() {
        if entry.heap_identity() == registry.heap_identity()
            || (entry.heap_identity() == 0 && !entry.value().is_heap_object())
        {
            roots.push(entry.value());
        }
    }
}
