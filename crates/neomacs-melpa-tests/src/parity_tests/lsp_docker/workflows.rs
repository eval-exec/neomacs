use expect_test::expect;

use super::ParityBatchCase;

fn legacy_client_initialization_registers_a_project_scoped_docker_transport() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-lsp-docker-test-with-project "legacy-client" "exit 99"
  (let* ((path-mappings
          (list (cons (directory-file-name project-root) "/workspace")))
         (_
          (lsp-docker-init-clients
           :path-mappings path-mappings
           :docker-image-id "registry.example/acme/pylsp:2026.08"
           :docker-container-name "pylsp-dev"
           :priority 40
           :client-packages nil
           :client-configs
           '((:server-id pylsp
              :docker-server-id pylsp-docker
              :server-command "pylsp --stdio"))))
         (client (gethash 'pylsp-docker lsp-clients))
         (connection (lsp--client-new-connection client))
         (uri->path (lsp--client-uri->path-fn client))
         (path->uri (lsp--client-path->uri-fn client)))
    (list
     :registered
     (sort (mapcar #'symbol-name (hash-table-keys lsp-clients)) #'string<)
     :priority (lsp--client-priority client)
     :major-modes (copy-sequence (lsp--client-major-modes client))
     :inside-project (and (funcall (plist-get connection :test?)) t)
     :outside-project
     (let ((buffer-file-name outside-file))
       (funcall (plist-get connection :test?)))
     :container-suffix lsp-docker-container-name-suffix
     :host-to-container-uri
     (copy-sequence (funcall path->uri source-file))
     :container-uri-to-host
     (neomacs-lsp-docker-test-normalize
      (funcall uri->path "file:///workspace/src/app.py") project-root)
     :unmapped-container-uri
     (copy-sequence (funcall uri->path "file:///opt/vendor/stubs.pyi"))
     :outside-mapping-error
     (condition-case err
         (progn (funcall path->uri outside-file) :ok)
       (error (error-message-string err)))
     :run-command
     (neomacs-lsp-docker-test-normalize-strings
      (lsp-docker-launch-new-container
       "pylsp-dev-1"
       path-mappings
       '("--network=none" "--userns=keep-id")
       "registry.example/acme/pylsp:2026.08"
       "pylsp --stdio")
      project-root)
     :docker-calls
     (neomacs-lsp-docker-test-docker-calls trace project-root))))
"####;
    let expected = expect![[
        r#"OK (:registered ("pylsp" "pylsp-docker") :priority 40 :major-modes (python-mode python-ts-mode) :inside-project t :outside-project nil :container-suffix 1 :host-to-container-uri "file:///workspace/src/app.py" :container-uri-to-host "<PROJECT>/src/app.py" :unmapped-container-uri "/docker:pylsp-dev-1:/opt/vendor/stubs.pyi" :outside-mapping-error "The path [ORACLE-SANDBOX]/legacy-client/outside/vendor.py is not under path mappings" :run-command ("docker" "run" "--name" "pylsp-dev-1" "--rm" "-i" "-v" "<PROJECT>:/workspace" "--network=none" "--userns=keep-id" "registry.example/acme/pylsp:2026.08" "pylsp" "--stdio") :docker-calls nil)"#
    ]];
    ParityBatchCase::value(
        "legacy_client_initialization_registers_a_project_scoped_docker_transport",
        elisp_form,
        expected,
    )
}

fn persistent_container_config_registers_a_real_project_client() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-lsp-docker-test-with-project
    "persistent-container"
    "if [ \"$1\" = container ] && [ \"$2\" = list ]; then
  printf \"'%s'\\n\" dev-pylsp build-cache
  exit 0
fi
printf 'UNEXPECTED docker invocation: %s\\n' \"$*\" >&2
exit 98"
  (neomacs-lsp-docker-test-write
   (expand-file-name ".lsp-docker.yml" project-root)
   "lsp:
  server:
    type: docker
    subtype: container
    name: dev-pylsp
    server: pylsp
  mappings:
    - source: .
      destination: /workspace
")
  (neomacs-lsp-docker-test-write outside-file "deploy(\"outside\")\n")
  (call-interactively #'lsp-docker-register)
  (let* ((docker-ids
          (seq-remove (lambda (id) (eq id 'pylsp))
                      (hash-table-keys lsp-clients)))
         (docker-id (car docker-ids))
         (client (gethash docker-id lsp-clients))
         (activation (lsp--client-activation-fn client))
         (uri->path (lsp--client-uri->path-fn client))
         (path->uri (lsp--client-path->uri-fn client)))
    (list
     :registered-count (hash-table-count lsp-clients)
     :one-docker-client (= (length docker-ids) 1)
     :docker-id
     (list :symbol (symbolp docker-id)
           :project-scoped
           (string-suffix-p "-pylsp-docker" (symbol-name docker-id)))
     :priority (lsp--client-priority client)
     :activation
     (list :project-python (and (funcall activation source-file 'python-mode) t)
           :project-rust (funcall activation source-file 'rust-mode)
           :outside
           (with-current-buffer (find-file-noselect outside-file)
             (condition-case err
                 (funcall activation outside-file 'python-mode)
               (error (list (car err) (error-message-string err))))))
     :path-round-trip
     (list
      :to-container (copy-sequence (funcall path->uri source-file))
      :to-host
      (neomacs-lsp-docker-test-normalize
       (funcall uri->path "file:///workspace/src/app.py") project-root))
     :launch-command
     (neomacs-lsp-docker-test-normalize-strings
      (lsp-docker-launch-existing-container "dev-pylsp") project-root)
     :docker-calls
     (neomacs-lsp-docker-test-docker-calls trace project-root))))
"####;
    let expected = expect![[
        r#"OK (:registered-count 2 :one-docker-client t :docker-id (:symbol t :project-scoped t) :priority 100 :activation (:project-python t :project-rust nil :outside (wrong-type-argument "Wrong type argument: stringp, nil")) :path-round-trip (:to-container "file:///workspace/src/app.py" :to-host "<PROJECT>/src/app.py") :launch-command ("docker" "start" "-ia" "dev-pylsp") :docker-calls (("container" "list" "--all" "--format" "'{{.Names}}'")))"#
    ]];
    ParityBatchCase::value(
        "persistent_container_config_registers_a_real_project_client",
        elisp_form,
        expected,
    )
}

fn multi_server_config_registers_image_and_existing_container_clients() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-lsp-docker-test-with-project
    "multi-server"
    "if [ \"$1\" = image ] && [ \"$2\" = list ]; then
  printf \"'%s'\\n\" registry.example/acme/clangd:17 unrelated/image:latest
  exit 0
fi
if [ \"$1\" = container ] && [ \"$2\" = list ]; then
  printf \"'%s'\\n\" dev-pylsp build-cache
  exit 0
fi
printf 'UNEXPECTED docker invocation: %s\\n' \"$*\" >&2
exit 98"
  (let ((cpp-file (expand-file-name "src/main.cpp" project-root)))
    (neomacs-lsp-docker-test-write cpp-file "int answer() { return 42; }\n")
    (puthash
     'clangd
     (make-lsp-client
      :new-connection (lsp-stdio-connection '("clangd" "--stdio"))
      :major-modes '(c-mode c++-mode c-ts-mode c++-ts-mode)
      :server-id 'clangd
      :priority 4
      :activation-fn
      (lambda (file mode)
        (and (memq mode '(c-mode c++-mode c-ts-mode c++-ts-mode))
             (string-suffix-p ".cpp" file))))
     lsp-clients)
    (neomacs-lsp-docker-test-write
     (expand-file-name ".lsp-docker/config.yaml" project-root)
     "lsp:
  server:
    - type: docker
      subtype: image
      name: registry.example/acme/clangd:17
      server: clangd
      launch_parameters: [--network=none, --userns=keep-id]
      launch_command: clangd --background-index
    - type: docker
      subtype: container
      name: dev-pylsp
      server: pylsp
  mappings:
    - source: .
      destination: /workspace
")
    (call-interactively #'lsp-docker-register)
    (let* ((docker-ids
            (seq-filter
             (lambda (id)
               (string-suffix-p "-docker" (symbol-name id)))
             (hash-table-keys lsp-clients)))
           (clangd-id
            (seq-find
             (lambda (id)
               (string-suffix-p "-clangd-docker" (symbol-name id)))
             docker-ids))
           (pylsp-id
            (seq-find
             (lambda (id)
               (string-suffix-p "-pylsp-docker" (symbol-name id)))
             docker-ids))
           (clangd-client (gethash clangd-id lsp-clients))
           (pylsp-client (gethash pylsp-id lsp-clients)))
      (list
       :registered-count (hash-table-count lsp-clients)
       :docker-client-kinds
       (sort
        (mapcar
         (lambda (id)
           (cond
            ((string-suffix-p "-clangd-docker" (symbol-name id)) 'clangd)
            ((string-suffix-p "-pylsp-docker" (symbol-name id)) 'pylsp)
            (t 'unexpected)))
         docker-ids)
        (lambda (left right)
          (string< (symbol-name left) (symbol-name right))))
       :priorities
       (list (lsp--client-priority clangd-client)
             (lsp--client-priority pylsp-client))
       :activation
       (list
        :clangd-cpp
        (and (funcall (lsp--client-activation-fn clangd-client)
                      cpp-file 'c++-mode)
             t)
        :clangd-python
        (funcall (lsp--client-activation-fn clangd-client)
                 source-file 'python-mode)
        :pylsp-python
        (and (funcall (lsp--client-activation-fn pylsp-client)
                      source-file 'python-mode)
             t))
       :shared-path-mapping
       (list
        (copy-sequence
         (funcall (lsp--client-path->uri-fn clangd-client) cpp-file))
        (copy-sequence
         (funcall (lsp--client-path->uri-fn pylsp-client) source-file)))
       :docker-calls
       (neomacs-lsp-docker-test-docker-calls trace project-root)))))
"####;
    let expected = expect![[
        r#"OK (:registered-count 4 :docker-client-kinds (clangd pylsp) :priorities (100 100) :activation (:clangd-cpp t :clangd-python nil :pylsp-python t) :shared-path-mapping ("file:///workspace/src/main.cpp" "file:///workspace/src/app.py") :docker-calls (("image" "list" "--format" "'{{.Repository}}:{{.Tag}}'") ("container" "list" "--all" "--format" "'{{.Names}}'")))"#
    ]];
    ParityBatchCase::value(
        "multi_server_config_registers_image_and_existing_container_clients",
        elisp_form,
        expected,
    )
}

fn missing_image_without_a_dockerfile_refuses_registration() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-lsp-docker-test-with-project
    "missing-image"
    "if [ \"$1\" = image ] && [ \"$2\" = list ]; then
  exit 0
fi
printf 'UNEXPECTED docker invocation: %s\\n' \"$*\" >&2
exit 98"
  (neomacs-lsp-docker-test-write
   (expand-file-name ".lsp-docker.yml" project-root)
   "lsp:
  server:
    type: docker
    subtype: image
    name: registry.example/acme/missing-pylsp:9
    server: pylsp
    launch_command: pylsp --stdio
  mappings:
    - source: .
      destination: /workspace
")
  (let ((registration-error
         (condition-case err
             (progn (call-interactively #'lsp-docker-register) :unexpected-success)
           (error (list (car err) (error-message-string err))))))
    (list
     :error registration-error
     :registered
     (sort (mapcar #'symbol-name (hash-table-keys lsp-clients)) #'string<)
     :build-buffer-created
     (and (get-buffer "*lsp-docker-build*") t)
     :docker-calls
     (neomacs-lsp-docker-test-docker-calls trace project-root))))
"####;
    let expected = expect![[
        r#"OK (:error (user-error "Cannot find the image registry.example/acme/missing-pylsp:9 but cannot build it too (missing Dockerfile)") :registered ("pylsp") :build-buffer-created nil :docker-calls (("image" "list" "--format" "'{{.Repository}}:{{.Tag}}'") ("image" "list" "--format" "'{{.Repository}}:{{.Tag}}'")))"#
    ]];
    ParityBatchCase::value(
        "missing_image_without_a_dockerfile_refuses_registration",
        elisp_form,
        expected,
    )
}

fn docker_daemon_failure_is_reported_without_registering_a_client() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-lsp-docker-test-with-project
    "daemon-failure"
    "if [ \"$1\" = image ] && [ \"$2\" = list ]; then
  printf 'Cannot connect to the Docker daemon\\n' >&2
  exit 23
fi
printf 'UNEXPECTED docker invocation: %s\\n' \"$*\" >&2
exit 98"
  (neomacs-lsp-docker-test-write
   (expand-file-name ".lsp-docker.yml" project-root)
   "lsp:
  server:
    type: docker
    subtype: image
    name: registry.example/acme/pylsp:stable
    server: pylsp
    launch_command: pylsp --stdio
  mappings:
    - source: .
      destination: /workspace
")
  (let ((registration-error
         (condition-case err
             (progn (call-interactively #'lsp-docker-register) :unexpected-success)
           (error (list (car err) (error-message-string err))))))
    (list
     :error registration-error
     :registered
     (sort (mapcar #'symbol-name (hash-table-keys lsp-clients)) #'string<)
     :docker-calls
     (neomacs-lsp-docker-test-docker-calls trace project-root))))
"####;
    let expected = expect![[
        r#"OK (:error (user-error "Cannot get the existing images list from the host, exit code: 23") :registered ("pylsp") :docker-calls (("image" "list" "--format" "'{{.Repository}}:{{.Tag}}'")))"#
    ]];
    ParityBatchCase::value(
        "docker_daemon_failure_is_reported_without_registering_a_client",
        elisp_form,
        expected,
    )
}

fn mapping_outside_the_project_is_rejected_before_docker_is_called() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-lsp-docker-test-with-project
    "outside-mapping"
    "printf 'Docker must not be called for an invalid mapping\\n' >&2
exit 97"
  (make-directory (file-name-directory outside-file) t)
  (neomacs-lsp-docker-test-write
   (expand-file-name ".lsp-docker.yml" project-root)
   "lsp:
  server:
    type: docker
    subtype: container
    name: dev-pylsp
    server: pylsp
  mappings:
    - source: ../outside
      destination: /workspace
")
  (let ((registration-error
         (condition-case err
             (progn (call-interactively #'lsp-docker-register) :unexpected-success)
           (error (list (car err) (error-message-string err))))))
    (list
     :error registration-error
     :registered
     (sort (mapcar #'symbol-name (hash-table-keys lsp-clients)) #'string<)
     :docker-calls
     (neomacs-lsp-docker-test-docker-calls trace project-root))))
"####;
    let expected = expect![[
        r#"OK (:error (user-error "Language server registration failed, check input parameters") :registered ("pylsp") :docker-calls nil)"#
    ]];
    ParityBatchCase::value(
        "mapping_outside_the_project_is_rejected_before_docker_is_called",
        elisp_form,
        expected,
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        legacy_client_initialization_registers_a_project_scoped_docker_transport(),
        persistent_container_config_registers_a_real_project_client(),
        multi_server_config_registers_image_and_existing_container_clients(),
        missing_image_without_a_dockerfile_refuses_registration(),
        docker_daemon_failure_is_reported_without_registering_a_client(),
        mapping_outside_the_project_is_rejected_before_docker_is_called(),
    ]
}
