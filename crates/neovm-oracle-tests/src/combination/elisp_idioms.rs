//! Oracle parity tests for common Elisp idioms and patterns:
//! `or` as default value, `and` as short-circuit guard,
//! association list CRUD, property list manipulation,
//! mapcar+lambda pipelines, buffer-local simulation,
//! while+accumulator iteration, push/pop stack ops,
//! and string building with mapconcat+format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// `or` as default value pattern
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_idiom_or_default_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((config '((name . "Alice") (theme . nil) (indent . 4))))
  ;; (or val default) idiom: use nil-valued entries' defaults
  (let ((name   (or (cdr (assq 'name config))   "Unknown"))
        (theme  (or (cdr (assq 'theme config))   "light"))
        (indent (or (cdr (assq 'indent config))  2))
        (lang   (or (cdr (assq 'lang config))    "en")))
    ;; theme is nil in config, so default "light" is used
    ;; lang is missing from config, so default "en" is used
    ;; name and indent have values and are used as-is
    (list name theme indent lang)))"#;
    let expect = expect_test::expect![[r#""OK (\"Alice\" \"light\" 4 \"en\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// `and` as short-circuit guard
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_idiom_and_short_circuit_guard() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((data '((user . ((name . "Alice") (age . 30) (active . t)))
                       (user . ((name . "Bob") (age . 17) (active . t)))
                       (user . ((name . "Carol") (age . 25) (active . nil)))
                       (user . ((name . "Dave") (age . 40) (active . t))))))
  ;; and-chain: check type, check active, check age >= 18, then collect name
  (let ((eligible nil))
    (dolist (entry data)
      (let ((info (cdr entry)))
        (and (consp info)
             (cdr (assq 'active info))
             (>= (cdr (assq 'age info)) 18)
             (setq eligible
                   (cons (cdr (assq 'name info)) eligible)))))
    (nreverse eligible)))"#;
    let expect = expect_test::expect![[r#""OK (\"Alice\" \"Dave\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Association list as simple database with CRUD operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_idiom_alist_crud() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((db nil)
                        (log nil))
  ;; CREATE: add entries
  (setq db (cons (cons "id1" '(name "Alice" score 90)) db))
  (setq db (cons (cons "id2" '(name "Bob" score 75)) db))
  (setq db (cons (cons "id3" '(name "Carol" score 88)) db))
  (setq log (cons (list 'after-create (length db)) log))
  ;; READ: look up by key
  (let ((bob-entry (assoc "id2" db)))
    (setq log (cons (list 'read-bob (cdr bob-entry)) log)))
  ;; UPDATE: modify Bob's score
  (let ((bob-entry (assoc "id2" db)))
    (when bob-entry
      (setcdr bob-entry (list 'name "Bob" 'score 95))))
  (let ((bob-entry (assoc "id2" db)))
    (setq log (cons (list 'updated-bob (cdr bob-entry)) log)))
  ;; DELETE: remove id1
  (setq db (let ((result nil))
             (dolist (entry db)
               (unless (string= (car entry) "id1")
                 (setq result (cons entry result))))
             (nreverse result)))
  (setq log (cons (list 'after-delete (length db)
                        (mapcar #'car db))
                  log))
  (nreverse log))"#;
    let expect = expect_test::expect![[
        r#""OK ((after-create 3) (read-bob (name \"Bob\" score 75)) (updated-bob (name \"Bob\" score 95)) (after-delete 2 (\"id3\" \"id2\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Property list manipulation idioms (plist-get/put chains)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_idiom_plist_manipulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((props '(:name "widget" :width 100 :height 50 :visible t)))
  ;; Read properties
  (let ((name (plist-get props :name))
        (w (plist-get props :width))
        (h (plist-get props :height)))
    ;; Update: set new values (plist-put returns updated plist)
    (setq props (plist-put props :width (* w 2)))
    (setq props (plist-put props :height (* h 2)))
    ;; Add new property
    (setq props (plist-put props :color "blue"))
    ;; Remove :visible by rebuilding without it
    (let ((cleaned nil))
      (while props
        (let ((key (car props))
              (val (cadr props)))
          (unless (eq key :visible)
            (setq cleaned (append cleaned (list key val)))))
        (setq props (cddr props)))
      (list name
            (plist-get cleaned :width)
            (plist-get cleaned :height)
            (plist-get cleaned :color)
            (plist-get cleaned :visible)))))"#;
    let expect = expect_test::expect![[r#""OK (\"widget\" 200 100 \"blue\" nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapcar + lambda filtering/transformation pipelines
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_idiom_mapcar_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((data '(1 2 3 4 5 6 7 8 9 10)))
  ;; Pipeline: square -> filter evens -> format as strings -> join
  (let* ((squared (mapcar (lambda (x) (* x x)) data))
         ;; Filter: keep only even results
         (evens (let ((result nil))
                  (dolist (x squared)
                    (when (= (% x 2) 0)
                      (setq result (cons x result))))
                  (nreverse result)))
         ;; Transform to strings
         (strings (mapcar (lambda (x) (format "%03d" x)) evens))
         ;; Join with separator
         (joined (mapconcat #'identity strings ", ")))
    (list squared evens strings joined)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 4 9 16 25 36 49 64 81 100) (4 16 36 64 100) (\"004\" \"016\" \"036\" \"064\" \"100\") \"004, 016, 036, 064, 100\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer-local-like variable simulation with alist
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_idiom_buffer_local_simulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((global-defaults '((indent-width . 4) (tab-mode . nil) (line-wrap . t)))
                        (buffer-locals (make-hash-table :test 'equal)))
  ;; Simulate buffer-local variables: per-buffer alists override globals
  (let ((make-local
         (lambda (buf-name var val)
           (let ((buf-alist (gethash buf-name buffer-locals nil)))
             (let ((existing (assq var buf-alist)))
               (if existing
                   (setcdr existing val)
                 (setq buf-alist (cons (cons var val) buf-alist))))
             (puthash buf-name buf-alist buffer-locals))))
        (get-var
         (lambda (buf-name var)
           (let* ((buf-alist (gethash buf-name buffer-locals nil))
                  (local (assq var buf-alist)))
             (if local
                 (cdr local)
               (cdr (assq var global-defaults)))))))
    ;; Set up local overrides for two buffers
    (funcall make-local "main.py" 'indent-width 2)
    (funcall make-local "main.py" 'tab-mode t)
    (funcall make-local "notes.md" 'line-wrap nil)
    ;; Query variables from different buffers
    (list
     ;; main.py: local indent-width=2, local tab-mode=t, global line-wrap=t
     (list (funcall get-var "main.py" 'indent-width)
           (funcall get-var "main.py" 'tab-mode)
           (funcall get-var "main.py" 'line-wrap))
     ;; notes.md: global indent-width=4, global tab-mode=nil, local line-wrap=nil
     (list (funcall get-var "notes.md" 'indent-width)
           (funcall get-var "notes.md" 'tab-mode)
           (funcall get-var "notes.md" 'line-wrap))
     ;; unknown.txt: all globals
     (list (funcall get-var "unknown.txt" 'indent-width)
           (funcall get-var "unknown.txt" 'tab-mode)
           (funcall get-var "unknown.txt" 'line-wrap)))))"#;
    let expect = expect_test::expect![[r#""OK ((2 t t) (4 nil nil) (4 nil t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// while + multiple accumulators iteration
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_idiom_while_multiple_accumulators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((numbers '(3 -1 4 -1 5 -9 2 -6 5 3 -5 8 9 -7 9))
                        (pos-sum 0) (neg-sum 0)
                        (pos-count 0) (neg-count 0)
                        (max-val nil) (min-val nil)
                        (running nil)
                        (lst nil))
  (setq lst numbers)
  (let ((running-total 0))
    (while lst
      (let ((n (car lst)))
        ;; Accumulate positive vs negative
        (if (>= n 0)
            (progn (setq pos-sum (+ pos-sum n))
                   (setq pos-count (1+ pos-count)))
          (setq neg-sum (+ neg-sum n))
          (setq neg-count (1+ neg-count)))
        ;; Track min and max
        (when (or (null max-val) (> n max-val))
          (setq max-val n))
        (when (or (null min-val) (< n min-val))
          (setq min-val n))
        ;; Running total
        (setq running-total (+ running-total n))
        (setq running (cons running-total running)))
      (setq lst (cdr lst))))
  (list (list 'pos pos-sum pos-count)
        (list 'neg neg-sum neg-count)
        (list 'range min-val max-val)
        (list 'running (nreverse running))))"#;
    let expect = expect_test::expect![[
        r#""OK ((pos 48 9) (neg -29 6) (range -9 9) (running (3 2 6 5 10 1 3 -3 2 5 0 8 17 10 19)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// push/pop stack operations via cons/car/cdr
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_idiom_stack_push_pop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((stack nil)
                        (trace nil))
  ;; Simulate RPN calculator: "3 4 + 2 * 5 -"
  ;; Push 3
  (push 3 stack)
  (setq trace (cons (list 'push 3 (copy-sequence stack)) trace))
  ;; Push 4
  (push 4 stack)
  (setq trace (cons (list 'push 4 (copy-sequence stack)) trace))
  ;; Pop two, add, push result
  (let ((b (pop stack))
        (a (pop stack)))
    (push (+ a b) stack)
    (setq trace (cons (list '+ a b (car stack)) trace)))
  ;; Push 2
  (push 2 stack)
  (setq trace (cons (list 'push 2 (copy-sequence stack)) trace))
  ;; Pop two, multiply, push result
  (let ((b (pop stack))
        (a (pop stack)))
    (push (* a b) stack)
    (setq trace (cons (list '* a b (car stack)) trace)))
  ;; Push 5
  (push 5 stack)
  (setq trace (cons (list 'push 5 (copy-sequence stack)) trace))
  ;; Pop two, subtract, push result
  (let ((b (pop stack))
        (a (pop stack)))
    (push (- a b) stack)
    (setq trace (cons (list '- a b (car stack)) trace)))
  (list (car stack) (nreverse trace)))"#;
    let expect = expect_test::expect![[
        r#""OK (9 ((push 3 (3)) (push 4 (4 3)) (+ 3 4 7) (push 2 (2 7)) (* 7 2 14) (push 5 (5 14)) (- 14 5 9)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// String building with mapconcat + format
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_idiom_mapconcat_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((people '(("Alice" 30 "engineer")
                                   ("Bob" 25 "designer")
                                   ("Carol" 35 "manager")
                                   ("Dave" 28 "developer"))))
  ;; Build a formatted table using mapconcat
  (let ((header (format "%-10s %4s  %-12s" "Name" "Age" "Role"))
        (separator (make-string 30 ?-))
        (rows (mapconcat
               (lambda (person)
                 (format "%-10s %4d  %-12s"
                         (nth 0 person)
                         (nth 1 person)
                         (nth 2 person)))
               people "\n"))
        ;; Summary line
        (summary (format "Total: %d people, avg age: %.1f"
                         (length people)
                         (/ (float (apply #'+ (mapcar #'cadr people)))
                            (length people)))))
    (mapconcat #'identity
               (list header separator rows separator summary)
               "\n")))"#;
    let expect = expect_test::expect![[
        r#""OK \"Name        Age  Role        \\n------------------------------\\nAlice        30  engineer    \\nBob          25  designer    \\nCarol        35  manager     \\nDave         28  developer   \\n------------------------------\\nTotal: 4 people, avg age: 29.5\"""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: multi-step data processing pipeline combining idioms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_idiom_full_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((raw-data '(("math" 85) ("math" 92) ("science" 78)
                                     ("math" 70) ("science" 95) ("science" 88)
                                     ("art" 100) ("art" 65) ("art" 90))))
  ;; Step 1: Group by subject using alist
  (let ((groups nil))
    (dolist (entry raw-data)
      (let* ((subject (car entry))
             (score (cadr entry))
             (existing (assoc subject groups)))
        (if existing
            (setcdr existing (cons score (cdr existing)))
          (setq groups (cons (cons subject (list score)) groups)))))
    ;; Step 2: Compute stats per group
    (let ((stats
           (mapcar
            (lambda (group)
              (let* ((subject (car group))
                     (scores (cdr group))
                     (n (length scores))
                     (total (apply #'+ scores))
                     (avg (/ (float total) n))
                     (max-s (apply #'max scores))
                     (min-s (apply #'min scores)))
                (list subject
                      (cons 'count n)
                      (cons 'avg avg)
                      (cons 'max max-s)
                      (cons 'min min-s))))
            groups)))
      ;; Step 3: Sort by average score descending
      (sort stats (lambda (a b)
                    (> (cdr (nth 2 a))
                       (cdr (nth 2 b))))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"science\" (count . 3) (avg . 87.0) (max . 95) (min . 78)) (\"art\" (count . 3) (avg . 85.0) (max . 100) (min . 65)) (\"math\" (count . 3) (avg . 82.33333333333333) (max . 92) (min . 70)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
