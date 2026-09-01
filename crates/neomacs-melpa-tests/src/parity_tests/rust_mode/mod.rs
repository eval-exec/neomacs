use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, RUST_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const RUST_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const RUST_MODE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'imenu)
(require 'rust-mode)

(defvar rust-mode-test-root (make-temp-file "rust-mode-parity-" t))
(defvar rust-mode-test-bin
  (file-name-as-directory
   (expand-file-name "bin" rust-mode-test-root)))

(defun rust-mode-test-write (path text)
  (make-directory (file-name-directory path) t)
  (with-temp-file path
    (set-buffer-file-coding-system 'utf-8-unix)
    (insert text))
  path)

(defun rust-mode-test-read (path)
  (with-temp-buffer
    (insert-file-contents path)
    (buffer-string)))

(defun rust-mode-test-normalize (text)
  (replace-regexp-in-string
   (regexp-quote (directory-file-name rust-mode-test-root))
   "[PROJECT]" text t t))

(defun rust-mode-test-open (relative text)
  (find-file-noselect
   (rust-mode-test-write
    (expand-file-name relative rust-mode-test-root)
    text)))

(defun rust-mode-test-context (needle offset)
  (save-excursion
    (goto-char (point-min))
    (search-forward needle)
    (let* ((position (+ (- (point) (length needle)) offset))
           (state (syntax-ppss position)))
      (list
       :position position
       :depth (nth 0 state)
       :string (and (nth 3 state) t)
       :comment (and (nth 4 state) t)
       :start (nth 8 state)))))

(defun rust-mode-test-face (needle &optional nth)
  (save-excursion
    (goto-char (point-min))
    (dotimes (_ (or nth 1))
      (search-forward needle))
    (let ((start (- (point) (length needle))))
      (list needle (get-text-property start 'face)))))

(defun rust-mode-test-imenu-names ()
  (mapcar
   (lambda (group)
     (cons (car group) (mapcar #'car (cdr group))))
   (imenu--generic-function rust-imenu-generic-expression)))

(defun rust-mode-test-install-tools ()
  (make-directory rust-mode-test-bin t)
  (let ((cargo (expand-file-name "cargo" rust-mode-test-bin))
        (rustfmt (expand-file-name "rustfmt" rust-mode-test-bin)))
    (rust-mode-test-write
     cargo
     (concat
      "#!/bin/sh\n"
      "set -eu\n"
      "root=$(CDPATH= cd -- \"$(dirname -- \"$0\")/..\" && pwd)\n"
      "printf '%s\\n' \"$@\" >> \"$root/cargo-argv.log\"\n"
      "if test \"${1-}\" = locate-project; then\n"
      "  printf '{\"root\":\"%s/Cargo.toml\"}\\n' \"$root\"\n"
      "else\n"
      "  printf 'cargo fixture completed: %s\\n' \"$*\"\n"
      "fi\n"))
    (set-file-modes cargo #o755)
    (rust-mode-test-write
     rustfmt
     (concat
      "#!/bin/sh\n"
      "set -eu\n"
      "input=$(cat)\n"
      "case \"$input\" in\n"
      "  *BROKEN*)\n"
      "    printf 'error: expected an item\\n --> <stdin>:2:5\\n' >&2\n"
      "    exit 1\n"
      "    ;;\n"
      "esac\n"
      "printf '%s\\n' \"$input\" | sed 's/fn main(){/fn main() {/; s/let answer=41;/    let answer = 41;/'\n"))
    (set-file-modes rustfmt #o755)))

(rust-mode-test-install-tools)
"####;

fn rust_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(RUST_MODE_MELPA_PIN, "rust-mode.el")
        .expect("prepare pinned rust-mode source below ./tmp")
        .with_prelude(RUST_MODE_TEST_PRELUDE)
        .with_timeout(RUST_MODE_TEST_TIMEOUT)
}

fn mode_activation_and_syntax_propertization_distinguish_rust_literals_comments_generics_and_operators()
-> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (rust-mode)
  (insert
   (concat
    "pub fn parse<T: AsRef<str>>(input: T) -> bool {\n"
    "    let raw = r###\"literal \\\"# and <tag>\"###;\n"
    "    let quote = 'λ';\n"
    "    /* outer /* inner */ done */\n"
    "    input.as_ref().len() < 10\n"
    "}\n"))
  (syntax-propertize (point-max))
  (goto-char (point-min))
  (search-forward "parse<")
  (backward-char 1)
  (let ((generic
         (buffer-substring-no-properties
          (point) (scan-sexps (point) 1))))
    (goto-char (point-min))
    (search-forward " < 10")
    (backward-char 4)
    (list
     :mode major-mode
     :parent (get major-mode 'derived-mode-parent)
     :indent indent-line-function
     :comment (list comment-start comment-end comment-multi-line)
     :parse-properties parse-sexp-lookup-properties
     :generic generic
     :comparison-syntax (char-syntax (char-after))
     :raw (rust-mode-test-context "literal" 2)
     :character (rust-mode-test-context "λ" 0)
     :nested-comment (rust-mode-test-context "inner" 2))))
"####;
    let expect = expect![[
        r####"OK (:mode rust-mode :parent prog-mode :indent rust-mode-indent-line :comment ("// " "" t) :parse-properties t :generic "<T: AsRef<str>>" :comparison-syntax 40 :raw (:position 70 :depth 1 :string t :comment nil :start 63) :character (:position 112 :depth 1 :string t :comment nil :start 111) :nested-comment (:position 134 :depth 1 :string nil :comment t :start 120))"####
    ]];
    ParityBatchCase::value(
        "mode_activation_and_syntax_propertization_distinguish_rust_literals_comments_generics_and_operators",
        elisp_form,
        expect,
    )
}

fn region_indentation_formats_impl_where_match_and_method_chain_as_a_real_edit() -> ParityBatchCase
{
    let elisp_form = r####"
(with-temp-buffer
  (rust-mode)
  (let ((rust-indent-method-chain t)
        (rust-indent-where-clause t))
    (insert
     (concat
      "impl<T> Publisher<T>\n"
      "where\n"
      "T: Send + Sync,\n"
      "{\n"
      "fn publish(&self, value: Option<T>) -> Result<T, Error> {\n"
      "match value {\n"
      "Some(item) => client\n"
      ".prepare(item)\n"
      ".send(),\n"
      "None => Err(Error::Missing),\n"
      "}\n"
      "}\n"
      "}\n"))
    (goto-char (point-min))
    (forward-line 7)
    (let ((tracked (copy-marker (point) t)))
      (indent-region (point-min) (point-max))
      (list
       :buffer
       (buffer-substring-no-properties (point-min) (point-max))
       :tracked-line (line-number-at-pos tracked)
       :tracked-column
       (save-excursion (goto-char tracked) (current-column))
       :modified (buffer-modified-p)
       :deactivate-mark deactivate-mark))))
"####;
    let expect = expect![[
        r####"OK (:buffer "impl<T> Publisher<T>\n    where\n    T: Send + Sync,\n{\n    fn publish(&self, value: Option<T>) -> Result<T, Error> {\n\11match value {\n\11    Some(item) => client\n\11\11.prepare(item)\n\11\11.send(),\n\11    None => Err(Error::Missing),\n\11}\n    }\n}\n" :tracked-line 8 :tracked-column 16 :modified t :deactivate-mark t)"####
    ]];
    ParityBatchCase::value(
        "region_indentation_formats_impl_where_match_and_method_chain_as_a_real_edit",
        elisp_form,
        expect,
    )
}

fn font_lock_classifies_real_declarations_attributes_bindings_types_macros_and_interpolation()
-> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (rust-mode)
  (insert
   (concat
    "#[derive(Debug)]\n"
    "pub async fn publish<'a, T: Display>(mut item: T, label: &'a str) -> Result<u32, Error> {\n"
    "    let mut count: u32 = 41u32;\n"
    "    println!(\"publishing {label}: {item:?}\");\n"
    "    Ok(count)\n"
    "}\n"))
  (font-lock-ensure (point-min) (point-max))
  (list
   :faces
   (list
    (rust-mode-test-face "#[derive(Debug)]")
    (rust-mode-test-face "pub")
    (rust-mode-test-face "async")
    (rust-mode-test-face "publish")
    (rust-mode-test-face "item")
    (rust-mode-test-face "Display")
    (rust-mode-test-face "u32")
    (rust-mode-test-face "println!")
    (rust-mode-test-face "{label}")
    (rust-mode-test-face "{item:?}"))
   :keyword-count
   (save-excursion
     (goto-char (point-min))
     (let ((count 0))
       (while (re-search-forward "\\_<\\(pub\\|async\\|fn\\|let\\|mut\\)\\_>" nil t)
         (when (eq (get-text-property (match-beginning 0) 'face)
                   'font-lock-keyword-face)
           (setq count (1+ count))))
       count))))
"####;
    let expect = expect![[
        r####"OK (:faces (("#[derive(Debug)]" font-lock-preprocessor-face) ("pub" font-lock-keyword-face) ("async" font-lock-keyword-face) ("publish" font-lock-function-name-face) ("item" font-lock-variable-name-face) ("Display" font-lock-type-face) ("u32" font-lock-type-face) ("println!" rust-builtin-formatting-macro) ("{label}" rust-string-interpolation) ("{item:?}" rust-string-interpolation)) :keyword-count 6)"####
    ]];
    ParityBatchCase::value(
        "font_lock_classifies_real_declarations_attributes_bindings_types_macros_and_interpolation",
        elisp_form,
        expect,
    )
}

fn imenu_and_defun_motion_navigate_real_functions_impls_traits_and_macros() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (rust-mode)
  (insert
   (concat
    "pub struct Release { version: u32 }\n\n"
    "trait Publish {\n    fn publish(&self);\n}\n\n"
    "impl Publish for Release {\n"
    "    fn publish(&self) { println!(\"{}\", self.version); }\n"
    "}\n\n"
    "macro_rules! release { ($v:expr) => { Release { version: $v } } }\n\n"
    "pub(crate) async fn deploy() {\n    let _r = release!(41);\n}\n"))
  (let ((index (rust-mode-test-imenu-names))
        begin end marked)
    (goto-char (point-min))
    (search-forward "println!")
    (rust-beginning-of-defun)
    (setq begin (list (line-number-at-pos) (current-column)))
    (rust-end-of-defun)
    (setq end (list (line-number-at-pos) (current-column)))
    (goto-char (point-max))
    (beginning-of-defun)
    (mark-defun)
    (setq marked
          (buffer-substring-no-properties
           (region-beginning) (region-end)))
    (list :index index :begin begin :end end :marked marked)))
"####;
    let expect = expect![[
        r####"OK (:index (("Macro" "release") ("Impl" "Publish") ("Trait" "Publish") ("Fn" "publish" "publish" "deploy") ("Struct" "Release")) :begin (8 0) :end (8 55) :marked "\npub(crate) async fn deploy() {\n    let _r = release!(41);\n}\n")"####
    ]];
    ParityBatchCase::value(
        "imenu_and_defun_motion_navigate_real_functions_impls_traits_and_macros",
        elisp_form,
        expect,
    )
}

fn documentation_fill_preserves_doc_comment_kinds_paragraph_boundaries_and_indentation()
-> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (rust-mode)
  (let ((fill-column 42)
        (sentence-end-double-space t)
        (colon-double-space nil))
    (insert
     (concat
      "impl Release {\n"
      "    /// Publish the selected artifact with a very descriptive label and stable metadata.\n"
      "    ///\n"
      "    /// Returns the deployed version and preserves the original release notes.\n"
      "    fn publish(&self) {}\n"
      "}\n"))
    (goto-char (point-min))
    (search-forward "selected")
    (fill-paragraph)
    (search-forward "Returns")
    (fill-paragraph)
    (list
     :buffer (buffer-string)
     :paragraphs
     (save-excursion
       (goto-char (point-min))
       (let (lines)
         (while (re-search-forward "^[[:space:]]*///.*$" nil t)
           (push (match-string-no-properties 0) lines))
         (nreverse lines))))))
"####;
    let expect = expect![[
        r####"OK (:buffer "impl Release {\n    /// Publish the selected artifact with\n    /// a very descriptive label and\n    /// stable metadata.\n    ///\n    /// Returns the deployed version and\n    /// preserves the original release\n    /// notes.\n    fn publish(&self) {}\n}\n" :paragraphs ("    /// Publish the selected artifact with" "    /// a very descriptive label and" "    /// stable metadata." "    ///" "    /// Returns the deployed version and" "    /// preserves the original release" "    /// notes."))"####
    ]];
    ParityBatchCase::value(
        "documentation_fill_preserves_doc_comment_kinds_paragraph_boundaries_and_indentation",
        elisp_form,
        expect,
    )
}

fn editing_commands_toggle_mutability_and_wrap_then_unwrap_a_debug_expression() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (rust-mode)
  (insert
   (concat
    "fn publish(&self) {\n"
    "    let value = compute(config);\n"
    "    let config = & Config::default();\n"
    "    send(compute(&config));\n"
    "}\n"))
  (goto-char (point-min))
  (rust-toggle-mutability)
  (forward-line 1)
  (rust-toggle-mutability)
  (forward-line 1)
  (rust-toggle-mutability)
  (goto-char (point-min))
  (search-forward "compute(&config)")
  (let ((end (point)))
    (search-backward "compute")
    (push-mark (point) t t)
    (goto-char end)
    (activate-mark)
    (rust-dbg-wrap-or-unwrap))
  (let ((wrapped (buffer-string))
        wrapped-point)
    (setq wrapped-point (point))
    (deactivate-mark)
    (search-backward "dbg!")
    (forward-char 2)
    (rust-dbg-wrap-or-unwrap)
    (list
     :wrapped wrapped
     :wrapped-point wrapped-point
     :unwrapped (buffer-string)
     :final-point (point))))
"####;
    let expect = expect![[
        r####"OK (:wrapped "fn publish(&mut self) {\n    let mut value = compute(config);\n    let config = &mut  Config::default();\n    send(dbg!(compute(&config)));\n}\n" :wrapped-point 135 :unwrapped "fn publish(&mut self) {\n    let mut value = compute(config);\n    let config = &mut  Config::default();\n    send(compute(&config));\n}\n" :final-point 113)"####
    ]];
    ParityBatchCase::value(
        "editing_commands_toggle_mutability_and_wrap_then_unwrap_a_debug_expression",
        elisp_form,
        expect,
    )
}

fn cargo_project_discovery_and_commands_use_manifest_root_environment_and_user_arguments()
-> ParityBatchCase {
    let elisp_form = r####"
(let* ((manifest
        (rust-mode-test-write
         (expand-file-name "Cargo.toml" rust-mode-test-root)
         "[package]\nname = \"release-fixture\"\nversion = \"0.1.0\"\n"))
       (source
        (rust-mode-test-open
         "src/lib.rs"
         "pub fn publish() -> u32 { 41 }\n"))
       (cargo (expand-file-name "cargo" rust-mode-test-bin))
       (log (expand-file-name "cargo-argv.log" rust-mode-test-root))
       project command)
  (when (file-exists-p log) (delete-file log))
  (with-current-buffer source
    (rust-mode)
    (let ((rust-cargo-bin cargo)
          (rust-cargo-locate-default-arguments '("--workspace"))
          (rust-cargo-default-arguments "--all-targets --features ui"))
      (setq project (rust-buffer-project))
      (cl-letf
          (((symbol-function 'compile)
            (lambda (compile-command &optional comint)
              (setq command
                    (list
                     :command (rust-mode-test-normalize compile-command)
                     :comint comint
                     :directory
                     (rust-mode-test-normalize default-directory)))
              'captured-compilation-buffer)))
        (rust-check))))
  (list
   :manifest (rust-mode-test-normalize manifest)
   :project (rust-mode-test-normalize project)
   :command command
   :cargo-argv (split-string (rust-mode-test-read log) "\n" t)))
"####;
    let expect = expect![[
        r####"OK (:manifest "[PROJECT]/Cargo.toml" :project "[PROJECT]/Cargo.toml" :command (:command "[PROJECT]/bin/cargo check --all-targets --features ui" :comint nil :directory "[PROJECT]/") :cargo-argv ("locate-project" "--workspace" "locate-project" "--workspace"))"####
    ]];
    ParityBatchCase::value(
        "cargo_project_discovery_and_commands_use_manifest_root_environment_and_user_arguments",
        elisp_form,
        expect,
    )
}

fn rustfmt_success_preserves_markers_and_failure_creates_navigable_diagnostics() -> ParityBatchCase
{
    let elisp_form = r####"
(let* ((rustfmt (expand-file-name "rustfmt" rust-mode-test-bin))
       (source
        (rust-mode-test-open
         "src/main.rs"
         "fn main(){\nlet answer=41;\nprintln!(\"{}\", answer);\n}\n"))
       success failure)
  (with-current-buffer source
    (rust-mode)
    (goto-char (point-min))
    (search-forward "answer")
    (let ((tracked (copy-marker (point) t))
          (rust-rustfmt-bin rustfmt)
          (rust-format-show-buffer nil)
          (rust-format-goto-problem nil))
      (setq success
            (list
             :message (rust-format-buffer)
             :buffer (buffer-string)
             :point (point)
             :marker (marker-position tracked)
             :marker-line (line-number-at-pos tracked)
             :modified (buffer-modified-p)))))
  (with-current-buffer (get-buffer-create "rustfmt-broken.rs")
    (erase-buffer)
    (insert "fn main() {\n    BROKEN\n}\n")
    (rust-mode)
    (let ((rust-rustfmt-bin rustfmt)
          (rust-format-show-buffer nil)
          (rust-format-goto-problem nil))
      (setq failure
            (condition-case error-data
                (progn (rust-format-buffer) :unexpected-success)
              (error
               (list
                :error error-data
                :source (buffer-string)
                :diagnostics
                (with-current-buffer rust-rustfmt-buffername
                  (list
                   :mode major-mode
                   :read-only buffer-read-only
                   :command (rust-mode-test-normalize compile-command)
                   :text (buffer-substring-no-properties
                          (point-min) (point-max)))))))))
    (kill-buffer (current-buffer)))
  (when (get-buffer rust-rustfmt-buffername)
    (kill-buffer rust-rustfmt-buffername))
  (list :success success :failure failure))
"####;
    let expect = expect![[
        r####"OK (:success (:message "Formatted buffer with rustfmt." :buffer "fn main() {\n    let answer = 41;\nprintln!(\"{}\", answer);\n}\n" :point 27 :marker 28 :marker-line 2 :modified t) :failure (:error (error "Rustfmt failed because of parsing errors, see *rustfmt* buffer for details") :source "fn main() {\n    BROKEN\n}\n" :diagnostics (:mode rust-format-mode :read-only t :command "[PROJECT]/bin/rustfmt nil" :text "error: expected an item\n --> rustfmt-broken.rs:2:5\n")))"####
    ]];
    ParityBatchCase::value(
        "rustfmt_success_preserves_markers_and_failure_creates_navigable_diagnostics",
        elisp_form,
        expect,
    )
}

#[test]
fn rust_mode_package_batch() {
    let cases = vec![
        mode_activation_and_syntax_propertization_distinguish_rust_literals_comments_generics_and_operators(),
        region_indentation_formats_impl_where_match_and_method_chain_as_a_real_edit(),
        font_lock_classifies_real_declarations_attributes_bindings_types_macros_and_interpolation(),
        imenu_and_defun_motion_navigate_real_functions_impls_traits_and_macros(),
        documentation_fill_preserves_doc_comment_kinds_paragraph_boundaries_and_indentation(),
        editing_commands_toggle_mutability_and_wrap_then_unwrap_a_debug_expression(),
        cargo_project_discovery_and_commands_use_manifest_root_environment_and_user_arguments(),
        rustfmt_success_preserves_markers_and_failure_creates_navigable_diagnostics(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed rust-mode parity test");
    assert_oracle_batch_cases(rust_mode_oracle(), test_name, "rust_mode_parity", &cases);
}
