//! Oracle parity tests for operating system simulation in pure Elisp.
//!
//! Simulates process table management, memory allocation strategies
//! (first-fit, best-fit, worst-fit), page replacement algorithms
//! (FIFO, LRU, optimal), file system directory tree, simple shell
//! command parser, and pipe/redirection simulation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Process table management (create, kill, list, scheduling)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_os_sim_process_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Manage a process table: create processes with PID, name, priority,
    // state (running/ready/blocked); support kill, state transitions, and listing.
    let form = r##"(progn
  (fset 'neovm--os-pt-create
    (lambda () (list (make-hash-table :test 'eql) 0)))  ;; (table next-pid)

  (fset 'neovm--os-pt-spawn
    (lambda (pt name priority)
      "Spawn process. Return PID."
      (let ((table (nth 0 pt))
            (pid (nth 1 pt)))
        (puthash pid (list name priority 'ready 0) table)  ;; (name prio state cpu-time)
        (setcar (cdr pt) (1+ pid))
        pid)))

  (fset 'neovm--os-pt-kill
    (lambda (pt pid)
      "Kill process by PID. Return t if found, nil otherwise."
      (let ((table (nth 0 pt)))
        (if (gethash pid table)
            (progn (remhash pid table) t)
          nil))))

  (fset 'neovm--os-pt-set-state
    (lambda (pt pid state)
      (let ((proc (gethash pid (nth 0 pt))))
        (when proc (setcar (cddr proc) state)))))

  (fset 'neovm--os-pt-tick
    (lambda (pt pid quantum)
      "Give CPU time to process."
      (let ((proc (gethash pid (nth 0 pt))))
        (when proc
          (setcar (cdddr proc) (+ (nth 3 proc) quantum))
          (funcall 'neovm--os-pt-set-state pt pid 'running)))))

  (fset 'neovm--os-pt-list
    (lambda (pt)
      "List all processes sorted by PID."
      (let ((procs nil))
        (maphash (lambda (pid data)
                   (push (cons pid data) procs))
                 (nth 0 pt))
        (sort procs (lambda (a b) (< (car a) (car b)))))))

  (fset 'neovm--os-pt-schedule-priority
    (lambda (pt)
      "Return PID of highest-priority ready process (lower number = higher prio)."
      (let ((best-pid nil) (best-prio nil))
        (maphash (lambda (pid data)
                   (when (eq (nth 2 data) 'ready)
                     (when (or (null best-prio) (< (nth 1 data) best-prio))
                       (setq best-pid pid best-prio (nth 1 data)))))
                 (nth 0 pt))
        best-pid)))

  (unwind-protect
      (let ((pt (funcall 'neovm--os-pt-create)))
        ;; Spawn processes
        (let ((p-init (funcall 'neovm--os-pt-spawn pt "init" 0))
              (p-shell (funcall 'neovm--os-pt-spawn pt "shell" 5))
              (p-editor (funcall 'neovm--os-pt-spawn pt "editor" 3))
              (p-bg (funcall 'neovm--os-pt-spawn pt "background" 10))
              (p-daemon (funcall 'neovm--os-pt-spawn pt "daemon" 1)))
          ;; Schedule: highest priority ready process
          (let ((s1 (funcall 'neovm--os-pt-schedule-priority pt)))
            ;; Run init for 5 ticks
            (funcall 'neovm--os-pt-tick pt p-init 5)
            ;; Block shell (waiting for input)
            (funcall 'neovm--os-pt-set-state pt p-shell 'blocked)
            ;; Schedule again (init is running, shell blocked)
            (let ((s2 (funcall 'neovm--os-pt-schedule-priority pt)))
              ;; Kill background process
              (let ((kill-result (funcall 'neovm--os-pt-kill pt p-bg))
                    (kill-missing (funcall 'neovm--os-pt-kill pt 999)))
                ;; Unblock shell
                (funcall 'neovm--os-pt-set-state pt p-shell 'ready)
                (let ((final-list (funcall 'neovm--os-pt-list pt))
                      (s3 (funcall 'neovm--os-pt-schedule-priority pt)))
                  (list
                    (list p-init p-shell p-editor p-bg p-daemon)
                    s1 s2 s3
                    kill-result kill-missing
                    final-list)))))))
    (fmakunbound 'neovm--os-pt-create)
    (fmakunbound 'neovm--os-pt-spawn)
    (fmakunbound 'neovm--os-pt-kill)
    (fmakunbound 'neovm--os-pt-set-state)
    (fmakunbound 'neovm--os-pt-tick)
    (fmakunbound 'neovm--os-pt-list)
    (fmakunbound 'neovm--os-pt-schedule-priority)))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// Memory allocation: first-fit, best-fit, worst-fit
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_os_sim_memory_allocation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate contiguous memory allocation with free list.
    // Free blocks: list of (start . size), sorted by address.
    let form = r#"(progn
  (fset 'neovm--os-mem-create
    (lambda (total-size) (list (list (cons 0 total-size)))))  ;; one big free block

  (fset 'neovm--os-mem-first-fit
    (lambda (mem size)
      "First-fit: return start address of allocated block, or nil."
      (let ((free-list (car mem))
            (found nil)
            (prev nil)
            (curr nil)
            (result nil))
        (setq curr free-list)
        (while (and curr (not found))
          (let ((block (car curr)))
            (when (>= (cdr block) size)
              (setq result (car block))
              (if (= (cdr block) size)
                  ;; Exact fit: remove block
                  (if prev
                      (setcdr prev (cdr curr))
                    (setcar mem (cdr curr)))
                ;; Split: shrink block
                (setcar block (+ (car block) size))
                (setcdr block (- (cdr block) size)))
              (setq found t)))
          (unless found
            (setq prev curr)
            (setq curr (cdr curr))))
        result)))

  (fset 'neovm--os-mem-best-fit
    (lambda (mem size)
      "Best-fit: smallest sufficient block."
      (let ((free-list (car mem))
            (best nil) (best-size nil) (best-prev nil)
            (prev nil) (curr nil))
        (setq curr free-list)
        (while curr
          (let ((block (car curr)))
            (when (and (>= (cdr block) size)
                       (or (null best-size) (< (cdr block) best-size)))
              (setq best curr best-size (cdr block) best-prev prev)))
          (setq prev curr curr (cdr curr)))
        (when best
          (let ((result (car (car best))))
            (if (= (cdr (car best)) size)
                (if best-prev
                    (setcdr best-prev (cdr best))
                  (setcar mem (cdr best)))
              (setcar (car best) (+ (car (car best)) size))
              (setcdr (car best) (- (cdr (car best)) size)))
            result)))))

  (fset 'neovm--os-mem-worst-fit
    (lambda (mem size)
      "Worst-fit: largest sufficient block."
      (let ((free-list (car mem))
            (worst nil) (worst-size nil) (worst-prev nil)
            (prev nil) (curr nil))
        (setq curr free-list)
        (while curr
          (let ((block (car curr)))
            (when (and (>= (cdr block) size)
                       (or (null worst-size) (> (cdr block) worst-size)))
              (setq worst curr worst-size (cdr block) worst-prev prev)))
          (setq prev curr curr (cdr curr)))
        (when worst
          (let ((result (car (car worst))))
            (if (= (cdr (car worst)) size)
                (if worst-prev
                    (setcdr worst-prev (cdr worst))
                  (setcar mem (cdr worst)))
              (setcar (car worst) (+ (car (car worst)) size))
              (setcdr (car worst) (- (cdr (car worst)) size)))
            result)))))

  (fset 'neovm--os-mem-free
    (lambda (mem addr size)
      "Free a block. Insert into sorted free list and coalesce neighbors."
      (let ((free-list (car mem))
            (new-block (cons addr size))
            (prev nil) (curr nil) (inserted nil))
        ;; Insert in address order
        (setq curr free-list)
        (while (and curr (not inserted))
          (if (< addr (car (car curr)))
              (progn
                (if prev
                    (setcdr prev (cons new-block curr))
                  (setcar mem (cons new-block curr)))
                (setq inserted t))
            (setq prev curr curr (cdr curr))))
        (unless inserted
          (if prev
              (setcdr prev (list new-block))
            (setcar mem (list new-block))))
        ;; Coalesce adjacent blocks
        (let ((fl (car mem)) (merged nil))
          (while fl
            (if (and merged (= (+ (car (car merged)) (cdr (car merged)))
                               (car (car fl))))
                ;; Merge with previous
                (setcdr (car merged) (+ (cdr (car merged)) (cdr (car fl))))
              (if merged
                  (progn (setcdr merged fl) (setq merged fl))
                (setq merged fl)))
            (setq fl (cdr fl)))
          (when merged (setcdr merged nil))))))

  (unwind-protect
      (let ((mem (funcall 'neovm--os-mem-create 100)))
        ;; First-fit allocations
        (let ((a1 (funcall 'neovm--os-mem-first-fit mem 20))
              (a2 (funcall 'neovm--os-mem-first-fit mem 30))
              (a3 (funcall 'neovm--os-mem-first-fit mem 10)))
          ;; Free middle block
          (funcall 'neovm--os-mem-free mem a2 30)
          ;; Now free list has hole at 20-49 and remainder at 60-99
          (let ((state-after-free (copy-sequence (car mem))))
            ;; Best-fit should pick the 30-unit hole for a 25-unit request
            (let ((a4 (funcall 'neovm--os-mem-best-fit mem 25)))
              ;; Worst-fit should pick the larger remaining block
              (let ((a5 (funcall 'neovm--os-mem-worst-fit mem 10)))
                (list a1 a2 a3 state-after-free a4 a5
                      ;; Remaining free blocks
                      (car mem)))))))
    (fmakunbound 'neovm--os-mem-create)
    (fmakunbound 'neovm--os-mem-first-fit)
    (fmakunbound 'neovm--os-mem-best-fit)
    (fmakunbound 'neovm--os-mem-worst-fit)
    (fmakunbound 'neovm--os-mem-free)))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// Page replacement: FIFO and LRU
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_os_sim_page_replacement() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // FIFO and LRU page replacement with a fixed-size frame buffer.
    let form = r#"(progn
  (fset 'neovm--os-fifo-replace
    (lambda (frame-count page-refs)
      "FIFO page replacement. Return (page-faults final-frames)."
      (let ((frames nil) (faults 0) (queue nil))
        (dolist (page page-refs)
          (if (memq page frames)
              nil  ;; hit
            ;; fault
            (setq faults (1+ faults))
            (if (< (length frames) frame-count)
                ;; Space available
                (progn
                  (setq frames (append frames (list page)))
                  (setq queue (append queue (list page))))
              ;; Evict oldest (front of queue)
              (let ((victim (car queue)))
                (setq queue (cdr queue))
                (setq frames (delq victim frames))
                (setq frames (append frames (list page)))
                (setq queue (append queue (list page)))))))
        (list faults frames))))

  (fset 'neovm--os-lru-replace
    (lambda (frame-count page-refs)
      "LRU page replacement. Return (page-faults final-frames)."
      (let ((frames nil) (faults 0) (access-order nil))
        (dolist (page page-refs)
          (if (memq page frames)
              ;; Hit: move to most-recently-used position
              (setq access-order (append (delq page access-order) (list page)))
            ;; Fault
            (setq faults (1+ faults))
            (if (< (length frames) frame-count)
                (progn
                  (setq frames (append frames (list page)))
                  (setq access-order (append access-order (list page))))
              ;; Evict LRU (front of access-order)
              (let ((victim (car access-order)))
                (setq access-order (cdr access-order))
                (setq frames (delq victim frames))
                (setq frames (append frames (list page)))
                (setq access-order (append access-order (list page)))))))
        (list faults frames))))

  (unwind-protect
      (let ((refs '(1 2 3 4 1 2 5 1 2 3 4 5))
            (frame-count 3))
        (let ((fifo-result (funcall 'neovm--os-fifo-replace frame-count refs))
              (lru-result (funcall 'neovm--os-lru-replace frame-count refs)))
          (list
            ;; FIFO results
            (nth 0 fifo-result)   ;; fault count
            (nth 1 fifo-result)   ;; final frames
            ;; LRU results
            (nth 0 lru-result)    ;; fault count
            (nth 1 lru-result)    ;; final frames
            ;; Test with different reference pattern (more locality)
            (let ((refs2 '(1 2 1 3 1 2 1 4 1 2 1 3)))
              (let ((fifo2 (funcall 'neovm--os-fifo-replace 3 refs2))
                    (lru2 (funcall 'neovm--os-lru-replace 3 refs2)))
                (list (nth 0 fifo2) (nth 0 lru2)))))))
    (fmakunbound 'neovm--os-fifo-replace)
    (fmakunbound 'neovm--os-lru-replace)))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// File system simulation: directory tree and file operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_os_sim_filesystem() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a hierarchical file system with directories and files.
    // Operations: mkdir, touch, ls, find, rm.
    let form = r#"(progn
  ;; FS node: (name type children content)
  ;; type = 'dir or 'file; children = alist for dirs; content = string for files
  (fset 'neovm--os-fs-create
    (lambda () (list "/" 'dir nil nil)))  ;; root dir

  (fset 'neovm--os-fs-resolve
    (lambda (fs path-parts)
      "Navigate path-parts from FS root. Return node or nil."
      (let ((node fs))
        (dolist (part path-parts)
          (when (and node (eq (nth 1 node) 'dir))
            (setq node (assoc part (nth 2 node)))))
        node)))

  (fset 'neovm--os-fs-mkdir
    (lambda (fs path-parts name)
      "Create directory NAME under PATH-PARTS."
      (let ((parent (funcall 'neovm--os-fs-resolve fs path-parts)))
        (when (and parent (eq (nth 1 parent) 'dir)
                   (not (assoc name (nth 2 parent))))
          (let ((new-dir (list name 'dir nil nil)))
            (setcar (cddr parent) (cons new-dir (nth 2 parent)))
            t)))))

  (fset 'neovm--os-fs-touch
    (lambda (fs path-parts name content)
      "Create/overwrite file NAME under PATH-PARTS with CONTENT."
      (let ((parent (funcall 'neovm--os-fs-resolve fs path-parts)))
        (when (and parent (eq (nth 1 parent) 'dir))
          (let ((existing (assoc name (nth 2 parent))))
            (if existing
                (setcar (cdddr existing) content)
              (setcar (cddr parent)
                      (cons (list name 'file nil content) (nth 2 parent)))))
          t))))

  (fset 'neovm--os-fs-ls
    (lambda (fs path-parts)
      "List entries in directory at PATH-PARTS."
      (let ((dir (funcall 'neovm--os-fs-resolve fs path-parts)))
        (when (and dir (eq (nth 1 dir) 'dir))
          (sort (mapcar (lambda (child)
                          (cons (nth 0 child) (nth 1 child)))
                        (nth 2 dir))
                (lambda (a b) (string< (car a) (car b))))))))

  (fset 'neovm--os-fs-find
    (lambda (fs node prefix)
      "Recursively list all paths under NODE."
      (let ((result (list (cons prefix (nth 1 node)))))
        (when (eq (nth 1 node) 'dir)
          (dolist (child (nth 2 node))
            (let ((child-path (concat prefix "/" (nth 0 child))))
              (setq result (append result
                                   (funcall 'neovm--os-fs-find fs child child-path))))))
        result)))

  (fset 'neovm--os-fs-rm
    (lambda (fs path-parts name)
      "Remove entry NAME from directory at PATH-PARTS."
      (let ((parent (funcall 'neovm--os-fs-resolve fs path-parts)))
        (when (and parent (eq (nth 1 parent) 'dir))
          (let ((children (nth 2 parent)))
            (setcar (cddr parent)
                    (cl-remove-if (lambda (c) (equal (nth 0 c) name)) children))
            t)))))

  (unwind-protect
      (progn
        (require 'cl-lib)
        (let ((fs (funcall 'neovm--os-fs-create)))
          ;; Build directory structure
          (funcall 'neovm--os-fs-mkdir fs nil "home")
          (funcall 'neovm--os-fs-mkdir fs '("home") "user")
          (funcall 'neovm--os-fs-mkdir fs '("home" "user") "docs")
          (funcall 'neovm--os-fs-mkdir fs '("home" "user") "bin")
          (funcall 'neovm--os-fs-mkdir fs nil "etc")
          ;; Create files
          (funcall 'neovm--os-fs-touch fs '("home" "user" "docs") "readme.txt" "Hello World")
          (funcall 'neovm--os-fs-touch fs '("home" "user" "docs") "notes.md" "# Notes")
          (funcall 'neovm--os-fs-touch fs '("home" "user" "bin") "script.sh" "#!/bin/sh")
          (funcall 'neovm--os-fs-touch fs '("etc") "config.ini" "[global]")
          ;; List operations
          (let ((ls-root (funcall 'neovm--os-fs-ls fs nil))
                (ls-docs (funcall 'neovm--os-fs-ls fs '("home" "user" "docs"))))
            ;; Find all paths
            (let ((all-paths (sort
                               (funcall 'neovm--os-fs-find fs fs "")
                               (lambda (a b) (string< (car a) (car b))))))
              ;; Remove a file
              (funcall 'neovm--os-fs-rm fs '("home" "user" "docs") "notes.md")
              (let ((ls-after-rm (funcall 'neovm--os-fs-ls fs '("home" "user" "docs"))))
                (list ls-root ls-docs all-paths ls-after-rm))))))
    (fmakunbound 'neovm--os-fs-create)
    (fmakunbound 'neovm--os-fs-resolve)
    (fmakunbound 'neovm--os-fs-mkdir)
    (fmakunbound 'neovm--os-fs-touch)
    (fmakunbound 'neovm--os-fs-ls)
    (fmakunbound 'neovm--os-fs-find)
    (fmakunbound 'neovm--os-fs-rm)))"##;
    let expect = expect_test::expect![[
        r#""OK (((\"etc\" . dir) (\"home\" . dir)) ((\"notes.md\" . file) (\"readme.txt\" . file)) ((\"\" . dir) (\"/etc\" . dir) (\"/etc/config.ini\" . file) (\"/home\" . dir) (\"/home/user\" . dir) (\"/home/user/bin\" . dir) (\"/home/user/bin/script.sh\" . file) (\"/home/user/docs\" . dir) (\"/home/user/docs/notes.md\" . file) (\"/home/user/docs/readme.txt\" . file)) ((\"readme.txt\" . file)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Simple shell command parser
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_os_sim_shell_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse shell-like commands: handle words, quoted strings, pipes, redirects.
    let form = r#"(progn
  (fset 'neovm--os-shell-tokenize
    (lambda (input)
      "Tokenize shell input into tokens: words, pipes (|), redirects (> >>)."
      (let ((tokens nil) (pos 0) (len (length input)))
        (while (< pos len)
          (let ((ch (aref input pos)))
            (cond
              ;; Whitespace: skip
              ((memq ch '(?\s ?\t))
               (setq pos (1+ pos)))
              ;; Pipe
              ((= ch ?|)
               (push '(pipe) tokens)
               (setq pos (1+ pos)))
              ;; Redirect
              ((= ch ?>)
               (if (and (< (1+ pos) len) (= (aref input (1+ pos)) ?>))
                   (progn (push '(append-redirect) tokens)
                          (setq pos (+ pos 2)))
                 (push '(redirect) tokens)
                 (setq pos (1+ pos))))
              ;; Quoted string
              ((= ch ?\")
               (let ((start (1+ pos)) (end nil))
                 (setq pos (1+ pos))
                 (while (and (< pos len) (not end))
                   (if (= (aref input pos) ?\")
                       (setq end pos)
                     (setq pos (1+ pos))))
                 (when end
                   (push (list 'word (substring input start end)) tokens)
                   (setq pos (1+ pos)))))
              ;; Regular word
              (t
               (let ((start pos))
                 (while (and (< pos len)
                             (not (memq (aref input pos) '(?\s ?\t ?| ?> ?\"))))
                   (setq pos (1+ pos)))
                 (push (list 'word (substring input start pos)) tokens))))))
        (nreverse tokens))))

  (fset 'neovm--os-shell-parse
    (lambda (tokens)
      "Parse tokens into pipeline of commands, each with args and redirects."
      (let ((commands nil) (current-cmd nil) (current-redirect nil))
        (dolist (tok tokens)
          (cond
            ((eq (car tok) 'pipe)
             (when current-cmd
               (push (list 'command (nreverse current-cmd) current-redirect) commands))
             (setq current-cmd nil current-redirect nil))
            ((memq (car tok) '(redirect append-redirect))
             (setq current-redirect (car tok)))
            ((eq (car tok) 'word)
             (if current-redirect
                 (progn
                   (push (list current-redirect (nth 1 tok)) (or current-cmd nil))
                   (setq current-cmd (or current-cmd nil))
                   (setq current-redirect nil))
               (push (nth 1 tok) current-cmd)))))
        (when current-cmd
          (push (list 'command (nreverse current-cmd) current-redirect) commands))
        (nreverse commands))))

  (unwind-protect
      (let ((tests (list
                     "ls -la /home"
                     "cat file.txt | grep pattern | sort"
                     "echo \"hello world\" > output.txt"
                     "find . -name \"*.rs\" | wc -l >> counts.log"
                     "ps aux | grep emacs | head -5")))
        (mapcar (lambda (cmd)
                  (let ((tokens (funcall 'neovm--os-shell-tokenize cmd)))
                    (list cmd tokens
                          (funcall 'neovm--os-shell-parse tokens))))
                tests))
    (fmakunbound 'neovm--os-shell-tokenize)
    (fmakunbound 'neovm--os-shell-parse)))"#;
    let expect = expect_test::expect![[r#""ERR (void-function \\(setf\\ or\\))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Pipe and redirection simulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_os_sim_pipe_redirection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate Unix pipe: each command is a function that takes input lines
    // and returns output lines. Chain them with pipe semantics.
    let form = r#"(progn
  ;; Built-in command simulators
  (fset 'neovm--os-cmd-echo
    (lambda (args _input)
      (list (mapconcat #'identity args " "))))

  (fset 'neovm--os-cmd-grep
    (lambda (args input)
      "Filter lines containing pattern (first arg)."
      (let ((pattern (car args)))
        (cl-remove-if-not
          (lambda (line) (string-match-p (regexp-quote pattern) line))
          input))))

  (fset 'neovm--os-cmd-sort
    (lambda (_args input)
      (sort (copy-sequence input) #'string<)))

  (fset 'neovm--os-cmd-uniq
    (lambda (_args input)
      "Remove adjacent duplicates."
      (let ((result nil) (prev nil))
        (dolist (line input)
          (unless (equal line prev)
            (push line result)
            (setq prev line)))
        (nreverse result))))

  (fset 'neovm--os-cmd-head
    (lambda (args input)
      "Return first N lines (default 5)."
      (let ((n (if args (string-to-number (car args)) 5)))
        (let ((result nil) (i 0))
          (while (and (< i n) input)
            (push (car input) result)
            (setq input (cdr input) i (1+ i)))
          (nreverse result)))))

  (fset 'neovm--os-cmd-wc
    (lambda (args input)
      "Count lines (-l), words (-w), or chars (-c)."
      (let ((flag (or (car args) "-l")))
        (cond
          ((equal flag "-l") (list (number-to-string (length input))))
          ((equal flag "-w")
           (list (number-to-string
                   (let ((count 0))
                     (dolist (line input)
                       (setq count (+ count (length (split-string line)))))
                     count))))
          (t (list (number-to-string
                     (let ((count 0))
                       (dolist (line input)
                         (setq count (+ count (length line))))
                       count))))))))

  (fset 'neovm--os-run-pipeline
    (lambda (pipeline initial-input)
      "Run a pipeline of (cmd-fn . args) pairs, threading input through."
      (let ((data initial-input))
        (dolist (stage pipeline)
          (let ((cmd-fn (car stage))
                (args (cdr stage)))
            (setq data (funcall cmd-fn args data))))
        data)))

  (unwind-protect
      (progn
        (require 'cl-lib)
        (let ((input-data '("apple pie" "banana split"
                            "cherry tart" "apple sauce"
                            "banana bread" "date cake"
                            "apple crisp" "cherry jam"
                            "elderberry wine" "fig newton")))
          (list
            ;; Pipeline 1: grep apple | sort
            (funcall 'neovm--os-run-pipeline
                     (list (cons 'neovm--os-cmd-grep '("apple"))
                           (cons 'neovm--os-cmd-sort nil))
                     input-data)
            ;; Pipeline 2: sort | head 3
            (funcall 'neovm--os-run-pipeline
                     (list (cons 'neovm--os-cmd-sort nil)
                           (cons 'neovm--os-cmd-head '("3")))
                     input-data)
            ;; Pipeline 3: grep "a" | sort | uniq | wc -l
            (funcall 'neovm--os-run-pipeline
                     (list (cons 'neovm--os-cmd-grep '("a"))
                           (cons 'neovm--os-cmd-sort nil)
                           (cons 'neovm--os-cmd-uniq nil)
                           (cons 'neovm--os-cmd-wc '("-l")))
                     input-data)
            ;; Pipeline 4: echo produces input, then wc counts
            (funcall 'neovm--os-run-pipeline
                     (list (cons 'neovm--os-cmd-wc '("-w")))
                     '("one two three" "four five" "six"))
            ;; Pipeline 5: grep cherry | wc -l
            (funcall 'neovm--os-run-pipeline
                     (list (cons 'neovm--os-cmd-grep '("cherry"))
                           (cons 'neovm--os-cmd-wc '("-l")))
                     input-data))))
    (fmakunbound 'neovm--os-cmd-echo)
    (fmakunbound 'neovm--os-cmd-grep)
    (fmakunbound 'neovm--os-cmd-sort)
    (fmakunbound 'neovm--os-cmd-uniq)
    (fmakunbound 'neovm--os-cmd-head)
    (fmakunbound 'neovm--os-cmd-wc)
    (fmakunbound 'neovm--os-run-pipeline)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"apple crisp\" \"apple pie\" \"apple sauce\") (\"apple crisp\" \"apple pie\" \"apple sauce\") (\"8\") (\"6\") (\"2\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Optimal page replacement (Belady's algorithm)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_os_sim_optimal_page_replacement() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Optimal (Belady's) algorithm: evict the page that will not be used
    // for the longest time in the future. Requires future knowledge.
    let form = r#"(progn
  (fset 'neovm--os-optimal-replace
    (lambda (frame-count page-refs)
      "Optimal page replacement. Return (page-faults eviction-log final-frames)."
      (let ((frames nil) (faults 0) (evictions nil) (ref-list page-refs) (time 0))
        (while ref-list
          (let ((page (car ref-list))
                (future (cdr ref-list)))
            (if (memq page frames)
                nil  ;; hit
              ;; fault
              (setq faults (1+ faults))
              (if (< (length frames) frame-count)
                  ;; Space available
                  (setq frames (append frames (list page)))
                ;; Evict: find frame whose next use is farthest away
                (let ((victim nil) (max-dist -1))
                  (dolist (f frames)
                    (let ((next-use (let ((pos 0) (found nil) (rem future))
                                      (while (and rem (not found))
                                        (when (eq (car rem) f) (setq found pos))
                                        (setq rem (cdr rem) pos (1+ pos)))
                                      (or found 999999))))
                      (when (> next-use max-dist)
                        (setq victim f max-dist next-use))))
                  (push (cons time victim) evictions)
                  (setq frames (delq victim frames))
                  (setq frames (append frames (list page)))))))
          (setq ref-list (cdr ref-list) time (1+ time)))
        (list faults (nreverse evictions) frames))))

  (unwind-protect
      (let ((refs '(1 2 3 4 1 2 5 1 2 3 4 5))
            (refs2 '(7 0 1 2 0 3 0 4 2 3 0 3 2 1 2)))
        (let ((r1 (funcall 'neovm--os-optimal-replace 3 refs))
              (r2 (funcall 'neovm--os-optimal-replace 4 refs2)))
          (list
            ;; Test 1: 3 frames
            (nth 0 r1)  ;; fault count
            (nth 1 r1)  ;; eviction log
            (nth 2 r1)  ;; final frames
            ;; Test 2: 4 frames, different reference string
            (nth 0 r2)
            (nth 1 r2)
            (nth 2 r2))))
    (fmakunbound 'neovm--os-optimal-replace)))"#;
    let expect = expect_test::expect![[
        r#""OK (7 ((3 . 3) (6 . 4) (9 . 1) (10 . 2)) (5 3 4) 7 ((5 . 7) (7 . 1) (13 . 0)) (2 3 4 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
