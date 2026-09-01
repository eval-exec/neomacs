use expect_test::expect;

use super::ParityBatchCase;

fn alectryon_registers_its_complete_callable_surface_with_exact_signatures() -> ParityBatchCase {
    ParityBatchCase::value(
        "alectryon_registers_its_complete_callable_surface_with_exact_signatures",
        r##"(let ((functions
         '(alectryon--coq-exit-hook
           alectryon--prog-plist alectryon--text-plist alectryon--prog-mode-p
           alectryon--config alectryon--config-code+markup
           alectryon--config-markup alectryon--config-frontend
           alectryon--config-backend alectryon--read-mode
           alectryon--set-mode-variable alectryon-set-text-mode
           alectryon-set-prog-mode alectryon--guess-text-mode
           alectryon--available-text-modes alectryon--read-text-mode
           alectryon--ensure-text-mode-set alectryon--run-converter
           alectryon--converter-args alectryon--point-marker
           alectryon--convert-from alectryon--set-mode alectryon--toggle
           alectryon-toggle alectryon--parse-errors
           alectryon--flycheck-verify-enabled
           alectryon--in-literate-comment-p
           alectryon--prog-syntactic-face-function
           alectryon--block-font-lock-keywords
           alectryon--gutter-marker-modification-hook
           alectryon--gutter-font-lock-keywords
           alectryon--prog-font-lock-keywords
           alectryon--insert-literate-block
           alectryon--insert-literate-gutter
           alectryon-insert-literate-markers alectryon-newline
           alectryon-preview alectryon--in-original-mode alectryon--save
           alectryon-customize alectryon--record-mode
           alectryon--flyspell-hook alectryon--flyspell-unhook
           alectryon-mode alectryon--prog-presentation-font-lock-keywords
           alectryon-presentation-mode alectryon-mode-maybe-enable)))
  (mapcar
   (lambda (fn)
     (list fn
           (help-function-arglist fn t)
           (commandp fn)))
   functions))"##,
        expect![
            "OK ((alectryon--coq-exit-hook nil nil) (alectryon--prog-plist nil nil) (alectryon--text-plist nil nil) (alectryon--prog-mode-p (&optional mode) nil) (alectryon--config (prop &optional text-or-prog) nil) (alectryon--config-code+markup nil nil) (alectryon--config-markup nil nil) (alectryon--config-frontend (&optional mode) nil) (alectryon--config-backend (&optional mode) nil) (alectryon--read-mode (prog-p) nil) (alectryon--set-mode-variable (prog-p mode) nil) (alectryon-set-text-mode (mode) t) (alectryon-set-prog-mode (mode) t) (alectryon--guess-text-mode nil nil) (alectryon--available-text-modes nil nil) (alectryon--read-text-mode nil nil) (alectryon--ensure-text-mode-set nil nil) (alectryon--run-converter (input args) nil) (alectryon--converter-args (&optional mode) nil) (alectryon--point-marker nil nil) (alectryon--convert-from (mode) nil) (alectryon--set-mode (mode) nil) (alectryon--toggle nil nil) (alectryon-toggle nil t) (alectryon--parse-errors (output checker buffer) nil) (alectryon--flycheck-verify-enabled nil nil) (alectryon--in-literate-comment-p (&optional ppss) nil) (alectryon--prog-syntactic-face-function (state) nil) (alectryon--block-font-lock-keywords nil nil) (alectryon--gutter-marker-modification-hook (from to) nil) (alectryon--gutter-font-lock-keywords nil nil) (alectryon--prog-font-lock-keywords nil nil) (alectryon--insert-literate-block nil nil) (alectryon--insert-literate-gutter nil nil) (alectryon-insert-literate-markers nil t) (alectryon-newline (arg) t) (alectryon-preview nil t) (alectryon--in-original-mode nil nil) (alectryon--save nil nil) (alectryon-customize nil t) (alectryon--record-mode nil nil) (alectryon--flyspell-hook nil nil) (alectryon--flyspell-unhook nil nil) (alectryon-mode (&optional arg) t) (alectryon--prog-presentation-font-lock-keywords nil nil) (alectryon-presentation-mode (&optional arg) t) (alectryon-mode-maybe-enable nil nil))"
        ],
    )
}

fn alectryon_registers_exact_configuration_constants_faces_and_custom_contract() -> ParityBatchCase
{
    ParityBatchCase::value(
        "alectryon_registers_exact_configuration_constants_faces_and_custom_contract",
        r##"(list
 (featurep 'alectryon)
 alectryon-executable
 (eq (indirect-variable 'flycheck-alectryon-executable)
     'alectryon-executable)
 (get 'alectryon-executable 'custom-type)
 (get 'alectryon-executable 'custom-group)
 (get 'alectryon-executable 'risky-local-variable)
 alectryon-prog-modes
 alectryon-text-modes
 alectryon--error-levels
 alectryon--point-marker-template
 (mapcar
  (lambda (face)
    (list face (facep face) (get face 'face-documentation)
          (face-all-attributes face nil)))
  '(alectryon-comment alectryon-comment-marker alectryon-gutter))
 (mapcar
  (lambda (variable)
    (list variable (local-variable-if-set-p variable)
          (get variable 'permanent-local)))
  '(alectryon-prog-mode alectryon-text-mode alectryon--original-mode)))"##,
        expect![[
            r#"OK (t "alectryon" t file nil t ((coq-mode :tag "coq" :exit-hooks (alectryon--coq-exit-hook) :comment-delimiters ("(*|" . "|*)") :comment-delimiters-re ("([*]|" . "|[*])") :annotations-re "([*]\\(\\(?:\\s-*[.][-a-z]+\\)+\\)\\s-*[*])") (lean4-mode :tag "lean4" :exit-hooks nil :comment-delimiters ("/-|" . "|-/") :comment-delimiters-re ("/-|" . "|-/") :annotations-re "/-\\(\\(?:\\s-*[.][-a-z]+\\)+\\)\\s-*-/") (dafny-mode :tag "dafny" :exit-hooks nil :comment-delimiters ("/// ") :comment-delimiters-re ("^///") :annotations-re nil)) ((rst-mode :tag "rst" :lint t :suffixes ("_rst[.][^./]+$")) (markdown-mode :tag "md" :lint t :suffixes ("_md[.][^./]+$")) (typst-ts-mode :tag "typst" :lint nil :suffixes ("_typst[.][^./]+$"))) (("debug" . info) ("info" . info) ("warning" . warning) ("error" . error) ("severe" . error)) "￼%s￼" ((alectryon-comment [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] "Face used to highlight Alectryon comments." ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))) (alectryon-comment-marker [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] "Face used to highlight Alectryon comment delimiters." ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))) (alectryon-gutter [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] "Face used for gutter-style literate comment markers (e.g. ///)." ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified)))) ((alectryon-prog-mode t t) (alectryon-text-mode t t) (alectryon--original-mode t t)))"#
        ]],
    )
}

fn alectryon_keymaps_menu_and_flycheck_checker_form_one_practical_ui_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "alectryon_keymaps_menu_and_flycheck_checker_form_one_practical_ui_contract",
        r##"(list
 (lookup-key alectryon-mode-map (kbd "C-c C-S-a"))
 (lookup-key alectryon-prog-mode-map (kbd "C-c C-S-a"))
 (lookup-key alectryon-prog-mode-map (kbd "C-c C-="))
 (lookup-key alectryon-prog-mode-map [remap newline])
 (lookup-key alectryon-text-mode-map (kbd "C-c C-S-a"))
 (keymap-parent alectryon-prog-mode-map)
 (keymap-parent alectryon-text-mode-map)
 (easy-menu-get-map alectryon-prog-mode-map '("Alectryon"))
 (memq 'alectryon flycheck-checkers)
 (flycheck-checker-get 'alectryon 'command)
 (flycheck-checker-get 'alectryon 'standard-input)
 (flycheck-checker-get 'alectryon 'error-parser)
 (flycheck-checker-get 'alectryon 'modes)
 (functionp (flycheck-checker-get 'alectryon 'predicate))
 (functionp (flycheck-checker-get 'alectryon 'verify)))"##,
        expect![[
            r#"OK (alectryon-toggle alectryon-toggle alectryon-insert-literate-markers alectryon-newline alectryon-toggle #1=(keymap (3 keymap (33554433 . alectryon-toggle))) #1# (keymap "Alectryon") (alectryon ada-gnat asciidoctor awk-gawk bazel-build-buildifier bazel-module-buildifier bazel-starlark-buildifier bazel-workspace-buildifier c/c++-clang c/c++-gcc c/c++-cppcheck cfengine coffee css-stylelint cuda-nvcc cwl d-dmd dockerfile-hadolint elixir-credo emacs-lisp emacs-lisp-checkdoc ember-template erlang-rebar3 erlang fortran-gfortran go-gofmt go-vet go-build go-test go-errcheck go-unconvert go-staticcheck groovy haml haml-lint handlebars haskell-stack-ghc haskell-ghc haskell-hlint html-tidy javascript-eslint javascript-oxlint javascript-standard json-python-json json-jq jsonnet less less-stylelint llvm-llc lua-luacheck lua markdown-markdownlint-cli markdown-markdownlint-cli2 markdown-mdl markdown-pymarkdown nix opam org-lint perl perl-perlcritic perl-perlimports php php-phpmd php-phpcs php-phpcs-changed processing proselint protobuf-protoc pug puppet-parser puppet-lint python-flake8 python-ruff python-pylint python-pycompile python-pyright python-mypy r-lintr r racket rpm-rpmlint rst-sphinx rst ruby-rubocop ruby-chef-cookstyle ruby-standard ruby-reek ruby rust-cargo rust rust-clippy salt-lint scala scala-scalastyle scheme-chicken sass-stylelint scss-stylelint sh-bash sh-posix-dash sh-posix-bash sh-zsh sh-shellcheck slim slim-lint sql-sqlint statix systemd-analyze tcl-nagelfar terraform terraform-tflint tex-chktex tex-lacheck texinfo textlint verilog-verilator vhdl-ghdl xml-xmllint yaml-actionlint yaml-jsyaml yaml-yamllint eglot-check lsp) ("alectryon" "--stdin-filename" source-original "--frontend" (eval (alectryon--config-frontend)) "--backend" "lint" "-") t alectryon--parse-errors (coq-mode lean4-mode dafny-mode rst-mode markdown-mode) t t)"#
        ]],
    )
}

fn alectryon_installed_runtime_payload_is_minimal_exact_and_does_not_vendor_project_assets()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alectryon_installed_runtime_payload_is_minimal_exact_and_does_not_vendor_project_assets",
        r##"(let* ((directory
         (file-name-directory (getenv "NEOMACS_PACKAGE_SOURCE")))
        (files (sort (directory-files directory nil "\\`[^.]") #'string-lessp)))
  (list
   files
   (mapcar
    (lambda (file)
      (let ((path (expand-file-name file directory)))
        (list file
              (file-attribute-size (file-attributes path))
              (file-readable-p path))))
    files)
   (seq-filter
    (lambda (file)
      (string-match-p "\\.\\(css\\|js\\|svg\\|png\\|py\\|rst\\)\\'" file))
    files)))"##,
        expect![[
            r#"OK (("alectryon-autoloads.el" "alectryon-pkg.el" "alectryon.el" "alectryon.elc") (("alectryon-autoloads.el" 1826 t) ("alectryon-pkg.el" 500 t) ("alectryon.el" 29000 t) ("alectryon.elc" 35652 t)) nil)"#
        ]],
    )
}

fn alectryon_autoloads_register_only_supported_programming_mode_hooks() -> ParityBatchCase {
    ParityBatchCase::value(
        "alectryon_autoloads_register_only_supported_programming_mode_hooks",
        r##"(list
 (featurep 'alectryon)
 (autoloadp (symbol-function 'alectryon-mode))
 (autoloadp (symbol-function 'alectryon-toggle))
 (autoloadp (symbol-function 'alectryon-mode-maybe-enable))
 (memq 'alectryon-mode-maybe-enable coq-mode-hook)
 (memq 'alectryon-mode-maybe-enable lean4-mode-hook)
 (memq 'alectryon-mode-maybe-enable dafny-mode-hook)
 (boundp 'rst-mode-hook)
 (and (boundp 'rst-mode-hook)
      (memq 'alectryon-mode-maybe-enable rst-mode-hook))
 (boundp 'alectryon-prog-modes)
 (featurep 'flycheck))"##,
        expect![
            "OK (nil t t t (alectryon-mode-maybe-enable) (alectryon-mode-maybe-enable) (alectryon-mode-maybe-enable) t nil nil nil)"
        ],
    )
}

pub(super) fn registry_alectryon_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        alectryon_registers_its_complete_callable_surface_with_exact_signatures(),
        alectryon_registers_exact_configuration_constants_faces_and_custom_contract(),
        alectryon_keymaps_menu_and_flycheck_checker_form_one_practical_ui_contract(),
        alectryon_installed_runtime_payload_is_minimal_exact_and_does_not_vendor_project_assets(),
    ]
}

pub(super) fn registry_alectryon_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![alectryon_autoloads_register_only_supported_programming_mode_hooks()]
}
