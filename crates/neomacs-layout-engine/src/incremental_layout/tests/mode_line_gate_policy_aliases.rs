//! Explicit-input parser aliases and malformed values preserve the existing policy.
//! Pure OsStr inputs do not mutate the environment or reset a process OnceLock.
use super::{ModeLineGate, parse_mode_line_gate};
use std::ffi::OsStr;

#[test]
fn explicit_mode_line_gate_policy_preserves_aliases_and_invalid_values() {
    for value in ["gnu", "on", "1", " GNU ", "On"] {
        assert_eq!(
            parse_mode_line_gate(Some(OsStr::new(value))),
            ModeLineGate::Gnu,
            "explicit ON: {value:?}"
        );
    }
    for value in [
        "off", "0", "false", "no", "legacy", "prove", "", "  ", "unknown", "enabled", "同期",
        "onx", "true", "yes", "sync",
    ] {
        assert_eq!(
            parse_mode_line_gate(Some(OsStr::new(value))),
            ModeLineGate::Legacy,
            "explicit baseline or malformed: {value:?}"
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        for bytes in [
            b"\xff".as_slice(),
            b"on\xff".as_slice(),
            b"\xffon".as_slice(),
        ] {
            assert_eq!(
                parse_mode_line_gate(Some(OsStr::from_bytes(bytes))),
                ModeLineGate::Legacy
            );
        }
    }
}
