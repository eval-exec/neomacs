//! Callback heat credit (`hof_credit`) under the background worker. A
//! `mapc`/`mapcar` caller with a selectable HOF site gains heat from the
//! callbacks it drives. Under `BgMode::Threaded` that heat sends the
//! caller's compile to a worker, which reads the callback's bytecode and
//! numeric feedback while this thread keeps running mappings: writing that
//! feedback, crediting heat, and probing the still-pending caller. Every
//! answer must match the interpreted run, the specpdl and depth must balance
//! around it, and the leaf installed once the worker is done must inline the
//! warm callback loop.

use std::time::Duration;

use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::jit::compile::{self, Inline2Mode, OptMode};
use crate::emacs_core::print::print_value;

/// The production front with the worker forced on: Opt backend, INLINE2
/// off, profit gate on, HOF inlining on. `stress` delays every worker job
/// and install by a random amount. Threading: scalar overrides on this test
/// thread; the worker reads only the jobs it is handed.
struct Production(Box<dyn std::any::Any>);

impl Production {
    fn enter(stress: bool) -> Self {
        compile::force_inline2_for_test(Some(Inline2Mode::Off));
        compile::force_profit_gate_for_test(true);
        compile::force_hof_inline_for_test(Some(true));
        compile::force_deopt_for_test(false);
        crate::emacs_core::jit::inline::force_inline_for_test(Some(true));
        force_mode_for_test(Some(BgMode::Threaded));
        force_stress_for_test(Some(stress));
        Self(Box::new(compile::opt_mode_scope_for_test(OptMode::Opt)))
    }
}

impl Drop for Production {
    fn drop(&mut self) {
        force_stress_for_test(None);
        force_mode_for_test(None);
        compile::force_inline2_for_test(None);
        compile::force_hof_inline_for_test(None);
        crate::emacs_core::jit::inline::force_inline_for_test(None);
    }
}

/// A mapping caller of each kind, an outer driver whose own loop may tier
/// up through OSR while the callers go pending, and a log of the
/// iterations at which the answer changed.
const DEFINITIONS: &str = r#"(progn
  (defun hbg-incs (l) (mapcar (lambda (x) (1+ x)) l))
  (defun hbg-walk (l) (length (mapc (lambda (x) (1+ x)) l)))
  (defun hbg-step (l)
    (let ((r (hbg-incs l)))
      (list (car r) (car (last r)) (length r) (hbg-walk r))))
  (defun hbg-drive (n len)
    (let ((l (number-sequence 1 len)) (log nil) (last nil) (i 0))
      (while (< i n)
        (let ((r (hbg-step l)))
          (unless (equal r last) (push (cons i r) log) (setq last r)))
        (setq i (1+ i)))
      (nreverse log))))"#;

const FUNCTIONS: [&str; 4] = ["hbg-incs", "hbg-walk", "hbg-step", "hbg-drive"];

/// Callbacks per mapping: 3200 / 32 = 100 heat of credit per call, so the
/// caller crosses the entry threshold within the first dozen iterations,
/// long before the run ends.
const LEN: usize = 3200;
const ITERATIONS: usize = 60;

fn leaf_inlines(ctx: &mut Context, name: &str, hof: &str) -> bool {
    let function = ctx
        .obarray
        .symbol_function_id(intern(name))
        .expect("defined");
    let data = function.get_bytecode_data().expect("byte-compiled");
    let Some(id) = data.jit_runtime().compiled_id() else {
        return false;
    };
    crate::emacs_core::jit::cache::compiled_leaf_ptr_for_test(id).is_some_and(|leaf| {
        // SAFETY: the cache owns the leaf until eviction; nothing evicts
        // while this thread only reads it.
        unsafe { &*leaf }.inline_deps().contains(&intern(hof))
    })
}

fn run(stress: bool) {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::test_utils::runtime_startup_context();
    ctx.eval_str(DEFINITIONS).expect("defined");
    let drive = format!("(hbg-drive {ITERATIONS} {LEN})");
    let reference = print_value(&ctx.eval_str(&drive).expect("interpreted run"));

    let _production = Production::enter(stress);
    ctx.eval_str(DEFINITIONS).expect("redefined");
    for name in FUNCTIONS {
        ctx.eval_str(&format!("(byte-compile '{name})"))
            .expect("byte-compiled");
    }
    let before = stats_snapshot();
    let specpdl = ctx.specpdl.len();
    let depth = ctx.depth;
    let compiled = print_value(&ctx.eval_str(&drive).expect("compiled run"));
    assert_eq!(ctx.specpdl.len(), specpdl, "specpdl balanced");
    assert_eq!(ctx.depth, depth, "depth balanced");
    assert_eq!(
        compiled, reference,
        "the same answers at the same iterations"
    );
    assert!(quiesce_for_test(Duration::from_secs(60)), "worker drained");
    let after = stats_snapshot();
    assert!(
        after.enqueued[JobClass::Entry as usize] > before.enqueued[JobClass::Entry as usize],
        "credited callers compiled on the worker: {after:?}"
    );

    // A finished job installs at its function's next probe, but the
    // background stress soak (`NEOVM_JIT_BG_STRESS`, on for `stress` runs)
    // deliberately makes a finished job read as running for 1..=4 further
    // probes (`bg::stress_probe_skips`'s STRESS_PROBE_SKIP_MAX, a
    // NonZeroU8), and the worker can finish after the drive's last call:
    // "install late under stress" is exactly that case. One warm run is then
    // a single probe per caller, not a guaranteed install, so give each
    // caller up to the soak's skip bound + the install probe (with one probe
    // of margin) through cheap no-op mappings before the warm run exercises
    // the installed leaves.
    const INSTALL_PROBES: usize = STRESS_PROBE_SKIP_MAX.get() as usize + 2;
    for _ in 0..INSTALL_PROBES {
        if leaf_inlines(&mut ctx, "hbg-incs", "mapcar")
            && leaf_inlines(&mut ctx, "hbg-walk", "mapc")
        {
            break;
        }
        ctx.eval_str("(progn (hbg-incs nil) (hbg-walk nil))")
            .expect("install probe");
    }
    let again = print_value(
        &ctx.eval_str(&format!("(hbg-step (number-sequence 1 {LEN}))"))
            .expect("warm run"),
    );
    assert_eq!(again, format!("(2 {} {LEN} {LEN})", LEN + 1));
    assert_eq!(ctx.specpdl.len(), specpdl, "specpdl balanced");
    assert!(
        leaf_inlines(&mut ctx, "hbg-incs", "mapcar"),
        "the installed mapcar caller inlines its warm callback"
    );
    assert!(
        leaf_inlines(&mut ctx, "hbg-walk", "mapc"),
        "the installed mapc caller inlines its warm callback"
    );
}

#[test]
fn jit_bg_credited_mapping_callers_compile_on_the_worker() {
    run(false);
}

#[test]
fn jit_bg_credited_mapping_callers_install_late_under_stress() {
    run(true);
}

/// A mid-callback error after the worker installed the caller: the inlined
/// loop deopts, the signal unwinds through the resumed callback to the
/// caller's caller, and the next call runs native again.
#[test]
fn jit_bg_installed_mapping_caller_unwinds_a_mid_callback_error() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::test_utils::runtime_startup_context();
    let _production = Production::enter(false);
    ctx.eval_str(
        r#"(progn
  (defun hbg-cars (l) (mapcar (lambda (x) (car x)) l))
  (defun hbg-outer (l) (condition-case err (hbg-cars l) (wrong-type-argument (list 'caught err))))
  (byte-compile 'hbg-cars)
  (byte-compile 'hbg-outer))"#,
    )
    .expect("defined");
    let good = format!("(hbg-cars (mapcar #'list (number-sequence 1 {LEN})))");
    for _ in 0..20 {
        ctx.eval_str(&good).expect("warm mapping");
    }
    assert!(quiesce_for_test(Duration::from_secs(60)), "worker drained");
    ctx.eval_str(&good).expect("install probe");
    assert!(
        leaf_inlines(&mut ctx, "hbg-cars", "mapcar"),
        "the credited caller installed with its loop inlined"
    );
    let specpdl = ctx.specpdl.len();
    let caught = ctx
        .eval_str("(hbg-outer '((1) (2) 3 (4)))")
        .expect("caught");
    assert_eq!(
        print_value(&caught),
        "(caught (wrong-type-argument listp 3))"
    );
    assert_eq!(ctx.specpdl.len(), specpdl, "specpdl balanced");
    let after = ctx.eval_str("(hbg-cars '((5) (6)))").expect("native again");
    assert_eq!(print_value(&after), "(5 6)");
}
