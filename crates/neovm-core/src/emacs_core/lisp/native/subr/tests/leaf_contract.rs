//! The leaf registry's contract (design `p1-2-builtin-intrinsics` §10.1):
//! dense, stable ids; the forbidden effects excluded; every Bcall leaf
//! attached to the builtin it names, and nothing else attached; a leaf that
//! changes under a registered builtin drops compiled code.

use super::*;
use crate::emacs_core::intern::intern;

/// The AOT leaf table (design §2.9 phase 2) would be indexed by these: a
/// renumbering must be a deliberate edit of this list.
#[test]
fn leaf_ids_are_dense_and_stable() {
    let golden = [
        (LeafId::Gethash, "gethash"),
        (LeafId::PlistGet, "plist-get"),
        (LeafId::GetCharProperty, "get-char-property"),
        (LeafId::Get, "get"),
        (LeafId::Length, "length"),
        (LeafId::Nth, "nth"),
        (LeafId::Nthcdr, "nthcdr"),
        (LeafId::Elt, "elt"),
        (LeafId::Memq, "memq"),
        (LeafId::Assq, "assq"),
        (LeafId::Member, "member"),
        (LeafId::Equal, "equal"),
        (LeafId::StringEqual, "string-equal"),
        (LeafId::StringLessp, "string-lessp"),
    ];
    assert_eq!(golden.len(), LeafId::COUNT);
    assert_eq!(LEAVES.len(), LeafId::COUNT);
    for (index, (id, name)) in golden.into_iter().enumerate() {
        assert_eq!(id.index(), index, "{name}");
        assert_eq!(LEAVES[index].id, id, "{name}: LEAVES is indexed by LeafId");
        assert_eq!(LEAVES[index].name, name);
        assert!(std::ptr::eq(id.spec(), LEAVES[index]));
    }
}

/// The const constructor already refuses these; restated at run time so a
/// declaration that bypassed it (a struct literal) is caught too.
#[test]
fn leaf_effects_exclude_gc_reentry_deopt_binding_writes() {
    for leaf in LEAVES {
        for forbidden in [
            Effects::MAY_GC,
            Effects::MAY_REENTER,
            Effects::MAY_DEOPT,
            Effects::WRITE_BINDINGS,
        ] {
            assert!(
                !leaf.effects.intersects(forbidden),
                "{} declares {forbidden:?}",
                leaf.name
            );
        }
    }
}

/// A Bcall leaf is found through the builtin it names, and an opcode leaf
/// through no builtin: its answers are the opcode's (`Bnth` signals with the
/// tail, `Fnth` with the list), which a `Bcall` of the builtin must not get.
#[test]
fn every_bcall_leaf_is_attached_to_its_registered_subr() {
    let ctx = Context::new();
    for leaf in LEAVES {
        let sym = intern(leaf.name);
        assert!(
            crate::emacs_core::eval::lookup_global_subr_entry(sym).is_some(),
            "{} names a registered builtin",
            leaf.name
        );
        match leaf.shape {
            LeafShape::Bcall => {
                let attached = subr_leaf(sym).expect("a Bcall leaf is attached");
                assert!(std::ptr::eq(attached, leaf), "{}", leaf.name);
                assert!(
                    ctx.obarray.symbol_function_id(sym).is_some(),
                    "{} is fbound",
                    leaf.name
                );
            }
            LeafShape::Opcode => {
                assert!(subr_leaf(sym).is_none(), "{} is opcode-only", leaf.name);
            }
        }
    }
}

fn test_gethash(_: &Context, _: Value, _: Value, _: Value) -> LeafResult {
    Err(LeafExit::Generic)
}

/// A second declaration for `gethash`: what a test re-registering the
/// builtin with a different leaf would install.
static OTHER_GETHASH: LeafSpec = LeafSpec::new(
    LeafId::Gethash,
    "gethash",
    LeafEntry::L3(test_gethash),
    LeafShape::Bcall,
    Effects::READ_HEAP,
    &[],
    Containment::Catch,
);

#[test]
fn recording_the_same_leaf_again_is_unchanged() {
    let _ctx = Context::new();
    let sym = intern("gethash");
    let current = subr_leaf(sym).expect("attached");
    assert_eq!(record_subr_leaf(sym, Some(current)), LeafChange::Unchanged);
    assert_eq!(
        record_subr_leaf(intern("leaf-contract-no-such-builtin"), None),
        LeafChange::Unchanged
    );
}

/// Registration normally runs before any compile; a leaf that changes under
/// a builtin afterwards must not stay baked into compiled code.
#[cfg(feature = "jit")]
#[test]
fn reinstalling_a_different_leaf_clears_the_jit_cache() {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::jit::cache;
    use crate::emacs_core::subr::{FixedMin3, SubrSpec};
    use crate::emacs_core::value::LambdaParams;

    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ctx = Context::new();
    // `(lambda (a) (+ a 1))`, hot, so one call compiles and caches it.
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![Op::StackRef(0), Op::Add1, Op::Return];
    f.max_stack = 4;
    f.jit_runtime().set_hot_for_test();
    let callee = Value::make_bytecode(f);
    crate::emacs_core::eval::push_scratch_gc_root(callee);
    assert_eq!(
        ctx.funcall_general_untraced(callee, vec![Value::fixnum(41)])
            .expect("runs"),
        Value::fixnum(42)
    );
    let id = callee
        .get_bytecode_data()
        .expect("bytecode")
        .jit_runtime()
        .compiled_id()
        .expect("tiered up");
    assert_eq!(cache::cache_entry_kind_for_test(id), "compiled", "premise");

    let spec = SubrSpec::fixed3(
        "gethash",
        crate::emacs_core::builtins::builtin_gethash_3,
        FixedMin3::Two,
    );
    // The same leaf again keeps the cache.
    ctx.register_subr(spec.leaf(&crate::emacs_core::builtins::leaves::GETHASH));
    assert_eq!(cache::cache_entry_kind_for_test(id), "compiled");
    // A different one clears it.
    ctx.register_subr(spec.leaf(&OTHER_GETHASH));
    assert_eq!(cache::cache_entry_kind_for_test(id), "none");
    assert!(std::ptr::eq(
        subr_leaf(intern("gethash")).expect("attached"),
        &OTHER_GETHASH
    ));
    // Restore the real one for the rest of this thread.
    ctx.register_subr(spec.leaf(&crate::emacs_core::builtins::leaves::GETHASH));
}
