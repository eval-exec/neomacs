//! Strict combo oracle probes, batch 58: DEEP characterization of the shr
//! HTML->text rendering divergence. Probes inline tags (b/i/strong/em/span)
//! after text, nested inline, text-element-text siblings, and adjacent block
//! elements to pinpoint which rendering rule differs.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_l2_shr_inline_tags_after_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"a\\nb\\nc\\n\" 0 1 (face shr-text shr-indentation nil) 1 2 (face nil) 2 3 (face (shr-text bold)) 3 4 (face nil) 4 5 (face shr-text)) #(\"a\\ni\\nc\\n\" 0 1 (face shr-text shr-indentation nil) 1 2 (face nil) 2 3 (face (shr-text italic)) 3 4 (face nil) 4 5 (face shr-text)) #(\"a\\ns\\nc\\n\" 0 1 (face shr-text shr-indentation nil) 1 2 (face nil) 2 3 (face (shr-text bold)) 3 4 (face nil) 4 5 (face shr-text)) #(\"a\\ne\\nc\\n\" 0 1 (face shr-text shr-indentation nil) 1 2 (face nil) 2 3 (face (shr-text italic)) 3 4 (face nil) 4 5 (face shr-text)) #(\"a\\nsp\\nc\\n\" 0 1 (face shr-text shr-indentation nil) 1 2 (face nil) 2 4 (face shr-text) 4 5 (face nil) 5 6 (face shr-text)))""#
    ]];
    // Divergence surfaced 2026-06-27 (systematic shr newline bug): for every
    // inline tag (b/i/strong/em/span) following text, GNU Emacs inserts a
    // newline+indent between the text node and the element, while Neomacs
    // keeps them inline ("a b c" vs GNU "a\n    b\n    c" style). Same root
    // cause as div_l2_shr_text_element_text_sibling / nested / block /
    // single-element below and the batch-45 shr tests.
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (with-temp-buffer (shr-insert-document '(p nil "a " (b nil "b") " c")) (buffer-string))
      (with-temp-buffer (shr-insert-document '(p nil "a " (i nil "i") " c")) (buffer-string))
      (with-temp-buffer (shr-insert-document '(p nil "a " (strong nil "s") " c")) (buffer-string))
      (with-temp-buffer (shr-insert-document '(p nil "a " (em nil "e") " c")) (buffer-string))
      (with-temp-buffer (shr-insert-document '(p nil "a " (span nil "sp") " c")) (buffer-string)))
"##,
        &["net/shr.el"],
        expect,
    );
}

#[test]
fn div_l2_shr_text_element_text_sibling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"Hello\\nworld\\n\" 0 1 (face shr-text shr-indentation nil) 1 5 (face shr-text) 5 6 (face nil) 6 11 (face (shr-text bold)))""#
    ]];
    // Divergence surfaced 2026-06-27: "Hello world" (inline) vs GNU "Hello\n
    // world" — shr newline-between-siblings bug (see div_l2_shr_inline_tags).
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (shr-insert-document '(p nil "Hello " (b nil "world")))
  (buffer-string))
"##,
        &["net/shr.el"],
        expect,
    );
}

#[test]
fn div_l2_shr_nested_inline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"a\\nb\\nbi\\nc\\n\" 0 1 (face shr-text shr-indentation nil) 1 2 (face nil) 2 3 (face (shr-text bold)) 3 4 (face nil) 4 6 (face (shr-text italic bold)) 6 7 (face nil) 7 8 (face shr-text))""#
    ]];
    // Divergence surfaced 2026-06-27: nested inline (b > i) also breaks per-
    // sibling in GNU; Neomacs inline. shr newline bug.
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (shr-insert-document '(p nil "a " (b nil "b " (i nil "bi")) " c"))
  (buffer-string))
"##,
        &["net/shr.el"],
        expect,
    );
}

#[test]
fn div_l2_shr_adjacent_block_elements() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"para\\none\\n\\n\\npara\\ntwo\\n\" 0 1 (face shr-text shr-indentation nil) 1 4 (face shr-text) 4 5 (face nil) 5 8 (face shr-text) 11 12 (face shr-text shr-indentation nil) 12 15 (face shr-text) 15 16 (face nil) 16 19 (face shr-text))""#
    ]];
    // Divergence surfaced 2026-06-27: adjacent <p> children also break
    // differently ("para one" inline vs GNU "para\n one"). shr newline bug.
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (shr-insert-document '(div nil (p nil "para one") (p nil "para two")))
  (buffer-string))
"##,
        &["net/shr.el"],
        expect,
    );
}

#[test]
fn div_l2_shr_single_element_no_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"only\\nbold\\n\" 0 1 (face (shr-text bold) shr-indentation nil) 1 4 (face (shr-text bold)) 4 5 (face nil) 5 9 (face (shr-text bold)))""#
    ]];
    // Divergence surfaced 2026-06-27: even a single <b> with no preceding
    // text node ("only bold") breaks in GNU vs inline in Neomacs. shr newline
    // bug is systematic across structures.
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (shr-insert-document '(p nil (b nil "only bold")))
  (buffer-string))
"##,
        &["net/shr.el"],
        expect,
    );
}
