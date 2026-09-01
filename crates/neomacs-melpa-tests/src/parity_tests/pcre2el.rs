use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, PCRE2EL_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PCRE2EL_TEST_TIMEOUT: Duration = Duration::from_secs(90);
const PCRE2EL_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'ring)
(require 'pcre2el)

(setq rxt-verbose-rx-translation nil)

(defun neomacs-pcre2el-test-current-match (string)
  "Describe the current match against STRING, including every subgroup."
  (let ((group-count (1- (/ (length (match-data)) 2))))
    (list :range (list (match-beginning 0) (match-end 0))
          :groups
          (cl-loop for group from 0 to group-count
                   collect (match-string-no-properties group string)))))

(defun neomacs-pcre2el-test-match (regexp string)
  "Return REGEXP's first exact match report for STRING."
  (when (string-match regexp string)
    (neomacs-pcre2el-test-current-match string)))

(defun neomacs-pcre2el-test-all-matches (regexp string)
  "Return every non-overlapping REGEXP match in STRING."
  (let ((start 0)
        matches)
    (while (and (<= start (length string))
                (string-match regexp string start))
      (let ((begin (match-beginning 0))
            (end (match-end 0)))
        (push (neomacs-pcre2el-test-current-match string) matches)
        (setq start (if (= begin end) (1+ end) end))))
    (nreverse matches)))

(defun neomacs-pcre2el-test-hash-pairs (table)
  "Return TABLE's string mappings in deterministic key order."
  (let (pairs)
    (maphash (lambda (key value) (push (cons key value) pairs)) table)
    (sort pairs (lambda (left right) (string< (car left) (car right))))))

(defun neomacs-pcre2el-test-outcome (thunk)
  "Return THUNK's value or its exact signal contract."
  (condition-case error-data
      (list :value (funcall thunk))
    (error
     (list :signal (car error-data)
           :data (cdr error-data)
           :message (error-message-string error-data)))))
"###;

fn pcre2el_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PCRE2EL_MELPA_PIN, "pcre2el.el")
        .expect("prepare revision-pinned pcre2el source below ./tmp")
        .with_prelude(PCRE2EL_TEST_PRELUDE)
        .with_timeout(PCRE2EL_TEST_TIMEOUT)
}

fn production_log_parser_translates_captures_and_rejects_malformed_deployments() -> ParityBatchCase
{
    let elisp_form = r###"
(let* ((case-fold-search nil)
       (pcre
        "\\A([A-Z]{3})\\s+release=(REL-\\d{4})\\s+region=(us-(?:east|west)-\\d)\\s+latency=(\\d{1,4})ms\\Z")
       (regexp (rxt-pcre-to-elisp pcre))
       (accepted
        '("INF release=REL-2048 region=us-east-1 latency=87ms"
          "WRN\trelease=REL-4096 region=us-west-2 latency=1200ms"))
       (rejected
        '("INF release=rel-2048 region=us-east-1 latency=87ms"
          "INF release=REL-48 region=eu-west-1 latency=87ms"
          "INF release=REL-2048 region=us-east-1 latency=12000ms trailing")))
  (list :pcre pcre
        :elisp regexp
        :rx (rxt-pcre-to-rx pcre)
        :accepted
        (mapcar (lambda (line)
                  (list line (neomacs-pcre2el-test-match regexp line)))
                accepted)
        :rejected
        (mapcar (lambda (line)
                  (list line (neomacs-pcre2el-test-match regexp line)))
                rejected)))
"###;
    let expected = expect![[
        r#"OK (:pcre "\\A([A-Z]{3})\\s+release=(REL-\\d{4})\\s+region=(us-(?:east|west)-\\d)\\s+latency=(\\d{1,4})ms\\Z" :elisp "\\`\\([A-Z]\\{3\\}\\)[\11\n\f\15 ]+release=\\(REL-[[:digit:]]\\{4\\}\\)[\11\n\f\15 ]+region=\\(us-\\(?:\\(?:ea\\|we\\)st\\)-[[:digit:]]\\)[\11\n\f\15 ]+latency=\\([[:digit:]]\\{1,4\\}\\)ms\\'" :rx (seq bos (submatch (= 3 (any (65 . 90)))) (+ (any 9 10 12 13 32)) "release=" (submatch "REL-" (= 4 digit)) (+ (any 9 10 12 13 32)) "region=" (submatch "us-" (or "east" "west") "-" digit) (+ (any 9 10 12 13 32)) "latency=" (submatch (** 1 4 digit)) "ms" eos) :accepted (("INF release=REL-2048 region=us-east-1 latency=87ms" (:range (0 50) :groups ("INF release=REL-2048 region=us-east-1 latency=87ms" "INF" "REL-2048" "us-east-1" "87"))) ("WRN\11release=REL-4096 region=us-west-2 latency=1200ms" (:range (0 52) :groups ("WRN\11release=REL-4096 region=us-west-2 latency=1200ms" "WRN" "REL-4096" "us-west-2" "1200")))) :rejected (("INF release=rel-2048 region=us-east-1 latency=87ms" nil) ("INF release=REL-48 region=eu-west-1 latency=87ms" nil) ("INF release=REL-2048 region=us-east-1 latency=12000ms trailing" nil)))"#
    ]];
    ParityBatchCase::value(
        "production_log_parser_translates_captures_and_rejects_malformed_deployments",
        elisp_form,
        expected,
    )
}

fn multiline_extended_incident_parser_preserves_lazy_capture_boundaries() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((case-fold-search nil)
       (pcre
        "\\A
         BEGIN[ ]+INCIDENT[ ]+(INC-\\d+)\\s*
         owner:[ ]*([a-z.]+)\\s*
         details:[ ]*(.*?)\\s*
         END[ ]+INCIDENT
         \\Z")
       (flags "sx")
       (regexp (rxt-pcre-to-elisp pcre flags))
       (incident
        "BEGIN INCIDENT INC-2048\nowner: release.bot\ndetails: first line\nsecond line\nEND INCIDENT")
       (truncated
        "BEGIN INCIDENT INC-2048\nowner: release.bot\ndetails: first line\nsecond line"))
  (list :elisp regexp
        :rx (rxt-pcre-to-rx pcre flags)
        :incident (neomacs-pcre2el-test-match regexp incident)
        :truncated (neomacs-pcre2el-test-match regexp truncated)))
"###;
    let expected = expect![[
        r#"OK (:elisp "\\`BEGIN +INCIDENT +\\(INC-[[:digit:]]+\\)[\11\n\f\15 ]*owner: *\\([.a-z]+\\)[\11\n\f\15 ]*details: *\\([^z-a]*?\\)[\11\n\f\15 ]*END +INCIDENT\\'" :rx (seq bos "BEGIN" (+ (any 32)) "INCIDENT" (+ (any 32)) (submatch "INC-" (+ digit)) (* (any 9 10 12 13 32)) "owner:" (* (any 32)) (submatch (+ (any 46 (97 . 122)))) (* (any 9 10 12 13 32)) "details:" (* (any 32)) (submatch (*? anything)) (* (any 9 10 12 13 32)) "END" (+ (any 32)) "INCIDENT" eos) :incident (:range (0 87) :groups ("BEGIN INCIDENT INC-2048\nowner: release.bot\ndetails: first line\nsecond line\nEND INCIDENT" "INC-2048" "release.bot" "first line\nsecond line")) :truncated nil)"#
    ]];
    ParityBatchCase::value(
        "multiline_extended_incident_parser_preserves_lazy_capture_boundaries",
        elisp_form,
        expected,
    )
}

fn configuration_redaction_replaces_only_secret_values_across_case_and_separator_variants()
-> ParityBatchCase {
    let elisp_form = r###"
(let* ((case-fold-search nil)
       (pcre "(?i)(api[_-]?key|token)\\s*[:=]\\s*([A-Z0-9_-]{6,})")
       (regexp (rxt-pcre-to-elisp pcre))
       (configuration
        "api_key = PROD_ABC123\ntoken: canary-789\nendpoint=https://deploy.example\nshort=abc\nAPI-KEY: SECOND_456\n")
       (before (neomacs-pcre2el-test-all-matches regexp configuration))
       (redacted
        (replace-regexp-in-string regexp "<redacted>" configuration t t 2)))
  (list :elisp regexp
        :before before
        :redacted redacted
        :remaining (neomacs-pcre2el-test-all-matches regexp redacted)))
"###;
    let expected = expect![[
        r#"OK (:elisp "\\([Aa][Pp][Ii][_-]?[Kk][Ee][Yy]\\|[Tt][Oo][Kk][Ee][Nn]\\)[\11\n\f\15 ]*[:=][\11\n\f\15 ]*\\([0-9A-Z_a-z-]\\{6,\\}\\)" :before ((:range (0 21) :groups ("api_key = PROD_ABC123" "api_key" "PROD_ABC123")) (:range (22 39) :groups ("token: canary-789" "token" "canary-789")) (:range (82 101) :groups ("API-KEY: SECOND_456" "API-KEY" "SECOND_456"))) :redacted "api_key = <redacted>\ntoken: <redacted>\nendpoint=https://deploy.example\nshort=abc\nAPI-KEY: <redacted>\n" :remaining nil)"#
    ]];
    ParityBatchCase::value(
        "configuration_redaction_replaces_only_secret_values_across_case_and_separator_variants",
        elisp_form,
        expected,
    )
}

fn command_vocabulary_round_trips_regexp_opt_and_enumerates_a_finite_release_matrix()
-> ParityBatchCase {
    let elisp_form = r###"
(let* ((case-fold-search nil)
       (commands
        '("deploy" "deployment" "deploy-canary" "rollback" "roll-forward"))
       (regexp (regexp-opt commands))
       (whole-regexp (concat "\\`" regexp "\\'"))
       (near-misses '("deployments" "deploy-blue" "roll" "rollback-now"))
       (finite-pcre "(?:blue|green)-(?:1|2)-(?:canary|stable)"))
  (list :regexp regexp
        :pcre (rxt-elisp-to-pcre regexp)
        :rx (rxt-elisp-to-rx regexp)
        :recovered (rxt-elisp-to-strings regexp)
        :commands
        (mapcar (lambda (command)
                  (list command (and (string-match-p whole-regexp command) t)))
                commands)
        :near-misses
        (mapcar (lambda (command)
                  (list command (and (string-match-p whole-regexp command) t)))
                near-misses)
        :finite-rx (rxt-pcre-to-rx finite-pcre)
        :finite-strings (rxt-pcre-to-strings finite-pcre)))
"###;
    let expected = expect![[
        r#"OK (:regexp "\\(?:deploy\\(?:-canary\\|ment\\)?\\|roll\\(?:-forward\\|back\\)\\)" :pcre "deploy(?:-canary|ment)?|roll(?:-forward|back)" :rx (or (seq "deploy" (\? (or "-canary" "ment"))) (seq "roll" (or "-forward" "back"))) :recovered ("deploy" "deploy-canary" "deployment" "roll-forward" "rollback") :commands (("deploy" t) ("deployment" t) ("deploy-canary" t) ("rollback" t) ("roll-forward" t)) :near-misses (("deployments" nil) ("deploy-blue" nil) ("roll" nil) ("rollback-now" nil)) :finite-rx (seq (or "blue" "green") "-" (any 49 50) "-" (or "canary" "stable")) :finite-strings ("blue-1-canary" "blue-1-stable" "blue-2-canary" "blue-2-stable" "green-1-canary" "green-1-stable" "green-2-canary" "green-2-stable"))"#
    ]];
    ParityBatchCase::value(
        "command_vocabulary_round_trips_regexp_opt_and_enumerates_a_finite_release_matrix",
        elisp_form,
        expected,
    )
}

fn source_literal_reader_and_elisp_rx_toggle_drive_real_editing_workflows() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((literal
        (with-temp-buffer
          (insert "$state =~ s/ (?i:pending|queued) /ready/x;")
          (goto-char (point-min))
          (search-forward "=~ ")
          (rxt-read-delimited-pcre)))
       (regexp (rxt-pcre-to-elisp literal))
       (release-board "pending, READY, queued, failed")
       (converted
        (replace-regexp-in-string regexp "ready" release-board t t)))
  (with-temp-buffer
    (emacs-lisp-mode)
    (insert "(setq release-id-regexp \"\\\\`REL-[0-9]+\\\\'\")")
    (goto-char (point-min))
    (search-forward "REL-")
    (rxt-toggle-elisp-rx)
    (let ((rx-source (buffer-string)))
      (goto-char (point-min))
      (search-forward "(rx")
      (goto-char (match-beginning 0))
      (rxt-toggle-elisp-rx)
      (let* ((roundtrip-source (buffer-string))
             (roundtrip-form (car (read-from-string roundtrip-source)))
             (roundtrip-regexp (nth 2 roundtrip-form)))
        (list :literal literal
              :elisp regexp
              :converted converted
              :rx-source rx-source
              :roundtrip-source roundtrip-source
              :roundtrip-regexp roundtrip-regexp
              :valid-release
              (and (string-match-p roundtrip-regexp "REL-1207") t)
              :invalid-release
              (and (string-match-p roundtrip-regexp "XREL-1207") t))))))
"###;
    let expected = expect![[
        r#"OK (:literal "(?x) (?i:pending|queued) " :elisp "[Pp][Ee][Nn][Dd][Ii][Nn][Gg]\\|[Qq][Uu][Ee][Uu][Ee][Dd]" :converted "ready, READY, ready, failed" :rx-source "(setq release-id-regexp (rx bos \"REL-\" (+ (any (?0 . ?9))) eos))" :roundtrip-source "(setq release-id-regexp \"\\\\`REL-[0-9]+\\\\'\")" :roundtrip-regexp "\\`REL-[0-9]+\\'" :valid-release t :invalid-release nil)"#
    ]];
    ParityBatchCase::value(
        "source_literal_reader_and_elisp_rx_toggle_drive_real_editing_workflows",
        elisp_form,
        expected,
    )
}

fn bounded_translation_cache_and_global_mode_keep_search_behavior_and_lifecycle_consistent()
-> ParityBatchCase {
    let elisp_form = r###"
(let ((pcre-mode-cache-size 2)
      (pcre-mode-cache (make-hash-table :test 'equal))
      (pcre-mode-reverse-cache (make-hash-table :test 'equal))
      (pcre-cache-ring (make-ring 2)))
  (unwind-protect
      (progn
        (when pcre-mode (pcre-mode -1))
        (let* ((release-pcre "REL-(\\d+)")
               (region-pcre "region=(us-(?:east|west)-\\d)")
               (latency-pcre "latency=(\\d+)ms")
               (release-regexp (pcre-to-elisp/cached release-pcre))
               (region-regexp (pcre-to-elisp/cached region-pcre))
               (release-again (pcre-to-elisp/cached release-pcre))
               (latency-regexp (pcre-to-elisp/cached latency-pcre))
               enabled disabled search-report)
          (let ((inhibit-message t))
            (pcre-mode 1))
          (setq enabled
                (list :mode pcre-mode
                      :isearch-start-hook
                      (and (memq #'pcre-isearch-mode-hook isearch-mode-hook) t)
                      :isearch-end-hook
                      (and (memq #'pcre-isearch-mode-end-hook
                                 isearch-mode-end-hook)
                           t)))
          (setq search-report
                (with-temp-buffer
                  (insert "release=REL-2048 region=us-east-1 latency=87ms\n"
                          "release=REL-4096 region=us-west-2 latency=1200ms\n")
                  (goto-char (point-min))
                  (let ((search (pcre-decorate-search-function
                                 #'re-search-forward))
                        reports)
                    (while (funcall search release-pcre nil t)
                      (push (list :point (point)
                                  :match (match-string-no-properties 0)
                                  :release (match-string-no-properties 1))
                            reports))
                    (nreverse reports))))
          (let ((inhibit-message t))
            (pcre-mode -1))
          (setq disabled
                (list :mode pcre-mode
                      :isearch-start-hook
                      (and (memq #'pcre-isearch-mode-hook isearch-mode-hook) t)
                      :isearch-end-hook
                      (and (memq #'pcre-isearch-mode-end-hook
                                 isearch-mode-end-hook)
                           t)))
          (list :translations
                (list release-regexp region-regexp release-again latency-regexp)
                :release-cache-object-reused (eq release-regexp release-again)
                :forward-cache (neomacs-pcre2el-test-hash-pairs pcre-mode-cache)
                :reverse-cache
                (neomacs-pcre2el-test-hash-pairs pcre-mode-reverse-cache)
                :ring (ring-elements pcre-cache-ring)
                :enabled enabled
                :search search-report
                :disabled disabled)))
    (when pcre-mode
      (let ((inhibit-message t))
        (pcre-mode -1)))))
"###;
    let expected = expect![[
        r#"OK (:translations ("REL-\\([[:digit:]]+\\)" "region=\\(us-\\(?:\\(?:ea\\|we\\)st\\)-[[:digit:]]\\)" "REL-\\([[:digit:]]+\\)" "latency=\\([[:digit:]]+\\)ms") :release-cache-object-reused t :forward-cache (("REL-(\\d+)" . "REL-\\([[:digit:]]+\\)") ("region=(us-(?:east|west)-\\d)" . "region=\\(us-\\(?:\\(?:ea\\|we\\)st\\)-[[:digit:]]\\)")) :reverse-cache (("REL-\\([[:digit:]]+\\)" . "REL-(\\d+)") ("region=\\(us-\\(?:\\(?:ea\\|we\\)st\\)-[[:digit:]]\\)" . "region=(us-(?:east|west)-\\d)")) :ring (("REL-(\\d+)" . "REL-\\([[:digit:]]+\\)") ("region=(us-(?:east|west)-\\d)" . "region=\\(us-\\(?:\\(?:ea\\|we\\)st\\)-[[:digit:]]\\)")) :enabled (:mode t :isearch-start-hook t :isearch-end-hook t) :search ((:point 17 :match "REL-2048" :release "2048") (:point 64 :match "REL-4096" :release "4096")) :disabled (:mode nil :isearch-start-hook nil :isearch-end-hook nil))"#
    ]];
    ParityBatchCase::value(
        "bounded_translation_cache_and_global_mode_keep_search_behavior_and_lifecycle_consistent",
        elisp_form,
        expected,
    )
}

fn invalid_or_untranslatable_patterns_report_precise_validation_failures() -> ParityBatchCase {
    let elisp_form = r###"
(list
 :lookahead
 (neomacs-pcre2el-test-outcome
  (lambda () (rxt-pcre-to-elisp "(?=ready)ready")))
 :missing-close
 (neomacs-pcre2el-test-outcome
  (lambda () (rxt-pcre-to-elisp "(release-(\\d+)")))
 :infinite-language
 (neomacs-pcre2el-test-outcome
  (lambda () (rxt-pcre-to-strings "(?:blue|green)+")))
 :emacs-symbol-boundary
 (neomacs-pcre2el-test-outcome
  (lambda () (rxt-elisp-to-pcre "\\_<deploy-release\\_>")))
 :unterminated-literal
 (neomacs-pcre2el-test-outcome
  (lambda ()
    (with-temp-buffer
      (insert "m/release-(\\d+)")
      (goto-char (point-min))
      (rxt-read-delimited-pcre)))))
"###;
    let expected = expect![[
        r#"OK (:lookahead (:signal rxt-invalid-regexp :data ("Unrecognized PCRE extended construction `(?='") :message "Invalid regexp: \"Unrecognized PCRE extended construction `(?='\"") :missing-close (:signal rxt-invalid-regexp :data ("Subexpression missing close paren") :message "Invalid regexp: \"Subexpression missing close paren\"") :infinite-language (:signal error :data ("Can’t generate all productions of unbounded repeat \"(?:blue|green)+\"") :message "Can’t generate all productions of unbounded repeat \"(?:blue|green)+\"") :emacs-symbol-boundary (:signal rxt-invalid-regexp :data ("No PCRE translation of `\\_<'") :message "Invalid regexp: \"No PCRE translation of `\\\\_<'\"") :unterminated-literal (:signal search-failed :data ("[^\\]\\(/\\)") :message "Search failed: \"[^\\\\]\\\\(/\\\\)\""))"#
    ]];
    ParityBatchCase::value(
        "invalid_or_untranslatable_patterns_report_precise_validation_failures",
        elisp_form,
        expected,
    )
}

#[test]
fn pcre2el_package_batch() {
    assert_oracle_batch_cases(
        pcre2el_oracle(),
        "pcre2el-package-batch",
        "pcre2el",
        &[
            production_log_parser_translates_captures_and_rejects_malformed_deployments(),
            multiline_extended_incident_parser_preserves_lazy_capture_boundaries(),
            configuration_redaction_replaces_only_secret_values_across_case_and_separator_variants(
            ),
            command_vocabulary_round_trips_regexp_opt_and_enumerates_a_finite_release_matrix(),
            source_literal_reader_and_elisp_rx_toggle_drive_real_editing_workflows(),
            bounded_translation_cache_and_global_mode_keep_search_behavior_and_lifecycle_consistent(
            ),
            invalid_or_untranslatable_patterns_report_precise_validation_failures(),
        ],
    );
}
