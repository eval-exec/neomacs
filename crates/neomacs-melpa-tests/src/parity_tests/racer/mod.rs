use std::time::Duration;

use expect_test::expect;

use crate::{
    CachedMelpaOracle, DASH_MELPA_PIN, F_MELPA_PIN, POS_TIP_MELPA_PIN, RACER_MELPA_PIN,
    RUST_MODE_MELPA_PIN, S_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const RACER_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const RACER_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'racer)

(defun racer-test-property-runs (string property)
  (let ((position 0)
        runs)
    (while (< position (length string))
      (let* ((value (get-text-property position property string))
             (next (or (next-single-property-change
                        position property string)
                       (length string))))
        (when value
          (push (list position next (copy-tree value)) runs))
        (setq position next)))
    (nreverse runs)))

(defun racer-test-buttons (string)
  (let ((position 0)
        buttons)
    (while (< position (length string))
      (if (get-text-property position 'button string)
          (let* ((button (cons string position))
                 (end (or (next-single-property-change
                           position 'button string)
                          (length string))))
              (push
               (list
                :range (list position end)
                :label (substring-no-properties string position end)
                :type (button-type button)
                :help-args (copy-tree (button-get button 'help-args))
                :path (button-get button 'path)
                :line (button-get button 'line)
                :column (button-get button 'column))
               buttons)
              (setq position end))
        (setq position
              (or (next-single-property-change
                   position 'button string)
                  (length string)))))
    (nreverse buttons)))

(defun racer-test-text-summary (string)
  (list
   :text (substring-no-properties string)
   :faces (racer-test-property-runs string 'face)
   :buttons (racer-test-buttons string)))

(defun racer-test-capf-summary (capf)
  (when capf
    (list
     :bounds (list (nth 0 capf) (nth 1 capf))
     :prefix (buffer-substring-no-properties
              (nth 0 capf) (nth 1 capf))
     :annotation (plist-get (nthcdr 3 capf) :annotation-function)
     :prefix-length (plist-get (nthcdr 3 capf) :company-prefix-length)
     :docsig (plist-get (nthcdr 3 capf) :company-docsig)
     :doc-buffer (plist-get (nthcdr 3 capf) :company-doc-buffer)
     :location (plist-get (nthcdr 3 capf) :company-location)
     :exit (plist-get (nthcdr 3 capf) :exit-function))))

(defun racer-test-normalize-root (value root)
  (replace-regexp-in-string
   (regexp-quote (directory-file-name root))
   "[PROJECT]"
   value t t))

(defvar racer-test--real-make-process nil)

(defun racer-test--make-process-with-silent-sentinel (&rest arguments)
  (let ((process (apply racer-test--real-make-process arguments)))
    (set-process-sentinel process #'ignore)
    (let* ((stderr-buffer (plist-get arguments :stderr))
           (stderr-process
            (and (bufferp stderr-buffer)
                 (get-buffer-process stderr-buffer))))
      (when stderr-process
        (set-process-sentinel stderr-process #'ignore)))
    process))

(defmacro racer-test-with-silent-process-sentinels (&rest body)
  (declare (indent 0) (debug t))
  `(let ((racer-test--real-make-process
          (symbol-function 'make-process)))
     (cl-letf (((symbol-function 'make-process)
                #'racer-test--make-process-with-silent-sentinel))
       ,@body)))
"##;

fn racer_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(RACER_MELPA_PIN, "racer.el")
        .expect("prepare pinned racer source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare pinned dash dependency")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare pinned s dependency")
        .with_melpa_dependency(F_MELPA_PIN)
        .expect("prepare pinned f dependency")
        .with_melpa_dependency(POS_TIP_MELPA_PIN)
        .expect("prepare pinned pos-tip dependency")
        .with_melpa_dependency(RUST_MODE_MELPA_PIN)
        .expect("prepare pinned rust-mode dependency")
        .with_prelude(RACER_TEST_PRELUDE)
        .with_timeout(RACER_TEST_TIMEOUT)
}

fn real_subprocess_completion_uses_cargo_root_environment_and_temporary_buffer_snapshot()
-> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (make-temp-file "racer-project-" t))
       (source-dir (expand-file-name "src/" root))
       (rust-src (expand-file-name "rust-src/" root))
       (cargo-home (expand-file-name "cargo-home/" root))
       (source-file (expand-file-name "main.rs" source-dir))
       (program (expand-file-name "fake-racer" root)))
  (unwind-protect
      (progn
        (make-directory source-dir t)
        (make-directory rust-src t)
        (make-directory cargo-home t)
        (with-temp-file (expand-file-name "Cargo.toml" root)
          (insert "[package]\nname = \"parity-app\"\nversion = \"0.1.0\"\n"))
        (with-temp-file source-file
          (insert "fn main() {}\n"))
        (with-temp-file program
          (insert
           (format
            (concat
             "#!/bin/sh\n"
             "printf '%%s\\n' 'PREFIX 3,8,ins' "
             "'MATCH insert,24,8,%s,Function,pub fn insert(&mut self, idx: usize, value: T)' "
             "'MATCH inspect,61,4,%s,Module,src/main.rs' 'END'\n")
            source-file source-file)))
        (set-file-modes program #o755)
        (let ((racer-cmd program)
              (racer-rust-src-path rust-src)
              (racer-cargo-home cargo-home)
              candidates
              state)
          (with-temp-buffer
            (setq default-directory source-dir)
            (set-visited-file-name source-file t t)
            (insert "fn main() {\n    values.ins\n}\n")
            (rust-mode)
            (goto-char (point-min))
            (search-forward "values.ins")
            (setq candidates
                  (racer-test-with-silent-process-sentinels
                    (racer-complete))
                  state racer--prev-state))
          (list
           :candidates
           (mapcar
            (lambda (candidate)
              (list
               :name (substring-no-properties candidate)
               :kind (get-text-property 0 'matchtype candidate)
               :line (get-text-property 0 'line candidate)
               :column (get-text-property 0 'col candidate)
               :file (file-relative-name
                      (get-text-property 0 'file candidate) root)
               :context (get-text-property 0 'ctx candidate)
               :annotation (racer-complete--annotation candidate)
               :docsig
               (substring-no-properties
                (racer-complete--docsig candidate))
               :location
               (let ((location (racer-complete--location candidate)))
                 (cons (file-relative-name (car location) root)
                       (cdr location)))))
            candidates)
           :invocation
           (let ((args (plist-get state :args)))
             (list
              :program (file-relative-name
                        (plist-get state :program) root)
              :command (nth 0 args)
              :line (nth 1 args)
              :column (nth 2 args)
              :source (file-relative-name (nth 3 args) root)
              :argument-count (length args)
              :temporary-file-deleted
              (not (file-exists-p (car (last args))))
              :working-directory
              (file-relative-name
               (plist-get state :default-directory) root)
              :rust-src
              (and (member (concat "RUST_SRC_PATH=" rust-src)
                           (plist-get state :process-environment))
                   t)
              :cargo-home
              (and (member (concat "CARGO_HOME=" cargo-home)
                           (plist-get state :process-environment))
                   t))))))
    (delete-directory root t)))
"##;
    let expect = expect![[
        r##"OK (:candidates ((:name "insert" :kind "Function" :line 24 :column 8 :file "src/main.rs" :context "pub fn insert(&mut self, idx: usize, value: T)" :annotation "(&mut self, idx: usize, value: T) : Function" :docsig "pub fn insert(&mut self, idx: usize, value: T)" :location ("src/main.rs" . 24)) (:name "inspect" :kind "Module" :line 61 :column 4 :file "src/main.rs" :context "src/main.rs" :annotation " src/main.rs : Module" :docsig "src/main.rs" :location ("src/main.rs" . 61))) :invocation (:program "fake-racer" :command "complete" :line "2" :column "14" :source "src/main.rs" :argument-count 5 :temporary-file-deleted t :working-directory "./" :rust-src t :cargo-home t))"##
    ]];
    ParityBatchCase::value(
        "real_subprocess_completion_uses_cargo_root_environment_and_temporary_buffer_snapshot",
        elisp_form,
        expect,
    )
}

fn minor_mode_installs_rust_completion_contract_and_respects_comment_and_string_contexts()
-> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (rust-mode)
  (insert "let answer = values.ins;\n// values.ins\nlet text = \"values.ins\";\n")
  (racer-mode 1)
  (let ((enabled
         (list
          :mode racer-mode
          :eldoc eldoc-documentation-function
          :capf completion-at-point-functions
          :binding (lookup-key racer-mode-map (kbd "M-."))))
        code comment comment-enabled string disabled)
    (goto-char (point-min))
    (search-forward "values.ins")
    (setq code (racer-test-capf-summary (racer-complete-at-point)))
    (search-forward "values.ins")
    (setq comment (racer-complete-at-point))
    (let ((racer-complete-in-comments t))
      (setq comment-enabled
            (racer-test-capf-summary (racer-complete-at-point))))
    (search-forward "values.ins")
    (setq string (racer-complete-at-point))
    (racer-mode 0)
    (setq disabled
          (list
           :mode racer-mode
           :eldoc eldoc-documentation-function
           :capf completion-at-point-functions))
    (list
     :enabled enabled
     :code code
     :comment-default comment
     :comment-enabled comment-enabled
     :string string
     :disabled disabled)))
"##;
    let expect = expect![[
        "OK (:enabled (:mode t :eldoc racer-eldoc :capf (racer-complete-at-point) :binding racer-find-definition) :code (:bounds (21 24) :prefix \"ins\" :annotation racer-complete--annotation :prefix-length t :docsig racer-complete--docsig :doc-buffer racer--describe :location racer-complete--location :exit racer-complete--insert-args) :comment-default nil :comment-enabled (:bounds (36 39) :prefix \"ins\" :annotation racer-complete--annotation :prefix-length t :docsig racer-complete--docsig :doc-buffer racer--describe :location racer-complete--location :exit racer-complete--insert-args) :string nil :disabled (:mode nil :eldoc racer-eldoc :capf (racer-complete-at-point)))"
    ]];
    ParityBatchCase::value(
        "minor_mode_installs_rust_completion_contract_and_respects_comment_and_string_contexts",
        elisp_form,
        expect,
    )
}

fn racer_protocol_parser_handles_escaped_fields_overloads_and_malformed_records() -> ParityBatchCase
{
    let elisp_form = r##"
(let* ((short
        (racer--split-snippet-match
         "MATCH open;open(path);12;3;/rust/fs.rs;Function;pub fn open(path: &Path);\"Open a file.\""))
       (long
        (racer--split-snippet-match
         "MATCH open;open(path, opts);30;7;/rust/options.rs;Function;pub fn open(path: &Path, opts: Options);\"Open with retry\\; audit and two lines\\nSecond line.\""))
       (malformed
        (racer--split-snippet-match
         "MATCH broken;broken();1;0;/rust/lib.rs;Function;sig"))
       (ordered (racer--order-descriptions (list short long))))
  (list
   :split-fields
   (racer--split-parts
    "MATCH item;\"display\\;name\";8;2;/rust/lib.rs;Struct;Item;\"Line one\\nLine two\"")
   :ordered
   (mapcar
    (lambda (description)
      (list
       :name (plist-get description :name)
       :line (plist-get description :line)
       :column (plist-get description :column)
       :path (plist-get description :path)
       :kind (plist-get description :kind)
       :signature (plist-get description :signature)
       :docstring (plist-get description :docstring)))
    ordered)
   :malformed malformed))
"##;
    let expect = expect![[
        r##"OK (:split-fields ("MATCH item" "display;name" "8" "2" "/rust/lib.rs" "Struct" "Item" "Line one\nLine two") :ordered ((:name "open" :line 30 :column 7 :path "/rust/options.rs" :kind "Function" :signature "pub fn open(path: &Path, opts: Options)" :docstring "Open with retry; audit and two lines\nSecond line.") (:name "open" :line 12 :column 3 :path "/rust/fs.rs" :kind "Function" :signature "pub fn open(path: &Path)" :docstring "Open a file.")) :malformed nil)"##
    ]];
    ParityBatchCase::value(
        "racer_protocol_parser_handles_escaped_fields_overloads_and_malformed_records",
        elisp_form,
        expect,
    )
}

fn rust_documentation_renderer_preserves_structure_code_and_navigation_metadata() -> ParityBatchCase
{
    let elisp_form = r####"
(let* ((markdown
        "# Vec operations
Build a [`Vec`](../vec/index.html) and call `push`.
Read the [allocation guide](https://example.test/alloc).

```rust
# #[allow(dead_code)]
let mut values = Vec::new();
values.push(3);
```

[`Vec`]: ../vec/struct.Vec.html")
       (rendered (racer--propertize-docstring markdown)))
  (racer-test-text-summary rendered))
"####;
    let expect = expect![[
        r##"OK (:text "Vec operations\n\nBuild a Vec and call push.\nRead the allocation guide.\n\n    let mut values = Vec::new();\n    values.push(3);\n\n" :faces ((0 14 racer-help-heading-face) (24 27 font-lock-type-face) (37 41 font-lock-variable-name-face) (52 68 button) (75 78 font-lock-keyword-face) (79 82 font-lock-keyword-face) (83 89 font-lock-variable-name-face) (92 95 font-lock-type-face)) :buttons ((:range (52 68) :label "allocation guide" :type help-url :help-args ("https://example.test/alloc") :path nil :line nil :column nil)))"##
    ]];
    ParityBatchCase::value(
        "rust_documentation_renderer_preserves_structure_code_and_navigation_metadata",
        elisp_form,
        expect,
    )
}

fn describe_builds_read_only_help_with_source_and_web_buttons_for_real_symbol_records()
-> ParityBatchCase {
    let elisp_form = r##"
(let (summary)
  (unwind-protect
      (cl-letf
          (((symbol-function 'racer--describe-at-point)
            (lambda (_name)
              (list
               '(:name "insert"
                 :line 24
                 :column 8
                 :path "/workspace/collections/src/vec.rs"
                 :kind "Function"
                 :signature "pub fn insert(&mut self, index: usize, element: T)"
                 :docstring "Insert `element` before `index`. See [ownership](https://example.test/ownership).")
               '(:name "insert"
                 :line 91
                 :column 4
                 :path "/workspace/map/src/entry.rs"
                 :kind "Method"
                 :signature "pub fn insert(&mut self, value: V) -> V"
                 :docstring nil)))))
        (let ((buffer (racer--describe "insert")))
          (with-current-buffer buffer
            (setq summary
                  (list
                   :buffer-name (buffer-name)
                   :mode major-mode
                   :read-only buffer-read-only
                   :contents
                   (racer-test-text-summary (buffer-string)))))))
    (when (get-buffer "*Racer Help*")
      (kill-buffer "*Racer Help*")))
  summary)
"##;
    let expect = expect![[
        r##"OK (:buffer-name "*Racer Help*" :mode racer-help-mode :read-only t :contents (:text "insert is a function defined in src/vec.rs.\n\n    pub fn insert(&mut self, index: usize, element: T)\n\nInsert element before index. See ownership.\n---------------------------------------------------------------\ninsert is a method defined in src/entry.rs.\n\n    pub fn insert(&mut self, value: V) -> V\n\nNot documented." :faces ((32 42 button) (49 52 font-lock-keyword-face) (53 55 font-lock-keyword-face) (56 62 font-lock-function-name-face) (63 64 rust-ampersand-face) (64 67 font-lock-keyword-face) (68 72 font-lock-keyword-face) (74 79 font-lock-variable-name-face) (81 86 font-lock-type-face) (88 95 font-lock-variable-name-face) (97 98 font-lock-type-face) (108 115 font-lock-variable-name-face) (123 128 font-lock-variable-name-face) (134 143 button) (239 251 button) (258 261 font-lock-keyword-face) (262 264 font-lock-keyword-face) (265 271 font-lock-function-name-face) (272 273 rust-ampersand-face) (273 276 font-lock-keyword-face) (277 281 font-lock-keyword-face) (283 288 font-lock-variable-name-face) (290 291 font-lock-type-face) (296 297 font-lock-type-face)) :buttons ((:range (32 42) :label "src/vec.rs" :type racer-src-button :help-args nil :path "/workspace/collections/src/vec.rs" :line 24 :column 8) (:range (134 143) :label "ownership" :type help-url :help-args ("https://example.test/ownership") :path nil :line nil :column nil) (:range (239 251) :label "src/entry.rs" :type racer-src-button :help-args nil :path "/workspace/map/src/entry.rs" :line 91 :column 4))))"##
    ]];
    ParityBatchCase::value(
        "describe_builds_read_only_help_with_source_and_web_buttons_for_real_symbol_records",
        elisp_form,
        expect,
    )
}

fn definition_navigation_opens_reported_file_and_moves_to_exact_utf8_source_location()
-> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (make-temp-file "racer-definition-" t))
       (source (expand-file-name "src/library.rs" root))
       opened
       result
       opened-buffer)
  (unwind-protect
      (progn
        (make-directory (file-name-directory source) t)
        (with-temp-file source
          (insert "// λ utilities\n\npub fn publish_artifact() {\n    println!(\"done\");\n}\n"))
        (with-temp-buffer
          (insert "publish_artifact();")
          (goto-char (point-min))
          (cl-letf
              (((symbol-function 'racer--call-at-point)
                (lambda (_command)
                  (list
                   "PREFIX 1,17,publish_artifact"
                   (format
                    "MATCH publish_artifact,3,7,%s,Function,pub fn publish_artifact()"
                    source)
                   "END"))))
            (racer--find-definition
             (lambda (path)
               (setq opened path
                     opened-buffer (find-file-noselect path))
               (switch-to-buffer opened-buffer)))
            (setq result
                  (list
                   :opened (file-relative-name opened root)
                   :line (line-number-at-pos)
                   :column (current-column)
                   :symbol (thing-at-point 'symbol t)
                   :line-text
                   (buffer-substring-no-properties
                    (line-beginning-position)
                    (line-end-position)))))))
    (when (buffer-live-p opened-buffer)
      (kill-buffer opened-buffer))
    (delete-directory root t))
  result)
"##;
    let expect = expect![[
        r##"OK (:opened "src/library.rs" :line 3 :column 7 :symbol "publish_artifact" :line-text "pub fn publish_artifact() {")"##
    ]];
    ParityBatchCase::value(
        "definition_navigation_opens_reported_file_and_moves_to_exact_utf8_source_location",
        elisp_form,
        expect,
    )
}

fn completed_functions_insert_non_self_arguments_and_eldoc_resolves_nested_call_signature()
-> ParityBatchCase {
    let elisp_form = r##"
(let ((function (copy-sequence "insert"))
      (module (copy-sequence "collections"))
      templatified
      inserted
      already-called
      non-function
      eldoc-result
      point-during-completion)
  (put-text-property
   0 1 'matchtype "Function" function)
  (put-text-property
   0 1 'ctx
   "pub fn insert(&mut self, index: usize, element: T)"
   function)
  (put-text-property 0 1 'matchtype "Module" module)
  (put-text-property
   0 1 'ctx "/workspace/rust/library/collections.rs" module)
  (cl-letf
      (((symbol-function 'company-template-c-like-templatify)
        (lambda (arguments)
          (setq templatified arguments))))
    (provide 'company-template)
    (with-temp-buffer
      (insert "values.insert")
      (racer-complete--insert-args function)
      (setq inserted (buffer-string))
      (erase-buffer)
      (insert "values.insert(")
      (goto-char (1- (point-max)))
      (racer-complete--insert-args function)
      (setq already-called (buffer-string))
      (goto-char (point-max))
      (racer-complete--insert-args module)
      (setq non-function (buffer-string)))
    (with-temp-buffer
      (rust-mode)
      (insert "values.insert(index, make_value())")
      (search-backward "make_value")
      (cl-letf
          (((symbol-function 'racer-complete)
            (lambda (&optional _ignore)
              (setq point-during-completion (point))
              (list function))))
        (setq eldoc-result
              (substring-no-properties (racer-eldoc))))))
  (list
   :inserted inserted
   :template templatified
   :already-called already-called
   :non-function non-function
   :eldoc eldoc-result
   :completion-point point-during-completion))
"##;
    let expect = expect![[
        r##"OK (:inserted "values.insert(index: usize, element: T)" :template "(index: usize, element: T)" :already-called "values.insert(" :non-function "values.insert(" :eldoc "pub fn insert(&mut self, index: usize, element: T)" :completion-point 14)"##
    ]];
    ParityBatchCase::value(
        "completed_functions_insert_non_self_arguments_and_eldoc_resolves_nested_call_signature",
        elisp_form,
        expect,
    )
}

fn failed_racer_process_records_reproducible_diagnostics_with_stdout_stderr_and_environment()
-> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (make-temp-file "racer-failure-" t))
       (rust-src (expand-file-name "rust-src/" root))
       (cargo-home (expand-file-name "cargo-home/" root))
       (program (expand-file-name "broken-racer" root))
       signal-data
       debug-summary)
  (unwind-protect
      (progn
        (make-directory rust-src t)
        (make-directory cargo-home t)
        (with-temp-file program
          (insert
           "#!/bin/sh\nprintf 'partial completion\\n'\nprintf 'index unavailable\\n' >&2\nexit 7\n"))
        (set-file-modes program #o755)
        (let ((racer-cmd program)
              (racer-rust-src-path rust-src)
              (racer-cargo-home cargo-home)
              (default-directory root))
          (setq signal-data
                (racer-test-with-silent-process-sentinels
                  (condition-case error-data
                      (racer--call "complete" "4" "12" "source.rs")
                    (error
                     (list
                      (car error-data)
                      (racer-test-normalize-root
                       (error-message-string error-data) root))))))
          (racer-debug)
          (with-current-buffer "*racer-debug*"
            (setq debug-summary
                  (list
                   :mode major-mode
                   :read-only buffer-read-only
                   :text
                   (racer-test-normalize-root
                    (buffer-substring-no-properties
                     (point-min) (point-max))
                    root))))))
    (when (get-buffer "*racer-debug*")
      (kill-buffer "*racer-debug*"))
    (delete-directory root t))
  (list
   :signal signal-data
   :process
   (list
    :program
    (racer-test-normalize-root
     (plist-get racer--prev-state :program) root)
    :args (plist-get racer--prev-state :args)
    :exit-code (plist-get racer--prev-state :exit-code)
    :stdout (plist-get racer--prev-state :stdout)
    :stderr (plist-get racer--prev-state :stderr))
   :debug debug-summary))
"##;
    let expect = expect![[
        r##"OK (:signal (user-error "[PROJECT]/broken-racer exited with 7. ‘M-x racer-debug’ for more info") :process (:program "[PROJECT]/broken-racer" :args ("complete" "4" "12" "source.rs") :exit-code 7 :stdout "partial completion\n" :stderr "index unavailable\n") :debug (:mode fundamental-mode :read-only t :text "The last racer command was:\n\n$ cd [ORACLE-WORKSPACE]/\n$ export CARGO_HOME=[PROJECT]/cargo-home/\n$ export RUST_SRC_PATH=[PROJECT]/rust-src/\n$ [PROJECT]/broken-racer complete 4 12 source.rs\n\nThis command terminated with exit code 7.\n\nstdout:\n\npartial completion\n\nstderr:\n\nindex unavailable\n\nThe temporary file will have been deleted. You should be\nable to reproduce the same output from racer with the\nfollowing command:\n\n$ CARGO_HOME=[PROJECT]/cargo-home/ RUST_SRC_PATH=[PROJECT]/rust-src/ [PROJECT]/broken-racer complete 4 12\n\nPlease report bugs on GitHub."))"##
    ]];
    ParityBatchCase::value(
        "failed_racer_process_records_reproducible_diagnostics_with_stdout_stderr_and_environment",
        elisp_form,
        expect,
    )
}

#[test]
fn racer_package_batch() {
    let cases = vec![
        real_subprocess_completion_uses_cargo_root_environment_and_temporary_buffer_snapshot(),
        minor_mode_installs_rust_completion_contract_and_respects_comment_and_string_contexts(),
        racer_protocol_parser_handles_escaped_fields_overloads_and_malformed_records(),
        rust_documentation_renderer_preserves_structure_code_and_navigation_metadata(),
        describe_builds_read_only_help_with_source_and_web_buttons_for_real_symbol_records(),
        definition_navigation_opens_reported_file_and_moves_to_exact_utf8_source_location(),
        completed_functions_insert_non_self_arguments_and_eldoc_resolves_nested_call_signature(),
        failed_racer_process_records_reproducible_diagnostics_with_stdout_stderr_and_environment(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed racer parity test");
    assert_oracle_batch_cases(racer_oracle(), test_name, "racer_parity", &cases);
}
