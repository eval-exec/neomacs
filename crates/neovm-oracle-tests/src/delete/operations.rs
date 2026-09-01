//! Oracle parity tests for deletion operations: `delete-char`,
//! `delete-backward-char`, `delete-region`, `erase-buffer`,
//! `delete-and-extract-region`, and complex deletion patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// delete-char
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_char_forward() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "abcdefgh")
                    (goto-char (point-min))
                    (delete-char 3)
                    (buffer-string))"####;
    let expect = expect_test::expect![[r#""OK \"defgh\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_delete_char_backward() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "abcdefgh")
                    (goto-char (point-max))
                    (delete-char -3)
                    (buffer-string))"####;
    let expect = expect_test::expect![[r#""OK \"abcde\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_delete_char_middle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "hello world")
                    (goto-char 6)  ;; before "world"
                    (delete-char 5) ;; delete "world"
                    (list (buffer-string) (point)))"####;
    let expect = expect_test::expect![[r#""OK (\"hellod\" 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// delete-region
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_region_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "0123456789")
                    (delete-region 4 8)
                    (list (buffer-string) (point)))"####;
    let expect = expect_test::expect![[r#""OK (\"012789\" 7)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_delete_region_entire() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "hello world")
                    (delete-region (point-min) (point-max))
                    (list (buffer-string) (buffer-size) (point)))"####;
    let expect = expect_test::expect![[r#""OK (\"\" 0 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// delete-and-extract-region
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_and_extract() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "hello beautiful world")
                    (let ((extracted (delete-and-extract-region 6 16)))
                      (list extracted
                            (buffer-string)
                            (point))))"####;
    let expect = expect_test::expect![[r#""OK (\" beautiful\" \"hello world\" 12)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// erase-buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_erase_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "lots of content here\nmore lines\nand more\n")
                    (let ((before-size (buffer-size)))
                      (erase-buffer)
                      (list before-size
                            (buffer-size)
                            (buffer-string)
                            (point))))"####;
    let expect = expect_test::expect![[r#""OK (41 0 \"\" 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: selective deletion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_selective_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Delete lines matching a pattern
    let form = r####"(with-temp-buffer
                    (insert "keep this\n")
                    (insert "# comment\n")
                    (insert "keep this too\n")
                    (insert "# another comment\n")
                    (insert "also keep\n")
                    (goto-char (point-min))
                    (while (not (eobp))
                      (if (looking-at "^#")
                          (delete-region (line-beginning-position)
                                         (1+ (line-end-position)))
                        (forward-line 1)))
                    (buffer-string))"####;
    let expect = expect_test::expect![[r#""OK \"keep this\\nkeep this too\\nalso keep\\n\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: extract and restructure
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_extract_restructure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extract tagged sections and reassemble
    let form = r####"(with-temp-buffer
                    (insert "HEADER: Title\n")
                    (insert "body line 1\n")
                    (insert "body line 2\n")
                    (insert "FOOTER: End\n")
                    ;; Extract header
                    (goto-char (point-min))
                    (let ((header
                           (when (looking-at "^HEADER: \\(.*\\)$")
                             (prog1 (match-string 1)
                               (delete-region
                                (line-beginning-position)
                                (1+ (line-end-position)))))))
                      ;; Extract footer
                      (goto-char (point-max))
                      (forward-line -1)
                      (let ((footer
                             (when (looking-at "^FOOTER: \\(.*\\)$")
                               (prog1 (match-string 1)
                                 (delete-region
                                  (line-beginning-position)
                                  (min (1+ (line-end-position))
                                       (point-max)))))))
                        (list header
                              footer
                              (buffer-string)))))"####;
    let expect =
        expect_test::expect![[r#""OK (\"Title\" \"End\" \"body line 1\\nbody line 2\\n\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: word-at-point deletion with undo tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_word_by_word() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "the quick brown fox")
                    (let ((deleted-words nil))
                      ;; Delete words from the front
                      (goto-char (point-min))
                      (dotimes (_ 2)
                        (let ((start (point)))
                          (skip-chars-forward "a-z")
                          (let ((word (buffer-substring start (point))))
                            (delete-region start (point))
                            ;; Also delete trailing space
                            (when (and (< (point) (point-max))
                                       (= (char-after (point)) ?\ ))
                              (delete-char 1))
                            (setq deleted-words
                                  (cons word deleted-words)))))
                      (list (nreverse deleted-words)
                            (buffer-string))))"####;
    let expect = expect_test::expect![[r#""OK ((\"the\" \"quick\") \"brown fox\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
