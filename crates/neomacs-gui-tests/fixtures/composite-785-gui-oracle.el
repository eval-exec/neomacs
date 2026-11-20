;;; composite-785-gui-oracle.el --- paired GNU/Neo rows for issue #785 -*- lexical-binding: t; -*-

;; Issue #785: FreeSerif Arabic (JA = U+062C U+0627, lone JIM) and
;; Devanagari (STE inside NAMASTE) compositions.  The same fixture runs
;; under GNU Emacs 31.1 and Neomacs on one X11 display under a private
;; fontconfig holding exactly one Latin-only mono face (Ubuntu Mono, the
;; default) and FreeSerif (the only Arabic/Devanagari-capable fallback).
;;
;; Rows are engine-internal facts only -- no pixel literals, because the
;; two engines may resolve a different pixel size for the same face:
;;   span POS START END   -- find-composition walk over every position
;;   comp START END (FROM TO CHAR)... -- shaped gstring glyph columns
;;   sumeq LABEL T/NIL    -- (round (string-pixel-width S)) equals the
;;                           sum of its composition's gstring widths

(defvar c785-result
  (or (getenv "NEOMACS_785_ORACLE_RESULT")
      (error "NEOMACS_785_ORACLE_RESULT is unset")))

(defvar c785-text "Latin\nجا\nج\nनमस्ते\n")

(defun c785-log (line)
  (write-region (concat line "\n") nil c785-result 'append))

(defun c785-glyph-cols (gstring)
  "Glyph (FROM TO CHAR) columns of a shaped gstring.
The glyph slots start at index 2 and run to (- (length GSTRING) 2)."
  (let ((i 0) (n (- (length gstring) 2)) (out ""))
    (while (< i n)
      (let ((g (aref gstring (+ i 2))))
        (when (vectorp g)
          (setq out (format "%s (%d %d %d)" out (aref g 0) (aref g 1) (aref g 2)))))
      (setq i (1+ i)))
    out))

(defun c785-gsum (gstring)
  "Sum of the gstring's glyph WIDTH slots, GNU's composition width."
  (let ((i 0) (n (- (length gstring) 2)) (sum 0))
    (while (< i n)
      (let ((g (aref gstring (+ i 2))))
        (when (vectorp g)
          (setq sum (+ sum (aref g 4)))))
      (setq i (1+ i)))
    sum))

(defun c785-walk ()
  "Print one span row per position and one comp row per composition."
  (let ((pos 1) comp)
    (while (<= pos (point-max))
      (setq comp (find-composition pos))
      (if comp
          (progn
            (c785-log (format "span %d %d %d" pos (nth 0 comp) (nth 1 comp)))
            (when (and (= pos (nth 0 comp)) (nth 2 comp))
              (c785-log (format "comp %d %d%s"
                                (nth 0 comp) (nth 1 comp)
                                (c785-glyph-cols (nth 2 comp))))))
        (c785-log (format "span %d nil" pos)))
      (setq pos (1+ pos)))))

(defun c785-sumeq (label string pos)
  "Round-trip one string: its measured width equals its gstring sum."
  (let* ((comp (find-composition pos))
         (gsum (and comp (nth 2 comp) (c785-gsum (nth 2 comp)))))
    (c785-log (format "sumeq %s %s"
                      label
                      (if (and gsum (= (round (string-pixel-width string)) gsum))
                          "t" "nil")))))

;; One Latin-only mono face as the default; FreeSerif is then the only
;; font either engine can fall back to for Arabic and Devanagari.
;;
;; Any error is recorded as a row and still exits: an unhandled error in
;; a GUI Emacs falls back to the command loop, which would look like a
;; hung oracle instead of a row divergence.
(condition-case err
    (progn
      (set-face-attribute 'default nil :family "Ubuntu Mono")

      (write-region "" nil c785-result)
      (erase-buffer)
      (insert c785-text)
      (goto-char (point-min))

      (c785-walk)

      ;; Lone JIM is its own one-glyph composition; JA and STE are the
      ;; shaped composites of the report; NAMASTE mixes two singleton
      ;; compositions with the STE composite.
      (c785-sumeq "jim" "ج" 10)
      (c785-sumeq "ja" "جا" 7)
      (c785-sumeq "ste" "स्ते" 14)
      (let ((comp (find-composition 14))
            (full (string-pixel-width "नमस्ते"))
            (na (string-pixel-width "न"))
            (ma (string-pixel-width "म")))
        (c785-log
         (format "sumeq namaste %s"
                 (if (and comp (nth 2 comp)
                          (= (round (- full na ma))
                             (c785-gsum (nth 2 comp))))
                     "t" "nil")))))
  (error (c785-log (format "error %S" err))))

(kill-emacs 0)
