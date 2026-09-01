//! Strong combo-complex-52 oracle tests — very deep multi-step
//! divergence-prone workflows: org-id multi-heading uniqueness,
//! checkbox nested dependency chains, sparse tree narrow/widen,
//! outline path level changes, multi-criteria sort chains,
//! drawer create/populate/check, internal link resolve/mutate,
//! timestamp schedule/reschedule/remove, list 3-level indent/sort,
//! macro cross-reference expansion, element create+set-element
//! deep mutation, and entity lookup with replacement.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-id: get/create on 5 headings → store links → verify uniqueness
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo52_org_id_multi_uniqueness() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"‘org-id-get’ expects a file-visiting buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-id)
  (let ((org-id-link-to-org-use-id t)
        (org-id-track-globally nil))
    (insert "* A\n* B\n* C\n* D\n* E\n")
    (let ((r '()))
      (let ((ids '()))
        (dolist (h '("A" "B" "C" "D" "E"))
          (goto-char (point-min))
          (search-forward (concat "* " h)) (beginning-of-line)
          (let ((id (org-id-get-create)))
            (push (list :heading h :id id) ids)))
        (push (list :all-ids (nreverse ids)) r)
        ;; verify uniqueness
        (let ((id-vals (mapcar (lambda (x) (plist-get x :id)) ids)))
          (push (list :all-unique (= (length id-vals) (length (delete-dups id-vals)))) r)))
      ;; store link on B, verify it references B's id
      (goto-char (point-min))
      (search-forward "* B") (beginning-of-line)
      (let ((link (org-store-link nil)))
        (push (list :b-link-has-id (when (stringp link) (string-match-p "id:" link))) r))
      ;; all IDs are non-nil strings
      (let ((all-ids (plist-get (nth 0 r) :all-ids)))
        (push (list :all-non-nil (cl-every (lambda (x) (and (plist-get x :id) (stringp (plist-get x :id)))) all-ids)) r))
      (nreverse r))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Checkbox nested dependency: [/] [%] cookies → toggle → update → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo52_checkbox_nested_dependency_cookies() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init \"* Tasks [/]\\n- [ ] Root A\\n  - [X] Child A1\\n  - [ ] Child A2\\n- [-] Parent B [/]\\n  - [X] Child B1\\n  - [ ] Child B2\\n    - [X] Grand B2a\\n    - [ ] Grand B2b\\n\") (:after-update \"* Tasks [0/2]\\n- [ ] Root A\\n  - [X] Child A1\\n  - [ ] Child A2\\n- [-] Parent B [1/2]\\n  - [X] Child B1\\n  - [ ] Child B2\\n    - [X] Grand B2a\\n    - [ ] Grand B2b\\n\") (:after-a2 \"* Tasks [1/2]\\n- [X] Root A\\n  - [X] Child A1\\n  - [X] Child A2\\n- [-] Parent B [1/2]\\n  - [X] Child B1\\n  - [-] Child B2\\n    - [X] Grand B2a\\n    - [ ] Grand B2b\\n\") (:after-b2b \"* Tasks [2/2]\\n- [X] Root A\\n  - [X] Child A1\\n  - [X] Child A2\\n- [X] Parent B [2/2]\\n  - [X] Child B1\\n  - [X] Child B2\\n    - [X] Grand B2a\\n    - [X] Grand B2b\\n\") (:item-count 8) (:checked-count 0) (:partial-count 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Tasks [/]\n")
  (insert "- [ ] Root A\n")
  (insert "  - [X] Child A1\n")
  (insert "  - [ ] Child A2\n")
  (insert "- [-] Parent B [/]\n")
  (insert "  - [X] Child B1\n")
  (insert "  - [ ] Child B2\n")
  (insert "    - [X] Grand B2a\n")
  (insert "    - [ ] Grand B2b\n")
  (let ((r '()))
    ;; initial state
    (push (list :init (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; update all cookies
    (org-update-statistics-cookies t)
    (push (list :after-update (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; toggle Child A2
    (goto-char (point-min))
    (search-forward "Child A2") (beginning-of-line) (org-toggle-checkbox)
    (org-update-statistics-cookies t)
    (push (list :after-a2 (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; toggle Grand B2b
    (goto-char (point-min))
    (search-forward "Grand B2b") (beginning-of-line) (org-toggle-checkbox)
    (org-update-statistics-cookies t)
    (push (list :after-b2b (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; element integrity: count items, checkboxes
    (push (list :item-count (length (org-element-map (org-element-parse-buffer) 'item #'identity))) r)
    (push (list :checked-count
                (length (org-element-map (org-element-parse-buffer) 'item
                          (lambda (i) (when (equal "X" (org-element-property :checkbox i)) i))))) r)
    (push (list :partial-count
                (length (org-element-map (org-element-parse-buffer) 'item
                          (lambda (i) (when (equal "-" (org-element-property :checkbox i)) i))))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Sparse tree: match → narrow → widen → rematch → verify consistent
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo52_sparse_tree_narrow_widen_rematch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:match-work (\"A\" \"A1\" \"A2\" \"B\" \"B1\" \"C\")) (:narrow-match-urgent (\"B\" \"B1\")) (:match-home (\"A\" \"A2\" \"B\" \"C\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A :work:\n** A1 :work:\n** A2 :home:\n* B :work:urgent:\n** B1 :urgent:\n* C :home:\n")
  (let ((r '()))
    ;; match :work:
    (org-match-sparse-tree nil "work")
    (push (list :match-work (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                                    (org-element-map (org-element-parse-buffer nil t) 'headline #'identity))) r)
    (org-remove-occur-highlights)
    ;; narrow to B subtree
    (goto-char (point-min))
    (search-forward "* B :work:urgent:") (beginning-of-line)
    (org-narrow-to-subtree)
    ;; match :urgent: within narrowed buffer
    (org-match-sparse-tree nil "urgent")
    (push (list :narrow-match-urgent (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                                             (org-element-map (org-element-parse-buffer nil t) 'headline #'identity))) r)
    (widen)
    (org-remove-occur-highlights)
    ;; match :home: in full buffer
    (org-match-sparse-tree nil "home")
    (push (list :match-home (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                                    (org-element-map (org-element-parse-buffer nil t) 'headline #'identity))) r)
    (org-remove-occur-highlights)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Outline path: get → demote → get again → level changed
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo52_outline_path_demote_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:path1 (\"A\" \"A1\" \"A1a\")) (:level1 4) (:path2 (\"A1\" \"A1a\")) (:level2 4) (:path3 nil) (:level3 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n** A1\n*** A1a\n**** A1a1\n* B\n")
  (let ((r '()))
    ;; path at A1a1
    (goto-char (point-min))
    (search-forward "**** A1a1") (beginning-of-line)
    (let* ((path1 (org-get-outline-path))
           (level1 (org-outline-level)))
      (push (list :path1 path1) r)
      (push (list :level1 level1) r))
    ;; promote A1 (decrease level)
    (goto-char (point-min))
    (search-forward "** A1") (beginning-of-line)
    (org-metaleft)   ;; becomes * A1 (level 1)
    ;; path at A1a1 again
    (goto-char (point-min))
    (search-forward "**** A1a1") (beginning-of-line)
    (let* ((path2 (org-get-outline-path))
           (level2 (org-outline-level)))
      (push (list :path2 path2) r)
      (push (list :level2 level2) r))
    ;; demote A (increase level)
    (goto-char (point-min))
    (org-metaright)
    ;; path at A1 again
    (goto-char (point-min))
    (search-forward "* A1") (beginning-of-line)
    (let* ((path3 (org-get-outline-path))
           (level3 (org-outline-level)))
      (push (list :path3 path3) r)
      (push (list :level3 level3) r))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Sort: entries by todo → resort by priority → resort by property
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo52_sort_multi_criteria_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Nothing to sort\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#B] Zebra\n")
  (insert ":PROPERTIES:\n:WEIGHT: 3\n:END:\n")
  (insert "* DONE [#A] Apple\n")
  (insert ":PROPERTIES:\n:WEIGHT: 1\n:END:\n")
  (insert "* TODO [#C] Mango\n")
  (insert ":PROPERTIES:\n:WEIGHT: 2\n:END:\n")
  (insert "* CANCELED [#B] Banana\n")
  (insert ":PROPERTIES:\n:WEIGHT: 4\n:END:\n")
  (let ((r '()))
    ;; initial order
    (push (list :init (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                              (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    ;; sort by todo (alphabetically)
    (goto-char (point-min))
    (org-sort-entries nil ?o)
    (push (list :by-todo (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                                  (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    ;; sort by priority
    (goto-char (point-min))
    (org-sort-entries nil ?p)
    (push (list :by-priority (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                                      (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    ;; sort by property WEIGHT
    (goto-char (point-min))
    (org-sort-entries nil ?r ?p "WEIGHT" nil #'string<)
    (push (list :by-weight (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                                    (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    ;; final buffer
    (push (list :buffer (buffer-substring-no-properties (point-min) (point-max))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Drawer: create property → create logbook → clock → check both drawers
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo52_drawer_property_logbook_clock_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:prop-count 1) (:prop-keys (\"CATEGORY\")) (:total-drawers 1) (:logbooks 1) (:prop-drawers-now 1) (:clock-count 1) (:status-still \"active\") (:owner-still \"alice\") (:buffer \"* Task\\n:PROPERTIES:\\n:STATUS:   active\\n:OWNER:    alice\\n:END:\\n:LOGBOOK:\\nCLOCK: [2026-06-15 Mon 12:00]--[2026-06-15 Mon 12:00] =>  0:00\\n:END:\\n\"))""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-clock)
  (let ((org-clock-persist nil))
    (insert "* Task\n")
    (let ((r '()))
      ;; create property drawer with values
      (goto-char (point-min))
      (org-insert-property-drawer)
      (org-entry-put nil "STATUS" "active")
      (org-entry-put nil "OWNER" "alice")
      (push (list :prop-count (length (org-element-map (org-element-parse-buffer) 'property-drawer #'identity))) r)
      (push (list :prop-keys (sort (mapcar #'car (org-entry-properties nil t)) #'string-lessp)) r)
      ;; clock in/out (creates logbook drawer)
      (goto-char (point-min))
      (org-clock-in nil) (org-clock-out nil nil)
      ;; check both drawer types now
      (push (list :total-drawers (length (org-element-map (org-element-parse-buffer) 'drawer #'identity))) r)
      (push (list :logbooks (length (org-element-map (org-element-parse-buffer) 'drawer
                                      (lambda (d) (when (equal "LOGBOOK" (org-element-property :drawer-name d)) d))))) r)
      (push (list :prop-drawers-now (length (org-element-map (org-element-parse-buffer) 'property-drawer #'identity))) r)
      ;; clock count
      (push (list :clock-count (length (org-element-map (org-element-parse-buffer) 'clock #'identity))) r)
      ;; verify properties still accessible
      (push (list :status-still (org-entry-get nil "STATUS")) r)
      (push (list :owner-still (org-entry-get nil "OWNER")) r)
      ;; buffer content
      (push (list :buffer (buffer-substring-no-properties (point-min) (point-max))) r)
      (nreverse r))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Internal link: create → resolve → modify target → resolve again
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo52_internal_link_create_resolve_modify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:link-types (\"custom-id\" \"fuzzy\")) (:link-paths (\"my-sec\" \"*Target Section\")) (:link-count 2) (:after-rename-paths (\"my-sec\" \"*Target Section\")) (:after-link-fix-paths (\"renamed-sec\" \"*Target Section\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ol)
  (insert "* Target Section\n")
  (insert ":PROPERTIES:\n:CUSTOM_ID: my-sec\n:END:\n\n")
  (insert "* Links\n")
  (insert "Link to [[#my-sec][Custom ID]] and [[*Target Section][Heading]].\n")
  (let ((r '()))
    ;; parse and examine links
    (let* ((tree (org-element-parse-buffer))
           (links (org-element-map tree 'link #'identity)))
      (push (list :link-types (mapcar (lambda (l) (org-element-property :type l)) links)) r)
      (push (list :link-paths (mapcar (lambda (l) (org-element-property :path l)) links)) r)
      (push (list :link-count (length links)) r))
    ;; modify CUSTOM_ID
    (goto-char (point-min))
    (search-forward "CUSTOM_ID: my-sec")
    (replace-match "CUSTOM_ID: renamed-sec")
    ;; reparsed: link path for [[#my-sec]] might now be broken, but on-disk it's still "my-sec"
    (let* ((tree (org-element-parse-buffer))
           (links (org-element-map tree 'link #'identity)))
      (push (list :after-rename-paths (mapcar (lambda (l) (org-element-property :path l)) links)) r))
    ;; fix the link to match new CUSTOM_ID
    (goto-char (point-min))
    (search-forward "[[#my-sec]") (beginning-of-line)
    (search-forward "#my-sec") (replace-match "#renamed-sec")
    (let* ((tree (org-element-parse-buffer))
           (links (org-element-map tree 'link #'identity)))
      (push (list :after-link-fix-paths (mapcar (lambda (l) (org-element-property :path l)) links)) r))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Timestamp: schedule → reschedule → remove → verify clean state
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo52_timestamp_schedule_reschedule_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:after-sched (\"scheduled\")) (:after-resched ((\"scheduled\" (timestamp (:standard-properties [20 nil nil nil 36 0 nil nil nil nil nil nil nil nil nil nil nil nil] :type active :range-type nil :raw-value \"<2024-04-01 Mon>\" :year-start 2024 :month-start 4 :day-start 1 :hour-start nil :minute-start nil :year-end 2024 :month-end 4 :day-end 1 :hour-end nil :minute-end nil))))) (:after-dead ((\"S\" \"D\"))) (:after-remove-sched ((nil \"D\"))) (:after-remove-dead 0) (:buffer \"* Event\\n\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Event\n")
  (let ((r '()))
    ;; schedule
    (goto-char (point-min))
    (org-schedule nil "<2024-03-01 Fri>")
    (push (list :after-sched
                (org-element-map (org-element-parse-buffer) 'planning
                  (lambda (p) (when (org-element-property :scheduled p) "scheduled")))) r)
    ;; reschedule
    (org-schedule nil "<2024-04-01 Mon>")
    (push (list :after-resched
                (org-element-map (org-element-parse-buffer) 'planning
                  (lambda (p) (when (org-element-property :scheduled p) (list "scheduled" (org-element-property :scheduled p)))))) r)
    ;; set deadline too
    (org-deadline nil "<2024-04-15 Mon>")
    (push (list :after-dead
                (org-element-map (org-element-parse-buffer) 'planning
                  (lambda (p) (list (when (org-element-property :scheduled p) "S")
                                    (when (org-element-property :deadline p) "D"))))) r)
    ;; remove schedule (org-schedule with nil or universal arg)
    (condition-case nil
        (progn (org-schedule '(4))
               (push (list :after-remove-sched
                           (org-element-map (org-element-parse-buffer) 'planning
                             (lambda (p) (list (when (org-element-property :scheduled p) "S")
                                               (when (org-element-property :deadline p) "D"))))) r))
      (error (push (list :remove-sched-error t) r)))
    ;; remove deadline
    (condition-case nil
        (progn (org-deadline '(4))
               (push (list :after-remove-dead
                           (length (org-element-map (org-element-parse-buffer) 'planning #'identity))) r))
      (error (push (list :remove-dead-error t) r)))
    ;; buffer state
    (push (list :buffer (buffer-substring-no-properties (point-min) (point-max))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// List: create 3-level nested → indent → outdent → sort → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo52_list_3level_indent_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Cannot outdent an item without its children\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- Fruits\n")
  (insert "  - Citrus\n")
  (insert "    - Orange\n")
  (insert "    - Lemon\n")
  (insert "  - Berries\n")
  (insert "    - Blueberry\n")
  (insert "    - Strawberry\n")
  (let ((r '()))
    ;; initial structure
    (push (list :init-items (mapcar (lambda (i) (list (org-element-property :level i)
                                                      (substring-no-properties
                                                       (or (org-element-property :raw-value i) ""))))
                                    (org-element-map (org-element-parse-buffer) 'item #'identity))) r)
    ;; sort top-level
    (goto-char (point-min))
    (org-sort-list nil ?a)
    (push (list :after-sort (mapcar (lambda (i) (substring-no-properties
                                                  (or (org-element-property :raw-value i) "")))
                                    (org-element-map (org-element-parse-buffer) 'item #'identity))) r)
    ;; indent "Berries" under "Fruits" → already is
    ;; outdent Orange to top level
    (goto-char (point-min))
    (search-forward "Orange") (beginning-of-line)
    (org-metaleft)  ;; outdent one level
    (org-metaleft)  ;; outdent to top
    (push (list :after-outdent (mapcar (lambda (i) (list (org-element-property :level i)
                                                         (substring-no-properties
                                                          (or (org-element-property :raw-value i) ""))))
                                       (org-element-map (org-element-parse-buffer) 'item #'identity))) r)
    ;; indent Orange back under Fruits
    (goto-char (point-min))
    (search-forward "Orange") (beginning-of-line)
    (org-metaright)
    (org-metaright)
    (push (list :after-indent (mapcar (lambda (i) (list (org-element-property :level i)
                                                        (substring-no-properties
                                                         (or (org-element-property :raw-value i) ""))))
                                      (org-element-map (org-element-parse-buffer) 'item #'identity))) r)
    ;; list integrity
    (push (list :plain-lists (length (org-element-map (org-element-parse-buffer) 'plain-list #'identity))) r)
    (push (list :buffer (buffer-substring-no-properties (point-min) (point-max))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Macro: define cross-reference macro → expand → verify nested expansion
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo52_macro_cross_reference_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:raw-headline \"{{{full}}} Release Notes\") (:has-pkg 13) (:has-ver 36) (:has-full nil) (:no-braces nil) (:has-greeting 108) (:macro-count 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+MACRO: pkg MyPackage\n")
  (insert "#+MACRO: ver 2.0\n")
  (insert "#+MACRO: full {{{pkg}}} v{{{ver}}}\n")
  (insert "#+MACRO: greeting (eval (concat \"Hello, \" user-login-name \"!\"))\n")
  (insert "\n* {{{full}}} Release Notes\n")
  (insert "We are happy to announce {{{full}}}.\n")
  (insert "{{{greeting}}}\n")
  (let ((r '()))
    ;; before expansion
    (push (list :raw-headline
                (substring-no-properties
                 (org-element-property :raw-value
                  (car (org-element-map (org-element-parse-buffer) 'headline #'identity))))) r)
    ;; interpret (expands macros)
    (let ((interpreted (substring-no-properties (org-element-interpret-data (org-element-parse-buffer)))))
      (push (list :has-pkg (string-match-p "MyPackage" interpreted)) r)
      (push (list :has-ver (string-match-p "2\\.0" interpreted)) r)
      (push (list :has-full (string-match-p "MyPackage v2\\.0" interpreted)) r)
      (push (list :no-braces (not (string-match-p "{{{" interpreted))) r)
      (push (list :has-greeting (string-match-p "Hello" interpreted)) r))
    ;; macro keyword count
    (push (list :macro-count (length (org-element-map (org-element-parse-buffer) 'keyword
                                      (lambda (k) (when (equal "MACRO" (org-element-property :key k)) k))))) r)
    (nreverse r)))"##,
        expect,
    );
}
