use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, GO_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'go-mode)

(defun neomacs-go-test-buffer (text mode body)
  "Run BODY in a temporary MODE buffer containing TEXT."
  (with-temp-buffer
    (insert text)
    (funcall mode)
    (goto-char (point-min))
    (funcall body)))

(defun neomacs-go-test-auto-mode (filename &optional text)
  "Return the mode selected for FILENAME containing TEXT."
  (with-temp-buffer
    (setq buffer-file-name filename)
    (when text (insert text))
    (set-auto-mode)
    major-mode))

(defun neomacs-go-test-face-runs ()
  "Return every nonempty font-lock face run in the current buffer."
  (font-lock-ensure)
  (let ((position (point-min)) runs)
    (while (< position (point-max))
      (let* ((face (get-text-property position 'face))
             (next (or (next-single-property-change position 'face nil
                                                      (point-max))
                       (point-max)))
             (text (buffer-substring-no-properties position next)))
        (when (and face (string-match-p "[^[:space:]]" text))
          (push (list face text) runs))
        (setq position next)))
    (nreverse runs)))

(defun neomacs-go-test-syntax-at (needle offset)
  "Return parser state OFFSET characters into NEEDLE."
  (save-excursion
    (goto-char (point-min))
    (search-forward needle)
    (let* ((position (+ (match-beginning 0) offset))
           (state (syntax-ppss position)))
      (list :position position
            :depth (car state)
            :string (and (nth 3 state) t)
            :comment (and (nth 4 state) t)
            :start (nth 8 state)))))

(defun neomacs-go-test-point-state ()
  "Describe point in a way useful for source-navigation assertions."
  (list :line (line-number-at-pos)
        :column (current-column)
        :symbol (thing-at-point 'symbol t)
        :line-text (buffer-substring-no-properties
                    (line-beginning-position) (line-end-position))))

(defun neomacs-go-test-path (name)
  "Return deterministic NAME below this oracle process's sandbox."
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun neomacs-go-test-write (name text &optional executable)
  "Write TEXT to deterministic sandbox NAME and return its path."
  (let ((path (neomacs-go-test-path name)))
    (make-directory (file-name-directory path) t)
    (with-temp-file path (insert text))
    (when executable (set-file-modes path #o755))
    path))

(defun neomacs-go-test-read-lines (path)
  "Return PATH's lines without trailing empty records."
  (with-temp-buffer
    (insert-file-contents path)
    (split-string (buffer-string) "\n" t)))

(defun neomacs-go-test-overlays ()
  "Describe coverage overlays in deterministic buffer order."
  (mapcar
   (lambda (overlay)
     (list :start (overlay-start overlay)
           :end (overlay-end overlay)
           :face (overlay-get overlay 'face)
           :help (overlay-get overlay 'help-echo)
           :text (buffer-substring-no-properties
                  (overlay-start overlay) (overlay-end overlay))))
   (sort (overlays-in (point-min) (point-max))
         (lambda (left right)
           (let ((left-start (overlay-start left))
                 (right-start (overlay-start right)))
             (if (= left-start right-start)
                 (< (overlay-end left) (overlay-end right))
               (< left-start right-start)))))))
"####;

fn source_files_select_complete_go_editor_modes_and_syntax() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (neomacs-go-test-path "mode-selection/"))
       (go-file (expand-file-name "service.go" root))
       (mod-file (expand-file-name "go.mod" root))
       (work-file (expand-file-name "go.work" root))
       (asm-file (expand-file-name "service_amd64.s" root))
       (descriptor (cadr (assq 'go-mode package-alist))))
  (make-directory root t)
  (neomacs-go-test-write "mode-selection/service.go" "package service\n")
  (list
   :package
   (list :name (package-desc-name descriptor)
         :version (package-version-join (package-desc-version descriptor))
         :requirements (package-desc-reqs descriptor)
         :feature (and (featurep 'go-mode) t))
   :auto-modes
   (list (neomacs-go-test-auto-mode go-file "package service\n")
         (neomacs-go-test-auto-mode mod-file "module example.com/service\n")
         (neomacs-go-test-auto-mode work-file "go 1.24\nuse ./service\n")
         (neomacs-go-test-auto-mode asm-file "TEXT ·start(SB),NOSPLIT,$0-0\n"))
   :editor
   (neomacs-go-test-buffer
    "package service\n\nvar raw = `path\\\\kept`\n// deploy safely\nvar retries = /* bounded */ 3\n"
    #'go-mode
    (lambda ()
      (list
       :mode major-mode
       :name mode-name
       :derived (and (derived-mode-p 'prog-mode) t)
       :indent indent-line-function
       :tabs indent-tabs-mode
       :comments (list comment-start comment-end comment-use-syntax
                       comment-region-function)
       :paragraph (list fill-paragraph-function
                        fill-forward-paragraph-function
                        adaptive-fill-function)
       :navigation (list beginning-of-defun-function end-of-defun-function)
       :keys
       (mapcar (lambda (key) (list key (lookup-key go-mode-map (kbd key))))
               '("C-c C-a" "C-c C-f a" "C-c C-f d" "C-c C-f i"
                 "C-c C-f m" "C-c C-f n" "C-c C-f r"))
       :hooks
       (list (and (memq #'go--reset-dangling-cache-before-change
                        before-change-functions)
                  t)
             (and (memq #'go--electric-indent-function
                        electric-indent-functions)
                  t))
       :syntax
       (list (neomacs-go-test-syntax-at "path\\\\kept" 5)
             (neomacs-go-test-syntax-at "deploy safely" 3)
             (neomacs-go-test-syntax-at "bounded" 3)))))))
"####;
    let expected = expect![[
        r#"OK (:package (:name go-mode :version "20260510.1707" :requirements ((emacs (26 1))) :feature t) :auto-modes (go-mode go-dot-mod-mode go-dot-work-mode go-asm-mode) :editor (:mode go-mode :name "Go" :derived t :indent go-mode-indent-line :tabs t :comments ("// " "" t go--comment-region) :paragraph (go-fill-paragraph go--fill-forward-paragraph go--find-fill-prefix) :navigation (go-beginning-of-defun go-end-of-defun) :keys (("C-c C-a" go-import-add) ("C-c C-f a" go-goto-arguments) ("C-c C-f d" go-goto-docstring) ("C-c C-f i" go-goto-imports) ("C-c C-f m" go-goto-method-receiver) ("C-c C-f n" go-goto-function-name) ("C-c C-f r" go-goto-return-values)) :hooks (t t) :syntax ((:position 34 :depth 0 :string t :comment nil :start 28) (:position 47 :depth 0 :string nil :comment t :start 41) (:position 78 :depth 0 :string nil :comment t :start 72))))"#
    ]];
    ParityBatchCase::value(
        "source_files_select_complete_go_editor_modes_and_syntax",
        elisp_form,
        expected,
    )
}

fn indenting_a_release_handler_formats_nested_types_literals_switches_and_calls() -> ParityBatchCase
{
    let elisp_form = r####"
(neomacs-go-test-buffer
 "package release

type Deployment struct {
Name string
Targets []Target
}

func build(
name string,
targets []Target,
) (*Deployment, error) {
plan := &Deployment{
Name: name,
Targets: []Target{
{Name: \"api\",
Ports: []int{
8080,
8443,
},
},
},
}
switch state := plan.State(); state {
case \"ready\",
\"staged\":
if err := validate(
plan,
); err != nil {
return nil,
fmt.Errorf(\"validate: %w\",
err)
}
default:
return nil, errors.New(
\"blocked\",
)
}
return plan, nil
}
"
 #'go-mode
 (lambda ()
   (let (first point-after-first)
     (goto-char (point-min))
     (search-forward "fmt.Errorf")
     (indent-region (point-min) (point-max))
     (setq first (buffer-string)
           point-after-first (point))
     (indent-region (point-min) (point-max))
     (list :formatted (buffer-string)
           :idempotent (equal first (buffer-string))
           :point (list point-after-first (point))
           :dangling-cache-size (hash-table-count go-dangling-cache)))))
"####;
    let expected = expect![[
        r#"OK (:formatted "package release\n\ntype Deployment struct {\n\11Name string\n\11Targets []Target\n}\n\nfunc build(\n\11name string,\n\11targets []Target,\n) (*Deployment, error) {\n\11plan := &Deployment{\n\11\11Name: name,\n\11\11Targets: []Target{\n\11\11\11{Name: \"api\",\n\11\11\11\11Ports: []int{\n\11\11\11\11\118080,\n\11\11\11\11\118443,\n\11\11\11\11},\n\11\11\11},\n\11\11},\n\11}\n\11switch state := plan.State(); state {\n\11case \"ready\",\n\11\11\"staged\":\n\11\11if err := validate(\n\11\11\11plan,\n\11\11); err != nil {\n\11\11\11return nil,\n\11\11\11\11fmt.Errorf(\"validate: %w\",\n\11\11\11\11\11err)\n\11\11}\n\11default:\n\11\11return nil, errors.New(\n\11\11\11\"blocked\",\n\11\11)\n\11}\n\11return plan, nil\n}\n" :idempotent t :point (426 426) :dangling-cache-size 37)"#
    ]];
    ParityBatchCase::value(
        "indenting_a_release_handler_formats_nested_types_literals_switches_and_calls",
        elisp_form,
        expected,
    )
}

fn fontification_distinguishes_declarations_calls_types_labels_strings_and_comments()
-> ParityBatchCase {
    let elisp_form = r####"
(neomacs-go-test-buffer
 "package release

import \"fmt\"

// Deployment describes one rollout.
type Deployment struct {
	Name string
	Ports []int
}

const DefaultRetries int = 3
var active *Deployment

func (deployment *Deployment) Route(target string, ports []int) (map[string]int, error) {
	counts := map[string]int{\"api\": len(ports)}
	if !ready(target) {
		return nil, fmt.Errorf(\"blocked: %s\", target)
	}
dispatch:
	switch value := any(target).(type) {
	case string, fmt.Stringer:
		counts[value]++
	default:
		goto dispatch
	}
	return counts, nil
}
"
 #'go-mode
 (lambda ()
   (list :runs (neomacs-go-test-face-runs)
         :comment-syntax (neomacs-go-test-syntax-at
                          "Deployment describes" 4)
         :string-syntax (neomacs-go-test-syntax-at "blocked: %s" 3))))
"####;
    let expected = expect![[
        r#"OK (:runs ((font-lock-keyword-face "package") (font-lock-keyword-face "import") (font-lock-string-face "\"fmt\"") (font-lock-comment-delimiter-face "// ") (font-lock-comment-face "Deployment describes one rollout.\n") (font-lock-keyword-face "type") (font-lock-type-face "Deployment") (font-lock-keyword-face "struct") (font-lock-type-face "string") (font-lock-type-face "int") (font-lock-keyword-face "const") (font-lock-constant-face "DefaultRetries") (font-lock-type-face "int") (font-lock-keyword-face "var") (font-lock-variable-name-face "active") (font-lock-type-face "Deployment") (font-lock-keyword-face "func") (font-lock-variable-name-face "deployment") (font-lock-type-face "Deployment") (font-lock-function-name-face "Route") (font-lock-variable-name-face "target") (font-lock-type-face "string") (font-lock-variable-name-face "ports") (font-lock-type-face "int") (font-lock-keyword-face "map") (font-lock-type-face "string") (font-lock-type-face "int") (font-lock-type-face "error") (font-lock-variable-name-face "counts") (font-lock-keyword-face "map") (font-lock-type-face "string") (font-lock-type-face "int") (font-lock-string-face "\"api\"") (font-lock-builtin-face "len") (font-lock-keyword-face "if") (font-lock-negation-char-face "!") (font-lock-function-name-face "ready") (font-lock-keyword-face "return") (font-lock-constant-face "nil") (font-lock-function-name-face "Errorf") (font-lock-string-face "\"blocked: %s\"") (font-lock-constant-face "dispatch") (font-lock-keyword-face "switch") (font-lock-variable-name-face "value") (font-lock-function-name-face "any") (font-lock-keyword-face "type") (font-lock-keyword-face "case") (font-lock-type-face "string") (font-lock-type-face "fmt.Stringer") (font-lock-keyword-face "default") (font-lock-keyword-face "goto") (font-lock-constant-face "dispatch") (font-lock-keyword-face "return") (font-lock-constant-face "nil")) :comment-syntax (:position 39 :depth 0 :string nil :comment t :start 32) :string-syntax (:position 361 :depth 3 :string t :comment nil :start 357))"#
    ]];
    ParityBatchCase::value(
        "fontification_distinguishes_declarations_calls_types_labels_strings_and_comments",
        elisp_form,
        expected,
    )
}

fn commenting_and_filling_preserve_go_line_and_block_comment_structure() -> ParityBatchCase {
    let elisp_form = r####"
(list
 :partial
 (neomacs-go-test-buffer
  "var retries int\n"
  #'go-mode
  (lambda ()
    (search-forward "retries")
    (let ((beginning (match-beginning 0))
          (end (match-end 0)))
      (comment-region beginning end)
      (let ((commented (buffer-string)))
        (goto-char (point-min))
        (search-forward "/* retries */")
        (uncomment-region (match-beginning 0) (match-end 0))
        (list :commented commented :restored (buffer-string))))))
 :lines
 (neomacs-go-test-buffer
  "func deploy() {\n\tvalidate()\n\tpublish()\n}\n"
  #'go-mode
  (lambda ()
    (goto-char (point-min))
    (forward-line 1)
    (let ((beginning (point))
          (end (line-beginning-position 3)))
      (comment-region beginning end)
      (let ((commented (buffer-string)))
        (uncomment-region beginning (point-max))
        (list :commented commented :restored (buffer-string))))))
 :filled
 (neomacs-go-test-buffer
  "func deploy() {\n\t// Validate every deployment target before publishing the release candidate to production.\n\t/* Preserve this detailed operational note while wrapping every line consistently for reviewers. */\n}\n"
  #'go-mode
  (lambda ()
    (let ((fill-column 58))
      (goto-char (point-min))
      (search-forward "Validate")
      (fill-paragraph)
      (search-forward "Preserve")
      (fill-paragraph)
      (list :text (buffer-string)
            :point (neomacs-go-test-point-state)
            :prefix (go--find-fill-prefix))))))
"####;
    let expected = expect![[
        r#"OK (:partial (:commented "var /* retries */ int\n" :restored "var retries int\n") :lines (:commented "func deploy() {\n\11// validate()\n\11// publish()\n}\n" :restored "func deploy() {\n\11validate()\n\11publish()\n}\n") :filled (:text "func deploy() {\n\11// Validate every deployment target before\n\11// publishing the release candidate to production.\n\11/* Preserve this detailed operational note while\n\11   wrapping every line consistently for\n\11   reviewers. */\n}\n" :point (:line 4 :column 19 :symbol "Preserve" :line-text "\11/* Preserve this detailed operational note while") :prefix "\11   "))"#
    ]];
    ParityBatchCase::value(
        "commenting_and_filling_preserve_go_line_and_block_comment_structure",
        elisp_form,
        expected,
    )
}

fn navigating_and_authoring_function_signatures_handles_methods_and_anonymous_callbacks()
-> ParityBatchCase {
    let elisp_form = r####"
(let ((source
       "package release

// Deploy publishes a release.
// It normalizes every requested target.
func (server *Server) Deploy(ctx context.Context, targets []string) (Report, error) {
	normalized := mapTargets(targets, func(target string) string {
		return strings.TrimSpace(target)
	})
	return Report{Targets: normalized}, nil
}

func Health() {
	return
}
"))
  (list
   :navigation
   (neomacs-go-test-buffer
    source #'go-mode
    (lambda ()
      (search-forward "TrimSpace")
      (list
       :anonymous-function
       (save-excursion (go-goto-function) (neomacs-go-test-point-state))
       :containing-method
       (save-excursion (go-goto-function t) (neomacs-go-test-point-state))
       :anonymous-name
       (save-excursion (go-goto-function-name) (neomacs-go-test-point-state))
       :method-name
       (save-excursion (go-goto-function-name t) (neomacs-go-test-point-state))
       :arguments
       (save-excursion (go-goto-arguments t) (neomacs-go-test-point-state))
       :returns
       (save-excursion (go-goto-return-values t) (neomacs-go-test-point-state))
       :receiver
       (save-excursion (go-goto-method-receiver t) (neomacs-go-test-point-state))
       :docstring
       (save-excursion (go-goto-docstring t) (neomacs-go-test-point-state))
       :defun
       (save-excursion
         (beginning-of-defun)
         (let ((beginning (point)))
           (end-of-defun)
           (buffer-substring-no-properties beginning (point)))))))
   :author-docstring
   (neomacs-go-test-buffer
    "package release\n\nfunc Health() {\n\treturn\n}\n" #'go-mode
    (lambda ()
      (search-forward "return")
      (go-goto-docstring)
      (list :text (buffer-string) :point (neomacs-go-test-point-state))))
   :author-receiver
   (neomacs-go-test-buffer
    "package release\n\nfunc Health() {\n\treturn\n}\n" #'go-mode
    (lambda ()
      (search-forward "return")
      (go-goto-method-receiver)
      (list :text (buffer-string) :point (neomacs-go-test-point-state))))
   :author-return
   (neomacs-go-test-buffer
    "package release\n\nfunc Health() {\n\treturn\n}\n" #'go-mode
    (lambda ()
      (search-forward "return")
      (go-goto-return-values)
      (list :text (buffer-string) :point (neomacs-go-test-point-state))))))
"####;
    let expected = expect![[
        r#"OK (:navigation (:anonymous-function (:line 6 :column 42 :symbol "func" :line-text "\11normalized := mapTargets(targets, func(target string) string {") :containing-method (:line 5 :column 0 :symbol "func" :line-text "func (server *Server) Deploy(ctx context.Context, targets []string) (Report, error) {") :anonymous-name (:line 6 :column 42 :symbol "func" :line-text "\11normalized := mapTargets(targets, func(target string) string {") :method-name (:line 5 :column 22 :symbol "Deploy" :line-text "func (server *Server) Deploy(ctx context.Context, targets []string) (Report, error) {") :arguments (:line 5 :column 29 :symbol "ctx" :line-text "func (server *Server) Deploy(ctx context.Context, targets []string) (Report, error) {") :returns (:line 5 :column 69 :symbol "Report" :line-text "func (server *Server) Deploy(ctx context.Context, targets []string) (Report, error) {") :receiver (:line 5 :column 6 :symbol "server" :line-text "func (server *Server) Deploy(ctx context.Context, targets []string) (Report, error) {") :docstring (:line 3 :column 3 :symbol "Deploy" :line-text "// Deploy publishes a release.") :defun "func (server *Server) Deploy(ctx context.Context, targets []string) (Report, error) {\n\11normalized := mapTargets(targets, func(target string) string {\n\11\11return strings.TrimSpace(target)\n\11})\n\11return Report{Targets: normalized}, nil\n}\n") :author-docstring (:text "package release\n\n// Health\nfunc Health() {\n\11return\n}\n" :point (:line 3 :column 9 :symbol "Health" :line-text "// Health")) :author-receiver (:text "package release\n\nfunc () Health() {\n\11return\n}\n" :point (:line 3 :column 6 :symbol nil :line-text "func () Health() {")) :author-return (:text "package release\n\nfunc Health()  {\n\11return\n}\n" :point (:line 3 :column 14 :symbol nil :line-text "func Health()  {")))"#
    ]];
    ParityBatchCase::value(
        "navigating_and_authoring_function_signatures_handles_methods_and_anonymous_callbacks",
        elisp_form,
        expected,
    )
}

fn import_management_creates_extends_aliases_and_revives_real_import_blocks() -> ParityBatchCase {
    let elisp_form = r####"
(cl-labels
    ((edit (text import &optional alias)
       (neomacs-go-test-buffer
        text #'go-mode
        (lambda ()
          (let ((messages nil))
            (cl-letf (((symbol-function 'read-from-minibuffer)
                       (lambda (&rest _ignored) alias))
                      ((symbol-function 'message)
                       (lambda (format-string &rest arguments)
                         (push (apply #'format-message format-string arguments)
                               messages))))
              (go-import-add (and alias t) import))
            (let ((text-after (buffer-string))
                  (point-after (point)))
              (goto-char (point-min))
              (list :text text-after
                    :point point-after
                    :import-location (go-goto-imports)
                    :import-point (neomacs-go-test-point-state)
                    :messages (nreverse messages))))))))
  (list
   :new-block
   (edit "package release\n\nfunc main() {}\n" "context")
   :existing-block
   (edit "package release\n\nimport (\n\t\"fmt\"\n)\n\nfunc main() {}\n"
         "context")
   :single-imports
   (edit "package release\n\nimport \"fmt\"\n\nfunc main() {}\n"
         "context")
   :aliased
   (edit "package release\n\nimport \"fmt\"\n\nfunc main() {}\n"
         "encoding/json" "jsonapi")
   :commented
   (edit "package release\n\n// import \"context\"\n\nfunc main() {}\n"
         "context")))
"####;
    let expected = expect![[
        r#"OK (:new-block (:text "package release\n\nimport (\n\11\"context\"\n)\n\nfunc main() {}\n" :point 1 :import-location block :import-point (:line 4 :column 17 :symbol nil :line-text "\11\"context\"") :messages ("No imports found, moving point after package declaration")) :existing-block (:text "package release\n\nimport (\n\11\"fmt\"\n\11\"context\"\n)\n\nfunc main() {}\n" :point 1 :import-location block :import-point (:line 5 :column 17 :symbol nil :line-text "\11\"context\"") :messages nil) :single-imports (:text "package release\n\nimport \"fmt\"\nimport \"context\"\n\nfunc main() {}\n" :point 1 :import-location single :import-point (:line 5 :column 0 :symbol nil :line-text "") :messages nil) :aliased (:text "package release\n\nimport \"fmt\"\nimport jsonapi \"encoding/json\"\n\nfunc main() {}\n" :point 1 :import-location single :import-point (:line 5 :column 0 :symbol nil :line-text "") :messages nil) :commented (:text "package release\n\nimport \"context\"\n\nfunc main() {}\n" :point 1 :import-location single :import-point (:line 4 :column 0 :symbol nil :line-text "") :messages nil))"#
    ]];
    ParityBatchCase::value(
        "import_management_creates_extends_aliases_and_revives_real_import_blocks",
        elisp_form,
        expected,
    )
}

fn formatter_integration_applies_tool_output_and_turns_failures_into_compilation_diagnostics()
-> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (neomacs-go-test-path "formatter/"))
       (arguments-file (expand-file-name "arguments" root))
       (success-script
        (neomacs-go-test-write
         "formatter/goimports"
         "#!/bin/sh
: > \"$NEOMACS_TEST_SANDBOX_ROOT/formatter/arguments\"
for argument in \"$@\"; do
  printf '%s\\n' \"$argument\" >> \"$NEOMACS_TEST_SANDBOX_ROOT/formatter/arguments\"
  target=$argument
done
printf 'package main\\n\\nfunc main() {\\n\\tprintln(\"ready\")\\n}\\n' > \"$target\"
"
         t))
       (failure-script
        (neomacs-go-test-write
         "formatter/gofmt-fail"
         "#!/bin/sh
for argument in \"$@\"; do target=$argument; done
printf '%s:2:3: expected semicolon\\n' \"$target\" >&2
exit 2
"
         t))
       (source-file (expand-file-name "service.go" root)))
  (list
   :success
   (neomacs-go-test-buffer
    "package main\nfunc main(){println(\"ready\")}\n" #'go-mode
    (lambda ()
      (setq buffer-file-name source-file)
      (goto-char (point-min))
      (search-forward "println")
      (let ((gofmt-command success-script)
            (gofmt-args '("-s"))
            (gofmt-show-errors 'buffer))
        (gofmt)
        (let* ((arguments (neomacs-go-test-read-lines arguments-file))
               (temporary (car (last arguments))))
          (list :text (buffer-string)
                :point (neomacs-go-test-point-state)
                :modified (buffer-modified-p)
                :arguments
                (list :srcdir-flag (equal (nth 0 arguments) "-srcdir")
                      :srcdir-file (equal (nth 1 arguments)
                                          (file-truename source-file))
                      :simplify (equal (nth 2 arguments) "-s")
                      :write-flag (equal (nth 3 arguments) "-w")
                      :temporary
                      (and temporary
                           (string-match-p "gofmt[[:alnum:]]+\\.go\\'"
                                           (file-name-nondirectory temporary))
                           t))
                :patch-buffer (get-buffer "*Gofmt patch*")
                :error-buffer (get-buffer "*Gofmt Errors*"))))))
   :failure
   (neomacs-go-test-buffer
    "package main\nfunc main( {\n}\n" #'go-mode
    (lambda ()
      (setq buffer-file-name source-file)
      (when (get-buffer "*Gofmt Errors*")
        (kill-buffer "*Gofmt Errors*"))
      (let ((gofmt-command failure-script)
            (gofmt-args nil)
            (gofmt-show-errors 'buffer))
        (save-window-excursion (gofmt))
        (prog1
            (list :text (buffer-string)
                  :mode (with-current-buffer "*Gofmt Errors*" major-mode)
                  :diagnostic
                  (with-current-buffer "*Gofmt Errors*"
                    (buffer-substring-no-properties (point-min) (point-max)))
                  :patch-buffer (get-buffer "*Gofmt patch*"))
          (kill-buffer "*Gofmt Errors*")))))))
"####;
    let expected = expect![[
        r#"OK (:success (:text "package main\n\nfunc main() {\n\11println(\"ready\")\n}\n" :point (:line 2 :column 0 :symbol nil :line-text "") :modified t :arguments (:srcdir-flag t :srcdir-file t :simplify t :write-flag t :temporary t) :patch-buffer nil :error-buffer nil) :failure (:text "package main\nfunc main( {\n}\n" :mode compilation-mode :diagnostic "gofmt errors:\nservice.go:2:3: expected semicolon\n" :patch-buffer nil))"#
    ]];
    ParityBatchCase::value(
        "formatter_integration_applies_tool_output_and_turns_failures_into_compilation_diagnostics",
        elisp_form,
        expected,
    )
}

fn module_and_workspace_files_indent_and_fontify_real_dependency_configuration() -> ParityBatchCase
{
    let elisp_form = r####"
(list
 :module
 (neomacs-go-test-buffer
  "module example.com/release

go 1.24
toolchain go1.24.1

require (
example.com/api v1.2.3
example.com/worker v0.0.0-20260701120000-abcdef123456 // indirect
)

replace example.com/api v1.2.3 => ../api v1.3.0
"
  #'go-dot-mod-mode
  (lambda ()
    (indent-region (point-min) (point-max))
    (list :text (buffer-string)
          :runs (neomacs-go-test-face-runs)
          :settings (list major-mode indent-tabs-mode comment-start
                          indent-line-function)
          :comment (neomacs-go-test-syntax-at "indirect" 3))))
 :workspace
 (neomacs-go-test-buffer
  "go 1.24
toolchain go1.24.1

use (
./api
./worker
)

replace example.com/shared => ../shared
"
  #'go-dot-work-mode
  (lambda ()
    (indent-region (point-min) (point-max))
    (list :text (buffer-string)
          :runs (neomacs-go-test-face-runs)
          :settings (list major-mode indent-tabs-mode comment-start
                          indent-line-function)))))
"####;
    let expected = expect![[
        r#"OK (:module (:text "module example.com/release\n\ngo 1.24\ntoolchain go1.24.1\n\nrequire (\n\11example.com/api v1.2.3\n\11example.com/worker v0.0.0-20260701120000-abcdef123456 // indirect\n)\n\nreplace example.com/api v1.2.3 => ../api v1.3.0\n" :runs ((font-lock-keyword-face "module") (font-lock-keyword-face "go") (font-lock-keyword-face "toolchain") (font-lock-keyword-face "require") (go-dot-mod-module-name "example.com/api") (go-dot-mod-module-semver "v1.2.3") (go-dot-mod-module-name "example.com/worker") (go-dot-mod-module-semver "v0.0.0") (go-dot-mod-module-version "-20260701120000-abcdef123456") (font-lock-comment-delimiter-face "// ") (font-lock-comment-face "indirect\n") (font-lock-keyword-face "replace") (go-dot-mod-module-name "../api") (go-dot-mod-module-semver "v1.3.0")) :settings (go-dot-mod-mode t "// " go-mode-indent-line) :comment (:position 152 :depth 1 :string nil :comment t :start 146)) :workspace (:text "go 1.24\ntoolchain go1.24.1\n\nuse (\n\11./api\n\11./worker\n)\n\nreplace example.com/shared => ../shared\n" :runs ((font-lock-keyword-face "go") (font-lock-keyword-face "toolchain") (font-lock-keyword-face "go") (font-lock-keyword-face "use") (font-lock-keyword-face "replace")) :settings (go-dot-work-mode t "// " go-mode-indent-line)))"#
    ]];
    ParityBatchCase::value(
        "module_and_workspace_files_indent_and_fontify_real_dependency_configuration",
        elisp_form,
        expected,
    )
}

fn coverage_profile_creates_a_live_indirect_source_view_with_count_overlays() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((source-text
        "package release

func Deploy() {
	validate()
	publish()
}

func Health() bool {
	return true
}
")
       (source-file
        (neomacs-go-test-write "coverage/service.go" source-text))
       (coverage-file
        (neomacs-go-test-write
         "coverage/coverage.out"
         "mode: count
example.com/release/service.go:3.1,6.2 3 0
example.com/release/service.go:8.1,10.2 2 5
example.com/release/other.go:1.1,2.2 1 99
"))
       (source-buffer (find-file-noselect source-file))
       result)
  (unwind-protect
      (with-current-buffer source-buffer
        (go-mode)
        (save-window-excursion (go-coverage coverage-file))
        (let ((coverage-buffer (get-buffer (concat (buffer-name) "<gocov>"))))
          (setq result
                (with-current-buffer coverage-buffer
                  (list :name (buffer-name)
                        :base (eq (buffer-base-buffer) source-buffer)
                        :mode major-mode
                        :source (buffer-string)
                        :profile go--coverage-current-file-name
                        :overlays (neomacs-go-test-overlays)
                        :parsed
                        (let ((parsed (go--coverage-parse-file
                                       coverage-file "service.go")))
                          (list
                           :ranges
                           (mapcar
                            (lambda (range)
                              (list (go--covered-start-line range)
                                    (go--covered-start-column range)
                                    (go--covered-end-line range)
                                    (go--covered-end-column range)
                                    (go--covered-covered range)
                                    (go--covered-count range)))
                            (car parsed))
                           :divisor (cadr parsed))))))
          (kill-buffer coverage-buffer)))
    (when (buffer-live-p source-buffer) (kill-buffer source-buffer)))
  result)
"####;
    let expected = expect![[
        r#"OK (:name "service.go<gocov>" :base t :mode go-mode :source "package release\n\nfunc Deploy() {\n\11validate()\n\11publish()\n}\n\nfunc Health() bool {\n\11return true\n}\n" :profile "[ORACLE-SANDBOX]/coverage/coverage.out" :overlays ((:start 1 :end 96 :face go-coverage-untracked :help nil :text "package release\n\nfunc Deploy() {\n\11validate()\n\11publish()\n}\n\nfunc Health() bool {\n\11return true\n}\n") (:start 18 :end 58 :face "go-coverage-0" :help "Count: 0" :text "func Deploy() {\n\11validate()\n\11publish()\n}") (:start 60 :end 95 :face "go-coverage-10" :help "Count: 5" :text "func Health() bool {\n\11return true\n}")) :parsed (:ranges ((8 1 10 2 t 5) (3 1 6 2 nil 0)) :divisor 1.6094379124341003))"#
    ]];
    ParityBatchCase::value(
        "coverage_profile_creates_a_live_indirect_source_view_with_count_overlays",
        elisp_form,
        expected,
    )
}

#[test]
fn go_mode_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(GO_MODE_MELPA_PIN, "go-mode.el")
            .expect("prepare revision-pinned Go Mode source below ./tmp")
            .with_timeout(Duration::from_secs(240))
            .with_prelude(PRELUDE),
        "go-mode-package-batch",
        "Go Mode",
        &[
            source_files_select_complete_go_editor_modes_and_syntax(),
            indenting_a_release_handler_formats_nested_types_literals_switches_and_calls(),
            fontification_distinguishes_declarations_calls_types_labels_strings_and_comments(),
            commenting_and_filling_preserve_go_line_and_block_comment_structure(),
            navigating_and_authoring_function_signatures_handles_methods_and_anonymous_callbacks(),
            import_management_creates_extends_aliases_and_revives_real_import_blocks(),
            formatter_integration_applies_tool_output_and_turns_failures_into_compilation_diagnostics(),
            module_and_workspace_files_indent_and_fontify_real_dependency_configuration(),
            coverage_profile_creates_a_live_indirect_source_view_with_count_overlays(),
        ],
    );
}
