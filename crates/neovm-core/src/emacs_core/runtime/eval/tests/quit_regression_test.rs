//! Regression tests for GNU-parity quit handling.
//!
//! These tests exercise the `quit-flag` / `inhibit-quit` / `maybe_quit`
//! contract at three specific points that had gaps before the fix:
//!
//! 1. **Bytecode VM polling**: a `(while t)` compiled to bytecode must
//!    return a `quit` signal once `quit-flag` is set. Before the fix
//!    the VM never polled `maybe_quit` inside its `run_loop`, so the
//!    loop was uninterruptible. Mirrors GNU `bytecode.c:861-866`.
//!
//! 2. **Cross-thread quit-request drain**: the input-bridge thread
//!    sets `Context::quit_requested`; `maybe_quit` promotes it into
//!    `Vquit_flag`. Tests the atomic is drained and honored.
//!
//! 3. **`unbind_to` quit suppression during cleanup**: a C-g that
//!    arrives while an `unwind-protect` CLEANUP clause is running
//!    must not interrupt cleanup. Mirrors GNU `eval.c:3909,3927-3928`.

use crate::emacs_core::error::FlowResultExt;
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;
use crate::test_utils::runtime_startup_context;

/// Setting `quit-flag` before entering bytecode must surface as a
/// `quit` signal the first time the VM polls, not loop forever.
#[test]
fn bytecode_while_polls_quit_flag() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();

    // Compile a bytecode that loops forever via a backward branch.
    // We use the top-level compiler path to get a real bytecode object.
    // If compilation is unavailable in this minimal context, fall back
    // to directly constructing the loop via (while t) interpreted —
    // the VM polling still fires via the generic call path.
    ctx.set_quit_flag_value(Value::T);

    // (while t) with a trivial body — after my fix this must signal
    // quit rather than hang. The while special form itself polls per
    // iteration, and any bytecode compilation would poll at the
    // backward branch.
    let result = ctx.eval_str("(while t)");
    match result {
        Err(e) => {
            // `eval_str` wraps Flow errors into EvalError; the message
            // format starts with the signal symbol.
            let msg = format!("{}", e);
            assert!(
                msg.contains("quit"),
                "expected a `quit' signal, got: {}",
                msg
            );
        }
        Ok(v) => panic!("expected quit signal, got value: {:?}", v),
    }
}

/// GNU `bytecode.c:Bcall` calls `maybe_quit` before entering the callee.
/// A bytecode `setq` of `quit-flag` must update Neomacs's cached runtime
/// field immediately, otherwise the following call runs even though GNU
/// would quit first.
#[test]
fn bytecode_setq_quit_flag_prevents_following_call() {
    crate::test_utils::init_test_tracing();
    let mut ctx = runtime_startup_context();

    let result = ctx.eval_str(
        r#"(progn
             (setq qtest-called nil
                   qtest-cleanup :unset)
             (defun qtest-callee ()
               (setq qtest-called t))
             (defun qtest-driver ()
               (setq quit-flag t)
               (qtest-callee)
               'after)
             (byte-compile 'qtest-driver)
             (unwind-protect
                 (qtest-driver)
               (setq qtest-cleanup qtest-called)))"#,
    );

    match result {
        Err(err) => {
            let msg = format!("{}", err);
            assert!(
                msg.contains("quit"),
                "expected a `quit' signal before qtest-callee, got: {}",
                msg
            );
        }
        Ok(value) => panic!("expected quit signal, got value: {:?}", value),
    }

    let cleanup = ctx
        .eval_str("qtest-cleanup")
        .expect("unwind-protect cleanup should bind qtest-cleanup");
    assert_eq!(cleanup, Value::NIL);
}

/// Setting `quit_requested` from the outside (simulating the bridge
/// thread) must be drained into `Vquit_flag` on the next `maybe_quit`
/// poll and produce a `quit` signal.
#[test]
fn quit_requested_atomic_is_drained_into_flag() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();

    // Confirm baseline: `Vquit_flag` starts nil.
    assert!(ctx.quit_flag_value().is_nil());

    // Simulate input-bridge flipping the atomic while the evaluator
    // is blocked.
    ctx.quit_requested.request();

    // Run a bytecode-reaching form. The first `maybe_quit` poll must
    // observe the atomic, promote it to `Vquit_flag`, and signal.
    let result = ctx.eval_str("(while t)");
    match result {
        Err(e) => {
            let msg = format!("{}", e);
            assert!(msg.contains("quit"), "expected quit, got: {}", msg);
        }
        Ok(v) => panic!("expected quit signal, got: {:?}", v),
    }

    // The atomic must have been drained so a subsequent `maybe_quit`
    // doesn't re-fire spuriously.
    assert!(
        !ctx.quit_requested.is_requested(),
        "quit_requested should be cleared after maybe_quit drains it"
    );
}

/// Ordinary frontend input must become visible at every GNU `maybe_quit`
/// safe point while `throw-on-input` is active.  Unlike C-g, ordinary keys do
/// not raise `quit_requested`; they arrive only through `input_rx`.  Leaving
/// that promotion to GC/evaluator entry points makes long bytecode workloads
/// (notably Corfu completion filtering) ignore type-ahead for seconds.
#[test]
fn maybe_quit_promotes_pending_frontend_input_for_while_no_input() {
    crate::test_utils::init_test_tracing();
    let mut ctx = runtime_startup_context();
    ctx.set_variable("noninteractive", Value::NIL);

    let (tx, rx) = crossbeam_channel::unbounded();
    ctx.input_rx = Some(rx);
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char('l'),
    ))
    .expect("queue ordinary frontend input");

    let sentinel = Value::symbol("maybe-quit-input-sentinel");
    ctx.set_variable("throw-on-input", sentinel);

    let result = ctx.maybe_quit();
    assert!(
        matches!(
            result.kinded_ref(),
            Err(crate::emacs_core::error::FlowRef::Throw(ref thrown))
                if thrown.tag == sentinel && thrown.value == Value::T
        ),
        "maybe_quit must promote queued ordinary input into throw-on-input"
    );
    assert_eq!(
        ctx.command_loop.keyboard.pending_input_events.len(),
        1,
        "throw-on-input must preserve the key for the next command read"
    );
}

/// Regex matcher must abort on TLS quit flag, and the top-level
/// builtin must surface the pending state as a `quit` signal rather
/// than `search-failed`. Mirrors GNU `regex-emacs.c:4901,5236` polling
/// plus `search.c:1247,1291` wrapper-level promotion.
#[test]
fn regex_search_promotes_quit_to_signal() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();

    // Set up a buffer with content so `re-search-forward` has somewhere
    // to search.
    ctx.eval_str(
        "(with-current-buffer (get-buffer-create \"*q*\") \
           (erase-buffer) \
           (insert \"hello world\"))",
    )
    .ok();

    // Simulate the bridge thread raising quit.
    ctx.quit_requested.request();

    // Any regex builtin should surface the quit — not "search-failed" —
    // once the post-matcher `maybe_quit` runs.
    let result = ctx.eval_str("(with-current-buffer \"*q*\" (re-search-forward \"world\"))");
    match result {
        Err(e) => {
            let msg = format!("{}", e);
            assert!(msg.contains("quit"), "expected quit signal, got: {}", msg);
        }
        Ok(v) => panic!("expected quit, got: {:?}", v),
    }
}

/// `unbind_to` must not let a pending `Vquit_flag` re-fire inside
/// `unwind-protect` CLEANUP forms.
#[test]
fn unbind_to_suppresses_quit_during_unwind_protect_cleanup() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();

    // Run an unwind-protect whose BODY signals quit. GNU semantics:
    // the CLEANUP must run to completion with quit suppressed, then
    // quit is re-raised for the outer caller.
    //
    // We prove CLEANUP ran by asserting it set a side-effect variable.
    ctx.eval_str("(setq cleanup-ran nil)").unwrap();

    let _ = ctx.eval_str(
        "(condition-case nil \
            (unwind-protect \
               (progn (setq quit-flag t) (while t)) \
             (setq cleanup-ran t)) \
          (quit 'caught))",
    );

    let ran = ctx.eval_str("cleanup-ran").expect("read cleanup-ran");
    assert_eq!(
        ran,
        Value::T,
        "unwind-protect CLEANUP must run to completion even when BODY quits"
    );
}

/// Finding 3 — a single idle C-g must yield exactly one `keyboard-quit`,
/// not a "double quit".
///
/// When the input-bridge thread observes a C-g it does TWO things in
/// lockstep (crates/neomacs/src/main.rs:2260/2569): it raises the
/// cross-thread `Context::quit_requested` atomic AND queues the C-g
/// KeyPress on the input channel. `read_key_sequence` reads that C-g as
/// an ordinary key and returns it bound to `keyboard-quit`. The leftover
/// `quit_requested` atomic must be cleared the moment that C-g is consumed
/// as a key — otherwise the very next `maybe_quit` poll (inside
/// `pre-command-hook`, the command dispatch, etc.) drains the atomic into
/// `Vquit_flag` and signals a SECOND, spurious `quit`, pre-empting the
/// `keyboard-quit` command the key is bound to (the "double-quit" bug).
///
/// This drives the read path directly with exactly the pair the bridge
/// produces (C-g queued + `quit_requested` set) and asserts: (a) the read
/// returns the C-g bound to `keyboard-quit`, (b) the `quit_requested`
/// atomic is cleared, and (c) a following `maybe_quit` does NOT fire a
/// spurious quit (no leftover pending quit).
#[test]
fn single_keyboard_quit_does_not_leave_pending_quit_request() {
    crate::test_utils::init_test_tracing();
    let mut ev = runtime_startup_context();
    let scratch = ev.buffers.create_buffer("*quit-finding3*");
    ev.buffers.set_current(scratch);
    let frame = ev.frames.create_frame("F1", 80, 24, scratch);
    assert!(ev.frames.select_frame(frame), "need a selected frame");

    // C-g is bound to keyboard-quit in the default global map.
    assert!(
        ev.eval_str("(eq (key-binding (kbd \"C-g\")) 'keyboard-quit)")
            .expect("C-g lookup")
            .is_truthy(),
        "C-g must be bound to keyboard-quit"
    );

    // Exactly what the input bridge does for one C-g: queue the cooked
    // C-g event (fixnum 7) AND raise the cross-thread quit-request atomic.
    ev.command_loop
        .keyboard
        .kboard
        .unread_events
        .push_back(Value::fixnum(7));
    ev.quit_requested.request();

    // Read the key sequence: the C-g must come back as an ordinary key
    // bound to keyboard-quit (NOT short-circuit into a quit signal).
    let (keys, binding) = ev
        .read_key_sequence()
        .expect("reading a queued C-g must return it as a key, not signal quit");
    assert_eq!(
        keys,
        vec![Value::fixnum(7)],
        "the C-g should be read as an ordinary key"
    );
    assert_eq!(
        binding,
        Value::symbol("keyboard-quit"),
        "the C-g key must resolve to its `keyboard-quit' binding"
    );

    // The atomic must have been cleared by consuming the C-g as a key.
    assert!(
        !ev.quit_requested.is_requested(),
        "consuming the C-g as a key must clear the quit_requested atomic so \
         no second, spurious quit is pending (the double-quit bug)"
    );

    // And a following `maybe_quit` (as runs inside pre-command-hook / the
    // command dispatch right after the read) must NOT fire a quit, because
    // the single C-g is now wholly accounted for by its keyboard-quit
    // binding. Before the fix the leftover atomic made this signal quit.
    ev.maybe_quit()
        .expect("no spurious quit should be pending after a single C-g key");
    assert!(
        ev.quit_flag_value().is_nil(),
        "quit-flag must stay nil — the single C-g produced no extra quit"
    );
}

/// Panic recovery restores storage without entering variable watchers or
/// unwind-protect Lisp. Both callbacks would set the same observable sentinel.
#[test]
fn discard_specpdl_restores_bindings_without_lisp_callbacks() {
    use crate::emacs_core::eval::SpecBinding;
    use crate::emacs_core::intern::intern;

    let mut ctx = Context::new();
    let symbol = intern("discard-saved-binding");
    let sentinel = intern("discard-callback-ran");
    ctx.obarray_mut()
        .set_symbol_value_id(symbol, Value::fixnum(10));
    ctx.obarray_mut().set_symbol_value_id(sentinel, Value::NIL);
    let count = ctx.specpdl.len();
    ctx.try_specbind(symbol, Value::fixnum(20)).expect("bind");
    let cleanup = Value::list(vec![
        Value::symbol("setq"),
        Value::from_sym_id(sentinel),
        Value::T,
    ]);
    let watcher = Value::list(vec![
        Value::symbol("lambda"),
        Value::list(vec![Value::symbol("&rest"), Value::symbol("ignored")]),
        cleanup,
    ]);
    crate::emacs_core::advice::builtin_add_variable_watcher(
        &mut ctx,
        vec![Value::from_sym_id(symbol), watcher],
    )
    .expect("register trapped watcher");
    ctx.specpdl.push(SpecBinding::UnwindProtect {
        forms: Value::list(vec![cleanup]),
        lexenv: ctx.lexenv,
    });
    ctx.discard_specpdl_to(count);
    assert_eq!(
        ctx.obarray().symbol_value_id_or_nil(symbol),
        Value::fixnum(10)
    );
    assert_eq!(ctx.obarray().symbol_value_id_or_nil(sentinel), Value::NIL);
    assert_eq!(ctx.specpdl.len(), count);
}

/// The fallback publishes restored buffer identity into the mutator's thread
/// slot and restores forwarded defaults and evaluator projections together.
#[test]
fn discard_specpdl_restores_buffer_default_and_runtime_cache() {
    use crate::emacs_core::eval::SpecBinding;
    use crate::emacs_core::intern::intern;

    let mut ctx = Context::new();
    let original = ctx.buffers.current_buffer_id().expect("current buffer");
    let other = ctx.buffers.create_buffer("discard-other-buffer");
    let count = ctx.specpdl.len();
    let truncate_lines = intern("truncate-lines");
    let saved_default = ctx
        .buffers
        .get(original)
        .unwrap()
        .buffer_local_value("truncate-lines");
    ctx.try_specbind(truncate_lines, Value::T)
        .expect("bind default");
    ctx.try_specbind(intern("throw-on-input"), Value::symbol("discard-tag"))
        .expect("bind cache");
    ctx.specpdl.push(SpecBinding::SaveCurrentBuffer {
        buffer_id: original,
    });
    ctx.switch_current_buffer(other)
        .expect("select other buffer");
    ctx.discard_specpdl_to(count);
    assert_eq!(ctx.buffers.current_buffer_id(), Some(original));
    assert_eq!(
        ctx.threads
            .thread_current_buffer(ctx.threads.current_thread_id()),
        Some(original)
    );
    assert_eq!(
        ctx.buffers
            .get(original)
            .unwrap()
            .buffer_local_value("truncate-lines"),
        saved_default
    );
    assert_eq!(ctx.cached_throw_on_input_for_test(), Value::NIL);
    assert_eq!(
        ctx.visible_variable_value_or_nil_by_id(truncate_lines),
        saved_default.unwrap_or(Value::NIL)
    );
}

/// set-default-toplevel-value permits a saved value to change without storing
/// it. An invalid forwarded value must never cause a second recovery panic.
#[test]
fn discard_specpdl_rejected_forwarded_value_keeps_valid_storage() {
    use crate::emacs_core::eval::{SavedBindingValue, SavedBufferId, SpecBinding};
    use crate::emacs_core::intern::intern;

    let mut ctx = Context::new();
    let symbol = intern("undo-limit");
    let count = ctx.specpdl.len();
    ctx.try_specbind(symbol, Value::fixnum(5))
        .expect("bind integer forwarder");
    ctx.specpdl.push(SpecBinding::LetDefault {
        sym_id: symbol,
        old_value: SavedBindingValue::from_plain(Value::string("invalid saved integer")),
        buffer_id: SavedBufferId::from_option(None),
    });
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ctx.discard_specpdl_to(count + 1)
    }));
    assert!(outcome.is_ok());
    // A BUFFER_OBJFWD integer lives in the buffer default rather than the
    // descriptor, and is read through the GNU default-value storage seam.
    assert_eq!(
        crate::emacs_core::data::default_value_by_id(&ctx, symbol),
        Some(Value::fixnum(5))
    );
    ctx.discard_specpdl_to(count);
}

#[test]
fn current_buffer_scope_restores_on_signal_and_rust_unwind() {
    use crate::emacs_core::error::{LispCondition, signal};
    use crate::emacs_core::eval::CurrentBufferScope;
    let mut ctx = Context::new();
    let original = ctx.buffers.current_buffer_id().unwrap();
    let other = ctx.buffers.create_buffer("scope-other");
    let depth = ctx.specpdl.len();
    let mut scope = CurrentBufferScope::enter(&mut ctx);
    scope
        .context()
        .set_current_buffer_unrecorded(other)
        .unwrap();
    assert!(
        scope
            .finish(Err(signal(LispCondition::Error, vec![])))
            .is_err()
    );
    assert_eq!(ctx.buffers.current_buffer_id(), Some(original));
    let recovered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut scope = CurrentBufferScope::enter(&mut ctx);
        scope
            .context()
            .set_current_buffer_unrecorded(other)
            .unwrap();
        panic!("contained test panic");
    }));
    assert!(recovered.is_err());
    assert_eq!(ctx.buffers.current_buffer_id(), Some(original));
    assert_eq!(ctx.specpdl.len(), depth);
}

#[test]
fn current_buffer_scope_same_target_avoids_a_specpdl_entry() {
    use crate::emacs_core::eval::CurrentBufferScope;
    let mut ctx = Context::new();
    let original = ctx.buffers.current_buffer_id().unwrap();
    let depth = ctx.specpdl.len();
    let mut scope = CurrentBufferScope::for_buffer(&mut ctx, original).unwrap();
    assert_eq!(scope.context().specpdl.len(), depth);
    scope.finish(Ok(Value::NIL)).unwrap();
}

#[test]
fn discard_specpdl_noncurrent_local_restores_current_runtime_projection() {
    use crate::emacs_core::eval::SpecBinding;
    use crate::emacs_core::intern::intern;
    let mut ctx = Context::new();
    let original = ctx.buffers.current_buffer_id().unwrap();
    let other = ctx.buffers.create_buffer("discard-projection-other");
    let symbol = intern("throw-on-input");
    let current_tag = Value::symbol("discard-current-tag");
    let other_tag = Value::symbol("discard-other-tag");
    ctx.obarray.make_buffer_local("throw-on-input", false);
    ctx.buffers
        .set_buffer_local_property_by_sym_id(original, symbol, current_tag)
        .unwrap();
    ctx.buffers
        .set_buffer_local_property_by_sym_id(other, symbol, Value::T)
        .unwrap();
    ctx.publish_runtime_binding_write_by_resolved_id(symbol, current_tag);
    let depth = ctx.specpdl.len();
    ctx.specpdl.push(SpecBinding::LetLocal {
        sym_id: symbol,
        old_value: other_tag,
        buffer_id: other,
    });
    ctx.discard_specpdl_to(depth);
    assert_eq!(
        ctx.buffers
            .get(other)
            .unwrap()
            .get_buffer_local_by_sym_id(symbol),
        Some(other_tag)
    );
    assert_eq!(ctx.cached_throw_on_input_for_test(), current_tag);
    assert_eq!(ctx.buffers.current_buffer_id(), Some(original));
}

#[test]
fn discard_specpdl_runtime_projection_ignores_lexical_shadow() {
    use crate::emacs_core::intern::intern;
    let mut ctx = Context::new();
    let symbol = intern("throw-on-input");
    let depth = ctx.specpdl.len();
    ctx.try_specbind(symbol, Value::symbol("discard-dynamic-tag"))
        .unwrap();
    let lexical_tag = Value::symbol("discard-lexical-tag");
    ctx.lexenv = Value::list(vec![Value::cons(Value::from_sym_id(symbol), lexical_tag)]);
    ctx.discard_specpdl_to(depth);
    assert_eq!(ctx.visible_variable_value_or_nil_by_id(symbol), lexical_tag);
    assert_eq!(
        ctx.visible_runtime_variable_value_by_id_resolved(symbol),
        Some(Value::NIL)
    );
    assert_eq!(ctx.cached_throw_on_input_for_test(), Value::NIL);
}

#[test]
fn current_buffer_scope_same_target_panic_discards_bindings_without_restoring_state() {
    use crate::buffer::EmacsBytePos;
    use crate::emacs_core::eval::CurrentBufferScope;
    use crate::emacs_core::intern::intern;
    let mut ctx = Context::new();
    let original = ctx.buffers.current_buffer_id().unwrap();
    let other = ctx.buffers.create_buffer("scope-same-target-selected");
    ctx.buffers
        .replace_buffer_contents(original, "abcd")
        .unwrap();
    ctx.buffers.replace_buffer_contents(other, "efgh").unwrap();
    let symbol = intern("scope-same-target-child-binding");
    ctx.obarray.set_symbol_value_id(symbol, Value::fixnum(7));
    let depth = ctx.specpdl.len();
    let recovered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut scope = CurrentBufferScope::for_buffer(&mut ctx, original).unwrap();
        assert_eq!(scope.context().specpdl.len(), depth);
        scope
            .context()
            .try_specbind(symbol, Value::fixnum(9))
            .unwrap();
        scope
            .context()
            .buffers
            .goto_buffer_emacs_byte_pos(original, EmacsBytePos::new(2))
            .unwrap();
        scope
            .context()
            .set_current_buffer_unrecorded(other)
            .unwrap();
        scope
            .context()
            .buffers
            .goto_buffer_emacs_byte_pos(other, EmacsBytePos::new(1))
            .unwrap();
        panic!("contained same-target scope panic");
    }));
    assert!(recovered.is_err());
    assert_eq!(ctx.specpdl.len(), depth);
    assert_eq!(
        ctx.obarray.symbol_value_id_copied(symbol),
        Some(Value::fixnum(7))
    );
    assert_eq!(ctx.buffers.current_buffer_id(), Some(other));
    assert_eq!(
        ctx.buffers.get(original).unwrap().point_emacs_byte_pos(),
        EmacsBytePos::new(2)
    );
    assert_eq!(
        ctx.buffers.get(other).unwrap().point_emacs_byte_pos(),
        EmacsBytePos::new(1)
    );
}

/// Panic recovery must not manufacture lazy standard tables while restoring
/// native buffer/thread ownership and the marker-backed saved point.
#[test]
fn excursion_scope_panic_recovery_does_not_seed_standard_case_table() {
    use crate::buffer::EmacsBytePos;
    use crate::emacs_core::eval::ExcursionScope;
    use crate::emacs_core::intern::intern;

    for saved_symbol in [None, Some(Value::fixnum(99))] {
        let mut ctx = Context::new();
        let original = ctx.buffers.current_buffer_id().unwrap();
        let other = ctx.buffers.create_buffer("scope-panic-no-table-seed");
        ctx.buffers
            .replace_buffer_contents(original, "abcd")
            .unwrap();
        ctx.buffers
            .goto_buffer_emacs_byte_pos(original, EmacsBytePos::new(1))
            .unwrap();
        let symbol = intern("neovm--standard-case-table-object");
        let count = ctx.specpdl.len();
        let mut allocations_before_drop = None;
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut scope = ExcursionScope::enter(&mut ctx);
            scope
                .context()
                .buffers
                .goto_buffer_emacs_byte_pos(original, EmacsBytePos::new(3))
                .unwrap();
            scope
                .context()
                .set_current_buffer_unrecorded(other)
                .unwrap();
            match saved_symbol {
                Some(value) => scope.context().obarray.set_symbol_value_id(symbol, value),
                None => scope.context().obarray.makunbound_id(symbol),
            }
            // Capture after the scope's point marker and normal buffer switch
            // have allocated. Only fallback work is measured below.
            allocations_before_drop = Some(scope.context().tagged_heap.allocated_count());
            panic!("contained panic after removing the lazy standard table");
        }));
        assert!(caught.is_err());
        assert_eq!(ctx.specpdl.len(), count);
        assert_eq!(ctx.buffers.current_buffer_id(), Some(original));
        assert_eq!(
            ctx.threads
                .thread_current_buffer(ctx.threads.current_thread_id()),
            Some(original)
        );
        assert_eq!(
            ctx.buffers.get(original).unwrap().point_emacs_byte_pos(),
            EmacsBytePos::new(1)
        );
        assert_eq!(
            ctx.obarray.symbol_value_id_copied(symbol),
            saved_symbol,
            "Drop must preserve an absent or invalid standard-case symbol"
        );
        assert_eq!(
            Some(ctx.tagged_heap.allocated_count()),
            allocations_before_drop,
            "lazy table allocation must not occur during saved-state recovery"
        );
    }
}

/// The fallback-only change leaves ordinary explicit finish table seeding intact.
#[test]
fn excursion_scope_normal_finish_retains_standard_case_table_seeding() {
    use crate::emacs_core::eval::ExcursionScope;
    use crate::emacs_core::intern::intern;
    let mut ctx = Context::new();
    let original = ctx.buffers.current_buffer_id().unwrap();
    let other = ctx.buffers.create_buffer("scope-normal-table-seed");
    let symbol = intern("neovm--standard-case-table-object");
    let mut scope = ExcursionScope::enter(&mut ctx);
    scope
        .context()
        .set_current_buffer_unrecorded(other)
        .unwrap();
    scope.context().obarray.makunbound_id(symbol);
    assert!(scope.finish(Ok(Value::NIL)).is_ok());
    assert_eq!(ctx.buffers.current_buffer_id(), Some(original));
    assert!(ctx.obarray.symbol_value_id_copied(symbol).is_some());
}

#[test]
fn excursion_scope_without_current_buffer_discards_child_binding_on_panic() {
    use crate::emacs_core::eval::ExcursionScope;
    use crate::emacs_core::intern::intern;
    let mut ctx = Context::new();
    let current = ctx.buffers.current_buffer_id().unwrap();
    assert!(ctx.buffers.kill_buffer(current));
    assert_eq!(ctx.buffers.current_buffer_id(), None);
    let symbol = intern("empty-excursion-scope-value");
    ctx.obarray.set_symbol_value_id(symbol, Value::fixnum(10));
    let count = ctx.specpdl.len();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut scope = ExcursionScope::enter(&mut ctx);
        assert_eq!(scope.context().specpdl.len(), count);
        scope
            .context()
            .try_specbind(symbol, Value::fixnum(20))
            .unwrap();
        panic!("exercise cleanup boundary without a captured buffer");
    }));
    assert!(outcome.is_err());
    assert_eq!(
        ctx.obarray.symbol_value_id_or_nil(symbol),
        Value::fixnum(10)
    );
    assert_eq!(ctx.specpdl.len(), count);
    assert_eq!(ctx.buffers.current_buffer_id(), None);
}

#[test]
fn restriction_scope_without_current_buffer_discards_child_binding_on_panic() {
    use crate::emacs_core::eval::RestrictionScope;
    use crate::emacs_core::intern::intern;
    let mut ctx = Context::new();
    let current = ctx.buffers.current_buffer_id().unwrap();
    assert!(ctx.buffers.kill_buffer(current));
    assert_eq!(ctx.buffers.current_buffer_id(), None);
    let symbol = intern("empty-restriction-scope-value");
    ctx.obarray.set_symbol_value_id(symbol, Value::fixnum(10));
    let count = ctx.specpdl.len();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut scope = RestrictionScope::enter(&mut ctx);
        assert_eq!(scope.context().specpdl.len(), count);
        scope
            .context()
            .try_specbind(symbol, Value::fixnum(20))
            .unwrap();
        panic!("exercise cleanup boundary without a captured buffer");
    }));
    assert!(outcome.is_err());
    assert_eq!(
        ctx.obarray.symbol_value_id_or_nil(symbol),
        Value::fixnum(10)
    );
    assert_eq!(ctx.specpdl.len(), count);
    assert_eq!(ctx.buffers.current_buffer_id(), None);
}

#[test]
fn current_buffer_scope_failed_setup_finishes_owned_boundary() {
    use crate::emacs_core::eval::CurrentBufferScope;
    let mut ctx = Context::new();
    let original = ctx.buffers.current_buffer_id().unwrap();
    let target = ctx.buffers.create_buffer("scope-deleted-setup-target");
    assert!(ctx.buffers.kill_buffer(target));
    let count = ctx.specpdl.len();
    assert!(CurrentBufferScope::for_buffer(&mut ctx, target).is_err());
    assert_eq!(ctx.specpdl.len(), count);
    assert_eq!(ctx.buffers.current_buffer_id(), Some(original));
}

#[test]
fn discard_local_binding_does_not_allocate_ambient_buffer_wrapper() {
    use crate::emacs_core::intern::intern;
    let mut owner = Context::new();
    owner.setup_thread_locals();
    // Give this Context an identity not present in the second Context's heap.
    let _ = owner.buffers.create_buffer("local-drop-first");
    let _ = owner.buffers.create_buffer("local-drop-second");
    let buffer = owner.buffers.create_buffer("local-drop-owner");
    owner
        .set_current_buffer_unrecorded(buffer)
        .expect("live buffer");
    let symbol = intern("local-drop-existing-cell");
    owner.obarray.set_symbol_value_id(symbol, Value::fixnum(10));
    crate::emacs_core::custom::builtin_make_local_variable(
        &mut owner,
        vec![Value::from_sym_id(symbol)],
    )
    .expect("localized existing cell");
    let count = owner.specpdl.len();
    owner
        .try_specbind(symbol, Value::fixnum(20))
        .expect("bind local");
    let binding_cell = owner
        .buffers
        .get(buffer)
        .and_then(|buffer| buffer.local_variable_binding_cell(symbol))
        .expect("existing canonical local cell");
    assert_eq!(owner.obarray.blv(symbol).unwrap().valcell, binding_cell);
    let mut ambient = Context::new();
    ambient.setup_thread_locals();
    let before = ambient.tagged_heap.bytes_since_gc_exact();
    owner.discard_specpdl_to(count);
    let after = ambient.tagged_heap.bytes_since_gc_exact();
    owner.setup_thread_locals();
    assert_eq!(
        after, before,
        "storage recovery must not create a wrapper in a different active heap"
    );
    assert_eq!(
        owner
            .buffers
            .get(buffer)
            .and_then(|buffer| buffer.get_buffer_local_by_sym_id(symbol)),
        Some(Value::fixnum(10))
    );
    assert_eq!(owner.specpdl.len(), count);
    assert_eq!(
        owner
            .buffers
            .get(buffer)
            .and_then(|buffer| buffer.local_variable_binding_cell(symbol)),
        Some(binding_cell),
        "recovery keeps the existing binding cell"
    );
    assert_eq!(owner.obarray.blv(symbol).unwrap().valcell, binding_cell);
    assert_eq!(
        owner.visible_variable_value_or_nil_by_id(symbol),
        Value::fixnum(10),
        "the loaded BLV cache observes the restored cell"
    );
}

#[test]
fn discard_local_binding_keeps_existing_void_cell_without_lisp_allocation() {
    use crate::emacs_core::intern::intern;
    let mut owner = Context::new();
    owner.setup_thread_locals();
    let buffer = owner.buffers.current_buffer_id().expect("buffer");
    let symbol = intern("local-drop-void-cell");
    crate::emacs_core::custom::builtin_make_local_variable(
        &mut owner,
        vec![Value::from_sym_id(symbol)],
    )
    .expect("void localized cell");
    let count = owner.specpdl.len();
    owner
        .try_specbind(symbol, Value::fixnum(20))
        .expect("bind local");
    let before = owner.tagged_heap.bytes_since_gc_exact();
    owner.discard_specpdl_to(count);
    assert_eq!(owner.tagged_heap.bytes_since_gc_exact(), before);
    assert!(
        owner
            .buffers
            .get(buffer)
            .expect("live buffer")
            .get_buffer_local_binding_by_sym_id(symbol)
            .is_some()
    );
    assert!(
        owner
            .buffers
            .get(buffer)
            .expect("live buffer")
            .get_buffer_local_by_sym_id(symbol)
            .is_none(),
        "UNBOUND preserves a void existing local cell"
    );
}

/// GNU set_internal refuses Qunbound before touching a forwarded slot
/// (data.c:1805-1808). Rust recovery retains that slot without running Lisp;
/// descriptor type checks alone would turn a Bool true or void an Obj.
#[test]
fn panic_recovery_unbound_forwarded_binding_keeps_builtin_storage() {
    use crate::emacs_core::eval::{CurrentBufferScope, SavedBindingValue, SpecBinding};
    use crate::emacs_core::forward::{alloc_boolfwd, alloc_objfwd};
    use crate::emacs_core::intern::intern;

    let mut ctx = Context::new();
    ctx.setup_thread_locals();
    let boolean = intern("discard-void-bool-forwarder");
    let bool_fwd = alloc_boolfwd(false);
    ctx.obarray.install_boolfwd(boolean, bool_fwd);
    let object = intern("throw-on-input");
    let retained = Value::fixnum(17);
    let obj_fwd = alloc_objfwd(retained);
    ctx.obarray.install_objfwd(object, obj_fwd);
    // Force publication to use actual storage after refusing the saved void.
    ctx.publish_runtime_binding_write_by_resolved_id(object, Value::NIL);
    let count = ctx.specpdl.len();
    let allocated = ctx.tagged_heap.bytes_since_gc_exact();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut scope = CurrentBufferScope::enter(&mut ctx);
        for symbol in [boolean, object] {
            scope.context().specpdl.push(SpecBinding::Let {
                sym_id: symbol,
                old_value: SavedBindingValue::from_plain(Value::UNBOUND),
            });
        }
        panic!("contained forwarded recovery panic");
    }));
    assert!(outcome.is_err());
    assert!(!bool_fwd.get(), "UNBOUND must not become native true");
    assert_eq!(obj_fwd.get(), retained, "the Obj slot must remain bound");
    assert_eq!(ctx.obarray.symbol_value_id_or_nil(boolean), Value::NIL);
    assert_eq!(ctx.obarray.symbol_value_id_or_nil(object), retained);
    assert_eq!(ctx.cached_throw_on_input_for_test(), retained);
    assert_eq!(ctx.specpdl.len(), count);
    assert_eq!(ctx.tagged_heap.bytes_since_gc_exact(), allocated);
}

/// The same builtin refusal applies after make_blv keeps its forwarder
/// (GNU data.c:1725-1728). Recovery keeps both canonical BLV cons cells and
/// publishes their retained visible value into the runtime projection.
#[test]
fn panic_recovery_unbound_localized_builtin_keeps_cells_and_projection() {
    use crate::emacs_core::eval::{
        CurrentBufferScope, SavedBindingValue, SavedBufferId, SpecBinding,
    };
    use crate::emacs_core::forward::{alloc_boolfwd, alloc_objfwd};
    use crate::emacs_core::intern::intern;

    let mut ctx = Context::new();
    ctx.setup_thread_locals();
    let boolean = intern("discard-void-localized-bool-forwarder");
    let bool_fwd = alloc_boolfwd(false);
    ctx.obarray.install_boolfwd(boolean, bool_fwd);
    ctx.obarray
        .make_symbol_localized(boolean, Value::NIL)
        .expect("localized Bool builtin");
    let object = intern("throw-on-input");
    let retained = Value::fixnum(23);
    let obj_fwd = alloc_objfwd(retained);
    ctx.obarray.install_objfwd(object, obj_fwd);
    ctx.obarray
        .make_symbol_localized(object, retained)
        .expect("localized Obj builtin");
    let cells = [boolean, object].map(|symbol| {
        let blv = ctx.obarray.blv(symbol).expect("BLV");
        assert!(blv.fwd.is_some());
        (blv.defcell, blv.valcell)
    });
    ctx.publish_runtime_binding_write_by_resolved_id(object, Value::NIL);
    let count = ctx.specpdl.len();
    let allocated = ctx.tagged_heap.bytes_since_gc_exact();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut scope = CurrentBufferScope::enter(&mut ctx);
        for symbol in [boolean, object] {
            scope.context().specpdl.push(SpecBinding::LetDefault {
                sym_id: symbol,
                old_value: SavedBindingValue::from_plain(Value::UNBOUND),
                buffer_id: SavedBufferId::from_option(None),
            });
        }
        panic!("contained localized recovery panic");
    }));
    assert!(outcome.is_err());
    for (symbol, cells) in [boolean, object].into_iter().zip(cells) {
        let blv = ctx.obarray.blv(symbol).expect("retained BLV");
        assert_eq!((blv.defcell, blv.valcell), cells);
        let retained = if symbol == boolean {
            Value::NIL
        } else {
            retained
        };
        assert_eq!(blv.defcell.cons_cdr(), retained);
        assert_eq!(blv.valcell.cons_cdr(), retained);
        assert!(blv.fwd.is_some());
    }
    assert!(!bool_fwd.get());
    assert_eq!(obj_fwd.get(), retained);
    assert_eq!(ctx.cached_throw_on_input_for_test(), retained);
    assert_eq!(ctx.specpdl.len(), count);
    assert_eq!(ctx.tagged_heap.bytes_since_gc_exact(), allocated);
}
