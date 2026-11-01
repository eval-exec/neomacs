;;; soak_common.el --- shared soak receipt marks -*- lexical-binding: t; -*-
;;
;; Loaded by every soak row. The driver records the SOAK-GC boundary marks
;; (one full collection at start and end; heap bytes = retained bytes from the
;; garbage-collect stats buckets, whose (name size used [free]) shape is the
;; same in GNU and Neomacs), the SOAK-PROGRESS minute marks (gcs-done total
;; plus the cumulative memory-use-counts allocation counters), and -- on
;; Neomacs -- the non-forcing heap-layout census: managed live bytes, page
;; backing bytes, allocated objects and live thread objects.  The census is
;; what tells a growth row WHICH memory grows: native RSS climbing with a flat
;; live heap is a native leak; live heap and thread objects climbing with RSS
;; is a GC-trigger divergence (thread objects not charged to the trigger).
;;
;; Neomacs exposes only total `gcs-done' -- no minor/major split accessor --
;; so the receipts record the total cycle count, which is the finest split the
;; product reports today.

(defvar soak-last-heap-kib nil
  "Most recent retained-heap KiB from a full `garbage-collect'.")

(defun soak-heap-kib-from (stats)
  (let ((sum 0))
    (dolist (bucket stats)
      (when (and (consp bucket)
                 (numberp (nth 1 bucket))
                 (numberp (nth 2 bucket)))
        (setq sum (+ sum (* (nth 1 bucket) (nth 2 bucket))))))
    (/ sum 1024)))

(defun soak-use-string ()
  (if (fboundp 'memory-use-counts)
      (mapconcat #'number-to-string (memory-use-counts) ",")
    "nil"))

(defun soak-heap-census ()
  "Return the Neomacs heap-layout census as key=value tokens, or nil.
Non-forcing: reads the allocator's live accounting, never collects.  GNU has
no such function, so its rows simply omit the census fields."
  (when (fboundp 'neomacs--heap-layout-stats)
    (condition-case nil
        (let* ((stats (neomacs--heap-layout-stats))
               (boxed (cdr (assq 'boxed stats)))
               (thread (cdr (assq 'thread boxed))))
          (format "live=%s backing=%s objs=%s thread-objs=%s"
                  (cdr (assq 'managed-live-bytes stats))
                  (cdr (assq 'page-backing-bytes stats))
                  (cdr (assq 'allocated-objects stats))
                  (cdr (assq 'objects thread))))
      (error nil))))

(defun soak-census-suffix ()
  (let ((census (soak-heap-census)))
    (if census (concat " " census) "")))

(defun soak-gc-mark (when)
  (setq soak-last-heap-kib (soak-heap-kib-from (garbage-collect)))
  (let ((gcs (if (boundp 'gcs-done) (symbol-value 'gcs-done) 0)))
    (princ (format "SOAK-GC t=%s gcs=%d heap=%d use=%s%s\n"
                   when gcs soak-last-heap-kib (soak-use-string)
                   (soak-census-suffix)))))

(defun soak-progress (seconds heap)
  (let ((gcs (if (boundp 'gcs-done) (symbol-value 'gcs-done) 0)))
    (princ (format "SOAK-PROGRESS t=%d gcs=%d use=%s%s%s\n"
                   seconds gcs (soak-use-string)
                   (if heap (format " heap=%d" heap) "")
                   (soak-census-suffix)))))

(defun soak-force-gc-p ()
  (let ((value (getenv "SOAK_FORCE_GC")))
    (and value (not (string= value "0")))))
