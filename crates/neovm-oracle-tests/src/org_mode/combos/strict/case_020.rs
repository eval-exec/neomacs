//! combo_strict_20.rs + strong_combo_complex_75.rs +
//! strong_combo_complex_76.rs — relentless probes: org-babel
//! with ob-lilypond/ob-octave/ob-sass/ob-sqlite, org-agenda
//! file interaction, org-element normalize on obscure types,
//! org-capture template fill, org-compat primitives,
//! org-entities all backends, multi-temp-buffer stress.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn strict_babel_esoteric_langs_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ob-lilypond ob-lilypond :ob-octave ob-octave :ob-sass ob-sass :ob-sqlite ob-sqlite :ob-maxima ob-maxima :ob-forth ob-forth)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (list
 :ob-lilypond (condition-case nil (require 'ob-lilypond) (error (featurep 'ob-lilypond)))
 :ob-octave (condition-case nil (require 'ob-octave) (error (featurep 'ob-octave)))
 :ob-sass (condition-case nil (require 'ob-sass) (error (featurep 'ob-sass)))
 :ob-sqlite (condition-case nil (require 'ob-sqlite) (error (featurep 'ob-sqlite)))
 :ob-maxima (condition-case nil (require 'ob-maxima) (error (featurep 'ob-maxima)))
 :ob-forth (condition-case nil (require 'ob-forth) (error (featurep 'ob-forth)))
 ))"##,
        expect,
    );
}
#[test]
fn strict_agenda_file_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:agenda-files-bound t :file-p t :file-to-front-fbound t :remove-file-fbound nil :agenda-files-list 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda) (list
 :agenda-files-bound (boundp 'org-agenda-files)
 :file-p (fboundp 'org-agenda-file-p)
 :file-to-front-fbound (fboundp 'org-agenda-file-to-front)
 :remove-file-fbound (fboundp 'org-agenda-remove-file)
 :agenda-files-list (when (boundp 'org-agenda-files) (length org-agenda-files))
 ))"##,
        expect,
    );
}
#[test]
fn strict_element_normalize_obscure_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((special-block (:type \"abstract\") \"abstract\\n  content\\nend\") (paragraph nil \"line1\\n\\n\\nline2\") (center-block nil \"c1\\nc2\\n  c3\\nc4\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-element) (list
 (org-element-normalize-contents '(special-block (:type "abstract") "  abstract\n    content\n  end"))
 (org-element-normalize-contents '(paragraph nil "  line1\n\n\n  line2"))
 (org-element-normalize-contents '(center-block nil "  c1\n  c2\n    c3\n  c4"))
 ))"##,
        expect,
    );
}
#[test]
fn strict_capture_template_fill() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:fill-template-fbound t :set-plist-fbound t :get-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-capture) (list
 :fill-template-fbound (fboundp 'org-capture-fill-template)
 :set-plist-fbound (fboundp 'org-capture-put)
 :get-fbound (fboundp 'org-capture-get)
 ))"##,
        expect,
    );
}
#[test]
fn strict_compat_primitives() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:buffer-substring nil :format-time t :time-less-p t :time-= nil :time-since t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-compat) (list
 :buffer-substring (fboundp 'org-buffer-substring-fountain)
 :format-time (fboundp 'org-format-time-string)
 :time-less-p (fboundp 'org-time-less-p)
 :time-= (fboundp 'org-time-=)
 :time-since (fboundp 'org-time-since)
 ))"##,
        expect,
    );
}
#[test]
fn strict_entities_all_backends() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name \"alpha\" :latex \"\\\\alpha\" :latex-math t :html \"&alpha;\" :ascii \"alpha\" :latin1 \"alpha\" :utf8 \"α\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-entities) (let ((e (org-entity-get "alpha")))
 (list :name (nth 0 e) :latex (nth 1 e) :latex-math (nth 2 e) :html (nth 3 e)
  :ascii (nth 4 e) :latin1 (nth 5 e) :utf8 (nth 6 e))))"##,
        expect,
    );
}
#[test]
fn strict_multibuffer_parallel_stress() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 13 3 9 5 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((r '()))
 (dolist (content '("* A\n** B\n" "| a | b |\n| 1 | 2 |\n" "#+begin_src emacs-lisp\n1\n#+end_src\n"
                    "- item1\n- item2\n" "#+TITLE: T\nContent.\n" "*bold* /italic/."))
   (with-temp-buffer (org-mode) (insert content)
     (push (length (org-element-map (org-element-parse-buffer) t #'identity)) r)))
 (nreverse r))"##,
        expect,
    );
}
#[test]
fn strict_org_sort_list_with_checkboxes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:sorted \"- [X] apple\\n- [ ] mango\\n- [ ] zebra\\n\") (:item-checkboxes (on off off)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "- [ ] zebra\n- [X] apple\n- [ ] mango\n")
 (let ((r '())) (goto-char (point-min))
  (org-sort-list nil ?a)
  (push (list :sorted (buffer-string)) r)
  (push (list :item-checkboxes (mapcar (lambda (i) (org-element-property :checkbox i))
    (org-element-map (org-element-parse-buffer) 'item #'identity))) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn strict_org_deadline_with_repeater() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:deadline \"<2024-01-15 Mon +1w>\") (:deadline-obj (timestamp (:standard-properties [18 nil nil nil 38 0 nil nil nil nil nil nil nil nil nil nil nil nil] :type active :range-type nil :raw-value \"<2024-01-15 Mon +1w>\" :year-start 2024 :month-start 1 :day-start 15 :hour-start nil :minute-start nil :year-end 2024 :month-end 1 :day-end 15 :hour-end nil :minute-end nil :repeater-type cumulate :repeater-value 1 :repeater-unit week))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (insert "* Task\n")
 (let ((r '())) (goto-char (point-min))
  (org-deadline nil "<2024-01-15 Mon +1w>")
  (push (list :deadline (org-entry-get nil "DEADLINE")) r)
  (let ((planning (car (org-element-map (org-element-parse-buffer) 'planning #'identity))))
    (when planning (push (list :deadline-obj (org-element-property :deadline planning)) r)))
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn strict_org_export_dispatcher() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:dispatch-fbound t :keybind-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ox) (list
 :dispatch-fbound (fboundp 'org-export-dispatch)
 :keybind-fbound (boundp 'org-export-dispatch-last-position)
 ))"##,
        expect,
    );
}
