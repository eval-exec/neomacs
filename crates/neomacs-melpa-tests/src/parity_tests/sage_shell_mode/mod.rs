use std::time::Duration;

use expect_test::expect;

use crate::{
    CachedMelpaOracle, DEFERRED_MELPA_PIN, LET_ALIST_GNU_ELPA_PIN, SAGE_SHELL_MODE_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const SAGE_SHELL_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const SAGE_SHELL_MODE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'button)
(require 'sage-shell-mode)

(defun sage-shell-mode-test-state (source interface &optional position edit-buffer)
  (with-temp-buffer
    (insert source)
    (goto-char (or position (point-max)))
    (let ((state (if edit-buffer
                     (sage-shell-edit:parse-current-state)
                   (sage-shell-cpl:parse-current-state interface))))
      (list
       :types (sage-shell-cpl:get state 'types)
       :interface (sage-shell-cpl:get state 'interface)
       :prefix (sage-shell-cpl:get state 'prefix)
       :base (sage-shell-cpl:get state 'var-base-name)
       :module (sage-shell-cpl:get state 'module-name)
       :call (sage-shell-cpl:get state 'in-function-call)
       :call-base (sage-shell-cpl:get state 'in-function-call-base-name)
       :call-end (sage-shell-cpl:get state 'in-function-call-end)))))

(defun sage-shell-mode-test-face-ranges (text)
  (let ((position 0)
        ranges)
    (while (< position (length text))
      (let* ((next (next-single-property-change position 'face text (length text)))
             (face (get-text-property position 'face text)))
        (when face
          (push (list position next face (substring-no-properties text position next)) ranges))
        (setq position next)))
    (nreverse ranges)))
"####;

fn sage_shell_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SAGE_SHELL_MODE_MELPA_PIN, "sage-shell-mode.el")
        .expect("prepare pinned sage-shell-mode source below ./tmp")
        .with_melpa_dependency(DEFERRED_MELPA_PIN)
        .expect("prepare pinned deferred dependency below ./tmp")
        .with_gnu_elpa_dependency(LET_ALIST_GNU_ELPA_PIN)
        .expect("prepare pinned let-alist dependency below ./tmp")
        .with_prelude(SAGE_SHELL_MODE_TEST_PRELUDE)
        .with_timeout(SAGE_SHELL_MODE_TEST_TIMEOUT)
}

fn repl_mode_activation_configures_a_real_comint_session_contract() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (let ((sage-shell:input-history-cache-file nil)
        (sage-shell:delete-temp-dir-p nil))
    (sage-shell-mode)
    (list
     :mode major-mode
     :parent (get major-mode 'derived-mode-parent)
     :process-buffer-same (eq sage-shell:process-buffer (current-buffer))
     :prompt-matches
     (mapcar (lambda (prompt) (and (string-match-p comint-prompt-regexp prompt) t))
             '("sage:" "sage0:" "....:" "gap:" "pari:" "(Pdb)" "ipdb>"))
     :prompt-read-only comint-prompt-read-only
     :prompt-regexp-mode comint-use-prompt-regexp
     :history-ignore-duplicates comint-input-ignoredups
     :redirect-completed comint-redirect-completed
     :parse-properties parse-sexp-lookup-properties
     :comment (list comment-start comment-start-skip)
     :eldoc eldoc-documentation-function
     :completion-hook completion-at-point-functions
     :output-filter-has-ansi
     (and (memq 'ansi-color-process-output comint-output-filter-functions) t))))
"####;
    let expect = expect![[
        r####"OK (:mode sage-shell-mode :parent comint-mode :process-buffer-same t :prompt-matches (t t t t t t t) :prompt-read-only t :prompt-regexp-mode t :history-ignore-duplicates t :redirect-completed t :parse-properties t :comment ("# " "^[ \11]*#+ *") :eldoc sage-shell:eldoc-function :completion-hook (sage-shell:completion-at-point-func comint-completion-at-point t) :output-filter-has-ansi nil)"####
    ]];
    ParityBatchCase::value(
        "repl_mode_activation_configures_a_real_comint_session_contract",
        elisp_form,
        expect,
    )
}

fn repl_completion_recognizes_attributes_interfaces_nested_calls_and_multiline_input()
-> ParityBatchCase {
    let elisp_form = r####"
(list
 (sage-shell-mode-test-state "sage: matrix.parent().base_r" "sage")
 (sage-shell-mode-test-state "sage: gap.eval(\"Group((1,2))\").Ord" "sage")
 (sage-shell-mode-test-state
  "sage: solve(matrix(ZZ, [[1, 2], [3, 4]]), algorithm=\"\""
  "sage")
 (sage-shell-mode-test-state
  "sage: def characteristic_polynomial(matrix):\n....:     matrix.char"
  "sage")
 (sage-shell-mode-test-state "gap: ConjugacyClassesSub" "gap"))
"####;
    let expect = expect![[
        r####"OK ((:types nil :interface "sage" :prefix 23 :base nil :module nil :call nil :call-base nil :call-end nil) (:types nil :interface "sage" :prefix 32 :base nil :module nil :call nil :call-base nil :call-end nil) (:types ("in-function-call" "interface") :interface "sage" :prefix nil :base nil :module nil :call "solve" :call-base nil :call-end 12) (:types ("attributes") :interface "sage" :prefix 63 :base "matrix" :module nil :call nil :call-base nil :call-end nil) (:types ("interface") :interface "gap" :prefix 6 :base nil :module nil :call nil :call-base nil :call-end nil))"####
    ]];
    ParityBatchCase::value(
        "repl_completion_recognizes_attributes_interfaces_nested_calls_and_multiline_input",
        elisp_form,
        expect,
    )
}

fn source_completion_tracks_multiline_imports_attributes_and_function_arguments() -> ParityBatchCase
{
    let elisp_form = r####"
(list
 (sage-shell-mode-test-state "from sage.rings.polynomial.polynomial_" "sage" nil t)
 (sage-shell-mode-test-state
  "from sage.matrix.constructor import (matrix,\n                                     diagonal_"
  "sage" nil t)
 (sage-shell-mode-test-state
  "def publish(matrix):\n    return matrix.parent().base_r"
  "sage" nil t)
 (sage-shell-mode-test-state
  "result = solve(matrix(ZZ, [[1, 2], [3, 4]]), algorithm=\"\""
  "sage" nil t))
"####;
    let expect = expect![[
        r####"OK ((:types ("modules") :interface "sage" :prefix 28 :base nil :module "sage.rings.polynomial" :call nil :call-base nil :call-end nil) (:types ("vars-in-module") :interface "sage" :prefix 83 :base nil :module "sage.matrix.constructor" :call nil :call-base nil :call-end nil) (:types ("interface") :interface "sage" :prefix 49 :base nil :module nil :call nil :call-base nil :call-end nil) (:types ("interface") :interface "sage" :prefix nil :base nil :module nil :call "solve" :call-base nil :call-end 15))"####
    ]];
    ParityBatchCase::value(
        "source_completion_tracks_multiline_imports_attributes_and_function_arguments",
        elisp_form,
        expect,
    )
}

fn eldoc_splits_real_nested_arguments_and_selects_positional_keyword_and_varargs() -> ParityBatchCase
{
    let elisp_form = r####"
(let* ((signature
        "solve(equations, variables=None, solution_dict=False, *flags, **options)")
       (arguments
        "[x^2 + y^2 == 1, x-y == 0], (x, y), solution_dict=True, algorithm=\"groebner,fast\"")
       (parts (sage-shell:-eldoc-split-buffer-args arguments)))
  (list
   :parts parts
   :positional-0 (sage-shell:-eldoc-highlight-beg-end "solve" signature nil 0)
   :positional-1 (sage-shell:-eldoc-highlight-beg-end "solve" signature nil 1)
   :keyword (sage-shell:-eldoc-highlight-beg-end
             "solve" signature "solution_dict" nil)
   :unknown-keyword (sage-shell:-eldoc-highlight-beg-end
                     "solve" signature "algorithm" nil)
   :varargs (sage-shell:-eldoc-highlight-beg-end "solve" signature nil 7)))
"####;
    let expect = expect![[
        r####"OK (:parts ("[x^2 + y^2 == 1  x-y == 0]" " (x  y)" " solution_dict=True" " algorithm=\"groebner fast\"") :positional-0 (6 . 15) :positional-1 (17 . 31) :keyword (33 . 52) :unknown-keyword (62 . 71) :varargs (54 . 60))"####
    ]];
    ParityBatchCase::value(
        "eldoc_splits_real_nested_arguments_and_selects_positional_keyword_and_varargs",
        elisp_form,
        expect,
    )
}

fn terminal_transcript_applies_carriage_returns_cursor_motion_and_output_fields() -> ParityBatchCase
{
    let elisp_form = r####"
(with-temp-buffer
  (sage-shell:-insert-str "sage: ")
  (sage-shell:-insert-and-handle-ansi-escape
   nil
   (concat (string 27) "[6D" (string 27) "[Jsage: matrix.rank()"
           (string 27) "[14D\r\r\n" (string 27) "[J4\n"))
  (list
   :text (buffer-substring-no-properties (point-min) (point-max))
   :point (point)
   :line (line-number-at-pos)
   :column (current-column)
   :field-runs
   (let ((position (point-min)) runs)
     (while (< position (point-max))
       (let* ((field (get-text-property position 'field))
              (next (next-single-property-change position 'field nil (point-max))))
         (push (list position next field
                     (buffer-substring-no-properties position next)) runs)
         (setq position next)))
     (nreverse runs))))
"####;
    let expect = expect![[
        r####"OK (:text "sage: matrix.rank()\n4\n" :point 23 :line 3 :column 0 :field-runs ((1 20 output "sage: matrix.rank()") (20 21 nil "\n") (21 23 output "4\n")))"####
    ]];
    ParityBatchCase::value(
        "terminal_transcript_applies_carriage_returns_cursor_motion_and_output_fields",
        elisp_form,
        expect,
    )
}

fn worksheet_navigation_and_send_current_preserve_block_boundaries_and_titles() -> ParityBatchCase {
    let elisp_form = r####"
(progn
  (require 'sage-shell-blocks)
  (with-temp-buffer
    (insert
     "### setup\nR.<x> = PolynomialRing(QQ)\n\n### compute roots\nf = x^2 - 1\nroots = f.roots()\n\n### publish\nprint(roots)\n")
    (goto-char (point-min))
    (search-forward "f = x^2")
    (let (sent)
      (cl-letf (((symbol-function 'sage-shell-edit:send-region)
                 (lambda (begin end)
                   (setq sent (buffer-substring-no-properties begin end)))))
        (sage-shell-blocks:send-current))
      (let ((after-send (list (line-number-at-pos) (current-column))))
        (sage-shell-blocks:forward 1)
        (let ((after-forward (list (line-number-at-pos) (current-column))))
          (sage-shell-blocks:backward 2)
          (list
           :sent sent
           :after-send after-send
           :after-forward after-forward
           :after-backward (list (line-number-at-pos) (current-column))
           :at-delimiter (looking-at sage-shell-blocks:delimiter)))))))
"####;
    let expect = expect![[
        r####"OK (:sent "### compute roots\nprint(\" ---- compute roots ---- \")\nf = x^2 - 1\nroots = f.roots()\n\n" :after-send (8 0) :after-forward (10 0) :after-backward (4 0) :at-delimiter t)"####
    ]];
    ParityBatchCase::value(
        "worksheet_navigation_and_send_current_preserve_block_boundaries_and_titles",
        elisp_form,
        expect,
    )
}

fn traceback_processing_turns_real_source_locations_into_navigable_buttons() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (let ((sage-shell:prefer-development-file-p nil)
        (sage-shell:process-buffer (current-buffer)))
    (insert
     "Traceback (most recent call last):\n/home/research/sage/model.py in solve_system(matrix)\n     40 coefficients = matrix.rows()\n---> 41 return eliminate(coefficients)\n     42\n")
    (sage-shell:make-error-links (point-min) (point-max))
    (goto-char (point-min))
    (search-forward "/home/research/sage/model.py")
    (let* ((position (- (point) (length "/home/research/sage/model.py")))
           (button (button-at position)))
      (list
       :text (buffer-substring-no-properties (point-min) (point-max))
       :button-label (and button (button-label button))
       :file (and button (button-get button 'sage-shell:file))
       :line (and button (button-get button 'sage-shell:line))
       :follow-link (and button (button-get button 'follow-link))
       :process-buffer-same
       (and button
            (eq (button-get button 'sage-shell:proc-buf) (current-buffer)))))))
"####;
    let expect = expect![[
        r####"OK (:text "Traceback (most recent call last):\n/home/research/sage/model.py in solve_system(matrix)\n     40 coefficients = matrix.rows()\n---> 41 return eliminate(coefficients)\n     42\n" :button-label "/home/research/sage/model.py" :file "/home/research/sage/model.py" :line 41 :follow-link t :process-buffer-same t)"####
    ]];
    ParityBatchCase::value(
        "traceback_processing_turns_real_source_locations_into_navigable_buttons",
        elisp_form,
        expect,
    )
}

fn completion_state_serialization_preserves_python_protocol_values_and_order() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((state
        '((interface . "sage")
          (types "interface" "in-function-call")
          (prefix . 37)
          (var-base-name . nil)
          (in-function-call . "solve")))
       (request (append state '((include-private . t)))))
  (list
   :state-valid (sage-shell-cpl-statep state)
   :dictionary (sage-shell:-to-python-dict request)
   :types (sage-shell:-to-python-list (sage-shell-cpl:get state 'types))
   :missing (sage-shell-cpl:get state 'module-name)
   :call (sage-shell-cpl:get state 'in-function-call)))
"####;
    let expect = expect![[
        r####"OK (:state-valid t :dictionary "{\"interface\": \"sage\", \"types\": [\"interface\", \"in-function-call\"], \"prefix\": 37, \"var-base-name\": None, \"in-function-call\": \"solve\", \"include-private\": True}" :types "[\"interface\", \"in-function-call\"]" :missing nil :call "solve")"####
    ]];
    ParityBatchCase::value(
        "completion_state_serialization_preserves_python_protocol_values_and_order",
        elisp_form,
        expect,
    )
}

#[test]
fn sage_shell_mode_package_batch() {
    let cases = vec![
        repl_mode_activation_configures_a_real_comint_session_contract(),
        repl_completion_recognizes_attributes_interfaces_nested_calls_and_multiline_input(),
        source_completion_tracks_multiline_imports_attributes_and_function_arguments(),
        eldoc_splits_real_nested_arguments_and_selects_positional_keyword_and_varargs(),
        terminal_transcript_applies_carriage_returns_cursor_motion_and_output_fields(),
        worksheet_navigation_and_send_current_preserve_block_boundaries_and_titles(),
        traceback_processing_turns_real_source_locations_into_navigable_buttons(),
        completion_state_serialization_preserves_python_protocol_values_and_order(),
    ];
    let thread = std::thread::current();
    let test_name = thread
        .name()
        .unwrap_or("unnamed sage-shell-mode parity test");
    assert_oracle_batch_cases(
        sage_shell_mode_oracle(),
        test_name,
        "sage_shell_mode_parity",
        &cases,
    );
}
