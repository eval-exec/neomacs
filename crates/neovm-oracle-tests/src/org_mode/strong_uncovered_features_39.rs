//! Strong uncovered-features-39 oracle tests — org-agenda views, org-columns.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-list
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf39_agenda_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"Week-agenda (W25):\\nMonday     15 June 2026 W25\\nTuesday    16 June 2026\\nWednesday  17 June 2026\\nThursday   18 June 2026\\nFriday     19 June 2026\\nSaturday   20 June 2026\\nSunday     21 June 2026\\n\" 0 18 (org-series-cmd nil org-redo-cmd (org-agenda-list 'nil nil 'week nil) org-last-args (nil nil week) org-agenda-type agenda org-agenda-structural-header t org-date-line t face org-agenda-structure) 18 19 (org-series-cmd nil org-redo-cmd (org-agenda-list 'nil nil 'week nil) org-last-args (nil nil week) org-agenda-type agenda) 19 46 (org-series-cmd nil org-redo-cmd (org-agenda-list 'nil nil 'week nil) org-last-args (nil nil week) org-agenda-type agenda day 739782 org-today t org-day-cnt 1 org-agenda-date-header t org-date-line t face org-agenda-date-today) 46 47 (org-series-cmd nil org-redo-cmd (org-agenda-list 'nil nil 'week nil) org-last-args (nil nil week) org-agenda-type agenda) 47 70 (org-series-cmd nil org-redo-cmd (org-agenda-list 'nil nil 'week nil) org-last-args (nil nil week) org-agenda-type agenda day 739783 org-day-cnt 2 org-agenda-date-header t org-date-line t face org-agenda-date) 70 71 (org-series-cmd nil org-redo-cmd (org-agenda-list 'nil nil 'week nil) org-last-args (nil nil week) org-agenda-type agenda) 71 94 (org-series-cmd nil org-redo-cmd (org-agenda-list 'nil nil 'week nil) org-last-args (nil nil week) org-agenda-type agenda day 739784 org-day-cnt 3 org-agenda-date-header t org-date-line t face org-agenda-date) 94 95 (org-series-cmd nil org-redo-cmd (org-agenda-list 'nil nil 'week nil) org-last-args (nil nil week) org-agenda-type agenda) 95 118 (org-series-cmd nil org-redo-cmd (org-agenda-list 'nil nil 'week nil) org-last-args (nil nil week) org-agenda-type agenda day 739785 org-day-cnt 4 org-agenda-date-header t org-date-line t face org-agenda-date) 118 119 (org-series-cmd nil org-redo-cmd (org-agenda-list 'nil nil 'week nil) org-last-args (nil nil week) org-agenda-type agenda) 119 142 (org-series-cmd nil org-redo-cmd (org-agenda-list 'nil nil 'week nil) org-last-args (nil nil week) org-agenda-type agenda day 739786 org-day-cnt 5 org-agenda-date-header t org-date-line t face org-agenda-date) 142 143 (org-series-cmd nil org-redo-cmd (org-agenda-list 'nil nil 'week nil) org-last-args (nil nil week) org-agenda-type agenda) 143 166 (org-series-cmd nil org-redo-cmd (org-agenda-list 'nil nil 'week nil) org-last-args (nil nil week) org-agenda-type agenda day 739787 org-day-cnt 6 org-agenda-date-header t org-date-line t face org-agenda-date-weekend) 166 167 (org-series-cmd nil org-redo-cmd (org-agenda-list 'nil nil 'week nil) org-last-args (nil nil week) org-agenda-type agenda) 167 190 (org-series-cmd nil org-redo-cmd (org-agenda-list 'nil nil 'week nil) org-last-args (nil nil week) org-agenda-type agenda day 739788 org-day-cnt 7 org-agenda-date-header t org-date-line t face org-agenda-date-weekend) 190 191 (org-series-cmd nil org-redo-cmd (org-agenda-list 'nil nil 'week nil) org-last-args (nil nil week) org-agenda-type agenda))""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T1\nSCHEDULED: <2026-01-15>\n* DONE D1\n* TODO T2\nDEADLINE: <2026-01-20>")
  (condition-case nil
      (org-agenda-list)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-todo-list
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf39_todo_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"Global list of TODO items of type: ALL\\nPress ‘N r’ (e.g. ‘0 r’) to search again: (0)[ALL]\\n\" 0 34 (org-series-cmd nil org-redo-cmd (org-todo-list (or (and (numberp current-prefix-arg) current-prefix-arg) nil current-prefix-arg nil)) org-last-args nil org-agenda-type todo org-agenda-structural-header t short-heading \"ToDo: ALL\" face org-agenda-structure) 34 35 (org-series-cmd nil org-redo-cmd (org-todo-list (or (and (numberp current-prefix-arg) current-prefix-arg) nil current-prefix-arg nil)) org-last-args nil org-agenda-type todo org-agenda-structural-header t) 35 38 (org-series-cmd nil org-redo-cmd (org-todo-list (or (and (numberp current-prefix-arg) current-prefix-arg) nil current-prefix-arg nil)) org-last-args nil org-agenda-type todo org-agenda-structural-header t face org-agenda-structure-filter) 38 39 (org-series-cmd nil org-redo-cmd (org-todo-list (or (and (numberp current-prefix-arg) current-prefix-arg) nil current-prefix-arg nil)) org-last-args nil org-agenda-type todo) 39 48 (org-series-cmd nil org-redo-cmd (org-todo-list (or (and (numberp current-prefix-arg) current-prefix-arg) nil current-prefix-arg nil)) org-last-args nil org-agenda-type todo face org-agenda-structure-secondary) 48 49 (org-series-cmd nil org-redo-cmd (org-todo-list (or (and (numberp current-prefix-arg) current-prefix-arg) nil current-prefix-arg nil)) org-last-args nil org-agenda-type todo font-lock-face help-key-binding face org-agenda-structure-secondary) 49 50 (org-series-cmd nil org-redo-cmd (org-todo-list (or (and (numberp current-prefix-arg) current-prefix-arg) nil current-prefix-arg nil)) org-last-args nil org-agenda-type todo face org-agenda-structure-secondary) 50 57 (org-series-cmd nil org-redo-cmd (org-todo-list (or (and (numberp current-prefix-arg) current-prefix-arg) nil current-prefix-arg nil)) org-last-args nil org-agenda-type todo face org-agenda-structure-secondary) 57 58 (org-series-cmd nil org-redo-cmd (org-todo-list (or (and (numberp current-prefix-arg) current-prefix-arg) nil current-prefix-arg nil)) org-last-args nil org-agenda-type todo face org-agenda-structure-secondary) 58 60 (org-series-cmd nil org-redo-cmd (org-todo-list (or (and (numberp current-prefix-arg) current-prefix-arg) nil current-prefix-arg nil)) org-last-args nil org-agenda-type todo face org-agenda-structure-secondary) 60 61 (org-series-cmd nil org-redo-cmd (org-todo-list (or (and (numberp current-prefix-arg) current-prefix-arg) nil current-prefix-arg nil)) org-last-args nil org-agenda-type todo font-lock-face help-key-binding face org-agenda-structure-secondary) 61 62 (org-series-cmd nil org-redo-cmd (org-todo-list (or (and (numberp current-prefix-arg) current-prefix-arg) nil current-prefix-arg nil)) org-last-args nil org-agenda-type todo face org-agenda-structure-secondary) 62 89 (org-series-cmd nil org-redo-cmd (org-todo-list (or (and (numberp current-prefix-arg) current-prefix-arg) nil current-prefix-arg nil)) org-last-args nil org-agenda-type todo face org-agenda-structure-secondary) 89 90 (org-series-cmd nil org-redo-cmd (org-todo-list (or (and (numberp current-prefix-arg) current-prefix-arg) nil current-prefix-arg nil)) org-last-args nil org-agenda-type todo))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T1\n* DONE D1\n* TODO T2\n* WAITING W1")
  (condition-case nil
      (org-todo-list)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-tags-view
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf39_tags_view() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"Headlines with TAGS match: work\\nPress ‘C-u r’ to search again\\n\" 0 26 (org-series-cmd nil org-redo-cmd (org-tags-view 'nil (if current-prefix-arg nil \"work\")) org-last-args (nil \"work\") org-agenda-type tags org-agenda-structural-header t short-heading \"Match: work\" face org-agenda-structure) 26 27 (org-series-cmd nil org-redo-cmd (org-tags-view 'nil (if current-prefix-arg nil \"work\")) org-last-args (nil \"work\") org-agenda-type tags org-agenda-structural-header t) 27 31 (org-series-cmd nil org-redo-cmd (org-tags-view 'nil (if current-prefix-arg nil \"work\")) org-last-args (nil \"work\") org-agenda-type tags org-agenda-structural-header t face org-agenda-structure-filter) 31 32 (org-series-cmd nil org-redo-cmd (org-tags-view 'nil (if current-prefix-arg nil \"work\")) org-last-args (nil \"work\") org-agenda-type tags) 32 39 (org-series-cmd nil org-redo-cmd (org-tags-view 'nil (if current-prefix-arg nil \"work\")) org-last-args (nil \"work\") org-agenda-type tags face org-agenda-structure-secondary) 39 42 (org-series-cmd nil org-redo-cmd (org-tags-view 'nil (if current-prefix-arg nil \"work\")) org-last-args (nil \"work\") org-agenda-type tags font-lock-face help-key-binding face org-agenda-structure-secondary) 42 43 (org-series-cmd nil org-redo-cmd (org-tags-view 'nil (if current-prefix-arg nil \"work\")) org-last-args (nil \"work\") org-agenda-type tags face org-agenda-structure-secondary) 43 44 (org-series-cmd nil org-redo-cmd (org-tags-view 'nil (if current-prefix-arg nil \"work\")) org-last-args (nil \"work\") org-agenda-type tags font-lock-face help-key-binding face org-agenda-structure-secondary) 44 45 (org-series-cmd nil org-redo-cmd (org-tags-view 'nil (if current-prefix-arg nil \"work\")) org-last-args (nil \"work\") org-agenda-type tags face org-agenda-structure-secondary) 45 61 (org-series-cmd nil org-redo-cmd (org-tags-view 'nil (if current-prefix-arg nil \"work\")) org-last-args (nil \"work\") org-agenda-type tags face org-agenda-structure-secondary) 61 62 (org-series-cmd nil org-redo-cmd (org-tags-view 'nil (if current-prefix-arg nil \"work\")) org-last-args (nil \"work\") org-agenda-type tags))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T1 :work:\n* T2 :home:\n* T3 :work:")
  (condition-case nil
      (org-tags-view nil "work")
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-search-view
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf39_search_view() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"Search words: keyword\\nPress ‘[’, ‘]’ to add/sub word, ‘{’, ‘}’ to add/sub regexp, ‘C-u r’ for a fresh search\\n\" 0 13 (org-series-cmd nil org-redo-cmd (org-search-view nil (if current-prefix-arg nil \"keyword\")) org-last-args (nil \"keyword\" nil) org-agenda-type search org-agenda-structural-header t face org-agenda-structure) 13 14 (org-series-cmd nil org-redo-cmd (org-search-view nil (if current-prefix-arg nil \"keyword\")) org-last-args (nil \"keyword\" nil) org-agenda-type search org-agenda-structural-header t) 14 21 (org-series-cmd nil org-redo-cmd (org-search-view nil (if current-prefix-arg nil \"keyword\")) org-last-args (nil \"keyword\" nil) org-agenda-type search org-agenda-structural-header t face org-agenda-structure-filter) 21 22 (org-series-cmd nil org-redo-cmd (org-search-view nil (if current-prefix-arg nil \"keyword\")) org-last-args (nil \"keyword\" nil) org-agenda-type search) 22 29 (org-series-cmd nil org-redo-cmd (org-search-view nil (if current-prefix-arg nil \"keyword\")) org-last-args (nil \"keyword\" nil) org-agenda-type search face org-agenda-structure-secondary) 29 30 (org-series-cmd nil org-redo-cmd (org-search-view nil (if current-prefix-arg nil \"keyword\")) org-last-args (nil \"keyword\" nil) org-agenda-type search font-lock-face help-key-binding face org-agenda-structure-secondary) 30 31 (org-series-cmd nil org-redo-cmd (org-search-view nil (if current-prefix-arg nil \"keyword\")) org-last-args (nil \"keyword\" nil) org-agenda-type search face org-agenda-structure-secondary) 31 33 (org-series-cmd nil org-redo-cmd (org-search-view nil (if current-prefix-arg nil \"keyword\")) org-last-args (nil \"keyword\" nil) org-agenda-type search face org-agenda-structure-secondary) 33 34 (org-series-cmd nil org-redo-cmd (org-search-view nil (if current-prefix-arg nil \"keyword\")) org-last-args (nil \"keyword\" nil) org-agenda-type search face org-agenda-structure-secondary) 34 35 (org-series-cmd nil org-redo-cmd (org-search-view nil (if current-prefix-arg nil \"keyword\")) org-last-args (nil \"keyword\" nil) org-agenda-type search font-lock-face help-key-binding face org-agenda-structure-secondary) 35 36 (org-series-cmd nil org-redo-cmd (org-search-view nil (if current-prefix-arg nil \"keyword\")) org-last-args (nil \"keyword\" nil) org-agenda-type search face org-agenda-structure-secondary) 36 54 (org-series-cmd nil org-redo-cmd (org-search-view nil (if current-prefix-arg nil \"keyword\")) org-last-args (nil \"keyword\" nil) org-agenda-type search face org-agenda-structure-secondary) 54 55 (org-series-cmd nil org-redo-cmd (org-search-view nil (if current-prefix-arg nil \"keyword\")) org-last-args (nil \"keyword\" nil) org-agenda-type search face org-agenda-structure-secondary) 55 56 (org-series-cmd nil org-redo-cmd (org-search-view nil (if current-prefix-arg nil \"keyword\")) org-last-args (nil \"keyword\" nil) org-agenda-type search font-lock-face help-key-binding face org-agenda-structure-secondary) 56 57 (org-series-cmd nil org-redo-cmd (org-search-view nil (if current-prefix-arg nil \"keyword\")) org-last-args (nil \"keyword\" nil) org-agenda-type search face org-agenda-structure-secondary) 57 59 (org-series-cmd nil org-redo-cmd (org-search-view nil (if current-prefix-arg nil \"keyword\")) org-last-args (nil \"keyword\" nil) org-agenda-type search face org-agenda-structure-secondary) 59 60 (org-series-cmd nil org-redo-cmd (org-search-view nil (if current-prefix-arg nil \"keyword\")) org-last-args (nil \"keyword\" nil) org-agenda-type search face org-agenda-structure-secondary) 60 61 (org-series-cmd nil org-redo-cmd (org-search-view nil (if current-prefix-arg nil \"keyword\")) org-last-args (nil \"keyword\" nil) org-agenda-type search font-lock-face help-key-binding face org-agenda-structure-secondary) 61 62 (org-series-cmd nil org-redo-cmd (org-search-view nil (if current-prefix-arg nil \"keyword\")) org-last-args (nil \"keyword\" nil) org-agenda-type search face org-agenda-structure-secondary) 62 82 (org-series-cmd nil org-redo-cmd (org-search-view nil (if current-prefix-arg nil \"keyword\")) org-last-args (nil \"keyword\" nil) org-agenda-type search face org-agenda-structure-secondary) 82 83 (org-series-cmd nil org-redo-cmd (org-search-view nil (if current-prefix-arg nil \"keyword\")) org-last-args (nil \"keyword\" nil) org-agenda-type search face org-agenda-structure-secondary) 83 86 (org-series-cmd nil org-redo-cmd (org-search-view nil (if current-prefix-arg nil \"keyword\")) org-last-args (nil \"keyword\" nil) org-agenda-type search font-lock-face help-key-binding face org-agenda-structure-secondary) 86 87 (org-series-cmd nil org-redo-cmd (org-search-view nil (if current-prefix-arg nil \"keyword\")) org-last-args (nil \"keyword\" nil) org-agenda-type search face org-agenda-structure-secondary) 87 88 (org-series-cmd nil org-redo-cmd (org-search-view nil (if current-prefix-arg nil \"keyword\")) org-last-args (nil \"keyword\" nil) org-agenda-type search font-lock-face help-key-binding face org-agenda-structure-secondary) 88 89 (org-series-cmd nil org-redo-cmd (org-search-view nil (if current-prefix-arg nil \"keyword\")) org-last-args (nil \"keyword\" nil) org-agenda-type search face org-agenda-structure-secondary) 89 108 (org-series-cmd nil org-redo-cmd (org-search-view nil (if current-prefix-arg nil \"keyword\")) org-last-args (nil \"keyword\" nil) org-agenda-type search face org-agenda-structure-secondary) 108 109 (org-series-cmd nil org-redo-cmd (org-search-view nil (if current-prefix-arg nil \"keyword\")) org-last-args (nil \"keyword\" nil) org-agenda-type search))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T1 keyword\n* T2 other\n* T3 keyword")
  (condition-case nil
      (org-search-view nil "keyword")
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-columns-get-format
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf39_columns_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-columns-get-format)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+COLUMNS: %25ITEM %TODO %3PRIORITY %TAGS %V\n* TODO [#A] T :tag:\n:PROPERTIES:\n:V: val\n:END:")
  (goto-char (point-min))
  (org-columns-get-format))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-columns-compile-format
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf39_columns_compile() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-columns-compile-format)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-columns-compile-format "%25ITEM %TODO %3PRIORITY %TAGS")"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-columns-uncompile-format
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf39_columns_uncompile() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-columns-uncompile-format)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-columns-uncompile-format '(("ITEM" 25) ("TODO" 0) ("PRIORITY" 3) ("TAGS" 0)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-columns-get-format-with-width
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf39_columns_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (void-function org-columns-get-format-with-width)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+COLUMNS: %25ITEM %TODO %3PRIORITY\n* T")
  (goto-char (point-min))
  (org-columns-get-format-with-width))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-columns-display
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf39_columns_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+COLUMNS: %25ITEM %TODO %3PRIORITY\n* TODO [#A] T\n* DONE [#B] D")
  (condition-case nil
      (org-columns-display)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-columns
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf39_agenda_columns() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+COLUMNS: %25ITEM %TODO %3PRIORITY\n* TODO [#A] T\n* DONE [#B] D")
  (condition-case nil
      (org-agenda-columns)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-view-mode-dispatch
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf39_agenda_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-agenda-view-mode-dispatch)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-filter
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf39_agenda_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T1 :work:\n* DONE D1 :home:\n* TODO T2 :work:")
  (condition-case nil
      (org-agenda-prepare-buffers (list (current-buffer)))
    (error nil))
  (list (length (org-map-entries (lambda () t) nil 'file))
        (length (org-map-entries (lambda () t) "work" 'file))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-filter-by-category
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf39_agenda_filter_cat() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-agenda-filter-by-category)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-filter-by-tag
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf39_agenda_filter_tag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-agenda-filter-by-tag)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-filter-by-regexp
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf39_agenda_filter_regexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-agenda-filter-by-regexp)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-filter-by-effort
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf39_agenda_filter_effort() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-agenda-filter-by-effort)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-filter-by-priority
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf39_agenda_filter_prio() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-agenda-filter-by-priority)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-filter-by-top-headline
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf39_agenda_filter_top() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-agenda-filter-by-top-headline)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-filter-remove-all
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf39_agenda_filter_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-agenda-filter-remove-all)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-get-restriction-lock
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf39_agenda_restriction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-agenda-get-restriction-lock)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-redo
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf39_agenda_redo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-agenda-redo)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-quit
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf39_agenda_quit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-agenda-quit)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-exit
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf39_agenda_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-agenda-exit)
  (error nil))"##,
        expect,
    );
}
