use super::*;

use neovm_edit::buffer::{EmacsBytePos, LispCharPos1};
use neovm_text::emacs_core::charset::prepared_mule::CharsetEncoder;
use neovm_text::emacs_core::text_host::TextHost;
use neovm_value::emacs_char;
use neovm_value::error::LispCondition;
use neovm_value::error::{EvalResult, Flow, signal};
use neovm_value::error::{expect_args, expect_max_args, expect_min_args};
use neovm_value::heap_registry::{HeapRegistryHandle, HeapRegistrySlot};
use neovm_value::tagged::value::SymbolMode;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

use crate::emacs_core::eval::Context;
use neovm_value::intern::intern;
use neovm_value::value::Value;

use neovm_text::emacs_core::charset::*;

#[test]
fn gc_tls_ownership_charset_new_heap_reset_drops_old_values() {
    {
        let mut first = crate::test_utils::new_context();
        let value = populate(&mut first);
        assert!(roots(&first).iter().any(|root| root.bits() == value.bits()));
    }
    let mut next = crate::test_utils::new_context();
    assert!(
        roots(&next)
            .iter()
            .all(|value| !value.is_heap_object()
                || next.tagged_heap().owns_heap_value_for_test(*value)),
        "new-heap reset retained a value from the dropped heap"
    );
    next.gc_collect_exact();
}

#[test]
fn gc_tls_ownership_charset_excludes_another_live_heap() {
    let mut first = crate::test_utils::new_context();
    let mut second = crate::test_utils::new_context();
    let value = populate(&mut second);
    assert!(
        roots(&second)
            .iter()
            .any(|root| root.bits() == value.bits())
    );
    assert!(
        roots(&first)
            .iter()
            .all(|value| !value.is_heap_object()
                || first.tagged_heap().owns_heap_value_for_test(*value)),
        "GC roots contain a value belonging to another live Context"
    );
    first.gc_collect_exact();
    second.setup_thread_locals();
    second.gc_collect_exact();
    assert!(second.tagged_heap().owns_heap_value_for_test(value));
}

#[test]
fn gc_tls_ownership_charset_metadata_follows_context_activation() {
    let mut first = crate::test_utils::new_context();
    let payload = populate(&mut first);
    let second = crate::test_utils::new_context();
    first.setup_thread_locals();
    assert!(
        roots(&first)
            .iter()
            .any(|root| root.bits() == payload.bits()),
        "reactivating a Context lost its charset Lisp attributes"
    );
    first.gc_collect_exact();
    let plist = CHARSET_REGISTRY
        .with(|slot| slot.borrow().plist(intern("ascii")).map(<[_]>::to_vec))
        .unwrap();
    assert!(
        plist
            .iter()
            .any(|(_, value)| value.bits() == payload.bits())
    );
    drop(second);
}

#[test]
fn gc_tls_ownership_charset_registries_collect_on_independent_owner_threads() {
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let worker_barrier = std::sync::Arc::clone(&barrier);
    let worker = std::thread::spawn(move || {
        let mut first = crate::test_utils::new_context();
        let value = populate(&mut first);
        worker_barrier.wait();
        first.setup_thread_locals();
        assert!(
            roots(&first).iter().any(|root| root.bits() == value.bits()),
            "worker Context lost its owning registry"
        );
        first.gc_collect_exact();
        assert!(first.tagged_heap().owns_heap_value_for_test(value));
    });
    barrier.wait();
    // Each Context is created on its owner. Registry collection on one thread
    // must remain independent of activation and collection on the other.
    let mut second = crate::test_utils::new_context();
    populate(&mut second);
    second.gc_collect_exact();
    worker.join().unwrap();
}

fn populate(_ctx: &mut Context) -> Value {
    let payload = Value::vector(vec![Value::fixnum(42)]);
    set_charset_plist_registry(
        intern("ascii"),
        vec![(intern("gc-tls-owned-plist"), payload)],
    );
    payload
}

fn roots(ctx: &Context) -> Vec<Value> {
    let mut roots = Vec::new();
    collect_charset_gc_roots(&mut roots, ctx.tagged_heap().tagged_heap_identity());
    roots
}
