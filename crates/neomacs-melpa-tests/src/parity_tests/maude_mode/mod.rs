use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, MAUDE_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const MAUDE_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const MAUDE_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'maude-mode)

(defun maude-test-find (text)
  (goto-char (point-min))
  (unless (search-forward text nil t)
    (error "Missing Maude fixture text: %s" text))
  (goto-char (match-beginning 0))
  (point))

(defun maude-test-token-state (text)
  (save-excursion
    (maude-test-find text)
    (let* ((start (point))
           (syntax (syntax-after start)))
      (list :text text
            :range (list start (+ start (length text)))
            :face (get-text-property start 'face)
            :font-lock-face (get-text-property start 'font-lock-face)
            :syntax (and syntax (list (car syntax) (cdr syntax)))))))

(defun maude-test-normalize-index (index)
  (mapcar
   (lambda (entry)
     (cons (car entry)
           (if (markerp (cdr entry))
               (marker-position (cdr entry))
             (cdr entry))))
   index))

(defun maude-test-line-state ()
  (list :point (point)
        :line (line-number-at-pos)
        :column (current-column)
        :text (buffer-substring-no-properties
               (line-beginning-position)
               (line-end-position))))
"##;

fn maude_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(MAUDE_MODE_MELPA_PIN, "maude-mode.el")
        .expect("prepare pinned Maude Mode source below ./tmp")
        .with_prelude(MAUDE_MODE_TEST_PRELUDE)
        .with_timeout(MAUDE_MODE_TEST_TIMEOUT)
}

fn module_editor_configures_the_mode_and_fontifies_a_real_executable_specification()
-> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (insert
   "fmod NAT-LIST is\n"
   "  protecting NAT .\n"
   "  sorts NatList .\n"
   "  subsort Nat < NatList .\n"
   "  op nil : -> NatList [ctor] .\n"
   "  op _;_ : Nat NatList -> NatList [ctor assoc id: nil] .\n"
   "  vars N M : Nat .\n"
   "  eq [head] : head(N ; L) = N .\n"
   "  ceq [guarded] : safe(N) = true if N =/= 0 .\n"
   "endfm\n\n"
   "red in NAT-LIST : head(1 ; nil) .\n")
  (maude-mode)
  (font-lock-ensure)
  (list
   :mode
   (list major-mode mode-name
         (derived-mode-p 'prog-mode)
         comment-start comment-start-skip comment-end
         indent-line-function beginning-of-defun-function
         end-of-defun-function
         (eq local-abbrev-table maude-mode-abbrev-table))
   :keys
   (mapcar
    (lambda (key) (list key (lookup-key (current-local-map) (kbd key))))
    '("C-c C-c" "C-c C-r" "C-M-x" "C-c C-b" "C-c C-z"))
   :index
   (maude-test-normalize-index
    (imenu-default-create-index-function))
   :tokens
   (mapcar
    #'maude-test-token-state
    '("fmod" "NAT-LIST" "protecting" "NatList" "op" "ctor"
      "eq" "[head]" "ceq" "[guarded]" "if" "endfm" "red"))))
"##;
    let expect = expect![[
        r####"OK (:mode (maude-mode "Maude" prog-mode "***" "---+[ \11]*\\|\\*\\*\\*+[ \11]*" "" maude-indent-line maude-beginning-of-defun maude-end-of-defun t) :keys (("C-c C-c" maude-next-action) ("C-c C-r" maude-send-region) ("C-M-x" maude-send-definition) ("C-c C-b" maude-send-buffer) ("C-c C-z" maude-switch-to-inferior-maude)) :index (("NAT-LIST" . 1)) :tokens ((:text "fmod" :range (1 5) :face (maude-start-face) :font-lock-face nil :syntax (2 nil)) (:text "NAT-LIST" :range (6 14) :face (maude-module-name-face) :font-lock-face nil :syntax (2 nil)) (:text "protecting" :range (20 30) :face (font-lock-keyword-face) :font-lock-face nil :syntax (2 nil)) (:text "NatList" :range (45 52) :face (font-lock-type-face) :font-lock-face nil :syntax (2 nil)) (:text "op" :range (83 85) :face nil :font-lock-face nil :syntax (2 nil)) (:text "ctor" :range (104 108) :face (maude-element-face) :font-lock-face nil :syntax (2 nil)) (:text "eq" :range (190 192) :face (font-lock-keyword-face) :font-lock-face nil :syntax (2 nil)) (:text "[head]" :range (193 199) :face (maude-label-face) :font-lock-face nil :syntax (4 93)) (:text "ceq" :range (222 225) :face (font-lock-keyword-face) :font-lock-face nil :syntax (2 nil)) (:text "[guarded]" :range (226 235) :face (maude-label-face) :font-lock-face nil :syntax (4 93)) (:text "if" :range (253 255) :face font-lock-keyword-face :font-lock-face nil :syntax (2 nil)) (:text "endfm" :range (266 271) :face (maude-start-face) :font-lock-face nil :syntax (2 nil)) (:text "red" :range (273 276) :face maude-start-face :font-lock-face nil :syntax (2 nil))))"####
    ]];
    ParityBatchCase::value(
        "module_editor_configures_the_mode_and_fontifies_a_real_executable_specification",
        elisp_form,
        expect,
    )
}

fn indentation_formats_modules_conditionals_parenthesized_terms_and_object_attributes()
-> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (insert
   "fmod ACCOUNT is\n"
   "sort Account .\n"
   "op empty : -> Account [ctor] .\n"
   "ceq balance(A) = N\n"
   "if active(A)\n"
   "then N\n"
   "else 0\n"
   "fi .\n"
   "eq nested(A) = (\n"
   "balance(A)\n"
   ") .\n"
   "< a : Account |\n"
   "balance : 10,\n"
   "active : true >\n"
   "endfm\n")
  (maude-mode)
  (let ((maude-indent 3)
        (indent-tabs-mode nil)
        before-lines)
    (setq before-lines
          (mapcar
           (lambda (line)
             (goto-char (point-min))
             (forward-line (1- line))
             (maude-test-line-state))
           '(1 2 4 5 6 7 8 9 10 11 12 13 14 15)))
    (indent-region (point-min) (point-max))
    (let ((after (buffer-substring-no-properties (point-min) (point-max))))
      (maude-test-find "balance(A)")
      (move-to-column 15)
      (maude-indent-line)
      (list
       :before before-lines
       :after after
       :reindent-point (maude-test-line-state)
       :final (buffer-substring-no-properties (point-min) (point-max))))))
"##;
    let expect = expect![[
        r####"OK (:before ((:point 1 :line 1 :column 0 :text "fmod ACCOUNT is") (:point 17 :line 2 :column 0 :text "sort Account .") (:point 63 :line 4 :column 0 :text "ceq balance(A) = N") (:point 82 :line 5 :column 0 :text "if active(A)") (:point 95 :line 6 :column 0 :text "then N") (:point 102 :line 7 :column 0 :text "else 0") (:point 109 :line 8 :column 0 :text "fi .") (:point 114 :line 9 :column 0 :text "eq nested(A) = (") (:point 131 :line 10 :column 0 :text "balance(A)") (:point 142 :line 11 :column 0 :text ") .") (:point 146 :line 12 :column 0 :text "< a : Account |") (:point 162 :line 13 :column 0 :text "balance : 10,") (:point 176 :line 14 :column 0 :text "active : true >") (:point 192 :line 15 :column 0 :text "endfm")) :after "fmod ACCOUNT is\n   sort Account .\n   op empty : -> Account [ctor] .\n   ceq balance(A) = N\n      if active(A)\n      then N\n      else 0\n    fi .\n   eq nested(A) = (\n        balance(A)\n      ) .\n   < a : Account |\n                   balance : 10,\n                   active : true >\nendfm\n" :reindent-point (:point 84 :line 4 :column 15 :text "   ceq balance(A) = N") :final "fmod ACCOUNT is\n   sort Account .\n   op empty : -> Account [ctor] .\n   ceq balance(A) = N\n      if active(A)\n      then N\n      else 0\n    fi .\n   eq nested(A) = (\n        balance(A)\n      ) .\n   < a : Account |\n                   balance : 10,\n                   active : true >\nendfm\n")"####
    ]];
    ParityBatchCase::value(
        "indentation_formats_modules_conditionals_parenthesized_terms_and_object_attributes",
        elisp_form,
        expect,
    )
}

fn navigation_and_imenu_traverse_functional_object_and_view_definitions() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (insert
   "fmod NAT is\n"
   "  sort Nat .\n"
   "endfm\n\n"
   "omod ACCOUNT is\n"
   "  class Account | balance : Nat .\n"
   "endom\n\n"
   "view NAT-AS-ACCOUNT from NAT to ACCOUNT is\n"
   "  sort Nat to Account .\n"
   "endv\n")
  (maude-mode)
  (let ((index
         (maude-test-normalize-index
          (imenu-default-create-index-function)))
        traversals)
    (dolist (needle '("sort Nat ." "class Account" "sort Nat to Account"))
      (maude-test-find needle)
      (maude-beginning-of-defun)
      (let ((start (maude-test-line-state)))
        (maude-end-of-defun)
        (push
         (list :needle needle
               :start start
               :end (maude-test-line-state)
               :definition
               (buffer-substring-no-properties
                (plist-get start :point)
                (line-end-position)))
         traversals)))
    (goto-char (point-max))
    (maude-beginning-of-defun)
    (let ((last-start (maude-test-line-state)))
      (maude-beginning-of-defun)
      (list :index index
            :traversals (nreverse traversals)
            :last-start last-start
            :previous-start (maude-test-line-state)))))
"##;
    let expect = expect![[
        r####"OK (:index (("NAT" . 1) ("ACCOUNT" . 33) ("NAT-AS-ACCOUNT" . 90)) :traversals ((:needle "sort Nat ." :start (:point 1 :line 1 :column 0 :text "fmod NAT is") :end (:point 31 :line 3 :column 5 :text "endfm") :definition "fmod NAT is\n  sort Nat .\nendfm") (:needle "class Account" :start (:point 33 :line 5 :column 0 :text "omod ACCOUNT is") :end (:point 88 :line 7 :column 5 :text "endom") :definition "omod ACCOUNT is\n  class Account | balance : Nat .\nendom") (:needle "sort Nat to Account" :start (:point 90 :line 9 :column 0 :text "view NAT-AS-ACCOUNT from NAT to ACCOUNT is") :end (:point 161 :line 11 :column 4 :text "endv") :definition "view NAT-AS-ACCOUNT from NAT to ACCOUNT is\n  sort Nat to Account .\nendv")) :last-start (:point 90 :line 9 :column 0 :text "view NAT-AS-ACCOUNT from NAT to ACCOUNT is") :previous-start (:point 33 :line 5 :column 0 :text "omod ACCOUNT is"))"####
    ]];
    ParityBatchCase::value(
        "navigation_and_imenu_traverse_functional_object_and_view_definitions",
        elisp_form,
        expect,
    )
}

fn abbrev_authoring_merges_operator_attributes_and_places_point_inside_value_slots()
-> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (maude-mode)
  (abbrev-mode 1)
  (insert "op _+_ : Nat Nat -> Nat ")
  (let (states expansions)
    (dolist (word '("assoc" "commutative" "id"))
      (insert word)
      (let ((expanded (expand-abbrev)))
        (push (and expanded (symbol-name expanded)) expansions)
        (push (list :word word
                    :content (buffer-string)
                    :point (point)
                    :column (current-column))
              states))
      (unless (string= word "id")
        (insert " ")))
    (insert " 0")
    (end-of-line)
    (insert "\nops values : -> Nat ")
    (insert "set")
    (let ((set-expanded (expand-abbrev)))
      (list :expansions (nreverse expansions)
            :states (nreverse states)
            :set-expansion (and set-expanded (symbol-name set-expanded))
            :document (buffer-string)
            :point (point)
            :abbrevs
            (mapcar
             (lambda (name)
               (let ((symbol (abbrev-symbol name maude-mode-abbrev-table)))
                 (list name
                       (and symbol (symbol-value symbol))
                       (and symbol (abbrev-get symbol :hook)))))
             '("assoc" "commutative" "id" "set" "list"))))))
"##;
    let expect = expect![[
        r####"OK (:expansions ("assoc" "commutative" "id") :states ((:word "assoc" :content "op _+_ : Nat Nat -> Nat [assoc] ." :point 32 :column 31) (:word "commutative" :content "op _+_ : Nat Nat -> Nat [assoc comm] ." :point 37 :column 36) (:word "id" :content "op _+_ : Nat Nat -> Nat [assoc comm id:] ." :point 40 :column 39)) :set-expansion "set" :document "op _+_ : Nat Nat -> Nat [assoc comm id: 0] .\nops values : -> Nat [comm assoc idem] ." :point 83 :abbrevs (("assoc" "[assoc]" nil) ("commutative" "[comm]" nil) ("id" "[id:]" nil) ("set" "[comm assoc idem]" nil) ("list" "[assoc right id:]" nil)))"####
    ]];
    ParityBatchCase::value(
        "abbrev_authoring_merges_operator_attributes_and_places_point_inside_value_slots",
        elisp_form,
        expect,
    )
}

fn source_commands_send_regions_paragraphs_definitions_buffers_and_files_through_comint()
-> ParityBatchCase {
    let elisp_form = r##"
(let ((source (generate-new-buffer "maude-test-source"))
      (inferior (generate-new-buffer "maude-test-inferior"))
      events checks)
  (unwind-protect
      (with-current-buffer source
        (setq buffer-file-name "/workspace/specs/account.maude")
        (insert
         "fmod ONE is\n  sort One .\nendfm\n\n"
         "fmod TWO is\n  sort Two .\nendfm\n")
        (maude-mode)
        (let ((inferior-maude-buffer inferior))
          (cl-letf
              (((symbol-function 'comint-send-region)
                (lambda (process start end)
                  (push (list :region
                              (eq process inferior)
                              start end
                              (buffer-substring-no-properties start end))
                        events)))
               ((symbol-function 'comint-send-string)
                (lambda (process string)
                  (push (list :string (eq process inferior) string) events)))
               ((symbol-function 'comint-check-source)
                (lambda (file) (push file checks))))
            (maude-test-find "sort One")
            (maude-send-region (line-beginning-position) (line-end-position))
            (maude-test-find "sort Two")
            (maude-send-paragraph)
            (maude-test-find "sort One")
            (deactivate-mark)
            (maude-send-definition)
            (maude-send-buffer)
            (maude-send-file "/workspace/specs/extra.maude")
            (list :events (nreverse events)
                  :checks (nreverse checks)
                  :last-source (eq maude-last-source-buffer source)
                  :point (point)
                  :mark (mark t)
                  :region-active (region-active-p)))))
    (when (buffer-live-p source) (kill-buffer source))
    (when (buffer-live-p inferior) (kill-buffer inferior))))
"##;
    let expect = expect![[
        r####"OK (:events ((:region t 13 25 "  sort One .") (:string t "\n") (:region t 32 64 "\nfmod TWO is\n  sort Two .\nendfm\n") (:region t 1 32 "fmod ONE is\n  sort One .\nendfm\n") (:string t "in /workspace/specs/account.maude\n") (:string t "in /workspace/specs/extra.maude\n")) :checks ("/workspace/specs/account.maude" "/workspace/specs/extra.maude") :last-source t :point 15 :mark 32 :region-active nil)"####
    ]];
    ParityBatchCase::value(
        "source_commands_send_regions_paragraphs_definitions_buffers_and_files_through_comint",
        elisp_form,
        expect,
    )
}

fn inferior_mode_filters_duplicate_prompts_and_parses_warning_and_advisory_locations()
-> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (inferior-maude-mode)
  (let ((beginning-prompt (maude-preoutput-filter "Maude> "))
        diagnostics)
    (insert "result Nat: 3")
    (let ((inline-prompt (maude-preoutput-filter "Maude> ")))
      (dolist (text '("Warning: \"spec.maude\", line 17: bad equation"
                      "Advisory: \"lib.maude\", line 9: unused sort"))
        (let* ((rule (if (string-prefix-p "Warning" text)
                         (nth 0 maude-compilation-regexp-alist)
                       (nth 1 maude-compilation-regexp-alist)))
               (regexp (nth 0 rule)))
          (string-match regexp text)
          (push (list :text text
                      :file (match-string (nth 1 rule) text)
                      :line (string-to-number
                             (match-string (nth 2 rule) text))
                      :type (and (nth 3 rule)
                                 (nth 3 rule)))
                diagnostics)))
      (list
       :mode (list major-mode mode-name (derived-mode-p 'comint-mode))
       :filter-local
       (and (memq #'maude-preoutput-filter
                  comint-preoutput-filter-functions)
            t)
       :key (lookup-key (current-local-map) (kbd "C-c C-z"))
       :prompts (list beginning-prompt inline-prompt)
       :buffer (buffer-string)
       :diagnostics (nreverse diagnostics)
       :regexp-alist maude-compilation-regexp-alist))))
"##;
    let expect = expect![[
        r####"OK (:mode (inferior-maude-mode "inferior-maude" comint-mode) :filter-local t :key maude-switch-back-to-source :prompts ("Maude> " "") :buffer "result Nat: 3" :diagnostics ((:text "Warning: \"spec.maude\", line 17: bad equation" :file "spec.maude" :line 17 :type nil) (:text "Advisory: \"lib.maude\", line 9: unused sort" :file "lib.maude" :line 9 :type 1)) :regexp-alist (("^Warning: \"\\([^\"]+\\)\", line \\([0-9]+\\)" 1 2) ("^Advisory: \"\\([^\"]+\\)\", line \\([0-9]+\\)" 1 2 1)))"####
    ]];
    ParityBatchCase::value(
        "inferior_mode_filters_duplicate_prompts_and_parses_warning_and_advisory_locations",
        elisp_form,
        expect,
    )
}

#[test]
fn maude_mode_package_batch() {
    let cases = vec![
        module_editor_configures_the_mode_and_fontifies_a_real_executable_specification(),
        indentation_formats_modules_conditionals_parenthesized_terms_and_object_attributes(),
        navigation_and_imenu_traverse_functional_object_and_view_definitions(),
        abbrev_authoring_merges_operator_attributes_and_places_point_inside_value_slots(),
        source_commands_send_regions_paragraphs_definitions_buffers_and_files_through_comint(),
        inferior_mode_filters_duplicate_prompts_and_parses_warning_and_advisory_locations(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed Maude Mode parity test");
    assert_oracle_batch_cases(maude_mode_oracle(), test_name, "maude_mode_parity", &cases);
}
