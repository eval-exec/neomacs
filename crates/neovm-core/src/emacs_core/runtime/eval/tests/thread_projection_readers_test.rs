//! Phase 0 step 2: a thread-projected variable (its cell `Forwarded` over a
//! `ThreadProjectionDescriptor`, its host-projection bit set) is refused by
//! every reader that works without an owner, and by every checked cached
//! writer, before any of them dereferences a value pointer. Admission does
//! not exist yet, so the owner-aware slow paths behind each refusal still
//! answer as for a void variable; these tests pin the refusal, not GNU
//! thread semantics.

use super::set_builtin_frontend_for_test;
use super::var_fast::{VarCacheTier, set_var_cache_tiers_for_test};
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::bytecode::{ByteCodeFunction, Vm};
use crate::emacs_core::eval::{Context, SymbolValueLookup};
use crate::emacs_core::forward::{
    CanonicalStorage, LispFwdType, ThreadProjectionDescriptor, alloc_objfwd,
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
    assert_eq!(projection.index().slot(), issued, "{name}: dense slot");
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
        // per-buffer readers answer the header's UNBOUND default, which
        // their callers' void handling refuses (a round-2 hook point).
        assert_eq!(ev.obarray.symbol_value_id_copied(id), None, "{name}");
        assert_eq!(
            ev.obarray.find_symbol_value(id),
            Some(Value::UNBOUND),
            "{name}: find_symbol_value"
        );
        assert_eq!(
            ev.obarray.default_value_id_copied(id),
            Some(Value::UNBOUND),
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
        assert_eq!(
            ev.builtin_var_value(id).filter(|value| !value.is_unbound()),
            None,
            "{name}: builtin_var_value"
        );
        // The interpreter's descriptor shortcuts.
        assert_eq!(ev.forwarded_descriptor_value(sym), None, "{name}");
        assert_eq!(ev.forwarded_buffer_obj_value(sym), None, "{name}");
        // The VM and the interpreter fall back to the general walk, which
        // has no owner to ask yet: void, never the old value.
        let vm = vm_read(&mut ev, name);
        assert_eq!(vm, "void-variable", "{name}: VM varref");
        assert_ne!(vm, before);
        // ROUND-2 HOOK: the interpreter's general walk ends at
        // `Obarray::find_symbol_value`, which answers the header's UNBOUND
        // default, and `find_symbol_value_by_id` binds it. Real per-buffer
        // reads never get there (`forwarded_buffer_obj_value` answers
        // first), so the owner-aware read goes on that cold fallback.
        let interp = eval_read(&mut ev, name);
        assert_eq!(interp, "#<unbound>", "{name}: eval");
        assert!(
            matches!(
                ev.find_symbol_value_by_id(id),
                Ok(SymbolValueLookup::Bound(value)) if value.is_unbound()
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
    let canonical = |ev: &Context, name: &str| projection_of(ev, intern(name)).canonical();
    match canonical(&ev, "tpr-plain") {
        CanonicalStorage::Plain(cell) => assert_eq!(cell.get(), Value::fixnum(4242)),
        other => panic!("tpr-plain kept {other:?}"),
    }
    assert!(matches!(
        canonical(&ev, "tpr-local"),
        CanonicalStorage::Localized(_)
    ));
    for (name, ty) in [
        ("tpr-obj", LispFwdType::Obj),
        ("tpr-int", LispFwdType::Int),
        ("tpr-bool", LispFwdType::Bool),
    ] {
        match canonical(&ev, name) {
            CanonicalStorage::Forwarded(builtin) => {
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
    // The alias still names its base; reading through it refuses with it.
    assert_eq!(vm_read(&mut ev, "tpr-alias"), "void-variable");
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
    match projection_of(&ev, intern("tpr-plain")).canonical() {
        CanonicalStorage::Plain(cell) => assert_eq!(cell.get(), Value::fixnum(4242)),
        other => panic!("tpr-plain kept {other:?}"),
    }
    set_var_cache_tiers_for_test(&[]);
}

/// A projected plain variable's canonical word is a GC root: the heap value
/// it held survives an exact collection.
#[test]
fn canonical_plain_word_survives_collection() {
    let mut ev = fixture();
    project(&mut ev, "tpr-heap");
    ev.gc_collect_exact();
    match projection_of(&ev, intern("tpr-heap")).canonical() {
        CanonicalStorage::Plain(cell) => assert_eq!(print_value(&cell.get()), "(tpr 1 2 3)"),
        other => panic!("tpr-heap kept {other:?}"),
    }
}
