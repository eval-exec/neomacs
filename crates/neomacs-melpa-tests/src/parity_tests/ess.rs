use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, ESS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r###"
(require 'cl-lib)
(require 'ess-site)
(require 'imenu)

(defun neomacs-ess-test-normalize-index (entries buffer)
  "Convert Imenu ENTRIES to stable names and line numbers in BUFFER."
  (mapcar
   (lambda (entry)
     (if (imenu--subalist-p entry)
         (cons (car entry)
               (neomacs-ess-test-normalize-index (cdr entry) buffer))
       (let ((position (if (listp (cdr entry)) (cadr entry) (cdr entry))))
         (if (and (numberp position) (< position 0))
             (list (car entry) :rescan)
           (list (car entry)
                 (with-current-buffer buffer
                   (line-number-at-pos position)))))))
   entries))

(defun neomacs-ess-test-token-state (token &optional occurrence offset)
  "Return TOKEN's face and syntactic state at OCCURRENCE plus OFFSET."
  (goto-char (point-min))
  (dotimes (_ (or occurrence 1))
    (search-forward token))
  (let* ((position (+ (match-beginning 0) (or offset 0)))
         (state (syntax-ppss position)))
    (list token
          :face (get-text-property position 'face)
          :string (and (nth 3 state) t)
          :comment (and (nth 4 state) t)
          :depth (car state))))

(defun neomacs-ess-test-project-root ()
  "Return the deterministic ESS project fixture root."
  (file-name-as-directory
   (expand-file-name "ess-project"
                     (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(defun neomacs-ess-test-write-project ()
  "Create a realistic deterministic R package and return its source file."
  (let* ((root (neomacs-ess-test-project-root))
         (source-directory (expand-file-name "R" root))
         (source (expand-file-name "checkout.R" source-directory)))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory source-directory t)
    (with-temp-file (expand-file-name "DESCRIPTION" root)
      (insert "Package: checkout\n"
              "Type: Package\n"
              "Title: Checkout Calculations\n"
              "Version: 1.2.3\n"
              "Authors@R: person(\"Neo\", \"Macs\", role = c(\"aut\", \"cre\"), email = \"neo@example.test\")\n"
              "Description: Deterministic parity fixture.\n"
              "License: GPL-3\n"))
    (with-temp-file source
      (insert "checkout_total <- function(items) {\n"
              "  sum(items)\n"
              "}\n"))
    source))
"###;

fn package_contract_exposes_r_modes_commands_keys_and_file_associations() -> ParityBatchCase {
    let elisp_form = r###"
(let ((descriptor (cadr (assq 'ess package-alist))))
  (with-temp-buffer
    (ess-r-mode)
    (list
     :package
     (list :name (package-desc-name descriptor)
           :version (package-version-join (package-desc-version descriptor))
           :requirements (package-desc-reqs descriptor)
           :features (mapcar #'featurep '(ess-site ess ess-mode ess-r-mode)))
     :modes
     (mapcar #'commandp
             '(ess-mode ess-r-mode inferior-ess-r-mode ess-r-help-mode
               ess-r-transcript-mode R-mode r-mode))
     :aliases
     (list (eq (indirect-function 'R-mode) (indirect-function 'ess-r-mode))
           (eq (indirect-function 'r-mode) (indirect-function 'ess-r-mode)))
     :keys
     (mapcar (lambda (key) (lookup-key ess-r-mode-map (kbd key)))
             '("C-c C-r" "C-c C-b" "C-c C-f" "C-c C-c"
               "C-c C-j" "C-c C-z" "C-c C-l" "C-c C-="))
     :files
     (mapcar (lambda (name)
               (assoc-default name auto-mode-alist #'string-match))
             '("analysis.R" "analysis.r" ".Rprofile" "NAMESPACE" "CITATION")))))
"###;
    let expected = expect![[
        r#"OK (:package (:name ess :version "20260723.934" :requirements ((emacs (25 1))) :features (t t t t)) :modes (t t t t t t t) :aliases (t t) :keys (ess-eval-region ess-eval-buffer ess-eval-function ess-eval-region-or-function-or-paragraph-and-step ess-eval-line ess-switch-to-inferior-or-script-buffer ess-load-file ess-cycle-assign) :files (ess-r-mode ess-r-mode ess-r-mode ess-r-mode ess-r-mode))"#
    ]];
    ParityBatchCase::value(
        "package_contract_exposes_r_modes_commands_keys_and_file_associations",
        elisp_form,
        expected,
    )
}

fn realistic_r_source_initializes_editor_services_and_semantic_index() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert "checkout_total <- function(items, tax = 0.2) {\n"
          "  subtotal <- sum(items)\n"
          "  subtotal * (1 + tax)\n"
          "}\n\n"
          "setClass(\"Order\", slots = c(total = \"numeric\"))\n"
          "setMethod(\"show\", \"Order\", function(object) object@total)\n\n"
          "orders <- read.csv(\"orders.csv\")\n"
          "library(dplyr)\n")
  (ess-r-mode)
  (font-lock-ensure)
  (let ((index (imenu--make-index-alist t)))
    (list
     :mode (list major-mode mode-name
                 (derived-mode-p 'prog-mode)
                 indent-tabs-mode
                 indent-line-function comment-indent-function)
     :comments (list comment-start comment-end comment-start-skip)
     :navigation (list beginning-of-defun-function end-of-defun-function)
     :syntax (mapcar (lambda (char) (char-syntax char)) '(?_ ?% ?` ?: ?@ ?$ ?\\))
     :completion (mapcar (lambda (fn) (and (symbolp fn) fn))
                         completion-at-point-functions)
     :xref (and (memq #'ess-r-xref-backend xref-backend-functions) t)
     :project (and (memq #'ess-r-project project-find-functions) t)
     :imenu (neomacs-ess-test-normalize-index index (current-buffer)))))
"###;
    let expected = expect![[
        r##"OK (:mode (ess-r-mode "ESS[R]" prog-mode nil ess-r-indent-line ess-calculate-indent) :comments ("#" "" "#+ *") :navigation (ess-r-beginning-of-defun ess-r-end-of-defun) :syntax (95 34 34 46 46 46 46) :completion (ess-roxy-complete-tag ess-filename-completion ess-r-package-completion ess-r-object-completion t) :xref t :project t :imenu (("*Rescan*" :rescan) ("Data" ("orders" 9)) ("Package" ("dplyr" 10)) ("Methods" ("\"show\", \"Order\"" 7)) ("Classes" ("\"Order\"" 6)) ("Functions" ("checkout_total" 1))))"##
    ]];
    ParityBatchCase::value(
        "realistic_r_source_initializes_editor_services_and_semantic_index",
        elisp_form,
        expected,
    )
}

fn production_pipeline_indents_idempotently_under_rrr_and_rstudio_styles() -> ParityBatchCase {
    let elisp_form = r###"
(let ((source
       (concat
        "checkout_report <- function(orders, tax_rate = 0.2) {\n"
        "valid <- orders[orders$status == \"paid\", ]\n"
        "if (nrow(valid) > 0) {\n"
        "valid |>\n"
        "transform(total = subtotal * (1 + tax_rate)) |>\n"
        "aggregate(total ~ customer, data = _, FUN = sum)\n"
        "} else {\n"
        "data.frame(customer = character(), total = numeric())\n"
        "}\n"
        "}\n")))
  (mapcar
   (lambda (style)
     (with-temp-buffer
       (insert source)
       (ess-r-mode)
       (ess-set-style style t)
       (indent-region (point-min) (point-max))
       (let ((once (buffer-string))
             (indents
              (save-excursion
                (goto-char (point-min))
                (let (columns)
                  (while (not (eobp))
                    (push (current-indentation) columns)
                    (forward-line 1))
                  (nreverse columns)))))
         (indent-region (point-min) (point-max))
         (list :style style :text once :indents indents
               :idempotent (equal once (buffer-string))))))
   '(RRR RStudio-)))
"###;
    let expected = expect![[
        r#"OK ((:style RRR :text "checkout_report <- function(orders, tax_rate = 0.2) {\n    valid <- orders[orders$status == \"paid\", ]\n    if (nrow(valid) > 0) {\n        valid |>\n            transform(total = subtotal * (1 + tax_rate)) |>\n            aggregate(total ~ customer, data = _, FUN = sum)\n    } else {\n        data.frame(customer = character(), total = numeric())\n    }\n}\n" :indents (0 4 4 8 12 12 4 8 4 0) :idempotent t) (:style RStudio- :text "checkout_report <- function(orders, tax_rate = 0.2) {\n  valid <- orders[orders$status == \"paid\", ]\n  if (nrow(valid) > 0) {\n    valid |>\n      transform(total = subtotal * (1 + tax_rate)) |>\n      aggregate(total ~ customer, data = _, FUN = sum)\n  } else {\n    data.frame(customer = character(), total = numeric())\n  }\n}\n" :indents (0 2 2 4 6 6 2 4 2 0) :idempotent t))"#
    ]];
    ParityBatchCase::value(
        "production_pipeline_indents_idempotently_under_rrr_and_rstudio_styles",
        elisp_form,
        expected,
    )
}

fn fontification_and_raw_string_syntax_distinguish_real_r_program_roles() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert "checkout_total <- function(items) {\n"
          "  note <- r\"(customer said \\\"ready\\\")\"\n"
          "  if (length(items) > 0 && TRUE) {\n"
          "    items %>% sum() %>% print()\n"
          "  }\n"
          "  # production fallback\n"
          "  NULL\n"
          "}\n")
  (ess-r-mode)
  (font-lock-ensure)
  (list
   :tokens
   (mapcar
    (lambda (request)
      (apply #'neomacs-ess-test-token-state request))
    '(("checkout_total") ("<-" 1) ("function") ("if") ("length")
      ("&&") ("TRUE") ("%>%" 1) ("sum") ("print") ("NULL")))
   :raw-prefix (neomacs-ess-test-token-state "r\"(" 1 0)
   :raw-embedded-quote (neomacs-ess-test-token-state "\\\"ready" 1 0)
   :comment (neomacs-ess-test-token-state "production fallback" 1 3)))
"###;
    let expected = expect![[
        r#"OK (:tokens (("checkout_total" :face font-lock-function-name-face :string nil :comment nil :depth 0) ("<-" :face ess-assignment-face :string nil :comment nil :depth 0) ("function" :face ess-keyword-face :string nil :comment nil :depth 0) ("if" :face ess-keyword-face :string nil :comment nil :depth 1) ("length" :face nil :string nil :comment nil :depth 2) ("&&" :face nil :string nil :comment nil :depth 2) ("TRUE" :face ess-constant-face :string nil :comment nil :depth 2) ("%>%" :face ess-%op%-face :string nil :comment nil :depth 2) ("sum" :face nil :string nil :comment nil :depth 2) ("print" :face nil :string nil :comment nil :depth 2) ("NULL" :face ess-constant-face :string nil :comment nil :depth 1)) :raw-prefix ("r\"(" :face nil :string nil :comment nil :depth 1) :raw-embedded-quote ("\\\"ready" :face font-lock-string-face :string t :comment nil :depth 1) :comment ("production fallback" :face font-lock-comment-face :string nil :comment t :depth 1))"#
    ]];
    ParityBatchCase::value(
        "fontification_and_raw_string_syntax_distinguish_real_r_program_roles",
        elisp_form,
        expected,
    )
}

fn assignment_and_fill_commands_transform_real_editing_sessions_and_cycle_state() -> ParityBatchCase
{
    let elisp_form = r###"
(with-temp-buffer
  (ess-r-mode)
  (let (assignment-states fill-states)
    (insert "checkout_total   ")
    (let ((last-input-event ?_))
      (ess-insert-assign 1))
    (push (buffer-string) assignment-states)
    (let ((last-input-event ?_))
      (ess-insert-assign 1))
    (push (buffer-string) assignment-states)
    (erase-buffer)
    (insert "invoice_total  ")
    (let ((this-command 'ess-cycle-assign)
          (last-command nil)
          (last-input-event ?=))
      (ess-cycle-assign))
    (push (buffer-string) assignment-states)
    (dotimes (_ 3)
      (let ((this-command 'ess-cycle-assign)
            (last-command 'ess-cycle-assign)
            (last-input-event ?=))
        (ess-cycle-assign))
      (push (buffer-string) assignment-states))
    (erase-buffer)
    (insert "summarise_orders(customer_id, paid_orders, refunded_orders, gross_total, net_total)")
    (search-backward "(")
    (forward-char)
    (let ((fill-column 42)
          (ess-blink-refilling nil)
          (last-command nil)
          (this-command 'fill-paragraph))
      (fill-paragraph))
    (push (buffer-string) fill-states)
    (let ((fill-column 42)
          (ess-blink-refilling nil)
          (last-command 'fill-paragraph)
          (this-command 'fill-paragraph))
      (fill-paragraph))
    (push (buffer-string) fill-states)
    (list :assignments (nreverse assignment-states)
          :fills (nreverse fill-states)
          :point (point)
          :style-level ess-fill--style-level)))
"###;
    let expected = expect![[
        r#"OK (:assignments ("checkout_total <- " "checkout_total_" "invoice_total <- " "invoice_total <<- " "invoice_total = " "invoice_total -> ") :fills ("summarise_orders(customer_id, paid_orders,\n                 refunded_orders,\n                 gross_total, net_total)" "summarise_orders(customer_id,\n                 paid_orders,\n                 refunded_orders,\n                 gross_total,\n                 net_total)") :point 18 :style-level 2)"#
    ]];
    ParityBatchCase::value(
        "assignment_and_fill_commands_transform_real_editing_sessions_and_cycle_state",
        elisp_form,
        expected,
    )
}

fn nested_function_navigation_marking_narrowing_and_indexing_agree() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert "checkout_report <- function(orders) {\n"
          "  apply_discount <- function(order) {\n"
          "    order$total * 0.9\n"
          "  }\n"
          "  vapply(orders, apply_discount, numeric(1))\n"
          "}\n\n"
          "healthcheck <- function() \"ready\"\n")
  (ess-r-mode)
  (goto-char (point-min))
  (search-forward "order$total")
  (let ((inner-begin
         (save-excursion
           (ess-r-beginning-of-function 1 nil)
           (list (line-number-at-pos)
                 (buffer-substring-no-properties
                  (line-beginning-position) (line-end-position)))))
        (inner-end
         (save-excursion
           (ess-r-end-of-function 1 nil)
           (list (line-number-at-pos) (current-column))))
        (outer-bounds
         (save-excursion
           (ess-mark-function-or-para)
           (list (line-number-at-pos (region-beginning))
                 (line-number-at-pos (region-end))
                 (buffer-substring-no-properties
                  (region-beginning) (region-end)))))
        (index (imenu--make-index-alist t)))
    (ess-narrow-to-defun-or-para)
    (list :inner-begin inner-begin
          :inner-end inner-end
          :outer-bounds outer-bounds
          :narrowed (list (line-number-at-pos (point-min))
                          (line-number-at-pos (point-max))
                          (buffer-substring-no-properties
                           (point-min) (line-end-position)))
          :index (neomacs-ess-test-normalize-index index (current-buffer)))))
"###;
    let expected = expect![[
        r#"OK (:inner-begin (2 "  apply_discount <- function(order) {") :inner-end (4 3) :outer-bounds (1 7 "checkout_report <- function(orders) {\n  apply_discount <- function(order) {\n    order$total * 0.9\n  }\n  vapply(orders, apply_discount, numeric(1))\n}\n") :narrowed (1 7 "checkout_report <- function(orders) {\n  apply_discount <- function(order) {\n    order$total * 0.9") :index (("*Rescan*" :rescan) ("Functions" ("checkout_report" 1) ("healthcheck" 7))))"#
    ]];
    ParityBatchCase::value(
        "nested_function_navigation_marking_narrowing_and_indexing_agree",
        elisp_form,
        expected,
    )
}

fn r_evaluation_load_help_and_namespace_commands_preserve_exact_protocol() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (ess-r-mode)
  (let ((ess-dialect "R")
        (ess-r-namespaced-load-verbose t))
    (list
     :eval-default
     (ess-build-eval-command "checkout_total(c(10, 20))")
     :eval-visible-output
     (ess-build-eval-command "checkout_total(c(10, 20))" t t)
     :eval-namespace
     (ess-build-eval-command "private_helper(order)" t t "checkout.R" "checkout")
     :load-default (ess-build-load-command "checkout.R")
     :load-namespace (ess-build-load-command "checkout.R" nil t "checkout")
     :help
     (cl-letf (((symbol-function 'ess-r-help--find-package)
                (lambda (_object) "base")))
       (mapcar #'ess-build-help-command
               '("mean" "stats::median" "checkout:::private_helper")))
     :namespaces
     (mapcar #'ess-r--split-namespace
             '("mean" "stats::median" "checkout:::private_helper"))
     :arguments
     (list (ess-r-build-args nil nil nil)
           (ess-r-build-args t t "checkout")))))
"###;
    let expected = expect![[
        r#"OK (:eval-default "base::as.environment('ESSR')$.ess.eval(\"checkout_total(c(10, 20))\", visibly = FALSE, output = FALSE)\n" :eval-visible-output "base::as.environment('ESSR')$.ess.eval(\"checkout_total(c(10, 20))\", visibly = TRUE, output = TRUE)\n" :eval-namespace "base::as.environment('ESSR')$.ess.ns_eval(\"private_helper(order)\", visibly = TRUE, output = TRUE, package = 'checkout', verbose = TRUE, file = 'checkout.R')\n" :load-default "base::as.environment('ESSR')$.ess.source('checkout.R', visibly = FALSE, output = FALSE)\n" :load-namespace "base::as.environment('ESSR')$.ess.ns_source('checkout.R', visibly = FALSE, output = TRUE, package = 'checkout', verbose = TRUE)\n" :help ("base::as.environment('ESSR')$.ess.help('mean', package = 'base')\n" "base::as.environment('ESSR')$.ess.help('median', package = 'stats')\n" "base::as.environment('ESSR')$.ess.help(':private_helper', package = 'checkout')\n") :namespaces (nil ("stats" . "median") ("checkout" . ":private_helper")) :arguments (", visibly = FALSE, output = FALSE" ", visibly = TRUE, output = TRUE, package = 'checkout', verbose = TRUE"))"#
    ]];
    ParityBatchCase::value(
        "r_evaluation_load_help_and_namespace_commands_preserve_exact_protocol",
        elisp_form,
        expected,
    )
}

fn r_package_project_discovery_reads_description_and_tracks_source_tree() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((source (neomacs-ess-test-write-project))
       (root (neomacs-ess-test-project-root))
       (buffer (find-file-noselect source)))
  (unwind-protect
      (with-current-buffer buffer
        (ess-r-mode)
        (hack-local-variables)
        (let* ((info (ess-r-project-info))
               (project (ess-r-project))
               (package (ess-r-package-info)))
          (list
           :mode (list major-mode ess-r-package-mode)
           :project
           (list :name (plist-get info :name)
                 :root-name (file-name-nondirectory
                             (directory-file-name (plist-get info :root)))
                 :instance (car project)
                 :project-root-name
                 (file-name-nondirectory
                  (directory-file-name (project-root project))))
           :package
           (list :name (plist-get package :name)
                 :root-name
                 (file-name-nondirectory
                  (directory-file-name (plist-get package :root))))
           :sources
           (mapcar (lambda (directory)
                     (file-relative-name directory root))
                   (ess-r-package-source-dirs)))))
    (when (buffer-live-p buffer) (kill-buffer buffer))
    (when (file-exists-p root) (delete-directory root t))))
"###;
    let expected = expect![[
        r#"OK (:mode (ess-r-mode t) :project (:name "ess-project" :root-name "ess-project" :instance ess-r-project :project-root-name "ess-project") :package (:name "checkout" :root-name "ess-project") :sources ("R"))"#
    ]];
    ParityBatchCase::value(
        "r_package_project_discovery_reads_description_and_tracks_source_tree",
        elisp_form,
        expected,
    )
}

fn inferior_r_mode_inherits_comint_and_installs_console_editing_contract() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (inferior-ess-r-mode)
  (list
   :mode (list major-mode mode-name
               (derived-mode-p 'inferior-ess-mode)
               (derived-mode-p 'comint-mode))
   :process (list comint-prompt-regexp comint-use-prompt-regexp
                  comint-input-sender comint-get-old-input)
   :editing (list indent-line-function
                  comint-input-autoexpand
                  comint-move-point-for-output)
   :keys
   (mapcar (lambda (key) (lookup-key inferior-ess-r-mode-map (kbd key)))
           '("RET" "C-c C-=" "C-c C-." "M-r" "M-n" "M-p"))
   :syntax (mapcar (lambda (char) (char-syntax char)) '(?% ?` ?_ ?: ?@ ?$))))
"###;
    let expected = expect![[
        r#"OK (:mode (inferior-ess-r-mode "iESS" inferior-ess-mode comint-mode) :process ("[]a-zA-Z0-9.[]*[+ ]*> \\(?:[>+.] \\)*" t inferior-ess-r-input-sender inferior-ess-get-old-input) :editing (indent-relative t nil) :keys (inferior-ess-send-input ess-cycle-assign ess-rutils-map comint-history-isearch-backward-regexp comint-next-input comint-previous-input) :syntax (46 34 95 46 46 46))"#
    ]];
    ParityBatchCase::value(
        "inferior_r_mode_inherits_comint_and_installs_console_editing_contract",
        elisp_form,
        expected,
    )
}

#[test]
fn ess_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(ESS_MELPA_PIN, "ess-site.el")
            .expect("prepare revision-pinned ESS below ./tmp")
            .with_timeout(Duration::from_secs(360))
            .with_prelude(PRELUDE),
        "ess-package-batch",
        "ESS",
        &[
            package_contract_exposes_r_modes_commands_keys_and_file_associations(),
            realistic_r_source_initializes_editor_services_and_semantic_index(),
            production_pipeline_indents_idempotently_under_rrr_and_rstudio_styles(),
            fontification_and_raw_string_syntax_distinguish_real_r_program_roles(),
            assignment_and_fill_commands_transform_real_editing_sessions_and_cycle_state(),
            nested_function_navigation_marking_narrowing_and_indexing_agree(),
            r_evaluation_load_help_and_namespace_commands_preserve_exact_protocol(),
            r_package_project_discovery_reads_description_and_tracks_source_tree(),
            inferior_r_mode_inherits_comint_and_installs_console_editing_contract(),
        ],
    );
}
