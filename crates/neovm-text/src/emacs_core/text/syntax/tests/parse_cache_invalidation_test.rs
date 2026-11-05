//! The syntax parse cache hears about every change a scan could read (P3.4
//! S3): text edits by their first byte, syntax-relevant property writes by
//! their first character, wholesale changes as "everything", and a change
//! that bypassed the notes as "everything" too.

use super::*;

use neovm_edit::buffer::syntax_cache::SyntaxMutationNotes;

#[test]
fn the_knob_is_on_unless_switched_off() {
    for (value, mode) in [
        (None, ParseCacheMode::On),
        (Some(""), ParseCacheMode::On),
        (Some("0"), ParseCacheMode::Off),
        (Some("off"), ParseCacheMode::Off),
        (Some("no"), ParseCacheMode::Off),
        (Some("false"), ParseCacheMode::Off),
        (Some("nil"), ParseCacheMode::Off),
        (Some("1"), ParseCacheMode::On),
        (Some(" ON "), ParseCacheMode::On),
        (Some("verify"), ParseCacheMode::Verify),
        (Some("Verify"), ParseCacheMode::Verify),
    ] {
        assert_eq!(parse_parse_cache_knob(value), mode, "{value:?}");
    }
}

#[test]
fn notes_are_drained_as_the_lowest_changed_position() {
    // The notes are the substrate's plain invalidation account (stage-3 C7);
    // the cache consumes its outcome before answering from its runs.
    let mut notes = SyntaxMutationNotes::default();
    assert_eq!(
        notes.take_invalidation(7, 3),
        Invalidation::All,
        "first sight"
    );
    assert_eq!(notes.take_invalidation(7, 3), Invalidation::Nothing);
    notes.note_edit(40);
    notes.note_edit(12);
    assert_eq!(
        notes.take_invalidation(9, 3),
        Invalidation::From {
            byte: 12,
            char: usize::MAX
        }
    );
    notes.note_prop_change(30, 1);
    notes.note_prop_change(50, 2);
    assert_eq!(
        notes.take_invalidation(9, 6),
        Invalidation::From {
            byte: usize::MAX,
            char: 30
        }
    );
    // A tick or epoch the notes do not account for.
    notes.note_prop_change(30, 1);
    assert_eq!(notes.take_invalidation(9, 8), Invalidation::All);
    assert_eq!(notes.take_invalidation(10, 8), Invalidation::All);
    notes.clear();
    assert_eq!(notes.take_invalidation(10, 8), Invalidation::All);
    assert_eq!(notes.take_invalidation(10, 8), Invalidation::Nothing);
}
