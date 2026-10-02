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
  (dotimes (i 1500) (neovm--dc-caller i)))"#;

/// Warm the arming program with direct calls ON or OFF and answer each
/// callee's slot in the caller: `(name, leaf entry, the slot's direct
/// entry)`.
fn armed_entries(direct: bool) -> Vec<(&'static str, *const u8, *const u8)> {
    crate::test_utils::init_test_tracing();
    force_profit_gate_for_test(false);
    // The calls must stay calls: the fuser would inline these callees.
    crate::emacs_core::jit::inline::force_inline_for_test(Some(false));
    crate::emacs_core::jit::force_profit_defer_for_test(Some(1));
    force_direct_call_for_test(Some(direct));
    // Every caller here is a small straight-line body: pin direct sites on
    // for every body, not only the unbounded ones (`DirectSitesMode`).
    force_direct_sites_for_test(Some(DirectSitesMode::All));
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
    force_direct_sites_for_test(None);
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

/// Direct sites go where they pay their compile back
/// (`DirectSitesMode::Unbounded`, the default): a caller whose body loops or
/// calls itself, or a re-tier of one that proved hot; a straight-line caller
/// keeps the shim call. `NEOVM_JIT_DIRECT_SITES=all` emits them in every
/// body.
#[test]
fn only_callers_that_loop_recurse_or_proved_hot_emit_direct_sites() {
    use crate::emacs_core::jit::compile::lowering::RegallocPolicy;
    crate::test_utils::init_test_tracing();
    force_profit_gate_for_test(false);
    crate::emacs_core::jit::inline::force_inline_for_test(Some(false));
    force_direct_call_for_test(Some(true));
    force_direct_sites_for_test(Some(DirectSitesMode::Unbounded));
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(
        r#"(progn
  (defun neovm--dcu-callee (a b) (if (> a b) (- a b) (+ a b)))
  (defun neovm--dcu-straight (x) (neovm--dcu-callee x 1))
  (defun neovm--dcu-loop (n)
    (let ((s 0))
      (while (> n 0) (setq s (neovm--dcu-callee s n) n (1- n)))
      s))
  (defun neovm--dcu-rec (n) (if (= n 0) 0 (neovm--dcu-callee (neovm--dcu-rec (1- n)) 1)))
  (dolist (f '(neovm--dcu-callee neovm--dcu-straight neovm--dcu-loop neovm--dcu-rec))
    (byte-compile f)))"#,
    )
    .expect("defined");
    let sites = |ev: &Context, name: &str, policy: RegallocPolicy| {
        let f = ev
            .obarray
            .symbol_function_id(intern(name))
            .expect("defined");
        let bc = f.get_bytecode_data().expect("byte-compiled");
        let before = direct_call::direct_sites_emitted_for_test();
        compile_bytecode_function_tiered(bc, Some(&ev.obarray), policy).expect("compiles");
        direct_call::direct_sites_emitted_for_test() - before
    };
    assert_eq!(
        sites(&ev, "neovm--dcu-straight", RegallocPolicy::Auto),
        0,
        "a straight-line caller keeps the shim call"
    );
    assert_eq!(
        sites(&ev, "neovm--dcu-straight", RegallocPolicy::Full),
        1,
        "a re-tier of a caller that proved hot calls directly"
    );
    assert_eq!(
        sites(&ev, "neovm--dcu-loop", RegallocPolicy::Auto),
        1,
        "a caller that loops calls directly"
    );
    assert_eq!(
        sites(&ev, "neovm--dcu-rec", RegallocPolicy::Auto),
        2,
        "a caller that recurses calls itself and its callee directly"
    );
    force_direct_sites_for_test(Some(DirectSitesMode::All));
    assert_eq!(
        sites(&ev, "neovm--dcu-straight", RegallocPolicy::Auto),
        1,
        "`all`: every caller"
    );
    force_direct_sites_for_test(None);
    force_direct_call_for_test(None);
}

/// T12: every clear drops the direct entry with the leaf -- a re-validation
/// (`clear_leaf`) and a retired callee (`unlink_spec_slots`) alike -- and
/// the next call through the site arms it again.
#[test]
fn clearing_a_slot_drops_its_direct_entry_and_the_next_call_rearms_it() {
    crate::test_utils::init_test_tracing();
    force_profit_gate_for_test(false);
    crate::emacs_core::jit::inline::force_inline_for_test(Some(false));
    crate::emacs_core::jit::force_profit_defer_for_test(Some(1));
    force_direct_call_for_test(Some(true));
    // Every caller here is a small straight-line body: pin direct sites on
    // for every body, not only the unbounded ones (`DirectSitesMode`).
    force_direct_sites_for_test(Some(DirectSitesMode::All));
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
    force_direct_sites_for_test(None);
}

// ---------------------------------------------------------------------------
// T2-T10, T15: every observable equals the shim's, under both knob values
// and the force harness.
// ---------------------------------------------------------------------------

/// How a scenario runs its compiled calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// `NEOVM_JIT_DIRECT_CALL` off: today's spec shim at every site.
    Shim,
    /// Direct calls on.
    Direct,
    /// Direct calls on under `NEOVM_JIT_FORCE_SLOW_SPEC`: no direct site is
    /// emitted and every call re-validates in the shim.
    DirectForcedSlow,
}

/// The warm-up count of the scenario programs: past the hot threshold with
/// room to spare, with the profitability deferral pinned to 1.
const WARM: &str = "(defvar neovm--dc-warm 1500)";

/// One scenario's run: the printed observation, and the engagement facts.
#[derive(Debug)]
struct Run {
    out: String,
    /// Direct sites compiled on the run's thread.
    direct_sites: usize,
    /// Calls that entered `neovm_jit_call_spec` during the observation.
    shim_calls: u64,
    /// Direct calls that left their hit path through the cold finish
    /// during the observation.
    cold_exits: u64,
}

/// Run PROGRAM (defines, byte-compiles and warms) then OBSERVE (one form,
/// printed; an escaping error is printed too) in a fresh runtime on a
/// thread of its own, so the knob overrides and the compiled-leaf cache are
/// the run's own. Asserts the observation leaves `depth` and the specpdl as
/// it found them.
fn run_in(mode: Mode, program: &'static str, observe: &'static str) -> Run {
    run_in_with(mode, DirectShapesKnob::OFF, program, observe)
}

/// [`run_in`] with the call shapes direct sites take
/// (`NEOVM_JIT_DIRECT_SHAPES`) in the modes that call directly.
fn run_in_with(
    mode: Mode,
    shapes: DirectShapesKnob,
    program: &'static str,
    observe: &'static str,
) -> Run {
    std::thread::Builder::new()
        .name(format!("direct-call-{mode:?}"))
        .stack_size(128 * 1024 * 1024)
        .spawn(move || {
            crate::test_utils::init_test_tracing();
            force_profit_gate_for_test(false);
            crate::emacs_core::jit::inline::force_inline_for_test(Some(false));
            force_direct_call_for_test(Some(mode != Mode::Shim));
            // Every caller here is a small straight-line body: pin direct sites on
            // for every body, not only the unbounded ones (`DirectSitesMode`).
            force_direct_sites_for_test(Some(DirectSitesMode::All));
            force_slow_spec_for_test(Some(mode == Mode::DirectForcedSlow));
            force_direct_shapes_for_test(Some(shapes));
            // Tier up at the hot threshold, callers included, so the
            // warm-ups stay short (they run under GC stress too).
            crate::emacs_core::jit::force_profit_defer_for_test(Some(1));
            let mut ev = crate::test_utils::runtime_startup_context();
            ev.refresh_attention_for_test();
            ev.eval_str(WARM).expect("warm-up count");
            ev.eval_str(program).expect("the scenario's program runs");
            let depth0 = ev.depth;
            let spec0 = ev.specpdl.len();
            let calls0 = SPEC_CALL_COUNT.load(Ordering::Relaxed);
            let cold0 = super::direct_call::DIRECT_COLD_EXITS.load(Ordering::Relaxed);
            let out = match ev.eval_str(observe) {
                Ok(v) => crate::emacs_core::print::print_value(&v),
                Err(e) => format!("escaped {e:?}"),
            };
            let shim_calls = SPEC_CALL_COUNT.load(Ordering::Relaxed) - calls0;
            let cold_exits = super::direct_call::DIRECT_COLD_EXITS.load(Ordering::Relaxed) - cold0;
            assert_eq!(ev.depth, depth0, "{mode:?}: depth restored");
            assert_eq!(ev.specpdl.len(), spec0, "{mode:?}: specpdl restored");
            let direct_sites = super::direct_call::direct_sites_emitted_for_test();
            force_slow_spec_for_test(None);
            force_direct_shapes_for_test(None);
            force_direct_call_for_test(None);
            force_direct_sites_for_test(None);
            crate::emacs_core::jit::inline::force_inline_for_test(None);
            crate::emacs_core::jit::force_profit_defer_for_test(None);
            Run {
                out,
                direct_sites,
                shim_calls,
                cold_exits,
            }
        })
        .expect("spawn")
        .join()
        .expect("the run does not crash")
}

/// Run a scenario in every [`Mode`]: the observations must be equal, the
/// direct run must have emitted direct sites and the forced run none.
/// Returns the three runs (shim, direct, forced).
fn differential(program: &'static str, observe: &'static str) -> [Run; 3] {
    let shim = run_in(Mode::Shim, program, observe);
    let direct = run_in(Mode::Direct, program, observe);
    let forced = run_in(Mode::DirectForcedSlow, program, observe);
    assert_eq!(
        direct.out, shim.out,
        "direct calls change nothing observable"
    );
    assert_eq!(forced.out, shim.out, "nor does the force harness");
    assert_eq!(shim.direct_sites, 0, "the knob off emits no direct site");
    assert!(
        direct.direct_sites > 0,
        "the direct run emitted direct sites"
    );
    assert_eq!(forced.direct_sites, 0, "the force harness emits none");
    [shim, direct, forced]
}

/// T2 (and T15): frames of every lean shape record the called symbol and
/// the caller's arguments, as GNU's `Bcall` does (the expectations are the
/// spec path's, `spec_frames.rs`), and a direct hit does not enter the shim.
#[test]
fn direct_calls_record_the_frames_the_shim_records() {
    const PROGRAM: &str = r#"(progn
  (defvar neovm--dcf-seen nil)
  (defun neovm--dcf-show ()
    (let (fs)
      (mapbacktrace
       (lambda (_evald f args _flags)
         (unless (eq f 'mapbacktrace)
           (push (list (if (symbolp f) f (type-of f)) args) fs))))
      (setq neovm--dcf-seen (nreverse fs))
      (list (backtrace-frame 0 'neovm--dcf-show) (backtrace-frame 1 'neovm--dcf-show))))
  (defun neovm--dcf-inner (x) (if (eq x 'show) (neovm--dcf-show) (+ x 1)))
  (defun neovm--dcf-outer (x y) (if y (neovm--dcf-inner x) 0))
  (defun neovm--dcf-mid3 (a b c) (list (neovm--dcf-inner a) b c))
  (defun neovm--dcf-top (x) (neovm--dcf-mid3 x 1 2))
  (defun neovm--dcf-four (a b c d) (if d (neovm--dcf-outer a b) c))
  (defun neovm--dcf-zero () (neovm--dcf-four 'show 7 0 t))
  (dolist (f '(neovm--dcf-show neovm--dcf-inner neovm--dcf-outer neovm--dcf-mid3
               neovm--dcf-top neovm--dcf-four neovm--dcf-zero))
    (byte-compile f))
  (dotimes (i neovm--dc-warm) (neovm--dcf-outer i 1) (neovm--dcf-top i) (neovm--dcf-four i 1 2 t)))"#;
    let [shim, direct, _] = differential(
        PROGRAM,
        r#"(list (neovm--dcf-outer 'show 7) (seq-take neovm--dcf-seen 3)
              (neovm--dcf-top 'show) (seq-take neovm--dcf-seen 3)
              (neovm--dcf-zero) (seq-take neovm--dcf-seen 5))"#,
    );
    assert!(
        shim.out.contains(
            "((neovm--dcf-show nil) (neovm--dcf-inner (show)) (neovm--dcf-outer (show 7)))"
        ),
        "{}",
        shim.out
    );
    assert!(
        shim.out.contains(
            "((neovm--dcf-show nil) (neovm--dcf-inner (show)) (neovm--dcf-mid3 (show 1 2)))"
        ),
        "{}",
        shim.out
    );
    assert!(
        shim.out
            .contains("(neovm--dcf-four (show 7 0 t)) (neovm--dcf-zero nil)"),
        "{}",
        shim.out
    );
    assert!(
        direct.shim_calls < shim.shim_calls,
        "direct hits bypass the shim: {} vs {}",
        direct.shim_calls,
        shim.shim_calls
    );
}

/// T15: a deep direct recursion runs without the shim: the shim sees only
/// the outermost call per recursion.
#[test]
fn a_direct_recursion_never_enters_the_shim() {
    const PROGRAM: &str = r#"(progn
  (defun neovm--dcr-ll (l n) (if (null l) n (neovm--dcr-ll (cdr l) (1+ n))))
  (byte-compile 'neovm--dcr-ll)
  (defvar neovm--dcr-list (make-list 2000 1))
  (let ((max-lisp-eval-depth 10000))
    (dotimes (_ 40) (neovm--dcr-ll neovm--dcr-list 0))))"#;
    const OBSERVE: &str = r#"(let ((max-lisp-eval-depth 10000))
  (list (neovm--dcr-ll neovm--dcr-list 0) (neovm--dcr-ll neovm--dcr-list 5)))"#;
    let [shim, direct, forced] = differential(PROGRAM, OBSERVE);
    assert_eq!(shim.out, "(2000 2005)");
    assert!(shim.shim_calls >= 4000, "{}", shim.shim_calls);
    assert!(
        direct.shim_calls < 10,
        "only the entries from Tier-0 reach the shim: {}",
        direct.shim_calls
    );
    assert!(forced.shim_calls >= 4000, "{}", forced.shim_calls);
}

/// T3: a frame flagged by `backtrace-debug` from inside the callee sends
/// its return through the exit debugger, whose value replaces the
/// callee's.
#[test]
fn a_flagged_direct_frame_returns_through_the_exit_debugger() {
    const PROGRAM: &str = r#"(progn
  (defvar neovm--dcd-log nil)
  (defvar neovm--dcd-arm nil)
  (defun neovm--dcd-flag (x)
    (when neovm--dcd-arm (backtrace-debug 0 t 'neovm--dcd-flag))
    (* x 2))
  (defun neovm--dcd-two (x y) (when neovm--dcd-arm (backtrace-debug 0 t 'neovm--dcd-two)) (+ x y))
  (defun neovm--dcd-three (x y z)
    (when neovm--dcd-arm (backtrace-debug 0 t 'neovm--dcd-three)) (+ x y z))
  (defun neovm--dcd-caller (x)
    (list (neovm--dcd-flag x) (neovm--dcd-two x 1) (neovm--dcd-three x 1 2)))
  (dolist (f '(neovm--dcd-flag neovm--dcd-two neovm--dcd-three neovm--dcd-caller))
    (byte-compile f))
  (dotimes (i neovm--dc-warm) (neovm--dcd-caller i)))"#;
    let [_, direct, _] = differential(
        PROGRAM,
        r#"(let ((debugger (lambda (&rest args)
                           (push args neovm--dcd-log)
                           (if (eq (car args) 'exit) (* 10 (cadr args)) nil)))
              (neovm--dcd-arm t))
          (list (neovm--dcd-caller 3) (reverse neovm--dcd-log)))"#,
    );
    assert!(
        direct.cold_exits >= 3,
        "each flagged frame left through the finish: {direct:?}"
    );
}

/// T4: `debug-on-next-call` set inside a compiled caller sends the next
/// call into the entry debugger with its frame flagged, then out through
/// the exit debugger.
#[test]
fn debug_on_next_call_takes_a_direct_site_through_the_debugger() {
    const PROGRAM: &str = r#"(progn
  (defvar neovm--dcn-log nil)
  (defvar neovm--dcn-arm nil)
  (defun neovm--dcn-target (x) (if (> x 0) (* x 3) 0))
  (defun neovm--dcn-caller (x)
    (when neovm--dcn-arm (setq debug-on-next-call t))
    (neovm--dcn-target x))
  (byte-compile 'neovm--dcn-target)
  (byte-compile 'neovm--dcn-caller)
  (dotimes (i neovm--dc-warm) (neovm--dcn-caller i)))"#;
    differential(
        PROGRAM,
        r#"(let ((debugger (lambda (&rest args)
                           (push (car args) neovm--dcn-log)
                           (if (eq (car args) 'exit) (cadr args) nil)))
              (neovm--dcn-arm t))
          (list (neovm--dcn-caller 5) (reverse neovm--dcn-log) debug-on-next-call))"#,
    );
}

/// T5: `max-lisp-eval-depth` bound by `let` stops a direct recursion at the
/// same call and with the same error as the shim.
#[test]
fn the_depth_limit_stops_a_direct_recursion_at_the_same_call() {
    const PROGRAM: &str = r#"(progn
  (defvar neovm--dcl-count 0)
  (defun neovm--dcl-rec (n)
    (setq neovm--dcl-count (1+ neovm--dcl-count))
    (if (= n 0) 0 (1+ (neovm--dcl-rec (1- n)))))
  (byte-compile 'neovm--dcl-rec)
  (dotimes (_ 60) (neovm--dcl-rec 100)))"#;
    differential(
        PROGRAM,
        r#"(list (let ((max-lisp-eval-depth 300))
                (setq neovm--dcl-count 0)
                (list (condition-case err (neovm--dcl-rec 1000) (error err)) neovm--dcl-count))
              (let ((max-lisp-eval-depth 300))
                (setq neovm--dcl-count 0)
                (list (neovm--dcl-rec 150) neovm--dcl-count)))"#,
    );
}

/// T6: `quit-flag` raised deep in a direct recursion signals `quit` at the
/// next call, and a quit request raised by another thread is seen too.
#[test]
fn a_quit_raised_during_a_direct_recursion_is_signalled() {
    const PROGRAM: &str = r#"(progn
  (defun neovm--dcq-rec (n k)
    (when (= n k) (setq quit-flag t))
    (if (= n 0) 0 (1+ (neovm--dcq-rec (1- n) k))))
  (byte-compile 'neovm--dcq-rec)
  (dotimes (_ 60) (neovm--dcq-rec 100 -1)))"#;
    differential(
        PROGRAM,
        r#"(list (condition-case err (neovm--dcq-rec 1000 500) (quit (list 'quit err)))
              quit-flag
              (neovm--dcq-rec 200 -1))"#,
    );
}

/// T6 (asynchronous): a `QuitRequest` another thread raises while a direct
/// recursion runs reaches the recursion through the asynchronous word.
#[test]
fn a_cross_thread_quit_request_stops_a_direct_recursion() {
    for mode in [Mode::Shim, Mode::Direct] {
        let out = std::thread::Builder::new()
            .stack_size(128 * 1024 * 1024)
            .spawn(move || {
                crate::test_utils::init_test_tracing();
                force_profit_gate_for_test(false);
                crate::emacs_core::jit::inline::force_inline_for_test(Some(false));
                force_direct_call_for_test(Some(mode == Mode::Direct));
                // Every caller here is a small straight-line body: pin direct sites on
                // for every body, not only the unbounded ones (`DirectSitesMode`).
                force_direct_sites_for_test(Some(DirectSitesMode::All));
                let mut ev = crate::test_utils::runtime_startup_context();
                ev.eval_str(
                    r#"(progn
  (defun neovm--dcqa-rec (n) (if (= n 0) 0 (1+ (neovm--dcqa-rec (1- n)))))
  (byte-compile 'neovm--dcqa-rec)
  (dotimes (_ 60) (neovm--dcqa-rec 100)))"#,
                )
                .expect("warm");
                let request = ev.quit_requested.clone();
                let raiser = std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    request.request();
                });
                let depth0 = ev.depth;
                let out = ev
                    .eval_str(
                        r#"(let ((max-lisp-eval-depth 5000))
  (condition-case nil
      (progn (dotimes (_ 10000000) (neovm--dcqa-rec 3000)) 'never-quit)
    (quit 'quit)))"#,
                    )
                    .map(|v| crate::emacs_core::print::print_value(&v))
                    .map_err(|e| format!("{e:?}"));
                raiser.join().expect("raiser");
                assert_eq!(ev.depth, depth0);
                force_direct_call_for_test(None);
                force_direct_sites_for_test(None);
                out
            })
            .expect("spawn")
            .join()
            .expect("no crash");
        assert_eq!(out.expect("evaluates"), "quit", "{mode:?}");
    }
}

/// T7: a direct callee that redefines its own symbol, collects, and then
/// deopts keeps running its old definition (the frame records the symbol,
/// which pins the old object), and the next call runs the new one.
#[test]
fn a_redefinition_under_a_direct_activation_keeps_the_old_code_running() {
    const PROGRAM: &str = r#"(progn
  (defvar neovm--dcx-armed nil)
  (defvar neovm--dcx-tab (make-hash-table :test 'eq :weakness 'key))
  (fset 'neovm--dcx-f
        (byte-compile
         (lambda (n x)
           (if (= n 0)
               (if neovm--dcx-armed
                   (progn
                     (fset 'neovm--dcx-f (lambda (_n _x) 'new))
                     (garbage-collect)
                     ;; A fixnum guard the warm-up never failed: a deopt
                     ;; into the old object, pinned by the frame.
                     (+ (car x) 0.5))
                 0)
             (1+ (neovm--dcx-f (1- n) x))))))
  (fset 'neovm--dcx-g (byte-compile (lambda (n x) (neovm--dcx-f n x))))
  (dotimes (_ neovm--dc-warm) (neovm--dcx-g 3 '(1)))
  (puthash (symbol-function 'neovm--dcx-f) t neovm--dcx-tab))"#;
    let [_, direct, _] = differential(
        PROGRAM,
        r#"(list (let ((neovm--dcx-armed t)) (neovm--dcx-g 3 '(40 50)))
              (neovm--dcx-g 3 '(1))
              (hash-table-count neovm--dcx-tab))"#,
    );
    assert!(
        direct.cold_exits >= 1,
        "the deopt left through the finish: {direct:?}"
    );
}

/// T8: a precise deopt and a guard failure inside a direct callee finish in
/// Tier-0 with the frame still pushed, and the caller continues natively.
#[test]
fn deopts_in_a_direct_callee_resume_and_return_to_the_caller() {
    const PROGRAM: &str = r#"(progn
  (defun neovm--dco-add (x y) (if (> x 0) (+ x y) (- y x)))
  (defun neovm--dco-car (x y) (if y (car x) x))
  (defun neovm--dco-caller (x y z)
    (list (neovm--dco-add x y) (neovm--dco-car z t) (neovm--dco-add y x)))
  (dolist (f '(neovm--dco-add neovm--dco-car neovm--dco-caller)) (byte-compile f))
  (dotimes (i neovm--dc-warm) (neovm--dco-caller i 2 '(9))))"#;
    let [_, direct, _] = differential(
        PROGRAM,
        r#"(list (neovm--dco-caller 1.5 2 '(7))
              (neovm--dco-caller 3 most-positive-fixnum '(8))
              (condition-case err (neovm--dco-caller 1 2 5) (error err))
              (neovm--dco-caller 4 5 '(6)))"#,
    );
    assert!(
        direct.cold_exits >= 3,
        "the deopts left through the finish: {direct:?}"
    );
}

/// T9: a signal raised 1000 direct levels down reaches the handler of an
/// outer compiled caller, and `handler-bind` and `signal-hook-function` see
/// every frame at signal time.
#[test]
fn a_signal_through_a_direct_recursion_sees_every_frame() {
    const PROGRAM: &str = r#"(progn
  (define-error 'neovm--dcs-err "direct-call test error")
  (defvar neovm--dcs-frames nil)
  (defvar neovm--dcs-hook-frames nil)
  (defun neovm--dcs-rec (n)
    (if (= n 0) (signal 'neovm--dcs-err (list 'bottom)) (1+ (neovm--dcs-rec (1- n)))))
  (defun neovm--dcs-catch (n)
    (condition-case err (neovm--dcs-rec n) (neovm--dcs-err (list 'caught err))))
  (byte-compile 'neovm--dcs-rec)
  (byte-compile 'neovm--dcs-catch)
  (dotimes (_ 60) (neovm--dcs-catch 100)))"#;
    let [_, direct, _] = differential(
        PROGRAM,
        r#"(let ((max-lisp-eval-depth 5000)
              (signal-hook-function
               (lambda (_sym _data)
                 (setq neovm--dcs-hook-frames (length (backtrace-frames))))))
          (list (handler-bind ((neovm--dcs-err
                                (lambda (_e)
                                  (setq neovm--dcs-frames
                                        (seq-count (lambda (f) (eq (cadr f) 'neovm--dcs-rec))
                                                   (backtrace-frames))))))
                  (neovm--dcs-catch 1000))
                neovm--dcs-frames
                (> neovm--dcs-hook-frames 1000)))"#,
    );
    assert!(
        direct.cold_exits >= 1000,
        "the signal unwound every direct level through the finish: {direct:?}"
    );
}

/// T10: a Rust panic contained in a direct callee's shim (here a subr it
/// calls) is healed by the framed ancestor, for inline (one and two
/// arguments) and pointer (three) frames, with a collection between the
/// callee's exit and the heal.
#[test]
fn a_panic_contained_in_a_direct_callee_is_healed() {
    const PROGRAM: &str = r#"(progn
  (defvar neovm--dcp-arm nil)
  (defun neovm--dcp-one (x) (if neovm--dcp-arm (neovm--internal-panic "direct-boom") (1+ x)))
  (defun neovm--dcp-three (a b c)
    (if neovm--dcp-arm (neovm--internal-panic "direct-boom3") (+ a b c)))
  (defun neovm--dcp-caller (x)
    (list (neovm--dcp-one x) (neovm--dcp-three x 1 2)))
  (dolist (f '(neovm--dcp-one neovm--dcp-three neovm--dcp-caller)) (byte-compile f))
  (dotimes (i neovm--dc-warm) (neovm--dcp-caller i)))"#;
    let [_, direct, _] = differential(
        PROGRAM,
        r#"(list (let ((neovm--dcp-arm t))
                (condition-case err (neovm--dcp-caller 1) (error (car err))))
              (progn (garbage-collect) (neovm--dcp-caller 2)))"#,
    );
    assert!(
        direct.cold_exits >= 1,
        "the panic left through the finish: {direct:?}"
    );
}

/// T11 with direct calls: a direct recursion with no depth limit ends in
/// GNU's "Bytecode stack overflow" (the callee's register-ABI entry guard),
/// not a crash, and the evaluator is whole afterwards.
#[test]
fn a_deep_direct_recursion_signals_bytecode_stack_overflow() {
    const PROGRAM: &str = r#"(progn
  (defun neovm--dcso-deep (n) (if (= n 0) 0 (1+ (neovm--dcso-deep (1- n)))))
  (byte-compile 'neovm--dcso-deep)
  (dotimes (_ 60) (neovm--dcso-deep 100)))"#;
    let [_, direct, _] = differential(
        PROGRAM,
        r#"(list (let ((max-lisp-eval-depth most-positive-fixnum))
                (condition-case err (neovm--dcso-deep 100000000) (error err)))
              (neovm--dcso-deep 500))"#,
    );
    assert_eq!(direct.out, "((error \"Bytecode stack overflow\") 500)");
    assert!(
        direct.shim_calls < 1000,
        "the recursion ran direct: {direct:?}"
    );
}

// P1.1 Stage 2 (`NEOVM_JIT_DIRECT_SHAPES`): the call shapes beyond the exact
// named call, on this file's harness.
#[path = "direct_call_shapes.rs"]
mod shapes;
