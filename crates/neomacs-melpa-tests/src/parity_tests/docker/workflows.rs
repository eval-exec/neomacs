use expect_test::expect;

use super::ParityBatchCase;

fn human_size_parsing_and_ordering_are_deterministic() -> ParityBatchCase {
    ParityBatchCase::value(
        "human_size_parsing_and_ordering_are_deterministic",
        r####"
(list :bytes-1k (docker-utils-human-size-to-bytes "1KB")
      :bytes-2mb (docker-utils-human-size-to-bytes "2MB")
      :bytes-plain (docker-utils-human-size-to-bytes "512")
      :mult-gb (docker-utils-unit-multiplier "GB")
      :sorted
      (and (docker-utils-human-size-predicate "1KB" "2MB") t)
      :not-sorted
      (docker-utils-human-size-predicate "2MB" "1KB")
      :bad
      (condition-case err
          (progn (docker-utils-human-size-to-bytes "nope") :ok)
        (error (error-message-string err))))
"####,
        expect![[
            r#"OK (:bytes-1k 1024 :bytes-2mb 2097152 :bytes-plain 512 :mult-gb 1073741824 :sorted t :not-sorted nil :bad "Unexpected size format: nope")"#
        ]],
    )
}

fn buffer_names_and_format_helpers_are_stable() -> ParityBatchCase {
    ParityBatchCase::value(
        "buffer_names_and_format_helpers_are_stable",
        r####"
(let* ((name (docker-utils-generate-new-buffer-name "docker" "ps" "-a"))
       (cols '((:name "ID" :width 10 :template "{{ json .ID }}" :sort t :format nil)
               (:name "Image" :width 20 :template "{{ json .Image }}" :sort t :format nil)
               (:name "Status" :width 15 :template "{{ json .Status }}" :sort nil :format nil)))
       (fmt (docker-utils-columns-list-format cols))
       (line "[\"abc123\",\"nginx\",\"Up 2 hours\"]")
       (parsed (docker-utils-parse cols line))
       (bad-parse
        (condition-case err
            (progn (docker-utils-parse cols "not-json") :ok)
          (error (error-message-string err)))))
  (list :name name
        :fmt-names (mapcar #'car (append fmt nil))
        :fmt-widths (mapcar #'cadr (append fmt nil))
        :parsed-id (car parsed)
        :parsed-cols (append (cadr parsed) nil)
        :bad-parse bad-parse))
"####,
        expect![[
            r#"OK (:name "* docker ps -a *" :fmt-names ("ID" "Image" "Status") :fmt-widths (10 20 15) :parsed-id "abc123" :parsed-cols ("nginx" "Up 2 hours") :bad-parse "Unrecognized keyword: \"not\"")"#
        ]],
    )
}

fn compute_args_merges_default_and_custom_by_name() -> ParityBatchCase {
    ParityBatchCase::value(
        "compute_args_merges_default_and_custom_by_name",
        r####"
(cl-letf (((symbol-function 'tablist-get-marked-items)
           (lambda ()
             '(("nginx-web" . [nil])
               ("other" . [nil])))))
  (list :matched
        (docker-utils-compute-args
         '("-i")
         '(("nginx" ("-it"))
           ("db" ("-d"))))
        :default
        (docker-utils-compute-args
         '("-i")
         '(("db" ("-d"))))))
"####,
        expect![[r#"OK (:matched ("-it") :default ("-i"))"#]],
    )
}

fn ensure_items_errors_when_selection_empty() -> ParityBatchCase {
    ParityBatchCase::value(
        "ensure_items_errors_when_selection_empty",
        r####"
(cl-letf (((symbol-function 'tablist-get-marked-items)
           (lambda () nil)))
  (list :empty
        (condition-case err
            (progn (docker-utils-ensure-items) :ok)
          (error (error-message-string err)))
        :ok
        (cl-letf (((symbol-function 'tablist-get-marked-items)
                   (lambda () '(("cid" . [nil])))))
          (progn (docker-utils-ensure-items) :ok))))
"####,
        expect![[r#"OK (:empty "This action cannot be used in an empty list" :ok :ok)"#]],
    )
}

fn run_start_builds_command_and_terminal_backend_selects_shell() -> ParityBatchCase {
    ParityBatchCase::value(
        "run_start_builds_command_and_terminal_backend_selects_shell",
        r####"
(let ((docker-show-messages t)
      (messages nil)
      (started nil))
  (cl-letf (((symbol-function 'message)
             (lambda (fmt &rest args)
               (push (apply #'format fmt args) messages)
               nil))
            ((symbol-function 'start-file-process-shell-command)
             (lambda (name buffer command)
               (setq started (list :name name :buffer buffer :command command))
               (start-process "docker-fake" buffer "true"))))
    (docker-run-start-file-process-shell-command "docker" "ps" "-a")
    (list :started started
          :messages (nreverse messages)
          :backend-shell
          (let ((docker-terminal-backend 'shell))
            (docker--terminal-backend))
          :backend-auto
          (let ((docker-terminal-backend 'auto))
            (docker--terminal-backend))
          :shell-available
          (and (docker--terminal-backend-available-p 'shell) t))))
"####,
        expect![[
            r#"OK (:started (:name "docker ps -a" :buffer "* docker ps -a *" :command "docker ps -a") :messages ("Running: docker ps -a") :backend-shell shell :backend-auto shell :shell-available t)"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        human_size_parsing_and_ordering_are_deterministic(),
        buffer_names_and_format_helpers_are_stable(),
        compute_args_merges_default_and_custom_by_name(),
        ensure_items_errors_when_selection_empty(),
        run_start_builds_command_and_terminal_backend_selects_shell(),
    ]
}
