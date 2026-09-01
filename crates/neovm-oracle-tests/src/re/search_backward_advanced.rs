//! Advanced oracle parity tests for `re-search-backward`:
//! BOUND, NOERROR, COUNT parameters, match data verification,
//! reverse log scanning, and opening delimiter matching.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic backward search from end of buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_re_search_backward_basic_from_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
  (insert "apple banana cherry date elderberry")
  (goto-char (point-max))
  (let ((pos (re-search-backward "\\b[a-z]+rry\\b" nil t)))
    (list pos (match-string 0) (point))))"#;
    let expect = expect_test::expect![[r#""OK (26 \"elderberry\" 26)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// BOUND parameter: don't search before position
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_re_search_backward_bound_param() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
  (insert "item-001 item-002 item-003 item-004 item-005")
  (goto-char (point-max))
  ;; BOUND=20 should prevent finding item-001 (at pos 1) and item-002 (at pos 10)
  (let ((found (re-search-backward "item-\\([0-9]+\\)" 20 t)))
    (list found
          (when found (match-string 0))
          (when found (match-string 1))
          ;; Now search without bound to verify earlier matches exist
          (progn
            (goto-char (point-max))
            (let ((all nil))
              (while (re-search-backward "item-\\([0-9]+\\)" nil t)
                (setq all (cons (match-string 1) all)))
              all)))))"#;
    let expect = expect_test::expect![[
        r#""OK (37 \"item-005\" \"005\" (\"001\" \"002\" \"003\" \"004\" \"005\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// NOERROR parameter: return nil instead of signaling
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_re_search_backward_noerror_param() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
  (insert "only plain text here no numbers")
  (goto-char (point-max))
  ;; With NOERROR=t, returns nil when not found
  (let ((result (re-search-backward "[0-9]+" nil t)))
    (list result (point)
          ;; Verify point doesn't move on failed search with NOERROR=t
          (let ((pos-before (point)))
            (re-search-backward "ZZZZNOTHERE" nil t)
            (= pos-before (point))))))"#;
    let expect = expect_test::expect![[r#""OK (nil 32 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// COUNT parameter: find Nth occurrence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_re_search_backward_count_param() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
  (insert "aaa-111 bbb-222 ccc-333 ddd-444 eee-555")
  (goto-char (point-max))
  ;; COUNT=3 means find the 3rd occurrence backward
  (let ((pos (re-search-backward "[a-z]+-[0-9]+" nil t 3)))
    (list pos
          (when pos (match-string 0))
          (when pos (point))
          ;; Also test COUNT=1 from same starting point for comparison
          (progn
            (goto-char (point-max))
            (re-search-backward "[a-z]+-[0-9]+" nil t 1)
            (match-string 0)))))"#;
    let expect = expect_test::expect![[r#""OK (19 \"c-333\" 19 \"e-555\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Match data set correctly after backward search
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_re_search_backward_match_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
  (insert "name:Alice age:30 role:developer name:Bob age:25 role:tester")
  (goto-char (point-max))
  (re-search-backward "\\(name\\):\\([^ ]+\\)" nil t)
  (let ((full (match-string 0))
        (key (match-string 1))
        (val (match-string 2))
        (beg0 (match-beginning 0))
        (end0 (match-end 0))
        (beg1 (match-beginning 1))
        (end1 (match-end 1))
        (beg2 (match-beginning 2))
        (end2 (match-end 2)))
    ;; Verify match data consistency
    (list full key val
          (= beg0 beg1)             ;; group 1 starts at group 0 start
          (< end1 beg2)             ;; group 1 ends before group 2 starts
          (= end0 end2)             ;; group 0 ends at group 2 end
          (- end0 beg0)             ;; total match length
          (buffer-substring beg0 end0))))"#;
    let expect =
        expect_test::expect![[r#""OK (\"name:Bob\" \"name\" \"Bob\" t t t 8 \"name:Bob\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple backward searches with alternating patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_re_search_backward_alternating_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
  (insert "START alpha=1 beta=2 gamma=3 MIDDLE delta=4 epsilon=5 zeta=6 END")
  (goto-char (point-max))
  ;; Collect all key=value pairs backward, also noting if they're before or after MIDDLE
  (let ((pairs nil)
        (middle-pos nil))
    ;; First find MIDDLE to know the boundary
    (save-excursion
      (goto-char (point-min))
      (when (re-search-forward "MIDDLE" nil t)
        (setq middle-pos (match-beginning 0))))
    ;; Now scan backward collecting pairs
    (while (re-search-backward "\\([a-z]+\\)=\\([0-9]+\\)" nil t)
      (let ((key (match-string 1))
            (val (match-string 2))
            (pos (match-beginning 0)))
        (setq pairs (cons (list key val (if (> pos middle-pos) "after" "before"))
                          pairs))))
    (list pairs (length pairs) middle-pos)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"a\" \"1\" \"before\") (\"a\" \"2\" \"before\") (\"a\" \"3\" \"before\") (\"a\" \"4\" \"after\") (\"n\" \"5\" \"after\") (\"a\" \"6\" \"after\")) 6 30)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: reverse scanning through log entries collecting timestamps
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_re_search_backward_reverse_log_scan() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--test-parse-log-backward
    (lambda (buf-content max-entries severity-filter)
      "Scan backward through log lines, collect up to MAX-ENTRIES matching SEVERITY-FILTER."
      (with-temp-buffer
        (insert buf-content)
        (goto-char (point-max))
        (let ((entries nil)
              (count 0)
              ;; Pattern: [TIMESTAMP] SEVERITY: message
              (pat "\\[\\([0-9:]+\\)\\] \\(INFO\\|WARN\\|ERROR\\): \\(.*\\)$"))
          (while (and (< count max-entries)
                      (re-search-backward pat nil t))
            (let ((timestamp (match-string 1))
                  (severity (match-string 2))
                  (message (match-string 3)))
              (when (or (null severity-filter)
                        (string= severity severity-filter))
                (setq entries (cons (list timestamp severity message) entries)
                      count (1+ count)))))
          entries))))

  (unwind-protect
      (let ((log-text "[09:01:00] INFO: Server started
[09:02:15] INFO: Connection from 10.0.0.1
[09:03:30] WARN: High memory usage detected
[09:04:45] ERROR: Database connection lost
[09:05:00] INFO: Retrying database connection
[09:05:30] ERROR: Retry failed after 3 attempts
[09:06:00] WARN: Switching to fallback mode
[09:06:30] INFO: Fallback mode active
[09:07:00] INFO: Service restored"))
        (list
         ;; Get last 3 entries (any severity)
         (funcall 'neovm--test-parse-log-backward log-text 3 nil)
         ;; Get all ERROR entries
         (funcall 'neovm--test-parse-log-backward log-text 100 "ERROR")
         ;; Get last 2 WARN entries
         (funcall 'neovm--test-parse-log-backward log-text 2 "WARN")
         ;; Get all entries
         (length (funcall 'neovm--test-parse-log-backward log-text 100 nil))))
    (fmakunbound 'neovm--test-parse-log-backward)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"09:06:00\" \"WARN\" \"Switching to fallback mode\") (\"09:06:30\" \"INFO\" \"Fallback mode active\") (\"09:07:00\" \"INFO\" \"Service restored\")) ((\"09:04:45\" \"ERROR\" \"Database connection lost\") (\"09:05:30\" \"ERROR\" \"Retry failed after 3 attempts\")) ((\"09:03:30\" \"WARN\" \"High memory usage detected\") (\"09:06:00\" \"WARN\" \"Switching to fallback mode\")) 9)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: find matching opening delimiter scanning backward
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_re_search_backward_find_opening_delimiter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--test-find-open-delim
    (lambda ()
      "From current point, scan backward to find matching open paren, tracking nesting."
      (let ((depth 1)
            (found nil))
        (while (and (> depth 0) (not found)
                    (re-search-backward "[()]" nil t))
          (let ((ch (char-after)))
            (cond
             ((= ch ?\)) (setq depth (1+ depth)))
             ((= ch ?\()
              (setq depth (1- depth))
              (when (= depth 0)
                (setq found (point)))))))
        found)))

  (unwind-protect
      (with-temp-buffer
        (insert "(defun outer (x)
  (let ((y (* x 2)))
    (if (> y 10)
        (+ y (inner (- y 5)))
      (- y 1))))")
        ;; Test 1: from after the last closing paren
        (goto-char (point-max))
        (let ((r1 (funcall 'neovm--test-find-open-delim)))
          ;; Test 2: position inside the (- y 5) form, after the closing paren
          (goto-char (point-min))
          (search-forward "(- y 5)" nil t)
          (let ((r2 (funcall 'neovm--test-find-open-delim)))
            ;; Test 3: from after (> y 10)
            (goto-char (point-min))
            (search-forward "(> y 10)" nil t)
            (let ((r3 (funcall 'neovm--test-find-open-delim)))
              ;; Return positions and the text starting at each found position
              (list
               (list r1 (when r1 (buffer-substring r1 (min (+ r1 12) (point-max)))))
               (list r2 (when r2 (buffer-substring r2 (min (+ r2 12) (point-max)))))
               (list r3 (when r3 (buffer-substring r3 (min (+ r3 12) (point-max))))))))))
    (fmakunbound 'neovm--test-find-open-delim)))"#;
    let expect =
        expect_test::expect![[r#""OK ((nil nil) (69 \"(inner (- y \") (43 \"(if (> y 10)\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Backward search with groups and replace simulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_re_search_backward_group_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
  (insert "TODO(alice): fix login bug\nFIXME(bob): update docs\nTODO(carol): add tests\nHACK(dave): workaround for issue #42\nTODO(eve): refactor parser")
  (goto-char (point-max))
  ;; Collect all TODO/FIXME/HACK annotations with author and description
  (let ((annotations nil))
    (while (re-search-backward "\\(TODO\\|FIXME\\|HACK\\)(\\([^)]+\\)): \\(.+\\)$" nil t)
      (let ((kind (match-string 1))
            (author (match-string 2))
            (desc (match-string 3))
            (line-start (match-beginning 0)))
        (setq annotations (cons (list kind author desc line-start) annotations))))
    ;; Sort by kind, then summarize
    (let ((todo-count 0) (fixme-count 0) (hack-count 0))
      (dolist (a annotations)
        (cond
         ((string= (car a) "TODO") (setq todo-count (1+ todo-count)))
         ((string= (car a) "FIXME") (setq fixme-count (1+ fixme-count)))
         ((string= (car a) "HACK") (setq hack-count (1+ hack-count)))))
      (list annotations
            (list 'todo todo-count 'fixme fixme-count 'hack hack-count)
            (length annotations)))))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"TODO\" \"alice\" \"fix login bug\" 1) (\"FIXME\" \"bob\" \"update docs\" 28) (\"TODO\" \"carol\" \"add tests\" 52) (\"HACK\" \"dave\" \"workaround for issue #42\" 75) (\"TODO\" \"eve\" \"refactor parser\" 112)) (todo 3 fixme 1 hack 1) 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
