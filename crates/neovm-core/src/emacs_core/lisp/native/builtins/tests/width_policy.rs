//! Width policy domain and representation checks; oracle snapshots live in gdm_widths.
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;
use crate::encoding::{CharacterWidthPolicy, default_char_width_table};

#[test]
fn gdm_width_policy_default_table_preserves_unicode_ranges() {
    crate::test_utils::init_test_tracing();
    let table = default_char_width_table();
    for (code, width) in [
        (0x80, 4),
        (0x9f, 4),
        (0xa0, 1),
        (0xff, 1),
        (0x4e2d, 2),
        (0x301, 0),
        (0x16fe4, 2),
        (0x3fff80, 4),
    ] {
        assert_eq!(
            crate::emacs_core::chartable::ct_lookup(&table, code).unwrap(),
            Value::fixnum(width)
        );
    }
}

#[test]
fn gdm_width_policy_ascii_is_not_overridden_by_width_table() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    ctx.eval_str("(setq char-width-table (make-char-table nil 999))")
        .unwrap();
    let policy = CharacterWidthPolicy::from_context(&ctx);
    assert_eq!(policy.character_width(b'a' as u32), 1);
    assert_eq!(policy.character_width(0xe9), 999);
}

#[test]
fn gdm_width_policy_display_glyphs_do_not_remap_recursively() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::test_utils::runtime_startup_context();
    ctx.eval_str("(progn (setq buffer-display-table (make-display-table)) (aset buffer-display-table ?a [?b]) (aset buffer-display-table ?b [?x ?y]))").unwrap();
    let policy = CharacterWidthPolicy::from_context(&ctx);
    assert_eq!(policy.width(b'a' as u32), 1);
    assert_eq!(policy.width(b'b' as u32), 2);
}
