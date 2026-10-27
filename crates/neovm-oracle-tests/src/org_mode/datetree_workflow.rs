use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_datetree_property_subtree_timestamp_cleanup_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"* Inbox\\n** Timeline\\n:PROPERTIES:\\n:DATE_TREE: t\\n:END:\\n*** 2026\\n**** 2026-05 May\\n\\n***** 2026-05-25 Monday\\n[2026-05-25 Mon]\\n****** Early\\n<2026-05-25 Mon>\\n***** 2026-05-26 Tuesday\\n[2026-05-26 Tue]\\n****** LateMoved stamp <2026-05-26 Tue>\\n\\nBody\\n***** 2026-05-27 Wednesday\\n[2026-05-27 Wed]\\n\\n** Other\\n\" 5 2)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-datetree)
  (with-temp-buffer
    (let ((org-datetree-add-timestamp 'inactive))
      (org-mode)
      (insert "* Inbox\n")
      (insert "** Timeline\n")
      (insert ":PROPERTIES:\n:DATE_TREE: t\n:END:\n")
      (insert "** Other\n")
      (goto-char (point-min))
      (search-forward "Timeline")
      (org-datetree-file-entry-under "* Late\nBody\n" '(5 27 2026))
      (goto-char (point-min))
      (search-forward "Timeline")
      (org-datetree-file-entry-under "* Early\n<2026-05-25 Mon>\n" '(5 25 2026))
      (goto-char (point-min))
      (search-forward "* Late")
      (insert "Moved stamp <2026-05-26 Tue>\n")
      (goto-char (point-min))
      (org-datetree-cleanup)
      (list
       (buffer-substring-no-properties (point-min) (point-max))
       (save-excursion
         (goto-char (point-min))
         (search-forward "2026-05-26")
         (org-outline-level))
       (save-excursion
         (goto-char (point-min))
         (search-forward "* Other")
         (org-outline-level))))))"#,
        expect,
    );
}

#[test]
fn org_datetree_iso_week_property_ordering_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((1 \"Weekly\") (2 \"2026\") (3 \"2026-W02\") (4 \"Earlier entry\") (4 \"2026-01-05 Monday\") (3 \"2026-W53\") (4 \"Thu entry\") (4 \"2026-12-31 Thursday\") (4 \"Fri entry\") (4 \"2027-01-01 Friday\") (1 \"Notes\")) \"* Weekly\\n:PROPERTIES:\\n:WEEK_TREE: t\\n:END:\\n** 2026\\n\\n*** 2026-W02\\n\\n**** Earlier entry\\n**** 2026-01-05 Monday\\n\\n*** 2026-W53\\n\\n**** Thu entry\\n**** 2026-12-31 Thursday\\n\\n**** Fri entry\\n**** 2027-01-01 Friday\\n\\n* Notes\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-datetree)
  (with-temp-buffer
    (org-mode)
    (insert "* Weekly\n")
    (insert ":PROPERTIES:\n:WEEK_TREE: t\n:END:\n")
    (insert "* Notes\n")
    (goto-char (point-min))
    (search-forward "Weekly")
    (beginning-of-line)
    (org-datetree-find-iso-week-create '(12 31 2026) 'subtree-at-point)
    (insert "\n**** Thu entry\n")
    (goto-char (point-min))
    (search-forward "Weekly")
    (beginning-of-line)
    (org-datetree-find-iso-week-create '(1 1 2027) 'subtree-at-point)
    (insert "\n**** Fri entry\n")
    (goto-char (point-min))
    (search-forward "Weekly")
    (beginning-of-line)
    (org-datetree-find-iso-week-create '(1 5 2026) 'subtree-at-point)
    (insert "\n**** Earlier entry\n")
    (let ((headlines nil))
      (org-element-map (org-element-parse-buffer) 'headline
        (lambda (headline)
          (push (list (org-element-property :level headline)
                      (org-element-property :raw-value headline))
                headlines)))
      (list (nreverse headlines)
            (buffer-substring-no-properties (point-min) (point-max))))))"#,
        expect,
    );
}

#[test]
fn org_datetree_month_and_day_find_existing_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"* 2026\\n\\n*** Month note\\n** 2026-05 May\\n*** 2026-05-27 Wednesday\\n**** Existing\\n\\n**** Day note\\n\\n** 2026-06 June\\n\\n**** New month day\\n*** 2026-06-02 Tuesday\\n\" 2 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-datetree)
  (with-temp-buffer
    (org-mode)
    (insert "* 2026\n")
    (insert "** 2026-05 May\n")
    (insert "*** 2026-05-27 Wednesday\n")
    (insert "**** Existing\n")
    (goto-char (point-min))
    (org-datetree-find-month-create '(5 1 2026))
    (insert "\n*** Month note\n")
    (goto-char (point-min))
    (org-datetree-find-date-create '(5 27 2026))
    (org-end-of-subtree t t)
    (insert "\n**** Day note\n")
    (goto-char (point-min))
    (org-datetree-find-date-create '(6 2 2026))
    (insert "\n**** New month day\n")
    (list
     (buffer-substring-no-properties (point-min) (point-max))
     (save-excursion
       (goto-char (point-min))
       (search-forward "2026-05 May")
       (org-outline-level))
     (save-excursion
       (goto-char (point-min))
       (search-forward "2026-06-02 Tuesday")
       (org-outline-level)))))"#,
        expect,
    );
}

#[test]
fn org_datetree_dual_tree_cleanup_level_matrix_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((1 \"Daily\") (2 \"2026\") (3 \"2026-05 May\") (4 \"2026-05-27 Wednesday\") (4 \"2026-05-28 Thursday\") (5 \"Day A\") (4 \"2026-05-29 Friday\") (5 \"Day B\") (1 \"Weekly\") (2 \"2026\") (3 \"2026-W22\") (4 \"Week entry\") (4 \"2026-05-27 Wednesday\") (1 \"Loose\")) ((\"Daily\" 1) (\"2026\" 2) (\"2026-05 May\" 3) (\"2026-05-28 Thursday\" 4) (\"Day A\" 5) (\"Weekly\" 1) (\"2026-W22\" 3) (\"Week entry\" 4) (\"Loose\" 1)) \"* Daily\\n:PROPERTIES:\\n:DATE_TREE: t\\n:END:\\n** 2026\\n*** 2026-05 May\\n**** 2026-05-27 Wednesday\\n<2026-05-27 Wed>\\n**** 2026-05-28 Thursday\\n<2026-05-28 Thu>\\n***** Day A\\nMove marker <2026-05-28 Thu>\\n\\nBody A\\n\\n**** 2026-05-29 Friday\\n<2026-05-29 Fri>\\n\\n***** Day B\\n<2026-05-29 Fri>\\n* Weekly\\n:PROPERTIES:\\n:WEEK_TREE: t\\n:END:\\n** 2026\\n*** 2026-W22\\n\\n**** Week entry\\n**** 2026-05-27 Wednesday\\n<2026-05-27 Wed>\\n\\n* Loose\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-datetree)
  (with-temp-buffer
    (let ((org-datetree-add-timestamp 'active))
      (org-mode)
      (insert "* Daily\n:PROPERTIES:\n:DATE_TREE: t\n:END:\n")
      (insert "* Weekly\n:PROPERTIES:\n:WEEK_TREE: t\n:END:\n")
      (insert "* Loose\n")
      (goto-char (point-min))
      (search-forward "Daily")
      (beginning-of-line)
      (org-datetree-file-entry-under "* Day A\nBody A\n" '(5 27 2026))
      (goto-char (point-min))
      (search-forward "Daily")
      (beginning-of-line)
      (org-datetree-file-entry-under "* Day B\n<2026-05-29 Fri>\n" '(5 29 2026))
      (goto-char (point-min))
      (search-forward "Weekly")
      (beginning-of-line)
      (org-datetree-find-iso-week-create '(5 27 2026) 'subtree-at-point)
      (insert "\n**** Week entry\n")
      (goto-char (point-min))
      (search-forward "Day A")
      (insert "\nMove marker <2026-05-28 Thu>\n")
      (org-datetree-cleanup)
      (let (heads)
        (org-element-map (org-element-parse-buffer) 'headline
          (lambda (headline)
            (push (list (org-element-property :level headline)
                        (org-element-property :raw-value headline))
                  heads)))
        (list (nreverse heads)
              (mapcar (lambda (needle)
                        (save-excursion
                          (goto-char (point-min))
                          (search-forward needle)
                          (list needle (org-outline-level))))
                      '("Daily" "2026" "2026-05 May"
                        "2026-05-28 Thursday" "Day A"
                        "Weekly" "2026-W22" "Week entry" "Loose"))
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"#,
        expect,
    );
}

#[test]
fn org_datetree_keep_restriction_subtree_property_matrix_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((initial nil nil ((1 \"Property target\" 1 70) (2 \"Existing child\" 52 70) (1 \"Narrow target\" 70 104) (2 \"Seed\" 86 104) (1 \"Explicit target\" 104 143) (2 \"Existing explicit\" 122 143)) ((\"Property target\" 1 1 nil nil) (\"Narrow target\" 6 1 nil nil) (\"Explicit target\" 9 1 nil nil) (\"2026\" absent) (\"2026-05 May\" absent) (\"2026-05-27 Wednesday\" absent) (\"2026-06 June\" absent) (\"2026-06-02 Tuesday\" absent) (\"2027-01 January\" absent) (\"2027-01-03 Sunday\" absent)) \"* Property target\\n:PROPERTIES:\\n:DATE_TREE: t\\n:END:\\n** Existing child\\n* Narrow target\\n** Seed\\nseed body\\n* Explicit target\\n** Existing explicit\\n\") (after-keep-restriction nil nil ((1 \"Property target\" 1 70) (2 \"Existing child\" 52 70) (1 \"Narrow target\" 70 104) (2 \"Seed\" 86 104) (1 \"2026\" 104 193) (2 \"2026-05 May\" 111 193) (3 \"2026-05-27 Wednesday\" 126 193) (4 \"Restricted insert\" 169 193) (1 \"Explicit target\" 193 232) (2 \"Existing explicit\" 211 232)) ((\"Property target\" 1 1 nil nil) (\"Narrow target\" 6 1 nil nil) (\"Explicit target\" 16 1 nil nil) (\"2026\" 9 1 nil nil) (\"2026-05 May\" 10 2 nil nil) (\"2026-05-27 Wednesday\" 11 3 nil nil) (\"2026-06 June\" absent) (\"2026-06-02 Tuesday\" absent) (\"2027-01 January\" absent) (\"2027-01-03 Sunday\" absent)) \"* Property target\\n:PROPERTIES:\\n:DATE_TREE: t\\n:END:\\n** Existing child\\n* Narrow target\\n** Seed\\nseed body\\n* 2026\\n** 2026-05 May\\n*** 2026-05-27 Wednesday\\n<2026-05-27 Wed>\\n\\n**** Restricted insert\\n\\n* Explicit target\\n** Existing explicit\\n\") (after-property-widen nil nil ((1 \"Property target\" 1 159) (2 \"Existing child\" 52 70) (2 \"2026\" 70 159) (3 \"2026-06 June\" 78 159) (4 \"2026-06-02 Tuesday\" 95 138) (4 \"Property insert\" 138 159) (1 \"Narrow target\" 159 193) (2 \"Seed\" 175 193) (1 \"2026\" 193 282) (2 \"2026-05 May\" 200 282) (3 \"2026-05-27 Wednesday\" 215 282) (4 \"Restricted insert\" 258 282) (1 \"Explicit target\" 282 321) (2 \"Existing explicit\" 300 321)) ((\"Property target\" 1 1 nil nil) (\"Narrow target\" 13 1 nil nil) (\"Explicit target\" 23 1 nil nil) (\"2026\" 6 2 nil nil) (\"2026-05 May\" 17 2 nil nil) (\"2026-05-27 Wednesday\" 18 3 nil nil) (\"2026-06 June\" 7 3 nil nil) (\"2026-06-02 Tuesday\" 8 4 nil nil) (\"2027-01 January\" absent) (\"2027-01-03 Sunday\" absent)) \"* Property target\\n:PROPERTIES:\\n:DATE_TREE: t\\n:END:\\n** Existing child\\n** 2026\\n*** 2026-06 June\\n**** 2026-06-02 Tuesday\\n<2026-06-02 Tue>\\n\\n\\n**** Property insert\\n* Narrow target\\n** Seed\\nseed body\\n* 2026\\n** 2026-05 May\\n*** 2026-05-27 Wednesday\\n<2026-05-27 Wed>\\n\\n**** Restricted insert\\n\\n* Explicit target\\n** Existing explicit\\n\") (after-explicit-subtree nil nil ((1 \"Property target\" 1 159) (2 \"Existing child\" 52 70) (2 \"2026\" 70 159) (3 \"2026-06 June\" 78 159) (4 \"2026-06-02 Tuesday\" 95 138) (4 \"Property insert\" 138 159) (1 \"Narrow target\" 159 193) (2 \"Seed\" 175 193) (1 \"2026\" 193 282) (2 \"2026-05 May\" 200 282) (3 \"2026-05-27 Wednesday\" 215 282) (4 \"Restricted insert\" 258 282) (1 \"Explicit target\" 282 442) (2 \"Existing explicit\" 300 321) (2 \"2027\" 321 442) (3 \"2027-01 January\" 329 416) (4 \"2027-01-03 Sunday\" 349 391) (4 \"Explicit day insert\" 391 416) (3 \"Explicit month insert\" 416 442)) ((\"Property target\" 1 1 nil nil) (\"Narrow target\" 13 1 nil nil) (\"Explicit target\" 23 1 nil nil) (\"2026\" 6 2 nil nil) (\"2026-05 May\" 17 2 nil nil) (\"2026-05-27 Wednesday\" 18 3 nil nil) (\"2026-06 June\" 7 3 nil nil) (\"2026-06-02 Tuesday\" 8 4 nil nil) (\"2027-01 January\" 26 3 nil nil) (\"2027-01-03 Sunday\" 27 4 nil nil)) \"* Property target\\n:PROPERTIES:\\n:DATE_TREE: t\\n:END:\\n** Existing child\\n** 2026\\n*** 2026-06 June\\n**** 2026-06-02 Tuesday\\n<2026-06-02 Tue>\\n\\n\\n**** Property insert\\n* Narrow target\\n** Seed\\nseed body\\n* 2026\\n** 2026-05 May\\n*** 2026-05-27 Wednesday\\n<2026-05-27 Wed>\\n\\n**** Restricted insert\\n\\n* Explicit target\\n** Existing explicit\\n** 2027\\n*** 2027-01 January\\n**** 2027-01-03 Sunday\\n<2027-01-03 Sun>\\n\\n\\n**** Explicit day insert\\n*** Explicit month insert\\n\") (after-cleanup nil nil ((1 \"Property target\" 1 159) (2 \"Existing child\" 52 70) (2 \"2026\" 70 159) (3 \"2026-06 June\" 78 159) (4 \"2026-06-02 Tuesday\" 95 138) (4 \"Property insert\" 138 159) (1 \"Narrow target\" 159 193) (2 \"Seed\" 175 193) (1 \"2026\" 193 282) (2 \"2026-05 May\" 200 282) (3 \"2026-05-27 Wednesday\" 215 282) (4 \"Restricted insert\" 258 282) (1 \"Explicit target\" 282 442) (2 \"Existing explicit\" 300 321) (2 \"2027\" 321 442) (3 \"2027-01 January\" 329 416) (4 \"2027-01-03 Sunday\" 349 391) (4 \"Explicit day insert\" 391 416) (3 \"Explicit month insert\" 416 442)) ((\"Property target\" 1 1 nil nil) (\"Narrow target\" 13 1 nil nil) (\"Explicit target\" 23 1 nil nil) (\"2026\" 6 2 nil nil) (\"2026-05 May\" 17 2 nil nil) (\"2026-05-27 Wednesday\" 18 3 nil nil) (\"2026-06 June\" 7 3 nil nil) (\"2026-06-02 Tuesday\" 8 4 nil nil) (\"2027-01 January\" 26 3 nil nil) (\"2027-01-03 Sunday\" 27 4 nil nil)) \"* Property target\\n:PROPERTIES:\\n:DATE_TREE: t\\n:END:\\n** Existing child\\n** 2026\\n*** 2026-06 June\\n**** 2026-06-02 Tuesday\\n<2026-06-02 Tue>\\n\\n\\n**** Property insert\\n* Narrow target\\n** Seed\\nseed body\\n* 2026\\n** 2026-05 May\\n*** 2026-05-27 Wednesday\\n<2026-05-27 Wed>\\n\\n**** Restricted insert\\n\\n* Explicit target\\n** Existing explicit\\n** 2027\\n*** 2027-01 January\\n**** 2027-01-03 Sunday\\n<2027-01-03 Sun>\\n\\n\\n**** Explicit day insert\\n*** Explicit month insert\\n\")) (error \"Not at heading\") 19 \"* Property target\\n:PROPERTIES:\\n:DATE_TREE: t\\n:END:\\n** Existing child\\n** 2026\\n*** 2026-06 June\\n**** 2026-06-02 Tuesday\\n<2026-06-02 Tue>\\n\\n\\n**** Property insert\\n* Narrow target\\n** Seed\\nseed body\\n* 2026\\n** 2026-05 May\\n*** 2026-05-27 Wednesday\\n<2026-05-27 Wed>\\n\\n**** Restricted insert\\n\\n* Explicit target\\n** Existing explicit\\n** 2027\\n*** 2027-01 January\\n**** 2027-01-03 Sunday\\n<2027-01-03 Sun>\\n\\n\\n**** Explicit day insert\\n*** Explicit month insert\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-datetree)
  (with-temp-buffer
    (let ((org-datetree-add-timestamp 'active)
          states)
      (org-mode)
      (insert "* Property target\n")
      (insert ":PROPERTIES:\n:DATE_TREE: t\n:END:\n")
      (insert "** Existing child\n")
      (insert "* Narrow target\n")
      (insert "** Seed\n")
      (insert "seed body\n")
      (insert "* Explicit target\n")
      (insert "** Existing explicit\n")
      (cl-labels
          ((heads
            ()
            (let (out)
              (org-element-map (org-element-parse-buffer) 'headline
                (lambda (headline)
                  (push (list (org-element-property :level headline)
                              (org-element-property :raw-value headline)
                              (org-element-property :begin headline)
                              (org-element-property :end headline))
                        out)))
              (nreverse out)))
           (line-info
            (needle)
            (save-excursion
              (goto-char (point-min))
              (if (search-forward needle nil t)
                  (list needle
                        (line-number-at-pos)
                        (org-outline-level)
                        (bound-and-true-p org-datetree-base-level)
                        (buffer-narrowed-p))
                (list needle 'absent))))
           (snapshot
            (label)
            (save-restriction
              (widen)
              (list label
                    (bound-and-true-p org-datetree-base-level)
                    (buffer-narrowed-p)
                    (heads)
                    (mapcar #'line-info
                            '("Property target" "Narrow target"
                              "Explicit target" "2026"
                              "2026-05 May" "2026-05-27 Wednesday"
                              "2026-06 June" "2026-06-02 Tuesday"
                              "2027-01 January" "2027-01-03 Sunday"))
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))
        (push (snapshot 'initial) states)
        (goto-char (point-min))
        (search-forward "Narrow target")
        (beginning-of-line)
        (org-narrow-to-subtree)
        (goto-char (point-min))
        (org-datetree-find-date-create '(5 27 2026) t)
        (org-end-of-subtree t t)
        (insert "\n**** Restricted insert\n")
        (push (snapshot 'after-keep-restriction) states)
        (widen)
        (goto-char (point-min))
        (search-forward "Narrow target")
        (org-datetree-find-date-create '(6 2 2026) nil)
        (org-end-of-subtree t t)
        (insert "\n**** Property insert\n")
        (push (snapshot 'after-property-widen) states)
        (goto-char (point-min))
        (search-forward "Explicit target")
        (beginning-of-line)
        (let ((explicit-pos (point)))
          (org-datetree-find-month-create '(1 3 2027) 'subtree-at-point)
          (org-end-of-subtree t t)
          (insert "\n*** Explicit month insert\n")
          (goto-char explicit-pos)
          (org-datetree-find-date-create '(1 3 2027) 'subtree-at-point)
          (org-end-of-subtree t t)
          (insert "\n**** Explicit day insert\n"))
        (push (snapshot 'after-explicit-subtree) states)
        (goto-char (point-min))
        (search-forward "seed body")
        (let ((not-heading-error
               (condition-case err
                   (org-datetree-find-date-create '(7 4 2026)
                                                  'subtree-at-point)
                 (error (list (car err) (cadr err))))))
          (org-datetree-cleanup)
          (push (snapshot 'after-cleanup) states)
          (list (nreverse states)
                not-heading-error
                (count-matches "^\\*+ " (point-min) (point-max))
                 (buffer-substring-no-properties
                  (point-min) (point-max))))))))"#,
        expect,
    );
}

#[test]
fn org_datetree_narrow_cleanup_sort_timestamp_shift_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((initial ((1 \"Journal\" 1 282) (2 \"2026\" 44 282) (3 \"2026-05 May\" 52 282) (4 \"2026-05-27 Wednesday\" 68 196) (5 \"Morning\" 112 154) (5 \"Evening\" 154 196) (4 \"2026-05-28 Thursday\" 196 282) (5 \"Tomorrow\" 239 282) (1 \"Archive\" 282 292)) ((\"Journal\" 1 1) (\"2026\" 2 5) (\"2026-05 May\" 3 6) (\"2026-05-27 Wednesday\" 4 7) (\"Morning\" 5 10) (\"Evening\" 5 12) (\"2026-05-28 Thursday\" 4 14) (\"Tomorrow\" 5 17) (\"Archive\" 1 19)) \"* Journal\\n:PROPERTIES:\\n:DATE_TREE: t\\n:END:\\n** 2026\\n*** 2026-05 May\\n**** 2026-05-27 Wednesday\\n[2026-05-27 Wed]\\n\\n***** Morning\\nBody <2026-05-27 Wed 08:00>\\n***** Evening\\nBody <2026-05-27 Wed 20:00>\\n**** 2026-05-28 Thursday\\n[2026-05-28 Thu]\\n\\n***** Tomorrow\\nBody <2026-05-28 Thu 09:00>\\n* Archive\\n\") (after-shift-hidden-place ((1 \"Journal\" 1 283) (2 \"2026\" 44 283) (3 \"2026-05 May\" 52 283) (4 \"2026-05-27 Wednesday\" 68 154) (5 \"Morning\" 112 154) (4 \"2026-05-28 Thursday\" 154 283) (5 \"Tomorrow\" 197 240) (5 \"Evening\" 240 283) (1 \"Archive\" 283 293)) ((\"Journal\" 1 1) (\"2026\" 2 5) (\"2026-05 May\" 3 6) (\"2026-05-27 Wednesday\" 4 7) (\"Morning\" 5 10) (\"Evening\" 5 17) (\"2026-05-28 Thursday\" 4 12) (\"Tomorrow\" 5 15) (\"Archive\" 1 20)) \"* Journal\\n:PROPERTIES:\\n:DATE_TREE: t\\n:END:\\n** 2026\\n*** 2026-05 May\\n**** 2026-05-27 Wednesday\\n[2026-05-27 Wed]\\n\\n***** Morning\\nBody <2026-05-27 Wed 08:00>\\n**** 2026-05-28 Thursday\\n[2026-05-28 Thu]\\n\\n***** Tomorrow\\nBody <2026-05-28 Thu 09:00>\\n***** Evening\\nBody <2026-05-26 Tue 20:00>\\n\\n* Archive\\n\") (after-cleanup ((1 \"Journal\" 1 325) (2 \"2026\" 44 325) (3 \"2026-05 May\" 52 325) (4 \"2026-05-26 Tuesday\" 69 153) (5 \"Evening\" 110 153) (4 \"2026-05-27 Wednesday\" 153 239) (5 \"Morning\" 197 239) (4 \"2026-05-28 Thursday\" 239 325) (5 \"Tomorrow\" 282 325) (1 \"Archive\" 325 335)) ((\"Journal\" 1 1) (\"2026\" 2 5) (\"2026-05 May\" 3 6) (\"2026-05-27 Wednesday\" 4 13) (\"Morning\" 5 16) (\"Evening\" 5 10) (\"2026-05-28 Thursday\" 4 18) (\"Tomorrow\" 5 21) (\"Archive\" 1 23)) \"* Journal\\n:PROPERTIES:\\n:DATE_TREE: t\\n:END:\\n** 2026\\n*** 2026-05 May\\n\\n**** 2026-05-26 Tuesday\\n[2026-05-26 Tue]\\n***** Evening\\nBody <2026-05-26 Tue 20:00>\\n\\n**** 2026-05-27 Wednesday\\n[2026-05-27 Wed]\\n\\n***** Morning\\nBody <2026-05-27 Wed 08:00>\\n**** 2026-05-28 Thursday\\n[2026-05-28 Thu]\\n\\n***** Tomorrow\\nBody <2026-05-28 Thu 09:00>\\n* Archive\\n\") (after-sort-day ((1 \"Journal\" 1 325) (2 \"2026\" 44 325) (3 \"2026-05 May\" 52 325) (4 \"2026-05-26 Tuesday\" 69 153) (5 \"Evening\" 110 153) (4 \"2026-05-27 Wednesday\" 153 239) (5 \"Morning\" 197 239) (4 \"2026-05-28 Thursday\" 239 325) (5 \"Tomorrow\" 282 325) (1 \"Archive\" 325 335)) ((\"Journal\" 1 1) (\"2026\" 2 5) (\"2026-05 May\" 3 6) (\"2026-05-27 Wednesday\" 4 13) (\"Morning\" 5 16) (\"Evening\" 5 10) (\"2026-05-28 Thursday\" 4 18) (\"Tomorrow\" 5 21) (\"Archive\" 1 23)) \"* Journal\\n:PROPERTIES:\\n:DATE_TREE: t\\n:END:\\n** 2026\\n*** 2026-05 May\\n\\n**** 2026-05-26 Tuesday\\n[2026-05-26 Tue]\\n***** Evening\\nBody <2026-05-26 Tue 20:00>\\n\\n**** 2026-05-27 Wednesday\\n[2026-05-27 Wed]\\n\\n***** Morning\\nBody <2026-05-27 Wed 08:00>\\n**** 2026-05-28 Thursday\\n[2026-05-28 Thu]\\n\\n***** Tomorrow\\nBody <2026-05-28 Thu 09:00>\\n* Archive\\n\") (after-copy-archive ((1 \"Journal\" 1 364) (2 \"2026\" 44 325) (3 \"2026-05 May\" 52 325) (4 \"2026-05-26 Tuesday\" 69 153) (5 \"Evening\" 110 153) (4 \"2026-05-27 Wednesday\" 153 239) (5 \"Morning\" 197 239) (4 \"2026-05-28 Thursday\" 239 325) (5 \"Tomorrow\" 282 325) (2 \"Morning\" 325 364) (1 \"Archive\" 364 374)) ((\"Journal\" 1 1) (\"2026\" 2 5) (\"2026-05 May\" 3 6) (\"2026-05-27 Wednesday\" 4 13) (\"Morning\" 5 16) (\"Evening\" 5 10) (\"2026-05-28 Thursday\" 4 18) (\"Tomorrow\" 5 21) (\"Archive\" 1 25)) \"* Journal\\n:PROPERTIES:\\n:DATE_TREE: t\\n:END:\\n** 2026\\n*** 2026-05 May\\n\\n**** 2026-05-26 Tuesday\\n[2026-05-26 Tue]\\n***** Evening\\nBody <2026-05-26 Tue 20:00>\\n\\n**** 2026-05-27 Wednesday\\n[2026-05-27 Wed]\\n\\n***** Morning\\nBody <2026-05-27 Wed 08:00>\\n**** 2026-05-28 Thursday\\n[2026-05-28 Thu]\\n\\n***** Tomorrow\\nBody <2026-05-28 Thu 09:00>\\n** Morning\\nBody <2026-05-27 Wed 08:00>\\n* Archive\\n\")) 11 ((\"2026-05-26 Tuesday\" 4 \"**** 2026-05-26 Tuesday\") (\"2026-05-27 Wednesday\" 4 \"**** 2026-05-27 Wednesday\") (\"2026-05-28 Thursday\" 4 \"**** 2026-05-28 Thursday\") (\"** Morning\" 5 \"***** Morning\")) \"* Journal\\n:PROPERTIES:\\n:DATE_TREE: t\\n:END:\\n** 2026\\n*** 2026-05 May\\n\\n**** 2026-05-26 Tuesday\\n[2026-05-26 Tue]\\n***** Evening\\nBody <2026-05-26 Tue 20:00>\\n\\n**** 2026-05-27 Wednesday\\n[2026-05-27 Wed]\\n\\n***** Morning\\nBody <2026-05-27 Wed 08:00>\\n**** 2026-05-28 Thursday\\n[2026-05-28 Thu]\\n\\n***** Tomorrow\\nBody <2026-05-28 Thu 09:00>\\n** Morning\\nBody <2026-05-27 Wed 08:00>\\n* Archive\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-datetree)
  (with-temp-buffer
    (let ((org-datetree-add-timestamp 'inactive)
          states)
      (org-mode)
      (insert "* Journal\n:PROPERTIES:\n:DATE_TREE: t\n:END:\n")
      (insert "* Archive\n")
      (goto-char (point-min))
      (search-forward "Journal")
      (beginning-of-line)
      (org-datetree-file-entry-under "* Morning\nBody <2026-05-27 Wed 08:00>\n" '(5 27 2026))
      (goto-char (point-min))
      (search-forward "Journal")
      (beginning-of-line)
      (org-datetree-file-entry-under "* Evening\nBody <2026-05-27 Wed 20:00>\n" '(5 27 2026))
      (goto-char (point-min))
      (search-forward "Journal")
      (beginning-of-line)
      (org-datetree-file-entry-under "* Tomorrow\nBody <2026-05-28 Thu 09:00>\n" '(5 28 2026))
      (let ((snapshot
             (lambda (label)
               (let (heads)
                 (org-element-map (org-element-parse-buffer) 'headline
                   (lambda (headline)
                     (push (list (org-element-property :level headline)
                                 (org-element-property :raw-value headline)
                                 (org-element-property :begin headline)
                                 (org-element-property :end headline))
                           heads)))
                 (list label
                       (nreverse heads)
                       (mapcar (lambda (needle)
                                 (save-excursion
                                   (goto-char (point-min))
                                   (search-forward needle)
                                   (list needle
                                         (org-outline-level)
                                         (line-number-at-pos))))
                               '("Journal" "2026" "2026-05 May"
                                 "2026-05-27 Wednesday"
                                 "Morning" "Evening"
                                 "2026-05-28 Thursday" "Tomorrow"
                                 "Archive"))
                       (buffer-substring-no-properties
                        (point-min) (point-max)))))))
        (push (funcall snapshot 'initial) states)
        (goto-char (point-min))
        (search-forward "Journal")
        (org-narrow-to-subtree)
        (goto-char (point-min))
        (search-forward "Evening")
        (beginning-of-line)
        (org-cut-subtree)
        (goto-char (point-max))
        (org-paste-subtree 5)
        (search-forward "2026-05-27 Wed 20:00")
        (org-timestamp-down-day 1)
        (widen)
        (push (funcall snapshot 'after-shift-hidden-place) states)
        (org-datetree-cleanup)
        (push (funcall snapshot 'after-cleanup) states)
        (goto-char (point-min))
        (search-forward "2026-05-27 Wednesday")
        (beginning-of-line)
        (org-sort-entries nil ?a)
        (push (funcall snapshot 'after-sort-day) states)
        (goto-char (point-min))
        (search-forward "Morning")
        (beginning-of-line)
        (org-copy-subtree)
        (goto-char (point-min))
        (search-forward "Archive")
        (beginning-of-line)
        (org-paste-subtree 2)
        (push (funcall snapshot 'after-copy-archive) states)
        (list (nreverse states)
              (count-matches "^\\*+ " (point-min) (point-max))
              (mapcar (lambda (needle)
                        (save-excursion
                          (goto-char (point-min))
                          (search-forward needle)
                          (list needle
                                (org-outline-level)
                                (buffer-substring-no-properties
                                 (line-beginning-position)
                                 (line-end-position)))))
                      '("2026-05-26 Tuesday"
                        "2026-05-27 Wednesday"
                        "2026-05-28 Thursday"
                        "** Morning"))
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"#,
        expect,
    );
}

#[test]
fn org_datetree_insert_find_cleanup_structure_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument fixnump nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-datetree)
  (with-temp-buffer
    (org-mode)
    ;; Insert entries for multiple dates
    (org-datetree-file-entry-under "* Meeting" '(5 27 2026))
    (org-datetree-file-entry-under "* Standup" '(5 27 2026))
    (org-datetree-file-entry-under "* Review" '(5 28 2026))
    (org-datetree-file-entry-under "* Sprint start" '(5 1 2026))
    (org-datetree-file-entry-under "* Retro" '(4 30 2026))
    (let* ((after-insert (buffer-substring-no-properties
                          (point-min) (point-max)))
           ;; Parse datetree structure
           (tree (org-element-parse-buffer))
           (headlines
            (org-element-map tree 'headline
              (lambda (h)
                (list (org-element-property :level h)
                      (org-element-property :raw-value h)))))
           ;; Count year/month/day headings
           (year-count (count-matches "^\\* [0-9]\\{4\\}" (point-min) (point-max)))
           (month-count (count-matches "^\\*\\* [0-9]\\{4\\}-[0-9]\\{2\\}" (point-min) (point-max)))
           (day-count (count-matches "^\\*\\*\\* [0-9]\\{4\\}-[0-9]\\{2\\}-[0-9]\\{2\\}" (point-min) (point-max))))
      ;; Find and navigate to a date
      (goto-char (point-min))
      (org-datetree-find-date-create '(5 27 2026))
      (let ((pos-after-find (line-number-at-pos))
            (heading-at-find (org-get-heading t t t t)))
        ;; Insert under found date
        (org-end-of-subtree)
        (insert "\n*** Extra entry\nExtra body.\n")
        (let ((after-extra (buffer-substring-no-properties
                            (point-min) (point-max))))
          ;; Find month
          (goto-char (point-min))
          (org-datetree-find-month-create '(5 2026))
          (let ((month-pos (line-number-at-pos))
                (month-heading (org-get-heading t t t t)))
             (list after-insert
                   headlines
                   year-count
                   month-count
                   day-count
                   pos-after-find
                   heading-at-find
                   after-extra
                   month-pos
                   month-heading
                   (count-matches "^\\*+ " (point-min) (point-max))
                   (buffer-substring-no-properties
                    (point-min) (point-max)))))))))"##,
        expect,
    );
}

#[test]
fn org_datetree_find_insert_edit_multi_date_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"2026\" 1) (\"2026-05 May\" 2) (\"2026-05-27 Wednesday\" 3) (\"Extra under May 27\" 2) (\"2026-05-28 Thursday\" 3) (\"2026-06 June\" 2) (\"2026-06-01 Monday\" 3)) 7 7 \"2026-05-28 Thursday\" \"\\n* 2026\\n** 2026-05 May\\nEntry for May 27.\\n*** 2026-05-27 Wednesday\\nEntry for May 28.\\n*** 2026-05-28 Thursday\\n\\n** Extra under May 27\\nExtra body.\\n\\n*** 2026-05-28 Thursday\\n** 2026-06 June\\nEntry for June 1.\\n*** 2026-06-01 Monday\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-datetree)
  (with-temp-buffer
    (org-mode)
    (org-datetree-find-date-create '(5 27 2026))
    (insert "Entry for May 27.\n")
    (org-datetree-find-date-create '(5 28 2026))
    (insert "Entry for May 28.\n")
    (org-datetree-find-date-create '(6 1 2026))
    (insert "Entry for June 1.\n")
    ;; Insert under May 27 again
    (goto-char (point-min))
    (org-datetree-find-date-create '(5 27 2026))
    (org-end-of-subtree)
    (insert "\n** Extra under May 27\nExtra body.\n")
    ;; Parse structure
    (let ((headlines
           (org-element-map (org-element-parse-buffer) 'headline
             (lambda (hl)
               (list (org-element-property :raw-value hl)
                     (org-element-property :level hl)))))
          (heading-count (count-matches "^\\*+ " (point-min) (point-max))))
      ;; Find date and check position
      (goto-char (point-min))
      (org-datetree-find-date-create '(5 28 2026))
      (let ((found-pos (line-number-at-pos))
            (found-heading (org-get-heading t t t t)))
        (list headlines
              heading-count
              found-pos
              found-heading
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}
