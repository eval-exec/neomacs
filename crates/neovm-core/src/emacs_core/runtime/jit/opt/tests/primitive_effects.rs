//! Primitive opcode effects describe the primitive body, independently of
//! its symbol's function cell. Genuine calls and hook-running primitives
//! retain collection and reentry effects.

use super::*;

fn assert_non_reentrant(op: &Op) -> Effects {
    let (effects, _) = op_effects(op);
    assert!(
        !effects.intersects(Effects::MAY_GC.with(Effects::MAY_REENTER)),
        "{op:?} runs its primitive directly: {effects:?}"
    );
    effects
}

#[test]
fn opt_primitive_array_opcodes_may_signal_without_running_lisp() {
    for op in [Op::Aref, Op::Aset] {
        let effects = assert_non_reentrant(&op);
        assert!(effects.contains(Effects::MAY_SIGNAL), "{op:?}");
        // Non-vector representations can allocate char-table substructures
        // or closure slots; allocation itself does not collect.
        assert!(effects.contains(Effects::ALLOCATES), "{op:?}");
    }
    assert!(op_effects(&Op::Aset).0.contains(Effects::WRITE_HEAP));
}

#[test]
fn opt_primitive_list_and_string_opcodes_use_body_effects() {
    for op in [
        Op::Car,
        Op::Cdr,
        Op::Setcar,
        Op::Setcdr,
        Op::Length,
        Op::Nth,
        Op::Nthcdr,
        Op::Elt,
        Op::Memq,
        Op::Member,
        Op::Assq,
        Op::Nreverse,
        Op::Nconc,
        Op::Equal,
        Op::StringEqual,
        Op::StringLessp,
        Op::Substring,
        Op::Concat(2),
        Op::Fset,
    ] {
        assert!(
            assert_non_reentrant(&op).contains(Effects::MAY_SIGNAL),
            "{op:?}"
        );
    }
    for op in [Op::CarSafe, Op::CdrSafe, Op::Cons, Op::List(2)] {
        assert!(
            !assert_non_reentrant(&op).contains(Effects::MAY_SIGNAL),
            "{op:?}"
        );
    }
    assert!(op_effects(&Op::Substring).0.contains(Effects::ALLOCATES));
}

#[test]
fn opt_primitive_numeric_opcodes_preserve_signal_effects_before_refinement() {
    for op in [
        Op::Add,
        Op::Sub,
        Op::Mul,
        Op::Div,
        Op::Rem,
        Op::Add1,
        Op::Sub1,
        Op::Negate,
        Op::Max,
        Op::Min,
        Op::Eqlsign,
        Op::Lss,
        Op::Gtr,
        Op::Leq,
        Op::Geq,
    ] {
        assert!(
            assert_non_reentrant(&op).contains(Effects::MAY_SIGNAL),
            "{op:?}"
        );
    }
}

#[test]
fn opt_primitive_safepoints_follow_gc_and_reentry_effects() {
    let opaque = Opcode::Opaque(Op::Aset);
    assert!(!opaque.is_safepoint(Effects::WRITE_HEAP.with(Effects::MAY_SIGNAL)));
    assert!(opaque.is_safepoint(Effects::MAY_GC));
    assert!(opaque.is_safepoint(Effects::MAY_REENTER));
    assert!(!Opcode::OpaqueBool(Op::StringEqual).is_safepoint(Effects::READ_HEAP));
    assert!(Opcode::Poll.is_safepoint(Effects::UNKNOWN));
}

#[test]
fn opt_primitive_hooks_and_genuine_calls_keep_reentry_and_roots() {
    for op in [
        Op::Call(1),
        Op::Apply(1),
        Op::CallBuiltinSym(crate::emacs_core::intern::intern("insert"), 1),
        Op::Set,
        Op::Unbind(1),
        Op::SaveWindowExcursion,
    ] {
        let effects = op_effects(&op).0;
        assert!(effects.contains(Effects::MAY_REENTER), "{op:?}");
        assert!(effects.contains(Effects::MAY_GC), "{op:?}");
        assert!(Opcode::Opaque(op.clone()).is_safepoint(effects), "{op:?}");
    }
}
