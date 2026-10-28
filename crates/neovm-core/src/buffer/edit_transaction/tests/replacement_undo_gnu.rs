//! GNU 31.1 replacement undo ordering and live deletion marker observations.
//!
//! The complete matrix keeps raw undo printing and an identity-labelled marker
//! projection, all hook snapshots, character/byte positions and insertion types.
//! Expected lists below are pasted from GNU stdout, never derived from Neo.
//!
//! GNU source: undo.c:64-65,96,172 independently runs first-change recording for
//! both recorders while insdel.c:1638-1639 records insertion then deletion.
//! MODIFF changes only afterward at insdel.c:1687. A clean replacement therefore
//! has two first-change entries; an already modified buffer has none.

use crate::test_utils::runtime_startup_eval_one;

// Actual GNU 31.1 fixture capture (2026-10-24), serialized with no other engine:
// CARGO_TARGET_DIR=/home/exec/nvme-rust-target/v10x-wt-lead-vmcall
// TMPDIR=$PWD/tmp/vmcall/undofix-oracle-v1/tmp XDG_RUNTIME_DIR=/run/user/1001
// DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1001/bus
// taskset -c 0-3,14-31 nix develop /home/exec/Projects/github.com/eval-exec/neomacs-main -c \
// /home/exec/Projects/github.com/eval-exec/neomacs-main/tmp/v10x/sandbox-run.sh \
// $NEOVM_FORCE_ORACLE_PATH -Q --batch -l \
// /home/exec/Projects/github.com/eval-exec/neomacs-main/tmp/wt-lead-vmcall/tmp/vmcall/undofix-oracle-v1/capture.el
// Full input, argv, environment, stdout/stderr, exit and hashes are retained in
// tmp/vmcall/undofix-oracle-v1/. The original sixteen-case GNU result was freshly
// reproduced byte for byte (SHA256 a2a75df2e01b2ce102694de7c21c1cfab6ab7820291a9508f477576717fa7523).
// New dirty-buffer expectations came only from the same sandboxed GNU capture.

const HELPERS: &str = r###"(progn
  (defun undometa--normalized-undo (undo markers)
    (if (eq undo t) t
      (mapcar
       (lambda (entry)
         (if (and (consp entry) (markerp (car entry)))
             (let ((rest markers) (index 0))
               (while (and rest (not (eq (car rest) (car entry))))
                 (setq rest (cdr rest) index (1+ index)))
               (cons (list 'marker (if rest index 'untracked)
                           (marker-position (car entry))
                           (marker-insertion-type (car entry)))
                     (cdr entry)))
           entry)) undo)))
  (defun undometa--snapshot (markers)
    (list (buffer-string) (point) (buffer-modified-p)
          (mapcar (lambda (m)
                    (let ((pos (marker-position m)))
                      (list pos (and pos (position-bytes pos))
                            (marker-insertion-type m))))
                  markers)
          (prin1-to-string buffer-undo-list)
          (prin1-to-string (undometa--normalized-undo buffer-undo-list markers))))
  (defun undometa--markers ()
    (let (result)
      ;; Before FROM, exactly FROM, interior, exactly TO, after TO.
      ;; Both insertion types are retained at every class, in creation order.
      (dolist (pos '(2 3 4 6 7))
        (setq result (append result (list (copy-marker pos nil)
                                          (copy-marker pos t)))))
      result))
  (defun undometa--edit (op)
    (cond ((eq op 'insert) (goto-char 3) (insert "λ"))
          ((eq op 'delete) (delete-region 3 6))
          ((eq op 'delete-point-at-end) (goto-char 6) (delete-region 3 6))
          ((eq op 'replace) (goto-char 3) (search-forward "中bc")
           (replace-match "ΩZ" t t))
          ((eq op 'replace-delete) (goto-char 3) (search-forward "中bc")
           (replace-match "" t t))
          ((eq op 'replace-insert) (goto-char 3) (re-search-forward "")
           (replace-match "ΩZ" t t))
          (t (error "unknown bounded undometa operation"))))
  (defun undometa--case (tag op initial phase final mutate)
    (let ((buf (generate-new-buffer (format " *undometa-%s*" tag))))
      (unwind-protect
          (with-current-buffer buf
            (setq-local before-change-functions nil after-change-functions nil)
            (setq buffer-undo-list t)
            (insert "aé中bcdef")
            (goto-char (point-max))
            (set-buffer-modified-p nil)
            (setq buffer-undo-list initial)
            (let ((markers (undometa--markers)) (log nil))
              (when (eq phase 'before)
                (setq-local before-change-functions
                  (list (lambda (_beg _end)
                          (push (list 'before-entry (undometa--snapshot markers)) log)
                          (setq buffer-undo-list final)
                          (when mutate
                            ;; Public versions of the already tested direct
                            ;; prepare/commit move, detach, attach and type change.
                            (set-marker (nth 0 markers) 4)
                            (set-marker (nth 4 markers) nil)
                            (set-marker-insertion-type (nth 1 markers) nil)
                            (setq markers (append markers (list (copy-marker 5 t)))))
                          (push (list 'before-exit (undometa--snapshot markers)) log)))))
              (when (eq phase 'after)
                (setq-local after-change-functions
                  (list (lambda (_beg _end _old)
                          (push (list 'after-entry (undometa--snapshot markers)) log)
                          (setq buffer-undo-list final)
                          (push (list 'after-exit (undometa--snapshot markers)) log)))))
              (let ((before (undometa--snapshot markers)))
                (undometa--edit op)
                (list tag op initial phase final before (nreverse log)
                      (undometa--snapshot markers)))))
        (when (buffer-live-p buf) (kill-buffer buf)))))
  (defun undometa--sequence ()
    (let ((buf (generate-new-buffer " *undometa-sequence*")))
      (unwind-protect
          (with-current-buffer buf
            (setq-local before-change-functions nil after-change-functions nil)
            (setq buffer-undo-list t)
            (insert "aé中bcdef")
            (goto-char (point-max))
            (set-buffer-modified-p nil)
            (setq buffer-undo-list nil)
            (let ((markers (undometa--markers)) (states nil))
              (push (list 'initial (undometa--snapshot markers)) states)
              (goto-char 3) (insert "λ")
              (push (list 'insert (undometa--snapshot markers)) states)
              (delete-region 3 7)
              (push (list 'delete (undometa--snapshot markers)) states)
              (goto-char 3) (search-forward "de") (replace-match "ΩZ" t t)
              (push (list 'replace (undometa--snapshot markers)) states)
              (nreverse states)))
        (when (buffer-live-p buf) (kill-buffer buf)))))
)"###;

const MATRIX_OBSERVE: &str = r###"(let ((inhibit-modification-hooks nil) result)
    ;; Independent operations expose the exact marker endpoint distinctions.
    (dolist (op '(insert delete delete-point-at-end replace replace-delete replace-insert))
      (push (undometa--case (symbol-name op) op nil nil nil nil) result))
    ;; Before hooks determine the deletion's live recording eligibility.
    ;; After hooks run after records/relocation and can replace the final list;
    ;; their entry snapshots preserve the records before that replacement.
    (dolist (phase '(before after))
      (dolist (initial '(nil t))
        (dolist (final '(nil t))
          (push (undometa--case (format "%s-%s-%s" phase initial final)
                               'delete initial phase final nil) result))))
    (push (undometa--case "before-live-markers" 'delete t 'before nil t) result)
    (push (list 'sequence (undometa--sequence)) result)
    (nreverse result))"###;

const DIRTY_HELPERS: &str = r###"(progn
  (defun undometa--first-change-count ()
    (let ((count 0))
      (dolist (entry buffer-undo-list)
        (when (and (consp entry) (eq (car entry) t))
          (setq count (1+ count))))
      count))
  (defun undometa--dirty-case (op)
    (let ((buf (generate-new-buffer (format " *undometa-dirty-%s*" op))))
      (unwind-protect
          (with-current-buffer buf
            (setq-local before-change-functions nil after-change-functions nil)
            (setq buffer-undo-list t)
            (insert "aé中bcdef")
            (goto-char (point-max))
            (set-buffer-modified-p t)
            (setq buffer-undo-list nil)
            (let* ((markers (undometa--markers))
                   (before (undometa--snapshot markers)))
              (undometa--edit op)
              (list op before (undometa--snapshot markers)
                    (undometa--first-change-count))))
        (when (buffer-live-p buf) (kill-buffer buf)))))
  (defun undometa--dirty-sequence ()
    (let ((buf (generate-new-buffer " *undometa-dirty-sequence*")))
      (unwind-protect
          (with-current-buffer buf
            (setq-local before-change-functions nil after-change-functions nil)
            (setq buffer-undo-list t)
            (insert "aé中bcdef")
            (goto-char (point-max))
            (set-buffer-modified-p t)
            (setq buffer-undo-list nil)
            (let ((markers (undometa--markers)) (states nil))
              (push (list 'initial (undometa--snapshot markers)) states)
              (goto-char 3) (insert "λ")
              (push (list 'insert (undometa--snapshot markers)) states)
              (delete-region 3 7)
              (push (list 'delete (undometa--snapshot markers)) states)
              (goto-char 3) (search-forward "de") (replace-match "ΩZ" t t)
              (push (list 'replace (undometa--snapshot markers)) states)
              (list 'dirty-sequence (nreverse states)
                    (undometa--first-change-count))))
        (when (buffer-live-p buf) (kill-buffer buf))))))"###;

const DIRTY_OBSERVE: &str = r###"(list (undometa--dirty-case 'replace)
      (undometa--dirty-case 'replace-delete)
      (undometa--dirty-case 'replace-insert)
      (undometa--dirty-sequence))"###;

const GNU_MATRIX: &str = r###"(("insert" insert nil nil nil ("aé中bcdef" 9 nil ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "nil" "nil") nil ("aéλ中bcdef" 4 t ((2 2 nil) (2 2 t) (3 4 nil) (4 6 t) (5 9 nil) (5 9 t) (7 11 nil) (7 11 t) (8 12 nil) (8 12 t)) "((3 . 4) (t . 0))" "((3 . 4) (t . 0))")) ("delete" delete nil nil nil ("aé中bcdef" 9 nil ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "nil" "nil") nil ("aédef" 6 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (4 5 nil) (4 5 t)) "((\"中bc\" . 3) (#<marker (moves after insertion) at 3 in  *undometa-delete*> . 3) (#<marker at 3 in  *undometa-delete*> . -1) (#<marker (moves after insertion) at 3 in  *undometa-delete*> . 2) (#<marker at 3 in  *undometa-delete*> . -3) (t . 0))" "((\"中bc\" . 3) ((marker 3 3 t) . 3) ((marker 4 3 nil) . -1) ((marker 5 3 t) . 2) ((marker 6 3 nil) . -3) (t . 0))")) ("delete-point-at-end" delete-point-at-end nil nil nil ("aé中bcdef" 9 nil ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "nil" "nil") nil ("aédef" 3 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (4 5 nil) (4 5 t)) "((\"中bc\" . -3) (#<marker (moves after insertion) at 3 in  *undometa-delete-point-at-end*> . 3) (#<marker at 3 in  *undometa-delete-point-at-end*> . -1) (#<marker (moves after insertion) at 3 in  *undometa-delete-point-at-end*> . 2) (#<marker at 3 in  *undometa-delete-point-at-end*> . -3) (t . 0))" "((\"中bc\" . -3) ((marker 3 3 t) . 3) ((marker 4 3 nil) . -1) ((marker 5 3 t) . 2) ((marker 6 3 nil) . -3) (t . 0))")) ("replace" replace nil nil nil ("aé中bcdef" 9 nil ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "nil" "nil") nil ("aéΩZdef" 5 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (5 7 nil) (5 7 t) (6 8 nil) (6 8 t)) "((\"中bc\" . -3) (t . 0) (6 . 8) (t . 0))" "((\"中bc\" . -3) (t . 0) (6 . 8) (t . 0))")) ("replace-delete" replace-delete nil nil nil ("aé中bcdef" 9 nil ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "nil" "nil") nil ("aédef" 3 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (4 5 nil) (4 5 t)) "((\"中bc\" . -3) (t . 0) (6 . 6) (t . 0))" "((\"中bc\" . -3) (t . 0) (6 . 6) (t . 0))")) ("replace-insert" replace-insert nil nil nil ("aé中bcdef" 9 nil ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "nil" "nil") nil ("aéΩZ中bcdef" 5 t ((2 2 nil) (2 2 t) (3 4 nil) (5 7 t) (6 10 nil) (6 10 t) (8 12 nil) (8 12 t) (9 13 nil) (9 13 t)) "((\"\" . -3) (t . 0) (3 . 5) (t . 0))" "((\"\" . -3) (t . 0) (3 . 5) (t . 0))")) ("before-nil-nil" delete nil before nil ("aé中bcdef" 9 nil ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "nil" "nil") ((before-entry ("aé中bcdef" 9 nil ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "nil" "nil")) (before-exit ("aé中bcdef" 9 nil ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "nil" "nil"))) ("aédef" 6 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (4 5 nil) (4 5 t)) "((\"中bc\" . 3) (#<marker (moves after insertion) at 3 in  *undometa-before-nil-nil*> . 3) (#<marker at 3 in  *undometa-before-nil-nil*> . -1) (#<marker (moves after insertion) at 3 in  *undometa-before-nil-nil*> . 2) (#<marker at 3 in  *undometa-before-nil-nil*> . -3) (t . 0))" "((\"中bc\" . 3) ((marker 3 3 t) . 3) ((marker 4 3 nil) . -1) ((marker 5 3 t) . 2) ((marker 6 3 nil) . -3) (t . 0))")) ("before-nil-t" delete nil before t ("aé中bcdef" 9 nil ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "nil" "nil") ((before-entry ("aé中bcdef" 9 nil ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "nil" "nil")) (before-exit ("aé中bcdef" 9 nil ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "t" "t"))) ("aédef" 6 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (4 5 nil) (4 5 t)) "t" "t")) ("before-t-nil" delete t before nil ("aé中bcdef" 9 nil ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "t" "t") ((before-entry ("aé中bcdef" 9 nil ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "t" "t")) (before-exit ("aé中bcdef" 9 nil ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "nil" "nil"))) ("aédef" 6 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (4 5 nil) (4 5 t)) "((\"中bc\" . 3) (#<marker (moves after insertion) at 3 in  *undometa-before-t-nil*> . 3) (#<marker at 3 in  *undometa-before-t-nil*> . -1) (#<marker (moves after insertion) at 3 in  *undometa-before-t-nil*> . 2) (#<marker at 3 in  *undometa-before-t-nil*> . -3) (t . 0))" "((\"中bc\" . 3) ((marker 3 3 t) . 3) ((marker 4 3 nil) . -1) ((marker 5 3 t) . 2) ((marker 6 3 nil) . -3) (t . 0))")) ("before-t-t" delete t before t ("aé中bcdef" 9 nil ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "t" "t") ((before-entry ("aé中bcdef" 9 nil ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "t" "t")) (before-exit ("aé中bcdef" 9 nil ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "t" "t"))) ("aédef" 6 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (4 5 nil) (4 5 t)) "t" "t")) ("after-nil-nil" delete nil after nil ("aé中bcdef" 9 nil ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "nil" "nil") ((after-entry ("aédef" 6 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (4 5 nil) (4 5 t)) "((\"中bc\" . 3) (#<marker (moves after insertion) at 3 in  *undometa-after-nil-nil*> . 3) (#<marker at 3 in  *undometa-after-nil-nil*> . -1) (#<marker (moves after insertion) at 3 in  *undometa-after-nil-nil*> . 2) (#<marker at 3 in  *undometa-after-nil-nil*> . -3) (t . 0))" "((\"中bc\" . 3) ((marker 3 3 t) . 3) ((marker 4 3 nil) . -1) ((marker 5 3 t) . 2) ((marker 6 3 nil) . -3) (t . 0))")) (after-exit ("aédef" 6 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (4 5 nil) (4 5 t)) "nil" "nil"))) ("aédef" 6 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (4 5 nil) (4 5 t)) "nil" "nil")) ("after-nil-t" delete nil after t ("aé中bcdef" 9 nil ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "nil" "nil") ((after-entry ("aédef" 6 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (4 5 nil) (4 5 t)) "((\"中bc\" . 3) (#<marker (moves after insertion) at 3 in  *undometa-after-nil-t*> . 3) (#<marker at 3 in  *undometa-after-nil-t*> . -1) (#<marker (moves after insertion) at 3 in  *undometa-after-nil-t*> . 2) (#<marker at 3 in  *undometa-after-nil-t*> . -3) (t . 0))" "((\"中bc\" . 3) ((marker 3 3 t) . 3) ((marker 4 3 nil) . -1) ((marker 5 3 t) . 2) ((marker 6 3 nil) . -3) (t . 0))")) (after-exit ("aédef" 6 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (4 5 nil) (4 5 t)) "t" "t"))) ("aédef" 6 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (4 5 nil) (4 5 t)) "t" "t")) ("after-t-nil" delete t after nil ("aé中bcdef" 9 nil ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "t" "t") ((after-entry ("aédef" 6 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (4 5 nil) (4 5 t)) "t" "t")) (after-exit ("aédef" 6 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (4 5 nil) (4 5 t)) "nil" "nil"))) ("aédef" 6 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (4 5 nil) (4 5 t)) "nil" "nil")) ("after-t-t" delete t after t ("aé中bcdef" 9 nil ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "t" "t") ((after-entry ("aédef" 6 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (4 5 nil) (4 5 t)) "t" "t")) (after-exit ("aédef" 6 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (4 5 nil) (4 5 t)) "t" "t"))) ("aédef" 6 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (4 5 nil) (4 5 t)) "t" "t")) ("before-live-markers" delete t before nil ("aé中bcdef" 9 nil ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "t" "t") ((before-entry ("aé中bcdef" 9 nil ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "t" "t")) (before-exit ("aé中bcdef" 9 nil ((4 7 nil) (2 2 nil) (3 4 nil) (3 4 t) (nil nil nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t) (5 8 t)) "nil" "nil"))) ("aédef" 6 t ((3 4 nil) (2 2 nil) (3 4 nil) (3 4 t) (nil nil nil) (3 4 t) (3 4 nil) (3 4 t) (4 5 nil) (4 5 t) (3 4 t)) "((\"中bc\" . 3) (#<marker at 3 in  *undometa-before-live-markers*> . -1) (#<marker (moves after insertion) at 3 in  *undometa-before-live-markers*> . 3) (#<marker (moves after insertion) at 3 in  *undometa-before-live-markers*> . 2) (#<marker at 3 in  *undometa-before-live-markers*> . -3) (#<marker (moves after insertion) at 3 in  *undometa-before-live-markers*> . 1) (t . 0))" "((\"中bc\" . 3) ((marker 0 3 nil) . -1) ((marker 3 3 t) . 3) ((marker 5 3 t) . 2) ((marker 6 3 nil) . -3) ((marker 10 3 t) . 1) (t . 0))")) (sequence ((initial ("aé中bcdef" 9 nil ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "nil" "nil")) (insert ("aéλ中bcdef" 4 t ((2 2 nil) (2 2 t) (3 4 nil) (4 6 t) (5 9 nil) (5 9 t) (7 11 nil) (7 11 t) (8 12 nil) (8 12 t)) "((3 . 4) (t . 0))" "((3 . 4) (t . 0))")) (delete ("aédef" 3 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (4 5 nil) (4 5 t)) "((\"λ中bc\" . 3) (#<marker (moves after insertion) at 3 in  *undometa-sequence*> . 3) (#<marker at 3 in  *undometa-sequence*> . -2) (#<marker (moves after insertion) at 3 in  *undometa-sequence*> . 2) (#<marker at 3 in  *undometa-sequence*> . -4) (3 . 4) (t . 0))" "((\"λ中bc\" . 3) ((marker 3 3 t) . 3) ((marker 4 3 nil) . -2) ((marker 5 3 t) . 2) ((marker 6 3 nil) . -4) (3 . 4) (t . 0))")) (replace ("aéΩZf" 5 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t)) "((\"de\" . -3) (5 . 7) (\"λ中bc\" . 3) (#<marker (moves after insertion) at 3 in  *undometa-sequence*> . 3) (#<marker at 3 in  *undometa-sequence*> . -2) (#<marker (moves after insertion) at 3 in  *undometa-sequence*> . 2) (#<marker at 3 in  *undometa-sequence*> . -4) (3 . 4) (t . 0))" "((\"de\" . -3) (5 . 7) (\"λ中bc\" . 3) ((marker 3 3 t) . 3) ((marker 4 3 nil) . -2) ((marker 5 3 t) . 2) ((marker 6 3 nil) . -4) (3 . 4) (t . 0))")))))"###;

const GNU_SEQUENCE: &str = r###"((initial ("aé中bcdef" 9 nil ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "nil" "nil")) (insert ("aéλ中bcdef" 4 t ((2 2 nil) (2 2 t) (3 4 nil) (4 6 t) (5 9 nil) (5 9 t) (7 11 nil) (7 11 t) (8 12 nil) (8 12 t)) "((3 . 4) (t . 0))" "((3 . 4) (t . 0))")) (delete ("aédef" 3 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (4 5 nil) (4 5 t)) "((\"λ中bc\" . 3) (#<marker (moves after insertion) at 3 in  *undometa-sequence*> . 3) (#<marker at 3 in  *undometa-sequence*> . -2) (#<marker (moves after insertion) at 3 in  *undometa-sequence*> . 2) (#<marker at 3 in  *undometa-sequence*> . -4) (3 . 4) (t . 0))" "((\"λ中bc\" . 3) ((marker 3 3 t) . 3) ((marker 4 3 nil) . -2) ((marker 5 3 t) . 2) ((marker 6 3 nil) . -4) (3 . 4) (t . 0))")) (replace ("aéΩZf" 5 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t)) "((\"de\" . -3) (5 . 7) (\"λ中bc\" . 3) (#<marker (moves after insertion) at 3 in  *undometa-sequence*> . 3) (#<marker at 3 in  *undometa-sequence*> . -2) (#<marker (moves after insertion) at 3 in  *undometa-sequence*> . 2) (#<marker at 3 in  *undometa-sequence*> . -4) (3 . 4) (t . 0))" "((\"de\" . -3) (5 . 7) (\"λ中bc\" . 3) ((marker 3 3 t) . 3) ((marker 4 3 nil) . -2) ((marker 5 3 t) . 2) ((marker 6 3 nil) . -4) (3 . 4) (t . 0))")))"###;

const GNU_DIRTY: &str = r###"((replace ("aé中bcdef" 9 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "nil" "nil") ("aéΩZdef" 5 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (5 7 nil) (5 7 t) (6 8 nil) (6 8 t)) "((\"中bc\" . -3) (6 . 8))" "((\"中bc\" . -3) (6 . 8))") 0) (replace-delete ("aé中bcdef" 9 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "nil" "nil") ("aédef" 3 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (4 5 nil) (4 5 t)) "((\"中bc\" . -3) (6 . 6))" "((\"中bc\" . -3) (6 . 6))") 0) (replace-insert ("aé中bcdef" 9 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "nil" "nil") ("aéΩZ中bcdef" 5 t ((2 2 nil) (2 2 t) (3 4 nil) (5 7 t) (6 10 nil) (6 10 t) (8 12 nil) (8 12 t) (9 13 nil) (9 13 t)) "((\"\" . -3) (3 . 5))" "((\"\" . -3) (3 . 5))") 0) (dirty-sequence ((initial ("aé中bcdef" 9 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (4 7 nil) (4 7 t) (6 9 nil) (6 9 t) (7 10 nil) (7 10 t)) "nil" "nil")) (insert ("aéλ中bcdef" 4 t ((2 2 nil) (2 2 t) (3 4 nil) (4 6 t) (5 9 nil) (5 9 t) (7 11 nil) (7 11 t) (8 12 nil) (8 12 t)) "((3 . 4))" "((3 . 4))")) (delete ("aédef" 3 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (4 5 nil) (4 5 t)) "((\"λ中bc\" . 3) (#<marker (moves after insertion) at 3 in  *undometa-dirty-sequence*> . 3) (#<marker at 3 in  *undometa-dirty-sequence*> . -2) (#<marker (moves after insertion) at 3 in  *undometa-dirty-sequence*> . 2) (#<marker at 3 in  *undometa-dirty-sequence*> . -4) (3 . 4))" "((\"λ中bc\" . 3) ((marker 3 3 t) . 3) ((marker 4 3 nil) . -2) ((marker 5 3 t) . 2) ((marker 6 3 nil) . -4) (3 . 4))")) (replace ("aéΩZf" 5 t ((2 2 nil) (2 2 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t) (3 4 nil) (3 4 t)) "((\"de\" . -3) (5 . 7) (\"λ中bc\" . 3) (#<marker (moves after insertion) at 3 in  *undometa-dirty-sequence*> . 3) (#<marker at 3 in  *undometa-dirty-sequence*> . -2) (#<marker (moves after insertion) at 3 in  *undometa-dirty-sequence*> . 2) (#<marker at 3 in  *undometa-dirty-sequence*> . -4) (3 . 4))" "((\"de\" . -3) (5 . 7) (\"λ中bc\" . 3) ((marker 3 3 t) . 3) ((marker 4 3 nil) . -2) ((marker 5 3 t) . 2) ((marker 6 3 nil) . -4) (3 . 4))"))) 0))"###;

fn assert_gnu_observation(observe: &str, expected: &str) {
    crate::test_utils::init_test_tracing();
    let form = format!(
        "(let ((print-length nil) (print-level nil) (print-circle nil)) (progn {HELPERS} {observe}))"
    );
    assert_eq!(runtime_startup_eval_one(&form), format!("OK {expected}"));
}

#[test]
fn gnu_undo_marker_matrix_matches_all_sixteen_cases() {
    // Includes clean replace/replace-delete/replace-insert, all eight public
    // before/after-hook undo toggles, live marker mutation, and the sequence.
    assert_gnu_observation(MATRIX_OBSERVE, GNU_MATRIX);
}

#[test]
fn gnu_replace_undo_compound_sequence_matches() {
    // This independent run keeps the entire insert -> delete -> replace list,
    // not just the final replacement's records or the text/marker endpoints.
    assert_gnu_observation("(undometa--sequence)", GNU_SEQUENCE);
}

#[test]
fn gnu_replace_undo_already_modified_records_no_first_change() {
    // Each replacement shape and the compound sequence start already modified
    // with an empty enabled undo list. Their captured first-change counts are0.
    let observe = format!("(progn {DIRTY_HELPERS} {DIRTY_OBSERVE})");
    assert_gnu_observation(&observe, GNU_DIRTY);
}
