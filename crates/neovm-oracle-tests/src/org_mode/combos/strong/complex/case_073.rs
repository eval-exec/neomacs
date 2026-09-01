//! Strong combo-complex-73/74 oracle tests — esoteric probes:
//! org-submit-bug-report, org-element with org-element-parse-
//! secondary-string nested, org-table with org-table-rotate-
//! recalculate-marks, org-agenda with filter-preset interactions,
//! org-babel with ob-java/ob-js/ob-julia availability,
//! org-export with org-export-data-for-backend, org-persist
//! with gc cycle, org-habit with org-habit-build-graph,
//! and org-compat with org-with-point-at boundary.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn combo73_submit_bug_report_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:submit-fbound t :version-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (list
   :submit-fbound (fboundp 'org-submit-bug-report)
   :version-fbound (fboundp 'org-version)
   ))"##,
        expect,
    );
}

#[test]
fn combo73_element_parse_secondary_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:result-type cons :has-bold ((bold (:standard-properties [8 nil 9 23 25 1 nil nil nil nil nil nil nil nil #<buffer  *Org parse*> nil nil (#(\"nested \" 0 7 (:parent (#(\"nested \" 0 7 (:parent #8)) (bold (:standard-properties [8 nil 9 23 25 1 nil nil nil nil nil nil nil nil #<buffer  *Org parse*> nil nil #8]) #(\"bold \" 0 5 (:parent #9)) (italic (:standard-properties [14 nil 15 22 23 0 nil nil nil nil nil nil nil nil #<buffer  *Org parse*> nil nil #9]) #(\"italic!\" 0 7 (:parent #10)))) #(\"end\" 0 3 (:parent #8))))) #2 #(\"end\" 0 3 (:parent (#(\"nested \" 0 7 (:parent #8)) (bold (:standard-properties [8 nil 9 23 25 1 nil nil nil nil nil nil nil nil #<buffer  *Org parse*> nil nil #8]) #(\"bold \" 0 5 (:parent #9)) (italic (:standard-properties [14 nil 15 22 23 0 nil nil nil nil nil nil nil nil #<buffer  *Org parse*> nil nil #9]) #(\"italic!\" 0 7 (:parent #10)))) #(\"end\" 0 3 (:parent #8))))))]) #(\"bold \" 0 5 (:parent (bold (:standard-properties [8 nil 9 23 25 1 nil nil nil nil nil nil nil nil #<buffer  *Org parse*> nil nil (#(\"nested \" 0 7 (:parent #8)) #5 #(\"end\" 0 3 (:parent #8)))]) #(\"bold \" 0 5 (:parent #5)) (italic (:standard-properties [14 nil 15 22 23 0 nil nil nil nil nil nil nil nil #<buffer  *Org parse*> nil nil #5]) #(\"italic!\" 0 7 (:parent #6)))))) (italic (:standard-properties [14 nil 15 22 23 0 nil nil nil nil nil nil nil nil #<buffer  *Org parse*> nil nil #2]) #(\"italic!\" 0 7 (:parent (italic (:standard-properties [14 nil 15 22 23 0 nil nil nil nil nil nil nil nil #<buffer  *Org parse*> nil nil (bold (:standard-properties [8 nil 9 23 25 1 nil nil nil nil nil nil nil nil #<buffer  *Org parse*> nil nil (#(\"nested \" 0 7 (:parent #12)) #9 #(\"end\" 0 3 (:parent #12)))]) #(\"bold \" 0 5 (:parent #9)) #6)]) #(\"italic!\" 0 7 (:parent #6)))))))) :has-italic ((italic (:standard-properties [14 nil 15 22 23 0 nil nil nil nil nil nil nil nil #<buffer  *Org parse*> nil nil (bold (:standard-properties [8 nil 9 23 25 1 nil nil nil nil nil nil nil nil #<buffer  *Org parse*> nil nil (#(\"nested \" 0 7 (:parent (#(\"nested \" 0 7 (:parent #11)) (bold (:standard-properties [8 nil 9 23 25 1 nil nil nil nil nil nil nil nil #<buffer  *Org parse*> nil nil #11]) #(\"bold \" 0 5 (:parent #12)) (italic (:standard-properties [14 nil 15 22 23 0 nil nil nil nil nil nil nil nil #<buffer  *Org parse*> nil nil #12]) #(\"italic!\" 0 7 (:parent #13)))) #(\"end\" 0 3 (:parent #11))))) #5 #(\"end\" 0 3 (:parent (#(\"nested \" 0 7 (:parent #11)) (bold (:standard-properties [8 nil 9 23 25 1 nil nil nil nil nil nil nil nil #<buffer  *Org parse*> nil nil #11]) #(\"bold \" 0 5 (:parent #12)) (italic (:standard-properties [14 nil 15 22 23 0 nil nil nil nil nil nil nil nil #<buffer  *Org parse*> nil nil #12]) #(\"italic!\" 0 7 (:parent #13)))) #(\"end\" 0 3 (:parent #11))))))]) #(\"bold \" 0 5 (:parent (bold (:standard-properties [8 nil 9 23 25 1 nil nil nil nil nil nil nil nil #<buffer  *Org parse*> nil nil (#(\"nested \" 0 7 (:parent #11)) #8 #(\"end\" 0 3 (:parent #11)))]) #(\"bold \" 0 5 (:parent #8)) (italic (:standard-properties [14 nil 15 22 23 0 nil nil nil nil nil nil nil nil #<buffer  *Org parse*> nil nil #8]) #(\"italic!\" 0 7 (:parent #9)))))) #2)]) #(\"italic!\" 0 7 (:parent (italic (:standard-properties [14 nil 15 22 23 0 nil nil nil nil nil nil nil nil #<buffer  *Org parse*> nil nil (bold (:standard-properties [8 nil 9 23 25 1 nil nil nil nil nil nil nil nil #<buffer  *Org parse*> nil nil (#(\"nested \" 0 7 (:parent #11)) #8 #(\"end\" 0 3 (:parent #11)))]) #(\"bold \" 0 5 (:parent #8)) #5)]) #(\"italic!\" 0 7 (:parent #5))))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  ;; parse-secondary-string with nested bold-inside-italic spec
  (let ((result (org-element-parse-secondary-string
                  "nested *bold /italic!/* end" '(bold italic))))
    (list
     :result-type (type-of result)
     :has-bold (org-element-map result 'bold #'identity)
     :has-italic (org-element-map result 'italic #'identity)
     )))"##,
        expect,
    );
}

#[test]
fn combo73_table_rotate_recalc_marks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:rotate-fbound nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (list
   :rotate-fbound (fboundp 'org-table-rotate-recalculate-marks)
   ))"##,
        expect,
    );
}

#[test]
fn combo73_agenda_filter_preset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:filter-preset-bound t :filter-fbound t :filter-category-fbound t :filter-effort-fbound t :top-headline-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda)
  (list
   :filter-preset-bound (boundp 'org-agenda-filter-preset)
   :filter-fbound (fboundp 'org-agenda-filter-by-tag)
   :filter-category-fbound (fboundp 'org-agenda-filter-by-category)
   :filter-effort-fbound (fboundp 'org-agenda-filter-by-effort)
   :top-headline-fbound (fboundp 'org-agenda-filter-by-top-headline)
   ))"##,
        expect,
    );
}

#[test]
fn combo73_babel_esoteric_langs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ob-java ob-java :ob-js ob-js :ob-julia ob-julia :ob-sed ob-sed :ob-screen ob-screen)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (list
   :ob-java (condition-case nil (require 'ob-java) (error (featurep 'ob-java)))
   :ob-js (condition-case nil (require 'ob-js) (error (featurep 'ob-js)))
   :ob-julia (condition-case nil (require 'ob-julia) (error (featurep 'ob-julia)))
   :ob-sed (condition-case nil (require 'ob-sed) (error (featurep 'ob-sed)))
   :ob-screen (condition-case nil (require 'ob-screen) (error (featurep 'ob-screen)))
   ))"##,
        expect,
    );
}

#[test]
fn combo73_export_data_for_backend() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:export-string-fbound t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ox) (require 'ox-ascii)
  (insert "*bold* /italic/.\n")
  (let* ((tree (org-element-parse-buffer))
         (info (org-export-get-environment))
         (para (car (org-element-map tree 'paragraph #'identity)))
         (r '()))
    (push (list :export-string-fbound (fboundp 'org-export-string-as)) r)
    ;; org-export-data-for-backend
    (condition-case nil
        (let ((out (when (fboundp 'org-export-data-with-backend)
                     (org-export-data-with-backend para info 'ascii))))
          (push (list :data-ok (and out (stringp out))) r))
      (error nil))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo73_persist_gc_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:gc-fbound t :read-fbound t :write-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-persist)
  (list
   :gc-fbound (fboundp 'org-persist-gc)
   :read-fbound (fboundp 'org-persist-read)
   :write-fbound (fboundp 'org-persist-write)
   ))"##,
        expect,
    );
}

#[test]
fn combo73_habit_build_graph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:build-graph-fbound t :parse-todo-fbound t :is-habit-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-habit)
  (list
   :build-graph-fbound (fboundp 'org-habit-build-graph)
   :parse-todo-fbound (fboundp 'org-habit-parse-todo)
   :is-habit-fbound (fboundp 'org-is-habit-p)
   ))"##,
        expect,
    );
}

#[test]
fn combo73_compat_with_point_at() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:with-point-at-fbound t :with-silent-fbound t :with-wide-buffer-fbound t :format-time-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-compat)
  (list
   :with-point-at-fbound (fboundp 'org-with-point-at)
   :with-silent-fbound (fboundp 'org-with-silent-modifications)
   :with-wide-buffer-fbound (fboundp 'org-with-wide-buffer)
   :format-time-fbound (fboundp 'org-format-time-string)
   ))"##,
        expect,
    );
}

#[test]
fn combo73_org_agenda_time_of_day() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:time-of-day-fbound t :format-item-fbound t :add-time-grid-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-agenda)
  (list
   :time-of-day-fbound (fboundp 'org-agenda-time-of-day-to-ampm)
   :format-item-fbound (fboundp 'org-agenda-format-item)
   :add-time-grid-fbound (fboundp 'org-agenda-add-time-grid-maybe)
   ))"##,
        expect,
    );
}
