use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, YAML_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const YAML_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const YAML_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'imenu)
(require 'yaml-mode)

(defun neomacs-yaml-mode-test-in-buffer (text body)
  "Run BODY in a displayed YAML buffer containing TEXT."
  (let ((buffer (generate-new-buffer "*yaml-mode-parity*")))
    (unwind-protect
        (save-window-excursion
          (set-window-buffer (selected-window) buffer)
          (set-buffer buffer)
          (insert text)
          (yaml-mode)
          (font-lock-ensure)
          (goto-char (point-min))
          (funcall body))
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))))

(defun neomacs-yaml-mode-test-face-spans ()
  "Return every non-nil font-lock face span and its source text."
  (let ((position (point-min)) spans)
    (while (< position (point-max))
      (let* ((face (get-text-property position 'face))
             (next (next-single-property-change
                    position 'face nil (point-max))))
        (when face
          (push (list position next
                      (buffer-substring-no-properties position next)
                      face)
                spans))
        (setq position next)))
    (nreverse spans)))

(defun neomacs-yaml-mode-test-text ()
  "Return the whole buffer as user-visible text, without font-lock metadata."
  (buffer-substring-no-properties (point-min) (point-max)))

(defun neomacs-yaml-mode-test-syntax-at (needle offset)
  "Describe syntax at OFFSET characters from NEEDLE's beginning."
  (save-excursion
    (goto-char (point-min))
    (search-forward needle)
    (let* ((position (+ (match-beginning 0) offset))
           (state (syntax-ppss position)))
      (list needle offset position
            (char-after position)
            (syntax-class (syntax-after position))
            (nth 3 state) (nth 4 state) (nth 8 state)
            (get-text-property position 'syntax-table)
            (get-text-property position 'yaml-block-literal)))))

(defun neomacs-yaml-mode-test-auto-mode (filename &optional contents)
  "Return the mode selected for FILENAME containing CONTENTS."
  (with-temp-buffer
    (setq buffer-file-name filename)
    (when contents (insert contents))
    (set-auto-mode)
    major-mode))
"##;

fn yaml_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(YAML_MODE_MELPA_PIN, "yaml-mode.el")
        .expect("prepare revision-pinned YAML Mode source below ./tmp")
        .with_prelude(YAML_MODE_TEST_PRELUDE)
        .with_timeout(YAML_MODE_TEST_TIMEOUT)
}

fn project_files_activate_yaml_editor_configuration_comments_and_imenu() -> ParityBatchCase {
    let elisp_form = r##"
(list
 :auto-modes
 (mapcar
  (lambda (extension)
    (list extension
          (neomacs-yaml-mode-test-auto-mode
           (concat temporary-file-directory "deployment" extension))))
  '(".yml" ".yaml" ".eyml" ".eyaml" ".raml"))
 :magic
 (neomacs-yaml-mode-test-auto-mode
  (concat temporary-file-directory "manifest") "%YAML 1.2\n---\n")
 :buffer
 (neomacs-yaml-mode-test-in-buffer
  "service: api\nenvironment: production\n"
  (lambda ()
    (let ((imenu-use-markers nil)
          commented uncommented)
      (comment-region (line-beginning-position) (line-end-position))
      (setq commented (neomacs-yaml-mode-test-text))
      (uncomment-region (line-beginning-position) (line-end-position))
      (setq uncommented (neomacs-yaml-mode-test-text))
      (list
       :mode major-mode
       :name mode-name
       :derived (derived-mode-p 'text-mode)
       :locals
       (list comment-start comment-start-skip comment-end
             indent-line-function indent-tabs-mode
             fill-paragraph-function page-delimiter
             syntax-propertize-function)
       :keys
       (mapcar (lambda (key) (lookup-key yaml-mode-map (kbd key)))
               '("|" ">" "-" "." "DEL"))
       :syntax
       (mapcar (lambda (character)
                 (list character
                       (syntax-class
                        (aref yaml-mode-syntax-table character))))
               '(?\' ?\" ?# ?- ?_ ?& ?* ?{ ?} ?[ ?]))
       :imenu
       (mapcar (lambda (entry)
                 (list (car entry)
                       (line-number-at-pos (cdr entry))))
               (imenu-default-create-index-function))
       :commented commented
       :uncommented uncommented)))))
"##;
    let expected = expect![[
        r####"OK (:auto-modes ((".yml" yaml-mode) (".yaml" yaml-mode) (".eyml" yaml-mode) (".eyaml" yaml-mode) (".raml" yaml-mode)) :magic yaml-mode :buffer (:mode yaml-mode :name "YAML" :derived text-mode :locals ("# " "#+ *" "" yaml-indent-line nil yaml-fill-paragraph "^---\\([ \11].*\\)*\n" yaml-mode-syntax-propertize-function) :keys (yaml-electric-bar-and-angle yaml-electric-bar-and-angle yaml-electric-dash-and-dot yaml-electric-dash-and-dot yaml-electric-backspace) :syntax ((39 7) (34 7) (35 11) (45 3) (95 3) (38 1) (42 1) (123 4) (125 5) (91 4) (93 5)) :imenu (("service" 1) ("environment" 2)) :commented "# service: api\nenvironment: production\n" :uncommented "service: api\nenvironment: production\n"))"####
    ]];
    ParityBatchCase::value(
        "project_files_activate_yaml_editor_configuration_comments_and_imenu",
        elisp_form,
        expected,
    )
}

fn structural_editing_indents_sequences_cycles_levels_and_runs_electric_yaml_keys()
-> ParityBatchCase {
    let elisp_form = r##"
(let (sequence-cycle block-literal folded-literal delimiters backspace)
  (setq sequence-cycle
        (neomacs-yaml-mode-test-in-buffer
         "-\nchild\n"
         (lambda ()
           (forward-line 1)
           (let (states)
             (dotimes (_ 3)
               (setq last-command this-command
                     this-command 'yaml-indent-line)
               (yaml-indent-line)
               (push (list (current-indentation)
                           (neomacs-yaml-mode-test-text))
                     states))
             (nreverse states)))))
  (setq block-literal
        (neomacs-yaml-mode-test-in-buffer
         "message: "
         (lambda ()
           (goto-char (point-max))
           (execute-kbd-macro "|")
           (list (neomacs-yaml-mode-test-text)
                 (line-number-at-pos)
                 (current-column)))))
  (setq folded-literal
        (neomacs-yaml-mode-test-in-buffer
         "summary: "
         (lambda ()
           (goto-char (point-max))
           (execute-kbd-macro ">")
           (list (neomacs-yaml-mode-test-text)
                 (line-number-at-pos)
                 (current-column)))))
  (setq delimiters
        (list
         (neomacs-yaml-mode-test-in-buffer
          "    "
          (lambda ()
            (goto-char (point-max))
            (execute-kbd-macro "---")
            (neomacs-yaml-mode-test-text)))
         (neomacs-yaml-mode-test-in-buffer
          "    "
          (lambda ()
            (goto-char (point-max))
            (execute-kbd-macro "...")
            (neomacs-yaml-mode-test-text)))))
  (setq backspace
        (neomacs-yaml-mode-test-in-buffer
         "root:\n      child\n"
         (lambda ()
           (forward-line 1)
           (back-to-indentation)
           (execute-kbd-macro (kbd "DEL"))
           (list (current-indentation)
                 (current-column)
                 (neomacs-yaml-mode-test-text)))))
  (list :sequence-cycle sequence-cycle
        :block-literal block-literal
        :folded-literal folded-literal
        :delimiters delimiters
        :backspace backspace))
"##;
    let expected = expect![[
        r####"OK (:sequence-cycle ((2 "-\n  child\n") (0 "-\nchild\n") (2 "-\n  child\n")) :block-literal ("message: |\n  " 2 2) :folded-literal ("summary: >-\n  " 2 2) :delimiters ("    ---" "    ...") :backspace (4 4 "root:\n    child\n"))"####
    ]];
    ParityBatchCase::value(
        "structural_editing_indents_sequences_cycles_levels_and_runs_electric_yaml_keys",
        elisp_form,
        expected,
    )
}

fn production_manifest_fontifies_yaml_semantics_and_reclassifies_inline_hash_edits()
-> ParityBatchCase {
    let elisp_form = r##"
(neomacs-yaml-mode-test-in-buffer
 "%YAML 1.2\n---\ndefaults: &defaults\n  enabled: True # production\n  token: foo#bar\n  tag: !!str\n  quoted: 'it''s # data'\n  note: |\n    first # literal\n    second \"quoted\"\ncopy: *defaults\n...\n"
 (lambda ()
   (let ((before-faces (neomacs-yaml-mode-test-face-spans))
         (hash-before (neomacs-yaml-mode-test-syntax-at "foo#bar" 3))
         hash-after hash-restored)
     (goto-char (point-min))
     (search-forward "foo#bar")
     (goto-char (match-beginning 0))
     (forward-char 3)
     (insert " ")
     (font-lock-flush)
     (font-lock-ensure)
     (setq hash-after (neomacs-yaml-mode-test-syntax-at "foo #bar" 5))
     (delete-char -1)
     (font-lock-flush)
     (font-lock-ensure)
     (setq hash-restored (neomacs-yaml-mode-test-syntax-at "foo#bar" 3))
     (list
      :faces before-faces
      :syntax
      (list
       hash-before hash-after hash-restored
       (neomacs-yaml-mode-test-syntax-at "# production" 2)
       (neomacs-yaml-mode-test-syntax-at "'it''s # data'" 7)
       (neomacs-yaml-mode-test-syntax-at "first # literal" 6)
       (neomacs-yaml-mode-test-syntax-at "second \"quoted\"" 7))
      :restored-text (neomacs-yaml-mode-test-text)))))
"##;
    let expected = expect![[
        r####"OK (:faces ((2 6 "YAML" font-lock-builtin-face) (11 14 "---" font-lock-comment-face) (15 23 "defaults" font-lock-variable-name-face) (25 34 "&defaults" font-lock-function-name-face) (37 44 "enabled" font-lock-variable-name-face) (46 50 "True" font-lock-constant-face) (51 53 "# " font-lock-comment-delimiter-face) (53 64 "production\n" font-lock-comment-face) (66 71 "token" font-lock-variable-name-face) (83 86 "tag" font-lock-variable-name-face) (88 93 "!!str" font-lock-type-face) (96 102 "quoted" font-lock-variable-name-face) (104 118 "'it''s # data'" font-lock-string-face) (121 125 "note" font-lock-variable-name-face) (139 141 "# " font-lock-comment-delimiter-face) (141 149 "literal\n" font-lock-comment-face) (160 168 "\"quoted\"" font-lock-string-face) (169 173 "copy" font-lock-variable-name-face) (175 184 "*defaults" font-lock-function-name-face) (185 188 "..." font-lock-comment-face)) :syntax (("foo#bar" 3 76 35 3 nil nil nil #1=(3) nil) ("foo #bar" 5 78 98 2 nil t 77 nil nil) ("foo#bar" 3 76 35 3 nil nil nil #1# nil) ("# production" 2 53 112 2 nil t 51 nil nil) ("'it''s # data'" 7 111 35 11 39 nil 104 nil nil) ("first # literal" 6 139 35 11 nil nil nil nil t) ("second \"quoted\"" 7 160 34 2 nil nil nil (2) t)) :restored-text "%YAML 1.2\n---\ndefaults: &defaults\n  enabled: True # production\n  token: foo#bar\n  tag: !!str\n  quoted: 'it''s # data'\n  note: |\n    first # literal\n    second \"quoted\"\ncopy: *defaults\n...\n")"####
    ]];
    ParityBatchCase::value(
        "production_manifest_fontifies_yaml_semantics_and_reclassifies_inline_hash_edits",
        elisp_form,
        expected,
    )
}

fn paragraph_filling_stays_inside_block_literals_and_preserves_comment_prefixes() -> ParityBatchCase
{
    let elisp_form = r##"
(list
 :literal
 (neomacs-yaml-mode-test-in-buffer
  "message: |\n  this deployment explanation is deliberately long enough to wrap within the literal block only\nnext: preserved\n"
  (lambda ()
    (forward-line 1)
    (let ((fill-column 34))
      (yaml-fill-paragraph))
    (list :text (neomacs-yaml-mode-test-text)
          :restriction (list (point-min) (point-max))
          :next-key (save-excursion
                      (goto-char (point-min))
                      (search-forward "next:")
                      (line-number-at-pos)))))
 :comment
 (neomacs-yaml-mode-test-in-buffer
  "# Deployment operators should verify the region and the release identifier before promotion\n# Additional context remains part of the same comment paragraph\nservice: api\n"
  (lambda ()
    (let ((fill-column 42))
      (yaml-fill-paragraph))
    (list :text (neomacs-yaml-mode-test-text)
          :service-line (save-excursion
                          (goto-char (point-min))
                          (search-forward "service:")
                          (line-number-at-pos))))))
"##;
    let expected = expect![[
        r####"OK (:literal (:text "message: |\n  this deployment explanation is\n  deliberately long enough to wrap\n  within the literal block only\nnext: preserved\n" :restriction (1 128) :next-key 5) :comment (:text "# Deployment operators should verify the\n# region and the release identifier before\n# promotion Additional context remains\n# part of the same comment paragraph\nservice: api\n" :service-line 5))"####
    ]];
    ParityBatchCase::value(
        "paragraph_filling_stays_inside_block_literals_and_preserves_comment_prefixes",
        elisp_form,
        expected,
    )
}

fn multi_document_navigation_and_imenu_build_a_stable_operations_outline() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-yaml-mode-test-in-buffer
 "---\nservice: api\nowner: platform\n--- %YAML:1.0\nservice: worker\nqueue: critical\n...\n"
 (lambda ()
   (let ((imenu-use-markers nil)
         forward-lines backward-line index)
     (setq index
           (mapcar (lambda (entry)
                     (list (car entry)
                           (line-number-at-pos (cdr entry))))
                   (imenu-default-create-index-function)))
     (goto-char (point-min))
     (dotimes (_ 3)
       (forward-page)
       (push (list (line-number-at-pos) (point)) forward-lines))
     (backward-page)
     (setq backward-line (list (line-number-at-pos) (point)))
     (list :index index
           :forward (nreverse forward-lines)
           :backward backward-line
           :delimiter page-delimiter))))
"##;
    let expected = expect![[
        r####"OK (:index (("service" 2) ("owner" 3) ("service" 5) ("queue" 6)) :forward ((2 5) (5 48) (8 84)) :backward (5 48) :delimiter "^---\\([ \11].*\\)*\n")"####
    ]];
    ParityBatchCase::value(
        "multi_document_navigation_and_imenu_build_a_stable_operations_outline",
        elisp_form,
        expected,
    )
}

#[test]
fn yaml_mode_package_batch() {
    assert_oracle_batch_cases(
        yaml_mode_oracle(),
        "yaml-mode-package-batch",
        "YAML Mode",
        &[
            project_files_activate_yaml_editor_configuration_comments_and_imenu(),
            structural_editing_indents_sequences_cycles_levels_and_runs_electric_yaml_keys(),
            production_manifest_fontifies_yaml_semantics_and_reclassifies_inline_hash_edits(),
            paragraph_filling_stays_inside_block_literals_and_preserves_comment_prefixes(),
            multi_document_navigation_and_imenu_build_a_stable_operations_outline(),
        ],
    );
}
