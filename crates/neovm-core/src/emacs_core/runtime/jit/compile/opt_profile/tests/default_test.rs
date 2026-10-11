//! Unset production profile and explicit escape paths, without env mutation.
//! Threading: each test supplies its own borrowed scalar configuration.

use super::*;

#[test]
fn opt_profile_unset_selects_the_qualified_lists48_osr_stack() {
    let absent = std::env::VarError::NotPresent;
    let selected = Profile::from_env(Err(&absent));
    assert_eq!(selected, Profile::Lists48Osr);
    let defaults = selected.defaults();
    assert_eq!(defaults.mode, OptMode::Opt);
    assert_eq!(defaults.admit, OptAdmit::ALL);
    assert_eq!(defaults.passes, OptPasses::parse(Some("fold,bool,reps")));
    assert_eq!(defaults.profit, OptProfitMode::PrimitiveLists);
    assert_eq!(defaults.early, OptEarlyMode::Hot);
    assert_eq!(defaults.max_ops, 48);
    assert!(defaults.fast);
    assert!(defaults.require_osr);
}

#[test]
fn opt_profile_explicit_off_and_invalid_keep_the_legacy_escape() {
    for text in ["off", "", "invalid", "lists128"] {
        assert_eq!(Profile::from_env(Ok(text)).defaults(), Defaults::default());
    }
    let invalid = std::env::VarError::NotUnicode(std::ffi::OsString::from("bad-entry"));
    assert_eq!(
        Profile::from_env(Err(&invalid)).defaults(),
        Defaults::default()
    );
    assert_eq!(
        resolve(
            Ok("legacy"),
            || panic!("explicit legacy must not consult the default profile"),
            OptMode::parse,
        ),
        OptMode::Legacy,
    );
}
