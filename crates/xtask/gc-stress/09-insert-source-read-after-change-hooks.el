;;; 09-insert-source-read-after-change-hooks.el --- late reads, under GC -*- lexical-binding: t -*-
;;; expect: ("Zbcdefgh" "aYcdefgh" bold nil "abcdefgh" "abcdefgh")

;; DIVERGENCES.md 164.  Probe 07 guards the memory-safety half of this seam
;; and deliberately mutates a DIFFERENT string, because when it was written
;; `insert' snapshotted its argument before `before-change-functions' and
;; pinning the GNU answer here would have failed for a reason 07 was not
;; testing.  164 landed that fix, so this probe pins the other half: the
;; source string is now read AFTER the hook, exactly as GNU's
;; `insert_from_string_1' does (`prepare_to_modify_buffer' at
;; src/insdel.c:1043 sits between the caller's SCHARS/SBYTES snapshot and
;; both `copy_text' at :1053 and `string_intervals (string)' at :1093).
;;
;; The two halves have to be pinned TOGETHER, and that is the whole point of
;; putting it here rather than in a unit test.  Reading the source late is
;; only safe if the source survives the hook, and the hook is arbitrary Lisp:
;; it can cons until the collector runs and, in `NEOVM_GC_STRESS=1', does so
;; on every allocation.  Each form below therefore conses hard *and* mutates
;; the very string the insert is holding.  A regression that drops the
;; specpdl root on `PendingInsert::Str' shows up here as a reclaimed string,
;; not as a wrong answer somewhere else three releases later.
;;
;; GNU is safe doing this for three reasons, all of which the port now
;; reproduces by construction rather than by copying early:
;;   - `string' is a rooted `Lisp_Object' scanned by `mark_stack';
;;   - `SDATA' re-reads `u.s.data', so a relocating GC
;;     (`compact_small_strings', src/alloc.c) is invisible to the caller;
;;   - `Faset' on a string (src/data.c:2658-2681) is length-preserving in
;;     chars AND bytes, so the pre-hook length cannot go stale.

(defvar gc-stress-sink nil)

(defun gc-stress-churn (&rest _)
  (setq gc-stress-sink (make-list 256 'churn))
  nil)

(prin1
 (list
  ;; 1. the hook conses hard and then mutates the string being inserted
  (let ((s (copy-sequence "abcdefgh")))
    (with-temp-buffer
      (add-hook 'before-change-functions
                (lambda (&rest _) (gc-stress-churn) (aset s 0 ?Z))
                nil t)
      (insert s)
      (buffer-string)))
  ;; 2. same, through insert-before-markers
  (let ((s (copy-sequence "abcdefgh")))
    (with-temp-buffer
      (add-hook 'before-change-functions
                (lambda (&rest _) (gc-stress-churn) (aset s 1 ?Y))
                nil t)
      (insert-before-markers s)
      (buffer-string)))
  ;; 3. the hook propertizes the string, so the interval plists it conses are
  ;; younger than the insert that has to graft them into the buffer
  (let ((s (copy-sequence "abcdefgh")))
    (with-temp-buffer
      (add-hook 'before-change-functions
                (lambda (&rest _)
                  (gc-stress-churn)
                  (put-text-property 0 3 'face (intern (concat "bo" "ld")) s))
                nil t)
      (insert s)
      (get-text-property 1 'face)))
  ;; 4. the hook strips properties the insert had already seen
  (let ((s (propertize (copy-sequence "abcdefgh") 'face 'italic)))
    (with-temp-buffer
      (add-hook 'before-change-functions
                (lambda (&rest _) (gc-stress-churn) (set-text-properties 0 8 nil s))
                nil t)
      (insert s)
      (get-text-property 1 'face)))
  ;; 5. insert-and-inherit shares general_insert_function
  (let ((s (copy-sequence "abcdefgh")))
    (with-temp-buffer
      (add-hook 'before-change-functions (lambda (&rest _) (gc-stress-churn)) nil t)
      (insert-and-inherit s)
      (buffer-string)))
  ;; 6. control: a hook that only conses must not disturb the text
  (let ((s (copy-sequence "abcdefgh")))
    (with-temp-buffer
      (add-hook 'before-change-functions (lambda (&rest _) (gc-stress-churn)) nil t)
      (insert s)
      (buffer-string)))))
(terpri)
