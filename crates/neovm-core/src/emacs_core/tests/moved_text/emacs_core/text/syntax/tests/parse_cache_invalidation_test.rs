//! The syntax parse cache hears about every change a scan could read (P3.4
//! S3): text edits by their first byte, syntax-relevant property writes by
//! their first character, wholesale changes as "everything", and a change
//! that bypassed the notes as "everything" too.

use super::*;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex};

use neovm_edit::buffer::syntax_cache::Invalidation;
use neovm_edit::buffer::syntax_cache::SyntaxMutationNotes;
use neovm_eval::emacs_core::eval::Context;

use neovm_text::emacs_core::syntax::parse_cache::*;

#[test]
fn buffer_changes_reach_the_cache() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
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

fn drain(eval: &crate::emacs_core::eval::Context) -> Invalidation {
    eval.edit
        .buffers
        .current_buffer()
        .expect("current buffer")
        .take_syntax_invalidation()
}
