//! LEAD 04:4xZ (e): the shared host-cfg probe must be visible at the Elisp
//! surface in a dev-shell build.
//!
//! Before the probe, `neovm-eval` had no build script, so its
//! `neomacs_have_lcms2` / `neomacs_have_dbus` branches were permanently
//! false while core's own probe was true: the lcms subrs never registered,
//! `dbus-event` was missing from while-no-input-ignore-events, and the nine
//! dbusbind variables were claimed absent. This test asserts the three
//! Elisp-visible consequences with the expected values derived from core's
//! probed cfgs; on the pre-probe base in a dev shell the actual values were
//! the absent ones and this test fails. It runs at P2 with the rest of the
//! core test target.

use crate::test_utils::runtime_startup_eval_one;

fn boolean(value: bool) -> &'static str {
    if value { "t" } else { "nil" }
}

#[test]
fn host_cfg_probe_is_visible_at_the_elisp_surface() {
    crate::test_utils::init_test_tracing();
    let result = runtime_startup_eval_one(
        "(list
           (fboundp 'lcms-cie-de2000)
           (and (memq 'dbus-event while-no-input-ignore-events) t)
           (mapcar (lambda (name) (and (boundp name) t))
                   '(dbus-compiled-version dbus-debug
                     dbus-message-type-error dbus-message-type-invalid
                     dbus-message-type-method-call dbus-message-type-method-return
                     dbus-message-type-signal dbus-registered-objects-table
                     dbus-runtime-version)))",
    );
    let dbus = cfg!(neomacs_have_dbus);
    let lcms2 = cfg!(neomacs_have_lcms2);
    let variables = std::iter::repeat(boolean(dbus))
        .take(9)
        .collect::<Vec<_>>()
        .join(" ");
    let expected = format!("OK ({} {} ({}))", boolean(lcms2), boolean(dbus), variables);
    assert_eq!(
        result, expected,
        "the shared host-cfg probe must reach the Elisp surface"
    );
}
