//! Pure preset/explicit-value precedence and actual scalar readers.
//! Threading: inputs and test overrides belong to the current test invocation;
//! no test mutates the process environment or retains Lisp state.

use super::*;
use crate::emacs_core::jit::compile::knobs::{
    jit_opt_admit, jit_opt_early, jit_opt_fast, jit_opt_max_ops, jit_opt_mode, jit_opt_passes,
    jit_opt_profit, opt_mode_scope_for_test,
};

#[test]
fn opt_profile_off_and_invalid_keep_original_parser_defaults() {
    for text in [None, Some("off"), Some(""), Some("lists"), Some("lists128")] {
        assert_eq!(Profile::parse(text), Profile::Off);
        assert_eq!(Profile::parse(text).defaults(), Defaults::default());
    }
    assert_eq!(OptMode::parse(None), OptMode::Legacy);
    assert_eq!(OptAdmit::parse(None), OptAdmit::default());
    assert_eq!(OptPasses::parse(None), OptPasses::default());
}

#[test]
fn opt_profile_presets_supply_only_the_bounded_lists_stack() {
    for (text, max) in [
        ("lists20", 20),
        ("lists32", 32),
        ("lists48", 48),
        ("lists64", 64),
    ] {
        let defaults = Profile::parse(Some(text)).defaults();
        assert_eq!(defaults.mode, OptMode::Opt);
        assert_eq!(defaults.admit, OptAdmit::ALL);
        assert_eq!(defaults.passes, OptPasses::parse(Some("fold,bool,reps")));
        assert_eq!(defaults.profit, OptProfitMode::Lists);
        assert_eq!(defaults.early, OptEarlyMode::Hot);
        assert_eq!(defaults.max_ops, max);
        assert!(defaults.fast);
    }
}

#[test]
fn opt_profile_explicit_replacements_and_legacy_escape_keep_old_parsing() {
    let defaults = Profile::Lists64.defaults();
    let absent = std::env::VarError::NotPresent;
    assert_eq!(
        resolve(Err(&absent), || defaults.mode, OptMode::parse),
        OptMode::Opt
    );
    assert_eq!(
        resolve(
            Ok("legacy"),
            || panic!("explicit legacy must not consult the preset"),
            OptMode::parse
        ),
        OptMode::Legacy
    );
    assert_eq!(
        resolve(Ok("off"), || defaults.mode, OptMode::parse),
        OptMode::Off
    );
    assert_eq!(
        resolve(Ok("garbage"), || defaults.mode, OptMode::parse),
        OptMode::Legacy
    );
    assert_eq!(
        resolve(Ok("args"), || defaults.admit, OptAdmit::parse),
        OptAdmit::parse(Some("args"))
    );
    assert_eq!(
        resolve(Ok(""), || defaults.admit, OptAdmit::parse),
        OptAdmit::default()
    );
    assert_eq!(
        resolve(Ok("all,-reps"), || defaults.passes, OptPasses::parse),
        OptPasses::parse(Some("all,-reps"))
    );
    assert_eq!(
        resolve(Ok("none"), || defaults.passes, OptPasses::parse),
        OptPasses::default()
    );
    assert_eq!(
        resolve(Ok("kernels"), || defaults.profit, OptProfitMode::parse),
        OptProfitMode::Kernels
    );
    assert_eq!(
        resolve(Ok("invalid"), || defaults.profit, OptProfitMode::parse),
        OptProfitMode::Off
    );
    assert_eq!(
        resolve(Ok("on"), || defaults.early, OptEarlyMode::parse),
        OptEarlyMode::On
    );
    assert_eq!(
        resolve(Ok(""), || defaults.early, OptEarlyMode::parse),
        OptEarlyMode::Off
    );
    let max = |value: Option<&str>| {
        value
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(0)
    };
    assert_eq!(resolve(Err(&absent), || defaults.max_ops, max), 64);
    assert_eq!(resolve(Ok("0"), || defaults.max_ops, max), 0);
    assert_eq!(resolve(Ok(" 32 "), || defaults.max_ops, max), 32);
    assert_eq!(resolve(Ok("invalid"), || defaults.max_ops, max), 0);
    assert!(!resolve(Ok("off"), || defaults.fast, |v| v == Some("on")));
    assert!(!resolve(Ok(" on "), || defaults.fast, |v| v == Some("on")));
}

#[test]
fn opt_profile_non_unicode_individual_values_do_not_activate_presets() {
    // Model the exact VarError returned for an explicitly present bad entry;
    // this test does not install or read it in the process environment.
    let invalid = std::env::VarError::NotUnicode(std::ffi::OsString::from("bad-entry"));
    let defaults = Profile::Lists48.defaults();
    assert_eq!(
        resolve(Err(&invalid), || defaults.mode, OptMode::parse),
        OptMode::Legacy
    );
    assert_eq!(
        resolve(Err(&invalid), || defaults.admit, OptAdmit::parse),
        OptAdmit::default()
    );
    assert_eq!(
        resolve(Err(&invalid), || defaults.passes, OptPasses::parse),
        OptPasses::default()
    );
    assert_eq!(
        resolve(Err(&invalid), || defaults.profit, OptProfitMode::parse),
        OptProfitMode::Off
    );
    assert_eq!(
        resolve(Err(&invalid), || defaults.early, OptEarlyMode::parse),
        OptEarlyMode::Off
    );
    assert_eq!(
        resolve(
            Err(&invalid),
            || defaults.max_ops,
            |v| v.and_then(|v| v.parse::<usize>().ok()).unwrap_or(0)
        ),
        0
    );
    assert!(!resolve(
        Err(&invalid),
        || defaults.fast,
        |v| v == Some("on")
    ));
}

#[test]
fn opt_profile_reader_scope_keeps_individual_override_highest_and_restores() {
    let _outer = scope_for_test(Profile::Off);
    assert_eq!(jit_opt_mode(), OptMode::Legacy);
    {
        let _profile = scope_for_test(Profile::Lists20);
        assert_eq!(jit_opt_mode(), OptMode::Opt);
        assert_eq!(jit_opt_admit(), OptAdmit::ALL);
        assert_eq!(jit_opt_passes(), OptPasses::parse(Some("fold,bool,reps")));
        assert_eq!(jit_opt_profit(), OptProfitMode::Lists);
        assert_eq!(jit_opt_early(), OptEarlyMode::Hot);
        assert_eq!(jit_opt_max_ops(), 20);
        assert!(jit_opt_fast());
        {
            let _individual = opt_mode_scope_for_test(OptMode::Legacy);
            assert_eq!(jit_opt_mode(), OptMode::Legacy);
            assert_eq!(jit_opt_max_ops(), 20);
        }
        assert_eq!(jit_opt_mode(), OptMode::Opt);
    }
    assert_eq!(jit_opt_mode(), OptMode::Legacy);
    assert_eq!(jit_opt_max_ops(), 0);
    assert!(!jit_opt_fast());
}
