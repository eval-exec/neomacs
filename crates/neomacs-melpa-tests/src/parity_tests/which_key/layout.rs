use expect_test::expect;

use super::ParityBatchCase;

fn which_key_column_normalization_and_joining_preserve_row_and_column_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_column_normalization_and_joining_preserve_row_and_column_order",
        r##"(list
               (which-key--normalize-columns
                '(("a" "b") ("c") () ("d" "e" "f")))
               (which-key--join-columns
                '(("A1" "A2") ("B1") ("C1" "C2" "C3"))))"##,
        expect![[
            r#"OK ((("a" "b" "") ("c" "" "") ("" "" "") ("d" "e" "f")) "C1 B1 A1\nC2  A2\nC3  ")"#
        ]],
    )
}

fn which_key_partition_list_handles_even_remainder_oversized_and_empty_inputs() -> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_partition_list_handles_even_remainder_oversized_and_empty_inputs",
        r##"(list
               (which-key--partition-list 2 '(a b c d))
               (which-key--partition-list 2 '(a b c d e))
               (which-key--partition-list 9 '(a b))
               (which-key--partition-list 1 '(a b c))
               (which-key--partition-list 3 nil))"##,
        expect!["OK (((a b) (c d)) ((a b) (c d) (e)) ((a b)) ((a) (b) (c)) nil)"],
    )
}

fn which_key_list_to_pages_reports_exact_page_strings_and_metadata() -> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_list_to_pages_reports_exact_page_strings_and_metadata",
        r##"(let* ((which-key-add-column-padding 0)
                    (which-key-min-column-description-width 0)
                    (which-key-max-display-columns nil)
                    (keys '(("a" ":" "alpha" nil)
                            ("b" ":" "beta" nil)
                            ("c" ":" "charlie" nil)
                            ("d" ":" "delta" nil)
                            ("e" ":" "echo" nil)))
                    (pages (which-key--list-to-pages keys 2 18)))
               (list
                (which-key--pages-pages pages)
                (which-key--pages-height pages)
                (which-key--pages-widths pages)
                (which-key--pages-keys/page pages)
                (which-key--pages-page-nums pages)
                (which-key--pages-num-pages pages)
                (which-key--pages-total-keys pages)))"##,
        expect![[
            r#"OK (("a:alpha c:charlie\nb:beta  d:delta  " "e:echo") 2 (17 6) (4 1) (1 2) 2 5)"#
        ]],
    )
}

fn which_key_list_to_pages_respects_the_maximum_column_limit() -> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_list_to_pages_respects_the_maximum_column_limit",
        r##"(let ((keys '(("a" ":" "alpha" nil)
                           ("b" ":" "beta" nil)
                           ("c" ":" "charlie" nil)
                           ("d" ":" "delta" nil))))
               (let ((which-key-max-display-columns 1))
                 (let ((pages (which-key--list-to-pages keys 2 80)))
                   (list
                    (which-key--pages-num-pages pages)
                    (which-key--pages-keys/page pages)
                    (which-key--pages-pages pages)))))"##,
        expect![[r#"OK (2 (2 2) ("a:alpha\nb:beta " "c:charlie\nd:delta  "))"#]],
    )
}

fn which_key_list_to_pages_signals_when_no_cell_can_fit_the_width() -> ParityBatchCase {
    ParityBatchCase::signal(
        "which_key_list_to_pages_signals_when_no_cell_can_fit_the_width",
        r##"(which-key--list-to-pages
               '(("a" ":" "alpha" nil)
                 ("b" ":" "beta" nil))
               2
               3)"##,
        expect!["ERR (wrong-type-argument wholenump -4)"],
    )
}

fn which_key_page_rotation_updates_every_parallel_page_field() -> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_page_rotation_updates_every_parallel_page_field",
        r##"(let ((pages
                    (make-which-key--pages
                     :pages '("one" "two" "three")
                     :widths '(10 20 30)
                     :keys/page '(1 2 3)
                     :page-nums '(1 2 3)
                     :num-pages 3
                     :total-keys 6)))
               (which-key--pages-set-current-page pages 1)
               (let ((once
                      (list
                       (which-key--pages-pages pages)
                       (which-key--pages-widths pages)
                       (which-key--pages-keys/page pages)
                       (which-key--pages-page-nums pages))))
                 (which-key--pages-set-current-page pages -2)
                 (list
                  once
                  (which-key--pages-pages pages)
                  (which-key--pages-widths pages)
                  (which-key--pages-keys/page pages)
                  (which-key--pages-page-nums pages))))"##,
        expect![[
            r#"OK ((("two" "three" "one") (20 30 10) (2 3 1) (2 3 1)) ("three" "one" "two") (30 10 20) (3 1 2) (3 1 2))"#
        ]],
    )
}

fn which_key_formatting_replaces_groups_extracts_keys_and_truncates_descriptions() -> ParityBatchCase
{
    ParityBatchCase::value(
        "which_key_formatting_replaces_groups_extracts_keys_and_truncates_descriptions",
        r##"(let ((which-key-separator " : ")
                    (which-key-prefix-prefix "+")
                    (which-key-max-description-length 7)
                    (which-key-show-docstrings nil)
                    (which-key-replacement-alist
                     '(((nil . "forward-char")
                        . (nil . "move-forward")))))
               (cl-letf (((symbol-function 'which-key--popup-max-dimensions)
                          (lambda () '(20 . 80))))
                 (list
                  (mapcar
                   (lambda (cell)
                     (mapcar #'substring-no-properties cell))
                   (which-key--format-and-replace
                    '(("C-c a" . "forward-char")
                      ("C-c p" . "group:project")
                      ("C-c x" . "prefix"))))
                  (mapcar
                   (lambda (cell)
                     (mapcar #'substring-no-properties cell))
                   (which-key--format-and-replace
                    '(("C-c a" . "forward-char"))
                    t)))))"##,
        expect![[
            r#"OK ((("a" " : " "move-..") ("p" " : " "+proj..") ("x" " : " "+prefix")) (("C-c a" " : " "move-..")))"#
        ]],
    )
}

fn which_key_docstring_formatting_covers_append_only_and_disabled_modes() -> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_docstring_formatting_covers_append_only_and_disabled_modes",
        r##"(progn
               (defun neomacs-which-key-documented-command ()
                 "First documentation line.
Second documentation line."
                 (interactive))
               (list
                (let ((which-key-show-docstrings nil))
                  (substring-no-properties
                   (which-key--maybe-add-docstring
                    "command"
                    "neomacs-which-key-documented-command")))
                (let ((which-key-show-docstrings t))
                  (substring-no-properties
                   (which-key--maybe-add-docstring
                    "command"
                    "neomacs-which-key-documented-command")))
                (let ((which-key-show-docstrings 'docstring-only))
                  (substring-no-properties
                   (which-key--maybe-add-docstring
                    "command"
                    "neomacs-which-key-documented-command")))
                (let ((which-key-show-docstrings t))
                  (which-key--maybe-add-docstring
                   "missing"
                   "neomacs-which-key-unknown-command"))))"##,
        expect![[
            r#"OK ("command" "command First documentation line." "First documentation line." "missing")"#
        ]],
    )
}

pub(super) fn layout_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        which_key_column_normalization_and_joining_preserve_row_and_column_order(),
        which_key_partition_list_handles_even_remainder_oversized_and_empty_inputs(),
        which_key_list_to_pages_reports_exact_page_strings_and_metadata(),
        which_key_list_to_pages_respects_the_maximum_column_limit(),
        which_key_list_to_pages_signals_when_no_cell_can_fit_the_width(),
        which_key_page_rotation_updates_every_parallel_page_field(),
        which_key_formatting_replaces_groups_extracts_keys_and_truncates_descriptions(),
        which_key_docstring_formatting_covers_append_only_and_disabled_modes(),
    ]
}
