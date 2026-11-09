//! Optimization equivalence and explicit refusal at the real fontification seam.
//! No process environment or TLS is mutated. GNU-derived property semantics
//! remain covered by textprop_test's fallback/LIMIT oracle; these cases compare
//! the new admitted path to that unchanged implementation in the same Context.

use super::*;
use crate::emacs_core::xdisp::{
    ensure_fontified_for_redisplay_with_mode, next_fontified_property_change,
    next_unfontified_position,
};
use neovm_value::error::EvalResult;
use neovm_value::value::Value;

#[test]
fn fontification_boundary_policy_defaults_on_and_keeps_explicit_off() {
    assert_eq!(parse(None), FontificationBoundaryMode::On);
    for value in ["on", "1", "true", "yes", " ON "] {
        assert_eq!(
            parse(Some(OsStr::new(value))),
            FontificationBoundaryMode::On
        );
    }
    for value in ["off", "0", "false", "no", "", "unknown"] {
        assert_eq!(
            parse(Some(OsStr::new(value))),
            FontificationBoundaryMode::Off
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        assert_eq!(
            parse(Some(OsStr::from_bytes(b"\xff"))),
            FontificationBoundaryMode::Off,
        );
    }
}
