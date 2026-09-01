use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, PHP_RUNTIME_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PHP_RUNTIME_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const PHP_RUNTIME_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)
(require 'php-runtime)

(defun php-runtime-test-cli ()
  (let ((path
         (expand-file-name
          "fake-php"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
    (with-temp-file path
      (insert
       "#!/bin/sh\n"
       "arg=$1\n"
       "if [ -f \"$arg\" ]; then\n"
       "  printf 'MODE=file\\nSCRIPT<<'\n"
       "  cat \"$arg\"\n"
       "  printf '>>\\nSTDIN<<'\n"
       "  cat\n"
       "  printf '>>\\n'\n"
       "  exit 0\n"
       "fi\n"
       "case \"$arg\" in\n"
       "  \"-recho strtoupper('apple');\") printf 'APPLE' ;;\n"
       "  \"-recho extension_loaded('json');\") printf '1' ;;\n"
       "  \"-recho extension_loaded('imaginary_ext');\") printf '0' ;;\n"
       "  \"-rFAIL\")\n"
       "    printf 'partial-output\\n'\n"
       "    printf 'fatal: bad program\\n' >&2\n"
       "    exit 7\n"
       "    ;;\n"
       "  *)\n"
       "    code=$(printf '%s' \"$arg\" | cut -c3-)\n"
       "    printf 'MODE=inline\\nCODE<<%s>>\\nSTDIN<<' \"$code\"\n"
       "    cat\n"
       "    printf '>>\\n'\n"
       "    ;;\n"
       "esac\n"))
    (set-file-modes path #o700)
    path))

(defun php-runtime-test-buffer-string (buffer)
  (with-current-buffer buffer
    (buffer-substring-no-properties (point-min) (point-max))))

(defun php-runtime-test-reset-error-buffer ()
  (let ((buffer (get-buffer-create php-runtime-error-buffer-name)))
    (with-current-buffer buffer
      (erase-buffer))
    buffer))

(defun php-runtime-test-temporary-runtime-buffers ()
  (sort
   (mapcar
    #'buffer-name
    (seq-filter
     (lambda (buffer)
       (let ((name (buffer-name buffer)))
         (or (string-prefix-p "*PHP temp" name)
             (string-prefix-p "*PHP output" name))))
     (buffer-list)))
   #'string<))
"##;

fn php_runtime_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PHP_RUNTIME_MELPA_PIN, "php-runtime.el")
        .expect("prepare pinned PHP Runtime source below ./tmp")
        .with_prelude(PHP_RUNTIME_TEST_PRELUDE)
        .with_timeout(PHP_RUNTIME_TEST_TIMEOUT)
}

fn executor_configuration_preserves_php_code_quoting_and_file_stdin_contracts() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((input-file
        (expand-file-name
         "orders.ndjson"
         (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (code "echo process_order(STDIN);")
       (executor
        (php-runtime-execute
         :executable (php-runtime-test-cli)
         :code (cons :string code)
         :stdin (cons :file input-file)))
       (customer "C:\\imports\\O'Reilly\nλ"))
  (list
   :literal (php-runtime-quote-string customer)
   :alias (php-runtime-\' customer)
   :null-detection
   (mapcar #'php-runtime-string-has-null-byte
           (list customer (concat "before" (string 0) "after")))
   :command-line (php-runtime--get-command-line-arg executor)
   :stdin-by-file (php-runtime--stdin-by-file-p executor)
   :input (file-name-nondirectory (php-runtime--get-input executor))
   :slots
   (list :executable (file-name-nondirectory (oref executor executable))
         :code (oref executor code)
         :handler (eq (oref executor handler)
                      #'php-runtime-default-handler)
         :stdout (oref executor stdout)
         :stderr (oref executor stderr))))
"##;
    let expect = expect![[
        r####"OK (:literal "'C:\\\\imports\\\\O\\'Reilly\nλ'" :alias "'C:\\\\imports\\\\O\\'Reilly\nλ'" :null-detection (nil t) :command-line "-recho process_order(STDIN);" :stdin-by-file t :input "orders.ndjson" :slots (:executable "fake-php" :code (:string . "echo process_order(STDIN);") :handler t :stdout nil :stderr nil))"####
    ]];
    ParityBatchCase::value(
        "executor_configuration_preserves_php_code_quoting_and_file_stdin_contracts",
        elisp_form,
        expect,
    )
}

fn inline_evaluation_streams_string_stdin_and_reclaims_internal_buffers() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((php-runtime-php-executable (php-runtime-test-cli))
       (error-buffer (php-runtime-test-reset-error-buffer))
       (before (php-runtime-test-temporary-runtime-buffers))
       (result
        (php-runtime-eval
         "$i = 0; while ($line = fgets(STDIN)) { echo ++$i, trim($line); }"
         "apple\norange\nbanana\n"))
       (after (php-runtime-test-temporary-runtime-buffers)))
  (list
   :result result
   :temporary-buffers (list :before before :after after)
   :error-buffer
   (list :live (buffer-live-p error-buffer)
         :contents (php-runtime-test-buffer-string error-buffer))))
"##;
    let expect = expect![[
        r####"OK (:result "MODE=inline\nCODE<<$i = 0; while ($line = fgets(STDIN)) { echo ++$i, trim($line); }>>\nSTDIN<<apple\norange\nbanana\n>>\n" :temporary-buffers (:before nil :after nil) :error-buffer (:live t :contents ""))"####
    ]];
    ParityBatchCase::value(
        "inline_evaluation_streams_string_stdin_and_reclaims_internal_buffers",
        elisp_form,
        expect,
    )
}

fn caller_owned_input_and_output_buffers_survive_with_content_and_point_intact() -> ParityBatchCase
{
    let elisp_form = r##"
(let ((input (generate-new-buffer " *php-runtime-orders*"))
      (output (generate-new-buffer " *php-runtime-response*")))
  (unwind-protect
      (progn
        (with-current-buffer input
          (insert "order-17\norder-29\n")
          (goto-char 8))
        (with-current-buffer output
          (insert "prefix:")
          (goto-char (point-max)))
        (let ((php-runtime-php-executable (php-runtime-test-cli))
              (php-runtime--kill-temp-output-buffer nil))
          (let ((result
                 (php-runtime-eval
                  "while ($line = fgets(STDIN)) { dispatch(trim($line)); }"
                  input
                  output)))
            (list
             :result result
             :input
             (with-current-buffer input
               (list :live (buffer-live-p input)
                     :point (point)
                     :contents
                     (buffer-substring-no-properties
                      (point-min) (point-max))))
             :output
             (with-current-buffer output
               (list :live (buffer-live-p output)
                     :point (point)
                     :contents
                     (buffer-substring-no-properties
                      (point-min) (point-max))))))))
    (when (buffer-live-p input) (kill-buffer input))
    (when (buffer-live-p output) (kill-buffer output))))
"##;
    let expect = expect![[
        r####"OK (:result "prefix:MODE=inline\nCODE<<while ($line = fgets(STDIN)) { dispatch(trim($line)); }>>\nSTDIN<<order-17\norder-29\n>>\n" :input (:live t :point 8 :contents "order-17\norder-29\n") :output (:live t :point 112 :contents "prefix:MODE=inline\nCODE<<while ($line = fgets(STDIN)) { dispatch(trim($line)); }>>\nSTDIN<<order-17\norder-29\n>>\n"))"####
    ]];
    ParityBatchCase::value(
        "caller_owned_input_and_output_buffers_survive_with_content_and_point_intact",
        elisp_form,
        expect,
    )
}

fn file_stdin_is_forwarded_without_mutating_the_source_artifact() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
       (input-file (expand-file-name "events.jsonl" root))
       (php-runtime-php-executable (php-runtime-test-cli)))
  (with-temp-file input-file
    (insert "{\"type\":\"created\",\"id\":17}\n"
            "{\"type\":\"paid\",\"id\":17}\n"))
  (let ((before
         (with-temp-buffer
           (insert-file-contents-literally input-file)
           (buffer-string)))
        (result
         (php-runtime-eval
          "while ($event = fgets(STDIN)) { handle(json_decode($event)); }"
          (cons :file input-file))))
    (list
     :result result
     :source-unchanged
     (equal
      before
      (with-temp-buffer
        (insert-file-contents-literally input-file)
        (buffer-string)))
     :source before)))
"##;
    let expect = expect![[
        r####"OK (:result "MODE=inline\nCODE<<while ($event = fgets(STDIN)) { handle(json_decode($event)); }>>\nSTDIN<<{\"type\":\"created\",\"id\":17}\n{\"type\":\"paid\",\"id\":17}\n>>\n" :source-unchanged t :source "{\"type\":\"created\",\"id\":17}\n{\"type\":\"paid\",\"id\":17}\n")"####
    ]];
    ParityBatchCase::value(
        "file_stdin_is_forwarded_without_mutating_the_source_artifact",
        elisp_form,
        expect,
    )
}

fn nul_containing_code_uses_a_php_script_file_without_losing_binary_content() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
       (script (expand-file-name "request.php" root))
       (code (concat "$payload = \"left" (string 0) "right\";\necho strlen($payload);"))
       (php-runtime-php-executable (php-runtime-test-cli))
       (php-runtime--eval-temp-script-name script)
       (result (php-runtime-eval code))
       (saved
        (with-temp-buffer
          (insert-file-contents-literally script)
          (buffer-string))))
  (list
   :result result
   :script-file (file-name-nondirectory script)
   :saved saved
   :open-tag (string-prefix-p php-runtime-php-open-tag saved)
   :nul-offset (seq-position saved 0)
   :code-preserved
   (equal (substring saved (length php-runtime-php-open-tag)) code)))
"##;
    let expect = expect![[
        r####"OK (:result "MODE=file\nSCRIPT<<<?php $payload = \"left\0right\";\necho strlen($payload);>>\nSTDIN<<>>\n" :script-file "request.php" :saved "<?php $payload = \"left\0right\";\necho strlen($payload);" :open-tag t :nul-offset 22 :code-preserved t)"####
    ]];
    ParityBatchCase::value(
        "nul_containing_code_uses_a_php_script_file_without_losing_binary_content",
        elisp_form,
        expect,
    )
}

fn custom_executor_handler_observes_exit_status_output_and_buffer_routing() -> ParityBatchCase {
    let elisp_form = r##"
(let ((stdout (generate-new-buffer " *php-runtime-stdout*"))
      (stderr (generate-new-buffer " *php-runtime-stderr*")))
  (unwind-protect
      (let* ((executor
              (php-runtime-execute
               :executable (php-runtime-test-cli)
               :code (cons :string "FAIL")
               :stdout stdout
               :stderr stderr
               :handler
               (lambda (status output)
                 (list :status status
                       :output output
                       :stdout-live (buffer-live-p stdout)
                       :stderr-live (buffer-live-p stderr)))))
             (handled (php-runtime-process executor)))
        (list
         :handled handled
         :stdout (php-runtime-test-buffer-string stdout)
         :stderr (php-runtime-test-buffer-string stderr)
         :stdout-reused (eq stdout (php-runtime-stdout-buffer executor))
         :slots
         (list :stdout (eq stdout (oref executor stdout))
               :stderr (eq stderr (oref executor stderr)))))
    (when (buffer-live-p stdout) (kill-buffer stdout))
    (when (buffer-live-p stderr) (kill-buffer stderr))))
"##;
    let expect = expect![[
        r####"OK (:handled (:status 7 :output "partial-output\nfatal: bad program\n" :stdout-live t :stderr-live t) :stdout "partial-output\nfatal: bad program\n" :stderr "" :stdout-reused t :slots (:stdout t :stderr t))"####
    ]];
    ParityBatchCase::value(
        "custom_executor_handler_observes_exit_status_output_and_buffer_routing",
        elisp_form,
        expect,
    )
}

fn default_handler_surfaces_failed_process_output_and_cleans_temporary_stdout() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((php-runtime-php-executable (php-runtime-test-cli))
       (error-buffer (php-runtime-test-reset-error-buffer))
       (before (php-runtime-test-temporary-runtime-buffers))
       (failure
        (condition-case error
            (list :returned (php-runtime-eval "FAIL"))
          (error
           (list :signal (car error)
                 :data (cdr error)
                 :message (error-message-string error)))))
       (after (php-runtime-test-temporary-runtime-buffers)))
  (list
   :failure failure
   :temporary-buffers (list :before before :after after)
   :error-buffer (php-runtime-test-buffer-string error-buffer)))
"##;
    let expect = expect![[
        r####"OK (:failure (:signal error :data ("partial-output\nfatal: bad program\n") :message "partial-output\nfatal: bad program\n") :temporary-buffers (:before nil :after nil) :error-buffer "")"####
    ]];
    ParityBatchCase::value(
        "default_handler_surfaces_failed_process_output_and_cleans_temporary_stdout",
        elisp_form,
        expect,
    )
}

fn expression_and_extension_wrappers_construct_executable_php_queries() -> ParityBatchCase {
    let elisp_form = r##"
(let ((php-runtime-php-executable (php-runtime-test-cli)))
  (list
   :expression (php-runtime-expr "strtoupper('apple')")
   :loaded (php-runtime-extension-loaded-p "json")
   :missing (php-runtime-extension-loaded-p "imaginary_ext")
   :quoted-query
   (format "extension_loaded(%s)"
           (php-runtime-quote-string "opcache\\'prod"))))
"##;
    let expect = expect![[
        r####"OK (:expression "APPLE" :loaded t :missing nil :quoted-query "extension_loaded('opcache\\\\\\'prod')")"####
    ]];
    ParityBatchCase::value(
        "expression_and_extension_wrappers_construct_executable_php_queries",
        elisp_form,
        expect,
    )
}

#[test]
fn php_runtime_package_batch() {
    let cases = vec![
        executor_configuration_preserves_php_code_quoting_and_file_stdin_contracts(),
        inline_evaluation_streams_string_stdin_and_reclaims_internal_buffers(),
        caller_owned_input_and_output_buffers_survive_with_content_and_point_intact(),
        file_stdin_is_forwarded_without_mutating_the_source_artifact(),
        nul_containing_code_uses_a_php_script_file_without_losing_binary_content(),
        custom_executor_handler_observes_exit_status_output_and_buffer_routing(),
        default_handler_surfaces_failed_process_output_and_cleans_temporary_stdout(),
        expression_and_extension_wrappers_construct_executable_php_queries(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed PHP Runtime parity test");
    assert_oracle_batch_cases(
        php_runtime_oracle(),
        test_name,
        "php_runtime_parity",
        &cases,
    );
}
