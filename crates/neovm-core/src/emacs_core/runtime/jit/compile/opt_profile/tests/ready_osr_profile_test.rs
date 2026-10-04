//! One-variable OSR preset and explicit scalar precedence, without env writes.
//! Threading: all inputs/scopes belong to this test's compiler thread and
//! contain no Lisp handles, source cache, native counters or runtime recording.

use super::*;
use crate::emacs_core::jit::compile::knobs::{
    jit_opt_admit, jit_opt_early, jit_opt_fast, jit_opt_max_ops, jit_opt_mode, jit_opt_passes,
    jit_opt_profit, jit_opt_require_osr, opt_mode_scope_for_test, opt_require_osr_scope_for_test,
};

#[test]
fn opt_profile_lists48_osr_adds_primitive_and_ready_opt_policy_to_old_defaults() {
    let selected = Profile::parse(Some("lists48-osr"));
    assert_eq!(selected, Profile::Lists48Osr);
    assert_eq!(
        selected.defaults(),
        Defaults {
            profit: OptProfitMode::PrimitiveLists,
            require_osr: true,
            ..Profile::Lists48.defaults()
        }
    );
    for text in [
        None,
        Some("off"),
        Some("invalid"),
        Some("lists20"),
        Some("lists32"),
        Some("lists48"),
        Some("lists64"),
    ] {
        assert!(!Profile::parse(text).defaults().require_osr);
    }
    assert!(!Defaults::default().require_osr);
    assert_eq!(OptMode::parse(None), OptMode::Legacy);
    assert_eq!(OptAdmit::parse(None), OptAdmit::default());
    assert_eq!(OptPasses::parse(None), OptPasses::default());
}

#[test]
fn opt_profile_lists48_osr_explicit_invalid_and_legacy_precedence_is_lazy() {
    let absent = std::env::VarError::NotPresent;
    let defaults = Profile::Lists48Osr.defaults();
    let parse = |value: Option<&str>| value == Some("on");
    assert!(resolve(Err(&absent), || defaults.require_osr, parse));
    assert!(resolve(
        Ok("on"),
        || panic!("explicit on must not consult preset"),
        parse
    ));
    for value in ["off", "", "invalid", " on "] {
        assert!(!resolve(
            Ok(value),
            || panic!("explicit values must retain their parser"),
            parse
        ));
    }
    let invalid = std::env::VarError::NotUnicode(std::ffi::OsString::from("bad-entry"));
    assert!(!resolve(
        Err(&invalid),
        || panic!("explicit non-Unicode must not consult preset"),
        parse
    ));
    assert_eq!(
        resolve(
            Ok("legacy"),
            || panic!("legacy must not consult preset"),
            OptMode::parse
        ),
        OptMode::Legacy
    );
}

#[test]
fn opt_profile_lists48_osr_actual_reader_restores_individual_override_and_legacy() {
    let _off = scope_for_test(Profile::Off);
    assert!(!jit_opt_require_osr());
    {
        let _selected = scope_for_test(Profile::Lists48Osr);
        assert_eq!(jit_opt_mode(), OptMode::Opt);
        assert_eq!(jit_opt_admit(), OptAdmit::ALL);
        assert_eq!(jit_opt_passes(), OptPasses::parse(Some("fold,bool,reps")));
        assert_eq!(jit_opt_profit(), OptProfitMode::PrimitiveLists);
        assert_eq!(jit_opt_early(), OptEarlyMode::Hot);
        assert_eq!(jit_opt_max_ops(), 48);
        assert!(jit_opt_fast());
        assert!(jit_opt_require_osr());
        {
            let _individual = opt_require_osr_scope_for_test(false);
            assert!(!jit_opt_require_osr());
            assert_eq!(jit_opt_max_ops(), 48);
        }
        assert!(jit_opt_require_osr());
        {
            let _legacy = opt_mode_scope_for_test(OptMode::Legacy);
            assert_eq!(jit_opt_mode(), OptMode::Legacy);
            assert!(
                jit_opt_require_osr(),
                "legacy selection ignores the policy without altering its configured scalar"
            );
        }
        assert_eq!(jit_opt_mode(), OptMode::Opt);
    }
    assert!(!jit_opt_require_osr());
    assert_eq!(jit_opt_mode(), OptMode::Legacy);
}
