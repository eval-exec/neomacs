use crate::fuzz_support::{
    RegexCase, RegexCheck, RegexDifferential, SearchTarget, check_regex_differential,
};

use neovm_text::emacs_core::regex_emacs::*;

#[test]
fn prepared_search_context_preserves_marker_bounds_narrowing_and_zero_counts() {
    crate::test_utils::init_test_tracing();
    let observed = crate::test_utils::runtime_startup_eval_one(
        r#"(progn
  (require 'syntax)
  (let (out)
    (dolist (text '("xabcbcz" "xa中b中bz"))
      (dolist (narrow '(nil t))
        (dolist (marker-bound '(nil t))
          (dolist (case '(("b" 1) ("b" 2) ("b" -1)
                          ("b" -2) ("" 0) ("z" 1)))
            (with-temp-buffer
              (insert text)
              (when narrow (narrow-to-region 2 7))
              (let* ((case-fold-search nil)
                     (count (nth 1 case))
                     (bound-pos (if (> count 0) (point-max) (point-min)))
                     (bound (if marker-bound (copy-marker bound-pos) bound-pos)))
                (goto-char (if (> count 0) (point-min) (point-max)))
                (set-match-data '(7 9))
                (push (list (condition-case err
                                (re-search-forward (car case) bound t count)
                              (error (car err)))
                            (point)
                            (mapcar (lambda (v) (if (bufferp v) 'buffer v))
                                    (match-data t))) out)))))))
    ;; Covered properties take a callback-free early return from preparation;
    ;; the same pattern is also exercised with callback-capable preparation.
    (dolist (covered '(nil t))
      (with-temp-buffer
        (insert "ab cd")
        (let ((parse-sexp-lookup-properties t)
              (case-fold-search nil)
              (calls 0)
              (bound (copy-marker 6)))
          (setq-local syntax-propertize-function
                      (lambda (_start _end)
                        (setq calls (1+ calls))
                        (goto-char 4)
                        (garbage-collect)))
          (setq-local syntax-propertize--done (if covered 6 1))
          (goto-char 1)
          (push (list (re-search-forward "\\w+" bound t) (point) calls
                      (mapcar (lambda (v) (if (bufferp v) 'buffer v))
                              (match-data t))) out))))
    (nreverse out)))"#,
    );
    // GNU Emacs 31.1: 48 combinations of text encoding, narrowing, marker
    // bounds and search count, plus covered/callback-capable syntax setup.
    assert_eq!(
        observed,
        concat!(
            "OK ((4 4 (3 4 buffer)) (6 6 (5 6 buffer)) (5 5 (5 6 buffer)) (3 3 (3 4 buffer)) (8 8 (8 8 ",
            "buffer)) (8 8 (7 8 buffer)) (4 4 (3 4 buffer)) (6 6 (5 6 buffer)) (5 5 (5 6 buffer)) (3 3 (3 4 ",
            "buffer)) (8 8 (8 8 buffer)) (8 8 (7 8 buffer)) (4 4 (3 4 buffer)) (6 6 (5 6 buffer)) (5 5 (5 6 ",
            "buffer)) (3 3 (3 4 buffer)) (7 7 (7 7 buffer)) (nil 2 (7 9)) (4 4 (3 4 buffer)) (6 6 (5 6 ",
            "buffer)) (5 5 (5 6 buffer)) (3 3 (3 4 buffer)) (7 7 (7 7 buffer)) (nil 2 (7 9)) (5 5 (4 5 ",
            "buffer)) (7 7 (6 7 buffer)) (6 6 (6 7 buffer)) (4 4 (4 5 buffer)) (8 8 (8 8 buffer)) (8 8 (7 8 ",
            "buffer)) (5 5 (4 5 buffer)) (7 7 (6 7 buffer)) (6 6 (6 7 buffer)) (4 4 (4 5 buffer)) (8 8 (8 8 ",
            "buffer)) (8 8 (7 8 buffer)) (5 5 (4 5 buffer)) (7 7 (6 7 buffer)) (6 6 (6 7 buffer)) (4 4 (4 5 ",
            "buffer)) (7 7 (7 7 buffer)) (nil 2 (7 9)) (5 5 (4 5 buffer)) (7 7 (6 7 buffer)) (6 6 (6 7 ",
            "buffer)) (4 4 (4 5 buffer)) (7 7 (7 7 buffer)) (nil 2 (7 9)) (3 3 1 (1 3 buffer)) (3 3 0 (1 3 ",
            "buffer)))",
        )
    );
}

#[test]
fn prepared_regexp_scoped_gc_preserves_compiled_and_callback_paths() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    // Load syntax support and exercise autoloads before stress: collecting
    // at every allocation during charprop.el loading tests bootstrap cost,
    // not the lifetime of a pattern passed through search preparation.
    eval.gc_stress = false;
    let warm = eval.eval_str(
        r#"(progn
  (require 'syntax)
  (defun prepared-regexp-scoped-gc ()
    (let (out)
      (dolist (lookup '(nil t))
        (with-temp-buffer
          (insert "ab cd")
          (let ((parse-sexp-lookup-properties lookup)
                (case-fold-search nil)
                (calls 0))
            (setq-local syntax-propertize-function
                        (lambda (_start _end)
                          (setq calls (1+ calls))
                          (put-text-property 1 3 'syntax-table (string-to-syntax "."))
                          (garbage-collect)))
            (setq-local syntax-propertize--done 1)
            (goto-char 1)
            (push (list (re-search-forward "b" 6 t) (> calls 0)) out)
            (goto-char 1)
            (push (list (re-search-forward "\\w" 6 t) (> calls 0)) out))))
      (nreverse out)))
  (prepared-regexp-scoped-gc))"#,
    );
    let expected = "OK ((3 nil) (2 nil) (3 nil) (5 t))";
    assert_eq!(crate::emacs_core::format_eval_result(&warm), expected);
    eval.gc_stress = true;
    let before = eval.gc_count;
    let result = eval.eval_str("(prepared-regexp-scoped-gc)");
    let collections = eval.gc_count - before;
    assert_eq!(crate::emacs_core::format_eval_result(&result), expected);
    // One explicit callback collection alone must not satisfy the test.
    assert!(collections > 1, "expected automatic stress collections");
    eprintln!("prepared-regexp scoped stress collections: {collections}");
}

#[test]
fn prepared_regexp_preserves_options_and_gc_during_syntax_propertize() {
    crate::test_utils::init_test_tracing();
    let observed = crate::test_utils::runtime_startup_eval_one(
        r#"(progn
  (require 'syntax)
(let (out)
  (dolist (lookup '(nil t))
    (dolist (case '(("b" 1 6 1 nil)
                    ("B" 1 6 1 t)
                    ("[bc]" 1 6 1 nil)
                    ("\\w" 1 6 1 nil)
                    ("\\w+" 1 6 1 nil)
                    ("\\(\\w\\)" 6 1 -1 nil)
                    ("\\w" 3 6 0 nil)
                    ("\\w" 3 3 0 nil)
                    ("z" 1 5 1 nil)))
      (dolist (noerror '(nil t move))
        (with-temp-buffer
          (insert "ab cd")
          (let ((parse-sexp-lookup-properties lookup)
                (case-fold-search (nth 4 case))
                (calls 0))
            (setq-local syntax-propertize-function
                        (lambda (start end)
                          (setq calls (1+ calls))
                          (put-text-property 1 3 'syntax-table (string-to-syntax "."))
                          (garbage-collect)))
            (setq-local syntax-propertize--done 1)
            (goto-char (nth 1 case))
            (set-match-data '(7 9))
            (let ((answer
                   (condition-case error
                       (re-search-forward (car case) (nth 2 case) noerror (nth 3 case))
                     (error (car error)))))
              (push (list answer (point) (> calls 0)
                          (mapcar (lambda (value) (if (bufferp value) 'buffer value))
                                  (match-data t))) out)))))))
  (nreverse out)))"#,
    );
    // GNU Emacs 31.1: plain/translated/captured searches, reverse counts,
    // zero counts, all noerror modes and syntax properties on/off. The
    // propertize callback allocates and collects, so prepared patterns must
    // not bypass the callback or retain a borrowed Lisp string across it.
    assert_eq!(
        observed,
        concat!(
            "OK ((3 3 nil (2 3 buffer)) (3 3 nil (2 3 buffer)) (3 3 nil (2 3 buffer)) (3 3 nil (2 3 buffer)) (3 3",
            " nil (2 3 buffer)) (3 3 nil (2 3 buffer)) (3 3 nil (2 3 buffer)) (3 3 nil (2 3 buffer)) (3 3 nil (2 ",
            "3 buffer)) (2 2 nil (1 2 buffer)) (2 2 nil (1 2 buffer)) (2 2 nil (1 2 buffer)) (3 3 nil (1 3 buffer",
            ")) (3 3 nil (1 3 buffer)) (3 3 nil (1 3 buffer)) (5 5 nil (5 6 5 6 buffer)) (5 5 nil (5 6 5 6 buffer",
            ")) (5 5 nil (5 6 5 6 buffer)) (error 3 nil (7 9)) (error 3 nil (7 9)) (error 3 nil (7 9)) (3 3 nil (",
            "3 3 buffer)) (3 3 nil (3 3 buffer)) (3 3 nil (3 3 buffer)) (search-failed 1 nil (7 9)) (nil 1 nil (7",
            " 9)) (nil 5 nil (7 9)) (3 3 nil (2 3 buffer)) (3 3 nil (2 3 buffer)) (3 3 nil (2 3 buffer)) (3 3 nil",
            " (2 3 buffer)) (3 3 nil (2 3 buffer)) (3 3 nil (2 3 buffer)) (3 3 nil (2 3 buffer)) (3 3 nil (2 3 bu",
            "ffer)) (3 3 nil (2 3 buffer)) (5 5 t (4 5 buffer)) (5 5 t (4 5 buffer)) (5 5 t (4 5 buffer)) (6 6 t ",
            "(4 6 buffer)) (6 6 t (4 6 buffer)) (6 6 t (4 6 buffer)) (5 5 t (5 6 5 6 buffer)) (5 5 t (5 6 5 6 buf",
            "fer)) (5 5 t (5 6 5 6 buffer)) (error 3 nil (7 9)) (error 3 nil (7 9)) (error 3 nil (7 9)) (3 3 nil ",
            "(3 3 buffer)) (3 3 nil (3 3 buffer)) (3 3 nil (3 3 buffer)) (search-failed 1 nil (7 9)) (nil 1 nil (",
            "7 9)) (nil 5 nil (7 9)))",
        )
    );
}

#[test]
fn bounded_search_view_preserves_gnu_boundaries() {
    crate::test_utils::init_test_tracing();
    let observed = crate::test_utils::runtime_startup_eval_one(
        r#"(let (out)
  ;; TEXT PATTERN START BOUND FOLD NARROW POSIX
  (dolist (case '(("aaTAIL" "a$" 1 3 nil nil nil)
                  ("aa\nTAIL" "a$" 1 3 nil nil nil)
                  ("aaTAIL" "a\\'" 1 3 nil nil nil)
                  ("aaTAIL" "a\\b" 1 3 nil nil nil)
                  ("aaTAIL" "a\\B" 1 3 nil nil nil)
                  ("aa TAIL" "a\\b" 1 3 nil nil nil)
                  ("aa_TAIL" "a\\_>" 1 3 nil nil nil)
                  ("aa TAIL" "a\\_>" 1 3 nil nil nil)
                  ("aaTAIL" "a\\>" 1 3 nil nil nil)
                  ("aa TAIL" "a\\>" 1 3 nil nil nil)
                  ("aaTAIL" "\\b" 3 3 nil nil nil)
                  ("aaTAIL" "\\B" 3 3 nil nil nil)
                  ("aa\nTAIL" "^\\(\\)" 2 4 nil nil nil)
                  ("aa\nTAIL" "^T" 2 4 nil nil nil)
                  ("aa\nTAIL" "^T" 2 5 nil nil nil)
                  ("é中日tail" "中\\b" 1 3 nil nil nil)
                  ("é中日tail" "中\\'" 1 3 nil nil nil)
                  ("é中日tail" "中$" 1 3 nil nil nil)
                  ("é中\ntail" "中$" 1 3 nil nil nil)
                  ("é中 tail" "中\\b" 1 3 nil nil nil)
                  ("é中 tail" "\\(é中\\)" 1 3 nil nil nil)
                  ("é中 tail" "\\(é中\\)" 1 2 nil nil nil)
                  ("AaTAIL" "^aa" 1 3 t nil nil)
                  ("AaTAIL" "^aa$" 1 3 t nil nil)
                  ("aaTAIL" "\\(a\\)\\1" 1 3 nil nil nil)
                  ("aaTAIL" "\\(a\\)\\1$" 1 3 nil nil nil)
                  ("aaTAIL" "\\(a\\|aa\\)" 1 3 nil nil t)
                  ("aaTAIL" "\\(a\\|aa\\)" 1 2 nil nil t)
                  ("aaTAIL" "a\\'" 1 3 nil (1 3) nil)
                  ("aaTAIL" "a$" 1 3 nil (1 3) nil)
                  ("aaTAIL" "a\\b" 1 3 nil (1 3) nil)
                  ("aaTAIL" "a\\B" 1 3 nil (1 3) nil)
                  ("xxaaTAIL" "\\`aa" 3 5 nil (3 8) nil)
                  ("xxaaTAIL" "aa\\'" 3 5 nil (3 5) nil)
                  ("xxaaTAIL" "aa\\'" 3 5 nil (3 8) nil)
                  ("" "\\'" 1 1 nil nil nil)))
    (with-temp-buffer
      (insert (nth 0 case))
      ;; Park the gap after the tested prefix, before applying narrowing.
      (goto-char (max 1 (- (point-max) 2)))
      (insert "x") (delete-char -1)
      (when (nth 5 case) (apply #'narrow-to-region (nth 5 case)))
      (goto-char (nth 2 case))
      (set-match-data '(7 9))
      (let* ((case-fold-search (nth 4 case))
             (answer (funcall (if (nth 6 case) #'posix-search-forward
                               #'re-search-forward)
                              (nth 1 case) (nth 3 case) t)))
        (push (list answer (point)
                    (mapcar (lambda (value) (if (bufferp value) 'buffer value))
                            (match-data t))) out))))
  (nreverse out))"#,
    );
    // Checked row by row against GNU Emacs 31.1.
    assert_eq!(
        observed,
        concat!(
            "OK ((nil 1 (7 9)) (3 3 (2 3 buffer)) (nil 1 (7 9)) (nil 1 (7 9)) (2 2 (1 2 buffer)) (3 3 (2 3 ",
            "buffer)) (nil 1 (7 9)) (3 3 (2 3 buffer)) (nil 1 (7 9)) (3 3 (2 3 buffer)) (nil 3 (7 9)) (3 3 (3 3 ",
            "buffer)) (4 4 (4 4 4 4 buffer)) (nil 2 (7 9)) (5 5 (4 5 buffer)) (nil 1 (7 9)) (nil 1 (7 9)) (nil 1 ",
            "(7 9)) (3 3 (2 3 buffer)) (3 3 (2 3 buffer)) (3 3 (1 3 1 3 buffer)) (nil 1 (7 9)) (3 3 (1 3 buffer))",
            " (nil 1 (7 9)) (3 3 (1 3 1 2 buffer)) (nil 1 (7 9)) (3 3 (1 3 1 3 buffer)) (2 2 (1 2 1 2 buffer)) (3",
            " 3 (2 3 buffer)) (3 3 (2 3 buffer)) (3 3 (2 3 buffer)) (2 2 (1 2 buffer)) (5 5 (3 5 buffer)) (5 5 (3",
            " 5 buffer)) (nil 3 (7 9)) (1 1 (1 1 buffer)))",
        )
    );
}

#[test]
fn bounded_search_view_preserves_gnu_raw_bytes_and_syntax() {
    crate::test_utils::init_test_tracing();
    let observed = crate::test_utils::runtime_startup_eval_one(
        r#"(let (out)
  (dolist (multibyte '(nil t))
    (with-temp-buffer
      (set-buffer-multibyte multibyte)
      (set-syntax-table (copy-syntax-table))
      (modify-syntax-entry 128 ".")
      (modify-syntax-entry #x3fff80 ".")
      (insert (unibyte-string ?a 128 ?b ?T ?A ?I ?L))
      (dolist (pattern (list "a$" "a\\'" "a\\b" "a\\B" "a." ".\\'"
                            (concat "a" (unibyte-string 128))))
        (dolist (bound '(2 3))
          (goto-char 6) (insert "x") (delete-char -1)
          (goto-char 1)
          (set-match-data '(7 9))
          (let* ((case-fold-search nil)
                 (answer (re-search-forward pattern bound t)))
            (push (list answer (point) (match-beginning 0) (match-end 0)) out))))))
  (with-temp-buffer
    (set-syntax-table (copy-syntax-table))
    (modify-syntax-entry #x200000 "w")
    (insert (string ?a #x200000 ?b ?T ?A ?I ?L))
    (dolist (pattern '("a$" "a\\'" "a." "a.\\'" "a\\b" "a\\B"))
      (dolist (bound '(2 3))
        (goto-char 6) (insert "x") (delete-char -1)
        (goto-char 1)
        (set-match-data '(7 9))
        (let* ((case-fold-search nil)
               (answer (re-search-forward pattern bound t)))
          (push (list answer (point) (match-beginning 0) (match-end 0)) out)))))
  (with-temp-buffer
    (insert "aXTAIL")
    (put-text-property 2 3 'syntax-table (string-to-syntax "."))
    (dolist (lookup '(nil t))
      (dolist (pattern '("a\\b" "a\\B" "a\\>" "a\\_>"))
        (goto-char 5) (insert "x") (delete-char -1)
        (goto-char 1)
        (set-match-data '(7 9))
        (let* ((case-fold-search nil)
               (parse-sexp-lookup-properties lookup)
               (answer (re-search-forward pattern 2 t)))
          (push (list answer (point) (match-beginning 0) (match-end 0)) out)))))
  (nreverse out))"#,
    );
    // Checked row by row against GNU Emacs 31.1.
    assert_eq!(
        observed,
        concat!(
            "OK ((nil 1 7 9) (nil 1 7 9) (nil 1 7 9) (nil 1 7 9) (2 2 1 2) (2 2 1 2) (nil 1 7 9) (nil 1 7 9) (nil",
            " 1 7 9) (3 3 1 3) (nil 1 7 9) (nil 1 7 9) (nil 1 7 9) (3 3 1 3) (nil 1 7 9) (nil 1 7 9) (nil 1 7 9) ",
            "(nil 1 7 9) (2 2 1 2) (2 2 1 2) (nil 1 7 9) (nil 1 7 9) (nil 1 7 9) (3 3 1 3) (nil 1 7 9) (nil 1 7 ",
            "9) (nil 1 7 9) (3 3 1 3) (nil 1 7 9) (nil 1 7 9) (nil 1 7 9) (nil 1 7 9) (nil 1 7 9) (3 3 1 3) (nil ",
            "1 7 9) (nil 1 7 9) (2 2 1 2) (2 2 1 2) (nil 1 7 9) (nil 1 7 9) (nil 1 7 9) (2 2 1 2) (nil 1 7 9) ",
            "(nil 1 7 9) (2 2 1 2) (nil 1 7 9) (2 2 1 2) (2 2 1 2))",
        )
    );
}

#[test]
fn bounded_search_view_leaves_a_distant_gap_in_place() {
    crate::test_utils::init_test_tracing();
    let observed = crate::test_utils::runtime_startup_eval_one(
        r#"(with-temp-buffer
             (insert (make-string 4000 ?a))
             (goto-char 2001) (insert "x") (delete-char -1)
             (goto-char 1)
             (let* ((case-fold-search nil)
                    (before (gap-position))
                    (answer (re-search-forward "z" 9 t)))
               (list before answer (gap-position) (point))))"#,
    );
    // GNU keeps the gap outside the short search span. This also checks
    // movement cost without depending on a timing threshold.
    assert_eq!(observed, "OK (2001 nil 2001 1)");
}

/// A BOUNDED search must not scan past its bound, and must still find a match
/// that ends exactly at it.
///
/// GNU's fastmap skip loop is bounded by the remaining RANGE
/// (`while (range > lim && !fastmap[*d]) { d++; range--; }`,
/// src/regex-emacs.c `re_search_2`), so a bounded search costs the bound, not
/// the buffer.  Our literal prefilter ran its SIMD scan to `text_len` and only
/// then rejected the candidate for being past `end`, which made every bounded
/// FAILING search cost the whole buffer: 34.6ms over 800KB against GNU's flat
/// 0.7ms.  Font-lock issues exactly this shape (a bounded search to LIMIT that
/// usually fails) on every fontified chunk.
///
/// The boundary rows are the ones that matter: the new scan bound is
/// `end + offset + longest-needle`, and if that arithmetic is wrong by even
/// one byte the match ending exactly at the bound disappears.  Pinned to GNU
/// Emacs 31.1.
#[test]
fn a_bounded_search_finds_a_match_ending_exactly_at_the_bound() {
    crate::test_utils::init_test_tracing();
    let observed = crate::test_utils::runtime_startup_eval_one(
        r#"(with-temp-buffer
             ;; Span well past PREFILTER_MIN_BUILD_SPAN so the prefilter is live.
             (insert (make-string 4000 ?a))
             (goto-char 2001) (delete-char 4) (insert "XYZZ")
             (list
              (progn (goto-char (point-min)) (re-search-forward "XYZZ" 2004 t))
              (progn (goto-char (point-min)) (re-search-forward "XYZZ" 2005 t))
              (progn (goto-char (point-min)) (re-search-forward "XYZZ" 2006 t))
              (progn (goto-char (point-min)) (re-search-forward "XYZZ" nil t))
              (progn (goto-char (point-min)) (re-search-forward "XYZZ" 300 t))
              (progn (goto-char (point-min)) (re-search-forward "QQQQ\\|XYZZ" 2005 t))
              (progn (goto-char 2100) (re-search-forward "XYZZ" nil t))))"#,
    );
    assert_eq!(observed, "OK (nil 2005 2005 2005 nil 2005 nil)");
}

/// Sweep the BOUND across every offset around a match, for several needle
/// lengths and alternation shapes, and pin the whole answer set to GNU.
///
/// This is the assertion the single-boundary test above could not make. The
/// prefilter's scan span now stops just past the search bound, and an error of
/// one byte there hides a match at exactly one offset, for exactly one needle
/// length -- which a hand-picked case will miss. Sweeping d from -6 to +40
/// over needles of length 2..9, in three alternation shapes, covers the ways
/// that arithmetic can be wrong.
///
/// The 240 rows below were verified identical to GNU Emacs 31.1 row by row
/// (not by hash -- `sxhash` is not comparable across implementations).
#[test]
fn bounded_searches_answer_like_gnu_across_a_bound_sweep() {
    crate::test_utils::init_test_tracing();
    let observed = crate::test_utils::runtime_startup_eval_one(
        r#"(let ((out '()))
             (dolist (needle '("XY" "XYZ" "XYZZ" "XYZZY" "XYZZYQWER"))
               (dolist (pat (list needle
                                  (concat "QQQQQQ\\|" needle)
                                  (concat needle "\\|ZZZZZZ")))
                 (with-temp-buffer
                   (insert (make-string 3000 ?a))
                   (let ((at 1500))
                     (goto-char at) (delete-char (length needle)) (insert needle)
                     (dolist (d '(-6 -3 -1 0 1 2 3 6 40))
                       (let ((bound (+ at (length needle) d)))
                         (when (and (> bound 1) (<= bound (point-max)))
                           (goto-char (point-min))
                           (push (re-search-forward pat bound t) out))))
                     (dolist (s '(1 1400 1499 1500 1501 1505 2000))
                       (goto-char s)
                       (push (re-search-forward pat nil t) out))))))
             (list (length out)
                   (length (delq nil (copy-sequence out)))
                   (apply #'+ (delq nil (copy-sequence out)))))"#,
    );
    // Taken from GNU Emacs 31.1, not hand-derived: 240 probes, 150 of which
    // find a match, the found positions summing to 225,690.  Any offset that
    // stopped matching moves the second and third numbers.
    assert_eq!(observed, "OK (240 150 225690)");
}

#[test]
fn bounded_bol_search_preserves_gnu_point_and_match_data() {
    crate::test_utils::init_test_tracing();
    let observed = crate::test_utils::runtime_startup_eval_one(
        r#"(let (out)
             ;; TEXT PATTERN START BOUND NOERROR BACKWARD CASE-FOLD
             (dolist (case '(("aa\nz" "^" 2 3 t nil nil)
                             ("aa\nz" "^" 2 4 t nil nil)
                             ("aa\nz" "^\\(\\)" 2 4 t nil nil)
                             ("aa\nz" "^\\(z\\)" 2 4 t nil nil)
                             ("aa\nz" "^\\(z\\)" 2 5 t nil nil)
                             ("aa\nz" "^z" 2 3 move nil nil)
                             ("aa\nz" "^z" 2 3 nil nil nil)
                             ("aa\nz" "^" 4 4 t nil nil)
                             ("aa\nz" "^z" 4 4 t nil nil)
                             ("aa\n" "^" 2 4 t nil nil)
                             ("" "^" 1 1 t nil nil)
                             ("z" "^" 1 1 t nil nil)
                             ("z" "^z" 1 1 t nil nil)
                             ("é中\nz" "^\\(\\)" 2 4 t nil nil)
                             ("é中\n日" "^\\(日\\)" 2 5 t nil nil)
                             ("é中\n日" "^日" 2 4 t nil nil)
                             ("aa\nZ" "^\\(z\\)" 2 5 t nil t)
                             ("aa\nZ" "^z" 2 5 t nil nil)
                             ("aa\nz" "^\\(z\\)" 5 2 t t nil)
                             ("aa\nz" "^" 5 4 t t nil)
                             ("aa\nz" "^z" 4 2 t t nil)))
               (with-temp-buffer
                 (insert (nth 0 case))
                 (goto-char (nth 2 case))
                 (set-match-data '(7 9))
                 (let* ((case-fold-search (nth 6 case))
                        (answer (condition-case err
                                    (funcall (if (nth 5 case)
                                                 #'re-search-backward
                                               #'re-search-forward)
                                             (nth 1 case) (nth 3 case) (nth 4 case))
                                  (search-failed (car err)))))
                   (push (list answer (point)
                               (mapcar (lambda (value)
                                         (if (bufferp value)
                                             (eq value (current-buffer)) value))
                                       (match-data t))) out))))
             (nreverse out))"#,
    );
    // Checked row by row against GNU Emacs 31.1. A trailing t in match data
    // asserts that the match belongs to the searched buffer.
    assert_eq!(
        observed,
        concat!(
            "OK ((nil 2 (7 9)) (4 4 (4 4 t)) (4 4 (4 4 4 4 t)) ",
            "(nil 2 (7 9)) (5 5 (4 5 4 5 t)) (nil 3 (7 9)) ",
            "(search-failed 2 (7 9)) (4 4 (4 4 t)) (nil 4 (7 9)) ",
            "(4 4 (4 4 t)) (1 1 (1 1 t)) (1 1 (1 1 t)) (nil 1 (7 9)) ",
            "(4 4 (4 4 4 4 t)) (5 5 (4 5 4 5 t)) (nil 2 (7 9)) ",
            "(5 5 (4 5 4 5 t)) (nil 2 (7 9)) (4 4 (4 5 4 5 t)) ",
            "(4 4 (4 4 t)) (nil 4 (7 9)))"
        )
    );
}

#[test]
fn search_entries_preserve_fresh_arguments_across_callback_gc() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.gc_stress = false;
    let warm = eval.eval_str(
        r#"(progn
  (require 'syntax)
  (require 'bytecomp)
  (dolist (name '(re-search-forward re-search-backward
                 posix-search-forward posix-search-backward))
    (let* ((alias (intern (concat "fixed-search-gc-alias-" (symbol-name name))))
           (wrapper (intern (concat "fixed-search-gc-call-" (symbol-name name))))
           (lexical-binding t))
      (fset alias (symbol-function name))
      (fset wrapper (byte-compile
                    `(lambda (pattern bound noerror count)
                       (,alias pattern bound noerror count))))))
  (defun fixed-search-scoped-gc ()
    (let (out)
      (dolist (name '(re-search-forward re-search-backward
                     posix-search-forward posix-search-backward))
        (with-temp-buffer
          (insert "ab cd")
          (let* ((backward (string-match-p "backward" (symbol-name name)))
                 (wrapper (intern (concat "fixed-search-gc-call-" (symbol-name name))))
                 (parse-sexp-lookup-properties t)
                 (case-fold-search nil)
                 (calls 0))
            (setq-local syntax-propertize-function
                        (lambda (_start _end)
                          (setq calls (1+ calls))
                          (put-text-property 1 3 'syntax-table (string-to-syntax "."))
                          (garbage-collect)))
            (setq-local syntax-propertize--done 1)
            (goto-char (if backward 6 1))
            (let ((answer (funcall wrapper (concat "\\" "w")
                                   (copy-marker (if backward 1 6)) t 1)))
              (push (list answer (point) (> calls 0)
                          (mapcar (lambda (v) (if (bufferp v) 'buffer v))
                                  (match-data t))) out)))))
      (nreverse out)))
  (fixed-search-scoped-gc))"#,
    );
    // GNU Emacs 31.1: all four regex entries retain fresh pattern/marker arguments.
    let expected = concat!(
        "OK ((5 5 t (4 5 buffer)) (5 5 t (5 6 buffer)) ",
        "(5 5 t (4 5 buffer)) (5 5 t (5 6 buffer)))",
    );
    assert_eq!(crate::emacs_core::format_eval_result(&warm), expected);
    eval.gc_stress = true;
    let before = eval.gc_count;
    let result = eval.eval_str("(fixed-search-scoped-gc)");
    let collections = eval.gc_count - before;
    assert_eq!(crate::emacs_core::format_eval_result(&result), expected);
    assert!(
        collections > 4,
        "expected automatic collections beyond four callbacks"
    );
    eprintln!("fixed search scoped stress collections: {collections}");
}

#[test]
fn prepared_regexp_zero_count_skips_compilation_after_argument_validation() {
    crate::test_utils::init_test_tracing();
    let observed = crate::test_utils::runtime_startup_eval_one(
        r#"(progn
(require 'syntax)
(let (out)
  (dolist (fn '(re-search-forward re-search-backward posix-search-forward posix-search-backward))
    (dolist (inhibit '(nil t))
      (with-temp-buffer
        (insert "éabc")
        (narrow-to-region 2 5)
        (let ((inhibit-changing-match-data inhibit)
              (parse-sexp-lookup-properties t)
              (calls 0))
          (setq-local syntax-propertize-function
                      (lambda (_start _end) (setq calls (1+ calls))))
          (setq-local syntax-propertize--done 1)
          (dolist (args (list (list "[" 2 t 0)
                              (list "[" (copy-marker 2) t 0)
                              (list 7 2 t 0)
                              (list "[" 2 t 'bad)
                              (list "[" 'bad t 0)
                              (list "[" 3 t 0)))
            (goto-char 2)
            (set-match-data '(17 19))
            (push (list (condition-case err (apply fn args)
                          (error (car err)))
                        (point) calls
                        (mapcar (lambda (v) (if (bufferp v) 'buffer v)) (match-data t))) out))))))
  (nreverse out))

)"#,
    );
    // GNU Emacs 31.1: all four regexp searches agree, with and without
    // match-data inhibition. Zero count skips invalid-regexp and callbacks,
    // but preserves STRING, COUNT and BOUND validation and marker coercion.
    let per_builtin = concat!(
        "(2 2 0 (2 2 buffer)) (2 2 0 (2 2 buffer)) (wrong-type-argument 2 0 (17 19)) (wrong-type-ar",
        "gument 2 0 (17 19)) (wrong-type-argument 2 0 (17 19)) (error 2 0 (17 19)) (2 2 0 (17 19)) ",
        "(2 2 0 (17 19)) (wrong-type-argument 2 0 (17 19)) (wrong-type-argument 2 0 (17 19)) (wrong",
        "-type-argument 2 0 (17 19)) (error 2 0 (17 19))",
    );
    assert_eq!(observed, format!("OK ({0} {0} {0} {0})", per_builtin));
}

#[test]
fn prepared_regexp_reuse_preserves_reverse_counts_posix_and_captures() {
    crate::test_utils::init_test_tracing();
    let observed = crate::test_utils::runtime_startup_eval_one(
        r#"(progn
(defun prepared-regexp-direction-captures ()
  (let (out)
    (dolist (fn '(re-search-forward re-search-backward posix-search-forward posix-search-backward))
      (dolist (count '(-2 -1 1 2))
        (dolist (case '(("b" nil) ("B" t) ("\\(a\\|ab\\)" nil) ("\\(a\\)\\(b\\)?" nil)))
          (with-temp-buffer
            (insert "xαab ab中bz")
            (narrow-to-region 2 10)
            (let* ((backward (string-match-p "backward" (symbol-name fn)))
                   (reverse (if (< count 0) (not backward) backward))
                   (start (if reverse (point-max) (point-min)))
                   (bound (copy-marker (if reverse (point-min) (point-max))))
                   (case-fold-search (nth 1 case))
                   (parse-sexp-lookup-properties nil))
              (goto-char start)
              (set-match-data '(17 19))
              (push (list (funcall fn (car case) bound t count) (point)
                          (mapcar (lambda (v) (if (bufferp v) 'buffer v)) (match-data t))) out))))))
    (nreverse out)))
(prepared-regexp-direction-captures)
)"#,
    );
    // GNU 31.1: four entry points, both directions and repeated matches,
    // marker bounds in a narrowed multibyte buffer, translated literals,
    // POSIX longest-match alternatives and captured optional groups.
    assert_eq!(
        observed,
        concat!(
            "OK ((7 7 (7 8 buffer)) (7 7 (7 8 buffer)) (3 3 (3 4 3 4 buffer)) (3 3 (3 5 3 4 4 5 buffer)",
            ") (9 9 (9 10 buffer)) (9 9 (9 10 buffer)) (6 6 (6 7 6 7 buffer)) (6 6 (6 8 6 7 7 8 buffer)",
            ") (5 5 (4 5 buffer)) (5 5 (4 5 buffer)) (4 4 (3 4 3 4 buffer)) (5 5 (3 5 3 4 4 5 buffer)) ",
            "(8 8 (7 8 buffer)) (8 8 (7 8 buffer)) (7 7 (6 7 6 7 buffer)) (8 8 (6 8 6 7 7 8 buffer)) (8",
            " 8 (7 8 buffer)) (8 8 (7 8 buffer)) (7 7 (6 7 6 7 buffer)) (8 8 (6 8 6 7 7 8 buffer)) (5 5",
            " (4 5 buffer)) (5 5 (4 5 buffer)) (4 4 (3 4 3 4 buffer)) (5 5 (3 5 3 4 4 5 buffer)) (9 9 (",
            "9 10 buffer)) (9 9 (9 10 buffer)) (6 6 (6 7 6 7 buffer)) (6 6 (6 8 6 7 7 8 buffer)) (7 7 (",
            "7 8 buffer)) (7 7 (7 8 buffer)) (3 3 (3 4 3 4 buffer)) (3 3 (3 5 3 4 4 5 buffer)) (7 7 (7 ",
            "8 buffer)) (7 7 (7 8 buffer)) (3 3 (3 5 3 5 buffer)) (3 3 (3 5 3 4 4 5 buffer)) (9 9 (9 10",
            " buffer)) (9 9 (9 10 buffer)) (6 6 (6 8 6 8 buffer)) (6 6 (6 8 6 7 7 8 buffer)) (5 5 (4 5 ",
            "buffer)) (5 5 (4 5 buffer)) (5 5 (3 5 3 5 buffer)) (5 5 (3 5 3 4 4 5 buffer)) (8 8 (7 8 bu",
            "ffer)) (8 8 (7 8 buffer)) (8 8 (6 8 6 8 buffer)) (8 8 (6 8 6 7 7 8 buffer)) (8 8 (7 8 buff",
            "er)) (8 8 (7 8 buffer)) (8 8 (6 8 6 8 buffer)) (8 8 (6 8 6 7 7 8 buffer)) (5 5 (4 5 buffer",
            ")) (5 5 (4 5 buffer)) (5 5 (3 5 3 5 buffer)) (5 5 (3 5 3 4 4 5 buffer)) (9 9 (9 10 buffer)",
            ") (9 9 (9 10 buffer)) (6 6 (6 8 6 8 buffer)) (6 6 (6 8 6 7 7 8 buffer)) (7 7 (7 8 buffer))",
            " (7 7 (7 8 buffer)) (3 3 (3 5 3 5 buffer)) (3 3 (3 5 3 4 4 5 buffer)))",
        )
    );
}

impl FuzzRng {
    fn new(seed: u64) -> Self {
        FuzzRng(seed ^ 0x9E3779B97F4A7C15)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }
    /// True with probability `num/den`.
    fn chance(&mut self, num: u64, den: u64) -> bool {
        self.next_u64() % den < num
    }
    fn pick_char(&mut self, xs: &[char]) -> char {
        xs[self.below(xs.len())]
    }
    fn pick_str<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[self.below(xs.len())]
    }
}

/// Tiny deterministic xorshift RNG so fuzz failures reproduce from a seed.
struct FuzzRng(u64);
