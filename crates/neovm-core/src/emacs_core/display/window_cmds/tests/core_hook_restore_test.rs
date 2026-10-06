//! Portable images stamped with legacy redisplay ownership must remain valid
//! when the GNU policy is selected in a later process. These tests use the
//! file-backed dump pipeline, and keep dumped Lisp contents authoritative.

use crate::emacs_core::data;
use crate::emacs_core::eval::RedisplayHookPolicyGuard;
use crate::emacs_core::format_eval_result;
use crate::emacs_core::forward::LispFwdType;
use crate::emacs_core::intern::intern;
use crate::emacs_core::pdump::{dump_to_file, load_from_dump};
use crate::emacs_core::symbol::SymbolRedirect;
use crate::emacs_core::{Context, Value};
use std::path::Path;

const HOOK: &str = "window-configuration-change-hook";

fn dump_legacy(path: &Path, setup: impl FnOnce(&mut Context)) {
    let _policy = RedisplayHookPolicyGuard::legacy();
    let mut eval = Context::new();
    setup(&mut eval);
    dump_to_file(&eval, path).expect("write legacy-policy portable image");
    // Drop the original Context before load installs the restored active heap.
}

#[test]
fn file_restore_initializes_only_missing_gnu_configuration_hook() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("portable-image fixture directory");
    let path = dir.path().join("missing-configuration-hook.pdump");
    let symbol = intern(HOOK);
    dump_legacy(&path, |eval| {
        assert!(
            eval.obarray().get_by_id(symbol).is_none(),
            "legacy fixture must omit the symbol slot, rather than install an intentional unbound cell"
        );
        assert_eq!(data::default_value_by_id(eval, symbol), None);
    });
    {
        let _policy = RedisplayHookPolicyGuard::legacy();
        let loaded = load_from_dump(&path).expect("restore legacy projection");
        assert!(loaded.obarray().get_by_id(symbol).is_none());
        assert_eq!(data::default_value_by_id(&loaded, symbol), None);
        assert_eq!(loaded.obarray().forward_type(symbol), None);
    }
    let _policy = RedisplayHookPolicyGuard::gnu();
    let mut loaded = load_from_dump(&path).expect("restore with GNU ownership");
    // GNU window.c:9383–9398 creates the nil special Obj cell before Lisp.
    assert_eq!(data::default_value_by_id(&loaded, symbol), Some(Value::NIL));
    assert!(loaded.obarray().is_special_id(symbol));
    assert_eq!(
        loaded.obarray().forward_type(symbol),
        Some(LispFwdType::Obj)
    );
    assert_eq!(
        data::default_value(&mut loaded, vec![Value::from_sym_id(symbol)])
            .expect("restored C-owned hook default"),
        Value::NIL
    );

    // Exercise the first configuration pass, whose Fdefault_value must finish
    // before the frontend is reached. The callback proves that boundary only;
    // it does not fabricate an accepted layout or test GNU idle acknowledgement.
    let buffer = loaded
        .buffer_manager()
        .current_buffer_id()
        .expect("restored current buffer");
    loaded
        .frame_manager_mut()
        .create_frame("restored", 80, 24, buffer);
    let called = std::rc::Rc::new(std::cell::Cell::new(false));
    let observed = called.clone();
    loaded.redisplay_fn = Some(Box::new(move |_| observed.set(true)));
    loaded.redisplay().expect("first restored GNU redisplay");
    assert!(called.get(), "configuration pass must reach the frontend");
}

#[test]
fn file_restore_preserves_configuration_hook_default_and_local_lists() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("portable-image fixture directory");
    let path = dir.path().join("configured-configuration-hook.pdump");
    let symbol = intern(HOOK);
    dump_legacy(&path, |eval| {
        eval.eval_str(
            "(progn
               (set-default 'window-configuration-change-hook
                            '(pdump-global-first pdump-global-last))
               (set-buffer (get-buffer-create \"pdump-local-first\"))
               (make-local-variable 'window-configuration-change-hook)
               (setq window-configuration-change-hook '(pdump-local-first t))
               (set-buffer (get-buffer-create \"pdump-local-second\"))
               (make-local-variable 'window-configuration-change-hook)
               (setq window-configuration-change-hook '(pdump-local-second)))",
        )
        .expect("install distinct default and local hook lists");
        assert_eq!(
            eval.obarray()
                .get_by_id(symbol)
                .map(|value| value.redirect()),
            Some(SymbolRedirect::Localized),
            "fixture must exercise the localized default cell"
        );
    });
    let _policy = RedisplayHookPolicyGuard::gnu();
    let mut loaded = load_from_dump(&path).expect("restore configured image under GNU policy");
    assert_eq!(
        loaded
            .obarray()
            .get_by_id(symbol)
            .map(|value| value.redirect()),
        Some(SymbolRedirect::Localized),
        "restore must retain the localized cell rather than replace it with a global nil forwarder"
    );
    assert!(loaded.obarray().is_special_id(symbol));
    assert_eq!(
        loaded
            .obarray()
            .blv(symbol)
            .and_then(|value| value.fwd)
            .map(|value| value.ty()),
        Some(LispFwdType::Obj),
        "existing activation must attach the normal GNU localized forwarder"
    );
    assert_eq!(
        format_eval_result(&loaded.eval_str(
            "(list
               (default-value 'window-configuration-change-hook)
               (buffer-local-value 'window-configuration-change-hook
                                   (get-buffer \"pdump-local-first\"))
               (buffer-local-value 'window-configuration-change-hook
                                   (get-buffer \"pdump-local-second\"))
               (local-variable-p 'window-configuration-change-hook
                                 (get-buffer \"pdump-local-first\"))
               (local-variable-p 'window-configuration-change-hook
                                 (get-buffer \"pdump-local-second\")))"
        )),
        "OK ((pdump-global-first pdump-global-last) (pdump-local-first t) (pdump-local-second) t t)"
    );
}

#[test]
fn file_restore_preserves_an_explicit_unbound_configuration_hook_cell() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("portable-image fixture directory");
    let path = dir.path().join("unbound-configuration-hook.pdump");
    let symbol = intern(HOOK);
    dump_legacy(&path, |eval| {
        // A plain unbound slot is saved Lisp state, not proof that the core
        // declaration was omitted. In particular, an undeclared legacy cell
        // can be explicitly made unbound and has no separate provenance bit.
        eval.obarray_mut().get_or_intern(HOOK);
        assert_eq!(
            eval.obarray().get_by_id(symbol).map(|value| value.plain()),
            Some(Value::UNBOUND)
        );
        assert!(!eval.obarray().is_special_id(symbol));
    });
    let _policy = RedisplayHookPolicyGuard::gnu();
    let mut loaded = load_from_dump(&path).expect("restore deliberately unbound cell");
    assert_eq!(
        loaded
            .obarray()
            .get_by_id(symbol)
            .map(|value| value.redirect()),
        Some(SymbolRedirect::Plainval)
    );
    assert_eq!(data::default_value_by_id(&loaded, symbol), None);
    assert_eq!(loaded.obarray().forward_type(symbol), None);
    assert!(!loaded.obarray().is_special_id(symbol));
    let flow = data::default_value(&mut loaded, vec![Value::from_sym_id(symbol)])
        .expect_err("explicit unbound default must remain visible to the GNU reader");
    let signal = flow.as_signal().expect("default-value signal");
    assert_eq!(signal.symbol, intern("void-variable"));
    assert_eq!(signal.data, vec![Value::from_sym_id(symbol)]);
}
