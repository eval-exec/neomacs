//! Ownership and stale-key checks for opt-only, scalar report metadata.
use super::*;
use crate::emacs_core::jit::opt::passes::fold::FoldStats;

fn counted(n: usize) -> OptCensus {
    OptCensus {
        fold: Some(FoldStats {
            guards_folded: n,
            ..FoldStats::default()
        }),
        ..OptCensus::default()
    }
}

#[test]
fn opt_census_owner_uses_existing_leaf_holds_without_snapshot_ownership() {
    let obs = LeafObs::new(false);
    let holds = call_feedback::FeedbackHolds::enter();
    attach(&obs, &counted(7));
    let holds = holds.finish();
    assert_eq!(holds.len(), 1);
    let owner = &holds[0];
    assert_eq!(Arc::strong_count(owner), 1, "the registry owns only a Weak");
    assert_eq!(
        owner.compiled_id(),
        None,
        "tokens do not consume source ids"
    );
    let copy = snapshot(&obs);
    assert_eq!(copy.opt_fold.unwrap().guards_folded, 7);
    assert_eq!(
        Arc::strong_count(owner),
        1,
        "reporting does not hold runtime state"
    );
    drop(holds);
    assert!(
        snapshot(&obs).is_empty(),
        "dead leaf ownership hides metadata"
    );
}

#[test]
fn opt_census_dead_owner_cannot_survive_reuse_of_the_same_observation_key() {
    let obs = LeafObs::new(false);
    let first = call_feedback::FeedbackHolds::enter();
    attach(&obs, &counted(11));
    let first = first.finish();
    assert_eq!(snapshot(&obs).opt_fold.unwrap().guards_folded, 11);
    drop(first);
    assert!(snapshot(&obs).is_empty());
    let second = call_feedback::FeedbackHolds::enter();
    attach(&obs, &counted(23));
    let second = second.finish();
    assert_eq!(second.len(), 1);
    assert_eq!(Arc::strong_count(&second[0]), 1);
    assert_eq!(snapshot(&obs).opt_fold.unwrap().guards_folded, 23);
    drop(second);
    assert!(snapshot(&obs).is_empty());
}
