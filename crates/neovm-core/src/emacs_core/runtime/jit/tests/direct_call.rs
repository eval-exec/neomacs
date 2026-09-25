//! Direct native calls (design `p1-1-direct-native-calls` §3.3-§3.5, P1.0
//! S2.1b/S2.1c): a speculated call site of a compiled caller enters a
//! compiled, exact-arity, frameless callee through the callee's register
//! entry, armed in the site's spec slot.
//!
//! T12 pins the arming; the behavioural tests (T2-T10) compare every
//! observable -- the value or signal, `mapbacktrace` and `backtrace-frame`,
//! the debugger's entry and exit, `max-lisp-eval-depth`, quits,
//! redefinition, deopts, contained panics -- between direct calls on, off,
//! `NEOVM_JIT_FORCE_SLOW_SPEC`, and the interpreter.

use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;

/// The compiled leaf the cache holds for the function SYM names.
fn cached_leaf(ev: &Context, sym: &str) -> Option<&'static CompiledLeaf> {
    let f = ev.obarray.symbol_function_id(intern(sym))?;
    let id = f.get_bytecode_data()?.jit_runtime().compiled_id_or_assign();
    let ptr = crate::emacs_core::jit::cache::compiled_leaf_ptr_for_test(id)?;
    // SAFETY: a cached leaf, alive while the cache holds it (the test does
    // not clear the cache while it looks).
    Some(unsafe { &*ptr })
}

/// The caller's Bytecode spec slot that caches CALLEE's leaf.
fn slot_calling<'a>(caller: &'a CompiledLeaf, callee: &CompiledLeaf) -> Option<&'a SpecSlot> {
    caller
        .bytecode_spec_slots()
        .find(|slot| std::ptr::eq(slot.leaf_ptr(), callee))
}

/// Callees of every arming shape. Each has a branch or an allocation, so
/// MIR's pure single-block inliner leaves its call a call.
const ARMING: &str = r#"(progn
  (defvar neovm--dc-special nil)
  (defun neovm--dc-exact (a b) (if (> a b) (- a b) (+ a b)))
  (defun neovm--dc-framed (a b) (let ((neovm--dc-special a)) (+ a b neovm--dc-special)))
  (defun neovm--dc-opt (a &optional b) (list a b))
  (defun neovm--dc-rest (a &rest r) (cons a r))
  (defun neovm--dc-wide (a b c d e f g) (list a b c d e f g))
  (defun neovm--dc-caller (x)
    (list (neovm--dc-exact x 1) (neovm--dc-framed x 2) (neovm--dc-opt x)
          (neovm--dc-rest x 1 2) (neovm--dc-wide x 1 2 3 4 5 6)))
  (dolist (f '(neovm--dc-exact neovm--dc-framed neovm--dc-opt neovm--dc-rest
               neovm--dc-wide neovm--dc-caller))
    (byte-compile f))
  (dotimes (i 6000) (neovm--dc-caller i)))"#;

/// Warm the arming program with direct calls ON or OFF and answer each
/// callee's slot in the caller: `(name, leaf entry, the slot's direct
/// entry)`.
fn armed_entries(direct: bool) -> Vec<(&'static str, *const u8, *const u8)> {
    crate::test_utils::init_test_tracing();
    force_profit_gate_for_test(false);
    // The calls must stay calls: the fuser would inline these callees.
    crate::emacs_core::jit::inline::force_inline_for_test(Some(false));
    force_direct_call_for_test(Some(direct));
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(ARMING).expect("warmed");
    let caller = cached_leaf(&ev, "neovm--dc-caller").expect("the caller is compiled");
    let out = [
        "neovm--dc-exact",
        "neovm--dc-framed",
        "neovm--dc-opt",
        "neovm--dc-rest",
        "neovm--dc-wide",
    ]
    .into_iter()
    .map(|name| {
        let callee = cached_leaf(&ev, name).unwrap_or_else(|| panic!("{name} is compiled"));
        let slot = slot_calling(caller, callee)
            .unwrap_or_else(|| panic!("the caller's slot caches {name}"));
        (name, callee.entry, slot.direct_entry())
    })
    .collect();
    force_direct_call_for_test(None);
    out
}

/// T12: only an exact-arity call of a frameless register-ABI leaf arms a
/// direct entry, and only with the knob on; the entry is the leaf's own.
#[test]
fn spec_slots_arm_a_direct_entry_only_for_exact_frameless_register_callees() {
    for (name, entry, direct) in armed_entries(true) {
        if name == "neovm--dc-exact" {
            assert_eq!(direct, entry, "{name}: armed with the leaf's entry");
        } else {
            assert!(direct.is_null(), "{name}: not directly callable");
        }
    }
    for (name, _, direct) in armed_entries(false) {
        assert!(direct.is_null(), "{name}: knob off arms nothing");
    }
}

/// T12: every clear drops the direct entry with the leaf -- a re-validation
/// (`clear_leaf`) and a retired callee (`unlink_spec_slots`) alike -- and
/// the next call through the site arms it again.
#[test]
fn clearing_a_slot_drops_its_direct_entry_and_the_next_call_rearms_it() {
    crate::test_utils::init_test_tracing();
    force_profit_gate_for_test(false);
    crate::emacs_core::jit::inline::force_inline_for_test(Some(false));
    force_direct_call_for_test(Some(true));
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(ARMING).expect("warmed");
    let caller = cached_leaf(&ev, "neovm--dc-caller").expect("caller");
    let callee = cached_leaf(&ev, "neovm--dc-exact").expect("callee");
    let slot = slot_calling(caller, callee).expect("slot");
    assert_eq!(slot.direct_entry(), callee.entry);
    slot.clear_leaf();
    assert!(slot.direct_entry().is_null() && slot.leaf_ptr().is_null());
    ev.eval_str("(neovm--dc-caller 5)").expect("runs");
    assert_eq!(
        slot.direct_entry(),
        callee.entry,
        "re-armed by the next call"
    );
    assert_eq!(
        crate::emacs_core::jit::cache::unlink_spec_slots(callee),
        1,
        "the caller's slot unlinks"
    );
    assert!(slot.direct_entry().is_null(), "unlinked with its leaf");
    ev.eval_str("(neovm--dc-caller 6)").expect("runs");
    assert_eq!(slot.direct_entry(), callee.entry);
    force_direct_call_for_test(None);
}
