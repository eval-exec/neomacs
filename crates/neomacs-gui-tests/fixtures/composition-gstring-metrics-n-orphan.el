;;; composition-gstring-metrics-n-orphan.el --- paired GNU/Neo gstring rows -*- lexical-binding: t; -*-

;; N-orphan-combining-advance: a lone combining mark must be composed from
;; the font driver's unshaped `text_extents`, exactly as GNU
;; `fill_gstring_body`/`font_fill_lglyph_metrics` (src/composite.c,
;; src/font.c) fills it before any rule runs, and the default
;; `auto-composition-function' must be the one that calls the matched rule
;; (src/composite.c `autocmp_chars`).  `compose-gstring-for-graphic`'s
;; `nchars = 1` branch then derives the spacing adjustment from those
;; metrics; a placeholder metric changes the displayed mark advance.
;;
;; Rows are pure Lisp-visible integers (no font-object printing, no pixel
;; literals from the two engines' own APIs):
;;   size SIZE            -- resolved face font :size (identity guard)
;;   rule T/NIL           -- composition-function-table has a rule for U+0301
;;   span POS FROM TO     -- find-composition at each position
;;   glyph I [FROM TO CHAR CODE WIDTH LBEARING RBEARING ASCENT DESCENT ADJ]
;;   call FUNC FROM TO FONT STRING DIRECTION -- auto-composition-function args
;;
;; Under the pinned FreeSerif-14 face GNU's U+0301 glyph is
;; [0 0 769 708 0 -4 0 9 -7 [4 0 4]]; the adjustment is what makes the
;; lone mark a 4-pixel cell instead of a full character cell.

(defvar n-orphan-result
  (or (getenv "NEOMACS_N_ORPHAN_ORACLE_RESULT")
      (error "NEOMACS_N_ORPHAN_ORACLE_RESULT is unset")))

(defvar n-orphan-text
  (decode-coding-string (unibyte-string #xcc #x81 #x61 #x62) 'utf-8))

(defun n-orphan-log (line)
  (write-region (concat line "\n") nil n-orphan-result 'append))

(defun n-orphan-glyph-row (index glyph)
  (if (vectorp glyph)
      (format "glyph %d %S" index glyph)
    (format "glyph %d nil" index)))

(defun n-orphan-dump-composition (pos)
  (let ((comp (find-composition pos)))
    (if comp
        (progn
          (n-orphan-log (format "span %d %d %d" pos (nth 0 comp) (nth 1 comp)))
          (when (and (nth 2 comp) (= pos (nth 0 comp)))
            ;; A gstring's glyph list ends at the first nil glyph
            ;; (`composition_gstring_p`, src/composite.c); Neomacs pads its
            ;; vector with nils, GNU's vector ends at the glyph count.
            (let ((g (nth 2 comp))
                  (i 0))
              (while (and (< i (- (length g) 2))
                          (vectorp (aref g (+ i 2))))
                (n-orphan-log (n-orphan-glyph-row i (aref g (+ i 2))))
                (setq i (1+ i))))))
      (n-orphan-log (format "span %d nil" pos)))))

(condition-case err
    (progn
      (set-frame-font "FreeSerif-14" nil t)
      (setq auto-composition-mode t)
      (write-region "" nil n-orphan-result)
      (erase-buffer)
      (insert n-orphan-text)
      (goto-char (point-min))
      (redisplay t)
      (let ((face-font (face-attribute 'default :font)))
        (n-orphan-log (format "size %S"
                              (and face-font (font-get face-font :size)))))
      (n-orphan-log (format "rule %s"
                            (if (aref composition-function-table #x0301) "t" "nil")))
      ;; 1) The lone leading mark: one-char automatic composition.
      (let ((pos 1))
        (while (<= pos 4)
          (n-orphan-dump-composition pos)
          (setq pos (1+ pos))))

      ;; 5) The shaped-gstring cache contract (GNU
      ;;    `composition_gstring_put_cache'): `font-shape-gstring' shapes a
      ;;    fresh gstring and caches a COPY carrying a fixnum lgstring-id;
      ;;    the input is not the cached object.  A second fresh gstring with
      ;;    the same header returns that copy through the early header lookup
      ;;    (src/font.c:4628-4634), and `composition-get-gstring' returns it
      ;;    from its own header lookup (src/composite.c:1981-1988).  An ASCII
      ;;    string gstring keeps the driver path deterministic.
      (let* ((font (face-attribute 'default :font))
             (proto (composition-get-gstring 0 1 font "x"))
             (cold-a (copy-sequence proto))
             (cold-b (copy-sequence proto))
             (shaped-a (font-shape-gstring cold-a nil))
             (shaped-b (font-shape-gstring cold-b nil))
             (first (composition-get-gstring 0 1 font "x"))
             (second (composition-get-gstring 0 1 font "x")))
        (n-orphan-log
         (format "cache-shape eq %s id-fixnum %s eq-source %s eq-get %s length %d"
                 (eq shaped-a shaped-b)
                 (and (fixnump (aref shaped-a 1)) t)
                 (eq shaped-a cold-a)
                 (eq first shaped-a)
                 (length shaped-a)))
        (n-orphan-log
         (format "cache-get eq %s id-fixnum %s"
                 (eq first second)
                 (and (fixnump (aref first 1)) t))))

      ;; 3) The rule-adjusted advance reaches the display: the composition
      ;;    width of the lone mark, the pen after it, and the x of the
      ;;    following character must equal GNU's gstring width (WADJUST when
      ;;    the rule stored an adjustment), not a full character cell.
      (n-orphan-log
       (format "wtps-mark %S" (car (window-text-pixel-size nil 1 2))))
      (n-orphan-log
       (format "wtps-mark-a %S" (car (window-text-pixel-size nil 1 3))))
      (n-orphan-log
       (format "posn-a %S" (car (posn-x-y (posn-at-point 2)))))

      ;; 2) The matched rule's FUNC must reach the buffer-local
      ;;    `auto-composition-function' (GNU autocmp_chars' first argument).
      ;;    U+0300 (not U+0301 above) keeps the shaped-gstring cache cold, so
      ;;    autocmp_chars really runs the rule instead of reusing a cached
      ;;    shaped gstring.
      (erase-buffer)
      (insert (decode-coding-string (unibyte-string #xcc #x80 #x61 #x62) 'utf-8))
      (goto-char (point-min))
      (let ((n-orphan-call nil))
        (setq-local auto-composition-function
                    (lambda (func from to font-object string direction)
                      (setq n-orphan-call
                            (list func from to
                                  (and font-object
                                       (fontp font-object 'font-object))
                                  (null string)
                                  direction))
                      (auto-compose-chars
                       func from to font-object string direction)))
        (find-composition 1)
        (n-orphan-log (format "call %S" n-orphan-call)))

      ;; 4) GNU autocmp_chars saves the match data around the call out to
      ;;    Lisp (`record_unwind_save_match_data`, src/composite.c:941), so a
      ;;    rule FUNC that runs a search cannot leak its match data to the
      ;;    caller.  U+0302 keeps the gstring cache cold so the rule really
      ;;    runs; the lambda clobbers the match data and then delegates.
      (erase-buffer)
      (insert (decode-coding-string (unibyte-string #xcc #x82 #x61 #x62) 'utf-8))
      (goto-char (point-min))
      (let ((saved (progn (string-match "b\\(c\\)" "abc") (match-data t)))
            (calls 0)
            (comp nil))
        (setq-local auto-composition-function
                    (lambda (func from to font-object string direction)
                      (setq calls (1+ calls))
                      (string-match "x\\(y\\)" "xyz")
                      (auto-compose-chars
                       func from to font-object string direction)))
        (setq comp (and (find-composition 1) t))
        (n-orphan-log (format "matchdata-calls %d comp %S" calls comp))
        (n-orphan-log (format "matchdata-after %S" (match-data t)))
        (n-orphan-log (format "matchdata-saved %S" saved))
        (n-orphan-log (format "matchdata-restored %s"
                              (if (equal (match-data t) saved) "t" "nil"))))

      ;; 6) Once a shaped gstring is cached, a later composition search must
      ;;    not call `auto-composition-function' again: GNU `autocmp_chars'
      ;;    skips the rule for a gstring whose ID is non-nil
      ;;    (src/composite.c:973).  A base + combining-mark sequence takes
      ;;    `compose-gstring-for-graphic`'s default branch, which calls
      ;;    `font-shape-gstring`, so the first search caches the shaped
      ;;    gstring (src/font.c:4685 composition_gstring_put_cache); the lone
      ;;    mark above only stores an adjustment and does not cache on this
      ;;    path.  The second search must then report zero rule calls.
      (erase-buffer)
      (insert (decode-coding-string
               (unibyte-string #x65 #xcc #x81 #x62) 'utf-8))
      (goto-char (point-min))
      (let ((calls 0))
        (setq-local auto-composition-function
                    (lambda (func from to font-object string direction)
                      (setq calls (1+ calls))
                      (auto-compose-chars
                       func from to font-object string direction)))
        (let ((comp (find-composition 1)))
          (n-orphan-log
           (format "cache-calls-cold %d span %S"
                   (if (> calls 0) 1 0) (and comp t))))
        (setq calls 0)
        (find-composition 1)
        (n-orphan-log (format "cache-calls-reshaped-search %d" calls))))
  (error (n-orphan-log (format "error %S" err))))

(kill-emacs 0)
