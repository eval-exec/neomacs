;;; font-info-cache-oracle.el --- live font-info cache staleness GUI oracle -*- lexical-binding: t -*-

;; Runs identically under GNU Emacs 31.1 and Neomacs on a window-system frame.
;;
;; Single-phase mode (GNU Emacs reference, NEOMACS_GUI_FONT_CACHE_PHASE2_CONF unset):
;;   Probes the active FONTCONFIG_FILE's "monospace" and "Noto Sans" families
;;   (repeat queries, vector/capability mutation isolation, distinct sizes,
;;   and absent family) and writes the result to NEOMACS_GUI_FONT_CACHE_RESULT.
;;
;; Two-phase live-staleness mode (Neomacs, NEOMACS_GUI_FONT_CACHE_PHASE2_CONF set):
;;   1. Phase 1 warms and hits FontResolver's positive caches ("monospace-12"
;;      x 10 -> DejaVu Sans Mono) and negative cache ("Noto Sans-12" x 5 -> nil)
;;      and writes NEOMACS_GUI_FONT_CACHE_RESULT.
;;   2. Overwrites NEOMACS_GUI_FONT_CACHE_LIVE_CONF with PHASE2_CONF (installing
;;      Noto Sans and switching monospace -> Noto Sans), stamps its mtime to
;;      `(current-time)', immediately calls `(clear-font-cache)' WITHOUT
;;      sleeping (triggering `FcInitReinitialize' and invalidating all caches),
;;      and runs Phase 2.
;;   3. Writes Phase 2 to NEOMACS_GUI_FONT_CACHE_RESULT_PHASE2.
;;      If any positive or negative cache returns a stale Phase 1 answer,
;;      Phase 2 diverges from GNU Emacs 31.1's Phase 2 oracle and fails.

(defun neomacs-font-cache-oracle-probe ()
  (let* ((mono-12 "monospace-12")
         (mono-20 "monospace-20")
         (noto-12 "Noto Sans-12")
         (first (font-info mono-12)))
    (unless (and (vectorp first) (> (aref first 2) 0) (> (aref first 3) 0))
      (error "Expected openable font for %s, got %S" mono-12 first))
    (let* ((first-metrics (seq-subseq first 2 12))
           (repeat-all-equal t))
      ;; Warm and exercise positive cache hits 10 times.
      (dotimes (_ 10)
        (let ((again (font-info mono-12)))
          (unless (equal (seq-subseq again 2 12) first-metrics)
            (setq repeat-all-equal nil))))
      ;; Warm and exercise negative (Phase 1) / newly installed (Phase 2) path.
      (let* ((noto-first (font-info noto-12))
             (noto-metrics (and (vectorp noto-first) (seq-subseq noto-first 2 12)))
             (noto-repeat-equal t))
        (dotimes (_ 5)
          (let* ((again (font-info noto-12))
                 (m (and (vectorp again) (seq-subseq again 2 12))))
            (unless (equal m noto-metrics)
              (setq noto-repeat-equal nil))))
        ;; Mutate returned vector slot 0 and capability cons; subsequent calls
        ;; must return fresh, unmutated values.
        (let* ((mut (font-info mono-12))
               (orig-cap (copy-tree (aref mut 13))))
          (aset mut 0 "neomacs-mutated-slot-0")
          (when (consp (aref mut 13))
            (setcar (aref mut 13) 'neomacs-mutated-cap))
          (let* ((after-mut (font-info mono-12))
                 (slot0-isolated (not (equal (aref after-mut 0) "neomacs-mutated-slot-0")))
                 (cap-isolated (equal (aref after-mut 13) orig-cap))
                 (info-20 (font-info mono-20))
                 (metrics-20 (and (vectorp info-20) (seq-subseq info-20 2 12))))
            (list :mono-12 first-metrics
                  :mono-repeat-equal repeat-all-equal
                  :noto-12 noto-metrics
                  :noto-repeat-equal noto-repeat-equal
                  :slot0-isolated slot0-isolated
                  :cap-isolated cap-isolated
                  :mono-20 metrics-20
                  :size-distinct (not (equal first-metrics metrics-20)))))))))

(defun neomacs-font-cache-oracle-write (path value)
  (make-directory (file-name-directory path) t)
  (with-temp-file path
    (prin1 value (current-buffer))
    (insert "\n")))

(let ((out1 (getenv "NEOMACS_GUI_FONT_CACHE_RESULT"))
      (out2 (getenv "NEOMACS_GUI_FONT_CACHE_RESULT_PHASE2"))
      (live-conf (getenv "NEOMACS_GUI_FONT_CACHE_LIVE_CONF"))
      (phase2-conf (getenv "NEOMACS_GUI_FONT_CACHE_PHASE2_CONF"))
      (exit-code 0))
  (condition-case err
      (progn
        (when out1
          (neomacs-font-cache-oracle-write out1 (neomacs-font-cache-oracle-probe)))
        (when (and out2 live-conf phase2-conf)
          ;; Mutate the live Fontconfig file on disk while the editor is running
          ;; and immediately call (clear-font-cache) with zero sleep.
          (copy-file phase2-conf live-conf t)
          (set-file-times live-conf (current-time))
          (clear-font-cache)
          (neomacs-font-cache-oracle-write out2 (neomacs-font-cache-oracle-probe))))
    (error
     (setq exit-code 1)
     (when out1
       (neomacs-font-cache-oracle-write out1 (list :error (error-message-string err))))))
  (run-at-time 1 nil (lambda () (kill-emacs exit-code))))

;;; font-info-cache-oracle.el ends here
