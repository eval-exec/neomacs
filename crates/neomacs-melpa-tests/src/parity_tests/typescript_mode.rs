use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, TYPESCRIPT_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r###"
(require 'cl-lib)
(require 'typescript-mode)

(defun neomacs-ts-test-face (text occurrence)
  "Return TEXT's face at OCCURRENCE in the current buffer."
  (save-excursion
    (goto-char (point-min))
    (dotimes (_ occurrence)
      (search-forward text))
    (get-text-property (match-beginning 0) 'face)))

(defun neomacs-ts-test-syntax (text occurrence)
  "Return stable syntax state inside TEXT at OCCURRENCE."
  (save-excursion
    (goto-char (point-min))
    (dotimes (_ occurrence)
      (search-forward text))
    (let* ((position (1+ (match-beginning 0)))
           (state (syntax-ppss position))
           (start (nth 8 state)))
      (list :depth (nth 0 state)
            :string (nth 3 state)
            :comment (and (nth 4 state) t)
            :start (and start
                        (save-excursion
                          (goto-char start)
                          (list (line-number-at-pos) (current-column))))))))

(defun neomacs-ts-test-indent (source &optional switch list-items expr-offset)
  "Indent SOURCE under the selected TypeScript indentation policy."
  (let ((typescript-indent-level 2)
        (typescript-indent-switch-clauses
         (if (eq switch :disabled) nil
           (if (null switch) t switch)))
        (typescript-indent-list-items
         (if (eq list-items :disabled) nil
           (if (null list-items) t list-items)))
        (typescript-expr-indent-offset (or expr-offset 0)))
    (with-temp-buffer
      (insert source)
      (typescript-mode)
      (indent-region (point-min) (point-max))
      (delete-trailing-whitespace)
      (buffer-substring-no-properties (point-min) (point-max)))))

(defun neomacs-ts-test-point-state ()
  "Return stable source navigation state at point."
  (list :point (point)
        :line (line-number-at-pos)
        :column (current-column)
        :text (buffer-substring-no-properties
               (line-beginning-position) (line-end-position))
        :context (typescript-syntactic-context)))

(defun neomacs-ts-test-compilation-messages ()
  "Return the unique parsed diagnostics in the current compilation buffer."
  (let ((position (point-min)) seen result)
    (while (< position (point-max))
      (let ((message (get-text-property position 'compilation-message)))
        (when (and message (not (memq message seen)))
          (push message seen)
          (let* ((location (compilation--message->loc message))
                 (file-structure (compilation--loc->file-struct location))
                 (file-spec (car file-structure)))
            (push (list :rule (compilation--message->rule message)
                        :type (compilation--message->type message)
                        :file (car file-spec)
                        :line (compilation--loc->line location)
                        :column (compilation--loc->col location))
                  result))))
      (setq position
            (or (next-single-property-change
                 position 'compilation-message nil (point-max))
                (point-max))))
    (nreverse result)))
"###;

fn package_and_mode_contract_configure_a_typescript_editor_buffer() -> ParityBatchCase {
    let elisp_form = r###"
(let ((descriptor (cadr (assq 'typescript-mode package-alist))))
  (with-temp-buffer
    (typescript-mode)
    (list
     :package
     (list :name (package-desc-name descriptor)
           :version (package-version-join (package-desc-version descriptor))
           :requirements (package-desc-reqs descriptor)
           :feature (and (featurep 'typescript-mode) t))
     :mode
     (list major-mode mode-name
           (eq indent-line-function #'typescript-indent-line)
           (eq beginning-of-defun-function #'typescript-beginning-of-defun)
           (eq end-of-defun-function #'typescript-end-of-defun)
           (eq syntax-propertize-function #'typescript-syntax-propertize)
           (eq fill-paragraph-function #'typescript-c-fill-paragraph)
           parse-sexp-ignore-comments parse-sexp-lookup-properties
           open-paren-in-column-0-is-defun-start)
     :comments (list comment-start comment-end comment-start-skip)
     :editing
     (list :binding (lookup-key typescript-mode-map (kbd "C-c '"))
           :electric-indent
           (mapcar (lambda (character)
                     (and (memq character electric-indent-chars) t))
                   (string-to-list "{}():;,"))
           :electric-layout electric-layout-rules
           :post-insert
           (and (memq #'typescript--post-self-insert-function
                      post-self-insert-hook)
                t))
     :recognition
     (mapcar (lambda (filename)
               (assoc-default filename auto-mode-alist #'string-match))
             '("service.ts" "component.tsx" "service.js"))
     :defaults
     (list typescript-indent-level typescript-expr-indent-offset
           typescript-indent-switch-clauses typescript-indent-list-items
           typescript-flat-functions typescript-autoconvert-to-template-flag
           typescript-enabled-frameworks)
     :diagnostics
     (mapcar (lambda (rule)
               (and (memq rule compilation-error-regexp-alist) t))
             '(typescript-tsc typescript-tsc-pretty typescript-tslint
               typescript-nglint-error typescript-nglint-warning)))))
"###;
    let expected = expect![[
        r#"OK (:package (:name typescript-mode :version "20250118.2056" :requirements ((emacs (24 3))) :feature t) :mode (typescript-mode "TypeScript" t t t t t t t nil) :comments ("// " "" "\\(//+\\|/\\*+\\)\\s *") :editing (:binding typescript-convert-to-template :electric-indent (t t t t t t t) :electric-layout ((59 . after) (123 . after) (125 . before)) :post-insert t) :recognition (typescript-mode typescript-mode javascript-mode) :defaults (4 0 t t nil nil (typescript mochikit prototype dojo exttypescript merrillpress)) :diagnostics (t t t t t))"#
    ]];
    ParityBatchCase::value(
        "package_and_mode_contract_configure_a_typescript_editor_buffer",
        elisp_form,
        expected,
    )
}

fn production_service_source_indents_classes_generics_switches_and_chains() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-ts-test-indent
 "@sealed
class CheckoutService<T extends Order> {
private readonly retries: Map<string, number> = new Map();
async quote(
order: Order,
options: { currency: string; coupons: string[] },
): Promise<Result<T>> {
const payload = {
id: order.id,
items: [
...order.items,
{ sku: 'fee', quantity: 1 },
],
};
switch (options.currency) {
case 'USD':
return this.client
.post<Result<T>>('/quote', payload)
.then((response) => response.data);
default:
throw new Error(`Unsupported ${options.currency}`);
}
}
}
")
"###;
    let expected = expect![[
        r#"OK "@sealed\nclass CheckoutService<T extends Order> {\n  private readonly retries: Map<string, number> = new Map();\n  async quote(\n    order: Order,\n    options: { currency: string; coupons: string[] },\n  ): Promise<Result<T>> {\n    const payload = {\n      id: order.id,\n      items: [\n\11...order.items,\n\11{ sku: 'fee', quantity: 1 },\n      ],\n    };\n    switch (options.currency) {\n      case 'USD':\n\11return this.client\n\11    .post<Result<T>>('/quote', payload)\n\11    .then((response) => response.data);\n      default:\n\11throw new Error(`Unsupported ${options.currency}`);\n    }\n  }\n}\n""#
    ]];
    ParityBatchCase::value(
        "production_service_source_indents_classes_generics_switches_and_chains",
        elisp_form,
        expected,
    )
}

fn indentation_policies_reflow_switches_comma_first_lists_and_continuations() -> ParityBatchCase {
    let elisp_form = r###"
(let ((source
       "function route(region: string): Config {
switch (region) {
case 'us':
return {
hosts: [
'api-1',
'api-2',
],
timeout:
baseTimeout +
retryBudget,
};
default:
return {
hosts:
[ 'fallback'
, 'secondary' ],
};
}
}
"))
  (list
   :standard (neomacs-ts-test-indent source t t 0)
   :flat-switch (neomacs-ts-test-indent source :disabled t 0)
   :comma-first (neomacs-ts-test-indent source t :disabled 1)))
"###;
    let expected = expect![[
        r#"OK (:standard "function route(region: string): Config {\n  switch (region) {\n    case 'us':\n      return {\n\11hosts: [\n\11  'api-1',\n\11  'api-2',\n\11],\n\11timeout:\n\11baseTimeout +\n\11  retryBudget,\n      };\n    default:\n      return {\n\11hosts:\n\11[ 'fallback'\n\11  , 'secondary' ],\n      };\n  }\n}\n" :flat-switch "function route(region: string): Config {\n  switch (region) {\n  case 'us':\n    return {\n      hosts: [\n\11'api-1',\n\11'api-2',\n      ],\n      timeout:\n      baseTimeout +\n\11retryBudget,\n    };\n  default:\n    return {\n      hosts:\n      [ 'fallback'\n\11, 'secondary' ],\n    };\n  }\n}\n" :comma-first "function route(region: string): Config {\n  switch (region) {\n    case 'us':\n      return {\n\11hosts: [\n\11  'api-1',\n\11  'api-2',\n\11],\n\11timeout:\n\11baseTimeout +\n\11   retryBudget,\n      };\n    default:\n      return {\n\11hosts:\n\11[ 'fallback'\n\11, 'secondary' ],\n      };\n  }\n}\n")"#
    ]];
    ParityBatchCase::value(
        "indentation_policies_reflow_switches_comma_first_lists_and_continuations",
        elisp_form,
        expected,
    )
}

fn production_types_and_jsdoc_receive_context_sensitive_fontification() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert
   "/**
 * Price an order for [[CheckoutService]] using `retryPolicy`.
 * @param {Order} order active checkout order
 * @returns {Promise<Quote>} calculated quote
 * @throws {PricingError} when the upstream is unavailable
 */
@injectable
export class CheckoutService<T extends Order> implements Pricer {
  private readonly client: ApiClient;
  async quote(order: Order, options?: QuoteOptions): Promise<Quote> {
    const label = `order ${order.id}`;
    const response = await this.client.post<Quote>('/quote', order);
    return response.data;
  }
}
interface WireShape { type: number; unknown: string; enabled: boolean }
const predicate = (value: unknown): value is Quote => value != null;
")
  (typescript-mode)
  (font-lock-ensure (point-min) (point-max))
  (syntax-propertize (point-max))
  (mapcar
   (lambda (probe)
     (list (car probe) (cdr probe)
           (neomacs-ts-test-face (car probe) (cdr probe))))
   '(("@param" . 1) ("{Order}" . 1) ("order active" . 1)
     ("[[CheckoutService]]" . 1) ("`retryPolicy`" . 1)
     ("@returns" . 1) ("{Promise<Quote>}" . 1)
     ("@throws" . 1) ("{PricingError}" . 1)
     ("@injectable" . 1) ("CheckoutService" . 2) ("private" . 1)
     ("ApiClient" . 1) ("quote" . 1) ("order" . 3)
     ("QuoteOptions" . 1) ("Promise" . 2) ("this" . 1)
     ("post" . 1) ("${" . 1) ("order.id" . 1)
     ("type" . 1) ("unknown" . 1) ("value" . 1)
     ("Quote" . 6))))
"###;
    let expected = expect![[
        r#"OK (("@param" 1 typescript-jsdoc-tag) ("{Order}" 1 typescript-jsdoc-type) ("order active" 1 typescript-jsdoc-value) ("[[CheckoutService]]" 1 typescript-jsdoc-value) ("`retryPolicy`" 1 typescript-jsdoc-value) ("@returns" 1 typescript-jsdoc-tag) ("{Promise<Quote>}" 1 typescript-jsdoc-type) ("@throws" 1 typescript-jsdoc-tag) ("{PricingError}" 1 typescript-jsdoc-value) ("@injectable" 1 font-lock-function-call-face) ("CheckoutService" 2 font-lock-type-face) ("private" 1 typescript-access-modifier-face) ("ApiClient" 1 font-lock-type-face) ("quote" 1 typescript-jsdoc-type) ("order" 3 typescript-jsdoc-value) ("QuoteOptions" 1 font-lock-type-face) ("Promise" 2 font-lock-type-face) ("this" 1 typescript-this-face) ("post" 1 font-lock-function-call-face) ("${" 1 font-lock-keyword-face) ("order.id" 1 default) ("type" 1 default) ("unknown" 1 default) ("value" 1 nil) ("Quote" 6 font-lock-type-face))"#
    ]];
    ParityBatchCase::value(
        "production_types_and_jsdoc_receive_context_sensitive_fontification",
        elisp_form,
        expected,
    )
}

fn syntax_distinguishes_hashbang_regex_division_comments_strings_and_templates() -> ParityBatchCase
{
    let elisp_form = r###"
(with-temp-buffer
  (insert
   "#!/usr/bin/env node
const route = /^\\/orders\\/[0-9]+$/i;
const ratio = total / Math.max(count, 1);
const endpoint = 'https://api.example.test//v1';
// /not-a-regexp/ operational note
const label = `order ${order.id}: ${ratio}`;
return /paid|pending/.test(label) && ratio > 0;
/* block / marker */
")
  (typescript-mode)
  (syntax-propertize (point-max))
  (font-lock-ensure (point-min) (point-max))
  (list
   :syntax
   (mapcar
    (lambda (probe)
      (list (car probe) (cdr probe)
            (neomacs-ts-test-syntax (car probe) (cdr probe))))
    '(("#!/usr" . 1) ("/^" . 1) ("orders" . 1)
      ("/ Math" . 1) ("https" . 1) ("//v1" . 1)
      ("not-a-regexp" . 1) ("order ${" . 1) ("order.id" . 1)
      ("paid" . 1) ("block" . 1)))
   :faces
   (mapcar
    (lambda (probe)
      (list probe (neomacs-ts-test-face probe 1)))
    '("/^\\/orders\\/[0-9]+$/" "total" "https://api.example.test//v1"
      "not-a-regexp" "${" "order.id" "paid|pending" "block"))))
"###;
    let expected = expect![[
        r##"OK (:syntax (("#!/usr" 1 (:depth 0 :string nil :comment t :start (1 0))) ("/^" 1 (:depth 0 :string 47 :comment nil :start (2 14))) ("orders" 1 (:depth 0 :string 47 :comment nil :start (2 14))) ("/ Math" 1 (:depth 0 :string nil :comment nil :start nil)) ("https" 1 (:depth 0 :string 39 :comment nil :start (4 17))) ("//v1" 1 (:depth 0 :string 39 :comment nil :start (4 17))) ("not-a-regexp" 1 (:depth 0 :string nil :comment t :start (5 0))) ("order ${" 1 (:depth 0 :string 96 :comment nil :start (6 14))) ("order.id" 1 (:depth 0 :string 96 :comment nil :start (6 14))) ("paid" 1 (:depth 0 :string 47 :comment nil :start (7 7))) ("block" 1 (:depth 0 :string nil :comment t :start (8 0)))) :faces (("/^\\/orders\\/[0-9]+$/" font-lock-string-face) ("total" nil) ("https://api.example.test//v1" font-lock-string-face) ("not-a-regexp" font-lock-comment-face) ("${" font-lock-keyword-face) ("order.id" default) ("paid|pending" font-lock-string-face) ("block" font-lock-comment-face)))"##
    ]];
    ParityBatchCase::value(
        "syntax_distinguishes_hashbang_regex_division_comments_strings_and_templates",
        elisp_form,
        expected,
    )
}

fn manual_and_typed_template_conversion_preserve_interpolated_expressions() -> ParityBatchCase {
    let elisp_form = r###"
(list
 :manual
 (with-temp-buffer
   (insert "const greeting = 'Hello ${user.name}, order ${order.id}';")
   (typescript-mode)
   (goto-char (point-min))
   (search-forward "user.name")
   (let ((point-before (point)))
     (typescript-convert-to-template)
     (list :text (buffer-string) :point-before point-before :point-after (point))))
 :plain-manual
 (with-temp-buffer
   (insert "const label = \"Checkout ready\";")
   (typescript-mode)
   (search-backward "Checkout")
   (typescript-convert-to-template)
   (buffer-string))
 :typed
 (with-temp-buffer
   (typescript-mode)
   (setq-local typescript-autoconvert-to-template-flag t)
   (insert "const title = 'Order ${order.id}")
   (let ((last-command-event ?\'))
     (insert "'")
     (run-hooks 'post-self-insert-hook))
   (list :text (buffer-string) :point (point)))
 :disabled
 (with-temp-buffer
   (typescript-mode)
   (insert "const title = 'Order ${order.id}")
   (let ((last-command-event ?\'))
     (insert "'")
     (run-hooks 'post-self-insert-hook))
   (buffer-string)))
"###;
    let expected = expect![[
        r#"OK (:manual (:text "const greeting = `Hello ${user.name}, order ${order.id}`;" :point-before 36 :point-after 36) :plain-manual "const label = `Checkout ready`;" :typed (:text "const title = `Order ${order.id}`" :point 33) :disabled "const title = 'Order ${order.id}'")"#
    ]];
    ParityBatchCase::value(
        "manual_and_typed_template_conversion_preserve_interpolated_expressions",
        elisp_form,
        expected,
    )
}

fn nested_function_navigation_and_cache_invalidation_follow_real_source_edits() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert
   "export function checkout(order: Order): Quote {
  const normalize = function(value: string): string {
    function trimAndLower(input: string): string {
      return input.trim().toLowerCase();
    }
    return trimAndLower(value);
  };
  return { label: normalize(order.label) };
}

function healthcheck(): boolean {
  return true;
}
")
  (typescript-mode)
  (goto-char (point-min))
  (search-forward "return input")
  (let ((inside (neomacs-ts-test-point-state)) nested-start nested-end outer-start)
    (typescript-beginning-of-defun)
    (setq nested-start (neomacs-ts-test-point-state))
    (typescript-end-of-defun)
    (setq nested-end (neomacs-ts-test-point-state))
    (goto-char (point-min))
    (search-forward "return trimAndLower")
    (typescript-beginning-of-defun)
    (setq outer-start (neomacs-ts-test-point-state))
    (goto-char (point-min))
    (insert "function bootstrap(): void { console.log('ready'); }\n\n")
    (goto-char (point-max))
    (search-backward "return true")
    (let ((after-edit (neomacs-ts-test-point-state)))
      (typescript-beginning-of-defun)
      (list :inside inside
            :nested-start nested-start
            :nested-end nested-end
            :outer-start outer-start
            :after-edit after-edit
            :healthcheck-start (neomacs-ts-test-point-state)
            :cache-end typescript--cache-end
            :text (buffer-string)))))
"###;
    let expected = expect![[
        r#"OK (:inside (:point 172 :line 4 :column 18 :text "      return input.trim().toLowerCase();" :context toplevel) :nested-start (:point 1 :line 1 :column 0 :text "export function checkout(order: Order): Quote {" :context toplevel) :nested-end (:point 336 :line 14 :column 0 :text "" :context toplevel) :outer-start (:point 1 :line 1 :column 0 :text "export function checkout(order: Order): Quote {" :context toplevel) :after-edit (:point 375 :line 14 :column 2 :text "  return true;" :context toplevel) :healthcheck-start (:point 1 :line 1 :column 0 :text "function bootstrap(): void { console.log('ready'); }" :context toplevel) :cache-end 375 :text "function bootstrap(): void { console.log('ready'); }\n\nexport function checkout(order: Order): Quote {\n  const normalize = function(value: string): string {\n    function trimAndLower(input: string): string {\n      return input.trim().toLowerCase();\n    }\n    return trimAndLower(value);\n  };\n  return { label: normalize(order.label) };\n}\n\nfunction healthcheck(): boolean {\n  return true;\n}\n")"#
    ]];
    ParityBatchCase::value(
        "nested_function_navigation_and_cache_invalidation_follow_real_source_edits",
        elisp_form,
        expected,
    )
}

fn filling_jsdoc_and_line_comments_preserves_typescript_comment_structure() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert
   "/**
 * Recalculate checkout totals after promotions while preserving tax jurisdiction details and audit metadata for downstream invoicing.
 * @param {Order} order the order currently being priced
 * @returns {Promise<Quote>} the final persisted quote
 */
function price(order: Order): Promise<Quote> {
  // Keep this operational explanation readable for on-call engineers who need to understand why retries use the same idempotency key.
  return calculate(order);
}
")
  (typescript-mode)
  (setq-local fill-column 58)
  (goto-char (point-min))
  (search-forward "checkout totals")
  (typescript-c-fill-paragraph)
  (search-forward "operational explanation")
  (typescript-c-fill-paragraph)
  (list :text (buffer-string)
        :fill-function fill-paragraph-function
        :comment-prefix c-block-comment-prefix
        :modified (buffer-modified-p)))
"###;
    let expected = expect![[
        r#"OK (:text "/**\n * Recalculate checkout totals after promotions while\n * preserving tax jurisdiction details and audit metadata\n * for downstream invoicing.  @param {Order} order the\n * order currently being priced @returns {Promise<Quote>}\n * the final persisted quote\n */\nfunction price(order: Order): Promise<Quote> {\n  // Keep this operational explanation readable for\n  // on-call engineers who need to understand why retries\n  // use the same idempotency key.\n  return calculate(order);\n}\n" :fill-function typescript-c-fill-paragraph :comment-prefix "* " :modified t)"#
    ]];
    ParityBatchCase::value(
        "filling_jsdoc_and_line_comments_preserves_typescript_comment_structure",
        elisp_form,
        expected,
    )
}

fn compilation_mode_parses_tsc_tslint_and_angular_diagnostics_into_locations() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((root (file-name-as-directory
              (expand-file-name "typescript-diagnostics"
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (default-directory root))
  (when (file-exists-p root)
    (delete-directory root t))
  (make-directory (expand-file-name "src" root) t)
  (unwind-protect
      (progn
        (with-temp-file (expand-file-name "src/order.ts" root)
          (insert "const order: string = 2;\nconsole.log(order.missing);\n"))
        (with-temp-buffer
          (setq default-directory root)
          (compilation-mode)
          (let ((inhibit-read-only t))
            (insert
             "src/order.ts(1,23): error TS2322: Type 'number' is not assignable to type 'string'.\n"
             "src/order.ts:2:19 - error TS2339: Property 'missing' does not exist on type 'string'.\n"
             "WARNING: (semicolon) src/order.ts[2, 27]: Missing semicolon\n"
             "ERROR: src/order.ts:1:7 - forbidden assignment\n"
             "WARNING: src/order.ts:2:1 - console use\n"))
          (let ((inhibit-read-only t))
            (compilation-parse-errors
             (point-min) (point-max)
             'typescript-tsc 'typescript-tsc-pretty 'typescript-tslint
             'typescript-nglint-error 'typescript-nglint-warning))
          (list :messages (neomacs-ts-test-compilation-messages)
                :errors compilation-num-errors-found
                :warnings compilation-num-warnings-found
                :infos compilation-num-infos-found
                :registered
                (mapcar
                 (lambda (rule)
                   (cdr (assq rule compilation-error-regexp-alist-alist)))
                 '(typescript-tsc typescript-tsc-pretty typescript-tslint
                   typescript-nglint-error typescript-nglint-warning)))))
    (when (file-exists-p root)
      (delete-directory root t))))
"###;
    let expected = expect![[
        r#"OK (:messages ((:rule typescript-tsc :type 2 :file "src/order.ts" :line 1 :column 23) (:rule typescript-tsc-pretty :type 2 :file "src/order.ts" :line 2 :column 19) (:rule typescript-tslint :type 1 :file "src/order.ts" :line 2 :column 27) (:rule typescript-nglint-error :type 2 :file "src/order.ts" :line 1 :column 7) (:rule typescript-nglint-warning :type 1 :file "src/order.ts" :line 2 :column 1)) :errors 3 :warnings 2 :infos 0 :registered (("^[[:blank:]]*\\([^(\15\n)]+\\)(\\([0-9]+\\),\\([0-9]+\\)):[[:blank:]]+error [[:alnum:]]+: [^\15\n]+$" 1 2 3 2) ("^[[:blank:]]*\\([^(\15\n)]+\\):\\([0-9]+\\):\\([0-9]+\\) - [[:blank:]]*error [[:alnum:]]+: [^\15\n]+$" 1 2 3 2) ("^[[:blank:]]*\\(?:\\(?:ERROR\\|\\(WARNING\\)\\):[[:blank:]]+\\)?\\((.*)[[:blank:]]+\\)?\\([^(\15\n)]+\\)\\[\\([[:digit:]]+\\), \\([[:digit:]]+\\)\\]: .*$" 3 4 5 (1)) ("ERROR:[[:blank:]]+\\([^(\15\n)]+\\):\\([[:digit:]]+\\):\\([[:digit:]]+\\) - .*$" 1 2 3 2) ("WARNING:[[:blank:]]+\\([^(\15\n)]+\\):\\([[:digit:]]+\\):\\([[:digit:]]+\\) - .*$" 1 2 3 1)))"#
    ]];
    ParityBatchCase::value(
        "compilation_mode_parses_tsc_tslint_and_angular_diagnostics_into_locations",
        elisp_form,
        expected,
    )
}

#[test]
fn typescript_mode_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(TYPESCRIPT_MODE_MELPA_PIN, "typescript-mode.el")
            .expect("prepare revision-pinned TypeScript Mode below ./tmp")
            .with_timeout(Duration::from_secs(240))
            .with_prelude(PRELUDE),
        "typescript-mode-package-batch",
        "TypeScript Mode",
        &[
            package_and_mode_contract_configure_a_typescript_editor_buffer(),
            production_service_source_indents_classes_generics_switches_and_chains(),
            indentation_policies_reflow_switches_comma_first_lists_and_continuations(),
            production_types_and_jsdoc_receive_context_sensitive_fontification(),
            syntax_distinguishes_hashbang_regex_division_comments_strings_and_templates(),
            manual_and_typed_template_conversion_preserve_interpolated_expressions(),
            nested_function_navigation_and_cache_invalidation_follow_real_source_edits(),
            filling_jsdoc_and_line_comments_preserves_typescript_comment_structure(),
            compilation_mode_parses_tsc_tslint_and_angular_diagnostics_into_locations(),
        ],
    );
}
