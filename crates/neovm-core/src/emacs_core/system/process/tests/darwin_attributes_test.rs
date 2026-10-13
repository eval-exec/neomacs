use super::*;

#[test]
fn elapsed_time_preserves_gnus_signed_wall_clock_difference() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::eval::Context::new();
    // GNU time-subtract yields these exact lists, including a negative
    // interval after a backward clock adjustment (sysdep.c:4403-4406).
    assert_eq!(
        format!("{}", elapsed_time((1000, 0), (1000, 1))),
        "(-1 65535 999999 0)"
    );
    assert_eq!(
        format!("{}", elapsed_time((1005, 200_000), (1000, 100_000))),
        "(0 5 100000 0)"
    );
}
