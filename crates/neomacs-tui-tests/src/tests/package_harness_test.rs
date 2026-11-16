//! The panic-reporting contract of the shared package harness: a phase that
//! panics must reach the suite's failure report with its payload, whatever
//! the payload's type.

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
}
