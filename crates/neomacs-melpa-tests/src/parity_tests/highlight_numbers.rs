use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, HIGHLIGHT_NUMBERS_MELPA_PIN, PARENT_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const HIGHLIGHT_NUMBERS_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const HIGHLIGHT_NUMBERS_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'cc-mode)
(require 'conf-mode)
(require 'scheme)
(require 'highlight-numbers)

(define-derived-mode neomacs-hnums-test-metrics-mode conf-mode "Metrics")
(define-derived-mode neomacs-hnums-test-service-metrics-mode
  neomacs-hnums-test-metrics-mode "Service-Metrics")
(define-derived-mode neomacs-hnums-test-disabled-metrics-mode
  neomacs-hnums-test-metrics-mode "Disabled-Metrics")
(define-derived-mode neomacs-hnums-test-generic-mode conf-mode "Generic-Numbers")

(defun neomacs-hnums-test-target-face-p (face)
  "Return non-nil when FACE includes the package's numeric face."
  (or (eq face 'highlight-numbers-number)
      (and (listp face)
           (memq 'highlight-numbers-number face))))

(defun neomacs-hnums-test-face-runs ()
  "Return exact spans carrying `highlight-numbers-number'."
  (font-lock-fontify-region (point-min) (point-max))
  (let ((position (point-min))
        (end (point-max))
        runs)
    (while (< position end)
      (let* ((face (get-text-property position 'face))
             (next (next-single-property-change position 'face nil end)))
        (when (neomacs-hnums-test-target-face-p face)
          (push (list :start position
                      :end next
                      :text (buffer-substring-no-properties position next)
                      :face face)
                runs))
        (setq position next)))
    (nreverse runs)))

(defun neomacs-hnums-test-fontify (mode contents)
  "Fontify CONTENTS in MODE and return stable package state and face spans."
  (with-temp-buffer
    (insert contents)
    (funcall mode)
    (font-lock-mode 1)
    (highlight-numbers-mode 1)
    (list :major-mode major-mode
          :enabled highlight-numbers-mode
          :keyword-local (local-variable-p 'highlight-numbers--keywords)
          :runs (neomacs-hnums-test-face-runs))))

(defun neomacs-hnums-test-fontify-with-samples (mode contents samples)
  "Fontify CONTENTS in MODE and report package spans plus SAMPLE faces."
  (with-temp-buffer
    (insert contents)
    (funcall mode)
    (font-lock-mode 1)
    (highlight-numbers-mode 1)
    (let ((runs (neomacs-hnums-test-face-runs)))
      (list :major-mode major-mode
            :enabled highlight-numbers-mode
            :keyword-local (local-variable-p 'highlight-numbers--keywords)
            :runs runs
            :samples
            (mapcar
             (lambda (sample)
               (goto-char (point-min))
               (search-forward sample)
               (let ((start (- (point) (length sample))))
                 (list :start start
                       :text sample
                       :face (get-text-property start 'face))))
             samples)))))

(defun neomacs-hnums-test-lifecycle-state ()
  "Return the observable mode, keyword, and fontification state."
  (list :enabled highlight-numbers-mode
        :keyword-local (local-variable-p 'highlight-numbers--keywords)
        :keyword-count (length highlight-numbers--keywords)
        :runs (neomacs-hnums-test-face-runs)))
"####;

fn highlight_numbers_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HIGHLIGHT_NUMBERS_MELPA_PIN, "highlight-numbers.el")
        .expect("prepare revision-pinned Highlight Numbers source below ./tmp")
        .with_melpa_dependency(PARENT_MODE_MELPA_PIN)
        .expect("prepare revision-pinned Parent Mode dependency below ./tmp")
        .with_prelude(HIGHLIGHT_NUMBERS_TEST_PRELUDE)
        .with_timeout(HIGHLIGHT_NUMBERS_TEST_TIMEOUT)
}

fn emacs_lisp_configuration_distinguishes_literals_from_prose() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-hnums-test-fontify
 'emacs-lisp-mode
 "(setq retries 3\n      threshold -12.5e+2\n      mask #xFF\n      bits #b1011\n      octal #o77\n      ratio 3/4\n      build42 7)\n(message \"retry 99\") ; ticket 123\n")
"####;
    let expected = expect![[
        r##"OK (:major-mode emacs-lisp-mode :enabled t :keyword-local t :runs ((:start 15 :end 16 :text "3" :face highlight-numbers-number) (:start 33 :end 41 :text "-12.5e+2" :face highlight-numbers-number) (:start 53 :end 57 :text "#xFF" :face highlight-numbers-number) (:start 69 :end 75 :text "#b1011" :face highlight-numbers-number) (:start 88 :end 92 :text "#o77" :face highlight-numbers-number) (:start 123 :end 124 :text "7" :face highlight-numbers-number)))"##
    ]];
    ParityBatchCase::value(
        "emacs_lisp_configuration_distinguishes_literals_from_prose",
        elisp_form,
        expected,
    )
}

fn scheme_numeric_workload_covers_radix_ratio_and_float_forms() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-hnums-test-fontify-with-samples
 'scheme-mode
 "(define samples (list -3 1/2 .75 6.02e23 #xFF #b1010))\n(display \"version 42\") ; issue 77\n"
 '("#xFF" "#b1010" "\"version 42\"" "; issue 77"))
"####;
    let expected = expect![[
        r##"OK (:major-mode scheme-mode :enabled t :keyword-local t :runs ((:start 23 :end 25 :text "-3" :face highlight-numbers-number) (:start 26 :end 29 :text "1/2" :face highlight-numbers-number) (:start 30 :end 33 :text ".75" :face highlight-numbers-number) (:start 34 :end 41 :text "6.02e23" :face highlight-numbers-number)) :samples ((:start 42 :text "#xFF" :face nil) (:start 47 :text "#b1010" :face nil) (:start 65 :text "\"version 42\"" :face font-lock-string-face) (:start 79 :text "; issue 77" :face font-lock-comment-delimiter-face)))"##
    ]];
    ParityBatchCase::value(
        "scheme_numeric_workload_covers_radix_ratio_and_float_forms",
        elisp_form,
        expected,
    )
}

fn c_family_build_constants_respect_suffix_and_token_rules() -> ParityBatchCase {
    let elisp_form = r####"
(list
 :c
 (neomacs-hnums-test-fontify
  'c-mode
  "unsigned retries = 42U;\nlong mask = 0x1AfL;\nfloat ratio = 3.14e-2F;\ndouble leading = .5;\nint value2 = 0755;\nconst char *label = \"release 99\"; // ticket 123\n")
 :cpp
 (neomacs-hnums-test-fontify
  'c++-mode
  "auto distance = 42_km;\nauto timeout = 250ms;\nauto mask = 0XCAFEULL;\nauto ratio = 6.02e23L;\n// release 314\n"))
"####;
    let expected = expect![[
        r#"OK (:c (:major-mode c-mode :enabled t :keyword-local t :runs ((:start 20 :end 23 :text "42U" :face highlight-numbers-number) (:start 37 :end 43 :text "0x1AfL" :face highlight-numbers-number) (:start 59 :end 67 :text "3.14e-2F" :face highlight-numbers-number) (:start 87 :end 88 :text "5" :face highlight-numbers-number) (:start 103 :end 107 :text "0755" :face highlight-numbers-number))) :cpp (:major-mode c++-mode :enabled t :keyword-local t :runs ((:start 17 :end 22 :text "42_km" :face highlight-numbers-number) (:start 39 :end 44 :text "250ms" :face highlight-numbers-number) (:start 58 :end 67 :text "0XCAFEULL" :face highlight-numbers-number) (:start 82 :end 90 :text "6.02e23L" :face highlight-numbers-number))))"#
    ]];
    ParityBatchCase::value(
        "c_family_build_constants_respect_suffix_and_token_rules",
        elisp_form,
        expected,
    )
}

fn inherited_custom_rules_and_explicit_opt_out_drive_service_logs() -> ParityBatchCase {
    let elisp_form = r####"
(let ((original-modelist highlight-numbers-modelist))
  (unwind-protect
      (progn
        (setq highlight-numbers-modelist
              (copy-hash-table original-modelist))
        (puthash 'neomacs-hnums-test-metrics-mode
                 (rx symbol-start (+ digit) (or "ms" "s") symbol-end)
                 highlight-numbers-modelist)
        (puthash 'neomacs-hnums-test-disabled-metrics-mode
                 'do-not-use
                 highlight-numbers-modelist)
        (list
         :inherited
         (neomacs-hnums-test-fontify
          'neomacs-hnums-test-service-metrics-mode
          "latency=250ms timeout=30s retries=4 build=v12\n")
         :disabled
         (neomacs-hnums-test-fontify
          'neomacs-hnums-test-disabled-metrics-mode
          "latency=250ms timeout=30s retries=4\n")
         :generic
         (neomacs-hnums-test-fontify
          'neomacs-hnums-test-generic-mode
          "batch=12items shard=7 build=v42 plain=003\n")))
    (setq highlight-numbers-modelist original-modelist)))
"####;
    let expected = expect![[
        r#"OK (:inherited (:major-mode neomacs-hnums-test-service-metrics-mode :enabled t :keyword-local t :runs ((:start 9 :end 14 :text "250ms" :face highlight-numbers-number) (:start 23 :end 26 :text "30s" :face highlight-numbers-number))) :disabled (:major-mode neomacs-hnums-test-disabled-metrics-mode :enabled t :keyword-local nil :runs nil) :generic (:major-mode neomacs-hnums-test-generic-mode :enabled t :keyword-local t :runs ((:start 7 :end 14 :text "12items" :face highlight-numbers-number) (:start 21 :end 22 :text "7" :face highlight-numbers-number) (:start 39 :end 42 :text "003" :face highlight-numbers-number))))"#
    ]];
    ParityBatchCase::value(
        "inherited_custom_rules_and_explicit_opt_out_drive_service_logs",
        elisp_form,
        expected,
    )
}

fn editing_and_mode_lifecycle_refontify_without_stale_number_faces() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (insert "retry_limit = 3\ntimeout_seconds = 30\n")
  (neomacs-hnums-test-generic-mode)
  (font-lock-mode 1)
  (highlight-numbers-mode 1)
  (let ((initial (neomacs-hnums-test-lifecycle-state))
        edited disabled reenabled)
    (goto-char (point-min))
    (search-forward "3")
    (replace-match "12" t t)
    (font-lock-flush (line-beginning-position) (line-end-position))
    (setq edited (neomacs-hnums-test-lifecycle-state))
    (highlight-numbers-mode -1)
    (setq disabled (neomacs-hnums-test-lifecycle-state))
    (highlight-numbers-mode 1)
    (setq reenabled (neomacs-hnums-test-lifecycle-state))
    (list :text (buffer-string)
          :initial initial
          :edited edited
          :disabled disabled
          :reenabled reenabled)))
"####;
    let expected = expect![[
        r#"OK (:text #("retry_limit = 12\ntimeout_seconds = 30\n" 0 11 (face font-lock-variable-name-face) 14 16 (face highlight-numbers-number) 17 32 (face font-lock-variable-name-face) 35 37 (face highlight-numbers-number)) :initial (:enabled t :keyword-local t :keyword-count 1 :runs ((:start 15 :end 16 :text "3" :face highlight-numbers-number) (:start 35 :end 37 :text "30" :face highlight-numbers-number))) :edited (:enabled t :keyword-local t :keyword-count 1 :runs ((:start 15 :end 17 :text "12" :face highlight-numbers-number) (:start 36 :end 38 :text "30" :face highlight-numbers-number))) :disabled (:enabled nil :keyword-local nil :keyword-count 0 :runs nil) :reenabled (:enabled t :keyword-local t :keyword-count 1 :runs ((:start 15 :end 17 :text "12" :face highlight-numbers-number) (:start 36 :end 38 :text "30" :face highlight-numbers-number))))"#
    ]];
    ParityBatchCase::value(
        "editing_and_mode_lifecycle_refontify_without_stale_number_faces",
        elisp_form,
        expected,
    )
}

#[test]
fn highlight_numbers_package_batch() {
    let cases = vec![
        emacs_lisp_configuration_distinguishes_literals_from_prose(),
        scheme_numeric_workload_covers_radix_ratio_and_float_forms(),
        c_family_build_constants_respect_suffix_and_token_rules(),
        inherited_custom_rules_and_explicit_opt_out_drive_service_logs(),
        editing_and_mode_lifecycle_refontify_without_stale_number_faces(),
    ];
    assert_oracle_batch_cases(
        highlight_numbers_oracle(),
        "highlight-numbers-package-batch",
        "highlight-numbers",
        &cases,
    );
}
