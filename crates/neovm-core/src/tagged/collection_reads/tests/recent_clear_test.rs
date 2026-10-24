//! Real cold misses must register scratch cells before shortcuts or rejection.
use super::*;
use crate::emacs_core::Value;

struct RestoreMode;

impl Drop for RestoreMode {
    fn drop(&mut self) {
        force_compiled_journal_for_test(None);
    }
}

fn clear_used() -> usize {
    let (used, dirty) = RECENT_READS.with(|recent| {
        (
            recent.words.iter().filter(|word| word.get() != 0).count(),
            recent.dirty.get(),
        )
    });
    RECENT_CLEARED_CELLS.with(|count| count.set(0));
    RECENT_CLEARED_MASKS.with(|count| count.set(0));
    clear_recent_reads();
    assert_eq!(RECENT_CLEARED_CELLS.with(Cell::get), used);
    assert_eq!(
        RECENT_CLEARED_MASKS.with(Cell::get),
        if dirty { 4 } else { 0 }
    );
    assert!(RECENT_READS.with(|recent| recent.words.iter().all(|word| word.get() == 0)));
    used
}

#[test]
fn recent_clear_tracks_collision_shortcuts_nested_scopes_and_replay() {
    let _restore = RestoreMode;
    force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
    let owners: Vec<_> = (0..4096)
        .map(|_| Value::cons(Value::NIL, Value::NIL))
        .collect();
    let slot = |bits: usize| ((bits >> 3) ^ (bits >> 11)) & 255;
    let source = owners[0];
    let collision = owners
        .iter()
        .copied()
        .find(|owner| owner.bits() != source.bits() && slot(owner.bits()) == slot(source.bits()))
        .expect("the live fixture must contain a real recent-read collision");
    let (_, cached) = capture(|| source.cons_car());
    let cached = cached.unwrap();
    let (_, outer) = capture(|| {
        OBSERVATION_STATE_ACCESSES.with(|count| count.set(0));
        source.cons_car();
        collision.cons_car();
        source.cons_car();
        assert_eq!(
            OBSERVATION_STATE_ACCESSES.with(Cell::get),
            2,
            "the collision retry takes the already-recorded publication shortcut"
        );
        assert_eq!(clear_used(), 1, "colliding owners share one dirty cell");
        assert!(cached.unchanged_and_observe());
        assert_eq!(clear_used(), 1, "transitive replay registers its miss");
        let (_, inner) = capture(|| source.cons_cdr());
        assert!(inner.unwrap().unchanged());
        assert_eq!(
            clear_used(),
            0,
            "inner retirement already cleared its scratch"
        );
        source.cons_car();
        assert_eq!(
            clear_used(),
            1,
            "outer publication shortcut still registers"
        );
        assert_eq!(clear_used(), 0, "an empty clear resets no masks or words");
    });
    source.set_car(Value::T);
    assert!(!cached.unchanged());
    assert!(!outer.unwrap().unchanged());
}

#[test]
fn recent_clear_tracks_real_fused_setter_projections() {
    let _restore = RestoreMode;
    force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
    let source = Value::cons(Value::NIL, Value::NIL);
    for value in [Value::T, Value::NIL] {
        let (_, reads) = capture(|| {
            assert!(crate::tagged::mutate::set_cons_car(source, value));
            assert_eq!(clear_used(), 1);
        });
        assert!(reads.unwrap().unchanged());
    }
}

#[test]
fn recent_clear_skips_a_replay_word_already_forgotten_by_admission() {
    let _restore = RestoreMode;
    force_compiled_journal_for_test(Some(CompiledJournalMode::Off));
    let source = Value::cons(Value::NIL, Value::NIL);
    let (_, unmarked) = capture(|| source.cons_car());
    let unmarked = unmarked.unwrap();
    force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
    let (_, reads) = capture(|| {
        assert!(unmarked.unchanged_and_observe());
        assert_eq!(clear_used(), 0, "unmarked replay forgets its scratch word");
        source.cons_car();
        assert_eq!(
            clear_used(),
            1,
            "the real projection publishes and registers"
        );
    });
    assert!(reads.unwrap().unchanged());
}
