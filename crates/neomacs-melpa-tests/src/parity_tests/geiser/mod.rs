use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, GEISER_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const GEISER_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const GEISER_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'geiser-mode)

(setq geiser-mode-auto-p t
      geiser-mode-start-repl-p nil
      geiser-mode-autodoc-p nil
      geiser-mode-smart-tab-p t)

(defun geiser-test-check-buffer ()
  (save-excursion
    (goto-char (point-min))
    (search-forward "#lang neomacs-test" nil t)))

(defun geiser-test-keywords ()
  (geiser-syntax--simple-keywords
   '("define-checkout" "order-case" "with-order" "with-order*")))

(defun geiser-test-marshall (procedure &rest arguments)
  (format "<%s%s>"
          procedure
          (if arguments
              (concat " " (mapconcat #'identity arguments " "))
            "")))

(defun geiser-test-find-module (&optional module)
  (or module '(commerce checkout)))

(define-geiser-implementation neomacs-test
  (check-buffer geiser-test-check-buffer)
  (keywords geiser-test-keywords)
  (case-sensitive t)
  (binding-forms '("with-order"))
  (binding-forms* '("with-order*"))
  (marshall-procedure geiser-test-marshall)
  (find-module geiser-test-find-module))

(provide 'geiser-neomacs-test)
(geiser-implementation-extension 'neomacs-test "ntscm")

(setq geiser-active-implementations '(neomacs-test)
      geiser-default-implementation 'neomacs-test)

(defvar geiser-test-sandbox
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun geiser-test-root (name)
  (let ((root (file-name-as-directory
               (expand-file-name name geiser-test-sandbox))))
    (when (file-exists-p root) (delete-directory root t))
    (make-directory root t)
    root))

(defun geiser-test-write (path contents)
  (make-directory (file-name-directory path) t)
  (write-region contents nil path nil 'silent)
  path)

(defun geiser-test-face-at (text)
  (goto-char (point-min))
  (search-forward text)
  (get-text-property (match-beginning 0) 'face))
"##;

fn geiser_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(GEISER_MELPA_PIN, "geiser.el")
        .expect("prepare pinned Geiser source below ./tmp")
        .with_prelude(GEISER_TEST_PRELUDE)
        .with_timeout(GEISER_TEST_TIMEOUT)
}

fn registered_implementation_activates_a_practical_scheme_editing_session() -> ParityBatchCase {
    ParityBatchCase::value(
        "registered_implementation_activates_a_practical_scheme_editing_session",
        r##"
(with-temp-buffer
  (setq buffer-file-name "/workspace/checkout.ntscm")
  (insert "#lang neomacs-test\n(define (checkout-total items) items)\n")
  (scheme-mode)
  (list
   :major-mode major-mode
   :geiser-mode geiser-mode
   :implementation geiser-impl--implementation
   :lighter (geiser-mode--lighter)
   :smart-tab (list geiser-smart-tab-mode tab-always-indent)
   :capfs completion-at-point-functions
   :bindings
   (mapcar (lambda (key) (key-binding (kbd key)))
           '("C-c C-z" "C-c C-c" "C-c C-e [" "C-c \\"))
   :generated-commands
   (list (commandp 'geiser-neomacs-test)
         (commandp 'geiser-neomacs-test-switch))
   :extension-match
   (geiser-impl--guess)))
"##,
        expect![[
            r##"OK (:major-mode scheme-mode :geiser-mode t :implementation neomacs-test :lighter " Neomacs-Test" :smart-tab (t complete) :capfs (geiser-capf--for-symbol geiser-capf--for-module geiser-capf--for-filename t) :bindings (geiser-mode-switch-to-repl geiser-eval-definition geiser-squarify geiser-insert-lambda) :generated-commands (t t) :extension-match neomacs-test)"##
        ]],
    )
}

fn scheme_editing_indents_fontifies_squarifies_and_inserts_lambda() -> ParityBatchCase {
    ParityBatchCase::value(
        "scheme_editing_indents_fontifies_squarifies_and_inserts_lambda",
        r##"
(with-temp-buffer
  (insert
   "#lang neomacs-test\n"
   "(define-checkout (quote-order items)\n"
   "(with-order ((subtotal 100)\n"
   "(shipping 12))\n"
   "(order-case (null? items)\n"
   "((#t) subtotal)\n"
   "(else (+ subtotal shipping)))))\n")
  (scheme-mode)
  (setq-local indent-tabs-mode nil)
  (indent-region (point-min) (point-max))
  (font-lock-ensure (point-min) (point-max))
  (let ((faces
         (list (geiser-test-face-at "define-checkout")
               (geiser-test-face-at "with-order")
               (geiser-test-face-at "order-case")
               (geiser-test-face-at "else"))))
    (goto-char (point-min))
    (search-forward "(with-order")
    (backward-char (length "(with-order"))
    (geiser-squarify 1)
    (goto-char (point-max))
    (insert "\n")
    (let ((geiser-insert-actual-lambda t))
      (geiser-insert-lambda t)
      (insert "item"))
    (list :source (buffer-substring-no-properties (point-min) (point-max))
          :faces faces
          :point (list (line-number-at-pos) (current-column))
          :balanced (condition-case nil
                        (progn (check-parens) t)
                      (error nil)))))
"##,
        expect![[
            r##"OK (:source "#lang neomacs-test\n(define-checkout (quote-order items)\n  [with-order ((subtotal 100)\n               (shipping 12))\n              (order-case (null? items)\n                          ((#t) subtotal)\n                          (else (+ subtotal shipping)))])\n\n(λ (item))" :faces (font-lock-keyword-face font-lock-keyword-face font-lock-keyword-face font-lock-keyword-face) :point (9 8) :balanced t)"##
        ]],
    )
}

fn completion_combines_lexical_bindings_and_remote_scheme_symbols() -> ParityBatchCase {
    ParityBatchCase::value(
        "completion_combines_lexical_bindings_and_remote_scheme_symbols",
        r##"
(with-temp-buffer
  (insert
   "#lang neomacs-test\n"
   "(define (quote-order items)\n"
   "  (with-order ((subtotal 100)\n"
   "               (shipping 12))\n"
   "    (with-order* ((tax-rate 0.20)\n"
   "                  (total (+ subtotal shipping)))\n"
   "      (list items sub))))\n")
  (scheme-mode)
  (goto-char (point-max))
  (search-backward "sub")
  (forward-char 3)
  (cl-letf (((symbol-function 'geiser-eval--send/result)
             (lambda (code &rest _)
               (if (string-match-p "module-completions" (format "%S" code))
                   '("(commerce checkout)" "(commerce inventory)")
                 '("subtotal" "subtotal-after-tax" "submit-order")))))
    (let* ((locals (geiser-completion--locals))
           (symbols (geiser-completion--symbol-list "sub"))
           (capf (geiser-capf--for-symbol))
           (beg (nth 0 capf))
           (end (nth 1 capf))
           (table (nth 2 capf)))
      (list :locals locals
            :symbols symbols
            :capf (list beg end
                        (buffer-substring-no-properties beg end)
                        (all-completions "sub" table))
            :module-completions
            (geiser-completion--complete "(commerce" t)))))
"##,
        expect![[
            r##"OK (:locals ("total" "tax-rate" "shipping" "subtotal" "items" "quote-order") :symbols ("subtotal" "subtotal-after-tax" "submit-order") :capf (209 212 "sub" ("subtotal" "subtotal-after-tax" "submit-order")) :module-completions ("(commerce checkout)" "(commerce inventory)"))"##
        ]],
    )
}

fn evaluation_protocol_marshals_requests_and_decodes_success_and_error_retorts() -> ParityBatchCase
{
    ParityBatchCase::value(
        "evaluation_protocol_marshals_requests_and_decodes_success_and_error_retorts",
        r##"
(with-temp-buffer
  (insert "#lang neomacs-test\n")
  (scheme-mode)
  (let* ((success
          '((result "42" "(order 17 ready)")
            (output "pricing cache hit" "audit queued")))
         (failure
          '((error (key . syntax-error)
                   (subr . quote-order)
                   (msg . "unexpected closing parenthesis")
                   (rest . "line 8"))
            (output "reader stopped"))))
    (list
     :requests
     (list
      (geiser-eval--scheme-str
       '(:eval "(+ subtotal tax)" (commerce checkout)))
      (geiser-eval--scheme-str
       '(:comp "(define (quote-order x) x)" :buffer))
      (geiser-eval--scheme-str '(:load-file "/workspace/checkout.scm"))
      (geiser-eval--scheme-str
       '(:ge completions "sub")))
     :success
     (list (geiser-eval--retort-p success)
           (geiser-eval--retort-result success)
           (geiser-eval--retort-result-str success "=> ")
           (geiser-eval--retort-output success))
     :failure
     (list (geiser-eval--retort-p failure)
           (geiser-eval--error-key (geiser-eval--retort-error failure))
           (geiser-eval--error-str (geiser-eval--retort-error failure))
           (geiser-eval--retort-output failure)))))
"##,
        expect![[
            r##"OK (:requests ("<eval (commerce checkout) \"(+ subtotal tax)\">" "<compile (commerce checkout) \"(define (quote-order x) x)\">" "<load-file \"/workspace/checkout.scm\">" "<completions \"sub\">") :success ((result "42" "(order 17 ready)") 42 "=> 42\n=> (order 17 ready)" ("pricing cache hit" "audit queued")) :failure ((error (key . syntax-error) (subr . quote-order) (msg . "unexpected closing parenthesis") (rest . "line 8")) syntax-error "Error (quote-order):: syntax-error\n  unexpected closing parenthesis\n  line 8" ("reader stopped")))"##
        ]],
    )
}

fn source_navigation_finds_definition_shapes_and_visits_reported_locations() -> ParityBatchCase {
    ParityBatchCase::value(
        "source_navigation_finds_definition_shapes_and_visits_reported_locations",
        r##"
(let* ((root (geiser-test-root "geiser-source-navigation"))
       (source (expand-file-name "checkout.scm" root))
       (contents
        (concat
         "#lang neomacs-test\n"
         "(define (checkout-total items)\n"
         "  (apply + (map cdr items)))\n\n"
         "(define-values (subtotal tax) (values 100 20))\n\n"
         "(define-syntax-rule (with-order order body ...)\n"
         "  (let ((current-order order)) body ...))\n"))
       buffer)
  (geiser-test-write source contents)
  (setq buffer (find-file-noselect source))
  (unwind-protect
      (with-current-buffer buffer
        (scheme-mode)
        (let ((function (geiser-edit--find-def 'checkout-total t))
              (value (geiser-edit--find-def 'subtotal t))
              (macro (geiser-edit--find-def 'with-order t))
              (location
               (geiser-edit--make-location
                "checkout-total" source "2" "2")))
          (let ((visited
                 (geiser-edit--try-edit-location
                  'checkout-total location 'noselect)))
            (list
             :definitions
             (mapcar
              (lambda (definition)
                (and definition
                     (list (line-number-at-pos (car definition))
                           (cdr definition))))
              (list function value macro))
             :location
             (list (geiser-edit--location-name location)
                   (file-name-nondirectory
                    (geiser-edit--location-file location))
                   (geiser-edit--location-line location)
                   (geiser-edit--location-column location))
             :visited
             (list (eq (car visited) buffer)
                   (line-number-at-pos (cdr visited))
                   (save-excursion
                     (goto-char (cdr visited))
                     (current-column))
                   (save-excursion
                     (goto-char (cdr visited))
                     (buffer-substring-no-properties
                      (line-beginning-position) (line-end-position))))))))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer))))
"##,
        expect![[
            r##"OK (:definitions ((2 "(checkout-total items)") (5 nil) (7 "(with-order order body ...)")) :location ("checkout-total" "checkout.scm" 2 2) :visited (t 2 2 "(define (checkout-total items)"))"##
        ]],
    )
}

#[test]
fn geiser_package_batch() {
    let cases = vec![
        registered_implementation_activates_a_practical_scheme_editing_session(),
        scheme_editing_indents_fontifies_squarifies_and_inserts_lambda(),
        completion_combines_lexical_bindings_and_remote_scheme_symbols(),
        evaluation_protocol_marshals_requests_and_decodes_success_and_error_retorts(),
        source_navigation_finds_definition_shapes_and_visits_reported_locations(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed Geiser parity test");
    assert_oracle_batch_cases(geiser_oracle(), test_name, "geiser_parity", &cases);
}
