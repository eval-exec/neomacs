//! Phase 0 step 2: a thread-projected variable (its cell `Forwarded` over a
//! `ThreadProjectionDescriptor`, its host-projection bit set) is refused by
//! every reader that works without an owner, and by every checked cached
//! writer, before any of them dereferences a value pointer. The slow paths
//! behind each refusal answer from the variable's canonical storage (round
//! 2); `thread_projection_transparency_test` checks that every operation
//! then behaves as on the unprojected variable.

use super::set_builtin_frontend_for_test;
use super::var_fast::{VarCacheTier, set_var_cache_tiers_for_test};
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::bytecode::{ByteCodeFunction, Vm};
use crate::emacs_core::eval::{Context, SymbolValueLookup};
use crate::emacs_core::forward::{
    CanonicalArm, LispFwdType, ThreadProjectionDescriptor, alloc_objfwd,
};
use crate::emacs_core::intern::{SymId, intern};
use crate::emacs_core::print::print_value;
use crate::emacs_core::symbol::SymbolRedirect;
use crate::emacs_core::value::{LambdaParams, Value};

/// Each variable shape a projection can wrap, with the value it reads
/// before projection.
const SHAPES: &[(&str, &str)] = &[
    ("tpr-plain", "4242"),
    ("tpr-obj", "4343"),
    ("tpr-int", "4444"),
    ("tpr-bool", "t"),
    ("tpr-local", "4546"),
];

fn fixture() -> Context {
    use crate::emacs_core::defvar_bool::ByteBooleanVars;
    let mut ev = Context::new();
    ev.obarray.intern("tpr-obj");
    ev.obarray
        .install_objfwd(intern("tpr-obj"), alloc_objfwd(Value::fixnum(4343)));
    ev.obarray.define_int_variable("tpr-int", 4444);
    ev.obarray
        .define_bool_variable("tpr-bool", true, ByteBooleanVars::ErasedByLreadInit);
    ev.eval_str(
        "(progn
           (defvar tpr-plain 4242)
           (defvar tpr-heap (list 'tpr 1 2 3))
           (defvar tpr-local 4545)
           (make-local-variable 'tpr-local)
           (setq tpr-local 4546)
           (defvaralias 'tpr-alias 'tpr-plain))",
    )
    .expect("fixture evaluates");
    ev
}

fn reader() -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![Op::VarRef(0), Op::Return];
    f
}

/// GNU `Bvarref` of NAME through the VM (`fast_path_var_ref`, then
/// `lookup_var_id`): the printed value or the signal's symbol.
fn vm_read(ev: &mut Context, name: &str) -> String {
    let mut f = reader();
    f.constants = vec![Value::symbol(name)].into();
    f.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(4);
    let mut vm = Vm::from_context(ev);
    match vm.execute(&f, vec![]) {
        Ok(value) => print_value(&value),
        Err(flow) => match flow.into_kind() {
            crate::emacs_core::error::FlowKind::Signal(sig) => sig.symbol_name().to_string(),
            other => format!("{other:?}"),
        },
    }
}

/// The interpreter's read of NAME.
fn eval_read(ev: &mut Context, name: &str) -> String {
    match ev.eval_str(name) {
        Ok(value) => print_value(&value),
        Err(error) => format!("{error:?}"),
    }
}

fn projection_of(ev: &Context, id: SymId) -> &'static ThreadProjectionDescriptor {
    ev.thread_projection_for_test(id)
        .expect("the World's table resolves a projected cell")
}

fn project(ev: &mut Context, name: &str) {
    let issued = ev.thread_projection_count_for_test();
    let projection = ev
        .project_thread_variable_for_test(intern(name))
        .unwrap_or_else(|| panic!("{name} projects"));
    // Slot 0 is the shared VOID descriptor's.
    assert_eq!(projection.index().slot(), issued + 1, "{name}: dense slot");
    assert_eq!(ev.thread_projection_count_for_test(), issued + 1);
    assert!(std::ptr::eq(projection_of(ev, intern(name)), projection));
}

#[test]
fn every_context_free_reader_refuses_a_projected_variable() {
    let mut ev = fixture();
    set_var_cache_tiers_for_test(&[VarCacheTier::Read]);
    set_builtin_frontend_for_test(Some(true));
    for &(name, before) in SHAPES {
        assert_eq!(vm_read(&mut ev, name), before, "{name} before projection");
    }
    for &(name, before) in SHAPES {
        let id = intern(name);
        project(&mut ev, name);
        let sym = ev.obarray.get_by_id(id).expect("symbol");
        assert_eq!(sym.redirect(), SymbolRedirect::Forwarded, "{name}");
        assert!(
            sym.flags().runtime_projected(),
            "{name}: checked writers refuse"
        );
        assert!(!sym.flags().is_plain_untrapped_unprojected(), "{name}");

        // The cell's header is a per-buffer header no buffer has.
        let buf_fwd = sym
            .forwarded_descriptor()
            .and_then(|fwd| fwd.as_buffer_obj_fwd())
            .expect("a per-buffer header");
        assert!(buf_fwd.buffer_slot().is_none(), "{name}: buffer_slot");
        assert_eq!(
            buf_fwd.slot_index().below(usize::from(u16::MAX)),
            None,
            "{name}: slot_index"
        );
        let current = ev.buffers.current_buffer().expect("a current buffer");
        let slot_read = buf_fwd.value_in(
            Some(&current.slots[..]),
            current.local_flags,
            Some(&ev.buffers.buffer_defaults[..]),
        );
        assert!(slot_read.is_unbound(), "{name}: value_in");

        // Obarray readers: `do_symval_forwarding` without context. The
        // per-buffer readers answer the header's UNBOUND default; their
        // context-aware callers find the projection first.
        assert_eq!(ev.obarray.symbol_value_id_copied(id), None, "{name}");
        assert_eq!(
            ev.obarray.find_symbol_value(id),
            Some(Value::UNBOUND),
            "{name}: find_symbol_value"
        );
        // ROUND-2 HOOK: the per-buffer arm's default read finds the
        // projection and answers the canonical default.
        assert_eq!(
            ev.obarray
                .default_value_id_copied(id)
                .as_ref()
                .map(print_value)
                .as_deref(),
            Some(if name == "tpr-local" { "4545" } else { before }),
            "{name}: default_value_id_copied"
        );
        assert!(
            matches!(
                ev.obarray.value_without_buffer_id(id),
                crate::emacs_core::symbol::BufferlessValue::Void
            ),
            "{name}: value_without_buffer_id"
        );
        // Stage A cached tiers (`var_fast.rs`) and the Rust builtin frontend.
        assert_eq!(ev.read_var_cached(id), None, "{name}: read_var_cached");
        // The builtin frontend's cached read refuses, and its fallback
        // (`special_variable_value_by_id`) answers the canonical value.
        assert_eq!(
            ev.builtin_var_value(id)
                .as_ref()
                .map(print_value)
                .as_deref(),
            Some(before),
            "{name}: builtin_var_value"
        );
        // The interpreter's descriptor shortcut refuses; its per-buffer
        // shortcut finds the projection on the slot it lacks.
        assert_eq!(ev.forwarded_descriptor_value(sym), None, "{name}");
        assert!(
            matches!(
                ev.forwarded_buffer_obj_value(id, sym),
                Some(SymbolValueLookup::Bound(_))
            ),
            "{name}"
        );
        // Behind the refusals, the VM's void branch and the interpreter's
        // per-buffer branch answer the canonical value.
        assert_eq!(vm_read(&mut ev, name), before, "{name}: VM varref");
        assert_eq!(eval_read(&mut ev, name), before, "{name}: eval");
        assert!(
            matches!(
                ev.find_symbol_value_by_id(id),
                Ok(SymbolValueLookup::Bound(value)) if print_value(&value) == before
            ),
            "{name}: find_symbol_value_by_id"
        );
    }
    set_var_cache_tiers_for_test(&[]);
    set_builtin_frontend_for_test(None);
}

/// The canonical record keeps exactly what the cell held, under the cell's
/// logical redirect.
#[test]
fn projection_keeps_the_cells_logical_storage() {
    let mut ev = fixture();
    for &(name, _) in SHAPES {
        project(&mut ev, name);
    }
    let arm = |ev: &Context, name: &str| projection_of(ev, intern(name)).arm();
    assert_eq!(arm(&ev, "tpr-plain"), CanonicalArm::Plain);
    assert_eq!(
        projection_of(&ev, intern("tpr-plain")).plain_value(),
        Value::fixnum(4242)
    );
    assert!(matches!(arm(&ev, "tpr-local"), CanonicalArm::Localized(_)));
    for (name, ty) in [
        ("tpr-obj", LispFwdType::Obj),
        ("tpr-int", LispFwdType::Int),
        ("tpr-bool", LispFwdType::Bool),
    ] {
        match arm(&ev, name) {
            CanonicalArm::Forwarded(builtin) => {
                assert_eq!(builtin.descriptor().ty(), ty, "{name}");
            }
            other => panic!("{name} kept {other:?}"),
        }
    }
}

/// An alias is never projected (it resolves to its base), and a projection
/// is never projected again.
#[test]
fn aliases_and_projections_are_not_projected() {
    let mut ev = fixture();
    assert_eq!(
        ev.project_thread_variable_for_test(intern("tpr-alias"))
            .map(ThreadProjectionDescriptor::index),
        None
    );
    project(&mut ev, "tpr-plain");
    assert_eq!(
        ev.project_thread_variable_for_test(intern("tpr-plain"))
            .map(ThreadProjectionDescriptor::index),
        None
    );
    assert_eq!(ev.thread_projection_count_for_test(), 1);
    // The alias still names its base and reads its canonical value.
    assert_eq!(vm_read(&mut ev, "tpr-alias"), "4242");
}

/// Every checked cached writer refuses a projection on the host-projection
/// bit: nothing is stored, nothing is pushed, the canonical word is intact.
#[test]
fn checked_cached_writers_refuse_a_projected_variable() {
    let mut ev = fixture();
    set_var_cache_tiers_for_test(&[
        VarCacheTier::Read,
        VarCacheTier::Set,
        VarCacheTier::Bind,
        VarCacheTier::Unbind,
    ]);
    for &(name, _) in SHAPES {
        project(&mut ev, name);
        let id = intern(name);
        let depth = ev.specpdl.len();
        assert!(!ev.try_set_var_cached(id, Value::fixnum(1)), "{name}: set");
        assert!(!ev.specbind_cached(id, Value::fixnum(2)), "{name}: bind");
        assert_eq!(ev.specpdl.len(), depth, "{name}: nothing pushed");
        assert!(
            !ev.obarray.is_plain_value_cell_id(id),
            "{name}: plain writers refuse"
        );
    }
    assert_eq!(
        projection_of(&ev, intern("tpr-plain")).plain_value(),
        Value::fixnum(4242)
    );
    set_var_cache_tiers_for_test(&[]);
}

/// A projected plain variable's canonical word is a GC root (an objfwd the
/// obarray traces): the heap value it held survives an exact collection,
/// and so does one written through the projection.
#[test]
fn canonical_plain_word_survives_collection() {
    let mut ev = fixture();
    project(&mut ev, "tpr-heap");
    ev.gc_collect_exact();
    assert_eq!(
        print_value(&projection_of(&ev, intern("tpr-heap")).plain_value()),
        "(tpr 1 2 3)"
    );
    ev.eval_str("(setq tpr-heap (list 'tpr 4 5))")
        .expect("setq evaluates");
    ev.gc_collect_exact();
    assert_eq!(
        print_value(&projection_of(&ev, intern("tpr-heap")).plain_value()),
        "(tpr 4 5)"
    );
    assert_eq!(eval_read(&mut ev, "tpr-heap"), "(tpr 4 5)");
}

/// A copy of a projected obarray is unprojected: each projected cell gets
/// (a copy of) the variable it projects and its logical host bit back, and
/// writes to the copy do not reach the source's canonical storage.
#[test]
fn obarray_clone_normalizes_projections() {
    let mut ev = fixture();
    let unprojected = fixture();
    for &(name, _) in SHAPES {
        project(&mut ev, name);
    }
    let mut copy = ev.obarray.clone();
    for &(name, _) in SHAPES {
        let id = intern(name);
        let sym = copy.get_by_id(id).expect("symbol");
        let want = unprojected.obarray.get_by_id(id).expect("symbol");
        assert_eq!(sym.redirect(), want.redirect(), "{name}: logical redirect");
        assert_eq!(
            sym.flags().runtime_projected(),
            want.flags().runtime_projected(),
            "{name}: logical host bit"
        );
        assert!(
            sym.forwarded_descriptor()
                .and_then(|fwd| fwd.thread_projection())
                .is_none(),
            "{name}: no projection in the copy"
        );
        assert_eq!(
            copy.default_value_id_copied(id),
            unprojected.obarray.default_value_id_copied(id),
            "{name}: default"
        );
    }
    copy.set_symbol_value_id(intern("tpr-plain"), Value::fixnum(1));
    copy.set_symbol_value_id(intern("tpr-obj"), Value::fixnum(2));
    assert_eq!(
        projection_of(&ev, intern("tpr-plain")).plain_value(),
        Value::fixnum(4242)
    );
    assert_eq!(eval_read(&mut ev, "tpr-obj"), "4343");
}

/// An image holds no owners: a projected symbol dumps exactly as the
/// variable it projects.
#[test]
fn pdump_writes_the_projected_variable() {
    use crate::emacs_core::pdump::convert::dumped_symbol_value_for_test;
    let mut ev = fixture();
    let unprojected = fixture();
    for &(name, _) in SHAPES {
        project(&mut ev, name);
    }
    for &(name, _) in SHAPES {
        let id = intern(name);
        assert_eq!(
            dumped_symbol_value_for_test(ev.obarray.get_by_id(id).expect("symbol")),
            dumped_symbol_value_for_test(unprojected.obarray.get_by_id(id).expect("symbol")),
            "{name}"
        );
    }
}

/// A projected LOCALIZED read in a buffer whose registered object does not
/// exist allocates nothing: admission republishes its host mirrors through
/// this exact reader with the collector closed (`republish_owner_mirrors`),
/// and a buffer's canonical object is materialized lazily. The value is the
/// one the buffer's own index names, the read materializes no object, and a
/// later read stays correct once the object exists and the cache can name it.
#[test]
fn projected_localized_read_allocates_nothing_without_the_buffer_object() {
    let mut ev = fixture();
    // A buffer no Lisp object exists for yet, with its own local binding of
    // the projected variable's base.
    let buffer_id = ev.buffers.create_buffer("tpr-unmaterialized");
    let id = intern("tpr-local");
    ev.buffers
        .set_buffer_local_property_by_sym_id(buffer_id, id, Value::fixnum(4547))
        .expect("the local binding is written");
    let buffer_object = || crate::tagged::gc::with_tagged_heap(|heap| heap.buffer_value(buffer_id));
    assert!(buffer_object().is_none(), "the buffer object starts absent");
    let allocated = ev.tagged_heap.allocated_count();

    project(&mut ev, "tpr-local");
    let projection = projection_of(&ev, id);
    let CanonicalArm::Localized(blv) = projection.arm() else {
        panic!("tpr-local keeps a localized arm")
    };
    let buffer = ev.buffers.get(buffer_id).expect("the buffer");
    let value = ev.obarray.read_projected_localized(id, blv, Some(buffer));
    assert_eq!(print_value(&value), "4547");
    assert_eq!(
        ev.tagged_heap.allocated_count(),
        allocated,
        "the read allocated nothing"
    );
    assert!(buffer_object().is_none(), "the read materialized no object");

    let object = Value::make_buffer(buffer_id);
    let buffer = ev.buffers.get(buffer_id).expect("the buffer");
    let value = ev.obarray.read_projected_localized(id, blv, Some(buffer));
    assert_eq!(print_value(&value), "4547");
    assert!(
        buffer_object().is_some_and(|value| crate::emacs_core::value::eq_value(&value, &object))
    );
}

/// A different view type makes the value-layer boundary explicit and counts
/// local-cell lookups independently of the BLV cache's returned value.
struct ProjectionBufferProbe<'a> {
    buffer: &'a crate::buffer::Buffer,
    local_cell_reads: &'a std::cell::Cell<usize>,
}

impl crate::emacs_core::value::buffer_vocab::BufferLocalView for ProjectionBufferProbe<'_> {
    fn buffer_id(&self) -> crate::emacs_core::value::buffer_vocab::BufferId {
        self.buffer.id
    }

    fn slot_value(&self, slot: crate::emacs_core::value::buffer_vocab::BufferSlot) -> Value {
        self.buffer.slots[slot.index()]
    }

    fn local_binding(&self, id: SymId, localized: bool) -> Option<Value> {
        self.buffer.get_buffer_local_by_sym_id_gated(id, localized)
    }

    fn local_binding_cell(&self, id: SymId) -> Option<Value> {
        self.local_cell_reads.set(self.local_cell_reads.get() + 1);
        self.buffer.local_variable_binding_cell(id)
    }
}

#[test]
fn projected_localized_generic_view_keeps_cache_hits_lazy() {
    let mut ev = fixture();
    let id = intern("tpr-local");
    let buffer_id = ev.buffers.create_buffer("tpr-generic-registered");
    ev.buffers
        .set_buffer_local_property_by_sym_id(buffer_id, id, Value::fixnum(4547))
        .expect("local binding");
    let object = Value::make_buffer(buffer_id);
    project(&mut ev, "tpr-local");
    let CanonicalArm::Localized(blv) = projection_of(&ev, id).arm() else {
        panic!("localized projection")
    };
    let reads = std::cell::Cell::new(0);
    let view = ProjectionBufferProbe {
        buffer: ev.buffers.get(buffer_id).expect("buffer"),
        local_cell_reads: &reads,
    };
    let allocated = ev.tagged_heap.allocated_count();
    assert_eq!(
        ev.obarray.read_projected_localized(id, blv, Some(&view)),
        Value::fixnum(4547)
    );
    assert_eq!(reads.get(), 1, "first read loads the new buffer's cell");
    // SAFETY: this obarray owns the projected BLV and no reference to the
    // record is retained across a read that may refresh its cache.
    unsafe {
        let cache = &*blv.blv().as_ptr();
        assert_eq!(cache.where_buf.get(), object);
        assert_eq!(cache.where_buf_id, buffer_id.0);
        assert!(cache.found);
        assert_eq!(
            Some(cache.valcell.get()),
            view.buffer.local_variable_binding_cell(id)
        );
        assert_eq!(
            cache.alist_epoch,
            crate::emacs_core::symbol::blv_alist_epoch()
        );
    }
    assert_eq!(
        ev.obarray.read_projected_localized(id, blv, Some(&view)),
        Value::fixnum(4547)
    );
    assert_eq!(reads.get(), 1, "cache hit must not resolve the cell again");
    assert_eq!(
        ev.obarray
            .read_projected_localized(id, blv, None::<&ProjectionBufferProbe<'_>>),
        Value::fixnum(4545)
    );
    assert_eq!(reads.get(), 1, "bufferless default needs no local lookup");
    // SAFETY: same owned BLV; the preceding bufferless read has completed.
    unsafe {
        assert_eq!((*blv.blv().as_ptr()).where_buf.get(), object);
        assert!((*blv.blv().as_ptr()).found);
    }
    assert_eq!(ev.tagged_heap.allocated_count(), allocated);
}

#[test]
fn projected_localized_generic_view_keeps_unregistered_cache_locus_nil() {
    let mut ev = fixture();
    let id = intern("tpr-local");
    let buffer_id = ev.buffers.create_buffer("tpr-generic-unregistered");
    project(&mut ev, "tpr-local");
    let CanonicalArm::Localized(blv) = projection_of(&ev, id).arm() else {
        panic!("localized projection")
    };
    let reads = std::cell::Cell::new(0);
    {
        let view = ProjectionBufferProbe {
            buffer: ev.buffers.get(buffer_id).expect("buffer"),
            local_cell_reads: &reads,
        };
        for expected_reads in 1..=2 {
            assert_eq!(
                ev.obarray.read_projected_localized(id, blv, Some(&view)),
                Value::fixnum(4545)
            );
            assert_eq!(reads.get(), expected_reads);
            // SAFETY: the projected BLV is owned by this obarray; inspect
            // only after each cache-writing read has returned.
            unsafe {
                let cache = &*blv.blv().as_ptr();
                assert!(cache.where_buf.get().is_nil());
                assert_eq!(cache.where_buf_id, crate::emacs_core::symbol::NO_WHERE_BUF);
                assert!(!cache.found);
                assert_eq!(cache.valcell.get(), cache.defcell.get());
                assert_eq!(
                    cache.alist_epoch,
                    crate::emacs_core::symbol::blv_alist_epoch()
                );
            }
        }
    }
    ev.buffers
        .set_buffer_local_property_by_sym_id(buffer_id, id, Value::fixnum(4548))
        .expect("new local binding");
    let view = ProjectionBufferProbe {
        buffer: ev.buffers.get(buffer_id).expect("buffer"),
        local_cell_reads: &reads,
    };
    let allocated = ev.tagged_heap.allocated_count();
    assert_eq!(
        ev.obarray.read_projected_localized(id, blv, Some(&view)),
        Value::fixnum(4548)
    );
    assert_eq!(reads.get(), 3);
    // SAFETY: same owned BLV; no cache writer overlaps these assertions.
    unsafe {
        let cache = &*blv.blv().as_ptr();
        assert!(cache.where_buf.get().is_nil());
        assert_eq!(cache.where_buf_id, crate::emacs_core::symbol::NO_WHERE_BUF);
        assert!(cache.found);
        assert_eq!(
            Some(cache.valcell.get()),
            view.buffer.local_variable_binding_cell(id)
        );
        assert_eq!(
            cache.alist_epoch,
            crate::emacs_core::symbol::blv_alist_epoch()
        );
    }
    assert!(crate::tagged::gc::with_tagged_heap(|heap| heap.buffer_value(buffer_id)).is_none());
    assert_eq!(ev.tagged_heap.allocated_count(), allocated);
}

#[test]
fn projected_localized_alias_preserves_local_void_and_bound_default() {
    let mut ev = fixture();
    ev.eval_str("(defvaralias 'tpr-local-alias 'tpr-local)")
        .expect("localized alias");
    project(&mut ev, "tpr-local");
    assert_eq!(vm_read(&mut ev, "tpr-local-alias"), "4546");
    assert_eq!(eval_read(&mut ev, "tpr-local-alias"), "4546");
    ev.eval_str("(makunbound 'tpr-local)")
        .expect("void local binding");
    assert!(matches!(
        ev.find_symbol_value_by_id(intern("tpr-local-alias")),
        Ok(SymbolValueLookup::Unbound)
    ));
    assert_eq!(
        print_value(
            &ev.eval_str("(list (boundp 'tpr-local-alias) (default-value 'tpr-local-alias))")
                .expect("void alias and bound default")
        ),
        "(nil 4545)"
    );
}
