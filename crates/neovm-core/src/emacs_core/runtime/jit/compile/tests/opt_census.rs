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

#[test]
fn opt_census_empty_plan_keeps_selected_identity_until_its_owner_drops() {
    let obs = LeafObs::new(false);
    assert!(!is_opt(&obs));
    let holds = call_feedback::FeedbackHolds::enter();
    attach(&obs, &OptCensus::default());
    let holds = holds.finish();
    assert_eq!(holds.len(), 1);
    assert_eq!(holds[0].compiled_id(), None);
    assert_eq!(Arc::strong_count(&holds[0]), 1);
    assert!(is_opt(&obs));
    let counts = snapshot(&obs);
    assert!(counts.is_opt);
    assert!(counts.is_empty());
    assert_eq!(counts.tier_name(LeafTier::Baseline), "opt");
    drop(holds);
    assert!(!is_opt(&obs));
    let absent = snapshot(&obs);
    assert!(!absent.is_opt);
    assert_eq!(absent.tier_name(LeafTier::Baseline), "baseline");
}

#[test]
fn opt_census_live_old_marker_requires_the_exact_actual_leaf_hold() {
    let obs = LeafObs::new(false);
    let old = call_feedback::FeedbackHolds::enter();
    attach(&obs, &OptCensus::default());
    let old = old.finish();
    // Model reuse of this observation key before an old leaf's later
    // feedback_holds field drops. The old token deliberately remains alive.
    let new_owner = Arc::new(RuntimeState::new());
    let unrelated = [new_owner];
    let registry = REGISTRY.get().unwrap();
    let entries = registry
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let entry = entries.get(&key(&obs)).unwrap();
    assert!(entry.owner.strong_count() > 0);
    assert!(owns_marker(entry, &old));
    assert!(!owns_marker(entry, &[]));
    assert!(!owns_marker(entry, &unrelated));
    assert_eq!(Arc::strong_count(&old[0]), 1);
    assert_eq!(Arc::strong_count(&unrelated[0]), 1);
}
