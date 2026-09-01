//! Oracle parity tests for real-world Elisp patterns:
//! configuration system with defvar + alist storage,
//! ring buffer implementation using vector + indices,
//! event system with handler registration and dispatch,
//! text indentation engine,
//! and undo system with reverse application.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Configuration system: defvar + alist storage + getter/setter functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rw_configuration_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--test-config-store nil "Alist of config values.")
  (defvar neovm--test-config-defaults nil "Alist of config defaults.")
  (defvar neovm--test-config-validators nil "Alist of config validators.")
  (defvar neovm--test-config-changelog nil "Log of config changes.")

  (fset 'neovm--test-config-register
    (lambda (key default &optional validator)
      (setq neovm--test-config-defaults
            (cons (cons key default) neovm--test-config-defaults))
      (when validator
        (setq neovm--test-config-validators
              (cons (cons key validator) neovm--test-config-validators)))))

  (fset 'neovm--test-config-get
    (lambda (key)
      (let ((entry (assq key neovm--test-config-store)))
        (if entry
            (cdr entry)
          (let ((def (assq key neovm--test-config-defaults)))
            (and def (cdr def)))))))

  (fset 'neovm--test-config-set
    (lambda (key value)
      (let ((validator (cdr (assq key neovm--test-config-validators))))
        (when (and validator (not (funcall validator value)))
          (error "Validation failed for %s" key))
        (let ((old (funcall 'neovm--test-config-get key))
              (entry (assq key neovm--test-config-store)))
          (if entry
              (setcdr entry value)
            (setq neovm--test-config-store
                  (cons (cons key value) neovm--test-config-store)))
          (setq neovm--test-config-changelog
                (cons (list key old value) neovm--test-config-changelog))
          value))))

  (fset 'neovm--test-config-reset
    (lambda (key)
      (let ((def (assq key neovm--test-config-defaults)))
        (when def
          (funcall 'neovm--test-config-set key (cdr def))))))

  (unwind-protect
      (progn
        ;; Register config options with defaults and validators
        (funcall 'neovm--test-config-register 'indent-width 4
                 (lambda (v) (and (integerp v) (>= v 1) (<= v 16))))
        (funcall 'neovm--test-config-register 'use-tabs nil)
        (funcall 'neovm--test-config-register 'line-length 80
                 (lambda (v) (and (integerp v) (>= v 40))))
        (funcall 'neovm--test-config-register 'theme "light")

        ;; Get defaults
        (let ((d1 (funcall 'neovm--test-config-get 'indent-width))
              (d2 (funcall 'neovm--test-config-get 'theme)))
          ;; Set custom values
          (funcall 'neovm--test-config-set 'indent-width 2)
          (funcall 'neovm--test-config-set 'theme "dark")
          (funcall 'neovm--test-config-set 'line-length 120)

          ;; Read back
          (let ((v1 (funcall 'neovm--test-config-get 'indent-width))
                (v2 (funcall 'neovm--test-config-get 'theme))
                (v3 (funcall 'neovm--test-config-get 'line-length)))
            ;; Reset one to default
            (funcall 'neovm--test-config-reset 'indent-width)
            (let ((v4 (funcall 'neovm--test-config-get 'indent-width)))
              ;; Test validation failure
              (let ((err-result
                     (condition-case err
                         (progn
                           (funcall 'neovm--test-config-set 'indent-width -5)
                           'no-error)
                       (error (list 'validation-error (cadr err))))))
                (list (list 'defaults d1 d2)
                      (list 'custom v1 v2 v3)
                      (list 'after-reset v4)
                      (list 'validation err-result)
                      (list 'changelog-length (length neovm--test-config-changelog))))))))
    (fmakunbound 'neovm--test-config-register)
    (fmakunbound 'neovm--test-config-get)
    (fmakunbound 'neovm--test-config-set)
    (fmakunbound 'neovm--test-config-reset)
    (makunbound 'neovm--test-config-store)
    (makunbound 'neovm--test-config-defaults)
    (makunbound 'neovm--test-config-validators)
    (makunbound 'neovm--test-config-changelog)))"#;
    let expect = expect_test::expect![[
        r#""OK ((defaults 4 \"light\") (custom 2 \"dark\" 120) (after-reset 4) (validation (validation-error \"Validation failed for indent-width\")) (changelog-length 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Ring buffer implementation using vector + two indices
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rw_ring_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Ring buffer: (vector data head tail count capacity)
  (fset 'neovm--test-ring-create
    (lambda (capacity)
      (vector (make-vector capacity nil) 0 0 0 capacity)))

  (fset 'neovm--test-ring-push
    (lambda (ring item)
      (let ((data (aref ring 0))
            (tail (aref ring 2))
            (count (aref ring 3))
            (cap (aref ring 4)))
        (aset data tail item)
        (aset ring 2 (% (1+ tail) cap))
        (if (= count cap)
            ;; Overwrite: advance head too
            (aset ring 1 (% (1+ (aref ring 1)) cap))
          (aset ring 3 (1+ count)))
        ring)))

  (fset 'neovm--test-ring-pop
    (lambda (ring)
      (let ((count (aref ring 3)))
        (if (= count 0)
            nil
          (let* ((data (aref ring 0))
                 (head (aref ring 1))
                 (item (aref data head)))
            (aset data head nil)
            (aset ring 1 (% (1+ head) (aref ring 4)))
            (aset ring 3 (1- count))
            item)))))

  (fset 'neovm--test-ring-to-list
    (lambda (ring)
      (let ((data (aref ring 0))
            (head (aref ring 1))
            (count (aref ring 3))
            (cap (aref ring 4))
            (result nil))
        (let ((i 0))
          (while (< i count)
            (setq result
                  (cons (aref data (% (+ head i) cap)) result))
            (setq i (1+ i))))
        (nreverse result))))

  (unwind-protect
      (let ((ring (funcall 'neovm--test-ring-create 4))
            (trace nil))
        ;; Push items
        (funcall 'neovm--test-ring-push ring 'a)
        (funcall 'neovm--test-ring-push ring 'b)
        (funcall 'neovm--test-ring-push ring 'c)
        (setq trace (cons (list 'after-abc
                                (funcall 'neovm--test-ring-to-list ring))
                          trace))
        ;; Push one more (still within capacity)
        (funcall 'neovm--test-ring-push ring 'd)
        (setq trace (cons (list 'after-d
                                (funcall 'neovm--test-ring-to-list ring))
                          trace))
        ;; Push beyond capacity — overwrites oldest
        (funcall 'neovm--test-ring-push ring 'e)
        (funcall 'neovm--test-ring-push ring 'f)
        (setq trace (cons (list 'after-ef
                                (funcall 'neovm--test-ring-to-list ring))
                          trace))
        ;; Pop items
        (let ((p1 (funcall 'neovm--test-ring-pop ring))
              (p2 (funcall 'neovm--test-ring-pop ring)))
          (setq trace (cons (list 'popped p1 p2
                                  (funcall 'neovm--test-ring-to-list ring))
                            trace)))
        ;; Push after pops
        (funcall 'neovm--test-ring-push ring 'g)
        (funcall 'neovm--test-ring-push ring 'h)
        (funcall 'neovm--test-ring-push ring 'i)
        (setq trace (cons (list 'final
                                (funcall 'neovm--test-ring-to-list ring))
                          trace))
        (nreverse trace))
    (fmakunbound 'neovm--test-ring-create)
    (fmakunbound 'neovm--test-ring-push)
    (fmakunbound 'neovm--test-ring-pop)
    (fmakunbound 'neovm--test-ring-to-list)))"#;
    let expect = expect_test::expect![[
        r#""OK ((after-abc (a b c)) (after-d (a b c d)) (after-ef (c d e f)) (popped c d (e f)) (final (f g h i)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Simple event system: register handlers, dispatch events, collect results
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rw_event_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--test-event-handlers (make-hash-table) "Event handler registry.")
  (defvar neovm--test-event-log nil "Event dispatch log.")

  (fset 'neovm--test-event-on
    (lambda (event handler &optional priority)
      (let ((priority (or priority 0))
            (handlers (gethash event neovm--test-event-handlers nil)))
        (puthash event
                 (sort (cons (cons priority handler) handlers)
                       (lambda (a b) (> (car a) (car b))))
                 neovm--test-event-handlers))))

  (fset 'neovm--test-event-emit
    (lambda (event &rest data)
      (let ((handlers (gethash event neovm--test-event-handlers nil))
            (results nil)
            (stopped nil))
        (dolist (entry handlers)
          (unless stopped
            (let ((result (apply (cdr entry) data)))
              (when (eq result 'stop-propagation)
                (setq stopped t))
              (setq results (cons result results)))))
        (setq neovm--test-event-log
              (cons (list event data (nreverse results) stopped)
                    neovm--test-event-log))
        (nreverse results))))

  (unwind-protect
      (progn
        ;; Register handlers with priorities
        (funcall 'neovm--test-event-on 'click
                 (lambda (x y)
                   (format "handler-A at (%d,%d)" x y))
                 10)
        (funcall 'neovm--test-event-on 'click
                 (lambda (x y)
                   (format "handler-B at (%d,%d)" x y))
                 5)
        (funcall 'neovm--test-event-on 'click
                 (lambda (x y)
                   (format "handler-C at (%d,%d)" x y))
                 1)

        ;; Handler that stops propagation
        (funcall 'neovm--test-event-on 'keypress
                 (lambda (key)
                   (if (string= key "Escape")
                       'stop-propagation
                     (format "key-high: %s" key)))
                 10)
        (funcall 'neovm--test-event-on 'keypress
                 (lambda (key) (format "key-low: %s" key))
                 1)

        ;; Dispatch events
        (let ((r1 (funcall 'neovm--test-event-emit 'click 100 200))
              (r2 (funcall 'neovm--test-event-emit 'keypress "Enter"))
              (r3 (funcall 'neovm--test-event-emit 'keypress "Escape"))
              (r4 (funcall 'neovm--test-event-emit 'unknown-event)))
          (list (list 'click-results r1)
                (list 'enter-results r2)
                (list 'escape-results r3)
                (list 'unknown-results r4)
                (list 'log-length (length neovm--test-event-log)))))
    (fmakunbound 'neovm--test-event-on)
    (fmakunbound 'neovm--test-event-emit)
    (makunbound 'neovm--test-event-handlers)
    (makunbound 'neovm--test-event-log)))"#;
    let expect = expect_test::expect![[
        r#""OK ((click-results (\"handler-C at (100,200)\")) (enter-results (\"key-low: Enter\")) (escape-results (stop-propagation)) (unknown-results nil) (log-length 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Text indentation engine: parse lines, compute indent levels, reindent
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rw_indentation_engine() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  ;; Insert poorly indented code
  (insert "begin\n")
  (insert "x = 1\n")
  (insert "if true\n")
  (insert "y = 2\n")
  (insert "if nested\n")
  (insert "z = 3\n")
  (insert "end\n")
  (insert "end\n")
  (insert "w = 4\n")
  (insert "end\n")
  ;; Indentation rules:
  ;; - "begin", "if.*" increase indent for subsequent lines
  ;; - "end" decreases indent for this line AND continues at decreased level
  (goto-char (point-min))
  (let ((indent-level 0)
        (indent-size 2)
        (result nil))
    (while (not (eobp))
      (let* ((line-start (line-beginning-position))
             (line-end (line-end-position))
             (raw-line (buffer-substring line-start line-end))
             ;; Trim leading whitespace
             (trimmed (if (string-match "^[ \t]*\\(.*\\)$" raw-line)
                         (match-string 1 raw-line)
                       raw-line)))
        ;; If line is "end", decrease indent before applying
        (when (string= trimmed "end")
          (setq indent-level (max 0 (1- indent-level))))
        ;; Record the properly indented line
        (let ((indented (concat (make-string (* indent-level indent-size) ?\s)
                                trimmed)))
          (setq result (cons indented result)))
        ;; If line starts with "begin" or "if", increase indent for next line
        (when (or (string= trimmed "begin")
                  (string-match "^if " trimmed)
                  (string= trimmed "if"))
          (setq indent-level (1+ indent-level))))
      (forward-line 1))
    ;; Return the reindented lines and rebuild the buffer
    (let ((indented-lines (nreverse result)))
      (erase-buffer)
      (dolist (line indented-lines)
        (insert line "\n"))
      (list indented-lines (buffer-string)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"begin\" \"  x = 1\" \"  if true\" \"    y = 2\" \"    if nested\" \"      z = 3\" \"    end\" \"  end\" \"  w = 4\" \"end\") \"begin\\n  x = 1\\n  if true\\n    y = 2\\n    if nested\\n      z = 3\\n    end\\n  end\\n  w = 4\\nend\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Undo system: record operations, apply undo in reverse
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rw_undo_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--test-undo-doc nil "The document state (list of chars as strings).")
  (defvar neovm--test-undo-stack nil "Stack of undo records.")

  (fset 'neovm--test-undo-init
    (lambda (text)
      (setq neovm--test-undo-doc
            (mapcar #'string (append text nil)))
      (setq neovm--test-undo-stack nil)))

  (fset 'neovm--test-undo-doc-string
    (lambda ()
      (apply #'concat neovm--test-undo-doc)))

  (fset 'neovm--test-undo-insert-at
    (lambda (pos char-str)
      ;; Record undo as (delete pos)
      (setq neovm--test-undo-stack
            (cons (list 'delete pos) neovm--test-undo-stack))
      ;; Perform insert
      (let ((before (let ((r nil) (i 0))
                      (while (< i pos)
                        (setq r (cons (nth i neovm--test-undo-doc) r))
                        (setq i (1+ i)))
                      (nreverse r)))
            (after (nthcdr pos neovm--test-undo-doc)))
        (setq neovm--test-undo-doc
              (append before (list char-str) after)))))

  (fset 'neovm--test-undo-delete-at
    (lambda (pos)
      ;; Record undo as (insert pos char)
      (let ((deleted (nth pos neovm--test-undo-doc)))
        (setq neovm--test-undo-stack
              (cons (list 'insert pos deleted) neovm--test-undo-stack))
        ;; Perform delete
        (let ((before (let ((r nil) (i 0))
                        (while (< i pos)
                          (setq r (cons (nth i neovm--test-undo-doc) r))
                          (setq i (1+ i)))
                        (nreverse r)))
              (after (nthcdr (1+ pos) neovm--test-undo-doc)))
          (setq neovm--test-undo-doc (append before after))))))

  (fset 'neovm--test-undo-replace-at
    (lambda (pos new-char-str)
      (let ((old (nth pos neovm--test-undo-doc)))
        ;; Record undo as (replace pos old-char)
        (setq neovm--test-undo-stack
              (cons (list 'replace pos old) neovm--test-undo-stack))
        ;; Perform replace by rebuilding
        (let ((result nil) (i 0))
          (dolist (ch neovm--test-undo-doc)
            (if (= i pos)
                (setq result (cons new-char-str result))
              (setq result (cons ch result)))
            (setq i (1+ i)))
          (setq neovm--test-undo-doc (nreverse result))))))

  (fset 'neovm--test-undo-do-undo
    (lambda ()
      (when neovm--test-undo-stack
        (let ((record (car neovm--test-undo-stack)))
          (setq neovm--test-undo-stack (cdr neovm--test-undo-stack))
          (cond
           ((eq (car record) 'insert)
            ;; Re-insert the deleted character (don't record reverse undo)
            (let ((pos (nth 1 record))
                  (ch (nth 2 record)))
              (let ((before (let ((r nil) (i 0))
                              (while (< i pos)
                                (setq r (cons (nth i neovm--test-undo-doc) r))
                                (setq i (1+ i)))
                              (nreverse r)))
                    (after (nthcdr pos neovm--test-undo-doc)))
                (setq neovm--test-undo-doc
                      (append before (list ch) after)))))
           ((eq (car record) 'delete)
            ;; Remove the inserted character
            (let ((pos (nth 1 record)))
              (let ((before (let ((r nil) (i 0))
                              (while (< i pos)
                                (setq r (cons (nth i neovm--test-undo-doc) r))
                                (setq i (1+ i)))
                              (nreverse r)))
                    (after (nthcdr (1+ pos) neovm--test-undo-doc)))
                (setq neovm--test-undo-doc (append before after)))))
           ((eq (car record) 'replace)
            ;; Restore old character
            (let ((pos (nth 1 record))
                  (old-ch (nth 2 record)))
              (let ((result nil) (i 0))
                (dolist (ch neovm--test-undo-doc)
                  (if (= i pos)
                      (setq result (cons old-ch result))
                    (setq result (cons ch result)))
                  (setq i (1+ i)))
                (setq neovm--test-undo-doc (nreverse result))))))))))

  (unwind-protect
      (progn
        (funcall 'neovm--test-undo-init "Hello")
        (let ((trace nil))
          (setq trace (cons (list 'init (funcall 'neovm--test-undo-doc-string)) trace))
          ;; Insert "!" at end (pos 5)
          (funcall 'neovm--test-undo-insert-at 5 "!")
          (setq trace (cons (list 'after-insert (funcall 'neovm--test-undo-doc-string)) trace))
          ;; Replace 'e' (pos 1) with 'a'
          (funcall 'neovm--test-undo-replace-at 1 "a")
          (setq trace (cons (list 'after-replace (funcall 'neovm--test-undo-doc-string)) trace))
          ;; Delete 'l' (pos 2)
          (funcall 'neovm--test-undo-delete-at 2)
          (setq trace (cons (list 'after-delete (funcall 'neovm--test-undo-doc-string)) trace))
          ;; Insert "X" at pos 0
          (funcall 'neovm--test-undo-insert-at 0 "X")
          (setq trace (cons (list 'after-insert2 (funcall 'neovm--test-undo-doc-string)) trace))
          ;; Now undo everything step by step
          (funcall 'neovm--test-undo-do-undo)
          (setq trace (cons (list 'undo-1 (funcall 'neovm--test-undo-doc-string)) trace))
          (funcall 'neovm--test-undo-do-undo)
          (setq trace (cons (list 'undo-2 (funcall 'neovm--test-undo-doc-string)) trace))
          (funcall 'neovm--test-undo-do-undo)
          (setq trace (cons (list 'undo-3 (funcall 'neovm--test-undo-doc-string)) trace))
          (funcall 'neovm--test-undo-do-undo)
          (setq trace (cons (list 'undo-4 (funcall 'neovm--test-undo-doc-string)) trace))
          (nreverse trace)))
    (fmakunbound 'neovm--test-undo-init)
    (fmakunbound 'neovm--test-undo-doc-string)
    (fmakunbound 'neovm--test-undo-insert-at)
    (fmakunbound 'neovm--test-undo-delete-at)
    (fmakunbound 'neovm--test-undo-replace-at)
    (fmakunbound 'neovm--test-undo-do-undo)
    (makunbound 'neovm--test-undo-doc)
    (makunbound 'neovm--test-undo-stack)))"#;
    let expect = expect_test::expect![[
        r#""OK ((init \"Hello\") (after-insert \"Hello!\") (after-replace \"Hallo!\") (after-delete \"Halo!\") (after-insert2 \"XHalo!\") (undo-1 \"Halo!\") (undo-2 \"Hallo!\") (undo-3 \"Hello!\") (undo-4 \"Hello\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Simple task scheduler with priority queue (heap-like via sorted list)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rw_task_scheduler() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--test-sched-queue nil "Priority queue as sorted alist.")
  (defvar neovm--test-sched-time 0 "Current virtual time.")
  (defvar neovm--test-sched-log nil "Execution log.")

  (fset 'neovm--test-sched-add
    (lambda (time name action)
      (let ((entry (list time name action)))
        (setq neovm--test-sched-queue
              (sort (cons entry neovm--test-sched-queue)
                    (lambda (a b) (< (car a) (car b))))))))

  (fset 'neovm--test-sched-run
    (lambda ()
      (while neovm--test-sched-queue
        (let* ((task (car neovm--test-sched-queue))
               (task-time (nth 0 task))
               (task-name (nth 1 task))
               (task-action (nth 2 task)))
          (setq neovm--test-sched-queue (cdr neovm--test-sched-queue))
          (setq neovm--test-sched-time task-time)
          (let ((result (funcall task-action)))
            (setq neovm--test-sched-log
                  (cons (list neovm--test-sched-time task-name result)
                        neovm--test-sched-log)))))))

  (unwind-protect
      (progn
        (setq neovm--test-sched-queue nil)
        (setq neovm--test-sched-time 0)
        (setq neovm--test-sched-log nil)
        ;; Schedule tasks out of order
        (funcall 'neovm--test-sched-add 30 "compile"
                 (lambda () "compiled"))
        (funcall 'neovm--test-sched-add 10 "parse"
                 (lambda ()
                   ;; Parsing schedules a follow-up task
                   (funcall 'neovm--test-sched-add 25 "optimize"
                            (lambda () "optimized"))
                   "parsed"))
        (funcall 'neovm--test-sched-add 5 "lex"
                 (lambda () "lexed"))
        (funcall 'neovm--test-sched-add 50 "link"
                 (lambda () "linked"))
        (funcall 'neovm--test-sched-add 15 "typecheck"
                 (lambda ()
                   ;; Type checking conditionally schedules
                   (funcall 'neovm--test-sched-add 20 "infer"
                            (lambda () "inferred"))
                   "checked"))
        ;; Run all
        (funcall 'neovm--test-sched-run)
        (list (nreverse neovm--test-sched-log)
              (list 'final-time neovm--test-sched-time)
              (list 'queue-empty (null neovm--test-sched-queue))))
    (fmakunbound 'neovm--test-sched-add)
    (fmakunbound 'neovm--test-sched-run)
    (makunbound 'neovm--test-sched-queue)
    (makunbound 'neovm--test-sched-time)
    (makunbound 'neovm--test-sched-log)))"#;
    let expect = expect_test::expect![[
        r#""OK (((5 \"lex\" \"lexed\") (10 \"parse\" \"parsed\") (15 \"typecheck\" \"checked\") (20 \"infer\" \"inferred\") (25 \"optimize\" \"optimized\") (30 \"compile\" \"compiled\") (50 \"link\" \"linked\")) (final-time 50) (queue-empty t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
