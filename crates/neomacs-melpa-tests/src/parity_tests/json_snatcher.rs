use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, JSON_SNATCHER_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'json-snatcher)

(defun neomacs-json-snatcher-test-reset ()
  "Reset JSON Snatcher's process-wide parse cache."
  (setq jsons-curr-token 0
        jsons-curr-region nil
        jsons-parsed (make-hash-table :test #'equal)
        jsons-parsed-regions (make-hash-table :test #'equal)))

(defmacro neomacs-json-snatcher-test-with-json (json &rest body)
  "Evaluate BODY in a clean buffer containing JSON."
  `(progn
     (neomacs-json-snatcher-test-reset)
     (with-temp-buffer
       (insert ,json)
       (goto-char (point-min))
       ,@body)))

(defun neomacs-json-snatcher-test-goto (needle &optional offset)
  "Move inside NEEDLE by OFFSET characters from its beginning."
  (goto-char (point-min))
  (search-forward needle)
  (goto-char (+ (match-beginning 0) (or offset 1))))

(defun neomacs-json-snatcher-test-path-at (needle &optional offset)
  "Return JSON Snatcher's path at NEEDLE plus OFFSET."
  (neomacs-json-snatcher-test-goto needle offset)
  (jsons-get-path))

(defun neomacs-json-snatcher-test-capture-printer (printer)
  "Run PRINTER and capture its printed and kill-ring results."
  (let ((output (generate-new-buffer " *json-snatcher-output*")))
    (unwind-protect
        (let* ((standard-output output)
               (value (funcall printer)))
          (list :value value
                :output (with-current-buffer output (buffer-string))
                :kill (car kill-ring)))
      (kill-buffer output))))
"####;

fn nested_release_document_resolves_object_and_array_paths_from_one_parse() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-json-snatcher-test-with-json
 "{\n  \"release\": {\n    \"targets\": [\n      {\"name\": \"linux\", \"enabled\": true},\n      {\"name\": \"windows\", \"enabled\": false}\n    ],\n    \"retries\": -1.25e+2,\n    \"notes\": null\n  }\n}\n"
 (let* ((windows (neomacs-json-snatcher-test-path-at "\"windows\"" 2))
        (tree (gethash (current-buffer) jsons-parsed))
        (enabled (neomacs-json-snatcher-test-path-at "false" 2))
        (retries (neomacs-json-snatcher-test-path-at "-1.25e+2" 3))
        (notes (neomacs-json-snatcher-test-path-at "null" 2)))
   (list :paths (list windows enabled retries notes)
         :same-tree (eq tree (gethash (current-buffer) jsons-parsed))
         :trees (hash-table-count jsons-parsed)
         :region-caches (hash-table-count jsons-parsed-regions)
         :regions (length (gethash (current-buffer) jsons-parsed-regions)))))
"####;
    let expected = expect![[
        r#"OK (:paths (("\"name\"" . #1=(1 "\"targets\"" . #2=("\"release\""))) ("\"enabled\"" . #1#) ("\"retries\"" . #2#) ("\"notes\"" . #2#)) :same-tree t :trees 1 :region-caches 1 :regions 14)"#
    ]];
    ParityBatchCase::value(
        "nested_release_document_resolves_object_and_array_paths_from_one_parse",
        elisp_form,
        expected,
    )
}

fn python_and_jq_printers_copy_the_same_nested_path() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-json-snatcher-test-with-json
 "{\"pipelines\":[{\"name\":\"build\"},{\"name\":\"deploy\",\"status\":\"green\"}]}"
 (neomacs-json-snatcher-test-goto "\"green\"" 2)
 (let ((kill-ring nil)
       (kill-ring-yank-pointer nil))
   (let ((python (neomacs-json-snatcher-test-capture-printer
                  #'jsons-print-path-python))
         (jq (neomacs-json-snatcher-test-capture-printer
              #'jsons-print-path-jq)))
     (list :python python
           :jq jq
           :kill-ring kill-ring
           :default-printer jsons-path-printer))))
"####;
    let expected = expect![[
        r#"OK (:python (:value "[\"pipelines\"][1][\"status\"]" :output "[\"pipelines\"][1][\"status\"]" :kill "[\"pipelines\"][1][\"status\"]") :jq (:value ".pipelines[1].status" :output ".pipelines[1].status" :kill ".pipelines[1].status") :kill-ring (".pipelines[1].status" "[\"pipelines\"][1][\"status\"]") :default-printer jsons-print-path-python)"#
    ]];
    ParityBatchCase::value(
        "python_and_jq_printers_copy_the_same_nested_path",
        elisp_form,
        expected,
    )
}

fn root_arrays_numbers_literals_and_escaped_strings_keep_their_indices() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-json-snatcher-test-with-json
 "[\n  {\"kind\":\"measurement\",\"value\":-0.75e-3},\n  true,\n  false,\n  null,\n  \"deploy \\\"now\\\"\"\n]"
 (list
  :paths
  (list
   (neomacs-json-snatcher-test-path-at "-0.75e-3" 3)
   (neomacs-json-snatcher-test-path-at "true" 2)
   (neomacs-json-snatcher-test-path-at "false" 2)
   (neomacs-json-snatcher-test-path-at "null" 2)
   (neomacs-json-snatcher-test-path-at "deploy" 2))
  :numbers
  (mapcar (lambda (token) (not (null (jsons-is-number token))))
          '("0" "-0" "12" "-0.75" "6.02e23" "1e-9"
            "01" ".5" "1." "1e" "--2"))))
"####;
    let expected = expect![[
        r#"OK (:paths (("\"value\"" 0) (1) (2) (3) (4)) :numbers (t t t t t t nil nil nil nil nil))"#
    ]];
    ParityBatchCase::value(
        "root_arrays_numbers_literals_and_escaped_strings_keep_their_indices",
        elisp_form,
        expected,
    )
}

fn token_edges_keys_and_whitespace_distinguish_inside_from_outside() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-json-snatcher-test-with-json
 "{ \"service\" : { \"status\" : true, \"count\" : 3 } }"
 (let (inside-key inside-value at-start at-end whitespace)
   (neomacs-json-snatcher-test-goto "\"status\"" 2)
   (setq inside-key (jsons-get-path))
   (neomacs-json-snatcher-test-goto "true" 2)
   (let ((token-start (match-beginning 0))
         (token-end (match-end 0)))
     (setq inside-value (jsons-get-path))
     (goto-char token-start)
     (setq at-start (jsons-get-path))
     (goto-char token-end)
     (setq at-end (jsons-get-path)))
   (goto-char (point-min))
   (search-forward ": ")
   (backward-char 1)
   (setq whitespace (jsons-get-path))
   (list :inside-key inside-key
         :inside-value inside-value
         :at-start at-start
         :at-end at-end
         :whitespace whitespace)))
"####;
    let expected = expect![[
        r#"OK (:inside-key #1=("\"status\"" "\"service\"") :inside-value #1# :at-start nil :at-end nil :whitespace nil)"#
    ]];
    ParityBatchCase::value(
        "token_edges_keys_and_whitespace_distinguish_inside_from_outside",
        elisp_form,
        expected,
    )
}

fn per_buffer_caches_are_reused_and_removed_when_buffers_die() -> ParityBatchCase {
    let elisp_form = r####"
(progn
  (neomacs-json-snatcher-test-reset)
  (let ((first (generate-new-buffer " *json-snatcher-first*"))
        (second (generate-new-buffer " *json-snatcher-second*"))
        first-path second-path before after-first after-second)
    (unwind-protect
        (progn
          (with-current-buffer first
            (insert "{\"alpha\":{\"value\":100}}")
            (setq first-path
                  (neomacs-json-snatcher-test-path-at "100" 1)))
          (with-current-buffer second
            (insert "{\"beta\":[10,20,30]}")
            (setq second-path
                  (neomacs-json-snatcher-test-path-at "20" 1)))
          (setq before
                (list (hash-table-count jsons-parsed)
                      (hash-table-count jsons-parsed-regions)))
          (kill-buffer first)
          (setq after-first
                (list (hash-table-count jsons-parsed)
                      (hash-table-count jsons-parsed-regions)
                      (not (null (gethash second jsons-parsed)))))
          (kill-buffer second)
          (setq after-second
                (list (hash-table-count jsons-parsed)
                      (hash-table-count jsons-parsed-regions)))
          (list :paths (list first-path second-path)
                :before before
                :after-first after-first
                :after-second after-second))
      (when (buffer-live-p first) (kill-buffer first))
      (when (buffer-live-p second) (kill-buffer second)))))
"####;
    let expected = expect![[
        r#"OK (:paths (("\"value\"" "\"alpha\"") (1 "\"beta\"")) :before (2 2) :after-first (1 1 t) :after-second (0 0))"#
    ]];
    ParityBatchCase::value(
        "per_buffer_caches_are_reused_and_removed_when_buffers_die",
        elisp_form,
        expected,
    )
}

fn interactive_entrypoint_honors_a_user_selected_path_printer() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-json-snatcher-test-with-json
 "{\"release\":{\"artifact\":\"neomacs.tar.gz\"}}"
 (neomacs-json-snatcher-test-goto "neomacs.tar.gz" 3)
 (let ((jsons-path-printer
        (lambda ()
          (let ((path (reverse (jsons-get-path))))
            (format "ROOT/%s"
                    (mapconcat
                     (lambda (part)
                       (if (numberp part)
                           (number-to-string part)
                         (substring part 1 -1)))
                     path "/"))))))
   (list :interactive (commandp #'jsons-print-path)
         :printer-result (call-interactively #'jsons-print-path)
         :cache-count (hash-table-count jsons-parsed))))
"####;
    let expected =
        expect![[r#"OK (:interactive t :printer-result "ROOT/release/artifact" :cache-count 1)"#]];
    ParityBatchCase::value(
        "interactive_entrypoint_honors_a_user_selected_path_printer",
        elisp_form,
        expected,
    )
}

fn json_snatcher_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(JSON_SNATCHER_MELPA_PIN, "json-snatcher.el")
        .expect("prepare pinned JSON Snatcher source below ./tmp")
        .with_timeout(Duration::from_secs(120))
        .with_prelude(PRELUDE)
}

#[test]
fn json_snatcher_practical_workflows_batch() {
    let cases = vec![
        nested_release_document_resolves_object_and_array_paths_from_one_parse(),
        python_and_jq_printers_copy_the_same_nested_path(),
        root_arrays_numbers_literals_and_escaped_strings_keep_their_indices(),
        token_edges_keys_and_whitespace_distinguish_inside_from_outside(),
        per_buffer_caches_are_reused_and_removed_when_buffers_die(),
        interactive_entrypoint_honors_a_user_selected_path_printer(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("json-snatcher parity batch");
    assert_oracle_batch_cases(
        json_snatcher_oracle(),
        test_name,
        "json-snatcher parity",
        &cases,
    );
}
