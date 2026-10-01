//! Evaluator-thread ownership for Lisp operands retained by native caches.
//! A lease carries no API for reading its values and cannot cross threads.
//! The evaluator owns only weak references, so cancellation and cache eviction
//! release roots without borrowing the evaluator or modifying the specpdl.

use crate::emacs_core::{Context, Value};
use std::cell::RefCell;
use std::rc::{Rc, Weak};

/// Keeps a captured object graph alive until the last evaluator-side owner
/// retires. Owned worker data must never contain this handle.
#[derive(Clone)]
pub struct OwnedRoots {
    values: Rc<[Value]>,
}

impl std::fmt::Debug for OwnedRoots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnedRoots")
            .field("len", &self.values.len())
            .finish()
    }
}

#[derive(Default)]
pub(crate) struct OwnedRootRegistry {
    leases: RefCell<Vec<Weak<[Value]>>>,
}

impl OwnedRootRegistry {
    fn retain(&self, values: impl IntoIterator<Item = Value>) -> OwnedRoots {
        let values: Rc<[Value]> = values
            .into_iter()
            .filter(|value| value.is_heap_object())
            .collect();
        if !values.is_empty() {
            let mut leases = self.leases.borrow_mut();
            leases.retain(|lease| lease.strong_count() != 0);
            leases.push(Rc::downgrade(&values));
        }
        OwnedRoots { values }
    }

    pub(crate) fn trace_roots(&self, visit: &mut dyn FnMut(Value)) {
        self.leases.borrow_mut().retain(|lease| {
            let Some(values) = lease.upgrade() else {
                return false;
            };
            for &value in values.iter() {
                visit(value);
            }
            true
        });
    }
}

impl Context {
    /// Retain already-live operands from this evaluator. This grants object
    /// lifetime only: callers must separately validate mutation and layout
    /// generations before using derived data.
    pub fn retain_gc_roots(&self, values: impl IntoIterator<Item = Value>) -> OwnedRoots {
        self.owned_roots.retain(values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloned_lease_survives_exact_gc_and_retires_with_last_owner() {
        let mut context = Context::new();
        context.gc_collect_exact();
        let baseline = context.tagged_heap.allocated_count();
        let value = Value::cons(Value::fixnum(17), Value::vector(vec![Value::fixnum(23)]));
        let lease = context.retain_gc_roots([value]);
        let pending = lease.clone();
        drop(lease);
        context.gc_collect_exact();
        assert_eq!(
            context.tagged_heap.allocated_count(),
            baseline + 2,
            "a pending native owner must keep the whole captured graph alive"
        );
        assert_eq!(value.cons_car(), Value::fixnum(17));
        assert_eq!(
            value.cons_cdr().as_vector_data().unwrap().as_slice(),
            &[Value::fixnum(23)]
        );
        drop(pending);
        context.gc_collect_exact();
        assert_eq!(context.tagged_heap.allocated_count(), baseline);
        assert!(context.owned_roots.leases.borrow().is_empty());
    }

    #[test]
    fn retired_leases_do_not_accumulate_between_collections() {
        let context = Context::new();
        let value = Value::cons(Value::fixnum(1), Value::NIL);
        for _ in 0..1000 {
            let lease = context.retain_gc_roots([value]);
            assert_eq!(context.owned_roots.leases.borrow().len(), 1);
            drop(lease);
        }
        let immediate = context.retain_gc_roots([Value::NIL, Value::fixnum(2)]);
        assert!(immediate.values.is_empty());
    }
}
