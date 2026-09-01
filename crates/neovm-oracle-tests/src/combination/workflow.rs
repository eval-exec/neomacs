//! Oracle parity tests for workflow and pipeline patterns in Elisp.
//!
//! Tests data processing pipelines (ETL), validation with short-circuit,
//! map-reduce patterns, error recovery pipelines, workflow engines
//! with conditional branching, and audit trail logging.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Data processing pipeline with stages (extract, transform, load)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_workflow_etl_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; ETL pipeline: extract raw records, transform (clean/enrich), load (aggregate)
  (fset 'neovm--test-etl-extract
    (lambda (raw-data)
      ;; Parse "name:age:score" strings into plists
      (mapcar
       (lambda (record-str)
         (let* ((parts nil)
                (current "")
                (i 0)
                (len (length record-str)))
           ;; Split on ':'
           (while (< i len)
             (let ((ch (aref record-str i)))
               (if (= ch ?:)
                   (progn (setq parts (cons current parts))
                          (setq current ""))
                 (setq current (concat current (char-to-string ch)))))
             (setq i (1+ i)))
           (setq parts (nreverse (cons current parts)))
           (list :name (nth 0 parts)
                 :age (string-to-number (nth 1 parts))
                 :score (string-to-number (nth 2 parts))
                 :raw record-str)))
       raw-data)))

  (fset 'neovm--test-etl-transform
    (lambda (records)
      ;; Clean: filter invalid ages, enrich: add grade and age-group
      (let ((result nil))
        (dolist (rec records)
          (let ((age (plist-get rec :age))
                (score (plist-get rec :score)))
            (when (and (> age 0) (< age 150) (>= score 0) (<= score 100))
              (let* ((grade (cond ((>= score 90) "A")
                                  ((>= score 80) "B")
                                  ((>= score 70) "C")
                                  ((>= score 60) "D")
                                  (t "F")))
                     (age-group (cond ((< age 18) "youth")
                                      ((< age 30) "young-adult")
                                      ((< age 50) "adult")
                                      (t "senior")))
                     (enriched (append rec
                                       (list :grade grade
                                             :age-group age-group))))
                (setq result (cons enriched result))))))
        (nreverse result))))

  (fset 'neovm--test-etl-load
    (lambda (records)
      ;; Aggregate: count by grade, average score by age-group
      (let ((grade-counts (make-hash-table :test 'equal))
            (group-scores (make-hash-table :test 'equal))
            (group-counts (make-hash-table :test 'equal))
            (total 0))
        (dolist (rec records)
          (let ((grade (plist-get rec :grade))
                (group (plist-get rec :age-group))
                (score (plist-get rec :score)))
            (puthash grade (1+ (or (gethash grade grade-counts) 0)) grade-counts)
            (puthash group (+ score (or (gethash group group-scores) 0)) group-scores)
            (puthash group (1+ (or (gethash group group-counts) 0)) group-counts)
            (setq total (1+ total))))
        ;; Build summary
        (let ((grade-summary nil)
              (group-summary nil))
          (maphash (lambda (k v) (setq grade-summary (cons (cons k v) grade-summary)))
                   grade-counts)
          (maphash (lambda (k v)
                     (let ((cnt (gethash k group-counts)))
                       (setq group-summary
                             (cons (list k (/ v cnt) cnt) group-summary))))
                   group-scores)
          (list :total total
                :grades (sort grade-summary (lambda (a b) (string< (car a) (car b))))
                :groups (sort group-summary (lambda (a b) (string< (car a) (car b)))))))))

  (unwind-protect
      (let* ((raw-data '("Alice:25:92" "Bob:17:78" "Carol:45:85"
                          "Dave:0:50" "Eve:62:95" "Frank:30:65"
                          "Grace:22:43" "Heidi:55:88" "Ivan:200:99"))
             (extracted (funcall 'neovm--test-etl-extract raw-data))
             (transformed (funcall 'neovm--test-etl-transform extracted))
             (loaded (funcall 'neovm--test-etl-load transformed)))
        (list
          ;; Number of valid records after transform
          (length transformed)
          ;; Loaded aggregation result
          loaded
          ;; Verify invalid records were filtered (Dave age=0, Ivan age=200)
          (mapcar (lambda (r) (plist-get r :name)) transformed)))
    (fmakunbound 'neovm--test-etl-extract)
    (fmakunbound 'neovm--test-etl-transform)
    (fmakunbound 'neovm--test-etl-load)))"#;
    let expect = expect_test::expect![[
        r#""OK (7 (:total 7 :grades ((\"A\" . 2) (\"B\" . 2) (\"C\" . 1) (\"D\" . 1) (\"F\" . 1)) :groups ((\"adult\" 75 2) (\"senior\" 91 2) (\"young-adult\" 67 2) (\"youth\" 78 1))) (\"Alice\" \"Bob\" \"Carol\" \"Eve\" \"Frank\" \"Grace\" \"Heidi\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Validation pipeline with short-circuit on first error
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_workflow_validation_short_circuit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Each validator returns (ok . value) or (err . message)
  ;; Pipeline stops at first error
  (fset 'neovm--test-make-validator
    (lambda (name check-fn transform-fn)
      (list :name name :check check-fn :transform transform-fn)))

  (fset 'neovm--test-run-pipeline
    (lambda (validators input)
      (let ((value input)
            (steps-run nil)
            (error-found nil)
            (remaining validators))
        (while (and remaining (not error-found))
          (let* ((v (car remaining))
                 (name (plist-get v :name))
                 (check (plist-get v :check))
                 (transform (plist-get v :transform)))
            (setq steps-run (cons name steps-run))
            (if (funcall check value)
                (setq value (funcall transform value))
              (setq error-found (list 'validation-error name value))))
          (setq remaining (cdr remaining)))
        (if error-found
            (list 'error error-found (nreverse steps-run))
          (list 'ok value (nreverse steps-run))))))

  (unwind-protect
      (let* ((validators
              (list
               ;; 1. Must be a string
               (funcall 'neovm--test-make-validator "type-check"
                        #'stringp
                        (lambda (v) v))
               ;; 2. Must not be empty
               (funcall 'neovm--test-make-validator "non-empty"
                        (lambda (v) (> (length v) 0))
                        (lambda (v) v))
               ;; 3. Trim whitespace
               (funcall 'neovm--test-make-validator "trim"
                        (lambda (v) t)
                        (lambda (v) (string-trim v)))
               ;; 4. Must be <= 20 chars after trim
               (funcall 'neovm--test-make-validator "length-check"
                        (lambda (v) (<= (length v) 20))
                        (lambda (v) v))
               ;; 5. Upcase
               (funcall 'neovm--test-make-validator "upcase"
                        (lambda (v) t)
                        #'upcase))))
        (list
          ;; Valid input
          (funcall 'neovm--test-run-pipeline validators "  hello world  ")
          ;; Fails at type-check (not a string)
          (funcall 'neovm--test-run-pipeline validators 42)
          ;; Fails at non-empty
          (funcall 'neovm--test-run-pipeline validators "")
          ;; Fails at length-check (too long after trim)
          (funcall 'neovm--test-run-pipeline validators
                   "this is a very long string that exceeds twenty chars")
          ;; Already trimmed, short
          (funcall 'neovm--test-run-pipeline validators "OK")))
    (fmakunbound 'neovm--test-make-validator)
    (fmakunbound 'neovm--test-run-pipeline)))"#;
    let expect = expect_test::expect![[
        r#""OK ((ok \"HELLO WORLD\" (\"type-check\" \"non-empty\" \"trim\" \"length-check\" \"upcase\")) (error (validation-error \"type-check\" 42) (\"type-check\")) (error (validation-error \"non-empty\" \"\") (\"type-check\" \"non-empty\")) (error (validation-error \"length-check\" \"this is a very long string that exceeds twenty chars\") (\"type-check\" \"non-empty\" \"trim\" \"length-check\")) (ok \"OK\" (\"type-check\" \"non-empty\" \"trim\" \"length-check\" \"upcase\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Map-reduce pattern (split data, process, merge results)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_workflow_map_reduce() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Split a list into N roughly equal chunks
  (fset 'neovm--test-chunk-list
    (lambda (lst n)
      (let* ((total (length lst))
             (chunk-size (max 1 (/ total n)))
             (chunks nil)
             (remaining lst))
        (while remaining
          (let ((chunk nil) (i 0))
            (while (and remaining (< i chunk-size))
              (setq chunk (cons (car remaining) chunk))
              (setq remaining (cdr remaining))
              (setq i (1+ i)))
            (setq chunks (cons (nreverse chunk) chunks))))
        (nreverse chunks))))

  ;; Map phase: apply function to each chunk
  (fset 'neovm--test-map-phase
    (lambda (chunks map-fn)
      (mapcar map-fn chunks)))

  ;; Reduce phase: merge all intermediate results
  (fset 'neovm--test-reduce-phase
    (lambda (intermediates reduce-fn initial)
      (let ((acc initial))
        (dolist (result intermediates)
          (setq acc (funcall reduce-fn acc result)))
        acc)))

  ;; Word frequency map-reduce
  (fset 'neovm--test-word-freq-mapper
    (lambda (words)
      ;; Count words in this chunk, return alist
      (let ((counts nil))
        (dolist (w words)
          (let ((entry (assoc w counts)))
            (if entry
                (setcdr entry (1+ (cdr entry)))
              (setq counts (cons (cons w 1) counts)))))
        counts)))

  (fset 'neovm--test-word-freq-reducer
    (lambda (acc partial)
      ;; Merge partial counts into accumulator
      (dolist (pair partial)
        (let ((entry (assoc (car pair) acc)))
          (if entry
              (setcdr entry (+ (cdr entry) (cdr pair)))
            (setq acc (cons (cons (car pair) (cdr pair)) acc)))))
      acc))

  (unwind-protect
      (let* ((words '("the" "cat" "sat" "on" "the" "mat"
                       "the" "cat" "ate" "the" "rat"
                       "on" "the" "mat" "sat" "the" "cat"))
             ;; Split into 3 chunks
             (chunks (funcall 'neovm--test-chunk-list words 3))
             ;; Map phase
             (mapped (funcall 'neovm--test-map-phase chunks
                              'neovm--test-word-freq-mapper))
             ;; Reduce phase
             (reduced (funcall 'neovm--test-reduce-phase mapped
                               'neovm--test-word-freq-reducer nil))
             ;; Sort by frequency descending, then alphabetically
             (sorted (sort reduced
                          (lambda (a b)
                            (if (= (cdr a) (cdr b))
                                (string< (car a) (car b))
                              (> (cdr a) (cdr b))))))
             ;; Total word count from reduced
             (total-words (apply #'+ (mapcar #'cdr sorted))))
        (list
          ;; Number of chunks
          (length chunks)
          ;; Intermediate results (one per chunk)
          (length mapped)
          ;; Final sorted word frequencies
          sorted
          ;; Total words counted
          total-words
          ;; Top 3 most frequent
          (let ((top nil) (i 0))
            (while (and (< i 3) (nth i sorted))
              (setq top (cons (nth i sorted) top))
              (setq i (1+ i)))
            (nreverse top))))
    (fmakunbound 'neovm--test-chunk-list)
    (fmakunbound 'neovm--test-map-phase)
    (fmakunbound 'neovm--test-reduce-phase)
    (fmakunbound 'neovm--test-word-freq-mapper)
    (fmakunbound 'neovm--test-word-freq-reducer)))"#;
    let expect = expect_test::expect![[
        r#""OK (4 4 ((\"the\" . 6) (\"cat\" . 3) (\"mat\" . 2) (\"on\" . 2) (\"sat\" . 2) (\"ate\" . 1) (\"rat\" . 1)) 17 ((\"the\" . 6) (\"cat\" . 3) (\"mat\" . 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Pipeline with error recovery (skip failed items, continue)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_workflow_error_recovery_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Process items through stages; if a stage fails for an item,
  ;; log the error and skip that item (continue with the rest)
  (fset 'neovm--test-resilient-pipeline
    (lambda (stages items)
      (let ((results nil)
            (errors nil)
            (current-items items))
        (dolist (stage stages)
          (let ((stage-name (car stage))
                (stage-fn (cdr stage))
                (passed nil)
                (stage-errors nil))
            (dolist (item current-items)
              (let ((result (condition-case err
                                (cons 'ok (funcall stage-fn item))
                              (error (cons 'err (list stage-name item
                                                      (error-message-string err)))))))
                (if (eq (car result) 'ok)
                    (setq passed (cons (cdr result) passed))
                  (setq stage-errors (cons (cdr result) stage-errors)))))
            (setq current-items (nreverse passed))
            (when stage-errors
              (setq errors (append errors (nreverse stage-errors))))))
        (list :results (nreverse current-items)
              :errors errors
              :input-count (length items)
              :output-count (length current-items)
              :error-count (length errors)))))

  (unwind-protect
      (let* ((stages
              (list
               ;; Stage 1: parse number from string
               (cons "parse"
                     (lambda (item)
                       (let ((n (string-to-number item)))
                         (if (and (numberp n) (not (= n 0))
                                  (not (string= item "")))
                             n
                           (signal 'error (list (format "cannot parse: %s" item)))))))
               ;; Stage 2: must be positive
               (cons "positive-check"
                     (lambda (item)
                       (if (> item 0) item
                         (signal 'error (list (format "not positive: %d" item))))))
               ;; Stage 3: compute sqrt (approximate via Newton's method)
               (cons "sqrt"
                     (lambda (item)
                       (let ((guess (/ (float item) 2.0))
                             (iter 0))
                         (while (< iter 20)
                           (setq guess (/ (+ guess (/ (float item) guess)) 2.0))
                           (setq iter (1+ iter)))
                         (cons item (round (* guess 100))))))))
             (input '("25" "0" "abc" "16" "-3" "100" "49" "" "9")))
        (funcall 'neovm--test-resilient-pipeline stages input))
    (fmakunbound 'neovm--test-resilient-pipeline)))"#;
    let expect = expect_test::expect![[
        r#""OK (:results ((9 . 300) (49 . 700) (100 . 1000) (16 . 400) (25 . 500)) :errors ((\"parse\" \"0\" \"cannot parse: 0\") (\"parse\" \"abc\" \"cannot parse: abc\") (\"parse\" \"\" \"cannot parse: \") (\"positive-check\" -3 \"not positive: -3\")) :input-count 9 :output-count 1 :error-count 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Workflow engine with conditional branching
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_workflow_conditional_branching() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; A workflow engine where each node is either:
  ;;   (action NAME FN) - execute FN and go to next
  ;;   (branch NAME PRED TRUE-NEXT FALSE-NEXT) - conditional jump
  ;;   (end NAME) - terminal node
  ;; Workflow is a hash table of name -> node

  (defvar neovm--test-workflow nil)
  (setq neovm--test-workflow (make-hash-table :test 'eq))

  (fset 'neovm--test-add-action
    (lambda (name fn next)
      (puthash name (list 'action name fn next) neovm--test-workflow)))

  (fset 'neovm--test-add-branch
    (lambda (name pred true-next false-next)
      (puthash name (list 'branch name pred true-next false-next)
               neovm--test-workflow)))

  (fset 'neovm--test-add-end
    (lambda (name)
      (puthash name (list 'end name) neovm--test-workflow)))

  (fset 'neovm--test-run-workflow
    (lambda (start-node state)
      (let ((current start-node)
            (trace nil)
            (max-steps 50)
            (steps 0))
        (while (and current (< steps max-steps))
          (let ((node (gethash current neovm--test-workflow)))
            (unless node (signal 'error (list "unknown node" current)))
            (let ((kind (nth 0 node))
                  (name (nth 1 node)))
              (setq trace (cons name trace))
              (setq steps (1+ steps))
              (cond
                ((eq kind 'action)
                 (setq state (funcall (nth 2 node) state))
                 (setq current (nth 3 node)))
                ((eq kind 'branch)
                 (if (funcall (nth 2 node) state)
                     (setq current (nth 3 node))
                   (setq current (nth 4 node))))
                ((eq kind 'end)
                 (setq current nil))))))
        (list :state state :trace (nreverse trace) :steps steps))))

  (unwind-protect
      (progn
        ;; Build an order processing workflow:
        ;; start -> validate -> branch(amount>100) ->
        ;;   true: apply-discount -> check-stock -> branch(in-stock) ->
        ;;     true: ship -> end-success
        ;;     false: backorder -> end-backorder
        ;;   false: check-stock -> ...
        (funcall 'neovm--test-add-action 'start
                 (lambda (s) (plist-put s :status "processing"))
                 'validate)
        (funcall 'neovm--test-add-action 'validate
                 (lambda (s)
                   (plist-put s :validated t))
                 'check-amount)
        (funcall 'neovm--test-add-branch 'check-amount
                 (lambda (s) (> (plist-get s :amount) 100))
                 'apply-discount 'check-stock)
        (funcall 'neovm--test-add-action 'apply-discount
                 (lambda (s)
                   (let ((amt (plist-get s :amount)))
                     (plist-put s :discount (round (* amt 0.1)))
                     (plist-put s :amount (round (* amt 0.9)))))
                 'check-stock)
        (funcall 'neovm--test-add-branch 'check-stock
                 (lambda (s) (plist-get s :in-stock))
                 'ship 'backorder)
        (funcall 'neovm--test-add-action 'ship
                 (lambda (s) (plist-put s :status "shipped"))
                 'end-success)
        (funcall 'neovm--test-add-action 'backorder
                 (lambda (s) (plist-put s :status "backordered"))
                 'end-backorder)
        (funcall 'neovm--test-add-end 'end-success)
        (funcall 'neovm--test-add-end 'end-backorder)

        (list
          ;; Order > 100, in stock -> discount + ship
          (funcall 'neovm--test-run-workflow 'start
                   (list :amount 200 :in-stock t))
          ;; Order > 100, out of stock -> discount + backorder
          (funcall 'neovm--test-run-workflow 'start
                   (list :amount 150 :in-stock nil))
          ;; Order <= 100, in stock -> no discount, ship
          (funcall 'neovm--test-run-workflow 'start
                   (list :amount 50 :in-stock t))
          ;; Order <= 100, out of stock -> no discount, backorder
          (funcall 'neovm--test-run-workflow 'start
                   (list :amount 30 :in-stock nil))))
    (fmakunbound 'neovm--test-add-action)
    (fmakunbound 'neovm--test-add-branch)
    (fmakunbound 'neovm--test-add-end)
    (fmakunbound 'neovm--test-run-workflow)
    (makunbound 'neovm--test-workflow)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:state (:amount 180 :in-stock t :status \"shipped\" :validated t :discount 20) :trace (start validate check-amount apply-discount check-stock ship end-success) :steps 7) (:state (:amount 135 :in-stock nil :status \"backordered\" :validated t :discount 15) :trace (start validate check-amount apply-discount check-stock backorder end-backorder) :steps 7) (:state (:amount 50 :in-stock t :status \"shipped\" :validated t) :trace (start validate check-amount check-stock ship end-success) :steps 6) (:state (:amount 30 :in-stock nil :status \"backordered\" :validated t) :trace (start validate check-amount check-stock backorder end-backorder) :steps 6))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Audit trail: log every step with input/output
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_workflow_audit_trail() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; An audit-aware pipeline that records input/output at every stage
  (defvar neovm--test-audit-log nil)

  (fset 'neovm--test-audit-wrap
    (lambda (stage-name fn)
      ;; Returns a wrapped function that logs before/after
      (lambda (input)
        (let* ((output (funcall fn input))
               (entry (list :stage stage-name
                            :input input
                            :output output)))
          (setq neovm--test-audit-log
                (cons entry neovm--test-audit-log))
          output))))

  (fset 'neovm--test-run-audited-pipeline
    (lambda (stages input)
      (setq neovm--test-audit-log nil)
      (let ((value input))
        (dolist (stage stages)
          (setq value (funcall (cdr stage) value)))
        (list :result value
              :audit (nreverse neovm--test-audit-log)))))

  (unwind-protect
      (let* ((stages
              (list
               (cons "normalize"
                     (funcall 'neovm--test-audit-wrap "normalize"
                              (lambda (data)
                                ;; Normalize: downcase all string values in alist
                                (mapcar (lambda (pair)
                                          (if (stringp (cdr pair))
                                              (cons (car pair) (downcase (cdr pair)))
                                            pair))
                                        data))))
               (cons "validate"
                     (funcall 'neovm--test-audit-wrap "validate"
                              (lambda (data)
                                ;; Add a :valid flag to each entry
                                (mapcar (lambda (pair)
                                          (list (car pair) (cdr pair)
                                                (and (stringp (cdr pair))
                                                     (> (length (cdr pair)) 0))))
                                        data))))
               (cons "filter"
                     (funcall 'neovm--test-audit-wrap "filter"
                              (lambda (data)
                                ;; Keep only valid entries
                                (let ((result nil))
                                  (dolist (entry data)
                                    (when (nth 2 entry)
                                      (setq result (cons entry result))))
                                  (nreverse result)))))
               (cons "format"
                     (funcall 'neovm--test-audit-wrap "format"
                              (lambda (data)
                                ;; Format as "key=value" strings
                                (mapcar (lambda (entry)
                                          (format "%s=%s" (nth 0 entry) (nth 1 entry)))
                                        data))))))
             (input '((name . "Alice Smith") (email . "ALICE@EXAMPLE.COM")
                      (age . 30) (city . "New York") (zip . "")))
             (result (funcall 'neovm--test-run-audited-pipeline stages input)))
        (list
          ;; Final result
          (plist-get result :result)
          ;; Number of audit entries
          (length (plist-get result :audit))
          ;; Stage names in order
          (mapcar (lambda (entry) (plist-get entry :stage))
                  (plist-get result :audit))
          ;; Verify audit trail captures correct stage count
          (= (length stages) (length (plist-get result :audit)))))
    (fmakunbound 'neovm--test-audit-wrap)
    (fmakunbound 'neovm--test-run-audited-pipeline)
    (makunbound 'neovm--test-audit-log)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"name=alice smith\" \"email=alice@example.com\" \"city=new york\") 4 (\"normalize\" \"validate\" \"filter\" \"format\") t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
