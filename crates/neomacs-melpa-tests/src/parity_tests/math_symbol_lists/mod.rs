use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, MATH_SYMBOL_LISTS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const MATH_SYMBOL_LISTS_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const MATH_SYMBOL_LISTS_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'math-symbol-lists)

(defun msl-test-standard-symbol (entry)
  (or (nth 3 entry)
      (when-let ((codepoint (nth 2 entry)))
        (when-let ((character (decode-char 'ucs codepoint)))
          (char-to-string character)))))

(defun msl-test-find-standard (command &optional list)
  (cl-find command
           (or list
               (append math-symbol-list-basic
                       math-symbol-list-extended))
           :key #'cadr
           :test #'string=))

(defun msl-test-find-standard-all (command)
  (cl-remove-if-not
   (lambda (entry) (string= command (nth 1 entry)))
   (append math-symbol-list-basic math-symbol-list-extended)))

(defun msl-test-find-package-all (command)
  (cl-remove-if-not
   (lambda (entry) (string= command (nth 2 entry)))
   math-symbol-list-packages))

(defun msl-test-completion-candidates ()
  (let ((seen (make-hash-table :test #'equal))
        candidates)
    (dolist (entry (append math-symbol-list-basic math-symbol-list-extended))
      (let* ((command (substring (nth 1 entry) 1))
             (symbol (msl-test-standard-symbol entry))
             (display (and symbol (concat command " " symbol))))
        (when (and display (not (gethash display seen)))
          (puthash display t seen)
          (push (cons display
                      (list :command command
                            :symbol symbol
                            :class (car entry)
                            :codepoint (nth 2 entry)))
                candidates))))
    (nreverse candidates)))

(defun msl-test-render-commands (commands)
  (mapconcat
   (lambda (command)
     (let ((entry (msl-test-find-standard command)))
       (unless entry
         (error "Missing math symbol command: %s" command))
       (msl-test-standard-symbol entry)))
   commands
   ""))

(defun msl-test-script-symbol (command list)
  (let ((entry (msl-test-find-standard command list)))
    (unless entry
      (error "Missing scripted symbol command: %s" command))
    (msl-test-standard-symbol entry)))

(defun msl-test-shape-counts (list)
  (let ((counts (make-hash-table :test #'eql)))
    (dolist (entry list)
      (puthash (length entry)
               (1+ (gethash (length entry) counts 0))
               counts))
    (let (result)
      (maphash (lambda (shape count)
                 (push (cons shape count) result))
               counts)
      (sort result (lambda (left right) (< (car left) (car right)))))))
"##;

fn math_symbol_lists_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(MATH_SYMBOL_LISTS_MELPA_PIN, "math-symbol-lists.el")
        .expect("prepare pinned Math Symbol Lists source below ./tmp")
        .with_prelude(MATH_SYMBOL_LISTS_TEST_PRELUDE)
        .with_timeout(MATH_SYMBOL_LISTS_TEST_TIMEOUT)
}

fn completion_engine_resolves_long_arrow_candidates_and_inserts_the_selected_symbol()
-> ParityBatchCase {
    let elisp_form = r##"
(let* ((candidates (msl-test-completion-candidates))
       (names (mapcar #'car candidates))
       (prefix "longright")
       (matches (all-completions prefix names))
       (try (try-completion prefix names))
       (selected-name
        (cl-find "longrightarrow"
                 matches
                 :test #'string-prefix-p))
       (selected (cdr (assoc selected-name candidates))))
  (with-temp-buffer
    (insert "state A ")
    (insert (plist-get selected :symbol))
    (insert " state B")
    (list
     :prefix prefix
     :try try
     :matches matches
     :selected-name selected-name
     :selected selected
     :document (buffer-string)
     :characters (string-to-list (buffer-string)))))
"##;
    let expect = expect![[
        r####"OK (:prefix "longright" :try "longright" :matches ("longrightarrow ⟶" "longrightsquigarrow ⟿") :selected-name "longrightarrow ⟶" :selected (:command "longrightarrow" :symbol "⟶" :class "arrow" :codepoint 10230) :document "state A ⟶ state B" :characters (115 116 97 116 101 32 65 32 10230 32 115 116 97 116 101 32 66))"####
    ]];
    ParityBatchCase::value(
        "completion_engine_resolves_long_arrow_candidates_and_inserts_the_selected_symbol",
        elisp_form,
        expect,
    )
}

fn formula_editor_renders_a_mixed_quantifier_relation_and_script_expression() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((commands '("\\forall" "\\alpha" "\\in" "\\int"
                   "\\leq" "\\infty" "\\rightarrow" "\\beta"
                   "\\neq"))
       (entries (mapcar #'msl-test-find-standard commands))
       (symbols (mapcar #'msl-test-standard-symbol entries))
       (sub-two (msl-test-script-symbol "_2" math-symbol-list-subscripts))
       (super-two (msl-test-script-symbol "^2" math-symbol-list-superscripts)))
  (with-temp-buffer
    (insert (nth 0 symbols) (nth 1 symbols) sub-two " "
            (nth 2 symbols) " domain: "
            (nth 3 symbols) " x" super-two " dx "
            (nth 4 symbols) " " (nth 5 symbols) " "
            (nth 6 symbols) " " (nth 7 symbols) super-two " "
            (nth 8 symbols) " 0")
    (list
     :commands commands
     :entries entries
     :symbols symbols
     :scripts (list sub-two super-two)
     :formula (buffer-string)
     :codepoints (string-to-list (buffer-string)))))
"##;
    let expect = expect![[
        r####"OK (:commands ("\\forall" "\\alpha" "\\in" "\\int" "\\leq" "\\infty" "\\rightarrow" "\\beta" "\\neq") :entries (("misc" "\\forall" 8704) ("greek" "\\alpha" 945) ("rel" "\\in" 8712) ("var" "\\int" 8747) ("rel" "\\leq" 8804) ("misc" "\\infty" 8734) ("arrow" "\\rightarrow" 8594) ("greek" "\\beta" 946) ("rel" "\\neq" 8800)) :symbols ("∀" "α" "∈" "∫" "≤" "∞" "→" "β" "≠") :scripts ("₂" "²") :formula "∀α₂ ∈ domain: ∫ x² dx ≤ ∞ → β² ≠ 0" :codepoints (8704 945 8322 32 8712 32 100 111 109 97 105 110 58 32 8747 32 120 178 32 100 120 32 8804 32 8734 32 8594 32 946 178 32 8800 32 48))"####
    ]];
    ParityBatchCase::value(
        "formula_editor_renders_a_mixed_quantifier_relation_and_script_expression",
        elisp_form,
        expect,
    )
}

fn duplicate_commands_preserve_canonical_and_styled_unicode_choices_with_package_conflicts()
-> ParityBatchCase {
    let elisp_form = r##"
(let* ((commands '("\\alpha" "\\partial" "\\rightarrow" "\\Digamma"))
       (standard
        (mapcar
         (lambda (command)
           (cons command (msl-test-find-standard-all command)))
         commands))
       (packages
        (mapcar
         (lambda (command)
           (cons command (msl-test-find-package-all command)))
         commands))
       (canonical
        (mapcar
         (lambda (command)
           (let ((entry (msl-test-find-standard command)))
             (list command
                   (car entry)
                   (msl-test-standard-symbol entry)
                   (nth 2 entry))))
         commands)))
  (list
   :canonical canonical
   :standard standard
   :packages packages
   :rendered (msl-test-render-commands commands)
   :standard-counts (mapcar (lambda (entry) (length (cdr entry))) standard)
   :package-counts (mapcar (lambda (entry) (length (cdr entry))) packages)
   :package-conflicts
   (mapcar
    (lambda (entry)
      (list (car entry)
            (mapcar (lambda (choice)
                      (list (nth 0 choice)
                            (nth 4 choice)
                            (and (nth 5 choice) t)))
                    (cdr entry))))
    packages)))
"##;
    let expect = expect![[
        r####"OK (:canonical (("\\alpha" "greek" "α" 945) ("\\partial" "misc" "∂" 8706) ("\\rightarrow" "arrow" "→" 8594) ("\\Digamma" "mathalpha" "Ϝ" 988)) :standard (("\\alpha" ("greek" "\\alpha" 945) ("mathalpha" "\\alpha" 945 "α") ("mathalpha" "\\alpha" 120572 "𝛼")) ("\\partial" ("misc" "\\partial" 8706) ("mathord" "\\partial" 8706 "∂") ("mathord" "\\partial" 120597 "𝜕")) ("\\rightarrow" ("arrow" "\\rightarrow" 8594) ("mathrel" "\\rightarrow" 8594 "→")) ("\\Digamma" ("mathalpha" "\\Digamma" 988 "Ϝ"))) :packages (("\\alpha" ("literal" "mathalpha" "\\alpha" 945 "α" t)) ("\\partial" ("literal" "mathord" "\\partial" 8706 "∂" t)) ("\\rightarrow") ("\\Digamma" ("amssymb" "mathalpha" "\\Digamma" 988 "Ϝ" t) ("wrisym" "mathalpha" "\\Digamma" 988 "Ϝ"))) :rendered "α∂→Ϝ" :standard-counts (3 3 2 1) :package-counts (1 1 0 2) :package-conflicts (("\\alpha" (("literal" "α" t))) ("\\partial" (("literal" "∂" t))) ("\\rightarrow" nil) ("\\Digamma" (("amssymb" "Ϝ" t) ("wrisym" "Ϝ" nil)))))"####
    ]];
    ParityBatchCase::value(
        "duplicate_commands_preserve_canonical_and_styled_unicode_choices_with_package_conflicts",
        elisp_form,
        expect,
    )
}

fn latex_export_collects_every_required_package_and_preserves_alternative_providers()
-> ParityBatchCase {
    let elisp_form = r##"
(let* ((commands '("\\checkmark" "\\iint" "\\boxplus"
                   "\\leadsto" "\\underparen"))
       (providers
        (mapcar
         (lambda (command)
           (cons command (msl-test-find-package-all command)))
         commands))
       (preferred
        (mapcar
         (lambda (entry)
           (let ((choice (car (cdr entry))))
             (list (car entry)
                   :package (nth 0 choice)
                   :class (nth 1 choice)
                   :codepoint (nth 3 choice)
                   :symbol (nth 4 choice)
                   :conflict (and (nth 5 choice) t))))
         providers))
       (required-packages
        (delete-dups (mapcar (lambda (entry) (plist-get (cdr entry) :package))
                             preferred))))
  (list
   :providers providers
   :preferred preferred
   :required-packages required-packages
   :preamble
   (mapconcat (lambda (package)
                (format "\\usepackage{%s}" package))
              required-packages
              "\n")
   :rendered
   (mapconcat (lambda (entry) (plist-get (cdr entry) :symbol))
              preferred
              " ")))
"##;
    let expect = expect![[
        r####"OK (:providers (("\\checkmark" ("amsfonts" "mathord" "\\checkmark" 10003 "✓")) ("\\iint" ("amsmath" "mathop" "\\iint" 8748 "∬") ("esint" "mathop" "\\iint" 8748 "∬") ("fourier" "mathop" "\\iint" 8748 "∬") ("wasysym" "mathop" "\\iint" 8748 "∬")) ("\\boxplus" ("amssymb" "mathbin" "\\boxplus" 8862 "⊞")) ("\\leadsto" ("txfonts" "mathrel" "\\leadsto" 10547 "⤳")) ("\\underparen" ("wrisym" "mathunder" "\\underparen" 9181 "⏝"))) :preferred (("\\checkmark" :package "amsfonts" :class "mathord" :codepoint 10003 :symbol "✓" :conflict nil) ("\\iint" :package "amsmath" :class "mathop" :codepoint 8748 :symbol "∬" :conflict nil) ("\\boxplus" :package "amssymb" :class "mathbin" :codepoint 8862 :symbol "⊞" :conflict nil) ("\\leadsto" :package "txfonts" :class "mathrel" :codepoint 10547 :symbol "⤳" :conflict nil) ("\\underparen" :package "wrisym" :class "mathunder" :codepoint 9181 :symbol "⏝" :conflict nil)) :required-packages ("amsfonts" "amsmath" "amssymb" "txfonts" "wrisym") :preamble "\\usepackage{amsfonts}\n\\usepackage{amsmath}\n\\usepackage{amssymb}\n\\usepackage{txfonts}\n\\usepackage{wrisym}" :rendered "✓ ∬ ⊞ ⤳ ⏝")"####
    ]];
    ParityBatchCase::value(
        "latex_export_collects_every_required_package_and_preserves_alternative_providers",
        elisp_form,
        expect,
    )
}

fn scientific_labels_render_supported_subscripts_and_superscripts_with_exact_fallbacks()
-> ParityBatchCase {
    let elisp_form = r##"
(let* ((sub-commands '("_2" "_beta" "_gamma" "_rho" "_varphi" "_x"))
       (super-commands '("^2" "^-" "^n" "^A" "^beta" "^varphi"))
       (subs
        (mapcar (lambda (command)
                  (msl-test-script-symbol command math-symbol-list-subscripts))
                sub-commands))
       (supers
        (mapcar (lambda (command)
                  (msl-test-script-symbol command math-symbol-list-superscripts))
                super-commands))
       (missing-sub (msl-test-find-standard "_q" math-symbol-list-subscripts))
       (missing-super (msl-test-find-standard "^C" math-symbol-list-superscripts)))
  (list
   :sub-commands sub-commands
   :sub-symbols subs
   :super-commands super-commands
   :super-symbols supers
   :labels
   (list
    (concat "H" (nth 0 subs) "O")
    (concat "v" (nth 1 subs) (nth 2 subs))
    (concat "ρ" (nth 3 subs) "φ" (nth 4 subs) "x" (nth 5 subs))
    (concat "x" (nth 0 supers) (nth 1 supers) (nth 2 supers))
    (concat "basis" (nth 3 supers) (nth 4 supers) (nth 5 supers)))
   :entries
   (append
    (mapcar (lambda (command)
              (msl-test-find-standard command math-symbol-list-subscripts))
            sub-commands)
    (mapcar (lambda (command)
              (msl-test-find-standard command math-symbol-list-superscripts))
            super-commands))
   :missing (list missing-sub missing-super)))
"##;
    let expect = expect![[
        r####"OK (:sub-commands ("_2" "_beta" "_gamma" "_rho" "_varphi" "_x") :sub-symbols ("₂" "ᵦ" "ᵧ" "ᵨ" "ᵩ" "ₓ") :super-commands ("^2" "^-" "^n" "^A" "^beta" "^varphi") :super-symbols ("²" "⁻" "ⁿ" "ᴬ" "ᵝ" "ᵠ") :labels ("H₂O" "vᵦᵧ" "ρᵨφᵩxₓ" "x²⁻ⁿ" "basisᴬᵝᵠ") :entries (("subscript" "_2" 8322 "₂") ("subscript" "_beta" 7526 "ᵦ") ("subscript" "_gamma" 7527 "ᵧ") ("subscript" "_rho" 7528 "ᵨ") ("subscript" "_varphi" 7529 "ᵩ") ("subscript" "_x" 8339 "ₓ") ("superscripts" "^2" 178 "²") ("superscripts" "^-" 8315 "⁻") ("superscripts" "^n" 8319 "ⁿ") ("superscripts" "^A" 7468 "ᴬ") ("superscripts" "^beta" 7517 "ᵝ") ("superscripts" "^varphi" 7520 "ᵠ")) :missing (nil nil))"####
    ]];
    ParityBatchCase::value(
        "scientific_labels_render_supported_subscripts_and_superscripts_with_exact_fallbacks",
        elisp_form,
        expect,
    )
}

fn command_palette_completes_real_latex_authoring_commands_and_preserves_odd_entries()
-> ParityBatchCase {
    let elisp_form = r##"
(let ((prefixes '("bibliogra" "text" "new" "makebox" "u")))
  (list
   :queries
   (mapcar
    (lambda (prefix)
      (list :prefix prefix
            :try (try-completion prefix math-symbol-list-latex-commands)
            :all (all-completions prefix math-symbol-list-latex-commands)))
    prefixes)
   :trailing-space-commands
   (cl-remove-if-not
    (lambda (command) (string-suffix-p " " command))
    math-symbol-list-latex-commands)
   :case-sensitive
   (list (all-completions "Alph" math-symbol-list-latex-commands)
         (all-completions "alph" math-symbol-list-latex-commands))
   :membership
   (mapcar
    (lambda (command)
      (cons command (and (member command math-symbol-list-latex-commands) t)))
    '("documentclass" "usepackage" "begin" "end" "section"
      "textbf" "includegraphics"))))
"##;
    let expect = expect![[
        r####"OK (:queries ((:prefix "bibliogra" :try "bibliography" :all ("bibliography" "bibliographystyle")) (:prefix "text" :try "text" :all ("textbf" "textfloatsep" "textfraction" "textheight" "textit" "textmd" "textnormal" "textrm" "textsc" "textsf" "textsl" "texttt" "textup" "textwidth")) (:prefix "new" :try "new" :all ("newcommand" "newcounter" "newenvironment" "newfont" "newlength" "newline" "newpage" "newsavebox" "newtheorem")) (:prefix "makebox" :try "makebox" :all ("makebox" "makebox ")) (:prefix "u" :try "u" :all ("u " "unboldmath" "unitlength" "upshape" "usebox" "usecounter" "usefont" "usepackage"))) :trailing-space-commands ("makebox " "u ") :case-sensitive (("Alph" "Alph\n    example") ("alph")) :membership (("documentclass" . t) ("usepackage" . t) ("begin" . t) ("end") ("section" . t) ("textbf" . t) ("includegraphics")))"####
    ]];
    ParityBatchCase::value(
        "command_palette_completes_real_latex_authoring_commands_and_preserves_odd_entries",
        elisp_form,
        expect,
    )
}

fn full_symbol_corpus_has_stable_shapes_unicode_pairs_conflicts_and_content_digests()
-> ParityBatchCase {
    let elisp_form = r##"
(let* ((lists
        `((basic . ,math-symbol-list-basic)
          (extended . ,math-symbol-list-extended)
          (packages . ,math-symbol-list-packages)
          (subscripts . ,math-symbol-list-subscripts)
          (superscripts . ,math-symbol-list-superscripts)))
       (standard-lists
        `((basic . ,math-symbol-list-basic)
          (extended . ,math-symbol-list-extended)
          (subscripts . ,math-symbol-list-subscripts)
          (superscripts . ,math-symbol-list-superscripts))))
  (list
   :summaries
   (mapcar
    (lambda (named)
      (let ((name (car named))
            (list (cdr named)))
        (list name
              :length (length list)
              :shapes (msl-test-shape-counts list)
              :sha256 (secure-hash 'sha256 (prin1-to-string list)))))
    lists)
   :unicode-mismatches
   (mapcar
    (lambda (named)
      (cons
       (car named)
       (cl-count-if
        (lambda (entry)
          (and (nth 3 entry)
               (not (string= (char-to-string (decode-char 'ucs (nth 2 entry)))
                             (nth 3 entry)))))
        (cdr named))))
    standard-lists)
   :package-unicode-mismatches
   (cl-count-if
    (lambda (entry)
      (not (string= (char-to-string (decode-char 'ucs (nth 3 entry)))
                    (nth 4 entry))))
    math-symbol-list-packages)
   :package-conflict-count
   (cl-count-if (lambda (entry) (nth 5 entry)) math-symbol-list-packages)
   :latex-command-count (length math-symbol-list-latex-commands)
   :latex-command-sha256
   (secure-hash 'sha256 (prin1-to-string math-symbol-list-latex-commands))))
"##;
    let expect = expect![[
        r####"OK (:summaries ((basic :length 279 :shapes ((2 . 50) (3 . 213) (4 . 16)) :sha256 "084d136e1609a6bc2e3027cb1164ae760d08de42a757cfcd0e9ad790bc69006e") (extended :length 2750 :shapes ((4 . 2750)) :sha256 "89a75b95d4876452902d37a7f48cd3aa193d8991a849792ec42711a682743be4") (packages :length 722 :shapes ((5 . 554) (6 . 168)) :sha256 "91f4d4031a0c9ec900a7c204c6e5a31a85c9c5f05dccbe35b9f1476d2bf3522e") (subscripts :length 37 :shapes ((4 . 37)) :sha256 "ddd391bacf0d21cadf82e3031acf75b88376406117e3963d0b32d58439d9d37e") (superscripts :length 66 :shapes ((4 . 66)) :sha256 "fd8747090073870bfbd5977378afe51955cd74e0ecad830f01d0b22265317b83")) :unicode-mismatches ((basic . 0) (extended . 0) (subscripts . 0) (superscripts . 0)) :package-unicode-mismatches 0 :package-conflict-count 168 :latex-command-count 323 :latex-command-sha256 "ab918739d37dc4c2793fe1ad4060049c17628bce8906954ff03226c53e955fe9")"####
    ]];
    ParityBatchCase::value(
        "full_symbol_corpus_has_stable_shapes_unicode_pairs_conflicts_and_content_digests",
        elisp_form,
        expect,
    )
}

#[test]
fn math_symbol_lists_package_batch() {
    let cases = vec![
        completion_engine_resolves_long_arrow_candidates_and_inserts_the_selected_symbol(),
        formula_editor_renders_a_mixed_quantifier_relation_and_script_expression(),
        duplicate_commands_preserve_canonical_and_styled_unicode_choices_with_package_conflicts(),
        latex_export_collects_every_required_package_and_preserves_alternative_providers(),
        scientific_labels_render_supported_subscripts_and_superscripts_with_exact_fallbacks(),
        command_palette_completes_real_latex_authoring_commands_and_preserves_odd_entries(),
        full_symbol_corpus_has_stable_shapes_unicode_pairs_conflicts_and_content_digests(),
    ];
    let thread = std::thread::current();
    let test_name = thread
        .name()
        .unwrap_or("unnamed Math Symbol Lists parity test");
    assert_oracle_batch_cases(
        math_symbol_lists_oracle(),
        test_name,
        "math_symbol_lists_parity",
        &cases,
    );
}
