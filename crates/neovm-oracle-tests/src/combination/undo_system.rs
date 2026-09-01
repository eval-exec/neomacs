//! Oracle parity tests for an undo/redo system implemented in Elisp:
//! command pattern with execute/undo, undo stack and redo stack,
//! compound commands (group multiple ops), undo history traversal,
//! selective undo, and undo limit management.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic undo/redo with command pattern: execute, undo, redo
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_undo_system_basic_execute_undo_redo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a simple undo system operating on a list-based "document".
    // Commands are (type . data) and know how to execute and undo themselves.
    let form = r#"(progn
  (defvar neovm--undo-doc nil)
  (defvar neovm--undo-stack nil)
  (defvar neovm--redo-stack nil)

  (fset 'neovm--undo-execute
    (lambda (cmd)
      "Execute CMD on the document, push onto undo stack, clear redo."
      (let ((type (car cmd)) (data (cdr cmd)))
        (cond
          ;; Insert at position: (insert pos . text)
          ((eq type 'insert)
           (let ((pos (car data)) (text (cdr data)))
             (setq neovm--undo-doc
                   (concat (substring neovm--undo-doc 0 pos)
                           text
                           (substring neovm--undo-doc pos)))
             (setq neovm--undo-stack
                   (cons (list 'delete pos (length text)) neovm--undo-stack))))
          ;; Delete at position: (delete pos . count)
          ((eq type 'delete)
           (let ((pos (car data)) (count (cdr data)))
             (let ((deleted (substring neovm--undo-doc pos (+ pos count))))
               (setq neovm--undo-doc
                     (concat (substring neovm--undo-doc 0 pos)
                             (substring neovm--undo-doc (+ pos count))))
               (setq neovm--undo-stack
                     (cons (list 'insert pos deleted) neovm--undo-stack)))))
          ;; Replace: (replace pos count . text)
          ((eq type 'replace)
           (let ((pos (car data)) (count (cadr data)) (text (cddr data)))
             (let ((old (substring neovm--undo-doc pos (+ pos count))))
               (setq neovm--undo-doc
                     (concat (substring neovm--undo-doc 0 pos)
                             text
                             (substring neovm--undo-doc (+ pos count))))
               (setq neovm--undo-stack
                     (cons (list 'replace pos (length text) old)
                           neovm--undo-stack)))))))
      (setq neovm--redo-stack nil)))

  (fset 'neovm--undo-undo
    (lambda ()
      "Undo the last command."
      (when neovm--undo-stack
        (let ((inv (car neovm--undo-stack)))
          (setq neovm--undo-stack (cdr neovm--undo-stack))
          (let ((type (car inv)) (data (cdr inv)))
            (cond
              ((eq type 'insert)
               (let ((pos (car data)) (text (cadr data)))
                 (setq neovm--undo-doc
                       (concat (substring neovm--undo-doc 0 pos)
                               text
                               (substring neovm--undo-doc pos)))
                 (setq neovm--redo-stack
                       (cons (list 'delete pos (length text))
                             neovm--redo-stack))))
              ((eq type 'delete)
               (let ((pos (car data)) (count (cadr data)))
                 (let ((deleted (substring neovm--undo-doc pos (+ pos count))))
                   (setq neovm--undo-doc
                         (concat (substring neovm--undo-doc 0 pos)
                                 (substring neovm--undo-doc (+ pos count))))
                   (setq neovm--redo-stack
                         (cons (list 'insert pos deleted)
                               neovm--redo-stack)))))
              ((eq type 'replace)
               (let ((pos (car data)) (count (cadr data)) (text (nth 2 data)))
                 (let ((old (substring neovm--undo-doc pos (+ pos count))))
                   (setq neovm--undo-doc
                         (concat (substring neovm--undo-doc 0 pos)
                                 text
                                 (substring neovm--undo-doc (+ pos count))))
                   (setq neovm--redo-stack
                         (cons (list 'replace pos (length text) old)
                               neovm--redo-stack)))))))))))

  (fset 'neovm--undo-redo
    (lambda ()
      "Redo the last undone command."
      (when neovm--redo-stack
        (let ((cmd (car neovm--redo-stack)))
          (setq neovm--redo-stack (cdr neovm--redo-stack))
          (let ((type (car cmd)) (data (cdr cmd)))
            (cond
              ((eq type 'insert)
               (let ((pos (car data)) (text (cadr data)))
                 (setq neovm--undo-doc
                       (concat (substring neovm--undo-doc 0 pos)
                               text
                               (substring neovm--undo-doc pos)))
                 (setq neovm--undo-stack
                       (cons (list 'delete pos (length text))
                             neovm--undo-stack))))
              ((eq type 'delete)
               (let ((pos (car data)) (count (cadr data)))
                 (let ((deleted (substring neovm--undo-doc pos (+ pos count))))
                   (setq neovm--undo-doc
                         (concat (substring neovm--undo-doc 0 pos)
                                 (substring neovm--undo-doc (+ pos count))))
                   (setq neovm--undo-stack
                         (cons (list 'insert pos deleted)
                               neovm--undo-stack)))))
              ((eq type 'replace)
               (let ((pos (car data)) (count (cadr data)) (text (nth 2 data)))
                 (let ((old (substring neovm--undo-doc pos (+ pos count))))
                   (setq neovm--undo-doc
                         (concat (substring neovm--undo-doc 0 pos)
                                 text
                                 (substring neovm--undo-doc (+ pos count))))
                   (setq neovm--undo-stack
                         (cons (list 'replace pos (length text) old)
                               neovm--undo-stack)))))))))))

  (unwind-protect
      (progn
        (setq neovm--undo-doc "Hello World"
              neovm--undo-stack nil
              neovm--redo-stack nil)
        ;; Execute: insert ", Beautiful" at pos 5
        (funcall 'neovm--undo-execute '(insert 5 . ", Beautiful"))
        (let ((after-insert neovm--undo-doc))
          ;; Execute: delete 6 chars at pos 17 (" World" -> remove "World" part)
          (funcall 'neovm--undo-execute '(delete 16 . 5))
          (let ((after-delete neovm--undo-doc))
            ;; Undo the delete
            (funcall 'neovm--undo-undo)
            (let ((after-undo1 neovm--undo-doc))
              ;; Undo the insert
              (funcall 'neovm--undo-undo)
              (let ((after-undo2 neovm--undo-doc))
                ;; Redo the insert
                (funcall 'neovm--undo-redo)
                (let ((after-redo1 neovm--undo-doc))
                  ;; Redo the delete
                  (funcall 'neovm--undo-redo)
                  (let ((after-redo2 neovm--undo-doc))
                    (list after-insert after-delete
                          after-undo1 after-undo2
                          after-redo1 after-redo2
                          (length neovm--undo-stack)
                          (length neovm--redo-stack)))))))))
    (fmakunbound 'neovm--undo-execute)
    (fmakunbound 'neovm--undo-undo)
    (fmakunbound 'neovm--undo-redo)
    (makunbound 'neovm--undo-doc)
    (makunbound 'neovm--undo-stack)
    (makunbound 'neovm--redo-stack)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hello, Beautiful World\" \"Hello, Beautifuld\" \"Hello, Beautiful World\" \"Hello World\" \"Hello, Beautiful World\" \"Hello, Beautifuld\" 2 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Compound commands: group multiple operations into one undo step
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_undo_system_compound_commands() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A compound command groups multiple primitive operations so that
    // a single undo reverses all of them at once.
    let form = r#"(progn
  (defvar neovm--undo2-state nil)
  (defvar neovm--undo2-stack nil)
  (defvar neovm--undo2-redo nil)

  ;; State is an alist (key . value)
  (fset 'neovm--undo2-get
    (lambda (key)
      (cdr (assq key neovm--undo2-state))))

  (fset 'neovm--undo2-set
    (lambda (key val)
      "Set KEY to VAL, return inverse operation."
      (let ((old (cdr (assq key neovm--undo2-state))))
        (let ((entry (assq key neovm--undo2-state)))
          (if entry
              (setcdr entry val)
            (setq neovm--undo2-state
                  (cons (cons key val) neovm--undo2-state))))
        (list 'set key old))))

  (fset 'neovm--undo2-apply-inverse
    (lambda (inv)
      "Apply an inverse operation, return the re-do operation."
      (let ((type (car inv)))
        (cond
          ((eq type 'set)
           (let ((key (nth 1 inv)) (old-val (nth 2 inv)))
             (funcall 'neovm--undo2-set key old-val)))))))

  (fset 'neovm--undo2-exec-compound
    (lambda (ops)
      "Execute a list of operations as a compound command."
      (let ((inverses nil))
        (dolist (op ops)
          (let ((type (car op)))
            (cond
              ((eq type 'set)
               (let ((inv (funcall 'neovm--undo2-set (nth 1 op) (nth 2 op))))
                 (setq inverses (cons inv inverses)))))))
        ;; Push the list of inverses as a single undo entry
        (setq neovm--undo2-stack (cons inverses neovm--undo2-stack))
        (setq neovm--undo2-redo nil))))

  (fset 'neovm--undo2-undo
    (lambda ()
      (when neovm--undo2-stack
        (let ((inverses (car neovm--undo2-stack))
              (redos nil))
          (setq neovm--undo2-stack (cdr neovm--undo2-stack))
          ;; Apply inverses in order (they were pushed in reverse)
          (dolist (inv inverses)
            (let ((redo (funcall 'neovm--undo2-apply-inverse inv)))
              (setq redos (cons redo redos))))
          (setq neovm--undo2-redo (cons redos neovm--undo2-redo))))))

  (fset 'neovm--undo2-redo
    (lambda ()
      (when neovm--undo2-redo
        (let ((redos (car neovm--undo2-redo))
              (inverses nil))
          (setq neovm--undo2-redo (cdr neovm--undo2-redo))
          (dolist (redo redos)
            (let ((inv (funcall 'neovm--undo2-apply-inverse redo)))
              (setq inverses (cons inv inverses))))
          (setq neovm--undo2-stack (cons inverses neovm--undo2-stack))))))

  (fset 'neovm--undo2-snapshot
    (lambda ()
      (sort (copy-sequence neovm--undo2-state)
            (lambda (a b) (string< (symbol-name (car a)) (symbol-name (car b)))))))

  (unwind-protect
      (progn
        (setq neovm--undo2-state nil
              neovm--undo2-stack nil
              neovm--undo2-redo nil)
        ;; Compound op 1: set x=10, y=20
        (funcall 'neovm--undo2-exec-compound
                 '((set x 10) (set y 20)))
        (let ((s1 (funcall 'neovm--undo2-snapshot)))
          ;; Compound op 2: set x=30, z=40
          (funcall 'neovm--undo2-exec-compound
                   '((set x 30) (set z 40)))
          (let ((s2 (funcall 'neovm--undo2-snapshot)))
            ;; Compound op 3: set y=50, z=60, w=70
            (funcall 'neovm--undo2-exec-compound
                     '((set y 50) (set z 60) (set w 70)))
            (let ((s3 (funcall 'neovm--undo2-snapshot)))
              ;; Undo compound 3 (y->20, z->40, w removed)
              (funcall 'neovm--undo2-undo)
              (let ((s4 (funcall 'neovm--undo2-snapshot)))
                ;; Undo compound 2 (x->10, z removed)
                (funcall 'neovm--undo2-undo)
                (let ((s5 (funcall 'neovm--undo2-snapshot)))
                  ;; Redo compound 2
                  (funcall 'neovm--undo2-redo)
                  (let ((s6 (funcall 'neovm--undo2-snapshot)))
                    (list s1 s2 s3 s4 s5 s6
                          (length neovm--undo2-stack)
                          (length neovm--undo2-redo)))))))))
    (fmakunbound 'neovm--undo2-get)
    (fmakunbound 'neovm--undo2-set)
    (fmakunbound 'neovm--undo2-apply-inverse)
    (fmakunbound 'neovm--undo2-exec-compound)
    (fmakunbound 'neovm--undo2-undo)
    (fmakunbound 'neovm--undo2-redo)
    (fmakunbound 'neovm--undo2-snapshot)
    (makunbound 'neovm--undo2-state)
    (makunbound 'neovm--undo2-stack)
    (makunbound 'neovm--undo2-redo)))"#;
    let expect = expect_test::expect![[
        r#""OK (((x . 30) (y . 20)) ((x . 30) (y . 20) (z . 40)) ((w) (x . 30) (y . 20) (z . 40)) ((w) (x . 30) (y . 20) (z . 40)) ((w) (x . 30) (y . 20) (z . 40)) ((w) (x . 30) (y . 20) (z . 40)) 2 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Undo history traversal: navigate through full history
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_undo_system_history_traversal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Maintain a linear history with a cursor. Navigate back and forth
    // through the history, showing the state at each point.
    let form = r#"(progn
  (defvar neovm--hist-states nil)
  (defvar neovm--hist-cursor 0)

  (fset 'neovm--hist-init
    (lambda (initial)
      (setq neovm--hist-states (list initial)
            neovm--hist-cursor 0)))

  (fset 'neovm--hist-current
    (lambda () (nth neovm--hist-cursor neovm--hist-states)))

  (fset 'neovm--hist-push
    (lambda (state)
      "Push new state, discarding any redo history after cursor."
      ;; Keep states up to cursor+1, then add new
      (let ((kept nil) (i 0))
        (dolist (s neovm--hist-states)
          (when (<= i neovm--hist-cursor)
            (setq kept (cons s kept)))
          (setq i (1+ i)))
        (setq neovm--hist-states (nreverse (cons state kept)))
        (setq neovm--hist-cursor (1- (length neovm--hist-states))))))

  (fset 'neovm--hist-undo
    (lambda ()
      (when (> neovm--hist-cursor 0)
        (setq neovm--hist-cursor (1- neovm--hist-cursor)))
      (funcall 'neovm--hist-current)))

  (fset 'neovm--hist-redo
    (lambda ()
      (when (< neovm--hist-cursor (1- (length neovm--hist-states)))
        (setq neovm--hist-cursor (1+ neovm--hist-cursor)))
      (funcall 'neovm--hist-current)))

  (fset 'neovm--hist-can-undo (lambda () (> neovm--hist-cursor 0)))
  (fset 'neovm--hist-can-redo
    (lambda () (< neovm--hist-cursor (1- (length neovm--hist-states)))))

  (unwind-protect
      (progn
        (funcall 'neovm--hist-init '(initial))
        (funcall 'neovm--hist-push '(step-1))
        (funcall 'neovm--hist-push '(step-2))
        (funcall 'neovm--hist-push '(step-3))
        (funcall 'neovm--hist-push '(step-4))
        (let ((at-end (funcall 'neovm--hist-current))
              (can-undo-1 (funcall 'neovm--hist-can-undo))
              (can-redo-1 (funcall 'neovm--hist-can-redo)))
          ;; Undo 3 times
          (funcall 'neovm--hist-undo)
          (funcall 'neovm--hist-undo)
          (let ((after-2-undos (funcall 'neovm--hist-current)))
            (funcall 'neovm--hist-undo)
            (let ((after-3-undos (funcall 'neovm--hist-current))
                  (can-undo-2 (funcall 'neovm--hist-can-undo))
                  (can-redo-2 (funcall 'neovm--hist-can-redo)))
              ;; Redo once
              (funcall 'neovm--hist-redo)
              (let ((after-redo (funcall 'neovm--hist-current)))
                ;; Push new state from middle: discards redo history
                (funcall 'neovm--hist-push '(branch-step))
                (let ((after-branch (funcall 'neovm--hist-current))
                      (can-redo-3 (funcall 'neovm--hist-can-redo))
                      (history-len (length neovm--hist-states)))
                  (list at-end can-undo-1 can-redo-1
                        after-2-undos after-3-undos
                        can-undo-2 can-redo-2
                        after-redo
                        after-branch can-redo-3
                        history-len
                        neovm--hist-cursor)))))))
    (fmakunbound 'neovm--hist-init)
    (fmakunbound 'neovm--hist-current)
    (fmakunbound 'neovm--hist-push)
    (fmakunbound 'neovm--hist-undo)
    (fmakunbound 'neovm--hist-redo)
    (fmakunbound 'neovm--hist-can-undo)
    (fmakunbound 'neovm--hist-can-redo)
    (makunbound 'neovm--hist-states)
    (makunbound 'neovm--hist-cursor)))"#;
    let expect = expect_test::expect![[
        r#""OK ((step-4) t nil (step-2) (step-1) t t (step-2) (branch-step) nil 4 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Undo limit management: bounded history with eviction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_undo_system_limit_management() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Undo history with a maximum size. When the limit is reached,
    // the oldest entries are evicted. Track eviction events.
    let form = r#"(progn
  (defvar neovm--ulim-value 0)
  (defvar neovm--ulim-stack nil)
  (defvar neovm--ulim-redo nil)
  (defvar neovm--ulim-max 5)
  (defvar neovm--ulim-evictions 0)

  (fset 'neovm--ulim-execute
    (lambda (new-val)
      "Set value to NEW-VAL, push old on undo stack with limit."
      (let ((old neovm--ulim-value))
        (setq neovm--ulim-value new-val)
        (setq neovm--ulim-stack (cons old neovm--ulim-stack))
        (setq neovm--ulim-redo nil)
        ;; Enforce limit: trim oldest if over max
        (when (> (length neovm--ulim-stack) neovm--ulim-max)
          (let ((trimmed (nbutlast (copy-sequence neovm--ulim-stack))))
            (setq neovm--ulim-stack trimmed)
            (setq neovm--ulim-evictions (1+ neovm--ulim-evictions)))))))

  (fset 'neovm--ulim-undo
    (lambda ()
      (when neovm--ulim-stack
        (let ((old (car neovm--ulim-stack)))
          (setq neovm--ulim-stack (cdr neovm--ulim-stack))
          (setq neovm--ulim-redo (cons neovm--ulim-value neovm--ulim-redo))
          (setq neovm--ulim-value old)))))

  (fset 'neovm--ulim-redo
    (lambda ()
      (when neovm--ulim-redo
        (let ((val (car neovm--ulim-redo)))
          (setq neovm--ulim-redo (cdr neovm--ulim-redo))
          (setq neovm--ulim-stack (cons neovm--ulim-value neovm--ulim-stack))
          (setq neovm--ulim-value val)))))

  (fset 'neovm--ulim-snapshot
    (lambda ()
      (list neovm--ulim-value
            (length neovm--ulim-stack)
            (length neovm--ulim-redo)
            neovm--ulim-evictions)))

  (unwind-protect
      (progn
        (setq neovm--ulim-value 0
              neovm--ulim-stack nil
              neovm--ulim-redo nil
              neovm--ulim-max 5
              neovm--ulim-evictions 0)
        ;; Execute 8 operations (limit is 5, so 3 evictions)
        (let ((snapshots nil))
          (dolist (v '(10 20 30 40 50 60 70 80))
            (funcall 'neovm--ulim-execute v)
            (setq snapshots (cons (funcall 'neovm--ulim-snapshot) snapshots)))
          (let ((after-all (funcall 'neovm--ulim-snapshot)))
            ;; Undo 3 times
            (funcall 'neovm--ulim-undo)
            (funcall 'neovm--ulim-undo)
            (funcall 'neovm--ulim-undo)
            (let ((after-undo3 (funcall 'neovm--ulim-snapshot)))
              ;; Try to undo beyond limit (should stop at oldest available)
              (funcall 'neovm--ulim-undo)
              (funcall 'neovm--ulim-undo)
              (funcall 'neovm--ulim-undo)  ;; should be no-op (stack empty)
              (let ((after-max-undo (funcall 'neovm--ulim-snapshot)))
                ;; Redo all
                (funcall 'neovm--ulim-redo)
                (funcall 'neovm--ulim-redo)
                (funcall 'neovm--ulim-redo)
                (funcall 'neovm--ulim-redo)
                (funcall 'neovm--ulim-redo)
                (let ((after-redo (funcall 'neovm--ulim-snapshot)))
                  (list (nreverse snapshots)
                        after-all after-undo3
                        after-max-undo after-redo)))))))
    (fmakunbound 'neovm--ulim-execute)
    (fmakunbound 'neovm--ulim-undo)
    (fmakunbound 'neovm--ulim-redo)
    (fmakunbound 'neovm--ulim-snapshot)
    (makunbound 'neovm--ulim-value)
    (makunbound 'neovm--ulim-stack)
    (makunbound 'neovm--ulim-redo)
    (makunbound 'neovm--ulim-max)
    (makunbound 'neovm--ulim-evictions)))"#;
    let expect = expect_test::expect![[
        r#""OK (((10 1 0 0) (20 2 0 0) (30 3 0 0) (40 4 0 0) (50 5 0 0) (60 5 0 1) (70 5 0 2) (80 5 0 3)) (80 5 0 3) (50 2 3 3) (30 0 5 3) (80 5 0 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Selective undo: undo a specific operation by index
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_undo_system_selective_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement selective undo where you can undo a specific past operation
    // rather than just the most recent one. Uses an operation log.
    let form = r#"(progn
  (defvar neovm--sel-items nil)
  (defvar neovm--sel-log nil)
  (defvar neovm--sel-counter 0)

  (fset 'neovm--sel-add
    (lambda (item)
      "Add ITEM to the set. Log the operation."
      (unless (member item neovm--sel-items)
        (setq neovm--sel-items (cons item neovm--sel-items))
        (setq neovm--sel-counter (1+ neovm--sel-counter))
        (setq neovm--sel-log
              (cons (list neovm--sel-counter 'add item) neovm--sel-log)))))

  (fset 'neovm--sel-remove
    (lambda (item)
      "Remove ITEM from the set. Log the operation."
      (when (member item neovm--sel-items)
        (setq neovm--sel-items (delete item neovm--sel-items))
        (setq neovm--sel-counter (1+ neovm--sel-counter))
        (setq neovm--sel-log
              (cons (list neovm--sel-counter 'remove item) neovm--sel-log)))))

  (fset 'neovm--sel-undo-by-id
    (lambda (id)
      "Undo the operation with the given ID."
      (let ((entry nil))
        ;; Find the log entry
        (dolist (e neovm--sel-log)
          (when (= (car e) id)
            (setq entry e)))
        (when entry
          (let ((op (nth 1 entry)) (item (nth 2 entry)))
            (cond
              ;; Undo an add: remove the item
              ((eq op 'add)
               (setq neovm--sel-items (delete item neovm--sel-items))
               (setq neovm--sel-counter (1+ neovm--sel-counter))
               (setq neovm--sel-log
                     (cons (list neovm--sel-counter 'selective-undo-add item)
                           neovm--sel-log)))
              ;; Undo a remove: add the item back
              ((eq op 'remove)
               (unless (member item neovm--sel-items)
                 (setq neovm--sel-items (cons item neovm--sel-items)))
               (setq neovm--sel-counter (1+ neovm--sel-counter))
               (setq neovm--sel-log
                     (cons (list neovm--sel-counter 'selective-undo-remove item)
                           neovm--sel-log)))))))))

  (fset 'neovm--sel-snapshot
    (lambda ()
      (list (sort (copy-sequence neovm--sel-items)
                  (lambda (a b) (string< (symbol-name a) (symbol-name b))))
            neovm--sel-counter)))

  (unwind-protect
      (progn
        (setq neovm--sel-items nil
              neovm--sel-log nil
              neovm--sel-counter 0)
        ;; Add a, b, c, d, e  (ops 1-5)
        (funcall 'neovm--sel-add 'a)
        (funcall 'neovm--sel-add 'b)
        (funcall 'neovm--sel-add 'c)
        (funcall 'neovm--sel-add 'd)
        (funcall 'neovm--sel-add 'e)
        (let ((s1 (funcall 'neovm--sel-snapshot)))
          ;; Remove b (op 6)
          (funcall 'neovm--sel-remove 'b)
          (let ((s2 (funcall 'neovm--sel-snapshot)))
            ;; Selectively undo op 3 (which added 'c): removes c
            (funcall 'neovm--sel-undo-by-id 3)
            (let ((s3 (funcall 'neovm--sel-snapshot)))
              ;; Selectively undo op 6 (which removed 'b): adds b back
              (funcall 'neovm--sel-undo-by-id 6)
              (let ((s4 (funcall 'neovm--sel-snapshot)))
                ;; Try undoing an already-undone add (op 1 added 'a)
                (funcall 'neovm--sel-undo-by-id 1)
                (let ((s5 (funcall 'neovm--sel-snapshot)))
                  (list s1 s2 s3 s4 s5
                        (length neovm--sel-log))))))))
    (fmakunbound 'neovm--sel-add)
    (fmakunbound 'neovm--sel-remove)
    (fmakunbound 'neovm--sel-undo-by-id)
    (fmakunbound 'neovm--sel-snapshot)
    (makunbound 'neovm--sel-items)
    (makunbound 'neovm--sel-log)
    (makunbound 'neovm--sel-counter)))"#;
    let expect = expect_test::expect![[
        r#""OK (((a b c d e) 5) ((a c d e) 6) ((a d e) 7) ((a b d e) 8) ((b d e) 9) 9)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Undo on a calculator: arithmetic operations with undo/redo
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_undo_system_calculator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // RPN calculator with undo: push, pop, add, mul, and undo/redo.
    let form = r#"(progn
  (defvar neovm--calc-stack nil)
  (defvar neovm--calc-undo nil)
  (defvar neovm--calc-redo nil)

  (fset 'neovm--calc-save
    (lambda ()
      "Save current stack state for undo."
      (setq neovm--calc-undo
            (cons (copy-sequence neovm--calc-stack)
                  neovm--calc-undo))
      (setq neovm--calc-redo nil)))

  (fset 'neovm--calc-push
    (lambda (n)
      (funcall 'neovm--calc-save)
      (setq neovm--calc-stack (cons n neovm--calc-stack))))

  (fset 'neovm--calc-op
    (lambda (op)
      "Apply binary OP to top two stack elements."
      (when (>= (length neovm--calc-stack) 2)
        (funcall 'neovm--calc-save)
        (let ((b (car neovm--calc-stack))
              (a (cadr neovm--calc-stack)))
          (setq neovm--calc-stack (cddr neovm--calc-stack))
          (let ((result (cond
                          ((eq op '+) (+ a b))
                          ((eq op '-) (- a b))
                          ((eq op '*) (* a b))
                          ((eq op '/) (if (zerop b) 'error (/ a b)))
                          (t 'unknown))))
            (setq neovm--calc-stack (cons result neovm--calc-stack)))))))

  (fset 'neovm--calc-undo
    (lambda ()
      (when neovm--calc-undo
        (setq neovm--calc-redo
              (cons (copy-sequence neovm--calc-stack)
                    neovm--calc-redo))
        (setq neovm--calc-stack (car neovm--calc-undo))
        (setq neovm--calc-undo (cdr neovm--calc-undo)))))

  (fset 'neovm--calc-redo
    (lambda ()
      (when neovm--calc-redo
        (setq neovm--calc-undo
              (cons (copy-sequence neovm--calc-stack)
                    neovm--calc-undo))
        (setq neovm--calc-stack (car neovm--calc-redo))
        (setq neovm--calc-redo (cdr neovm--calc-redo)))))

  (unwind-protect
      (progn
        (setq neovm--calc-stack nil
              neovm--calc-undo nil
              neovm--calc-redo nil)
        ;; Compute (3 + 4) * 5
        (funcall 'neovm--calc-push 3)
        (funcall 'neovm--calc-push 4)
        (funcall 'neovm--calc-op '+)
        (let ((after-add (copy-sequence neovm--calc-stack)))
          (funcall 'neovm--calc-push 5)
          (funcall 'neovm--calc-op '*)
          (let ((after-mul (copy-sequence neovm--calc-stack)))
            ;; Undo the multiply
            (funcall 'neovm--calc-undo)
            (let ((undo1 (copy-sequence neovm--calc-stack)))
              ;; Undo the push 5
              (funcall 'neovm--calc-undo)
              (let ((undo2 (copy-sequence neovm--calc-stack)))
                ;; Redo push 5
                (funcall 'neovm--calc-redo)
                (let ((redo1 (copy-sequence neovm--calc-stack)))
                  ;; Redo multiply
                  (funcall 'neovm--calc-redo)
                  (let ((redo2 (copy-sequence neovm--calc-stack)))
                    ;; Now push 2 and subtract: 35 - 2 = 33
                    (funcall 'neovm--calc-push 2)
                    (funcall 'neovm--calc-op '-)
                    (let ((final (copy-sequence neovm--calc-stack)))
                      ;; Undo all the way back
                      (funcall 'neovm--calc-undo)
                      (funcall 'neovm--calc-undo)
                      (funcall 'neovm--calc-undo)
                      (funcall 'neovm--calc-undo)
                      (funcall 'neovm--calc-undo)
                      (funcall 'neovm--calc-undo)
                      (funcall 'neovm--calc-undo)
                      (let ((all-undone (copy-sequence neovm--calc-stack)))
                        (list after-add after-mul
                              undo1 undo2
                              redo1 redo2
                              final all-undone
                              (length neovm--calc-undo)
                              (length neovm--calc-redo)))))))))))
    (fmakunbound 'neovm--calc-save)
    (fmakunbound 'neovm--calc-push)
    (fmakunbound 'neovm--calc-op)
    (fmakunbound 'neovm--calc-undo)
    (fmakunbound 'neovm--calc-redo)
    (makunbound 'neovm--calc-stack)
    (makunbound 'neovm--calc-undo)
    (makunbound 'neovm--calc-redo)))"#;
    let expect = expect_test::expect![[r#""OK ((7) (35) (5 7) (7) (5 7) (35) (33) nil 0 7)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Undo tree: branching undo with multiple alternate futures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_undo_system_branching_tree() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Instead of a linear undo, maintain a tree where branches represent
    // alternate "futures" after undo+new-edit. Track total nodes and depth.
    let form = r#"(progn
  ;; Tree node: (value parent children)
  (defvar neovm--utree-root nil)
  (defvar neovm--utree-current nil)

  (fset 'neovm--utree-make-node
    (lambda (value parent)
      (list value parent nil)))

  (fset 'neovm--utree-value (lambda (node) (car node)))
  (fset 'neovm--utree-parent (lambda (node) (cadr node)))
  (fset 'neovm--utree-children (lambda (node) (nth 2 node)))

  (fset 'neovm--utree-init
    (lambda (initial)
      (let ((root (funcall 'neovm--utree-make-node initial nil)))
        (setq neovm--utree-root root
              neovm--utree-current root))))

  (fset 'neovm--utree-edit
    (lambda (new-value)
      "Add a new node as child of current, move to it."
      (let ((node (funcall 'neovm--utree-make-node new-value neovm--utree-current)))
        ;; Add to parent's children
        (setcar (cddr neovm--utree-current)
                (cons node (funcall 'neovm--utree-children neovm--utree-current)))
        (setq neovm--utree-current node))))

  (fset 'neovm--utree-undo
    (lambda ()
      "Move to parent (undo)."
      (let ((parent (funcall 'neovm--utree-parent neovm--utree-current)))
        (when parent
          (setq neovm--utree-current parent)))))

  (fset 'neovm--utree-redo
    (lambda (branch-idx)
      "Move to child at BRANCH-IDX (redo along a branch)."
      (let ((children (funcall 'neovm--utree-children neovm--utree-current)))
        (when (and children (< branch-idx (length children)))
          (setq neovm--utree-current (nth branch-idx children))))))

  (fset 'neovm--utree-depth
    (lambda (node)
      "Depth of NODE from root."
      (let ((d 0) (n node))
        (while (funcall 'neovm--utree-parent n)
          (setq n (funcall 'neovm--utree-parent n) d (1+ d)))
        d)))

  (fset 'neovm--utree-count-nodes
    (lambda (node)
      "Count all nodes in subtree rooted at NODE."
      (let ((count 1))
        (dolist (child (funcall 'neovm--utree-children node))
          (setq count (+ count (funcall 'neovm--utree-count-nodes child))))
        count)))

  (unwind-protect
      (progn
        ;; Build a tree:
        ;; root(0) -> A(1) -> B(2) -> C(3)
        ;;                \-> D(4)
        ;;         \-> E(5)
        (funcall 'neovm--utree-init 0)
        (funcall 'neovm--utree-edit 1)      ;; 0 -> 1(A)
        (funcall 'neovm--utree-edit 2)      ;; 1 -> 2(B)
        (funcall 'neovm--utree-edit 3)      ;; 2 -> 3(C)
        (let ((at-c (funcall 'neovm--utree-value neovm--utree-current))
              (depth-c (funcall 'neovm--utree-depth neovm--utree-current)))
          ;; Undo to B, then branch to D
          (funcall 'neovm--utree-undo)      ;; back to 2(B)
          (funcall 'neovm--utree-undo)      ;; back to 1(A)
          (funcall 'neovm--utree-edit 4)    ;; 1 -> 4(D) (new branch)
          (let ((at-d (funcall 'neovm--utree-value neovm--utree-current))
                (depth-d (funcall 'neovm--utree-depth neovm--utree-current)))
            ;; Undo back to A, then to root, then branch to E
            (funcall 'neovm--utree-undo)    ;; back to 1(A)
            (funcall 'neovm--utree-undo)    ;; back to 0(root)
            (funcall 'neovm--utree-edit 5)  ;; 0 -> 5(E) (new branch from root)
            (let ((at-e (funcall 'neovm--utree-value neovm--utree-current))
                  (total-nodes (funcall 'neovm--utree-count-nodes neovm--utree-root))
                  ;; Root has 2 children: A(1) and E(5)
                  (root-children-count
                    (length (funcall 'neovm--utree-children neovm--utree-root))))
              ;; Navigate: undo to root, redo to A (branch 1, since children are
              ;; in reverse order: E was added last -> index 0, A -> index 1)
              (funcall 'neovm--utree-undo)
              (funcall 'neovm--utree-redo 1)  ;; go to A (older child)
              (let ((nav-a (funcall 'neovm--utree-value neovm--utree-current))
                    ;; A has 2 children: B and D
                    (a-children-count
                      (length (funcall 'neovm--utree-children neovm--utree-current))))
                (list at-c depth-c
                      at-d depth-d
                      at-e total-nodes
                      root-children-count
                      nav-a a-children-count))))))
    (fmakunbound 'neovm--utree-make-node)
    (fmakunbound 'neovm--utree-value)
    (fmakunbound 'neovm--utree-parent)
    (fmakunbound 'neovm--utree-children)
    (fmakunbound 'neovm--utree-init)
    (fmakunbound 'neovm--utree-edit)
    (fmakunbound 'neovm--utree-undo)
    (fmakunbound 'neovm--utree-redo)
    (fmakunbound 'neovm--utree-depth)
    (fmakunbound 'neovm--utree-count-nodes)
    (makunbound 'neovm--utree-root)
    (makunbound 'neovm--utree-current)))"#;
    let expect = expect_test::expect![[r#""OK (3 3 4 2 5 6 2 1 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
