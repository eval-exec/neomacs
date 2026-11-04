//! Parking must preserve the collection census after excluding World groups.

use crate::emacs_core::bytecode::ByteCodeFunctionExt;
use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
#[cfg(feature = "jit")]
use crate::emacs_core::eval::SpecBinding;
use crate::emacs_core::eval::{
    Context, ThreadingMode, collect_thread_local_gc_roots, push_scratch_gc_roots,
    restore_scratch_gc_roots, save_scratch_gc_roots,
};
use neovm_value::error::Flow;
use neovm_value::intern::intern;
use neovm_value::tagged::gc::CacheRootScan;
use neovm_value::value::Value;
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};

struct ScratchScope(usize);

impl Drop for ScratchScope {
    fn drop(&mut self) {
        restore_scratch_gc_roots(self.0);
    }
}

fn multiset(values: impl IntoIterator<Item = Value>) -> BTreeMap<usize, usize> {
    let mut counts = BTreeMap::new();
    for value in values {
        *counts.entry(value.bits()).or_default() += 1;
    }
    counts
}

#[test]
fn parked_roots_equal_full_census_without_world_plus_parked_tls() {
    let mut owner = Context::new_with_threading(ThreadingMode::Green);
    owner
        .eval_str("(thread-join (make-thread (lambda () nil)))")
        .expect("seat the owner and join the bootstrap peer");
    assert!(owner.green_seat().is_some());
    let specpdl_base = owner.specpdl.len();
    owner
        .eval_str("(defvar g3f-census-binding nil)")
        .expect("define the projected binding");
    let symbol = intern("g3f-census-binding");
    owner
        .thread_projection_for_test(symbol)
        .expect("Green admission already projected the variable");
    let outer_binding = Value::string("outer owner slab only");
    let inner_binding = Value::string("inner owner slab only");
    owner.try_specbind(symbol, outer_binding).expect("outer");
    owner.try_specbind(symbol, inner_binding).expect("inner");
    assert_eq!(owner.owner.slab().unwrap().records_of(symbol).count(), 2);

    // Repeated independent root slots ensure this is a multiset test.
    let window_root = Value::string("only the native operand window");
    assert_eq!(owner.jit_root_stack_top, 0);
    owner.jit_root_stack.resize(2, Value::NIL);
    owner.jit_root_stack[..2].fill(window_root);
    owner.jit_root_stack_top = 2;
    let scratch_scope = ScratchScope(save_scratch_gc_roots());
    let scratch_root = Value::string("only scratch root slots");
    push_scratch_gc_roots(&[scratch_root, scratch_root, scratch_root]);

    let position_root = Value::string("aжb");
    assert_eq!(
        neovm_value::string_pos_cache::string_char_to_byte(
            position_root,
            position_root.as_lisp_string().unwrap(),
            2,
        ),
        3
    );
    let regex_root = Value::make_char_table(Value::NIL, Value::NIL, 0);
    let pattern = neovm_value::heap_types::LispString::from_utf8("g3f-census-regex");
    let searched = neovm_value::heap_types::LispString::from_utf8("g3f-census-regex");
    let compiled = neovm_text::emacs_core::regex::compile_string_search_pattern(
        &pattern,
        &searched,
        true,
        false,
        Some(regex_root),
        &neovm_text::emacs_core::regex_emacs::DefaultSyntaxLookup,
    )
    .expect("populate the regexp translation cache");
    assert!(compiled.translate.is_some());
    drop(compiled);

    let flow_root = Value::string("only a Rust-held in-flight throw");
    let flow = Flow::throw(Value::symbol("g3f-census-throw"), flow_root);
    #[cfg(feature = "jit")]
    let redefined_root = {
        use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
        use neovm_value::value::LambdaParams;
        let called = Value::symbol("g3f-census-redefined");
        let mut source = ByteCodeFunction::new(LambdaParams::simple(Vec::new()));
        source.ops = vec![Op::Constant(0), Op::Return];
        source.constants = vec![Value::NIL].into();
        source.seal_hand_assembled_ops();
        let source = Value::make_bytecode(source);
        owner.specpdl.push(SpecBinding::Backtrace1 {
            function: called,
            arg: Value::NIL,
            debug_on_exit: false,
        });
        crate::emacs_core::jit::cache::pin_redefined_function(
            intern("g3f-census-redefined"),
            source,
        );
        source
    };

    let parked_owner = owner.park_owner();

    // The const-generic walk itself is the group classification oracle.
    // There is no second list of World/private group names in this test.
    let private_groups: BTreeSet<_> = owner.private_root_group_names().into_iter().collect();
    let current_group = Cell::new("");
    let mut private = Vec::new();
    let mut world = Vec::new();
    owner.trace_roots(
        owner.tagged_heap.identity(),
        |roots| {
            neovm_value::tagged::transport::collect_shared_root_gc_roots(&owner.tagged_heap, roots)
        },
        &mut |group| current_group.set(group),
        &mut |value| {
            if private_groups.contains(current_group.get()) {
                private.push(value);
            } else {
                world.push(value);
            }
        },
    );
    let mut tls = Vec::new();
    let mut stats = Vec::new();
    collect_thread_local_gc_roots(
        &mut tls,
        owner.tagged_heap.identity(),
        owner.tagged_heap.gc_collections(),
        CacheRootScan::Parked,
        &mut stats,
    );
    for (value, origin) in [
        (scratch_root, "scratch-thread-local"),
        (position_root, "string-pos-cache-thread-local"),
        (regex_root, "regex-thread-local"),
    ] {
        assert!(
            tls.iter()
                .any(|root| root.0.bits() == value.bits() && root.1 == origin),
            "fixture must populate {origin}"
        );
    }
    let world = multiset(world);
    for value in [outer_binding, inner_binding] {
        assert!(
            world.contains_key(&value.bits()),
            "World traces the owner slab"
        );
        assert!(
            !private.iter().any(|root| root.bits() == value.bits()),
            "the parked census does not duplicate World-owned binding storage"
        );
    }
    private.extend(tls.into_iter().map(|root| root.0));
    let expected = multiset(private);
    assert_eq!(expected.get(&window_root.bits()), Some(&2));
    assert_eq!(expected.get(&scratch_root.bits()), Some(&3));
    assert!(
        expected.contains_key(&flow_root.bits()),
        "live Flow registry payload"
    );
    #[cfg(feature = "jit")]
    assert!(
        expected.contains_key(&redefined_root.bits()),
        "called redefined source pin"
    );

    let actual = owner.collect_parked_roots();
    assert_eq!(multiset(actual), expected);

    owner.resume_owner(parked_owner);
    drop(flow);
    drop(scratch_scope);
    owner.jit_root_stack_top = 0;
    owner.unbind_to(specpdl_base);
    owner.green_teardown();
}
