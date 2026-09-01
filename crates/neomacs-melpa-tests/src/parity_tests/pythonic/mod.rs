use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, F_MELPA_PIN, PYTHONIC_MELPA_PIN, S_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PYTHONIC_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const PYTHONIC_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'pythonic)

(defun pythonic-test-normalize-path (path)
  (directory-file-name path))

(defun pythonic-test-same-path-p (left right)
  (equal
   (pythonic-test-normalize-path left)
   (pythonic-test-normalize-path right)))

(defun pythonic-test-ancestor-path-p
    (ancestor descendant)
  (let ((ancestor
         (file-name-as-directory
          (pythonic-test-normalize-path
           ancestor)))
        (descendant
         (file-name-as-directory
          (pythonic-test-normalize-path
           descendant))))
    (and
     (not (equal ancestor descendant))
     (string-prefix-p ancestor descendant))))

(defun pythonic-test-process-summary (process)
  (list
   :name (process-name process)
   :status (process-status process)
   :exit-status (process-exit-status process)
   :query-on-exit
   (process-query-on-exit-flag process)
   :command (process-command process)))
"##;

fn pythonic_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PYTHONIC_MELPA_PIN, "pythonic.el")
        .expect("prepare pinned pythonic source below ./tmp")
        .with_melpa_dependency(F_MELPA_PIN)
        .expect("prepare pinned f dependency")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare pinned s dependency")
        .with_prelude(PYTHONIC_TEST_PRELUDE)
        .with_timeout(PYTHONIC_TEST_TIMEOUT)
}

fn tramp_connection_profiles_report_method_user_host_port_and_platform_predicates()
-> ParityBatchCase {
    let elisp_form = r##"
(mapcar
 (lambda (directory)
   (let ((default-directory directory)
         (pythonic-directory-aliases nil))
     (list
      :directory directory
      :local (pythonic-local-p)
      :remote (pythonic-remote-p)
      :docker
      (and (pythonic-remote-docker-p) t)
      :ssh
      (and (pythonic-remote-ssh-p) t)
      :vagrant
      (and (pythonic-remote-vagrant-p) t)
      :method
      (and (pythonic-remote-p)
           (pythonic-remote-method))
      :user
      (and (pythonic-remote-p)
           (pythonic-remote-user))
      :host
      (and (pythonic-remote-p)
           (pythonic-remote-host))
      :port
      (and (pythonic-remote-p)
           (pythonic-remote-port)))))
 '("/workspace/project/"
   "/ssh:deploy@build.example#2222:/srv/neomacs/"
   "/sshx:vagrant@localhost:/vagrant/app/"
   "/docker:root@neomacs-ci:/workspace/"))
"##;
    let expect = expect![[
        r##"OK ((:directory "/workspace/project/" :local t :remote nil :docker nil :ssh nil :vagrant nil :method nil :user nil :host nil :port nil) (:directory "/ssh:deploy@build.example#2222:/srv/neomacs/" :local nil :remote t :docker nil :ssh t :vagrant nil :method "ssh" :user "deploy" :host "build.example" :port 2222) (:directory "/sshx:vagrant@localhost:/vagrant/app/" :local nil :remote t :docker nil :ssh t :vagrant t :method "sshx" :user "vagrant" :host "localhost" :port nil) (:directory "/docker:root@neomacs-ci:/workspace/" :local nil :remote t :docker t :ssh nil :vagrant nil :method "docker" :user "root" :host "neomacs-ci" :port nil))"##
    ]];
    ParityBatchCase::value(
        "tramp_connection_profiles_report_method_user_host_port_and_platform_predicates",
        elisp_form,
        expect,
    )
}

fn directory_aliases_translate_nested_workspace_paths_in_both_directions() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((root
        (make-temp-file
         "pythonic-alias-workspace-" t))
       (source
        (expand-file-name "services/api/" root))
       (nested
        (expand-file-name
         "src/neomacs_api/worker.py"
         source))
       (mount
        "/docker:root@neomacs-api:/workspace/api")
       (pythonic-directory-aliases
        (list (list source mount))))
  (unwind-protect
      (progn
        (make-directory
         (file-name-directory nested) t)
        (write-region
         "print('worker')\n" nil nested nil 'silent)
        (let ((aliased
               (pythonic-aliased-path nested))
              (unaliased
               (cl-letf
                   (((symbol-function 'f-same-p)
                     #'pythonic-test-same-path-p)
                    ((symbol-function
                      'f-ancestor-of-p)
                     #'pythonic-test-ancestor-path-p))
                 (pythonic-unaliased-path
                  "/docker:root@neomacs-api:/workspace/api/src/neomacs_api/worker.py"))))
          (list
           :root-aliased
           (pythonic-aliased-path source)
           :nested-aliased aliased
           :round-trip-relative
           (file-relative-name unaliased root)
           :has-alias
           (list
            (pythonic-has-alias-p source)
            (pythonic-has-alias-p nested)
            (pythonic-has-alias-p root))
           :outside
           (pythonic-aliased-path
            "/opt/unrelated/main.py"))))
    (delete-directory root t)))
"##;
    let expect = expect![[
        r##"OK (:root-aliased "/docker:root@neomacs-api:/workspace/api/" :nested-aliased "/docker:root@neomacs-api:/workspace/api/src/neomacs_api/worker.py" :round-trip-relative "services/api/src/neomacs_api/worker.py" :has-alias (t t nil) :outside "/opt/unrelated/main.py")"##
    ]];
    ParityBatchCase::value(
        "directory_aliases_translate_nested_workspace_paths_in_both_directions",
        elisp_form,
        expect,
    )
}

fn emacs_and_python_filename_views_round_trip_through_a_container_mount() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((root
        (make-temp-file
         "pythonic-filename-workspace-" t))
       (project
        (expand-file-name "project/" root))
       (module
        (expand-file-name "src/app.py" project))
       (mount
        "/docker:root@neomacs-dev:/workspace/project")
       (pythonic-directory-aliases
        (list (list project mount))))
  (unwind-protect
      (progn
        (make-directory
         (file-name-directory module) t)
        (write-region
         "VERSION = 42\n" nil module nil 'silent)
        (cl-letf
            (((symbol-function 'f-same-p)
              #'pythonic-test-same-path-p)
             ((symbol-function 'f-ancestor-of-p)
              #'pythonic-test-ancestor-path-p))
          (let* ((default-directory project)
                 (python-name
                  (pythonic-python-readable-file-name
                   module))
                 (emacs-name
                  (pythonic-emacs-readable-file-name
                   python-name)))
            (list
             :remote-via-alias
             (pythonic-remote-p)
             :python-name python-name
             :emacs-relative
             (file-relative-name emacs-name root)
             :local-pass-through
             (let ((default-directory root)
                   (pythonic-directory-aliases nil))
               (file-relative-name
                (pythonic-python-readable-file-name
                 module)
                root))
             :reject-tramp-python-name
             (condition-case error-data
                 (pythonic-emacs-readable-file-name
                  "/ssh:user@host:/srv/app.py")
               (error
                (list
                 (car error-data)
                 (error-message-string
                  error-data))))))))
    (delete-directory root t)))
"##;
    let expect = expect![[
        r##"OK (:remote-via-alias t :python-name "/workspace/project/src/app.py" :emacs-relative "project/src/app.py" :local-pass-through "project/src/app.py" :reject-tramp-python-name (error "/ssh:user@host:/srv/app.py can not be tramp path"))"##
    ]];
    ParityBatchCase::value(
        "emacs_and_python_filename_views_round_trip_through_a_container_mount",
        elisp_form,
        expect,
    )
}

fn docker_compose_project_discovery_parsing_and_service_alias_selection_form_one_workflow()
-> ParityBatchCase {
    let elisp_form = r##"
(let* ((root
        (make-temp-file
         "pythonic-compose-project-" t))
       (service-directory
        (expand-file-name "services/web/src/" root))
       (compose-file
        (expand-file-name "compose.dev.yml" root))
       (pythonic-docker-compose-filename
        "compose.dev.yml")
       (pythonic-docker-compose-service-name
        "worker")
       (pythonic-directory-aliases nil)
       (struct
        '(("services"
           ("web"
            ("volumes"
             "./services/web:/app"
             "cache:/cache"))
           ("worker"
            ("volumes"
             "./services/worker:/worker"
             "/host/logs:/logs"))))))
  (unwind-protect
      (progn
        (make-directory service-directory t)
        (write-region
         "services:\n  web: {}\n"
         nil compose-file nil 'silent)
        (let* ((default-directory
                service-directory)
               (project
                (pythonic-get-docker-compose-project))
               (filename
                (pythonic-get-docker-compose-filename
                 project))
               parsed-call)
          (cl-letf
              (((symbol-function 'call-process)
                (lambda
                  (program infile destination display
                           &rest args)
                  (setq
                   parsed-call
                   (list
                    program infile
                    (eq destination t)
                    display
                    (append
                     (butlast args)
                     (list
                      (file-relative-name
                       (car (last args))
                       root)))))
                  (insert
                   "{\"services\":{\"web\":{\"volumes\":[\"./services/web:/app\"]}}}")
                  0)))
            (let ((parsed
                   (pythonic-read-docker-compose-file
                    filename)))
              (cl-letf
                  (((symbol-function
                     'hack-dir-local-variables-non-file-buffer)
                    #'ignore)
                   ((symbol-function
                     'pythonic-read-docker-compose-file)
                    (lambda (_filename) struct))
                   ((symbol-function
                     'pythonic-get-docker-compose-container)
                    (lambda (_filename service)
                      (concat "neomacs-" service "-1"))))
                (let ((alias
                       (pythonic-set-docker-compose-alias)))
                  (list
                   :project-found
                   (equal
                    (file-name-as-directory project)
                    (file-name-as-directory root))
                   :filename
                   (file-relative-name filename root)
                   :volumes
                   (pythonic-get-docker-compose-volumes
                    struct)
                   :parsed parsed
                   :parser-call parsed-call
                   :selected-alias
                   (list
                    (file-relative-name
                     (car alias) root)
                    (cadr alias))
                   :installed-aliases
                   (mapcar
                    (lambda (entry)
                      (list
                       (file-relative-name
                        (car entry) root)
                       (cadr entry)))
                    pythonic-directory-aliases))))))))
    (delete-directory root t)))
"##;
    let expect = expect![[
        r##"OK (:project-found t :filename "compose.dev.yml" :volumes (("worker" "./services/worker" "/worker") ("web" "./services/web" "/app")) :parsed (("services" ("web" ("volumes" "./services/web:/app")))) :parser-call ("python" nil t nil ("-c" "\nfrom __future__ import print_function\nimport json, sys, yaml\nprint(json.dumps(yaml.safe_load(open(sys.argv[-1], 'r'))))\n" "compose.dev.yml")) :selected-alias ("services/worker" "/docker:root@neomacs-worker-1:/worker") :installed-aliases (("services/worker" "/docker:root@neomacs-worker-1:/worker")))"##
    ]];
    ParityBatchCase::value(
        "docker_compose_project_discovery_parsing_and_service_alias_selection_form_one_workflow",
        elisp_form,
        expect,
    )
}

fn synchronous_process_runs_in_the_requested_workspace_with_virtualenv_environment()
-> ParityBatchCase {
    let elisp_form = r##"
(let* ((root
        (make-temp-file
         "pythonic-sync-process-" t))
       (workspace
        (expand-file-name "service/" root))
       (virtualenv
        (expand-file-name "venv/" root))
       (pythonic-interpreter "sh")
       (python-shell-virtualenv-root
        virtualenv)
       (python-shell-process-environment
        '("DEPLOYMENT_ENV=staging"))
       status output)
  (unwind-protect
      (progn
        (make-directory workspace t)
        (make-directory
         (expand-file-name "bin/" virtualenv) t)
        (with-temp-buffer
          (setq status
                (pythonic-call-process
                 :buffer t
                 :cwd workspace
                 :args
                 '("-c"
                   "printf 'cwd=%s\\nenv=%s\\nvenv=%s\\n' \"$PWD\" \"$DEPLOYMENT_ENV\" \"$VIRTUAL_ENV\"")))
          (setq output (buffer-string)))
        (list
         :status status
         :output
         (replace-regexp-in-string
          (regexp-quote root)
          "<ROOT>"
          output)
         :default-directory-unchanged
         (not (equal default-directory workspace))))
    (delete-directory root t)))
"##;
    let expect = expect![[
        r##"OK (:status 0 :output "cwd=<ROOT>/service\nenv=staging\nvenv=<ROOT>/venv\n" :default-directory-unchanged t)"##
    ]];
    ParityBatchCase::value(
        "synchronous_process_runs_in_the_requested_workspace_with_virtualenv_environment",
        elisp_form,
        expect,
    )
}

fn asynchronous_process_streams_output_invokes_sentinel_and_exposes_process_contract()
-> ParityBatchCase {
    let elisp_form = r##"
(let* ((root
        (make-temp-file
         "pythonic-async-process-" t))
       (workspace
        (expand-file-name "worker/" root))
       (pythonic-interpreter "sh")
       chunks
       events
       process
       summary)
  (unwind-protect
      (progn
        (make-directory workspace t)
        (setq process
              (pythonic-start-process
               :process "pythonic-worker"
               :buffer nil
               :cwd workspace
               :args
               '("-c"
                 "printf 'phase=prepare\\n'; printf 'phase=publish\\n'")
               :filter
               (lambda (_process output)
                 (push output chunks))
               :sentinel
               (lambda (proc event)
                 (push
                  (list
                   (process-status proc)
                   (string-trim event))
                  events))
               :query-on-exit nil))
        ;; Wait for the sentinel's own record, not for the process to die.
        ;; This case pins :output, which the FILTER assembles, and :events,
        ;; which the SENTINEL pushes -- so both halves of the pin are written
        ;; after the child is already gone.  `process-live-p' going nil is the
        ;; wrong fact and it is wrong in a specific direction: GNU reaps the
        ;; child in `handle_child_signal', setting `raw_status_new'
        ;; (src/process.c:7748), which is all `process-status' needs to answer
        ;; `exit' (src/process.c:1188-1189), and in the same pass calling
        ;; `delete_read_fd' (src/process.c:7760), which STOPS ordinary reading
        ;; of the pipe.  The bytes the child had already written are recovered
        ;; only by the drain loop in `status_notify' (src/process.c:7896-7911),
        ;; which runs immediately before `exec_sentinel' (src/process.c:7937).
        ;; So `events' becoming non-nil proves the drain has happened; the
        ;; child being dead proves the opposite is still possible.
        (let ((deadline (+ (float-time) 30)))
          (while (and (null events) (< (float-time) deadline))
            (accept-process-output nil 0.05)))
        (unless events
          (error "pythonic worker never ran its sentinel; :output holds only \
as much of the child's stream as had been read"))
        (setq summary
              (pythonic-test-process-summary
               process))
        (list
         :output
         (apply #'concat (nreverse chunks))
         :events (nreverse events)
         :process summary))
    (when (and process
               (process-live-p process))
      (delete-process process))
    (delete-directory root t)))
"##;
    let expect = expect![[
        r##"OK (:output "phase=prepare\nphase=publish\n" :events ((exit "finished")) :process (:name "pythonic-worker" :status exit :exit-status 0 :query-on-exit nil :command ("sh" "-c" "printf 'phase=prepare\\n'; printf 'phase=publish\\n'")))"##
    ]];
    ParityBatchCase::value(
        "asynchronous_process_streams_output_invokes_sentinel_and_exposes_process_contract",
        elisp_form,
        expect,
    )
}

fn virtualenv_activation_normalizes_local_and_remote_paths_then_deactivates_cleanly()
-> ParityBatchCase {
    let elisp_form = r##"
(let* ((root
        (make-temp-file
         "pythonic-virtualenv-" t))
       (local-env
        (expand-file-name "venv/" root))
       (python-shell-virtualenv-root nil)
       states)
  (unwind-protect
      (progn
        (make-directory local-env t)
        (let ((default-directory root)
              (pythonic-directory-aliases nil))
          (pythonic-activate local-env)
          (push
           (list
            :local
            (file-relative-name
             python-shell-virtualenv-root root))
           states)
          (pythonic-deactivate)
          (push
           (list :local-deactivated
                 python-shell-virtualenv-root)
           states))
        (let ((default-directory
               "/ssh:deploy@build.example:/srv/app/")
              (pythonic-directory-aliases nil))
          (pythonic-activate
           "/ssh:deploy@build.example:/srv/venv/")
          (push
           (list
            :remote
            python-shell-virtualenv-root)
           states)
          (pythonic-deactivate)
          (push
           (list :remote-deactivated
                 python-shell-virtualenv-root)
           states))
        (nreverse states))
    (delete-directory root t)))
"##;
    let expect = expect![[
        r##"OK ((:local "venv/") (:local-deactivated nil) (:remote "/srv/venv/") (:remote-deactivated nil))"##
    ]];
    ParityBatchCase::value(
        "virtualenv_activation_normalizes_local_and_remote_paths_then_deactivates_cleanly",
        elisp_form,
        expect,
    )
}

#[test]
fn pythonic_package_batch() {
    let cases = vec![
        tramp_connection_profiles_report_method_user_host_port_and_platform_predicates(),
        directory_aliases_translate_nested_workspace_paths_in_both_directions(),
        emacs_and_python_filename_views_round_trip_through_a_container_mount(),
        docker_compose_project_discovery_parsing_and_service_alias_selection_form_one_workflow(),
        synchronous_process_runs_in_the_requested_workspace_with_virtualenv_environment(),
        asynchronous_process_streams_output_invokes_sentinel_and_exposes_process_contract(),
        virtualenv_activation_normalizes_local_and_remote_paths_then_deactivates_cleanly(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed pythonic parity test");
    assert_oracle_batch_cases(pythonic_oracle(), test_name, "pythonic_parity", &cases);
}
