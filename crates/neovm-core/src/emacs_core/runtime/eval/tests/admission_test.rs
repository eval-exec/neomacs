//! Phase 0 step 2, round 3c: thread admission turns the owner's live `let`
//! entries into owner bindings, publishes what the outermost ones hide as
//! canonical values, and is one-time and sticky. Ports C4's admission tests
//! (`facade_binding_admission_test.rs:38, :210, :238`) and reconstruction
//! tests (`facade_bindings_test.rs:648, :775`) to the owner-slab model.

use super::super::owner_bindings::{Channel, UndoTarget};
use super::super::{NativeUnwindAction, SpecBinding};
use super::{AdmissionError, ThreadingMode};
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::{SymId, intern};
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::Value;

fn green() -> Context {
    crate::test_utils::init_test_tracing();
    Context::new_with_threading(ThreadingMode::Green)
}

fn eval(ev: &mut Context, src: &str) -> String {
    match ev.eval_str(src) {
        Ok(value) => print_value(&value),
        Err(error) => format!("ERR {error:?}"),
    }
}

fn canonical(ev: &mut Context, id: SymId, channel: Channel) -> String {
    print_value(&ev.owner_canonical_value(id, channel))
}

fn owner_entries(ev: &Context) -> usize {
    ev.specpdl
        .iter()
        .filter(|entry| {
            matches!(
                entry,
                SpecBinding::NativeUnwind {
                    action: NativeUnwindAction::OwnerBinding(_)
                }
            )
        })
        .count()
}

fn legacy_lets(ev: &Context) -> usize {
    ev.specpdl
        .iter()
        .filter(|entry| entry.let_bound_symbol().is_some())
        .count()
}

#[test]
fn admission_needs_a_green_world_and_is_sticky() {
    let mut sim = Context::new();
    assert!(matches!(sim.admit_threads(), Err(AdmissionError::NotGreen)));
    assert!(!sim.threads_admitted());

    let mut ev = green();
    ev.admit_threads().expect("admission");
    assert!(ev.threads_admitted());
    assert!(matches!(
        ev.admit_threads(),
        Err(AdmissionError::AlreadyAdmitted)
    ));
    assert!(ev.threads_admitted());
}

#[test]
fn admission_reconstructs_the_bottom_saved_value_and_nested_bindings() {
    let mut ev = green();
    eval(&mut ev, "(defvar tad-bottom 1)");
    let id = intern("tad-bottom");
    ev.try_specbind(id, Value::fixnum(2)).expect("bind");
    let inner = ev.specpdl.len();
    ev.try_specbind(id, Value::fixnum(3)).expect("bind");
    ev.admit_threads().expect("admission");

    assert!(ev.obarray.thread_projection_of(id).is_some());
    assert_eq!(owner_entries(&ev), 2);
    assert_eq!(legacy_lets(&ev), 0);
    assert_eq!(eval(&mut ev, "tad-bottom"), "3");
    assert_eq!(canonical(&mut ev, id, Channel::Default), "1");
    let undo: Vec<_> = ev
        .owner
        .slab()
        .expect("the owner slab")
        .records_of(id)
        .map(|record| record.undo)
        .collect();
    assert!(matches!(
        undo[..],
        [UndoTarget::Inherited { suspended: None }, UndoTarget::Previous(outer)]
            if outer.get() == Value::fixnum(2)
    ));

    ev.unbind_to(inner);
    assert_eq!(eval(&mut ev, "tad-bottom"), "2");
    ev.unbind_to(0);
    assert_eq!(eval(&mut ev, "tad-bottom"), "1");
    assert_eq!(canonical(&mut ev, id, Channel::Default), "1");
}

#[test]
fn admission_publishes_the_first_saved_value_per_channel() {
    let mut ev = green();
    eval(
        &mut ev,
        "(progn (defvar tad-first 10) (defvar tad-auto 20)
                (make-variable-buffer-local 'tad-auto)
                (set-buffer (get-buffer-create \"tad-local\"))
                (setq tad-auto 30))",
    );
    let first = intern("tad-first");
    let auto = intern("tad-auto");
    let buffer = ev.buffers.current_buffer_id().expect("a current buffer");
    ev.try_specbind(first, Value::fixnum(11)).expect("bind");
    ev.try_specbind(first, Value::fixnum(12)).expect("bind");
    // A local exists in the current buffer: these are LetLocal bindings.
    ev.try_specbind(auto, Value::fixnum(31)).expect("bind");
    ev.try_specbind(auto, Value::fixnum(32)).expect("bind");
    ev.admit_threads().expect("admission");

    assert_eq!(owner_entries(&ev), 4);
    assert_eq!(canonical(&mut ev, first, Channel::Default), "10");
    assert_eq!(canonical(&mut ev, auto, Channel::BufferLocal(buffer)), "30");
    assert_eq!(canonical(&mut ev, auto, Channel::Default), "20");
    assert_eq!(eval(&mut ev, "(list tad-first tad-auto)"), "(12 32)");
    ev.unbind_to(0);
    assert_eq!(eval(&mut ev, "(list tad-first tad-auto)"), "(10 30)");
}

#[test]
fn a_capacity_refusal_leaves_the_owner_as_it_was() {
    let mut ev = green();
    eval(&mut ev, "(defvar tad-capacity 1)");
    let id = intern("tad-capacity");
    ev.try_specbind(id, Value::fixnum(2)).expect("bind");
    let before = ev.specpdl.len();
    let projections = ev.thread_projection_count_for_test();
    let inhibit = ev.gc_inhibit_depth;

    assert!(matches!(
        ev.admit_threads_refusing_capacity_for_test(),
        Err(AdmissionError::Capacity)
    ));
    assert!(!ev.threads_admitted());
    assert!(ev.owner.is_dormant(), "no slab was installed");
    assert_eq!(ev.gc_inhibit_depth, inhibit);
    assert_eq!(ev.specpdl.len(), before);
    assert_eq!(legacy_lets(&ev), 1);
    assert_eq!(owner_entries(&ev), 0);
    assert!(ev.obarray.thread_projection_of(id).is_none());
    assert_eq!(ev.thread_projection_count_for_test(), projections);
    assert_eq!(eval(&mut ev, "tad-capacity"), "2");
    ev.unbind_to(0);
    assert_eq!(eval(&mut ev, "tad-capacity"), "1");

    // The refusal left nothing behind: admission still succeeds.
    ev.admit_threads().expect("admission");
    assert!(ev.obarray.thread_projection_of(id).is_some());
}

#[test]
fn a_collection_right_after_admission_keeps_hidden_values() {
    let mut ev = green();
    eval(&mut ev, "(defvar tad-gc (cons 41 42))");
    let id = intern("tad-gc");
    ev.try_specbind(id, Value::NIL).expect("bind");
    ev.admit_threads().expect("admission");
    ev.gc_collect_exact();
    assert_eq!(
        eval(
            &mut ev,
            "(let ((i 0)) (while (< i 2000) (make-list 10 'garbage) (setq i (1+ i))))"
        ),
        "nil"
    );
    ev.gc_collect_exact();
    assert_eq!(eval(&mut ev, "tad-gc"), "nil");
    ev.unbind_to(0);
    assert_eq!(eval(&mut ev, "(car tad-gc)"), "41");
}

#[test]
fn an_inhibition_overflow_refuses_and_keeps_the_depth() {
    let mut ev = green();
    ev.gc_inhibit_depth = usize::MAX;
    assert!(matches!(
        ev.admit_threads(),
        Err(AdmissionError::InhibitDepthOverflow)
    ));
    assert_eq!(ev.gc_inhibit_depth, usize::MAX);
    assert!(!ev.threads_admitted());
    ev.gc_inhibit_depth = 0;
}

#[test]
fn a_symbol_born_after_admission_starts_on_void_and_holds_values() {
    let mut ev = green();
    ev.admit_threads().expect("admission");
    let id = intern("tad-born-later");
    ev.obarray.ensure_symbol_id(id);
    let projection = ev.obarray.thread_projection_of(id).expect("born projected");
    assert!(projection.is_void());
    assert_eq!(eval(&mut ev, "(boundp 'tad-born-later)"), "nil");
    assert_eq!(
        eval(&mut ev, "(progn (setq tad-born-later 7) tad-born-later)"),
        "7"
    );
    assert!(
        !ev.obarray
            .thread_projection_of(id)
            .expect("still projected")
            .is_void(),
        "a write materializes a descriptor of its own"
    );
    assert_eq!(
        eval(
            &mut ev,
            "(list (let ((tad-born-later 8)) tad-born-later) tad-born-later)"
        ),
        "(8 7)"
    );
}

#[test]
fn a_void_variable_stays_on_void_until_written() {
    let mut ev = green();
    eval(
        &mut ev,
        "(progn (defvar tad-void 0) (makunbound 'tad-void))",
    );
    ev.admit_threads().expect("admission");
    let id = intern("tad-void");
    assert!(
        ev.obarray
            .thread_projection_of(id)
            .expect("projected")
            .is_void()
    );
    assert_eq!(eval(&mut ev, "(boundp 'tad-void)"), "nil");
    assert_eq!(
        eval(
            &mut ev,
            "(list (let ((tad-void 1)) tad-void) (boundp 'tad-void))"
        ),
        "(1 nil)"
    );
}

#[test]
fn a_killed_local_binding_is_captured() {
    let mut ev = green();
    eval(
        &mut ev,
        "(progn (defvar tad-killed 1) (make-variable-buffer-local 'tad-killed)
                (set-buffer (get-buffer-create \"tad-killed\"))
                (setq tad-killed 2))",
    );
    let id = intern("tad-killed");
    let buffer = ev.buffers.current_buffer_id().expect("a current buffer");
    ev.try_specbind(id, Value::fixnum(3)).expect("bind");
    eval(&mut ev, "(kill-local-variable 'tad-killed)");
    ev.admit_threads().expect("admission");

    let record = *ev
        .owner
        .slab()
        .expect("the owner slab")
        .records_of(id)
        .next()
        .expect("the binding became an owner record");
    assert_eq!(record.channel, Channel::BufferLocal(buffer));
    assert!(matches!(record.undo, UndoTarget::Captured(saved) if saved.get() == Value::fixnum(2)));
    assert_eq!(eval(&mut ev, "tad-killed"), "1");
    // Made again, the local takes back what the binding hid at its pop.
    eval(&mut ev, "(make-local-variable 'tad-killed)");
    ev.unbind_to(0);
    assert_eq!(eval(&mut ev, "tad-killed"), "2");
}

#[test]
fn a_per_buffer_slot_binding_stays_in_place() {
    let mut ev = green();
    eval(
        &mut ev,
        "(progn (set-buffer (get-buffer-create \"tad-slot\"))
                (set (make-local-variable 'fill-column) 33))",
    );
    let id = intern("fill-column");
    ev.try_specbind(id, Value::fixnum(44)).expect("bind");
    ev.try_specbind(intern("buffer-undo-list"), Value::T)
        .expect("bind");
    ev.admit_threads().expect("admission");

    assert_eq!(owner_entries(&ev), 0);
    assert!(matches!(
        ev.specpdl[..],
        [SpecBinding::LetLocal { .. }, SpecBinding::LetLocal { .. }]
    ));
    assert!(ev.obarray.thread_projection_of(id).is_some());
    assert!(
        ev.obarray
            .thread_projection_of(intern("buffer-undo-list"))
            .is_none()
    );
    assert_eq!(
        eval(&mut ev, "(list fill-column buffer-undo-list)"),
        "(44 t)"
    );
    ev.unbind_to(0);
    assert_eq!(eval(&mut ev, "fill-column"), "33");
}

#[test]
fn admission_moves_the_function_epoch_and_republishes_mirrors() {
    let mut ev = green();
    let id = intern("inhibit-quit");
    ev.try_specbind(id, Value::T).expect("bind");
    let epoch = ev.obarray.function_epoch();
    ev.admit_threads().expect("admission");
    assert_ne!(ev.obarray.function_epoch(), epoch);
    assert_eq!(ev.inhibit_quit, Value::T, "the owner's binding is its view");
    assert_eq!(canonical(&mut ev, id, Channel::Default), "nil");
    ev.unbind_to(0);
    assert_eq!(ev.inhibit_quit, Value::NIL);
}

#[test]
fn an_owner_binding_of_debug_on_next_call_takes_the_header() {
    let mut ev = green();
    let id = intern("debug-on-next-call");
    ev.try_specbind(id, Value::T).expect("bind");
    ev.admit_threads().expect("admission");
    assert!(ev.debug_on_next_call_is_armed());
    assert_eq!(canonical(&mut ev, id, Channel::Default), "nil");
    ev.unbind_to(0);
    assert!(!ev.debug_on_next_call_is_armed());
}

#[test]
fn symbols_born_after_admission_behave_as_before() {
    // Each form runs on symbols no cell exists for yet, in a Sim World and
    // in an admitted Green one; the results must agree.
    let forms = [
        "(let ((tfr-a 1)) (list tfr-a (boundp 'tfr-a)))",
        "(list (boundp 'tfr-a) (let ((tfr-b 2)) (let ((tfr-b 3)) tfr-b)))",
        "(progn (defvar tfr-c 4) (list tfr-c (let ((tfr-c 5)) tfr-c) tfr-c))",
        "(progn (set 'tfr-d 6) (list tfr-d (let ((tfr-d 7)) tfr-d) tfr-d))",
        "(progn (set-default 'tfr-e 8) (list tfr-e (default-value 'tfr-e)))",
        "(progn (make-local-variable 'tfr-f) (setq tfr-f 9)
                (list tfr-f (local-variable-p 'tfr-f) (default-boundp 'tfr-f)))",
        "(progn (make-variable-buffer-local 'tfr-g) (setq tfr-g 10)
                (list tfr-g (local-variable-p 'tfr-g) (default-value 'tfr-g)))",
        "(progn (defvaralias 'tfr-h 'tfr-i) (setq tfr-h 11) (list tfr-h tfr-i))",
        "(funcall (lambda (tfr-j) (list tfr-j (symbol-value 'tfr-j))) 12)",
        "(progn (defvar tfr-k) (let ((tfr-k 13)) (list tfr-k (boundp 'tfr-k))))",
        "(list (boundp 'tfr-a) (boundp 'tfr-b) (boundp 'tfr-j) (boundp 'tfr-k))",
        // Variables valued before admission, aliased after it.
        "(progn (defvaralias 'tfr-pre 'tfr-pre-base) (list tfr-pre tfr-pre-base))",
        "(progn (setq tfr-pre 14) (list tfr-pre-base (indirect-variable 'tfr-pre)))",
        "(condition-case e (defvaralias 'tfr-pre-local 'tfr-a) (error (car e)))",
    ];
    let setup = "(progn (defvar tfr-pre 1) (defvar tfr-pre-local 2)
                        (make-variable-buffer-local 'tfr-pre-local))";
    let mut sim = Context::new();
    let mut ev = green();
    eval(&mut sim, setup);
    eval(&mut ev, setup);
    ev.admit_threads().expect("admission");
    for form in forms {
        let expected = eval(&mut sim, form);
        assert!(!expected.starts_with("ERR"), "{form} => {expected}");
        assert_eq!(eval(&mut ev, form), expected, "{form}");
    }
}

/// Resident set size in KiB (`/proc/self/statm`), or 0 off Linux.
fn rss_kib() -> u64 {
    std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|statm| statm.split_whitespace().nth(1)?.parse::<u64>().ok())
        .map_or(0, |pages| pages * 4)
}

/// Cold inventory of admission on the loaded runtime image: the cells it
/// projects, how many own a descriptor, its time and memory. Run inside
/// `tmp/v10x/sandbox-run.sh` with `--run-ignored only`.
#[test]
#[cfg(feature = "perf-probes")]
fn admission_inventory_probe() {
    use crate::emacs_core::symbol::AdmissionPlanCell;

    let mut ev = crate::test_utils::runtime_startup_context();
    ev.set_threading_mode_at_construction(ThreadingMode::Green);
    eval(&mut ev, "(defvar tad-probe 1)");
    let id = intern("tad-probe");
    ev.try_specbind(id, Value::fixnum(2)).expect("bind");
    let undo_list = super::super::buffer_undo_list_symbol();
    let (mut cells, mut owned, mut void, mut skipped) = (0usize, 0usize, 0usize, 0usize);
    for (id, _) in ev.obarray.iter_symbols() {
        cells += 1;
        match (id == undo_list, ev.obarray.admission_plan(id)) {
            (false, Some(AdmissionPlanCell::Owned)) => owned += 1,
            (false, Some(AdmissionPlanCell::Void)) => void += 1,
            _ => skipped += 1,
        }
    }
    let entries = ev.specpdl.len();
    ev.gc_collect_exact();
    let rss_before = rss_kib();
    let started = std::time::Instant::now();
    ev.admit_threads().expect("admission");
    let elapsed = started.elapsed();
    let rss_after = rss_kib();
    let issued = ev.thread_projection_count_for_test();
    let descriptor = std::mem::size_of::<crate::emacs_core::forward::ThreadProjectionDescriptor>();
    println!(
        "ADMISSION-PROBE cells={cells} owned={owned} void={void} skipped={skipped} \
         specpdl={entries} issued={issued} descriptor_bytes={descriptor} \
         arena_kib={} admit_us={} rss_before_kib={rss_before} rss_after_kib={rss_after} \
         rss_delta_kib={}",
        issued.div_ceil(256) * 256 * descriptor / 1024,
        elapsed.as_micros(),
        rss_after.saturating_sub(rss_before),
    );
    assert_eq!(issued, owned, "every owned cell got a descriptor");
    // The loaded image keeps working once admitted.
    assert_eq!(eval(&mut ev, "tad-probe"), "2");
    assert_eq!(
        eval(
            &mut ev,
            "(list (let ((case-fold-search nil)) (string-match \"a\" \"A\"))
                   (let ((fill-column 33)) fill-column)
                   (with-temp-buffer (insert \"abc\") (buffer-string))
                   (progn (defvar tad-probe-new 5) (let ((tad-probe-new 6)) tad-probe-new)))"
        ),
        "(nil 33 \"abc\" 6)"
    );
    ev.unbind_to(0);
    assert_eq!(eval(&mut ev, "tad-probe"), "1");
}

/// A `let` of a per-buffer variable in a buffer with no local binds its
/// default in place (design §1.3b): GNU's `set_default`, which every
/// non-local buffer's slot -- and so every direct slot reader (redisplay's
/// `BVAR`) -- shows. The entry is a legacy `LetDefault`, swapped at
/// park/resume.
#[test]
fn a_default_binding_of_a_per_buffer_variable_reaches_direct_slot_readers() {
    let mut ev = green();
    eval(&mut ev, "(set-buffer (get-buffer-create \"tad-truncate\"))");
    ev.admit_threads().expect("admission");
    let id = intern("truncate-lines");
    let info = crate::buffer::buffer::lookup_buffer_slot_by_sym_id(id).expect("a per-buffer slot");
    let buffer = ev.buffers.current_buffer_id().expect("a current buffer");
    let count = ev.specpdl.len();
    ev.try_specbind(id, Value::T).expect("bind");
    let slot = |ev: &Context| ev.buffers.get(buffer).expect("live").slots[info.offset.index()];
    assert_eq!(eval(&mut ev, "truncate-lines"), "t");
    assert_eq!(slot(&ev), Value::T, "a direct slot reader sees the binding");
    ev.unbind_to(count);
    assert_eq!(slot(&ev), Value::NIL);
}

/// Review condition 2 of round 3: `defvaralias` of a plain variable another
/// owner has let-bound. GNU's let-bound refusal walks only the current
/// thread's specpdl (`src/eval.c:703-711`), so GNU succeeds. The generated
/// `oracle_green_threads` pin confirms the deterministic result: the parked
/// owner's binding follows the alias and restores the target's outer value.
#[test]
fn defvaralias_of_a_variable_another_owner_binds_follows_the_gnu_oracle() {
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::force_jit_off_for_test(true);
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            r##"(eval '
(progn
  (defvar g3e-source 'source-global)
  (defvar g3e-target 'target-global)
  (let* ((mutex (make-mutex))
         (condition (make-condition-variable mutex))
         (ready nil) (go nil) (worker nil))
    (mutex-lock mutex)
    (setq worker
          (make-thread
           (lambda ()
             (let ((g3e-source 'source-bound))
               (mutex-lock mutex)
               (setq ready t)
               (condition-notify condition)
               (while (null go) (condition-wait condition))
               (mutex-unlock mutex)
               (list g3e-source g3e-target (default-toplevel-value 'g3e-source) (default-toplevel-value 'g3e-target))))))
    (while (null ready) (condition-wait condition))
    (let ((aliased (defvaralias 'g3e-source 'g3e-target)))
      (setq go t)
      (condition-notify condition)
      (mutex-unlock mutex)
      (list aliased (thread-join worker) g3e-source g3e-target))))
 t)"##
        ),
        "(g3e-target (source-bound source-bound target-global source-bound) target-global target-global)"
    );
}

fn truncate_slot(ev: &Context, buffer: crate::buffer::BufferId) -> Value {
    let info = crate::buffer::buffer::lookup_buffer_slot_by_sym_id(intern("truncate-lines"))
        .expect("a per-buffer slot");
    ev.buffers.get(buffer).expect("live").slots[info.offset.index()]
}

#[test]
fn a_parked_default_binding_lets_another_owner_set_the_default() {
    let mut ev = green();
    eval(
        &mut ev,
        "(set-buffer (get-buffer-create \"tad-park-default\"))",
    );
    ev.admit_threads().expect("admission");
    let buffer = ev.buffers.current_buffer_id().expect("a current buffer");
    ev.try_specbind(intern("truncate-lines"), Value::T)
        .expect("bind");
    assert!(matches!(ev.specpdl[..], [SpecBinding::LetDefault { .. }]));
    assert_eq!(truncate_slot(&ev, buffer), Value::T);
    let parked = ev.park_owner();
    // The other owner sees the outer default and sets it.
    assert_eq!(truncate_slot(&ev, buffer), Value::NIL);
    eval(&mut ev, "(set-default 'truncate-lines 'other)");
    ev.resume_owner(parked);
    assert_eq!(truncate_slot(&ev, buffer), Value::T, "the binding is back");
    assert_eq!(eval(&mut ev, "truncate-lines"), "t");
    ev.unbind_to(0);
    // The other owner's write is what the binding hid when it popped.
    assert_eq!(eval(&mut ev, "(default-value 'truncate-lines)"), "other");
    assert_eq!(eval(&mut ev, "truncate-lines"), "other");
}

#[test]
fn a_buffer_made_inside_a_default_binding_starts_with_it() {
    let mut ev = green();
    eval(
        &mut ev,
        "(set-buffer (get-buffer-create \"tad-made-inside\"))",
    );
    ev.admit_threads().expect("admission");
    ev.try_specbind(intern("truncate-lines"), Value::T)
        .expect("bind");
    assert_eq!(
        eval(
            &mut ev,
            "(save-current-buffer (set-buffer (get-buffer-create \"tad-made-inside-2\")) truncate-lines)"
        ),
        "t"
    );
    ev.unbind_to(0);
    assert_eq!(
        eval(
            &mut ev,
            "(save-current-buffer (set-buffer \"tad-made-inside-2\") truncate-lines)"
        ),
        "nil"
    );
}

#[test]
fn killing_a_buffer_inside_a_default_binding_keeps_the_restore() {
    let mut ev = green();
    eval(
        &mut ev,
        "(progn (get-buffer-create \"tad-kill-other\")
                (set-buffer (get-buffer-create \"tad-kill-home\")))",
    );
    ev.admit_threads().expect("admission");
    ev.try_specbind(intern("truncate-lines"), Value::T)
        .expect("bind");
    eval(&mut ev, "(kill-buffer \"tad-kill-other\")");
    let parked = ev.park_owner();
    ev.resume_owner(parked);
    ev.unbind_to(0);
    assert_eq!(eval(&mut ev, "(default-value 'truncate-lines)"), "nil");
    assert_eq!(eval(&mut ev, "truncate-lines"), "nil");
}

#[test]
fn admission_leaves_a_per_buffer_default_binding_legacy() {
    let mut ev = green();
    eval(
        &mut ev,
        "(set-buffer (get-buffer-create \"tad-legacy-default\"))",
    );
    ev.try_specbind(intern("truncate-lines"), Value::T)
        .expect("bind");
    ev.admit_threads().expect("admission");
    assert!(matches!(ev.specpdl[..], [SpecBinding::LetDefault { .. }]));
    let buffer = ev.buffers.current_buffer_id().expect("a current buffer");
    assert_eq!(truncate_slot(&ev, buffer), Value::T);
    ev.unbind_to(0);
    assert_eq!(truncate_slot(&ev, buffer), Value::NIL);
}

#[test]
fn selected_keyboard_binding_stays_owned_across_admission_and_thread_exchange() {
    let mut ev = green();
    let buffer = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buffer);
    let tty = ev
        .frames
        .create_frame_on_terminal("terminal", 1, 80, 25, buffer);
    let gui = ev
        .frames
        .create_frame_on_terminal("graphical", 0, 800, 600, buffer);
    ev.frames
        .get_mut(gui)
        .unwrap()
        .set_window_system(Some(Value::symbol("neo")));
    ev.set_variable("keyboard-test-terminal", Value::make_frame(tty.0));
    ev.set_variable("keyboard-test-graphical", Value::make_frame(gui.0));
    assert_eq!(
        eval(
            &mut ev,
            "(progn (select-frame keyboard-test-graphical) window-system)"
        ),
        "neo"
    );
    let symbol = intern("window-system");
    let depth = ev.specpdl.len();
    ev.try_specbind(symbol, Value::string("bound graphical keyboard"))
        .unwrap();
    assert_eq!(
        eval(
            &mut ev,
            "(progn (select-frame keyboard-test-terminal) window-system)"
        ),
        "nil"
    );

    ev.admit_threads().unwrap();
    assert!(
        ev.obarray.thread_projection_of(symbol).is_none(),
        "a default owner projection must not hide keyboard-local storage"
    );
    let suspended = ev.suspend_dynamic_bindings_for_thread_switch().unwrap();
    assert_eq!(
        eval(
            &mut ev,
            "(progn (select-frame keyboard-test-graphical) window-system)"
        ),
        "neo"
    );
    assert_eq!(
        eval(
            &mut ev,
            "(progn (select-frame keyboard-test-terminal) window-system)"
        ),
        "nil"
    );
    ev.resume_dynamic_bindings_for_thread_switch(suspended)
        .unwrap();
    assert_eq!(eval(&mut ev, "window-system"), "nil");
    ev.eval_str("(garbage-collect)").unwrap();
    assert_eq!(
        eval(
            &mut ev,
            "(progn (select-frame keyboard-test-graphical) window-system)"
        ),
        "\"bound graphical keyboard\""
    );
    assert_eq!(
        eval(
            &mut ev,
            "(progn (select-frame keyboard-test-terminal) window-system)"
        ),
        "nil"
    );
    ev.unbind_to(depth);
    assert_eq!(
        eval(
            &mut ev,
            "(progn (select-frame keyboard-test-graphical) window-system)"
        ),
        "neo"
    );
}
