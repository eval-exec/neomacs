use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, EVIL_MELPA_PIN, EVIL_NUMBERS_MELPA_PIN, SHIFT_NUMBER_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const EVIL_NUMBERS_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const EVIL_NUMBERS_TEST_PRELUDE: &str = r####"
(require 'cl-lib)

(defun neomacs-evil-numbers-location ()
  "Describe point exactly after a public evil-numbers operation."
  (list :point (point)
        :line (line-number-at-pos)
        :column (current-column)
        :character (and (char-after) (char-to-string (char-after)))))
"####;

fn evil_numbers_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EVIL_NUMBERS_MELPA_PIN, "evil-numbers.el")
        .expect("prepare revision-pinned evil-numbers source below ./tmp")
        .with_melpa_dependency(EVIL_MELPA_PIN)
        .expect("prepare revision-pinned Evil dependency below ./tmp")
        .with_melpa_dependency(SHIFT_NUMBER_MELPA_PIN)
        .expect("prepare revision-pinned shift-number dependency below ./tmp")
        .with_prelude(EVIL_NUMBERS_TEST_PRELUDE)
        .with_timeout(EVIL_NUMBERS_TEST_TIMEOUT)
}

fn release_manifest_edits_mixed_radices_padding_signs_and_cursor_positions() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (let ((evil-numbers-pad-default nil)
        (evil-numbers-separator-chars nil)
        (evil-numbers-case nil)
        (evil-numbers-negative t)
        (evil-numbers-use-cursor-at-end-of-number nil)
        operations)
    (insert "# Release 42 capacity manifest\n"
            "workers = 0099\n"
            "retries = -01\n"
            "features = 0b0111\n"
            "permissions = 0o077\n"
            "mask = 0x00ff\n"
            "channel = stable\n")
    (set-buffer-modified-p nil)
    (dolist (operation
             '(("workers = " evil-numbers/inc-at-pt 3)
               ("retries = " evil-numbers/dec-at-pt 4)
               ("features = " evil-numbers/inc-at-pt 1)
               ("permissions = " evil-numbers/dec-at-pt 1)
               ("mask = " evil-numbers/inc-at-pt 2)))
      (goto-char (point-min))
      (search-forward (nth 0 operation))
      (let ((result (funcall (nth 1 operation) (nth 2 operation) nil nil nil)))
        (push (list :field (nth 0 operation)
                    :result result
                    :location (neomacs-evil-numbers-location))
              operations)))
    (list :operations (nreverse operations)
          :text (buffer-string)
          :modified (buffer-modified-p))))
"####;
    let expected = expect![[
        r##"OK (:operations ((:field "workers = " :result (:marker nil nil) :location (:point 45 :line 2 :column 13 :character "2")) (:field "retries = " :result (:marker nil nil) :location (:point 59 :line 3 :column 12 :character "5")) (:field "features = " :result (:marker nil nil) :location (:point 77 :line 4 :column 16 :character "0")) (:field "permissions = " :result (:marker nil nil) :location (:point 97 :line 5 :column 18 :character "6")) (:field "mask = " :result (:marker nil nil) :location (:point 111 :line 6 :column 12 :character "1"))) :text "# Release 42 capacity manifest\nworkers = 0102\nretries = -05\nfeatures = 0b1000\npermissions = 0o076\nmask = 0x0101\nchannel = stable\n" :modified t)"##
    ]];
    ParityBatchCase::value(
        "release_manifest_edits_mixed_radices_padding_signs_and_cursor_positions",
        elisp_form,
        expected,
    )
}

fn telemetry_literals_preserve_grouping_unicode_scripts_and_hex_policy() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (let ((evil-numbers-pad-default t)
        (evil-numbers-separator-chars "_,")
        (evil-numbers-case 'upcase)
        (evil-numbers-negative t)
        operations)
    (insert "requests=9_999_998\n"
            "bytes=1,099\n"
            "latency_scale=10⁻³\n"
            "water_molecules=H₂O\n"
            "feature_mask=0x00af\n")
    (dolist (operation
             '(("requests=" evil-numbers/inc-at-pt 5)
               ("bytes=" evil-numbers/inc-at-pt 901)
               ("latency_scale=10" evil-numbers/inc-at-pt 2)
               ("water_molecules=H" evil-numbers/dec-at-pt 1)
               ("feature_mask=" evil-numbers/inc-at-pt 2)))
      (goto-char (point-min))
      (search-forward (nth 0 operation))
      (let ((result (funcall (nth 1 operation) (nth 2 operation) nil nil nil)))
        (push (list :field (nth 0 operation)
                    :result result
                    :location (neomacs-evil-numbers-location))
              operations)))
    (list :operations (nreverse operations)
          :text (buffer-string)
          :point (point)
          :modified (buffer-modified-p))))
"####;
    let expected = expect![[
        r#"OK (:operations ((:field "requests=" :result (:marker nil nil) :location (:point 19 :line 1 :column 18 :character "3")) (:field "bytes=" :result (:marker nil nil) :location (:point 31 :line 2 :column 10 :character "0")) (:field "latency_scale=10" :result (:marker nil nil) :location (:point 50 :line 3 :column 17 :character "⁵")) (:field "water_molecules=H" :result (:marker nil nil) :location (:point 69 :line 4 :column 17 :character "₁")) (:field "feature_mask=" :result (:marker nil nil) :location (:point 90 :line 5 :column 18 :character "1"))) :text "requests=10_000_003\nbytes=2,000\nlatency_scale=10⁻⁵\nwater_molecules=H₁O\nfeature_mask=0x00B1\n" :point 90 :modified t)"#
    ]];
    ParityBatchCase::value(
        "telemetry_literals_preserve_grouping_unicode_scripts_and_hex_policy",
        elisp_form,
        expected,
    )
}

fn linewise_incremental_edit_assigns_progressive_deployment_values() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (let ((evil-numbers-pad-default nil)
        beg end result)
    (insert "node-a replicas=01 port=8000 shard=0\n"
            "node-b replicas=01 port=8000 shard=0\n"
            "node-c replicas=01 port=8000 shard=0\n"
            "footer: generated values stop above this line\n")
    (goto-char (point-min))
    (setq beg (line-beginning-position))
    (forward-line 2)
    (setq end (line-end-position))
    (goto-char beg)
    (set-mark end)
    (activate-mark)
    (setq result
          (evil-numbers/inc-at-pt-incremental
           2 beg end 'line '(t)))
    (list :return result
          :selection (list beg end (point) (mark t) (region-active-p))
          :location (neomacs-evil-numbers-location)
          :text (buffer-string))))
"####;
    let expected = expect![[
        r#"OK (:return (:marker nil nil) :selection (1 111 1 113 t) :location (:point 1 :line 1 :column 0 :character "n") :text "node-a replicas=03 port=8004 shard=6\nnode-b replicas=09 port=8010 shard=12\nnode-c replicas=15 port=8016 shard=18\nfooter: generated values stop above this line\n")"#
    ]];
    ParityBatchCase::value(
        "linewise_incremental_edit_assigns_progressive_deployment_values",
        elisp_form,
        expected,
    )
}

fn blockwise_incremental_edit_changes_only_the_replica_column() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (let ((evil-numbers-pad-default nil)
        beg end result)
    (insert "zone  replicas  port   budget\n"
            "east  007       8100   50\n"
            "west  012       8200   60\n"
            "edge  099       8300   70\n")
    (goto-char (point-min))
    (search-forward "007")
    (setq beg (- (point) 3))
    (search-forward "099")
    ;; `evil-apply-on-block' treats the end column as exclusive when it
    ;; constructs each per-line region, so retain the position after 099.
    (setq end (point))
    (goto-char beg)
    (set-mark end)
    (activate-mark)
    (setq result
          (evil-numbers/inc-at-pt-incremental
           3 beg end 'block '(t)))
    (list :return result
          :selection (list beg end (point) (mark t) (region-active-p))
          :location (neomacs-evil-numbers-location)
          :text (buffer-string))))
"####;
    let expected = expect![[
        r#"OK (:return (:marker nil nil) :selection (37 92 37 92 t) :location (:point 37 :line 2 :column 6 :character "0") :text "zone  replicas  port   budget\neast  010       8100   50\nwest  018       8200   60\nedge  108       8300   70\n")"#
    ]];
    ParityBatchCase::value(
        "blockwise_incremental_edit_changes_only_the_replica_column",
        elisp_form,
        expected,
    )
}

fn search_boundaries_cursor_end_and_negative_policy_match_interactive_editing() -> ParityBatchCase {
    let elisp_form = r####"
(let (blank-line same-line cursor-end negative-enabled negative-disabled messages)
  (cl-letf (((symbol-function 'message)
             (lambda (format-string &rest arguments)
               (push (apply #'format format-string arguments) messages))))
    (setq blank-line
          (with-temp-buffer
            (insert "release ready\nnext=41\n")
            (goto-char (point-min))
            (search-forward "ready")
            (let ((before (point))
                  (result (evil-numbers/inc-at-pt 1 nil nil nil)))
              (list :return result :before before :after (point)
                    :text (buffer-string)))))
    (setq same-line
          (with-temp-buffer
            (insert "release candidate=41 status=ready")
            (goto-char (point-min))
            (let ((result (evil-numbers/inc-at-pt 1 nil nil nil)))
              (list :return result
                    :location (neomacs-evil-numbers-location)
                    :text (buffer-string)))))
    (setq cursor-end
          (with-temp-buffer
            (insert "release=41)")
            (goto-char (point-min))
            (search-forward "41")
            (let* ((before (point))
                   (evil-numbers-use-cursor-at-end-of-number nil)
                   (disabled (evil-numbers/inc-at-pt 1 nil nil nil))
                   (after-disabled (point))
                   (evil-numbers-use-cursor-at-end-of-number t)
                   (enabled (evil-numbers/inc-at-pt 1 nil nil nil)))
              (list :before before
                    :disabled (list disabled after-disabled)
                    :enabled (list enabled (neomacs-evil-numbers-location))
                    :text (buffer-string)))))
    (setq negative-enabled
          (with-temp-buffer
            (let ((evil-numbers-negative t))
              (insert "release_delta=-5")
              (goto-char (point-min))
              (search-forward "=")
              (let ((result (evil-numbers/inc-at-pt 2 nil nil nil)))
                (list :return result
                      :location (neomacs-evil-numbers-location)
                      :text (buffer-string))))))
    (setq negative-disabled
          (with-temp-buffer
            (let ((evil-numbers-negative nil))
              (insert "release_delta=-5")
              (goto-char (point-min))
              (search-forward "=")
              (let ((result (evil-numbers/inc-at-pt 2 nil nil nil)))
                (list :return result
                      :location (neomacs-evil-numbers-location)
                      :text (buffer-string)))))))
  (list :blank-line blank-line
        :same-line same-line
        :cursor-end cursor-end
        :negative-enabled negative-enabled
        :negative-disabled negative-disabled
        :messages (nreverse messages)))
"####;
    let expected = expect![[
        r#"OK (:blank-line (:return (:marker nil nil) :before 14 :after 14 :text "release ready\nnext=41\n") :same-line (:return (:marker nil nil) :location (:point 20 :line 1 :column 19 :character "2") :text "release candidate=42 status=ready") :cursor-end (:before 11 :disabled ((:marker nil nil) 11) :enabled ((:marker nil nil) (:point 10 :line 1 :column 9 :character "2")) :text "release=42)") :negative-enabled (:return (:marker nil nil) :location (:point 16 :line 1 :column 15 :character "3") :text "release_delta=-3") :negative-disabled (:return (:marker nil nil) :location (:point 16 :line 1 :column 15 :character "7") :text "release_delta=-7") :messages ("No number at point or until end of line" "No number at point or until end of line"))"#
    ]];
    ParityBatchCase::value(
        "search_boundaries_cursor_end_and_negative_policy_match_interactive_editing",
        elisp_form,
        expected,
    )
}

#[test]
fn evil_numbers_package_batch() {
    let cases = vec![
        release_manifest_edits_mixed_radices_padding_signs_and_cursor_positions(),
        telemetry_literals_preserve_grouping_unicode_scripts_and_hex_policy(),
        linewise_incremental_edit_assigns_progressive_deployment_values(),
        blockwise_incremental_edit_changes_only_the_replica_column(),
        search_boundaries_cursor_end_and_negative_policy_match_interactive_editing(),
    ];
    assert_oracle_batch_cases(
        evil_numbers_oracle(),
        "evil-numbers-package-batch",
        "evil-numbers",
        &cases,
    );
}
