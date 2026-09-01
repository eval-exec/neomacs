//! Strong workflow combo oracle tests — real-world usage patterns.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// Workflow: GTD-style task management
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_gtd_task_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((nil \"NEXT Process inbox\") (nil \"WAITING Delegate task\") (\"DONE\" \"Completed task\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (setq org-todo-keywords '((sequence "NEXT" "WAITING" "DONE")))
  (insert "* NEXT Process inbox\n* WAITING Delegate task\n* DONE Completed task")
  (goto-char (point-min))
  (let ((s1 (list (org-get-todo-state) (org-get-heading t t t t))))
    (org-todo 'right)  ; NEXT -> WAITING
    (forward-line)
    (let ((s2 (list (org-get-todo-state) (org-get-heading t t t t))))
      (org-todo 'right)  ; WAITING -> DONE
      (forward-line)
      (let ((s3 (list (org-get-todo-state) (org-get-heading t t t t))))
        (list s1 s2 s3)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: Project planning with deadlines
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_project_planning_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"Project Alpha\" nil nil) (\"Design phase\" \"TODO\" \"<2026-02-01>\") (\"Implementation\" \"TODO\" \"<2026-03-01>\") (\"Testing\" \"TODO\" \"<2026-04-01>\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Project Alpha\n** TODO Design phase\nDEADLINE: <2026-02-01>\n** TODO Implementation\nDEADLINE: <2026-03-01>\n** TODO Testing\nDEADLINE: <2026-04-01>")
  (goto-char (point-min))
  (let ((tasks '()))
    (while (not (eobp))
      (when (org-at-heading-p)
        (push (list (org-get-heading t t t t)
                    (org-get-todo-state)
                    (org-entry-get nil "DEADLINE"))
              tasks))
      (forward-line))
    (nreverse tasks)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: Meeting notes with action items
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_meeting_notes_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 \"Meeting 2026-01-15\" nil) (2 \"Attendees\" nil) (2 \"Action items\" nil) (3 \"Alice: Prepare report\" \"TODO\") (3 \"Bob: Review code\" \"TODO\") (2 \"Notes\" nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Meeting 2026-01-15\n** Attendees\n- Alice\n- Bob\n** Action items\n*** TODO Alice: Prepare report\nDEADLINE: <2026-01-20>\n*** TODO Bob: Review code\nDEADLINE: <2026-01-18>\n** Notes\nDiscussion points here")
  (let* ((tree (org-element-parse-buffer))
         (headlines (org-element-map tree 'headline
                      (lambda (h)
                        (list (org-element-property :level h)
                              (org-element-property :raw-value h)
                              (org-element-property :todo-keyword h))))))
    headlines))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: Time tracking with clock reports
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_time_tracking_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"1:30\") (\"1:00\") (\"1:00\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Coding\n:LOGBOOK:\nCLOCK: [2026-01-15 09:00]--[2026-01-15 10:30] =>  1:30\nCLOCK: [2026-01-15 11:00]--[2026-01-15 12:00] =>  1:00\n:END:\n* Meetings\n:LOGBOOK:\nCLOCK: [2026-01-15 14:00]--[2026-01-15 15:00] =>  1:00\n:END:")
  (let* ((tree (org-element-parse-buffer))
         (clocks (org-element-map tree 'clock
                   (lambda (c)
                     (list (org-element-property :duration c))))))
    clocks))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: Habit tracking
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_habit_tracking_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"<2026-01-15 .+1d/2d>\" \"habit\" \".+1d/2d\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Exercise\nSCHEDULED: <2026-01-15 .+1d/2d>\n:PROPERTIES:\n:STYLE: habit\n:END:")
  (goto-char (point-min))
  (let ((sched (org-entry-get nil "SCHEDULED"))
        (style (org-entry-get nil "STYLE"))
        (repeat (org-get-repeat)))
    (list sched style repeat)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: Contact management
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_contact_management_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"Alice Smith\" \"alice@example.com\" \"123-456\" \"work\") (\"Bob Jones\" \"bob@example.com\" \"789-012\" \"personal\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Alice Smith\n:PROPERTIES:\n:EMAIL: alice@example.com\n:PHONE: 123-456\n:CATEGORY: work\n:END:\n* Bob Jones\n:PROPERTIES:\n:EMAIL: bob@example.com\n:PHONE: 789-012\n:CATEGORY: personal\n:END:")
  (goto-char (point-min))
  (let ((contacts '()))
    (while (not (eobp))
      (when (org-at-heading-p)
        (push (list (org-get-heading t t t t)
                    (org-entry-get nil "EMAIL")
                    (org-entry-get nil "PHONE")
                    (org-entry-get nil "CATEGORY"))
              contacts))
      (forward-line))
    (nreverse contacts)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: Reading list with status
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_reading_list_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"TO-READ Book A\" nil nil nil) (\"READING Book B\" nil \"50%\" nil) (\"FINISHED Book C\" nil nil \"5\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (setq org-todo-keywords '((sequence "TO-READ" "READING" "FINISHED")))
  (insert "* TO-READ Book A\n* READING Book B\n:PROPERTIES:\n:PROGRESS: 50%\n:END:\n* FINISHED Book C\n:PROPERTIES:\n:RATING: 5\n:END:")
  (goto-char (point-min))
  (let ((books '()))
    (while (not (eobp))
      (when (org-at-heading-p)
        (push (list (org-get-heading t t t t)
                    (org-get-todo-state)
                    (org-entry-get nil "PROGRESS")
                    (org-entry-get nil "RATING"))
              books))
      (forward-line))
    (nreverse books)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: Bug tracking
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_bug_tracking_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"BUG Login fails\" nil \"high\" \"Alice\") (\"FIXING UI glitch\" nil \"low\" \"Bob\") (\"FIXED Memory leak\" nil \"medium\" \"Charlie\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (setq org-todo-keywords '((sequence "BUG" "FIXING" "FIXED" "CLOSED")))
  (insert "* BUG Login fails\n:PROPERTIES:\n:SEVERITY: high\n:ASSIGNEE: Alice\n:END:\n* FIXING UI glitch\n:PROPERTIES:\n:SEVERITY: low\n:ASSIGNEE: Bob\n:END:\n* FIXED Memory leak\n:PROPERTIES:\n:SEVERITY: medium\n:ASSIGNEE: Charlie\n:END:")
  (goto-char (point-min))
  (let ((bugs '()))
    (while (not (eobp))
      (when (org-at-heading-p)
        (push (list (org-get-heading t t t t)
                    (org-get-todo-state)
                    (org-entry-get nil "SEVERITY")
                    (org-entry-get nil "ASSIGNEE"))
              bugs))
      (forward-line))
    (nreverse bugs)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: Journal entries
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_journal_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 \"2026-01-15\") (2 \"Morning\") (2 \"Afternoon\") (2 \"Evening\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* 2026-01-15\n** Morning\nGood start to the day\n** Afternoon\nProductive meeting\n** Evening\nRelaxing")
  (let* ((tree (org-element-parse-buffer))
         (headlines (org-element-map tree 'headline
                      (lambda (h)
                        (list (org-element-property :level h)
                              (org-element-property :raw-value h))))))
    headlines))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: Recipe collection
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_recipe_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 \"Pasta Carbonara\") (2 \"Ingredients\") (2 \"Steps\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Pasta Carbonara\n** Ingredients\n- 200g spaghetti\n- 100g pancetta\n- 2 eggs\n** Steps\n1. Cook pasta\n2. Fry pancetta\n3. Mix eggs and cheese\n4. Combine all")
  (let* ((tree (org-element-parse-buffer))
         (headlines (org-element-map tree 'headline
                      (lambda (h)
                        (list (org-element-property :level h)
                              (org-element-property :raw-value h))))))
    headlines))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: Workout tracking
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_workout_tracking_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 \"2026-01-15 Workout\") (2 \"Cardio\") (2 \"Strength\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* 2026-01-15 Workout\n** Cardio\n- 30 min running\n- 15 min cycling\n** Strength\n- 3x10 bench press\n- 3x10 squats\n:PROPERTIES:\n:DURATION: 60min\n:CALORIES: 500\n:END:")
  (let* ((tree (org-element-parse-buffer))
         (headlines (org-element-map tree 'headline
                      (lambda (h)
                        (list (org-element-property :level h)
                              (org-element-property :raw-value h))))))
    headlines))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: Travel planning
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_travel_planning_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 \"Trip to Paris\" nil) (2 \"Flights\" nil) (3 \"Book outbound\" \"TODO\") (3 \"Book return\" \"TODO\") (2 \"Hotels\" nil) (3 \"Book hotel\" \"TODO\") (2 \"Activities\" nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Trip to Paris\n** Flights\n*** TODO Book outbound\nDEADLINE: <2026-02-01>\n*** TODO Book return\nDEADLINE: <2026-02-01>\n** Hotels\n*** TODO Book hotel\nDEADLINE: <2026-02-15>\n** Activities\n- Visit Eiffel Tower\n- Louvre Museum\n- Notre Dame")
  (let* ((tree (org-element-parse-buffer))
         (headlines (org-element-map tree 'headline
                      (lambda (h)
                        (list (org-element-property :level h)
                              (org-element-property :raw-value h)
                              (org-element-property :todo-keyword h))))))
    headlines))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: Shopping list
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_shopping_list_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"* Groceries\" \"* Groceries\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Groceries\n- [ ] Milk\n- [ ] Bread\n- [ ] Eggs\n- [X] Butter\n* Hardware\n- [ ] Screws\n- [ ] Paint\n- [X] Brushes")
  (goto-char (point-min))
  (org-update-statistics-cookies t)
  (let ((h1 (buffer-substring-no-properties (line-beginning-position) (line-end-position))))
    (forward-line 4)
    (org-toggle-checkbox)  ; check Milk
    (org-update-statistics-cookies t)
    (goto-char (point-min))
    (let ((h2 (buffer-substring-no-properties (line-beginning-position) (line-end-position))))
      (list h1 h2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: Kanban board
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_kanban_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((nil \"BACKLOG Feature A\") (\"TODO\" \"Feature B\") (nil \"IN-PROGRESS Feature C\") (\"DONE\" \"Feature D\") (nil \"BACKLOG Feature E\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (setq org-todo-keywords '((sequence "BACKLOG" "TODO" "IN-PROGRESS" "DONE")))
  (insert "* BACKLOG Feature A\n* TODO Feature B\n* IN-PROGRESS Feature C\n* DONE Feature D\n* BACKLOG Feature E")
  (goto-char (point-min))
  (let ((board '()))
    (while (not (eobp))
      (when (org-at-heading-p)
        (push (list (org-get-todo-state) (org-get-heading t t t t)) board))
      (forward-line))
    (nreverse board)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: Daily planner
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_daily_planner_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"2026-01-15 Wednesday\" nil) (\"Morning routine\" \"TODO\") (\"Team standup\" \"TODO\") (\"Code review\" \"TODO\") (\"End of day\" \"TODO\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* 2026-01-15 Wednesday\n** TODO Morning routine\nSCHEDULED: <2026-01-15 08:00>\n** TODO Team standup\nSCHEDULED: <2026-01-15 10:00>\n** TODO Code review\nSCHEDULED: <2026-01-15 14:00>\n** TODO End of day\nSCHEDULED: <2026-01-15 17:00>")
  (let* ((tree (org-element-parse-buffer))
         (tasks (org-element-map tree 'headline
                  (lambda (h)
                    (list (org-element-property :raw-value h)
                          (org-element-property :todo-keyword h))))))
    tasks))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: Inventory management
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_inventory_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"Electronics\" \"20\" \"199\") (\"Laptop\" \"20\" \"199\") (\"Monitor\" \"20\" \"199\") (\"Furniture\" \"20\" \"199\") (\"Desk\" \"20\" \"199\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Electronics\n** Laptop\n:PROPERTIES:\n:QTY: 5\n:COST: 999\n:END:\n** Monitor\n:PROPERTIES:\n:QTY: 10\n:COST: 299\n:END:\n* Furniture\n** Desk\n:PROPERTIES:\n:QTY: 20\n:COST: 199\n:END:")
  (let* ((tree (org-element-parse-buffer))
         (items (org-element-map tree 'headline
                  (lambda (h)
                    (list (org-element-property :raw-value h)
                          (org-entry-get nil "QTY")
                          (org-entry-get nil "COST"))))))
    items))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: Project status tracking
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_project_status_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 \"PLANNING Project A\" nil) (2 \"ACTIVE Module 1\" nil) (3 \"Task 1.1\" \"DONE\") (3 \"Task 1.2\" \"TODO\") (2 \"BLOCKED Module 2\" nil) (3 \"WAITING Task 2.1\" nil) (1 \"COMPLETE Project B\" nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (setq org-todo-keywords '((sequence "PLANNING" "ACTIVE" "BLOCKED" "COMPLETE")))
  (insert "* PLANNING Project A\n:PROPERTIES:\n:DEADLINE: 2026-03-01\n:END:\n** ACTIVE Module 1\n*** DONE Task 1.1\n*** TODO Task 1.2\n** BLOCKED Module 2\n*** WAITING Task 2.1\n* COMPLETE Project B")
  (let* ((tree (org-element-parse-buffer))
         (headlines (org-element-map tree 'headline
                      (lambda (h)
                        (list (org-element-property :level h)
                              (org-element-property :raw-value h)
                              (org-element-property :todo-keyword h))))))
    headlines))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: Expense tracking
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_expense_tracking_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* January 2026\n** Food\n| Date | Item | Amount |\n|------+-------+--------|\n| 01/15 | Lunch | 12.50 |\n| 01/16 | Dinner | 25.00 |\n** Transport\n| Date | Item | Amount |\n|------+-------+--------|\n| 01/15 | Bus | 2.50 |\n| 01/16 | Taxi | 15.00 |")
  (let* ((tree (org-element-parse-buffer))
         (tables (org-element-map tree 'table
                   (lambda (t)
                     (length (org-element-contents t))))))
    tables))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: Goal tracking
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_goal_tracking_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"Run marathon\" \"TODO\" nil) (\"Lose 10kg\" \"TODO\" nil) (\"Get promotion\" \"TODO\" nil) (\"Learn Rust\" \"TODO\" nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* 2026 Goals\n** Fitness\n*** TODO Run marathon\nDEADLINE: <2026-06-01>\n:PROPERTIES:\n:PROGRESS: 30%\n:END:\n*** TODO Lose 10kg\nDEADLINE: <2026-12-31>\n:PROPERTIES:\n:PROGRESS: 20%\n:END:\n** Career\n*** TODO Get promotion\nDEADLINE: <2026-09-01>\n*** TODO Learn Rust\nDEADLINE: <2026-03-01>")
  (let* ((tree (org-element-parse-buffer))
         (goals (org-element-map tree 'headline
                  (lambda (h)
                    (when (org-element-property :todo-keyword h)
                      (list (org-element-property :raw-value h)
                            (org-element-property :todo-keyword h)
                            (org-entry-get nil "PROGRESS")))))))
    goals))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: Software release planning
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_release_planning_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"Release v2.0\" nil \"8d\") (\"Feature A\" \"TODO\" \"8d\") (\"Feature B\" \"TODO\" \"8d\") (\"Bugfix 1\" \"DONE\" \"8d\") (\"Feature C\" \"TODO\" \"8d\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Release v2.0\n** TODO Feature A\n:PROPERTIES:\n:ESTIMATE: 5d\n:END:\n** TODO Feature B\n:PROPERTIES:\n:ESTIMATE: 3d\n:END:\n** DONE Bugfix 1\n:PROPERTIES:\n:ESTIMATE: 1d\n:END:\n** TODO Feature C\n:PROPERTIES:\n:ESTIMATE: 8d\n:END:")
  (let* ((tree (org-element-parse-buffer))
         (features (org-element-map tree 'headline
                     (lambda (h)
                       (list (org-element-property :raw-value h)
                             (org-element-property :todo-keyword h)
                             (org-entry-get nil "ESTIMATE"))))))
    features))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: Book notes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_book_notes_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 \"Deep Work by Cal Newport\") (2 \"Chapter 1: Deep Work is Valuable\") (2 \"Chapter 2: Deep Work is Rare\") (2 \"Chapter 3: Deep Work is Meaningful\") (2 \"Action Items\") (3 \"Block 2 hours daily for deep work\") (3 \"Turn off notifications\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Deep Work by Cal Newport\n** Chapter 1: Deep Work is Valuable\nKey insight: Deep work is becoming increasingly rare and valuable.\n** Chapter 2: Deep Work is Rare\nDistractions are everywhere.\n** Chapter 3: Deep Work is Meaningful\nFocus leads to fulfillment.\n** Action Items\n*** TODO Block 2 hours daily for deep work\n*** TODO Turn off notifications")
  (let* ((tree (org-element-parse-buffer))
         (headlines (org-element-map tree 'headline
                      (lambda (h)
                        (list (org-element-property :level h)
                              (org-element-property :raw-value h))))))
    headlines))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: Event planning
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_event_planning_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"Annual Conference 2026\" nil nil nil) (\"Book venue\" \"TODO\" nil nil) (\"Invite speakers\" \"TODO\" nil nil) (\"Arrange catering\" \"TODO\" nil nil) (\"Set date\" \"DONE\" nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Annual Conference 2026\n** TODO Book venue\nDEADLINE: <2026-03-01>\n:PROPERTIES:\n:BUDGET: 5000\n:END:\n** TODO Invite speakers\nDEADLINE: <2026-04-01>\n** TODO Arrange catering\nDEADLINE: <2026-05-01>\n** DONE Set date\nCLOSED: [2026-01-15]")
  (let* ((tree (org-element-parse-buffer))
         (tasks (org-element-map tree 'headline
                  (lambda (h)
                    (list (org-element-property :raw-value h)
                          (org-element-property :todo-keyword h)
                          (org-entry-get nil "DEADLINE")
                          (org-entry-get nil "BUDGET"))))))
    tasks))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: Learning log
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_learning_log_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"Rust Programming\" nil) (\"Ownership and Borrowing\" \"TODO\") (\"Variables and Types\" \"DONE\") (\"Error Handling\" \"TODO\") (\"Emacs Lisp\" nil) (\"Basic Functions\" \"DONE\") (\"Macros\" \"TODO\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Rust Programming\n** TODO Ownership and Borrowing\nSCHEDULED: <2026-01-15>\n** DONE Variables and Types\nCLOSED: [2026-01-10]\n** TODO Error Handling\nSCHEDULED: <2026-01-20>\n* Emacs Lisp\n** DONE Basic Functions\nCLOSED: [2026-01-05]\n** TODO Macros\nSCHEDULED: <2026-01-25>")
  (let* ((tree (org-element-parse-buffer))
         (topics (org-element-map tree 'headline
                   (lambda (h)
                     (list (org-element-property :raw-value h)
                           (org-element-property :todo-keyword h))))))
    topics))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: Health tracking
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_health_tracking_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"2026-01-15 Health Log\" nil nil \"7.5\") (\"Weight\" nil nil \"7.5\") (\"Blood Pressure\" nil nil \"7.5\") (\"Exercise\" nil nil \"7.5\") (\"Sleep\" nil nil \"7.5\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* 2026-01-15 Health Log\n** Weight\n:PROPERTIES:\n:VALUE: 75kg\n:END:\n** Blood Pressure\n:PROPERTIES:\n:SYSTOLIC: 120\n:DIASTOLIC: 80\n:END:\n** Exercise\n- 30 min running\n- 15 min stretching\n** Sleep\n:PROPERTIES:\n:HOURS: 7.5\n:QUALITY: good\n:END:")
  (let* ((tree (org-element-parse-buffer))
         (entries (org-element-map tree 'headline
                    (lambda (h)
                      (list (org-element-property :raw-value h)
                            (org-entry-get nil "VALUE")
                            (org-entry-get nil "SYSTOLIC")
                            (org-entry-get nil "HOURS"))))))
    entries))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: Subscription management
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_subscription_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"Subscriptions\" \"49.99\" \"2026-03-01\" \"cancelled\") (\"Netflix\" \"49.99\" \"2026-03-01\" \"cancelled\") (\"Spotify\" \"49.99\" \"2026-03-01\" \"cancelled\") (\"Gym\" \"49.99\" \"2026-03-01\" \"cancelled\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Subscriptions\n** Netflix\n:PROPERTIES:\n:COST: 15.99\n:RENEWAL: 2026-02-01\n:STATUS: active\n:END:\n** Spotify\n:PROPERTIES:\n:COST: 9.99\n:RENEWAL: 2026-01-20\n:STATUS: active\n:END:\n** Gym\n:PROPERTIES:\n:COST: 49.99\n:RENEWAL: 2026-03-01\n:STATUS: cancelled\n:END:")
  (let* ((tree (org-element-parse-buffer))
         (subs (org-element-map tree 'headline
                 (lambda (h)
                   (list (org-element-property :raw-value h)
                         (org-entry-get nil "COST")
                         (org-entry-get nil "RENEWAL")
                         (org-entry-get nil "STATUS"))))))
    subs))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: Plant watering schedule
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_plant_watering_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"Plants\" nil \"Kitchen\") (\"Water fern\" \"TODO\" \"Kitchen\") (\"Water cactus\" \"TODO\" \"Kitchen\") (\"Water orchid\" \"TODO\" \"Kitchen\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Plants\n** TODO Water fern\nSCHEDULED: <2026-01-15 .+3d>\n:PROPERTIES:\n:LOCATION: Living room\n:END:\n** TODO Water cactus\nSCHEDULED: <2026-01-15 .+14d>\n:PROPERTIES:\n:LOCATION: Bedroom\n:END:\n** TODO Water orchid\nSCHEDULED: <2026-01-15 .+7d>\n:PROPERTIES:\n:LOCATION: Kitchen\n:END:")
  (let* ((tree (org-element-parse-buffer))
         (plants (org-element-map tree 'headline
                   (lambda (h)
                     (list (org-element-property :raw-value h)
                           (org-element-property :todo-keyword h)
                           (org-entry-get nil "LOCATION"))))))
    plants))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: Car maintenance
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_car_maintenance_workflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"Car Maintenance 2026\" nil nil nil) (\"Oil change\" \"DONE\" nil nil) (\"Tire rotation\" \"TODO\" nil nil) (\"Brake inspection\" \"TODO\" nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Car Maintenance 2026\n** DONE Oil change\nCLOSED: [2026-01-10]\n:PROPERTIES:\n:MILEAGE: 50000\n:COST: 45\n:END:\n** TODO Tire rotation\nDEADLINE: <2026-04-01>\n:PROPERTIES:\n:MILEAGE: 50000\n:END:\n** TODO Brake inspection\nDEADLINE: <2026-06-01>")
  (let* ((tree (org-element-parse-buffer))
         (tasks (org-element-map tree 'headline
                  (lambda (h)
                    (list (org-element-property :raw-value h)
                          (org-element-property :todo-keyword h)
                          (org-entry-get nil "MILEAGE")
                          (org-entry-get nil "COST"))))))
    tasks))"##,
        expect,
    );
}
