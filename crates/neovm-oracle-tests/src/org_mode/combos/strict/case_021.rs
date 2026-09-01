//! org-agenda dedicated oracle tests — comprehensive probing of
//! org-agenda internals: org-agenda-list, org-todo-list,
//! org-tags-view, org-search-view, org-agenda-get-todos/scheduled/
//! deadlines/timestamps/sexps/blocks/progress, org-agenda-todo,
//! org-agenda-todo-custom-commands, org-agenda-filter, and
//! org-agenda-archive.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn strict_agenda_get_todos_and_scheduled() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:get-todos-fbound t :get-scheduled-fbound t :get-deadlines-fbound t :get-timestamps-fbound t :get-sexps-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda) (list
 :get-todos-fbound (fboundp 'org-agenda-get-todos)
 :get-scheduled-fbound (fboundp 'org-agenda-get-scheduled)
 :get-deadlines-fbound (fboundp 'org-agenda-get-deadlines)
 :get-timestamps-fbound (fboundp 'org-agenda-get-timestamps)
 :get-sexps-fbound (fboundp 'org-agenda-get-sexps)
 ))"##,
        expect,
    );
}
#[test]
fn strict_agenda_get_blocks_and_progress() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:get-blocks-fbound t :get-progress-fbound t :get-day-entries-fbound t :get-timeline-fbound nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda) (list
 :get-blocks-fbound (fboundp 'org-agenda-get-blocks)
 :get-progress-fbound (fboundp 'org-agenda-get-progress)
 :get-day-entries-fbound (fboundp 'org-agenda-get-day-entries)
 :get-timeline-fbound (fboundp 'org-agenda-get-timeline)
 ))"##,
        expect,
    );
}
#[test]
fn strict_agenda_todo_list_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:todo-entries (\"A\" \"B\" \"D\")) (:scheduled-entries (\"A\")) (:deadline-entries (\"D\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-agenda)
 (insert "* TODO A :work:\nSCHEDULED: <2024-06-01 Sat>\n** TODO B :urgent:\n")
 (insert "* DONE C :home:\n* TODO D :work:\nDEADLINE: <2024-06-15 Sat>\n")
 (let ((r '()))
  (push (list :todo-entries (org-map-entries (lambda () (org-get-heading t t t t)) "TODO=\"TODO\"")) r)
  (push (list :scheduled-entries (org-map-entries (lambda () (org-get-heading t t t t))
    "SCHEDULED<>\"\"")) r)
  (push (list :deadline-entries (org-map-entries (lambda () (org-get-heading t t t t))
    "DEADLINE<>\"\"")) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn strict_agenda_sort_strategies_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:strategy-bound t :strategy-default ((agenda habit-down time-up urgency-down category-keep) (todo urgency-down category-keep) (tags urgency-down category-keep) (search category-keep)) :cmp-fbound nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda)
 (let ((valid-strategies '(time-up time-down category-up category-down
   priority-up priority-down effort-up effort-down
   todo-state-up todo-state-down habit-up habit-down
   alpha-up alpha-down user-defined-up user-defined-down
   ts-up ts-down scheduled-up scheduled-down
   deadline-up deadline-down timestamp-up timestamp-down)))
 (list :strategy-bound (boundp 'org-agenda-sorting-strategy)
  :strategy-default (when (boundp 'org-agenda-sorting-strategy)
    org-agenda-sorting-strategy)
  :cmp-fbound (fboundp 'org-agenda-cmp-user-defined))))"##,
        expect,
    );
}
#[test]
fn strict_agenda_buffer_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 75)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-agenda)
 (insert "* TODO Task\nSCHEDULED: <2024-07-01 Mon>\n:PROPERTIES:\n:CATEGORY: dev\n:END:\n")
 (let ((r '()))
  (push (list :get-category (org-get-category)) r)
  (push (list :get-priority (org-get-priority (point))) r)
  (push (list :todo-state (org-get-todo-state)) r)
  (push (list :heading (org-get-heading t t t t)) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn strict_agenda_tags_view_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:work-count 3) (:urgent-count 3) (:work+urgent 2) (:work|home 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-agenda)
 (insert "* A :work:urgent:\n** A1 :work:\n* B :home:\n** B1 :urgent:home:\n* C :work:home:\n")
 (let ((r '()))
  (push (list :work-count (length (org-map-entries (lambda () t) "work"))) r)
  (push (list :urgent-count (length (org-map-entries (lambda () t) "urgent"))) r)
  (push (list :work+urgent (length (org-map-entries (lambda () t) "work+urgent"))) r)
  (push (list :work|home (length (org-map-entries (lambda () t) "work|home"))) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn strict_agenda_custom_commands_all_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:cmd-count 4 :cmd-keys (\"a\" \"t\" \"m\" \"s\") :cmd-names (\"Agenda\" \"Todo list\" \"Match tags\" \"Search\") :cmd-types (agenda alltodo tags search))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda)
 (let ((org-agenda-custom-commands
        '(("a" "Agenda" agenda "" ((org-agenda-span 'week)))
          ("t" "Todo list" alltodo "" ((org-agenda-overriding-header "All TODOs")))
          ("m" "Match tags" tags "work" ((org-agenda-overriding-header "Work items")))
          ("s" "Search" search "" ((org-agenda-overriding-header "Search results"))))))
 (list :cmd-count (length org-agenda-custom-commands)
  :cmd-keys (mapcar #'car org-agenda-custom-commands)
  :cmd-names (mapcar #'cadr org-agenda-custom-commands)
  :cmd-types (mapcar #'caddr org-agenda-custom-commands))))"##,
        expect,
    );
}
#[test]
fn strict_agenda_clock_report() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 10 17)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-clock) (require 'org-agenda)
 (insert "* Task\n")
 (let ((r '())) (goto-char (point-min))
  (org-clock-in nil) (org-clock-out nil nil)
  (push (list :org-clocking-buffer-fbound (fboundp 'org-clocking-buffer)) r)
  (goto-char (point-min)) (insert "#+BEGIN: clocktable :maxlevel 2 :scope file\n#+END:\n")
  (goto-char (point-min)) (search-forward "#+BEGIN: clocktable") (beginning-of-line)
  (org-dblock-update)
  (push (list :has-clocktable (> (length (buffer-string)) 0)) r)
  (nreverse r))))"##,
        expect,
    );
}
#[test]
fn strict_agenda_span_and_start_day() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:span-bound t :span-default week :start-day-bound t :start-day-default nil :start-on-weekday-bound t :ndays-bound nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda) (list
 :span-bound (boundp 'org-agenda-span)
 :span-default (when (boundp 'org-agenda-span) org-agenda-span)
 :start-day-bound (boundp 'org-agenda-start-day)
 :start-day-default (when (boundp 'org-agenda-start-day) org-agenda-start-day)
 :start-on-weekday-bound (boundp 'org-agenda-start-on-weekday)
 :ndays-bound (boundp 'org-agenda-ndays)
 ))"##,
        expect,
    );
}
#[test]
fn strict_agenda_archives_and_dim() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:archive-mode-fbound t :archive-with-fbound t :dim-blocked-fbound t :show-inherited-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda) (list
 :archive-mode-fbound (fboundp 'org-agenda-archive)
 :archive-with-fbound (fboundp 'org-agenda-archive-with)
 :dim-blocked-fbound (boundp 'org-agenda-dim-blocked-tasks)
 :show-inherited-fbound (boundp 'org-agenda-show-inherited-tags)
 ))"##,
        expect,
    );
}
