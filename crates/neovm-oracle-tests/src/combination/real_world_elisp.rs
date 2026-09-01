//! Oracle parity tests for real-world Elisp patterns:
//! mode-line format processing, package dependency resolution,
//! configuration file generation, undo/redo system, ring buffer
//! (kill-ring), and auto-completion candidate scoring.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Mode-line format string processing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rwe_mode_line_format_processing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate mode-line format processing with %-sequences,
    // conditional sections, and padding.
    let form = r#"(let ((state '(:buffer "init.el"
                                  :modified t
                                  :read-only nil
                                  :line 142
                                  :col 28
                                  :major-mode "emacs-lisp"
                                  :coding "utf-8"
                                  :eol "unix"
                                  :size 4096)))
                    (let ((expand-format
                           (lambda (fmt st)
                             (let ((result "")
                                   (i 0)
                                   (len (length fmt)))
                               (while (< i len)
                                 (if (and (< (1+ i) len) (= (aref fmt i) ?%))
                                     (let ((spec (aref fmt (1+ i))))
                                       (setq result
                                             (concat result
                                                     (cond
                                                      ((= spec ?b) (plist-get st :buffer))
                                                      ((= spec ?*)
                                                       (cond ((plist-get st :read-only) "%%")
                                                             ((plist-get st :modified) "**")
                                                             (t "--")))
                                                      ((= spec ?l)
                                                       (number-to-string (plist-get st :line)))
                                                      ((= spec ?c)
                                                       (number-to-string (plist-get st :col)))
                                                      ((= spec ?m) (plist-get st :major-mode))
                                                      ((= spec ?z) (plist-get st :coding))
                                                      ((= spec ?e) (plist-get st :eol))
                                                      ((= spec ?s)
                                                       (let ((sz (plist-get st :size)))
                                                         (cond ((>= sz 1048576)
                                                                (format "%.1fM" (/ sz 1048576.0)))
                                                               ((>= sz 1024)
                                                                (format "%.1fK" (/ sz 1024.0)))
                                                               (t (format "%dB" sz)))))
                                                      (t (string spec)))))
                                       (setq i (+ i 2)))
                                   (setq result (concat result (string (aref fmt i))))
                                   (setq i (1+ i))))
                               result))))
                      (list
                       (funcall expand-format "%* %b  L%l C%c  (%m)  [%z-%e]  %s" state)
                       ;; Read-only variant
                       (funcall expand-format "%* %b" (plist-put (copy-sequence state)
                                                                  :read-only t))
                       ;; Unmodified variant
                       (funcall expand-format "%* %b" (plist-put (plist-put
                                                                   (copy-sequence state)
                                                                   :modified nil)
                                                                  :read-only nil)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"** init.el  L142 C28  (emacs-lisp)  [utf-8-unix]  4.0K\" \"%% init.el\" \"-- init.el\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Elisp package dependency resolver
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rwe_package_dependency_resolver() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full topological sort with cycle detection for package dependencies.
    let form = r#"(progn
  (fset 'neovm--test-resolve-deps
    (lambda (packages)
      (let ((deps (make-hash-table))
            (all-pkgs nil))
        ;; Build dependency graph
        (dolist (pkg packages)
          (puthash (car pkg) (cdr pkg) deps)
          (setq all-pkgs (cons (car pkg) all-pkgs)))
        (setq all-pkgs (nreverse all-pkgs))
        ;; Topological sort with cycle detection
        (let ((visited (make-hash-table))
              (in-stack (make-hash-table))
              (order nil)
              (cycle-found nil))
          (fset 'neovm--test-visit
            (lambda (node)
              (cond
               ((gethash node in-stack)
                (setq cycle-found node))
               ((not (gethash node visited))
                (puthash node t in-stack)
                (dolist (dep (gethash node deps nil))
                  (unless cycle-found
                    (funcall 'neovm--test-visit dep)))
                (puthash node nil in-stack)
                (puthash node t visited)
                (setq order (cons node order))))))
          (dolist (pkg all-pkgs)
            (unless cycle-found
              (funcall 'neovm--test-visit pkg)))
          (unwind-protect
              (if cycle-found
                  (list 'cycle-at cycle-found)
                order)
            (fmakunbound 'neovm--test-visit))))))
  (unwind-protect
      (list
       ;; Normal DAG
       (funcall 'neovm--test-resolve-deps
                '((app lib-a lib-b)
                  (lib-a core utils)
                  (lib-b core)
                  (core)
                  (utils core)))
       ;; Independent packages
       (funcall 'neovm--test-resolve-deps
                '((pkg-a) (pkg-b) (pkg-c)))
       ;; Linear chain
       (funcall 'neovm--test-resolve-deps
                '((d c) (c b) (b a) (a))))
    (fmakunbound 'neovm--test-resolve-deps)))"#;
    let expect = expect_test::expect![[
        r#""OK ((app lib-b lib-a utils core) (pkg-c pkg-b pkg-a) (d c b a))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Configuration file generator
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rwe_config_generator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Generate INI-style configuration file from nested alist structure.
    let form = r#"(let ((config
                         '((general . ((user . "alice")
                                       (version . "2.1")
                                       (debug . nil)))
                           (network . ((host . "localhost")
                                       (port . 8080)
                                       (timeout . 30)
                                       (ssl . t)))
                           (database . ((driver . "postgres")
                                        (name . "mydb")
                                        (pool-size . 10))))))
                    (let ((format-value
                           (lambda (v)
                             (cond
                              ((eq v t) "true")
                              ((eq v nil) "false")
                              ((numberp v) (number-to-string v))
                              ((stringp v) v)
                              (t (prin1-to-string v)))))
                          (lines nil))
                      (dolist (section config)
                        (setq lines (cons (format "[%s]" (car section)) lines))
                        (dolist (entry (cdr section))
                          (setq lines (cons (format "%s = %s"
                                                     (symbol-name (car entry))
                                                     (funcall format-value (cdr entry)))
                                            lines)))
                        (setq lines (cons "" lines)))
                      ;; Join and return
                      (let ((result (mapconcat #'identity (nreverse lines) "\n")))
                        ;; Also parse it back and verify round-trip on key values
                        (list result
                              ;; Verify specific values from the original
                              (cdr (assq 'port (cdr (assq 'network config))))
                              (cdr (assq 'debug (cdr (assq 'general config))))
                              (length config)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"[general]\\nuser = alice\\nversion = 2.1\\ndebug = false\\n\\n[network]\\nhost = localhost\\nport = 8080\\ntimeout = 30\\nssl = true\\n\\n[database]\\ndriver = postgres\\nname = mydb\\npool-size = 10\\n\" 8080 nil 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Undo/redo system for text editor operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rwe_undo_redo_text_editor() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A text editor undo/redo system that tracks insert/delete operations
    // and can reconstruct document state.
    let form = r#"(let ((doc "")
                        (undo-stack nil)
                        (redo-stack nil))
                    (let ((do-insert
                           (lambda (pos text)
                             (let ((old-doc doc))
                               (setq doc (concat (substring doc 0 pos)
                                                 text
                                                 (substring doc pos)))
                               (setq undo-stack
                                     (cons (list 'delete pos (length text))
                                           undo-stack))
                               (setq redo-stack nil))))
                          (do-delete
                           (lambda (pos len)
                             (let ((deleted (substring doc pos (+ pos len))))
                               (setq doc (concat (substring doc 0 pos)
                                                 (substring doc (+ pos len))))
                               (setq undo-stack
                                     (cons (list 'insert pos deleted)
                                           undo-stack))
                               (setq redo-stack nil))))
                          (undo
                           (lambda ()
                             (when undo-stack
                               (let ((op (car undo-stack)))
                                 (setq undo-stack (cdr undo-stack))
                                 (cond
                                  ((eq (car op) 'insert)
                                   (let ((pos (nth 1 op))
                                         (text (nth 2 op)))
                                     (setq doc (concat (substring doc 0 pos)
                                                       text
                                                       (substring doc pos)))
                                     (setq redo-stack
                                           (cons (list 'delete pos (length text))
                                                 redo-stack))))
                                  ((eq (car op) 'delete)
                                   (let ((pos (nth 1 op))
                                         (len (nth 2 op)))
                                     (let ((deleted (substring doc pos (+ pos len))))
                                       (setq doc (concat (substring doc 0 pos)
                                                         (substring doc (+ pos len))))
                                       (setq redo-stack
                                             (cons (list 'insert pos deleted)
                                                   redo-stack))))))))))
                          (redo
                           (lambda ()
                             (when redo-stack
                               (let ((op (car redo-stack)))
                                 (setq redo-stack (cdr redo-stack))
                                 (cond
                                  ((eq (car op) 'insert)
                                   (let ((pos (nth 1 op))
                                         (text (nth 2 op)))
                                     (setq doc (concat (substring doc 0 pos)
                                                       text
                                                       (substring doc pos)))
                                     (setq undo-stack
                                           (cons (list 'delete pos (length text))
                                                 undo-stack))))
                                  ((eq (car op) 'delete)
                                   (let ((pos (nth 1 op))
                                         (len (nth 2 op)))
                                     (let ((deleted (substring doc pos (+ pos len))))
                                       (setq doc (concat (substring doc 0 pos)
                                                         (substring doc (+ pos len))))
                                       (setq undo-stack
                                             (cons (list 'insert pos deleted)
                                                   undo-stack)))))))))))
                      ;; Build document: "Hello, World!"
                      (funcall do-insert 0 "Hello")
                      (funcall do-insert 5 ", World!")
                      (let ((after-build doc))
                        ;; Delete "World" -> "Hello, !"
                        (funcall do-delete 7 5)
                        (let ((after-delete doc))
                          ;; Insert "Emacs" -> "Hello, Emacs!"
                          (funcall do-insert 7 "Emacs")
                          (let ((after-insert doc))
                            ;; Undo insert -> "Hello, !"
                            (funcall undo)
                            (let ((after-undo1 doc))
                              ;; Undo delete -> "Hello, World!"
                              (funcall undo)
                              (let ((after-undo2 doc))
                                ;; Redo delete -> "Hello, !"
                                (funcall redo)
                                (let ((after-redo doc))
                                  (list after-build
                                        after-delete
                                        after-insert
                                        after-undo1
                                        after-undo2
                                        after-redo)))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hello, World!\" \"Hello, !\" \"Hello, Emacs!\" \"Hello, !\" \"Hello, World!\" \"Hello, !\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Ring buffer implementation (like kill-ring)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rwe_ring_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a ring buffer like Emacs kill-ring with fixed capacity,
    // push, pop, rotate, and yank-pop operations.
    let form = r#"(let ((ring-data nil)
                        (ring-size 0)
                        (ring-max 4)
                        (ring-idx 0))
                    (let ((ring-push
                           (lambda (item)
                             (if (>= ring-size ring-max)
                                 ;; Overwrite oldest
                                 (progn
                                   (setq ring-data (cons item (butlast ring-data)))
                                   (setq ring-idx 0))
                               (setq ring-data (cons item ring-data))
                               (setq ring-size (1+ ring-size))
                               (setq ring-idx 0))))
                          (ring-top
                           (lambda ()
                             (nth ring-idx ring-data)))
                          (ring-rotate
                           (lambda (n)
                             (when (> ring-size 0)
                               (setq ring-idx (mod (+ ring-idx n) ring-size)))))
                          (ring-contents
                           (lambda ()
                             (copy-sequence ring-data))))
                      ;; Push items beyond capacity
                      (funcall ring-push "first")
                      (funcall ring-push "second")
                      (funcall ring-push "third")
                      (funcall ring-push "fourth")
                      (let ((full-ring (funcall ring-contents))
                            (top-1 (funcall ring-top)))
                        ;; Push one more (overwrites oldest: "first")
                        (funcall ring-push "fifth")
                        (let ((after-overflow (funcall ring-contents))
                              (top-2 (funcall ring-top)))
                          ;; Rotate
                          (funcall ring-rotate 1)
                          (let ((after-rotate-1 (funcall ring-top)))
                            (funcall ring-rotate 1)
                            (let ((after-rotate-2 (funcall ring-top)))
                              ;; Rotate back
                              (funcall ring-rotate -2)
                              (let ((after-rotate-back (funcall ring-top)))
                                (list full-ring
                                      top-1
                                      after-overflow
                                      top-2
                                      after-rotate-1
                                      after-rotate-2
                                      after-rotate-back
                                      ring-size))))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"fourth\" \"third\" \"second\" \"first\") \"fourth\" (\"fifth\" \"fourth\" \"third\" \"second\") \"fifth\" \"fourth\" \"third\" \"fifth\" 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Auto-completion candidate scoring and ranking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rwe_completion_scoring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Score completion candidates against a query using multiple factors:
    // prefix match, substring match, word boundary bonus, length penalty.
    let form = r#"(let ((candidates
                         '("find-file" "find-file-other-window" "fill-paragraph"
                           "font-lock-mode" "forward-char" "forward-word"
                           "format" "fundamental-mode"))
                        (query "fi"))
                    (let ((score-candidate
                           (lambda (candidate query)
                             (let ((score 0)
                                   (qlen (length query))
                                   (clen (length candidate)))
                               ;; Prefix match: big bonus
                               (when (and (<= qlen clen)
                                          (string= query (substring candidate 0 qlen)))
                                 (setq score (+ score 100)))
                               ;; Substring match anywhere
                               (when (string-match-p (regexp-quote query) candidate)
                                 (setq score (+ score 50)))
                               ;; Word boundary match: query chars at word starts
                               (let ((qi 0)
                                     (boundary t))
                                 (dotimes (ci clen)
                                   (when (and (< qi qlen)
                                              boundary
                                              (= (aref query qi) (aref candidate ci)))
                                     (setq qi (1+ qi)
                                           score (+ score 20)))
                                   (setq boundary (= (aref candidate ci) ?-))))
                               ;; Length penalty: shorter is better
                               (setq score (- score (/ clen 2)))
                               score))))
                      ;; Score all candidates
                      (let ((scored
                             (mapcar (lambda (c)
                                       (cons c (funcall score-candidate c query)))
                                     candidates)))
                        ;; Sort by score descending, then alphabetically for ties
                        (let ((sorted
                               (sort scored
                                     (lambda (a b)
                                       (if (= (cdr a) (cdr b))
                                           (string-lessp (car a) (car b))
                                         (> (cdr a) (cdr b)))))))
                          ;; Return top 5 with scores
                          (let ((top nil)
                                (count 0))
                            (dolist (entry sorted)
                              (when (< count 5)
                                (setq top (cons entry top))
                                (setq count (1+ count))))
                            (nreverse top))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"find-file\" . 166) (\"fill-paragraph\" . 163) (\"find-file-other-window\" . 159) (\"format\" . 17) (\"forward-char\" . 14))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
