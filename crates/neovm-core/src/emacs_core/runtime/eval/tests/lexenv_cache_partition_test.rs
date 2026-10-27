//! The lexenv assq cache reserves its first two slots for nil and t.
//!
//! GNU `eval_sub` consults the lexical environment for constants too: a
//! crafted environment binds them (`(eval 'nil '((nil . 42)))` -> 42), so the
//! cache still stores their answers.  But the DEFSYM const prefix assigns
//! fixed ids, and the old low-nibble index let a hot dynamic symbol evict the
//! nil entry on every iteration of `(while (re-search-forward ... nil t))`.
//! These tests pin the partition and the GNU constant-binding behaviour.

use super::*;
use crate::emacs_core::intern::{NIL_SYM_ID, SymId, T_SYM_ID, intern};
use crate::emacs_core::{Context, Value};

#[test]
fn constant_slots_are_reserved_from_every_dynamic_symbol() {
    for id in 0..256u32 {
        let sym = SymId(id);
        match LexenvAssqCacheSlot::of(0x1234_5678_9abc_usize, sym) {
            LexenvAssqCacheSlot::Constant(constant) => {
                assert_eq!(constant.symbol(), sym);
                assert!(id < 2, "SymId({id}) classified as a constant");
            }
            LexenvAssqCacheSlot::Dynamic(slot) => {
                assert!(id >= 2, "seed SymId({id}) classified as dynamic");
                assert!(
                    slot.index() >= LexenvConstant::COUNT,
                    "dynamic SymId({id}) landed in constant slot {}",
                    slot.index()
                );
            }
        }
    }
}

#[test]
fn colliding_dynamic_read_keeps_the_cached_constant_answer() {
    let ev = Context::new();
    let lexenv = Value::NIL;
    assert!(ev.lexenv_lookup_cached_in(lexenv, NIL_SYM_ID).is_none());

    // The top-level environment contributes no pointer bits to the old
    // `mixed & 15` index, and the multiplier is 1 modulo 16, so every id
    // congruent to 0 (mod 16) used to land in nil's slot.  Interning a few
    // names finds one regardless of the process-global registry state.
    let mut colliding = None;
    for probe in 0..64 {
        let sym = intern(&format!("lexenv-cache-partition-probe-{probe}"));
        if sym.0 & 15 == 0 {
            colliding = Some(sym);
            break;
        }
    }
    let colliding = colliding.expect("64 probes must include an id congruent to 0 (mod 16)");
    assert!(ev.lexenv_lookup_cached_in(lexenv, colliding).is_none());

    let entry = ev.lexenv_assq_cache.entries[LexenvConstant::Nil.slot()]
        .get()
        .expect("the dynamic lookup evicted nil's cached answer");
    assert_eq!(entry.symbol, NIL_SYM_ID);
    assert_eq!(entry.lexenv_bits, lexenv.bits());
    assert!(entry.cell.is_nil());
}

#[test]
fn crafted_lexenv_constant_binding_survives_dynamic_reads() {
    let mut ev = Context::new();
    assert_eq!(
        ev.eval_str("(eval 'nil '((nil . 42)))").unwrap(),
        Value::fixnum(42)
    );
    assert_eq!(
        ev.eval_str("(eval 't '((t . 43)))").unwrap(),
        Value::fixnum(43)
    );
    ev.eval_str("(defvar lexenv-cache-partition-dynamic 7)")
        .unwrap();
    // A quoted alist is a constant object, so all four reads use the same
    // environment identity and the cache answers from its dedicated slots.
    let result = ev
        .eval_str(
            "(let ((env '((nil . 42) (t . 43))))
               (list (eval 'nil env)
                     (eval 'lexenv-cache-partition-dynamic env)
                     (eval 'nil env)
                     (eval 't env)))",
        )
        .unwrap();
    assert_eq!(
        result,
        Value::list(vec![
            Value::fixnum(42),
            Value::fixnum(7),
            Value::fixnum(42),
            Value::fixnum(43),
        ])
    );
}
