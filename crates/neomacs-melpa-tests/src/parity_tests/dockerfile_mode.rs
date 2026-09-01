use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, DOCKERFILE_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r###"
(require 'cl-lib)
(require 'dockerfile-mode)
(require 'imenu)

(defun neomacs-dockerfile-test-token (text &optional occurrence offset)
  "Return TEXT's fontification and syntax state at OCCURRENCE plus OFFSET."
  (save-excursion
    (goto-char (point-min))
    (dotimes (_ (or occurrence 1)) (search-forward text))
    (let* ((position (+ (match-beginning 0) (or offset 0)))
           (state (syntax-ppss position)))
      (list text
            :range (list (match-beginning 0) (match-end 0))
            :face (get-text-property position 'face)
            :font-lock-face (get-text-property position 'font-lock-face)
            :string (and (nth 3 state) t)
            :comment (and (nth 4 state) t)))))

(defun neomacs-dockerfile-test-index (entries buffer)
  "Normalize Dockerfile Imenu ENTRIES to names and source lines."
  (mapcar
   (lambda (entry)
     (if (imenu--subalist-p entry)
         (cons (car entry)
               (neomacs-dockerfile-test-index (cdr entry) buffer))
       (let ((position (if (listp (cdr entry)) (cadr entry) (cdr entry))))
         (if (and (numberp position) (< position 0))
             (list (car entry) :rescan)
           (list (car entry)
                 (with-current-buffer buffer
                   (line-number-at-pos position)))))))
   entries))
"###;

fn package_contract_configures_dockerfile_buffers_keys_files_and_defaults() -> ParityBatchCase {
    let elisp_form = r###"
(let ((descriptor (cadr (assq 'dockerfile-mode package-alist))))
  (with-temp-buffer
    (dockerfile-mode)
    (list
     :package
     (list :name (package-desc-name descriptor)
           :version (package-version-join (package-desc-version descriptor))
           :requirements (package-desc-reqs descriptor)
           :feature (and (featurep 'dockerfile-mode) t))
     :mode
     (list major-mode mode-name (derived-mode-p 'prog-mode)
           indent-line-function
           (and (eq local-abbrev-table dockerfile-mode-abbrev-table) t)
           require-final-newline parse-sexp-ignore-comments
           font-lock-defaults)
     :comments (list comment-start comment-end comment-start-skip)
     :syntax (mapcar (lambda (character) (char-syntax character))
                     '(?# ?\n ?' ?= ?" ?_ ?-))
     :keys
     (mapcar (lambda (key) (lookup-key dockerfile-mode-map (kbd key)))
             '("C-c C-b" "C-c M-b" "C-c C-c"))
     :commands
     (mapcar #'commandp
             '(dockerfile-mode dockerfile-build-buffer
               dockerfile-build-no-cache-buffer comment-region))
     :recognition
     (mapcar (lambda (filename)
               (assoc-default filename auto-mode-alist #'string-match))
             '("/srv/app/Dockerfile" "/srv/app/Dockerfile.release"
               "/srv/app/Containerfile.dev" "/srv/app/service.dockerfile"
               "/srv/app/dockerfile" "/srv/app/service.txt"))
     :defaults
     (list dockerfile-mode-command dockerfile-use-sudo
           dockerfile-build-force-rm dockerfile-build-pull
           dockerfile-build-args dockerfile-build-progress
           dockerfile-build-extra-options dockerfile-use-buildkit
           dockerfile-enable-auto-indent dockerfile-indent-offset))))
"###;
    let expected = expect![[
        r##"OK (:package (:name dockerfile-mode :version "20251221.1644" :requirements ((emacs (24))) :feature t) :mode (dockerfile-mode "Dockerfile" prog-mode dockerfile-indent-line-function t t t (dockerfile-font-lock-keywords nil t)) :comments ("#" "" "#+ *") :syntax (60 62 34 46 34 95 95) :keys (dockerfile-build-buffer dockerfile-build-no-cache-buffer comment-region) :commands (t t t t) :recognition (dockerfile-mode dockerfile-mode dockerfile-mode dockerfile-mode dockerfile-mode text-mode) :defaults ("docker" nil nil nil nil "auto" nil nil t 4))"##
    ]];
    ParityBatchCase::value(
        "package_contract_configures_dockerfile_buffers_keys_files_and_defaults",
        elisp_form,
        expected,
    )
}

fn multistage_container_source_fontifies_roles_and_indexes_build_stages() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert "# syntax=docker/dockerfile:1\n"
          "ARG RUST_VERSION=1.79\n"
          "FROM --platform=$BUILDPLATFORM rust:${RUST_VERSION} AS builder\n"
          "WORKDIR /workspace\n"
          "COPY Cargo.toml Cargo.lock ./\n"
          "RUN --mount=type=cache,target=/usr/local/cargo/registry \\\n"
          "    cargo build --release --jobs $BUILD_THREADS\n"
          "FROM debian:bookworm-slim AS runtime # production image\n"
          "ENV APP_ENV=production \\\n"
          "    PORT=8080\n"
          "HEALTHCHECK CMD curl --fail http://localhost:8080/health || exit 1\n"
          "COPY --from=builder /workspace/target/release/checkout /usr/local/bin/\n"
          "ENTRYPOINT [\"checkout\"]\n")
  (dockerfile-mode)
  (font-lock-ensure)
  (let ((index (imenu--make-index-alist t)))
    (list
     :tokens
     (mapcar (lambda (request)
               (apply #'neomacs-dockerfile-test-token request))
             '(("syntax=docker/dockerfile:1")
               ("ARG") ("RUST_VERSION" 1)
               ("FROM" 1) ("$BUILDPLATFORM") ("rust:${RUST_VERSION}")
               ("builder") ("WORKDIR") ("RUN") ("$BUILD_THREADS")
               ("FROM" 2) ("debian:bookworm-slim") ("runtime")
               ("production image") ("ENV") ("HEALTHCHECK")
               ("COPY" 2) ("ENTRYPOINT")))
     :index (neomacs-dockerfile-test-index index (current-buffer))
     :case-fold font-lock-keywords-case-fold-search
     :mode major-mode
     :text (buffer-substring-no-properties (point-min) (point-max)))))
"###;
    let expected = expect![[
        r##"OK (:tokens (("syntax=docker/dockerfile:1" :range (3 29) :face font-lock-comment-face :font-lock-face nil :string nil :comment t) ("ARG" :range (30 33) :face font-lock-keyword-face :font-lock-face nil :string nil :comment nil) ("RUST_VERSION" :range (34 46) :face font-lock-variable-name-face :font-lock-face nil :string nil :comment nil) ("FROM" :range (52 56) :face font-lock-keyword-face :font-lock-face nil :string nil :comment nil) ("$BUILDPLATFORM" :range (68 82) :face nil :font-lock-face nil :string nil :comment nil) ("rust:${RUST_VERSION}" :range (83 103) :face dockerfile-image-name :font-lock-face nil :string nil :comment nil) ("builder" :range (107 114) :face dockerfile-image-alias :font-lock-face nil :string nil :comment nil) ("WORKDIR" :range (115 122) :face font-lock-keyword-face :font-lock-face nil :string nil :comment nil) ("RUN" :range (164 167) :face font-lock-keyword-face :font-lock-face nil :string nil :comment nil) ("$BUILD_THREADS" :range (255 269) :face nil :font-lock-face nil :string nil :comment nil) ("FROM" :range (270 274) :face font-lock-keyword-face :font-lock-face nil :string nil :comment nil) ("debian:bookworm-slim" :range (275 295) :face dockerfile-image-name :font-lock-face nil :string nil :comment nil) ("runtime" :range (299 306) :face dockerfile-image-alias :font-lock-face nil :string nil :comment nil) ("production image" :range (309 325) :face font-lock-comment-face :font-lock-face nil :string nil :comment t) ("ENV" :range (326 329) :face font-lock-keyword-face :font-lock-face nil :string nil :comment nil) ("HEALTHCHECK" :range (365 376) :face font-lock-keyword-face :font-lock-face nil :string nil :comment nil) ("COPY" :range (432 436) :face font-lock-keyword-face :font-lock-face nil :string nil :comment nil) ("ENTRYPOINT" :range (503 513) :face font-lock-keyword-face :font-lock-face nil :string nil :comment nil)) :index (("*Rescan*" :rescan) ("Stage" ("builder" 3) ("runtime" 8))) :case-fold t :mode dockerfile-mode :text "# syntax=docker/dockerfile:1\nARG RUST_VERSION=1.79\nFROM --platform=$BUILDPLATFORM rust:${RUST_VERSION} AS builder\nWORKDIR /workspace\nCOPY Cargo.toml Cargo.lock ./\nRUN --mount=type=cache,target=/usr/local/cargo/registry \\\n    cargo build --release --jobs $BUILD_THREADS\nFROM debian:bookworm-slim AS runtime # production image\nENV APP_ENV=production \\\n    PORT=8080\nHEALTHCHECK CMD curl --fail http://localhost:8080/health || exit 1\nCOPY --from=builder /workspace/target/release/checkout /usr/local/bin/\nENTRYPOINT [\"checkout\"]\n")"##
    ]];
    ParityBatchCase::value(
        "multistage_container_source_fontifies_roles_and_indexes_build_stages",
        elisp_form,
        expected,
    )
}

fn auto_indentation_aligns_continuations_idempotently_and_respects_disable_policy()
-> ParityBatchCase {
    let elisp_form = r###"
(let ((source
       (concat
        "FROM alpine:3.20 AS runtime\n"
        "RUN apk add --no-cache \\\n"
        "curl \\\n"
        "ca-certificates\n"
        "ENV APP_ENV=production \\\n"
        "PORT=8080\n"
        "\n"
        "# Keep the final image deterministic.\n"
        "CMD [\"checkout\"]\n")))
  (list
   :enabled
   (with-temp-buffer
     (insert source)
     (let ((dockerfile-indent-offset 4)
           (dockerfile-enable-auto-indent t))
       (dockerfile-mode)
       (font-lock-ensure)
       (indent-region (point-min) (point-max))
       (let ((once (buffer-substring-no-properties (point-min) (point-max)))
             (columns
              (save-excursion
                (goto-char (point-min))
                (let (result)
                  (while (not (eobp))
                    (push (current-indentation) result)
                    (forward-line 1))
                  (nreverse result)))))
         (font-lock-ensure)
         (indent-region (point-min) (point-max))
         (list :text once :columns columns
               :idempotent (equal once (buffer-substring-no-properties
                                        (point-min) (point-max)))))))
   :disabled
   (with-temp-buffer
     (insert source)
     (let ((dockerfile-indent-offset 4)
           (dockerfile-enable-auto-indent nil))
       (dockerfile-mode)
       (font-lock-ensure)
       (indent-region (point-min) (point-max))
       (buffer-substring-no-properties (point-min) (point-max))))))
"###;
    let expected = expect![[
        r#"OK (:enabled (:text "FROM alpine:3.20 AS runtime\nRUN apk add --no-cache \\\n    curl \\\n    ca-certificates\nENV APP_ENV=production \\\n    PORT=8080\n\n# Keep the final image deterministic.\nCMD [\"checkout\"]\n" :columns (0 0 4 4 0 4 0 0 0) :idempotent t) :disabled "FROM alpine:3.20 AS runtime\nRUN apk add --no-cache \\\ncurl \\\nca-certificates\nENV APP_ENV=production \\\nPORT=8080\n\n# Keep the final image deterministic.\nCMD [\"checkout\"]\n")"#
    ]];
    ParityBatchCase::value(
        "auto_indentation_aligns_continuations_idempotently_and_respects_disable_policy",
        elisp_form,
        expected,
    )
}

fn comment_editing_round_trips_instructions_and_preserves_hashes_inside_shell_strings()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert "FROM alpine:3.20\n"
          "RUN echo 'release #42' \"ready #43\"\n"
          "# deployment note\n")
  (dockerfile-mode)
  (font-lock-ensure)
  (let ((syntax
         (mapcar (lambda (request)
                   (apply #'neomacs-dockerfile-test-token request))
                 '(("release #42" 1 9)
                   ("ready #43" 1 7)
                   ("deployment note" 1 3)))))
    (comment-region (point-min)
                    (save-excursion (goto-char (point-min)) (forward-line 2) (point)))
    (let ((commented (buffer-substring-no-properties (point-min) (point-max))))
      (uncomment-region (point-min)
                        (save-excursion (goto-char (point-min)) (forward-line 2) (point)))
      (list :syntax syntax
            :commented commented
            :restored (buffer-substring-no-properties (point-min) (point-max))
            :mode (list comment-start comment-end comment-start-skip
                        parse-sexp-ignore-comments)))))
"###;
    let expected = expect![[
        r##"OK (:syntax (("release #42" :range (28 39) :face font-lock-string-face :font-lock-face nil :string t :comment nil) ("ready #43" :range (42 51) :face font-lock-string-face :font-lock-face nil :string t :comment nil) ("deployment note" :range (55 70) :face font-lock-comment-face :font-lock-face nil :string nil :comment t)) :commented "# FROM alpine:3.20\n# RUN echo 'release #42' \"ready #43\"\n# deployment note\n" :restored "FROM alpine:3.20\nRUN echo 'release #42' \"ready #43\"\n# deployment note\n" :mode ("#" "" "#+ *" t))"##
    ]];
    ParityBatchCase::value(
        "comment_editing_round_trips_instructions_and_preserves_hashes_inside_shell_strings",
        elisp_form,
        expected,
    )
}

fn production_build_options_generate_the_exact_safe_compilation_command() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert "FROM rust:1.79 AS builder\nRUN cargo build --release\n")
  (setq buffer-file-name "/workspace/checkout service/Dockerfile.release"
        default-directory "/workspace/checkout service/")
  (dockerfile-mode)
  (let ((dockerfile-mode-command "podman")
        (dockerfile-use-sudo t)
        (dockerfile-build-force-rm t)
        (dockerfile-build-pull t)
        (dockerfile-build-args
         '("HTTP_PROXY=http://proxy.internal:8080"
           "FEATURE_SET=payments canary" "TOKEN=x\\=y"))
        (dockerfile-build-progress "plain")
        (dockerfile-build-extra-options "--network host --target runtime")
        (dockerfile-use-buildkit t)
        (saved 0)
        captured)
    (cl-letf (((symbol-function 'save-buffer)
               (lambda (&rest _) (setq saved (1+ saved)) t))
              ((symbol-function 'compilation-start)
               (lambda (command &optional mode name-function &rest rest)
                 (setq captured
                       (list :command command :mode mode :rest rest
                             :buffer-name (funcall name-function "compilation")))
                 'fake-compilation-buffer)))
      (let ((result
             (dockerfile-build-buffer
              "registry.example.test/checkout:canary" t)))
        (list :result result :saved saved :captured captured
              :tag (dockerfile-tag-string "registry.example.test/checkout:canary")
              :args (dockerfile-build-arg-string)
              :filename (dockerfile-standard-filename buffer-file-name))))))
"###;
    let expected = expect![[
        r#"OK (:result fake-compilation-buffer :saved 1 :captured (:command "DOCKER_BUILDKIT=1 sudo podman build --no-cache --force-rm  --pull  --tag registry.example.test/checkout\\:canary  --build-arg=HTTP_PROXY=http\\://proxy.internal\\:8080 --build-arg=FEATURE_SET=payments\\ canary --build-arg=TOKEN=x\\\\=y --progress plain --network host --target runtime -f /workspace/checkout\\ service/Dockerfile.release /workspace/checkout\\ service/" :mode nil :rest nil :buffer-name "*docker-build-output: registry.example.test/checkout:canary *") :tag "--tag registry.example.test/checkout\\:canary " :args "--build-arg=HTTP_PROXY=http\\://proxy.internal\\:8080 --build-arg=FEATURE_SET=payments\\ canary --build-arg=TOKEN=x\\\\=y" :filename "/workspace/checkout service/Dockerfile.release")"#
    ]];
    ParityBatchCase::value(
        "production_build_options_generate_the_exact_safe_compilation_command",
        elisp_form,
        expected,
    )
}

fn remote_no_cache_build_uses_remote_localnames_without_contacting_the_host() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert "FROM alpine:3.20\n")
  (setq buffer-file-name
        "/ssh:builder@example.test:/srv/checkout/Dockerfile"
        default-directory "/ssh:builder@example.test:/srv/checkout/")
  (dockerfile-mode)
  (let ((dockerfile-mode-command "docker")
        (dockerfile-use-sudo nil)
        (dockerfile-build-force-rm nil)
        (dockerfile-build-pull nil)
        (dockerfile-build-args nil)
        (dockerfile-build-progress "auto")
        (dockerfile-build-extra-options nil)
        (dockerfile-use-buildkit nil)
        captured)
    (cl-letf (((symbol-function 'save-buffer) (lambda (&rest _) t))
              ((symbol-function 'compilation-start)
               (lambda (command &optional mode name-function &rest _)
                 (setq captured
                       (list :command command :mode mode
                             :buffer-name (funcall name-function "compilation")))
                 'remote-build)))
      (list :result (dockerfile-build-no-cache-buffer "")
            :captured captured
            :file-localname (file-remote-p buffer-file-name 'localname)
            :directory-localname (file-remote-p default-directory 'localname)
            :blank-tag (dockerfile-tag-string "")
            :quoted-tag (dockerfile-tag-string "checkout canary")))))
"###;
    let expected = expect![[
        r#"OK (:result remote-build :captured (:command "docker build --no-cache     --progress auto  -f /srv/checkout/Dockerfile /srv/checkout/" :mode nil :buffer-name "*docker-build-output:  *") :file-localname "/srv/checkout/Dockerfile" :directory-localname "/srv/checkout/" :blank-tag "" :quoted-tag "--tag checkout\\ canary ")"#
    ]];
    ParityBatchCase::value(
        "remote_no_cache_build_uses_remote_localnames_without_contacting_the_host",
        elisp_form,
        expected,
    )
}

fn image_name_prompt_uses_buffer_configuration_history_and_obsolete_alias() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (dockerfile-mode)
  (let ((dockerfile-image-name "registry.example.test/team/checkout:dev")
        call)
    (cl-letf (((symbol-function 'read-string)
               (lambda (prompt &optional initial history &rest rest)
                 (setq call (list prompt initial history rest))
                 "registry.example.test/team/checkout:release")))
      (let ((selected (dockerfile-read-image-name)))
        (setq docker-image-name "registry.example.test/team/checkout:alias")
        (list :selected selected
              :call call
              :canonical dockerfile-image-name
              :alias docker-image-name
              :alias-target (indirect-variable 'docker-image-name)
              :history dockerfile-image-name-history)))))
"###;
    let expected = expect![[
        r#"OK (:selected "registry.example.test/team/checkout:release" :call ("Image name: " "registry.example.test/team/checkout:dev" dockerfile-image-name-history nil) :canonical "registry.example.test/team/checkout:alias" :alias "registry.example.test/team/checkout:alias" :alias-target dockerfile-image-name :history nil)"#
    ]];
    ParityBatchCase::value(
        "image_name_prompt_uses_buffer_configuration_history_and_obsolete_alias",
        elisp_form,
        expected,
    )
}

fn from_instruction_recognition_handles_platform_alias_comments_and_invalid_lines()
-> ParityBatchCase {
    let elisp_form = r###"
(let ((case-fold-search t))
  (mapcar
   (lambda (line)
     (if (string-match dockerfile--from-regex line)
         (list line :image (match-string 1 line) :alias (match-string 2 line)
               :match (match-string 0 line))
       (list line :no-match)))
   '("FROM alpine:3.20"
     "from --platform=linux/amd64 rust:1.79 AS builder"
     "  FROM scratch as final # ship this stage"
     "FROM ${REGISTRY}/team/app:${TAG}"
     "RUN echo FROM alpine"
     "FROM alpine unexpected-token")))
"###;
    let expected = expect![[
        r#"OK (("FROM alpine:3.20" :image "alpine:3.20" :alias nil :match "FROM alpine:3.20") ("from --platform=linux/amd64 rust:1.79 AS builder" :image "rust:1.79" :alias "builder" :match "from --platform=linux/amd64 rust:1.79 AS builder") ("  FROM scratch as final # ship this stage" :image "scratch" :alias "final" :match "  FROM scratch as final # ship this stage") ("FROM ${REGISTRY}/team/app:${TAG}" :image "${REGISTRY}/team/app:${TAG}" :alias nil :match "FROM ${REGISTRY}/team/app:${TAG}") ("RUN echo FROM alpine" :no-match) ("FROM alpine unexpected-token" :no-match))"#
    ]];
    ParityBatchCase::value(
        "from_instruction_recognition_handles_platform_alias_comments_and_invalid_lines",
        elisp_form,
        expected,
    )
}

#[test]
fn dockerfile_mode_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(DOCKERFILE_MODE_MELPA_PIN, "dockerfile-mode.el")
            .expect("prepare revision-pinned Dockerfile Mode below ./tmp")
            .with_timeout(Duration::from_secs(240))
            .with_prelude(PRELUDE),
        "dockerfile-mode-package-batch",
        "Dockerfile Mode",
        &[
            package_contract_configures_dockerfile_buffers_keys_files_and_defaults(),
            multistage_container_source_fontifies_roles_and_indexes_build_stages(),
            auto_indentation_aligns_continuations_idempotently_and_respects_disable_policy(),
            comment_editing_round_trips_instructions_and_preserves_hashes_inside_shell_strings(),
            production_build_options_generate_the_exact_safe_compilation_command(),
            remote_no_cache_build_uses_remote_localnames_without_contacting_the_host(),
            image_name_prompt_uses_buffer_configuration_history_and_obsolete_alias(),
            from_instruction_recognition_handles_platform_alias_comments_and_invalid_lines(),
        ],
    );
}
