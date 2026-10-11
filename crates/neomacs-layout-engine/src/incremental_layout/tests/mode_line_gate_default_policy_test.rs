//! Conditional absence-only regression; explicit values are separate controls.
//! Pure OsStr input does not mutate environment or process policy caches.
use super::{ModeLineGate, parse_mode_line_gate};

#[test]
fn promoted_mode_line_gate_defaults_only_when_absent() {
    assert_eq!(parse_mode_line_gate(None), ModeLineGate::Gnu);
}
