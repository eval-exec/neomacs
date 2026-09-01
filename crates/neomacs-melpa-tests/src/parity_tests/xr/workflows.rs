use expect_test::expect;

use super::ParityBatchCase;

fn translates_and_round_trips_a_release_log_regexp_across_dialects() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((regexp
        "\\`\\(?:ERROR\\|WARN\\)[[:space:]]+\\([[:digit:]]\\{4\\}-[[:digit:]]\\{2\\}-[[:digit:]]\\{2\\}\\)[[:space:]]+\\(?:api\\|worker\\):[[:space:]]+\\(.+\\)\\'")
       (medium (xr regexp))
       (round-trip (rx-to-string medium t))
       (samples
        '("ERROR 2026-08-03 api: deploy failed λ"
          "WARN 2026-08-04 worker: retry 2/5"
          "INFO 2026-08-03 api: healthy"
          "WARN 2026-8-03 api: malformed date"
          "WARN 2026-08-03 web: wrong component")))
  (list
   :medium medium
   :brief (xr regexp 'brief)
   :terse (xr regexp 'terse)
   :verbose (xr regexp 'verbose)
   :round-trip round-trip
   :matches
   (mapcar
    (lambda (line)
      (let ((original-match (string-match regexp line)))
        (list
         line
         (and original-match
              (list (match-string 1 line) (match-string 2 line)))
         (let ((round-trip-match (string-match round-trip line)))
           (and round-trip-match
                (list (match-string 1 line) (match-string 2 line)))))))
    samples))))
"####;
    let expect = expect![[
        r####"OK (:medium (seq bos (or "ERROR" "WARN") (one-or-more space) (group (= 4 digit) "-" (= 2 digit) "-" (= 2 digit)) (one-or-more space) (or "api" "worker") ":" (one-or-more space) (group (one-or-more nonl)) eos) :brief (seq bos (or "ERROR" "WARN") (1+ space) (group (= 4 digit) "-" (= 2 digit) "-" (= 2 digit)) (1+ space) (or "api" "worker") ":" (1+ space) (group (1+ nonl)) eos) :terse (: bos (| "ERROR" "WARN") (+ space) (group (= 4 digit) "-" (= 2 digit) "-" (= 2 digit)) (+ space) (| "api" "worker") ":" (+ space) (group (+ nonl)) eos) :verbose (seq string-start (or "ERROR" "WARN") (one-or-more space) (group (= 4 digit) "-" (= 2 digit) "-" (= 2 digit)) (one-or-more space) (or "api" "worker") ":" (one-or-more space) (group (one-or-more not-newline)) string-end) :round-trip "\\`\\(?:ERROR\\|WARN\\)[[:space:]]+\\([[:digit:]]\\{4\\}-[[:digit:]]\\{2\\}-[[:digit:]]\\{2\\}\\)[[:space:]]+\\(?:api\\|worker\\):[[:space:]]+\\(.+\\)\\'" :matches (("ERROR 2026-08-03 api: deploy failed λ" ("2026-08-03" "deploy failed λ") ("2026-08-03" "deploy failed λ")) ("WARN 2026-08-04 worker: retry 2/5" ("2026-08-04" "retry 2/5") ("2026-08-04" "retry 2/5")) ("INFO 2026-08-03 api: healthy" nil nil) ("WARN 2026-8-03 api: malformed date" nil nil) ("WARN 2026-08-03 web: wrong component" nil nil)))"####
    ]];
    ParityBatchCase::value(
        "translates_and_round_trips_a_release_log_regexp_across_dialects",
        elisp_form,
        expect,
    )
}

fn lints_real_filename_and_log_patterns_with_grouped_diagnostics() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let ((text-quoting-style 'grave)
      (file-pattern "^releases/[A-z]+.\\(json\\|json\\)$")
      (log-pattern "\\(?:[ab]+a*\\)*\\|\\(?:warn\\|warn\\)")
      (operator-token-pattern "[0-9+-/*][&-+=]")
      (bad-posix-pattern "archives/[+[:alnum]]+\\.tar"))
  (list
   :file-pattern file-pattern
   :file-default (xr-lint file-pattern 'file)
   :file-all (xr-lint file-pattern 'file 'all)
   :log-pattern log-pattern
   :log-default (xr-lint log-pattern)
   :log-all (xr-lint log-pattern nil 'all)
   :operator-token-pattern operator-token-pattern
   :operator-token-default (xr-lint operator-token-pattern)
   :operator-token-all (xr-lint operator-token-pattern nil 'all)
   :bad-posix-pattern bad-posix-pattern
   :bad-posix-diagnostics (xr-lint bad-posix-pattern 'file))))
"####;
    let expect = expect![[
        r####"OK (:file-pattern "^releases/[A-z]+.\\(json\\|json\\)$" :file-default (((0 0 "Use \\` instead of ^ in file-matching regexp" warning)) ((11 13 "Range `A-z' between upper and lower case includes symbols" warning)) ((16 16 "Possibly unescaped `.' in file-matching regexp" warning)) ((25 28 "Duplicated alternative branch" warning) (19 22 "Previous occurrence here" info)) ((31 31 "Use \\' instead of $ in file-matching regexp" warning))) :file-all (((0 0 "Use \\` instead of ^ in file-matching regexp" warning)) ((11 13 "Range `A-z' between upper and lower case includes symbols" warning)) ((16 16 "Possibly unescaped `.' in file-matching regexp" warning)) ((25 28 "Duplicated alternative branch" warning) (19 22 "Previous occurrence here" info)) ((31 31 "Use \\' instead of $ in file-matching regexp" warning))) :log-pattern "\\(?:[ab]+a*\\)*\\|\\(?:warn\\|warn\\)" :log-default (((9 10 "Repetition subsumed by preceding repetition" warning) (4 8 "Subsuming repetition here" info)) ((13 13 "Repetition of effective repetition" warning) (0 12 "This expression contains a repetition" info)) ((26 29 "Duplicated alternative branch" warning) (20 23 "Previous occurrence here" info))) :log-all (((9 10 "Repetition subsumed by preceding repetition" warning) (4 8 "Subsuming repetition here" info)) ((13 13 "Repetition of effective repetition" warning) (0 12 "This expression contains a repetition" info)) ((26 29 "Duplicated alternative branch" warning) (20 23 "Previous occurrence here" info))) :operator-token-pattern "[0-9+-/*][&-+=]" :operator-token-default nil :operator-token-all (((4 6 "Suspect character range `+-/': should `-' be literal?" warning)) ((10 12 "Suspect character range `&-+': should `-' be literal?" warning))) :bad-posix-pattern "archives/[+[:alnum]]+\\.tar" :bad-posix-diagnostics (((11 18 "Possibly missing `:' after character class" warning))))"####
    ]];
    ParityBatchCase::value(
        "lints_real_filename_and_log_patterns_with_grouped_diagnostics",
        elisp_form,
        expect,
    )
}

fn converts_skip_sets_used_for_identifier_scanning_and_lints_a_bad_set() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((identifier-set "A-Za-z0-9_-")
       (negated-set "^[:space:]0-9")
       (bad-set "A-Fa-z3D-KM-N!3-7\\!b")
       (bad-posix-set "A-Za-z0-9_[:space]")
       (text-quoting-style 'grave)
       scanned pretty)
  (with-temp-buffer
    (insert "build-42_alpha.yaml next")
    (goto-char (point-min))
    (skip-chars-forward identifier-set)
    (setq scanned
          (list (buffer-substring-no-properties (point-min) (point))
                (point)
                (char-after))))
  (with-temp-buffer
    (xr-skip-set-pp negated-set 'terse)
    (setq pretty (buffer-string)))
  (list
   :identifier-rx (xr-skip-set identifier-set)
   :identifier-terse (xr-skip-set identifier-set 'terse)
   :negated-rx (xr-skip-set negated-set)
   :scan scanned
   :pretty pretty
   :bad-set bad-set
   :diagnostics (xr-skip-set-lint bad-set)
   :bad-posix-set bad-posix-set
   :bad-posix-diagnostics (xr-skip-set-lint bad-posix-set))))
"####;
    let expect = expect![[
        r####"OK (:identifier-rx (any "0-9A-Za-z" "_-") :identifier-terse (in "0-9A-Za-z" "_-") :negated-rx (not (any "0-9" space)) :scan ("build-42_alpha" 15 46) :pretty "(not (in \"0-9\" space))\n" :bad-set "A-Fa-z3D-KM-N!3-7\\!b" :diagnostics (((7 9 "Ranges `A-F' and `D-K' overlap" warning)) ((10 12 "Two-element range `M-N'" warning)) ((14 16 "Range `3-7' includes character `3'" warning)) ((17 18 "Duplicated character `!'" warning)) ((17 18 "Unnecessarily escaped `!'" warning)) ((19 19 "Character `b' included in range `a-z'" warning))) :bad-posix-set "A-Za-z0-9_[:space]" :bad-posix-diagnostics (((10 17 "Possibly missing `:' after character class" warning)) ((12 12 "Character `s' included in range `a-z'" warning)) ((13 13 "Character `p' included in range `a-z'" warning)) ((14 14 "Character `a' included in range `a-z'" warning)) ((15 15 "Character `c' included in range `a-z'" warning)) ((16 16 "Character `e' included in range `a-z'" warning))))"####
    ]];
    ParityBatchCase::value(
        "converts_skip_sets_used_for_identifier_scanning_and_lints_a_bad_set",
        elisp_form,
        expect,
    )
}

fn pretty_prints_complex_rx_and_reports_precise_parse_failures() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let ((text-quoting-style 'grave)
      pretty skip-pretty parse-errors lint-errors)
  (with-temp-buffer
    (insert "before|after")
    (goto-char (point-min))
    (search-forward "|")
    (xr-pp "\\`\\(?:Ångström\\|Ωmega\\)[[:space:]]+\\([[:word:]_-]+\\)\\'" 'verbose)
    (setq pretty (list (buffer-string) (point))))
  (with-temp-buffer
    (insert "left|right")
    (goto-char (point-min))
    (search-forward "|")
    (xr-skip-set-pp "^ac-nq\\-u")
    (setq skip-pretty (list (buffer-string) (point))))
  (setq parse-errors
        (mapcar
         (lambda (regexp)
           (condition-case error-data
               (list regexp :value (xr regexp))
             (error (list regexp :error error-data))))
         '("[unterminated" "\\(missing-close" "trailing\\" "\\(?0:bad\\)")))
  (setq lint-errors
        (mapcar
         (lambda (regexp) (list regexp (xr-lint regexp)))
         '("[unterminated" "trailing\\" "\\(?0:bad\\)")))
  (list
   :pretty pretty
   :skip-pretty skip-pretty
   :manual-pretty
   (xr-pp-rx-to-str
   '(seq bos (one-or-more (or "release" (not (any space)))) eos))
   :parse-errors parse-errors
   :lint-errors lint-errors)))
"####;
    let expect = expect![[
        r####"OK (:pretty ("before|(seq string-start (or \"Ångström\" \"Ωmega\") (one-or-more space)\n     (group (one-or-more (any \"_-\" word))) string-end)\nafter" 125) :skip-pretty ("left|(not (any \"c-n\" \"aqu-\"))\nright" 31) :manual-pretty "(seq bos (one-or-more (or \"release\" (not (any space)))) eos)\n" :parse-errors (("[unterminated" :error (xr-parse-error "Unterminated character alternative" 0 12)) ("\\(missing-close" :error (xr-parse-error "Missing \\)" 0 14)) ("trailing\\" :error (xr-parse-error "Backslash at end of regexp" 8 8)) ("\\(?0:bad\\)" :error (xr-parse-error "Invalid \\(? syntax" 0 2))) :lint-errors (("[unterminated" (((0 12 "Unterminated character alternative" error)))) ("trailing\\" (((8 8 "Backslash at end of regexp" error)))) ("\\(?0:bad\\)" (((0 2 "Invalid \\(? syntax" error))))))"####
    ]];
    ParityBatchCase::value(
        "pretty_prints_complex_rx_and_reports_precise_parse_failures",
        elisp_form,
        expect,
    )
}

pub(super) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        translates_and_round_trips_a_release_log_regexp_across_dialects(),
        lints_real_filename_and_log_patterns_with_grouped_diagnostics(),
        converts_skip_sets_used_for_identifier_scanning_and_lints_a_bad_set(),
        pretty_prints_complex_rx_and_reports_precise_parse_failures(),
    ]
}
