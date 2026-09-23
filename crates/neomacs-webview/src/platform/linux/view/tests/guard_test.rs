use super::guard_wpe_callback;

// The five WPE callbacks themselves require a live libwpe/WebKit runtime and
// raw C pointers to WPE buffers/views, so they are not unit-testable here.
// These tests cover the one piece of new logic — the panic-containment helper
// — which is the whole point of the change: a panicking body must never unwind
// past the guard, and the normal path must be a transparent passthrough.

#[test]
fn ok_path_passes_value_through() {
    assert_eq!(guard_wpe_callback("test", 0, || 42), 42);
}

#[test]
fn ok_path_runs_unit_body() {
    let mut ran = false;
    guard_wpe_callback("test", (), || ran = true);
    assert!(ran, "body must run on the normal path");
}

#[test]
fn static_str_panic_returns_neutral() {
    // A panic hook line on stderr is expected; the test still passes because
    // the panic is contained rather than propagated.
    assert_eq!(guard_wpe_callback("test", 7, || panic!("boom")), 7);
}

#[test]
fn string_panic_returns_neutral() {
    // Exercises the String (owned) payload downcast branch.
    let neutral = guard_wpe_callback("test", -1, || panic!("{}", String::from("dynamic")));
    assert_eq!(neutral, -1);
}
