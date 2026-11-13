//! Phase 0 step 2, round 3c: admission is invisible to the owner it admits.
//!
//! - A randomized history run by an owner admitted from the start (eager)
//!   and by one admitted part-way (late) must look the same at every step,
//!   and so must the canonical storage both leave behind (C4's
//!   `facade_bindings_test.rs:775`).
//! - Admission from inside running Lisp -- a live interpreter or VM frame,
//!   a variable watcher, an `unwind-protect` cleanup, a `backtrace-eval` --
//!   gives the same results as a Sim World where the same call does nothing.

use super::super::owner_bindings::Channel;
use super::super::{NativeUnwindAction, SpecBinding};
use super::ThreadingMode;
use crate::emacs_core::eval::subr_abi::SubrFn;
use crate::emacs_core::eval::subr_abi::SubrValueExt;
use crate::emacs_core::eval::{Context, SubrEntry, register_global_subr_entry};
use crate::emacs_core::print::print_value;
use neovm_value::error::EvalResult;
use neovm_value::intern::{SymId, intern};
use neovm_value::tagged::header::SubrDispatchKind;
use neovm_value::value::Value;

fn eval(ev: &mut Context, src: &str) -> String {
    match ev.eval_str(src) {
        Ok(value) => print_value(&value),
        Err(error) => format!("ERR {error:?}"),
    }
}

// ---------------------------------------------------------------------------
// Eager owner vs late admission
// ---------------------------------------------------------------------------

const VARS: [&str; 3] = ["trnd-a", "trnd-b", "trnd-c"];
const BUFFERS: [&str; 2] = ["trnd-1", "trnd-2"];

const SETUP: &str = r#"
(progn
  (defvar trnd-a 0)
  (defvar trnd-b 10)
  (make-variable-buffer-local 'trnd-b)
  (defvar trnd-c 20)
  (get-buffer-create "trnd-1")
  (get-buffer-create "trnd-2")
  (set-buffer "trnd-1")
  (fset 'trnd-get
        (lambda (sym) (condition-case nil (symbol-value sym) (void-variable 'void))))
  (fset 'trnd-default
        (lambda (sym) (condition-case nil (default-value sym) (void-variable 'void))))
  (fset 'trnd-obs
        (lambda ()
          (list (mapcar (lambda (b)
                          (save-current-buffer
                            (set-buffer b)
                            (list (trnd-get 'trnd-a)
                                  (trnd-get 'trnd-b) (local-variable-p 'trnd-b)
                                  (trnd-get 'trnd-c) (local-variable-p 'trnd-c))))
                        '("trnd-1" "trnd-2"))
                (trnd-default 'trnd-a) (trnd-default 'trnd-b) (trnd-default 'trnd-c)
                (buffer-name)))))
"#;

#[derive(Clone, Copy, Debug)]
enum Op {
    Bind(usize, i64),
    Unbind,
    Set(usize, i64),
    SetDefault(usize, i64),
    /// `trnd-b` or `trnd-c` (index 1 or 2).
    MakeLocal(usize),
    KillLocal(usize),
    Makunbound,
    Switch(usize),
    /// `backtrace-eval`'s exchange of the specpdl suffix of this many
    /// entries (1-based fraction of the specpdl), then its rewind entry.
    BacktraceEval(usize),
}

struct Arm {
    ev: Context,
    /// The specpdl depth before each live binding or `backtrace-eval`, and
    /// whether it is a `backtrace-eval`.
    frames: Vec<(usize, bool)>,
}

impl Arm {
    #[doc(hidden)]
    pub fn new(admit: bool) -> Self {
        let mut ev = Context::new_with_threading(ThreadingMode::Green);
        let setup = eval(&mut ev, SETUP);
        assert!(!setup.starts_with("ERR"), "{setup}");
        if admit {
            ev.admit_threads().expect("eager admission");
        }
        Self {
            ev,
            frames: Vec::new(),
        }
    }

    #[doc(hidden)]
    pub fn apply(&mut self, op: Op) {
        let ev = &mut self.ev;
        // Direct calls below run on whichever Context last installed its
        // thread state; this arm's must be the installed one.
        ev.setup_thread_locals();
        match op {
            Op::Bind(var, value) => {
                self.frames.push((ev.specpdl.len(), false));
                ev.try_specbind(intern(VARS[var]), Value::fixnum(value))
                    .expect("bind");
            }
            Op::Unbind => {
                if let Some((depth, _)) = self.frames.pop() {
                    ev.unbind_to(depth);
                }
            }
            Op::Set(var, value) => {
                eval(ev, &format!("(setq {} {value})", VARS[var]));
            }
            Op::SetDefault(var, value) => {
                eval(ev, &format!("(set-default '{} {value})", VARS[var]));
            }
            Op::MakeLocal(var) => {
                eval(ev, &format!("(make-local-variable '{})", VARS[var]));
            }
            Op::KillLocal(var) => {
                eval(ev, &format!("(kill-local-variable '{})", VARS[var]));
            }
            Op::Makunbound => {
                eval(ev, "(makunbound 'trnd-a)");
            }
            Op::Switch(buffer) => {
                eval(ev, &format!("(set-buffer \"{}\")", BUFFERS[buffer]));
            }
            Op::BacktraceEval(fraction) => {
                let depth = ev.specpdl.len();
                if depth == 0 {
                    return;
                }
                let distance = 1 + fraction % depth;
                ev.specpdl_swap_suffix_for_backtrace_eval(distance, true)
                    .expect("backtrace-eval exchange");
                self.frames.push((depth, true));
                ev.push_specpdl_with(|| SpecBinding::NativeUnwind {
                    action: NativeUnwindAction::BacktraceEvalRewind { distance },
                });
            }
        }
    }

    #[doc(hidden)]
    pub fn observe(&mut self) -> String {
        eval(&mut self.ev, "(trnd-obs)")
    }

    /// Canonical storage: what an owner with no bindings would see.
    #[doc(hidden)]
    pub fn canonical(&mut self) -> Vec<String> {
        self.ev.setup_thread_locals();
        let buffers: Vec<_> = BUFFERS
            .iter()
            .map(|name| {
                self.ev
                    .buffers
                    .find_buffer_by_name(name)
                    .expect("trnd buffer")
            })
            .collect();
        let mut seen = Vec::new();
        for name in VARS {
            let id: SymId = intern(name);
            seen.push(print_value(
                &self.ev.owner_canonical_value(id, Channel::Default),
            ));
            for &buffer in &buffers {
                seen.push(print_value(
                    &self
                        .ev
                        .owner_canonical_value(id, Channel::BufferLocal(buffer)),
                ));
            }
        }
        seen
    }
}

fn ops(seed: u64, steps: usize) -> Vec<Op> {
    let mut random = seed;
    let mut next = move || {
        random = random
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (random >> 33) as usize
    };
    (0..steps)
        .map(|step| {
            let value = 100 + step as i64;
            match next() % 12 {
                0..=2 => Op::Bind(next() % VARS.len(), value),
                3 | 4 => Op::Unbind,
                5 => Op::Set(next() % VARS.len(), value),
                6 => Op::SetDefault(next() % VARS.len(), value),
                7 => Op::MakeLocal(1 + next() % 2),
                8 => Op::KillLocal(1 + next() % 2),
                9 => Op::Makunbound,
                10 => Op::Switch(next() % BUFFERS.len()),
                _ => Op::BacktraceEval(next()),
            }
        })
        .collect()
}

/// `TRND_SEEDS` and `TRND_STEPS` widen the run (defaults 256 and 64); it
/// passed 2000 x 64 and 600 x 200 when written.
#[test]
fn a_late_admission_matches_an_owner_admitted_from_the_start() {
    crate::test_utils::init_test_tracing();
    let knob = |name: &str, default: usize| {
        std::env::var(name)
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(default)
    };
    let steps = knob("TRND_STEPS", 64);
    let seeds = knob("TRND_SEEDS", 256) as u64;
    for seed in 1..=seeds {
        let history = ops(seed, steps);
        let admit_at = (seed as usize * 7) % steps;
        let mut eager = Arm::new(true);
        let mut late = Arm::new(false);
        let mut log = Vec::new();
        for (step, op) in history.iter().copied().enumerate() {
            if step == admit_at {
                late.ev
                    .admit_threads()
                    .unwrap_or_else(|error| panic!("seed {seed}: late admission: {error}"));
                log.push("ADMIT".to_string());
                assert_eq!(
                    late.canonical(),
                    eager.canonical(),
                    "seed {seed}, admitted at {step}: canonical storage\n{log:#?}"
                );
            }
            eager.apply(op);
            late.apply(op);
            log.push(format!("{op:?}"));
            assert_eq!(
                late.observe(),
                eager.observe(),
                "seed {seed}, step {step}, admitted at {admit_at}\n{log:#?}"
            );
            if step >= admit_at {
                assert_eq!(
                    late.canonical(),
                    eager.canonical(),
                    "seed {seed}, step {step}: canonical storage\n{log:#?}"
                );
            }
        }
        eager.ev.setup_thread_locals();
        eager.ev.unbind_to(0);
        late.ev.setup_thread_locals();
        late.ev.unbind_to(0);
        assert_eq!(
            late.observe(),
            eager.observe(),
            "seed {seed}: after unwinding\n{log:#?}"
        );
        assert_eq!(late.canonical(), eager.canonical(), "seed {seed}: unwound");
    }
}

// ---------------------------------------------------------------------------
// Admission from inside running Lisp
// ---------------------------------------------------------------------------

/// `(tad-admit)`: admit this World's threads if it runs green ones and has
/// not yet; nothing in a Sim World. Returns t.
fn admit_from_lisp(ctx: &mut Context) -> EvalResult {
    if ctx.threading_mode() == ThreadingMode::Green && !ctx.threads_admitted() {
        ctx.admit_threads().expect("admission from Lisp");
    }
    Ok(Value::T)
}

fn with_admit_subr(mode: ThreadingMode) -> Context {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new_with_threading(mode);
    let id = intern("tad-admit");
    register_global_subr_entry(
        id,
        SubrEntry {
            function: Some(SubrFn::A0(admit_from_lisp)),
            min_args: 0,
            max_args: Some(0),
            dispatch_kind: SubrDispatchKind::Builtin,
            interactive_spec: None,
        },
    );
    ev.set_function("tad-admit", <Value as SubrValueExt>::subr_from_sym_id(id));
    ev
}

/// Run each form in a Sim World, where `(tad-admit)` does nothing, and in
/// a Green one, where it admits; the printed results must agree, and the
/// Green World must have admitted.
fn assert_transparent(setup: &str, forms: &[&str]) {
    let mut sim = with_admit_subr(ThreadingMode::Sim);
    let mut green = with_admit_subr(ThreadingMode::Green);
    eval(&mut sim, setup);
    eval(&mut green, setup);
    for form in forms {
        let expected = eval(&mut sim, form);
        assert!(!expected.starts_with("ERR"), "{form} => {expected}");
        assert_eq!(eval(&mut green, form), expected, "{form}");
    }
    assert!(green.threads_admitted(), "the forms admitted");
    assert!(!sim.threads_admitted());
}

#[test]
fn admission_under_live_interpreter_frames() {
    assert_transparent(
        "(progn (defvar tal-v 1) (defvar tal-w 10)
                (fset 'tal-inner (lambda () (list tal-v (tad-admit) tal-v (setq tal-v 3) tal-v)))
                (fset 'tal-outer (lambda () (let ((tal-v 2) (tal-w 20)) (list (tal-inner) tal-v tal-w)))))",
        &[
            "(list (tal-outer) tal-v tal-w)",
            "(let ((tal-v 4)) (list (let ((tal-v 5)) tal-v) tal-v))",
            "(list tal-v tal-w)",
        ],
    );
}

/// The byte-code of `(lambda () (let ((tal-b 2)) (list (tad-admit) tal-b)))`.
fn bytecode_binder(ev: &mut Context) -> String {
    // constants: [tal-b 2 tad-admit list]
    // 193 const 2; 24 varbind tal-b; 195 const list; 194 const tad-admit;
    // 32 call 0; 8 varref tal-b; 34 call 2; 41 unbind 1; 135 return
    eval(
        ev,
        "(fset 'tal-compiled
               (make-byte-code
                nil
                (unibyte-string 193 24 195 194 32 8 34 41 135)
                [tal-b 2 tad-admit list]
                4))",
    )
}

#[test]
fn admission_under_a_live_vm_frame() {
    let mut results = Vec::new();
    for mode in [ThreadingMode::Sim, ThreadingMode::Green] {
        let mut ev = with_admit_subr(mode);
        eval(
            &mut ev,
            "(progn (defvar tal-b 1)
                    (fset 'tad-admit-real (symbol-function 'tad-admit))
                    (fset 'tad-admit (lambda () t)))",
        );
        bytecode_binder(&mut ev);
        // Warm the function, so a tier that compiles hot code may run the
        // admitting call.
        eval(
            &mut ev,
            "(let ((i 0)) (while (< i 3000) (tal-compiled) (setq i (1+ i))))",
        );
        assert!(!ev.threads_admitted());
        eval(
            &mut ev,
            "(fset 'tad-admit (symbol-function 'tad-admit-real))",
        );
        results.push(eval(
            &mut ev,
            "(list (tal-compiled) tal-b (tal-compiled) tal-b)",
        ));
        assert_eq!(ev.threads_admitted(), mode == ThreadingMode::Green);
    }
    assert_eq!(results[0], "((t 2) 1 (t 2) 1)");
    assert_eq!(results[1], results[0]);
}

#[test]
fn admission_inside_a_variable_watcher() {
    assert_transparent(
        "(progn (defvar taw-v 1) (defvar taw-log nil)
                (add-variable-watcher 'taw-v
                  (lambda (_sym new op _where)
                    (setq taw-log (cons (list op new) taw-log))
                    (if (eq op 'let) (tad-admit)))))",
        &[
            "(list (let ((taw-v 2)) (list taw-v (let ((taw-v 3)) taw-v) taw-v)) taw-v)",
            "(progn (let ((taw-v 4)) (setq taw-v 5)) (list taw-v taw-log))",
        ],
    );
}

#[test]
fn admission_inside_an_unwind_protect_cleanup() {
    assert_transparent(
        "(progn (defvar tau-v 1) (defvar tau-w 10))",
        &[
            "(let ((tau-v 2)) (list (unwind-protect tau-v (tad-admit) (setq tau-w 11)) tau-v tau-w))",
            "(list tau-v tau-w)",
        ],
    );
}

#[test]
fn admission_inside_a_cleanup_run_by_a_throw() {
    assert_transparent(
        "(progn (defvar tat-v 1) (defvar tat-w 10))",
        &["(list (catch 'tat-tag
                     (let ((tat-v 2) (tat-w 20))
                       (let ((tat-v 3))
                         (unwind-protect (throw 'tat-tag (list tat-v tat-w))
                           (tad-admit)))))
                   tat-v tat-w)"],
    );
}

#[test]
fn admission_inside_a_backtrace_eval() {
    assert_transparent(
        "(progn (defvar tab-v 1)
                (fset 'tab-inner
                      (lambda ()
                        (let ((tab-v 3))
                          (list tab-v
                                (backtrace-eval '(list tab-v (tad-admit) tab-v (setq tab-v 9)) 0)
                                tab-v))))
                (fset 'tab-outer (lambda () (let ((tab-v 2)) (list (tab-inner) tab-v)))))",
        &["(list (tab-outer) tab-v)", "tab-v"],
    );
}

#[test]
fn a_backtrace_eval_in_progress_rewinds_after_admission() {
    let mut ev = Context::new_with_threading(ThreadingMode::Green);
    eval(&mut ev, "(defvar tab-r 1)");
    let id = intern("tab-r");
    ev.try_specbind(id, Value::fixnum(2)).expect("bind");
    ev.try_specbind(id, Value::fixnum(3)).expect("bind");
    let depth = ev.specpdl.len();
    // `backtrace-eval` of the innermost binding's frame.
    ev.specpdl_swap_suffix_for_backtrace_eval(1, true)
        .expect("exchange");
    ev.push_specpdl_with(|| SpecBinding::NativeUnwind {
        action: NativeUnwindAction::BacktraceEvalRewind { distance: 1 },
    });
    assert_eq!(eval(&mut ev, "tab-r"), "2");
    ev.admit_threads().expect("admission");
    assert_eq!(
        eval(&mut ev, "tab-r"),
        "2",
        "the exchange survives admission"
    );
    eval(&mut ev, "(setq tab-r 7)");
    ev.unbind_to(depth);
    assert_eq!(
        eval(&mut ev, "tab-r"),
        "3",
        "the rewind restores the frame's binding"
    );
    ev.unbind_to(depth - 1);
    assert_eq!(
        eval(&mut ev, "tab-r"),
        "7",
        "the exchange's write is the outer binding's"
    );
    ev.unbind_to(0);
    assert_eq!(eval(&mut ev, "tab-r"), "1");
}

/// Two Worlds on one OS thread used to share the process's one collector
/// thread: a concurrent mark World A left running occupied it until A's
/// mutator stopped the job, so World B's synchronous collection
/// (`mark_all_on_gc_thread`) waited forever. Each heap now has its own
/// collector (`tagged/gc/gc_thread.rs` `COLLECTORS`).
#[test]
fn a_second_worlds_sync_collection_completes_while_the_first_marks() {
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut first = Context::new();
        let mut second = Context::new();
        first.setup_thread_locals();
        let mut started = false;
        // A generational heap runs up to 64 minors before a concurrent major.
        for _ in 0..4096 {
            eval(
                &mut first,
                "(let ((i 0)) (while (< i 2000) (make-list 8 i) (setq i (1+ i))))",
            );
            first.gc_collect_from_current_roots_impl(false);
            if first.tagged_heap.concurrent_mark_running() {
                started = true;
                break;
            }
        }
        assert!(started, "the first World never launched a concurrent mark");
        // The first World's mutator moves on without stopping its job.
        second.setup_thread_locals();
        second.gc_collect_from_current_roots_impl(true);
        let _ = done_tx.send(());
    });
    assert!(
        done_rx
            .recv_timeout(std::time::Duration::from_secs(30))
            .is_ok(),
        "the second World's synchronous collection is stuck behind the first World's concurrent mark"
    );
}
