//! L6.4's suspend half: a detached owner leaves no heap on its OS thread,
//! and reattaching it restores everything its next serial turn reads.

use super::{DetachedWorldCarriers, HostRefusal};
use crate::emacs_core::eval::parallel_entry::serial_turn::{
    OwnerSerialState, SerialTurn, TurnRefusal,
};
use crate::emacs_core::eval::{Context, SpecBinding, ThreadingMode};
use crate::emacs_core::print::print_value;
use neovm_value::intern::{SymId, intern};
use neovm_value::value::Value;

fn eval(ev: &mut Context, src: &str) -> String {
    match ev.eval_str(src) {
        Ok(value) => print_value(&value),
        Err(error) => format!("ERR {error:?}"),
    }
}

/// State only this owner has, held where a serial turn reads it through the
/// carrier TLS the detach takes down: its buffer's case table maps ?Q down to
/// ?a, its syntax table makes ?a punctuation, and its match data comes from
/// one match made before the detach.
const OWNER_STATE: &str = "(progn
  (let ((down (copy-sequence (standard-case-table))))
    (aset down ?Q ?a)
    (set-case-table down))
  (let ((table (copy-syntax-table)))
    (modify-syntax-entry ?a \".\" table)
    (set-syntax-table table))
  (string-match \"b+\" \"abbbc\"))";

/// Reads the owner state back without changing it: no form here matches a
/// regexp, so `(match-end 0)` is still the pre-detach match. Also an
/// ordinary global and the signal path.
const PROBE: &str = "(list sd-global (downcase ?Q) (char-syntax ?a) (match-end 0)
                           (condition-case err (signal 'sd-error '(1 2))
                             (sd-error (cdr err))))";

/// The probe's answer for the owner state, not the standard tables':
/// (downcase ?Q) is ?a (97), not ?q, and (char-syntax ?a) is ?. (46), not ?w.
const OWNER_ANSWER: &str = "(42 97 46 4 (1 2))";

fn owner() -> Context {
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    eval(
        &mut ev,
        "(progn (defvar sd-global 41) (setq sd-global 42)
                (put 'sd-error 'error-conditions '(sd-error error))
                (get-buffer-create \" sd-other\"))",
    );
    ev
}

fn detach(ev: &mut Context) -> super::DetachedOwner {
    match ev.detach_serial_owner() {
        Ok(detached) => detached,
        Err(refusal) => panic!("this Context's heap is installed: {refusal:?}"),
    }
}

/// This thread names no heap at all. Stricter than the carrier's job-start
/// check, which accepts an armed test fallback heap.
fn names_no_heap() -> bool {
    !neovm_value::tagged::gc::tagged_heap_is_installed()
}

fn reattach(ev: &mut Context, detached: super::DetachedOwner) {
    if ev.reattach_serial_owner(detached).is_err() {
        panic!("no other heap is installed on this thread");
    }
    assert!(
        neovm_value::tagged::gc::tagged_heap_is_current(&ev.tagged_heap),
        "reattach installs the owner's own heap"
    );
}

/// Inside a turn: the thread allocates into this owner's heap.
fn owns_the_thread(ev: &mut Context) -> bool {
    neovm_value::tagged::gc::tagged_heap_is_current(&ev.tagged_heap)
}

#[test]
fn l64_detach_stows_the_world_carriers_and_reattach_returns_them() {
    let mut ev =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    let _baton = ev.start_green_runtime().expect("start the Green runtime");
    assert_eq!(
        ev.serial_world_parked_for_test(),
        Some((false, false)),
        "a started runtime holds placeholders; the thread still has the world"
    );
    let detached = detach(&mut ev);
    assert_eq!(
        ev.serial_world_parked_for_test(),
        Some((true, true)),
        "the same detach stows the carried world and the warm carrier"
    );
    reattach(&mut ev, detached);
    assert_eq!(
        ev.serial_world_parked_for_test(),
        Some((false, false)),
        "reattach puts both back on this thread"
    );
}

/// (b) (c) (d) on the owner that has a warm carrier. A Sim owner skips the
/// stow, so the row above is the carrier half and the begin row is the
/// Sim half. This one runs the same reattach `begin` calls.
#[test]
fn l64_green_reattach_installs_the_heap_and_restores_frame_slab_buffer_and_carrier() {
    let mut ev =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    let _baton = ev.start_green_runtime().expect("start the Green runtime");
    eval(
        &mut ev,
        "(progn (defvar sd-global 41) (setq sd-global 42)
                (put 'sd-error 'error-conditions '(sd-error error))
                (get-buffer-create \" sd-other\"))",
    );
    assert_eq!(eval(&mut ev, OWNER_STATE), "1", "the match starts at 1");
    assert_eq!(
        eval(&mut ev, PROBE),
        OWNER_ANSWER,
        "the owner state is set before the detach"
    );
    assert_eq!(eval(&mut ev, "(lossage-size 417)"), "417");
    ev.display_var_change_count = 77;
    ev.face_change_count = 19;
    let buffer = ev.edit.buffers.current_buffer_id();
    let buffer_name = eval(&mut ev, "(buffer-name)");
    let frame = ev.frame_manager().selected;
    let frame_count = ev.frame_manager().frames.len();
    let slab = ev.ensure_owner_slab() as *const _ as usize;
    assert!(buffer.is_some(), "the owner has a current buffer");
    let detached = detach(&mut ev);
    assert_eq!(
        detached.world_carriers(),
        DetachedWorldCarriers::Stowed,
        "detaching the World carrier holder records Stowed"
    );
    assert!(names_no_heap(), "(b) detach takes this owner's heap down");
    assert_eq!(
        ev.serial_world_parked_for_test(),
        Some((true, true)),
        "(d) detach stows the warm carrier"
    );
    assert_eq!(
        ev.display_var_change_count, 0,
        "OwnerCarried::take moved the warm display counter out of the dormant Context"
    );
    assert_eq!(
        ev.face_change_count, 0,
        "OwnerCarried::take moved the warm face counter out of the dormant Context"
    );
    reattach(&mut ev, detached);
    assert_eq!(
        ev.serial_world_parked_for_test(),
        Some((false, false)),
        "(d) reattach restores the warm carrier before resume_owner refreshes the mirrors"
    );
    assert_eq!(
        eval(&mut ev, "(lossage-size)"),
        "417",
        "F5: non-default carried TLS value (lossage-size 417 != 300 placeholder) round-trips across detach and reattach"
    );
    assert_eq!(
        ev.display_var_change_count, 77,
        "F5: warm OwnerCarried display_var_change_count round-trips (drop(warm) would lose it)"
    );
    assert_eq!(
        ev.face_change_count, 19,
        "F5: warm OwnerCarried face_change_count round-trips (drop(warm) would lose it)"
    );
    assert_eq!(
        ev.edit.buffers.current_buffer_id(),
        buffer,
        "(c) the owner's buffer is back"
    );
    assert_eq!(
        eval(&mut ev, "(buffer-name)"),
        buffer_name,
        "(c) the owner's buffer is the one from before the detach"
    );
    assert_eq!(
        ev.frame_manager().selected,
        frame,
        "(c) the owner's frame selection is unchanged"
    );
    assert_eq!(
        ev.frame_manager().frames.len(),
        frame_count,
        "(c) reattach does not add or drop a frame"
    );
    assert_eq!(
        ev.owner.slab().map(|slab| slab as *const _ as usize),
        Some(slab),
        "(c) the owner's slab is back"
    );
    assert_eq!(
        eval(&mut ev, PROBE),
        OWNER_ANSWER,
        "(d) the mirrors show this owner again"
    );
}

#[test]
fn l64_detach_leaves_no_heap_on_this_thread_and_reattach_restores_the_turn() {
    let mut ev = owner();
    assert_eq!(eval(&mut ev, OWNER_STATE), "1", "the match starts at 1");
    assert_eq!(eval(&mut ev, PROBE), OWNER_ANSWER, "the owner state is set");
    let detached = detach(&mut ev);
    assert_eq!(
        detached.world_carriers(),
        DetachedWorldCarriers::NotStowed,
        "a Sim World has no Green runtime, so detach records NotStowed"
    );
    assert!(names_no_heap(), "a detached owner's thread names no heap");
    super::super::super::green::carrier_tls::carrier_tls_idle_state_and_check();
    reattach(&mut ev, detached);
    assert_eq!(
        eval(&mut ev, PROBE),
        OWNER_ANSWER,
        "the next turn reads the owner's own tables and match data"
    );
}

#[test]
fn l64_an_in_place_binding_swaps_out_on_detach_and_back_on_reattach() {
    // `buffer-undo-list` always binds in place: its storage is the buffer's
    // undo state, never an owner cell, so its `let` is a LetLocal entry that
    // the detach swaps with the buffer.
    let mut ev = owner();
    let id: SymId = intern("buffer-undo-list");
    eval(
        &mut ev,
        "(save-current-buffer (set-buffer \" sd-other\") (setq buffer-undo-list nil))",
    );
    let home = eval(&mut ev, "(buffer-name)");
    eval(&mut ev, "(set-buffer \" sd-other\")");
    let count = ev.specpdl.len();
    ev.try_specbind(id, Value::T).expect("bind");
    assert_eq!(eval(&mut ev, "buffer-undo-list"), "t");
    eval(&mut ev, &format!("(set-buffer {home})"));
    // The value the binding's LetLocal entry saves.
    let saved = |ev: &Context| {
        ev.specpdl
            .iter()
            .skip(count)
            .find_map(|entry| match entry {
                SpecBinding::LetLocal {
                    sym_id, old_value, ..
                } if *sym_id == id => Some(*old_value),
                _ => None,
            })
            .expect("the binding is a buffer-local let")
    };
    assert_eq!(saved(&ev), Value::NIL, "the let saves the value it hid");

    let detached = detach(&mut ev);
    assert_eq!(
        saved(&ev),
        Value::T,
        "while detached the let saves the owner's binding and the buffer holds nil"
    );
    reattach(&mut ev, detached);
    assert_eq!(saved(&ev), Value::NIL, "the binding is back in the buffer");

    assert_eq!(
        eval(
            &mut ev,
            "(save-current-buffer (set-buffer \" sd-other\") buffer-undo-list)"
        ),
        "t",
        "the owner's binding is back in the buffer's storage"
    );
    ev.unbind_to(count);
    assert_eq!(
        eval(
            &mut ev,
            "(save-current-buffer (set-buffer \" sd-other\") buffer-undo-list)"
        ),
        "nil",
        "unbinding restores the value the binding hid"
    );
}

#[test]
fn l64_two_turns_in_a_row_detach_and_reattach_the_same_owner() {
    let mut ev = owner();
    eval(&mut ev, OWNER_STATE);
    for turn in 0..2 {
        let detached = detach(&mut ev);
        reattach(&mut ev, detached);
        assert_eq!(eval(&mut ev, PROBE), OWNER_ANSWER, "turn {turn}");
    }
}

type State = OwnerSerialState<Context, (u64, &'static str)>;

/// An owner built on this thread and kept dormant: its heap leaves the
/// thread as the state is built.
fn dormant(owner_id: u64) -> State {
    let mut ev = owner();
    eval(&mut ev, OWNER_STATE);
    OwnerSerialState::new(owner_id, Box::new(ev))
        .ok()
        .expect("a freshly built Context detaches")
}

fn ended(turn: SerialTurn<'_, Context, (u64, &'static str)>) -> (u64, &'static str) {
    match turn.end() {
        Ok(ticket) => ticket,
        Err((_, refusal)) => panic!("the turn's own heap is installed: {refusal:?}"),
    }
}

#[test]
fn l64_turns_through_the_owner_state_reattach_and_detach_its_context() {
    let mut state = dormant(7);
    assert!(names_no_heap(), "a dormant owner's thread names no heap");
    for ticket in ["ticket-1", "ticket-2"] {
        let mut turn = state.begin((7, ticket)).ok().expect("own ticket");
        assert!(
            turn.with_world(owns_the_thread),
            "begin installs the owner's heap"
        );
        assert_eq!(turn.with_world(|ev| eval(ev, PROBE)), OWNER_ANSWER);
        assert_eq!(ended(turn), (7, ticket));
        assert!(
            names_no_heap(),
            "an ended turn leaves no heap on this thread"
        );
    }
}

#[test]
fn l64_begin_installs_the_heap_and_reattaches_frame_slab_buffer_and_mirrors() {
    let mut ev = owner();
    eval(&mut ev, OWNER_STATE);
    let buffer = ev.edit.buffers.current_buffer_id();
    let buffer_name = eval(&mut ev, "(buffer-name)");
    // A batch owner has not run window-init. `selected-frame` is the display
    // layer, so this frame manager is empty. Reattach keeps that. It does not
    // synthesize a frame the owner never had.
    let frames = ev.frame_manager();
    let frame = frames.selected;
    let frame_count = frames.frames.len();
    let slab = ev.ensure_owner_slab() as *const _ as usize;
    assert!(buffer.is_some(), "the owner has a current buffer");
    assert!(frame.is_none(), "a batch owner has no selected frame");
    assert_eq!(frame_count, 0, "a batch owner has no frames");
    let mut state = OwnerSerialState::new(7, Box::new(ev))
        .ok()
        .expect("a freshly built Context detaches");
    assert!(
        names_no_heap(),
        "the loan's later install has not happened yet"
    );
    let mut turn = state.begin((7, "ticket")).ok().expect("own ticket");
    assert!(
        turn.with_world(owns_the_thread),
        "(b) begin installs this owner's heap"
    );
    turn.with_world(|ev| {
        assert_eq!(
            ev.edit.buffers.current_buffer_id(),
            buffer,
            "(c) the owner's buffer is back"
        );
        assert_eq!(
            eval(ev, "(buffer-name)"),
            buffer_name,
            "(c) the owner's buffer is the one from before the detach"
        );
        assert_eq!(
            ev.frame_manager().selected,
            frame,
            "(c) the owner's frame selection is unchanged"
        );
        assert_eq!(
            ev.frame_manager().frames.len(),
            frame_count,
            "(c) reattach does not add or drop a frame"
        );
        assert_eq!(
            ev.owner.slab().map(|slab| slab as *const _ as usize),
            Some(slab),
            "(c) the owner's slab is back"
        );
    });
    assert_eq!(
        turn.with_world(|ev| eval(ev, PROBE)),
        OWNER_ANSWER,
        "(d) the mirrors show this owner again"
    );
    assert_eq!(ended(turn), (7, "ticket"));
    assert!(names_no_heap(), "end takes the installed heap back down");
}

#[test]
fn l64_a_second_owner_cannot_begin_while_another_owners_turn_is_live() {
    let mut first = dormant(7);
    let mut second = dormant(8);
    let mut turn = first.begin((7, "ticket-1")).ok().expect("own ticket");
    let refused = second.begin((8, "ticket-2")).err().expect("heap installed");
    assert_eq!(
        refused,
        (
            (8, "ticket-2"),
            TurnRefusal::Host(HostRefusal::HeapInstalled)
        ),
        "the second owner's ticket comes back and its Context stays dormant"
    );
    assert!(
        turn.with_world(owns_the_thread),
        "the refusal left the live turn's TLS alone"
    );
    assert_eq!(turn.with_world(|ev| eval(ev, PROBE)), OWNER_ANSWER);
    assert_eq!(ended(turn), (7, "ticket-1"));
    let mut turn = second.begin((8, "ticket-2")).ok().expect("thread is free");
    assert!(
        turn.with_world(owns_the_thread),
        "the second owner's own heap"
    );
    assert_eq!(turn.with_world(|ev| eval(ev, PROBE)), OWNER_ANSWER);
    assert_eq!(ended(turn), (8, "ticket-2"));
}

#[test]
fn l64_dropping_a_dormant_owner_is_ordinary_teardown() {
    let state = dormant(7);
    drop(state);
    assert!(
        names_no_heap(),
        "teardown of a dormant owner leaves no heap on this thread"
    );
    let mut next = dormant(8);
    let mut turn = next.begin((8, "ticket-1")).ok().expect("own ticket");
    assert_eq!(turn.with_world(|ev| eval(ev, PROBE)), OWNER_ANSWER);
    assert_eq!(ended(turn), (8, "ticket-1"));
}

#[test]
fn l64_detach_refuses_a_context_whose_heap_is_not_installed() {
    let mut first = owner();
    let first_proof = detach(&mut first);
    let mut second = owner();
    assert!(
        owns_the_thread(&mut second),
        "the last Context built is installed"
    );
    match first.detach_serial_owner() {
        Ok(_) => panic!("the first Context's heap is not the installed one"),
        Err(refusal) => assert_eq!(refusal, HostRefusal::NotInstalled),
    }
    assert!(
        owns_the_thread(&mut second),
        "the refusal left the installed Context's TLS alone"
    );
    assert_eq!(eval(&mut second, "(+ 1 2)"), "3");
    drop(second);
    reattach(&mut first, first_proof);
}

#[test]
fn l64_a_turn_whose_context_was_swapped_out_cannot_end() {
    // A Context that is not installed: built, then detached by hand.
    let mut stranger = owner();
    let stranger_proof = detach(&mut stranger);
    let mut state = dormant(7);
    let mut turn = state.begin((7, "ticket-1")).ok().expect("own ticket");
    // The body swaps the stranger in, which `with_world`'s `&mut` allows.
    turn.with_world(|ev| std::mem::swap(ev, &mut stranger));
    let Err((mut turn, refusal)) = turn.end() else {
        panic!("the swapped-in Context is not installed");
    };
    assert_eq!(refusal, HostRefusal::NotInstalled);
    assert!(
        owns_the_thread(&mut stranger),
        "the owner's Context, now outside the state, still has its TLS"
    );
    let Err((mut turn, wait, refusal)) = turn.park("mutex") else {
        panic!("park refuses the same way");
    };
    assert_eq!((wait, refusal), ("mutex", HostRefusal::NotInstalled));
    // Swapping back lets the turn end.
    turn.with_world(|ev| std::mem::swap(ev, &mut stranger));
    assert_eq!(ended(turn), (7, "ticket-1"));
    assert!(names_no_heap());
    reattach(&mut stranger, stranger_proof);
}

/// A `HostTransition` arrives by value, and a host could forward it to
/// another host. `Context` is the only production host; the other two
/// implement nothing that can forward one.
#[test]
fn l64_context_is_the_only_production_serial_host() {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("readable source directory") {
            let path = entry.expect("directory entry").path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk(&root, &mut files);
    let mut impls: Vec<(String, String)> = Vec::new();
    for file in files {
        let source = std::fs::read_to_string(&file).expect("readable source");
        for line in source.lines() {
            let line = line.trim();
            if line.starts_with("impl") && line.contains("SerialHost for ") {
                let path = file.strip_prefix(&root).expect("under src");
                impls.push((path.display().to_string(), line.to_owned()));
            }
        }
    }
    impls.sort();
    let dir = "emacs_core/runtime/eval/parallel_entry";
    let expected = [
        (
            format!("{dir}/serial_detach.rs"),
            "impl SerialHost for Context {".to_owned(),
        ),
        (
            // Static assertions only; its methods discard the token.
            format!("{dir}/serial_turn.rs"),
            "impl SerialHost for AssertedHost {".to_owned(),
        ),
        (
            // cfg(test) stand-in.
            format!("{dir}/serial_turn.rs"),
            "impl SerialHost for Counted {".to_owned(),
        ),
    ];
    assert_eq!(impls, expected, "a new SerialHost impl needs review");
}

#[test]
fn l64_owner_serial_state_refuses_a_seated_green_context() {
    let mut ev =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    ev.start_green_threads_for_test().expect("seat main");
    assert!(ev.green_seat().is_some(), "main holds a Green seat");
    let Err((mut returned, refusal)) =
        OwnerSerialState::<Context, (u64, &'static str)>::new(7, Box::new(ev))
    else {
        panic!("F3: a seated Green Context must be refused at the dormant door");
    };
    assert_eq!(
        refusal,
        HostRefusal::Seated,
        "F3: detach_serial_owner returns HostRefusal::Seated before AbortOnPanic"
    );
    assert!(
        owns_the_thread(&mut returned),
        "refusal leaves the seated Context's heap installed"
    );
    assert_eq!(
        eval(&mut returned, "(+ 1 2)"),
        "3",
        "the handed-back seated Context remains usable"
    );
}

#[test]
fn l64_further_green_owner_detaches_without_corrupting_unparked_main() {
    let mut main =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    assert_eq!(eval(&mut main, "(lossage-size 417)"), "417");
    main.display_var_change_count = 77;
    let baton = main.start_green_runtime().expect("start the Green runtime");
    let (to_main, from_worker) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        baton.enter();
        let owner = Context::new_green_owner(&baton);
        let mut state: State = OwnerSerialState::new(7, Box::new(owner))
            .ok()
            .expect("a further Green owner detaches cleanly while main is unparked");
        let Err((ticket, refusal)) = state.begin((7, "ticket-1")) else {
            panic!("A-1: begin must refuse while main holds the World carriers");
        };
        assert_eq!(ticket, (7, "ticket-1"));
        assert_eq!(refusal, TurnRefusal::Host(HostRefusal::WorldHeldElsewhere));
        assert!(names_no_heap(), "refusal keeps the worker thread detached");
        drop(state);
        to_main.send(baton).expect("return baton to main");
    });
    let baton = from_worker.recv().expect("baton returns from worker");
    worker.join().expect("worker thread finishes cleanly");
    baton.enter();
    main.seat_green_main_for_test(baton);
    main.green_park_runnable_for_test()
        .expect("main parks and resumes without two-places abort");
    assert_eq!(
        eval(&mut main, "(lossage-size)"),
        "417",
        "main's non-default carried lossage-size survived the further owner's detach"
    );
    assert_eq!(
        main.display_var_change_count, 77,
        "main's warm redisplay counter survived the further owner's detach"
    );
}

#[test]
fn l64_further_green_owner_acquires_stowed_world_carriers_on_begin() {
    let mut main =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    assert_eq!(eval(&mut main, "(lossage-size 417)"), "417");
    main.display_var_change_count = 77;
    let baton = main.start_green_runtime().expect("start the Green runtime");
    let main_detached = detach(&mut main);
    let (to_main, from_worker) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        baton.enter();
        let owner = Context::new_green_owner(&baton);
        let mut state: State = OwnerSerialState::new(7, Box::new(owner))
            .ok()
            .expect("a further Green owner detaches cleanly while world carriers are stowed");
        let mut turn = state.begin((7, "ticket-1")).ok().expect("own ticket");
        turn.with_world(|ev| {
            assert_eq!(
                eval(ev, "(lossage-size)"),
                "417",
                "begin acquired the stowed non-default carried lossage-size"
            );
            assert_eq!(
                ev.display_var_change_count, 77,
                "begin acquired the stowed warm display_var_change_count"
            );
            assert_eq!(eval(ev, "(lossage-size 523)"), "523");
            ev.display_var_change_count = 88;
        });
        assert_eq!(ended(turn), (7, "ticket-1"));
        drop(state);
        to_main.send(baton).expect("return baton to main");
    });
    let baton = from_worker.recv().expect("baton returns from worker");
    worker.join().expect("worker thread finishes cleanly");
    baton.enter();
    reattach(&mut main, main_detached);
    assert_eq!(
        eval(&mut main, "(lossage-size)"),
        "523",
        "main reattached the worker's updated carried lossage-size"
    );
    assert_eq!(
        main.display_var_change_count, 88,
        "main reattached the worker's updated warm display_var_change_count"
    );
    main.seat_green_main_for_test(baton);
    main.green_park_runnable_for_test()
        .expect("main parks and resumes cleanly after serial handoff");
    assert_eq!(eval(&mut main, "(lossage-size)"), "523");
    assert_eq!(main.display_var_change_count, 88);
}

#[test]
fn l64_reattach_unstows_world_carriers_before_resume_owner_swap_in() {
    let mut ev =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    let _baton = ev.start_green_runtime().expect("start the Green runtime");
    let id = intern("tab-width");
    assert!(
        ev.binds_in_place(id),
        "tab-width is a forwarded buffer-obj display variable"
    );
    ev.display_var_change_count = 40;
    let count = ev.specpdl.len();
    ev.try_specbind(id, Value::fixnum(4))
        .expect("bind default tab-width");
    let before_detach = ev.display_var_change_count;
    assert!(
        before_detach > 40,
        "specbind bumped display_var_change_count from 40"
    );
    let detached = detach(&mut ev);
    reattach(&mut ev, detached);
    assert_eq!(
        ev.display_var_change_count,
        before_detach + 2,
        "F2: detach swap-out (+1) and reattach swap-in (+1) must both survive warm restore"
    );
    ev.unbind_to(count);
}

#[test]
fn l64_begin_refuses_to_run_against_placeholders_when_world_is_held_elsewhere() {
    let mut main =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    assert_eq!(eval(&mut main, "(lossage-size 417)"), "417");
    main.display_var_change_count = 77;
    let baton = main.start_green_runtime().expect("start the Green runtime");
    let (to_worker, worker_rx) = std::sync::mpsc::channel::<super::super::super::world::Baton>();
    let (to_main, from_worker) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        // Phase 1: main has not stowed the World carriers -> begin must refuse
        // with WorldHeldElsewhere rather than run against placeholders.
        baton.enter();
        let owner = Context::new_green_owner(&baton);
        let mut state: State = OwnerSerialState::new(7, Box::new(owner))
            .ok()
            .expect("further owner detaches cleanly while main holds the World");
        let Err((ticket, refusal)) = state.begin((7, "ticket-1")) else {
            panic!("A-1: begin must refuse rather than run against placeholders");
        };
        assert_eq!(ticket, (7, "ticket-1"));
        assert_eq!(
            refusal,
            TurnRefusal::Host(HostRefusal::WorldHeldElsewhere),
            "A-1: refusal names WorldHeldElsewhere before AbortOnPanic"
        );
        assert!(names_no_heap(), "refusal keeps worker TLS down");
        to_main.send(baton).expect("return baton so main can stow");

        // Phase 2: once main stows the World carriers, the same dormant state
        // begins cleanly and observes the World's real carried and warm state.
        let baton = worker_rx.recv().expect("baton returns after main stowed");
        baton.enter();
        let mut turn = state
            .begin((7, "ticket-2"))
            .ok()
            .expect("once stowed, begin acquires the real World carriers");
        turn.with_world(|ev| {
            assert_eq!(
                eval(ev, "(lossage-size)"),
                "417",
                "turn sees the World's real carried value, not the 300 placeholder"
            );
            assert_eq!(
                ev.display_var_change_count, 77,
                "turn sees the World's real warm value, not 0"
            );
            assert_eq!(eval(ev, "(lossage-size 529)"), "529");
            ev.display_var_change_count = 91;
        });
        assert_eq!(ended(turn), (7, "ticket-2"));
        assert!(names_no_heap(), "end detaches worker TLS again");
        drop(state);
        to_main.send(baton).expect("return baton to main");
    });

    let baton = from_worker.recv().expect("baton returns from phase 1");
    baton.enter();
    let main_detached = detach(&mut main);
    to_worker
        .send(baton)
        .expect("send baton back after stowing");

    let baton = from_worker.recv().expect("baton returns from phase 2");
    worker.join().expect("worker finishes cleanly");
    baton.enter();
    reattach(&mut main, main_detached);
    assert_eq!(eval(&mut main, "(lossage-size)"), "529");
    assert_eq!(main.display_var_change_count, 91);
    main.seat_green_main_for_test(baton);
    main.green_park_runnable_for_test()
        .expect("main parks and resumes cleanly");
    assert_eq!(eval(&mut main, "(lossage-size)"), "529");
    assert_eq!(main.display_var_change_count, 91);
}
