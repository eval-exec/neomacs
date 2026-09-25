//! The syntax parse cache hears about every change a scan could read (P3.4
//! S3): text edits by their first byte, syntax-relevant property writes by
//! their first character, wholesale changes as "everything", and a change
//! that bypassed the notes as "everything" too.

use super::*;

#[test]
fn the_knob_is_off_unless_switched_on() {
    for (value, mode) in [
        (None, ParseCacheMode::Off),
        (Some(""), ParseCacheMode::Off),
        (Some("0"), ParseCacheMode::Off),
        (Some("off"), ParseCacheMode::Off),
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
    let mut cache = SyntaxParseCache::default();
    assert_eq!(cache.drain(7, 3), Invalidation::All, "first sight");
    assert_eq!(cache.drain(7, 3), Invalidation::Nothing);
    cache.note_edit(40);
    cache.note_edit(12);
    assert_eq!(
        cache.drain(9, 3),
        Invalidation::From {
            byte: 12,
            char: usize::MAX
        }
    );
    cache.note_prop_change(30, 1);
    cache.note_prop_change(50, 2);
    assert_eq!(
        cache.drain(9, 6),
        Invalidation::From {
            byte: usize::MAX,
            char: 30
        }
    );
    // A tick or epoch the notes do not account for.
    cache.note_prop_change(30, 1);
    assert_eq!(cache.drain(9, 8), Invalidation::All);
    assert_eq!(cache.drain(10, 8), Invalidation::All);
    cache.clear();
    assert_eq!(cache.drain(10, 8), Invalidation::All);
    assert_eq!(cache.drain(10, 8), Invalidation::Nothing);
}

fn drain(eval: &crate::emacs_core::eval::Context) -> Invalidation {
    eval.buffers
        .current_buffer()
        .expect("current buffer")
        .with_syntax_parse_cache(|_, invalidation| invalidation)
}

#[test]
fn buffer_changes_reach_the_cache() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.eval_str("(insert \"(a b) ;; c\\n\\\"d é\\\" (e)\")")
        .expect("insert");
    let _ = drain(&eval);
    assert_eq!(drain(&eval), Invalidation::Nothing);

    // An insertion at char 17 is at byte 18 (é is two bytes); `insert` also
    // sets the inserted text's properties, from the same character.
    eval.eval_str("(goto-char 18) (insert \"x\")")
        .expect("insert");
    let Invalidation::From { byte, char } = drain(&eval) else {
        panic!("an edit has an extent")
    };
    assert_eq!((byte, char), (18, 17));

    // A deletion and a replace-match: the lower of the two (the replacement
    // text's properties are set too).
    eval.eval_str("(delete-region 20 22) (goto-char 1) (looking-at \"(a\") (replace-match \"[a\")")
        .expect("delete, replace");
    assert_eq!(drain(&eval), Invalidation::From { byte: 0, char: 0 });

    // A `syntax-table' property: its first character.
    eval.eval_str("(put-text-property 5 9 'syntax-table '(1))")
        .expect("put");
    assert_eq!(
        drain(&eval),
        Invalidation::From {
            byte: usize::MAX,
            char: 4
        }
    );
    // `category' too; `face' is not read by a scan.
    eval.eval_str("(put-text-property 7 8 'face 'bold)")
        .expect("face");
    assert_eq!(drain(&eval), Invalidation::Nothing);
    eval.eval_str("(put-text-property 3 4 'category 'c)")
        .expect("category");
    assert_eq!(
        drain(&eval),
        Invalidation::From {
            byte: usize::MAX,
            char: 2
        }
    );
    eval.eval_str("(remove-text-properties 6 19 '(syntax-table nil))")
        .expect("remove");
    assert_eq!(
        drain(&eval),
        Invalidation::From {
            byte: usize::MAX,
            char: 5
        }
    );
    eval.eval_str("(set-text-properties 10 12 nil)")
        .expect("set");
    assert_eq!(
        drain(&eval),
        Invalidation::From {
            byte: usize::MAX,
            char: 9
        }
    );

    // Wholesale changes.
    eval.eval_str("(set-buffer-multibyte nil) (set-buffer-multibyte t)")
        .expect("multibyte");
    assert_eq!(drain(&eval), Invalidation::All);
    assert_eq!(drain(&eval), Invalidation::Nothing);
}
