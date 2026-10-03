//! Cold selected array-version ownership, independent of main's hot layouts.
//! Threading: settings are compiler-thread scalars; each test owns its source,
//! observation and feedback holds, while registry publication is synchronized.

use super::*;
use crate::emacs_core::bytecode::Op;
use crate::emacs_core::jit::Runtime;
use crate::emacs_core::jit::compile::{
    OptMode, OptPasses, force_opt_for_test, force_opt_passes_for_test,
};
use crate::emacs_core::jit::feedback::arrays::ObservedArrayKind;
use crate::emacs_core::jit::tier2::{T2Cells, T2Origin};

struct Settings;
impl Settings {
    fn enter(mode: OptMode) -> Self {
        force_opt_for_test(Some(mode), None);
        force_opt_passes_for_test(Some(OptPasses {
            range: true,
            ..OptPasses::default()
        }));
        Self
    }
}
impl Drop for Settings {
    fn drop(&mut self) {
        force_opt_for_test(None, None);
        force_opt_passes_for_test(None);
    }
}
fn profiling_obs() -> Box<LeafObs> {
    let mut obs = LeafObs::new(false);
    obs.t2 = T2Cells::with_origin(T2Origin::Profiling, 2, None);
    obs
}

#[test]
fn array_stability_mask_changes_and_rearm_share_one_exact_snapshot() {
    let _settings = Settings::enter(OptMode::Opt);
    let source = Runtime::new();
    let sites = source.array_sites_for(&[Op::Aref]);
    let site = sites.site_at(0).unwrap();
    let obs = profiling_obs();
    let holds = call_feedback::FeedbackHolds::enter();
    attach(&obs);
    let _owner = holds.finish();
    site.observe(ObservedArrayKind::PlainVector);
    let version = read(&obs, &source, 1).unwrap();
    assert!(version.changed);
    publish(&obs, version);
    for _ in 0..3 {
        site.observe(ObservedArrayKind::PlainVector);
    }
    assert!(
        !read(&obs, &source, 1).unwrap().changed,
        "samples do not change a mask version"
    );
    site.observe(ObservedArrayKind::PlainRecord);
    assert!(read(&obs, &source, 1).unwrap().changed);
    reset(&obs, &source, 1);
    assert!(
        !read(&obs, &source, 1).unwrap().changed,
        "rearm captures current masks"
    );
    site.observe(ObservedArrayKind::Other);
    assert!(read(&obs, &source, 1).unwrap().changed);
}

#[test]
fn array_stability_dead_token_cannot_reuse_an_observation_version() {
    let _settings = Settings::enter(OptMode::Opt);
    let source = Runtime::new();
    let _sites = source.array_sites_for(&[Op::Aref]);
    let obs = profiling_obs();
    let holds = call_feedback::FeedbackHolds::enter();
    attach(&obs);
    let owner = holds.finish();
    reset(&obs, &source, 1);
    assert!(!read(&obs, &source, 1).unwrap().changed);
    drop(owner);
    assert!(read(&obs, &source, 1).is_none());
    // Simulate address reuse without any allocator/pointer assumption: an
    // independent token at the exact same live observation key starts fresh.
    let holds = call_feedback::FeedbackHolds::enter();
    attach(&obs);
    let _new_owner = holds.finish();
    assert!(read(&obs, &source, 1).unwrap().changed);
}

#[test]
fn array_stability_off_and_legacy_add_no_ownership_hold() {
    for mode in [OptMode::Off, OptMode::Legacy] {
        let _settings = Settings::enter(mode);
        let source = Runtime::new();
        let obs = profiling_obs();
        let holds = call_feedback::FeedbackHolds::enter();
        attach(&obs);
        assert!(holds.finish().is_empty());
        assert!(read(&obs, &source, 1).is_none());
    }
}
