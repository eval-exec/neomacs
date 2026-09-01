use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, HAML_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r###"
(require 'cl-lib)
(require 'haml-mode)

(defun neomacs-haml-test-token (text &optional occurrence offset)
  "Return stable fontification and syntax data for TEXT."
  (save-excursion
    (goto-char (point-min))
    (dotimes (_ (or occurrence 1)) (search-forward text))
    (let* ((start (+ (match-beginning 0) (or offset 0)))
           (state (syntax-ppss start)))
      (list text
            :line (line-number-at-pos start)
            :column (save-excursion (goto-char start) (current-column))
            :face (get-text-property start 'face)
            :font-lock-face (get-text-property start 'font-lock-face)
            :string (and (nth 3 state) t)
            :comment (and (nth 4 state) t)))))

(defun neomacs-haml-test-lines ()
  "Return every line's indentation and property-free text."
  (save-excursion
    (goto-char (point-min))
    (let (lines)
      (while (not (eobp))
        (push (list :line (line-number-at-pos)
                    :indent (current-indentation)
                    :text (buffer-substring-no-properties
                           (line-beginning-position) (line-end-position)))
              lines)
        (forward-line 1))
      (nreverse lines))))

(defun neomacs-haml-test-goto-line (line)
  "Move to LINE's indentation."
  (goto-char (point-min))
  (forward-line (1- line))
  (back-to-indentation))
"###;

fn package_contract_configures_haml_buffers_keys_syntax_and_defaults() -> ParityBatchCase {
    let elisp_form = r###"
(let ((descriptor (cadr (assq 'haml-mode package-alist))))
  (with-temp-buffer
    (haml-mode)
    (list
     :package
     (list :name (package-desc-name descriptor)
           :version (package-version-join (package-desc-version descriptor))
           :requirements (package-desc-reqs descriptor)
           :features (mapcar #'featurep '(haml-mode ruby-mode css-mode js)))
     :mode
     (list major-mode mode-name (derived-mode-p 'prog-mode)
           (eq (syntax-table) haml-mode-syntax-table)
           indent-line-function indent-region-function
           parse-sexp-lookup-properties comment-start
           font-lock-defaults font-lock-extend-region-functions
           jit-lock-contextually font-lock-multiline
           electric-indent-inhibit indent-tabs-mode)
     :syntax (mapcar (lambda (character) (char-syntax character))
                     '(?: ?' ?# ?. ?% ?- ?{ ?}))
     :keys
     (mapcar (lambda (key) (lookup-key haml-mode-map (kbd key)))
             '("DEL" "C-c C-f" "C-c C-b" "C-c C-u" "C-c C-d"
               "C-c C-k" "C-c C-r" "C-c C-l"))
     :commands
     (mapcar #'commandp
             '(haml-mode haml-comment-block haml-uncomment-block
               haml-forward-sexp haml-backward-sexp haml-up-list
               haml-down-list haml-electric-backspace
               haml-kill-line-and-indent haml-output-region
               haml-output-buffer haml-replace-region))
     :recognition
     (mapcar (lambda (filename)
               (assoc-default filename auto-mode-alist #'string-match))
             '("/srv/views/orders/show.haml" "/srv/views/show.html.haml"
               "/srv/views/show.erb"))
     :defaults
     (list haml-indent-offset haml-backspace-backdents-nesting
           haml-indent-function (length haml-block-openers)
           (length haml-empty-elements)))))
"###;
    let expected = expect![[
        r#"OK (:package (:name haml-mode :version "20250714.1441" :requirements ((emacs (24 1)) (cl-lib (0 5))) :features (t t t t)) :mode (haml-mode "Haml" prog-mode t haml-indent-line haml-indent-region t "-#" ((haml-font-lock-keywords) t t) (haml-extend-region-contextual) t t t nil) :syntax (46 34 46 46 119 95 40 41) :keys (haml-electric-backspace haml-forward-sexp haml-backward-sexp haml-up-list haml-down-list haml-kill-line-and-indent haml-output-region haml-output-buffer) :commands (t t t t t t t t t t t t) :recognition (haml-mode haml-mode nil) :defaults (2 t haml-indent-p 6 16))"#
    ]];
    ParityBatchCase::value(
        "package_contract_configures_haml_buffers_keys_syntax_and_defaults",
        elisp_form,
        expected,
    )
}

fn production_template_fontifies_haml_ruby_interpolation_filters_and_comments() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert "!!! 5\n"
          "%main#checkout.dashboard{data: {controller: \"release\"}, class: [\"state\", status]}\n"
          "  %h1= \"Deploy #{service_name}\"\n"
          "  - if canary?\n"
          "    %p.notice Canary: #{percent}%\n"
          "  :css\n"
          "    .dashboard { color: red; }\n"
          "  :javascript\n"
          "    const ready = true;\n"
          "  :unknown\n"
          "    operator-only payload\n"
          "  / operator-visible note\n"
          "    nested visible note\n"
          "  -# secret rollout\n"
          "    token=do-not-render\n")
  (haml-mode)
  (font-lock-ensure)
  (list
   :tokens
   (mapcar
    (lambda (request) (apply #'neomacs-haml-test-token request))
    '(("!!! 5") ("%main") ("#checkout") (".dashboard" 1)
      ("controller") ("release") ("class") ("state") ("status")
      ("%h1") ("Deploy") ("#{" 1) ("service_name")
      ("- if") ("canary?") ("%p") (".notice") ("#{" 2)
      ("percent") (":css") ("color") ("red")
      (":javascript") ("const") ("true")
      (":unknown") ("operator-only payload")
      ("operator-visible note") ("nested visible note")
      ("secret rollout") ("token=do-not-render")))
   :multiline-runs
   (mapcar
    (lambda (text)
      (save-excursion
        (goto-char (point-min))
        (search-forward text)
        (list text
              (get-text-property (match-beginning 0) 'face)
              (get-text-property (1- (match-end 0)) 'face))))
    '("operator-only payload" "nested visible note"
      "secret rollout" "token=do-not-render"))))
"###;
    let expected = expect![[
        r##"OK (:tokens (("!!! 5" :line 1 :column 0 :face font-lock-constant-face :font-lock-face nil :string nil :comment nil) ("%main" :line 2 :column 0 :face font-lock-keyword-face :font-lock-face nil :string nil :comment nil) ("#checkout" :line 2 :column 5 :face font-lock-function-name-face :font-lock-face nil :string nil :comment nil) (".dashboard" :line 2 :column 14 :face font-lock-variable-name-face :font-lock-face nil :string nil :comment nil) ("controller" :line 2 :column 32 :face font-lock-constant-face :font-lock-face nil :string nil :comment nil) ("release" :line 2 :column 45 :face font-lock-string-face :font-lock-face nil :string t :comment nil) ("class" :line 2 :column 56 :face font-lock-keyword-face :font-lock-face nil :string nil :comment nil) ("state" :line 2 :column 65 :face font-lock-string-face :font-lock-face nil :string t :comment nil) ("status" :line 2 :column 73 :face nil :font-lock-face nil :string nil :comment nil) ("%h1" :line 3 :column 2 :face font-lock-keyword-face :font-lock-face nil :string nil :comment nil) ("Deploy" :line 3 :column 8 :face font-lock-string-face :font-lock-face nil :string t :comment nil) ("#{" :line 3 :column 15 :face font-lock-string-face :font-lock-face nil :string t :comment nil) ("service_name" :line 3 :column 17 :face font-lock-string-face :font-lock-face nil :string t :comment nil) ("- if" :line 4 :column 2 :face font-lock-preprocessor-face :font-lock-face nil :string nil :comment nil) ("canary?" :line 4 :column 7 :face nil :font-lock-face nil :string nil :comment nil) ("%p" :line 5 :column 4 :face font-lock-keyword-face :font-lock-face nil :string nil :comment nil) (".notice" :line 5 :column 6 :face font-lock-variable-name-face :font-lock-face nil :string nil :comment nil) ("#{" :line 5 :column 22 :face (font-lock-variable-name-face) :font-lock-face nil :string nil :comment nil) ("percent" :line 5 :column 24 :face nil :font-lock-face nil :string nil :comment nil) (":css" :line 6 :column 2 :face font-lock-preprocessor-face :font-lock-face nil :string nil :comment nil) ("color" :line 7 :column 17 :face css-property :font-lock-face nil :string nil :comment nil) ("red" :line 7 :column 24 :face nil :font-lock-face nil :string nil :comment nil) (":javascript" :line 8 :column 2 :face font-lock-preprocessor-face :font-lock-face nil :string nil :comment nil) ("const" :line 9 :column 4 :face font-lock-keyword-face :font-lock-face nil :string nil :comment nil) ("true" :line 9 :column 18 :face font-lock-constant-face :font-lock-face nil :string nil :comment nil) (":unknown" :line 10 :column 2 :face font-lock-preprocessor-face :font-lock-face nil :string nil :comment nil) ("operator-only payload" :line 11 :column 4 :face font-lock-string-face :font-lock-face nil :string nil :comment nil) ("operator-visible note" :line 12 :column 4 :face font-lock-comment-face :font-lock-face nil :string nil :comment nil) ("nested visible note" :line 13 :column 4 :face font-lock-comment-face :font-lock-face nil :string nil :comment nil) ("secret rollout" :line 14 :column 5 :face font-lock-comment-face :font-lock-face nil :string nil :comment nil) ("token=do-not-render" :line 15 :column 4 :face font-lock-comment-face :font-lock-face nil :string nil :comment nil)) :multiline-runs (("operator-only payload" font-lock-string-face font-lock-string-face) ("nested visible note" font-lock-comment-face font-lock-comment-face) ("secret rollout" font-lock-comment-face font-lock-comment-face) ("token=do-not-render" font-lock-comment-face font-lock-comment-face)))"##
    ]];
    ParityBatchCase::value(
        "production_template_fontifies_haml_ruby_interpolation_filters_and_comments",
        elisp_form,
        expected,
    )
}

fn line_indentation_follows_real_nesting_and_cycles_available_parent_levels() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert "%main\n"
          "%h1 Dashboard\n"
          "%section\n"
          "%p= message\n"
          "- if canary\n"
          "= release_url\n")
  (let ((haml-indent-offset 2))
    (haml-mode)
    (dotimes (line 6)
      (neomacs-haml-test-goto-line (1+ line))
      (let ((this-command 'haml-indent-line)
            (last-command nil))
        (haml-indent-line)))
    (let ((initial (neomacs-haml-test-lines))
          cycles openers)
      (neomacs-haml-test-goto-line 6)
      (dotimes (_ 4)
        (let ((this-command 'haml-indent-line)
              (last-command 'haml-indent-line))
          (haml-indent-line))
        (push (current-indentation) cycles))
      (dolist (sample '("%main" "%h1 Dashboard" "%img{src: avatar_url}"
                        "- if canary" "/" ":markdown" "plain text"))
        (with-temp-buffer
          (insert sample)
          (haml-mode)
          (goto-char (point-min))
          (push (list sample (haml-indent-p)) openers)))
      (list :initial initial
            :cycles (nreverse cycles)
            :final (neomacs-haml-test-lines)
            :openers (nreverse openers)))))
"###;
    let expected = expect![[
        r#"OK (:initial ((:line 1 :indent 0 :text "%main") (:line 2 :indent 2 :text "  %h1 Dashboard") (:line 3 :indent 2 :text "  %section") (:line 4 :indent 4 :text "    %p= message") (:line 5 :indent 4 :text "    - if canary") (:line 6 :indent 6 :text "      = release_url")) :cycles (4 2 0 6) :final ((:line 1 :indent 0 :text "%main") (:line 2 :indent 2 :text "  %h1 Dashboard") (:line 3 :indent 2 :text "  %section") (:line 4 :indent 4 :text "    %p= message") (:line 5 :indent 4 :text "    - if canary") (:line 6 :indent 6 :text "      = release_url")) :openers (("%main" t) ("%h1 Dashboard" nil) ("%img{src: avatar_url}" nil) ("- if canary" t) ("/" t) (":markdown" t) ("plain text" nil)))"#
    ]];
    ParityBatchCase::value(
        "line_indentation_follows_real_nesting_and_cycles_available_parent_levels",
        elisp_form,
        expected,
    )
}

fn multiline_attribute_hashes_report_structure_and_drive_continuation_indentation()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert "%article(\n"
          "  data-controller=\"release\"\n"
          "  aria-live=\"polite\"\n"
          ")\n"
          "  Deploy\n"
          "%button{class: \"primary\",\n"
          "        data: {confirm: \"Ship?\"}}\n"
          "  Ship\n")
  (haml-mode)
  (let (records)
    (dotimes (index 8)
      (neomacs-haml-test-goto-line (1+ index))
      (push
       (list :line (1+ index)
             :text (buffer-substring-no-properties
                    (line-beginning-position) (line-end-position))
             :attributes (haml-parse-multiline-attr-hash)
             :computed (haml-compute-indentation)
             :indent-p
             (condition-case error-data
                 (haml-indent-p)
               (error (list (car error-data) (cadr error-data)))))
       records))
    (list :records (nreverse records)
          :new-style
          (progn
            (neomacs-haml-test-goto-line 2)
            (haml-unclosed-attr-hash-p))
          :old-style
          (progn
            (neomacs-haml-test-goto-line 7)
            (haml-unclosed-attr-hash-p)))))
"###;
    let expected = expect![[
        r#"OK (:records ((:line 1 :text "%article(" :attributes nil :computed (0 nil) :indent-p nil) (:line 2 :text "  data-controller=\"release\"" :attributes nil :computed (0 nil) :indent-p nil) (:line 3 :text "  aria-live=\"polite\"" :attributes nil :computed (2 nil) :indent-p nil) (:line 4 :text ")" :attributes nil :computed (2 nil) :indent-p nil) (:line 5 :text "  Deploy" :attributes nil :computed (0 nil) :indent-p nil) (:line 6 :text "%button{class: \"primary\"," :attributes ((indent . 0) (hash-indent . 8) (point . 71)) :computed (2 nil) :indent-p 8) (:line 7 :text "        data: {confirm: \"Ship?\"}}" :attributes ((indent . 0) (hash-indent . 8) (point . 71)) :computed (8 t) :indent-p 2) (:line 8 :text "  Ship" :attributes nil :computed (2 t) :indent-p nil)) :new-style t :old-style nil)"#
    ]];
    ParityBatchCase::value(
        "multiline_attribute_hashes_report_structure_and_drive_continuation_indentation",
        elisp_form,
        expected,
    )
}

fn structural_navigation_marks_and_moves_across_nested_template_blocks() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert "%main\n"
          "  %nav\n"
          "    %a Home\n"
          "\n"
          "  %section\n"
          "    %p Body\n"
          "%footer Footer\n")
  (haml-mode)
  (let (forward backward down up marked leaf-error)
    (neomacs-haml-test-goto-line 1)
    (haml-forward-sexp)
    (setq forward (list (line-number-at-pos) (current-column)))
    (haml-backward-sexp)
    (setq backward (list (line-number-at-pos) (current-column)))
    (neomacs-haml-test-goto-line 1)
    (haml-down-list 2)
    (setq down (list (line-number-at-pos) (current-column)))
    (neomacs-haml-test-goto-line 6)
    (haml-up-list 2)
    (setq up (list (line-number-at-pos) (current-column)))
    (neomacs-haml-test-goto-line 1)
    (haml-mark-sexp-but-not-next-line)
    (setq marked (buffer-substring-no-properties (point) (mark)))
    (deactivate-mark)
    (neomacs-haml-test-goto-line 3)
    (setq leaf-error
          (condition-case error-data
              (progn (haml-down-list) :moved)
            (error (list (car error-data) (cadr error-data)))))
    (list :forward forward :backward backward :down down :up up
          :marked marked :leaf-error leaf-error)))
"###;
    let expected = expect![[
        r#"OK (:forward (7 0) :backward (1 0) :down (3 4) :up (1 0) :marked "%main\n  %nav\n    %a Home\n\n  %section\n    %p Body" :leaf-error (error "Nothing is nested beneath this line"))"#
    ]];
    ParityBatchCase::value(
        "structural_navigation_marks_and_moves_across_nested_template_blocks",
        elisp_form,
        expected,
    )
}

fn block_comment_backspace_and_kill_commands_preserve_template_structure() -> ParityBatchCase {
    let elisp_form = r###"
(let ((source "%main\n  %section\n    %p One\n    %p Two\n%footer Footer\n"))
  (list
   :comment-roundtrip
   (with-temp-buffer
     (insert source)
     (haml-mode)
     (neomacs-haml-test-goto-line 2)
     (haml-comment-block)
     (let ((commented (buffer-substring-no-properties (point-min) (point-max))))
       (neomacs-haml-test-goto-line 2)
       (haml-uncomment-block)
       (list :commented commented
             :restored (buffer-substring-no-properties (point-min) (point-max)))))
   :backspace-tree
   (with-temp-buffer
     (insert source)
     (haml-mode)
     (neomacs-haml-test-goto-line 2)
     (let ((haml-backspace-backdents-nesting t))
       (haml-electric-backspace 1))
     (buffer-substring-no-properties (point-min) (point-max)))
   :backspace-line
   (with-temp-buffer
     (insert source)
     (haml-mode)
     (neomacs-haml-test-goto-line 2)
     (let ((haml-backspace-backdents-nesting nil))
       (haml-electric-backspace 1))
     (buffer-substring-no-properties (point-min) (point-max)))
   :kill-parent
   (with-temp-buffer
     (insert source)
     (haml-mode)
     (neomacs-haml-test-goto-line 2)
     (haml-kill-line-and-indent)
     (buffer-substring-no-properties (point-min) (point-max)))))
"###;
    let expected = expect![[
        r#"OK (:comment-roundtrip (:commented "%main\n  -#\n    %section\n      %p One\n      %p Two\n%footer Footer\n" :restored "%main\n  %section\n    %p One\n    %p Two\n%footer Footer\n") :backspace-tree "%main\n%section\n  %p One\n  %p Two\n%footer Footer\n" :backspace-line "%main\n%section\n    %p One\n    %p Two\n%footer Footer\n" :kill-parent "%main\n  %p One\n  %p Two\n%footer Footer\n")"#
    ]];
    ParityBatchCase::value(
        "block_comment_backspace_and_kill_commands_preserve_template_structure",
        elisp_form,
        expected,
    )
}

fn compiler_commands_normalize_selected_haml_and_route_output_consistently() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert "  %section\n    %p Release ready\n")
  (haml-mode)
  (let ((kill-ring nil)
        (interprogram-cut-function nil)
        calls)
    (cl-letf
        (((symbol-function 'shell-command-on-region)
          (lambda (start end command &optional output-buffer replace &rest _)
            (push (list :command command :output output-buffer :replace replace
                        :input (buffer-substring-no-properties start end))
                  calls)
            (when replace
              (delete-region start end)
              (goto-char start)
              (insert "<section><p>Release ready</p></section>"))
            0)))
      (haml-output-buffer)
      (haml-replace-region (point-min) (point-max)))
    (list :calls (nreverse calls)
          :buffer (buffer-substring-no-properties (point-min) (point-max))
          :kill (car kill-ring))))
"###;
    let expected = expect![[
        r#"OK (:calls ((:command "haml" :output "haml-output" :replace nil :input "%section\n  %p Release ready\n") (:command "haml" :output "haml-output" :replace t :input "%section\n  %p Release ready\n")) :buffer "<section><p>Release ready</p></section>" :kill "  %section\n    %p Release ready\n")"#
    ]];
    ParityBatchCase::value(
        "compiler_commands_normalize_selected_haml_and_route_output_consistently",
        elisp_form,
        expected,
    )
}

fn contextual_refontification_and_limited_sexp_scanning_tolerate_partial_edits() -> ParityBatchCase
{
    let elisp_form = r###"
(list
 :regions
 (with-temp-buffer
   (insert "%main\n"
           "  :css\n"
           "    .panel { color: red; }\n"
           "    .alert { color: orange; }\n"
           "  %p After filter\n"
           "  / review note\n"
           "    nested note\n"
           "%footer Footer\n")
   (haml-mode)
   (let (filter-bounds filter-extended comment-bounds comment-extended)
     (neomacs-haml-test-goto-line 4)
     (setq filter-bounds (haml-find-containing-block haml-filter-re))
     (unwind-protect
         (progn
           (set 'font-lock-beg (point))
           (set 'font-lock-end (1+ (point)))
           (haml-extend-region-filter)
           (setq filter-extended
                 (list (line-number-at-pos (symbol-value 'font-lock-beg))
                       (line-number-at-pos (symbol-value 'font-lock-end)))))
       (makunbound 'font-lock-beg)
       (makunbound 'font-lock-end))
     (neomacs-haml-test-goto-line 7)
     (setq comment-bounds (haml-find-containing-block haml-comment-re))
     (unwind-protect
         (progn
           (set 'font-lock-beg (point))
           (set 'font-lock-end (1+ (point)))
           (haml-extend-region-comment)
           (setq comment-extended
                 (list (line-number-at-pos (symbol-value 'font-lock-beg))
                       (line-number-at-pos (symbol-value 'font-lock-end)))))
       (makunbound 'font-lock-beg)
       (makunbound 'font-lock-end))
     (list :filter
           (list (list (line-number-at-pos (car filter-bounds))
                       (line-number-at-pos (cdr filter-bounds)))
                 filter-extended)
           :comment
           (list (list (line-number-at-pos (car comment-bounds))
                       (line-number-at-pos (cdr comment-bounds)))
                 comment-extended))))
 :sexps
 (mapcar
  (lambda (source)
    (with-temp-buffer
      (insert source)
      (goto-char (point-min))
      (haml-limited-forward-sexp (point-max))
      (list source :point (point) :remaining
            (buffer-substring-no-properties (point) (point-max)))))
  '("(deploy(service)) trailing" "(deploy(service" "[one, {two: 2}] tail"))
 :nested-match
 (with-temp-buffer
   (insert ":markdown\n  # Release\n  Ready\n%p sibling\n")
   (goto-char (point-min))
   (when (re-search-forward haml-filter-re nil t)
     (buffer-substring-no-properties (match-beginning 0) (match-end 0)))))
"###;
    let expected = expect![[
        r#"OK (:regions (:filter ((2 4) (2 4)) :comment ((6 7) (6 7))) :sexps (("(deploy(service)) trailing" :point 18 :remaining " trailing") ("(deploy(service" :point 16 :remaining "") ("[one, {two: 2}] tail" :point 16 :remaining " tail")) :nested-match ":markdown\n  # Release\n  Ready")"#
    ]];
    ParityBatchCase::value(
        "contextual_refontification_and_limited_sexp_scanning_tolerate_partial_edits",
        elisp_form,
        expected,
    )
}

#[test]
fn haml_mode_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(HAML_MODE_MELPA_PIN, "haml-mode.el")
            .expect("prepare revision-pinned Haml Mode below ./tmp")
            .with_timeout(Duration::from_secs(300))
            .with_prelude(PRELUDE),
        "haml-mode-package-batch",
        "Haml Mode",
        &[
            package_contract_configures_haml_buffers_keys_syntax_and_defaults(),
            production_template_fontifies_haml_ruby_interpolation_filters_and_comments(),
            line_indentation_follows_real_nesting_and_cycles_available_parent_levels(),
            multiline_attribute_hashes_report_structure_and_drive_continuation_indentation(),
            structural_navigation_marks_and_moves_across_nested_template_blocks(),
            block_comment_backspace_and_kill_commands_preserve_template_structure(),
            compiler_commands_normalize_selected_haml_and_route_output_consistently(),
            contextual_refontification_and_limited_sexp_scanning_tolerate_partial_edits(),
        ],
    );
}
