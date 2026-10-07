use super::*;
use neovm_core::emacs_core::{Context, Value};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[test]
fn daemon_options_use_optional_inline_names_and_gnu_prefixes() {
    for flag in [
        "--daemon",
        "--dae",
        "-daemon",
        "--bg-daemon",
        "-bg-daemon",
        "--daemon=",
    ] {
        let options = parse_option(flag).unwrap();
        assert!(options.background, "{flag}");
        assert_eq!(options.name, None);
    }
    assert_eq!(
        parse_option("--fg-daemon=work"),
        Some(Options {
            background: false,
            name: Some("work".into())
        })
    );
    assert_eq!(parse_option("--da"), None);
    assert_eq!(parse_option("--fg-dae"), None);
    assert_eq!(parse_option("--daemon-other"), None);
    assert_eq!(parse_option("-daemon=work"), None);
}

#[test]
fn daemon_startup_keeps_action_operands_and_normal_modes_distinct() {
    let startup = super::super::parse_startup_options(
        [
            "neomacs",
            "--eval",
            "\"--daemon=operand\"",
            "--fg-daemon=work",
        ]
        .map(str::to_owned),
    )
    .unwrap();
    assert_eq!(
        startup.daemon,
        Some(Options {
            background: false,
            name: Some("work".into())
        })
    );
    assert!(!startup.noninteractive);
    assert_eq!(startup.frontend, super::super::FrontendKind::Tty);
    assert_eq!(
        startup.forwarded_args,
        vec!["neomacs", "--eval", "\"--daemon=operand\""]
    );
    let batch =
        super::super::parse_startup_options(["neomacs", "--batch"].map(str::to_owned)).unwrap();
    assert!(batch.noninteractive && batch.daemon.is_none());
    let ordinary = super::super::parse_startup_options(["neomacs"].map(str::to_owned)).unwrap();
    assert_eq!(ordinary.frontend, super::super::FrontendKind::Gui);
    assert!(ordinary.daemon.is_none());
    let literal =
        super::super::parse_startup_options(["neomacs", "--", "--daemon"].map(str::to_owned))
            .unwrap();
    assert!(literal.daemon.is_none());
    assert_eq!(literal.forwarded_args, vec!["neomacs", "--", "--daemon"]);
}

#[test]
fn daemon_initialized_consumes_a_failing_notifier_and_rejects_retry() {
    let mut eval = Context::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let notified = Arc::clone(&calls);
    eval.configure_daemon(
        None,
        Some(Box::new(move || {
            if notified.fetch_add(1, Ordering::SeqCst) == 0 {
                Err("I/O error during daemon initialization: fail-once".into())
            } else {
                Ok(())
            }
        })),
    );
    eval.set_variable("after-init-time", Value::T);
    // GNU src/emacs.c marks initialization consumed before reporting I/O
    // failure. A notifier that could succeed on retry must still run once.
    let result = "(condition-case err (daemon-initialized) (error (car (cdr err))))";
    assert_eq!(
        eval.eval_str(result).unwrap().as_utf8_str(),
        Some("I/O error during daemon initialization: fail-once")
    );
    assert_eq!(
        eval.eval_str(result).unwrap().as_utf8_str(),
        Some("The daemon has already been initialized")
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(eval.eval_str("(daemonp)").unwrap(), Value::T);
}

#[test]
fn daemon_identity_and_initialization_are_host_owned_and_one_shot() {
    let mut eval = Context::new();
    assert_eq!(eval.eval_str("(daemonp)").unwrap(), Value::NIL);
    assert!(eval.eval_str("(daemon-initialized)").is_err());
    let calls = Arc::new(AtomicUsize::new(0));
    let notified = Arc::clone(&calls);
    eval.configure_daemon(
        Some("work".into()),
        Some(Box::new(move || {
            notified.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })),
    );
    eval.set_variable("after-init-time", Value::NIL);
    assert_eq!(
        eval.eval_str("(daemonp)").unwrap().as_utf8_str(),
        Some("work")
    );
    assert!(eval.eval_str("(daemon-initialized)").is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    eval.set_variable("after-init-time", Value::T);
    assert_eq!(eval.eval_str("(daemon-initialized)").unwrap(), Value::T);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(eval.eval_str("(daemon-initialized)").is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        eval.eval_str("(daemonp)").unwrap().as_utf8_str(),
        Some("work")
    );
    let mut unnamed = Context::new();
    unnamed.configure_daemon(None, None);
    assert_eq!(unnamed.eval_str("(daemonp)").unwrap(), Value::T);
}
