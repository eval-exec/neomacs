//! The panic-reporting contract of the shared package harness: a phase that
//! panics must reach the suite's failure report with its payload, whatever
//! the payload's type.

use std::any::Any;
use std::panic::panic_any;

use crate::package_harness::{catch_phase, panic_text};

#[test]
fn string_panic_payloads_become_the_report_text() {
    assert_eq!(
        panic_text(Box::new(String::from("beacon row missing"))),
        "beacon row missing"
    );
    assert_eq!(panic_text(Box::new("static message")), "static message");
    assert_eq!(
        panic_text(Box::new(Box::<str>::from("boxed message"))),
        "boxed message"
    );
    assert_eq!(
        panic_text(Box::new(Box::new(String::from("boxed string message")))),
        "boxed string message"
    );
}

#[test]
fn a_non_string_payload_stays_in_the_report() {
    // `panic_any` produces a payload with no message; dropping it would
    // leave the report with nothing from the panic but the phase label.
    let report = catch_phase("wait for row", || panic_any(42u32)).unwrap_err();
    assert!(
        report.starts_with("wait for row: #<non-string panic payload"),
        "{report}"
    );
    assert_ne!(report, "wait for row: non-string panic payload");
    // The identity is the payload's, not the delivery box's: distinct
    // payload types print distinct identities, where every payload used to
    // print as `Box<dyn Any + Send>`.
    let u32_identity = panic_text(Box::new(42u32) as Box<dyn Any + Send>);
    let unit_identity = panic_text(Box::new(()) as Box<dyn Any + Send>);
    assert_ne!(
        u32_identity, unit_identity,
        "every payload reports as the same opaque identity"
    );
}

#[test]
fn a_rethrown_panic_payload_unwraps_to_its_message() {
    // A caught panic re-thrown arrives wrapped in its own delivery box; the
    // report reaches through to the message instead of stopping at a box.
    let original: Box<dyn Any + Send> = Box::new(String::from("original failure"));
    let report = catch_phase("rethrown", || panic_any(original)).unwrap_err();
    assert!(report.contains("rethrown: original failure"), "{report}");
}

#[test]
fn a_unit_payload_names_the_expect_test_mismatch() {
    // `expect-test`'s `assert_eq` prints its Expect/Actual diff on stdout and
    // then panics with `()` through `resume_unwind`, precisely to keep the
    // failure backtrace-free.  A unit payload in this suite is therefore a
    // snapshot mismatch, and the report must say so -- where the diff went --
    // rather than print an identity that identifies nothing.
    let report = catch_phase("checkpoint", || panic_any(())).unwrap_err();
    assert!(
        report.contains("expect-test reports a snapshot mismatch"),
        "{report}"
    );
}

#[test]
fn a_caught_panic_reports_where_it_started() {
    let report = catch_phase("located", || panic!("message-less location probe")).unwrap_err();
    assert!(
        report.contains("panicked at ") && report.contains("package_harness_test.rs"),
        "{report}"
    );
}
