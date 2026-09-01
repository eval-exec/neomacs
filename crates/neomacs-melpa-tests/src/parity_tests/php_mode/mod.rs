use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, PHP_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PHP_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const PHP_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)
(require 'imenu)
(require 'php-mode)
(require 'php)
(require 'php-project)
(require 'php-format)
(require 'php-complete)

(defun php-mode-test-face-token (token &optional occurrence)
  (save-excursion
    (goto-char (point-min))
    (dotimes (_ (or occurrence 1))
      (search-forward token))
    (let* ((start (- (point) (length token)))
           (state (syntax-ppss start)))
      (list token
            :faces
            (mapcar
             (lambda (offset)
               (get-text-property (+ start offset) 'face))
             (number-sequence 0 (1- (length token))))
            :string (and (nth 3 state) t)
            :comment (and (nth 4 state) t)))))

(defun php-mode-test-normalize-imenu (index)
  (cl-loop
   for item in index
   unless (equal (car-safe item) "*Rescan*")
   collect
   (if (imenu--subalist-p item)
       (cons (car item)
             (php-mode-test-normalize-imenu (cdr item)))
     (let ((position (cdr item)))
       (list
        (car item)
        (cond
         ((markerp position) (marker-position position))
         ((overlayp position) (overlay-start position))
         ((integerp position) position)
         (t position)))))))

(defun php-mode-test-command-insertion (source command)
  (with-temp-buffer
    (insert source)
    (php-mode)
    (goto-char (point-min))
    (search-forward "/*CURSOR*/")
    (delete-region (- (point) (length "/*CURSOR*/")) (point))
    (let ((start (point)))
      (funcall command)
      (buffer-substring-no-properties start (point)))))
"##;

fn php_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PHP_MODE_MELPA_PIN, "php-mode.el")
        .expect("prepare pinned PHP Mode source below ./tmp")
        .with_prelude(PHP_MODE_TEST_PRELUDE)
        .with_timeout(PHP_MODE_TEST_TIMEOUT)
}

fn php8_service_code_indents_namespaces_promoted_properties_match_and_chains() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (let ((php-mode-enable-backup-style-variables nil)
        (php-mode-warn-if-html-template nil))
    (insert "<?php
namespace App\\Service;

final readonly class OrderService
{
public function __construct(
private Repository $repository,
private Logger $logger,
) {
}

public function summarize(array $orders): array
{
return array_map(
fn (Order $order): string => match ($order->status) {
Status::Paid => $order
->customer()
->displayName(),
default => 'pending',
},
$orders,
);
}
}
")
    (php-mode)
    (php-enable-psr2-coding-style)
    (let ((inhibit-message t))
      (indent-region (point-min) (point-max)))
    (buffer-substring-no-properties (point-min) (point-max))))
"###;
    let expect = expect![[
        r####"OK "<?php\nnamespace App\\Service;\n\nfinal readonly class OrderService\n{\n    public function __construct(\n        private Repository $repository,\n        private Logger $logger,\n    ) {\n    }\n\n    public function summarize(array $orders): array\n    {\n        return array_map(\n            fn (Order $order): string => match ($order->status) {\n                Status::Paid => $order\n                ->customer()\n                ->displayName(),\n                default => 'pending',\n            },\n            $orders,\n        );\n    }\n}\n""####
    ]];
    ParityBatchCase::value(
        "php8_service_code_indents_namespaces_promoted_properties_match_and_chains",
        elisp_form,
        expect,
    )
}

fn php85_application_tokens_receive_precise_keyword_type_variable_string_and_operator_faces()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert "<?php
#[Route('/orders')]
enum Status: string
{
    case Paid = 'paid';
}

readonly class Order
{
    public function __construct(public ?Customer $customer) {}

    public function slug(): string
    {
        // normalize for routing
        $slug = $this->name |> trim(...) |> strtolower(...);
        return \"Order {$this->id}\";
    }
}
")
  (php-mode)
  (font-lock-ensure)
  (mapcar
   (lambda (probe)
     (apply #'php-mode-test-face-token probe))
   '(("Route")
     ("enum")
     ("Status")
     ("string")
     ("readonly")
     ("public")
     ("?Customer")
     ("$customer")
     ("normalize")
     ("|>" 1)
     ("|>" 2)
     ("trim")
     ("strtolower")
     ("$this->id"))))
"###;
    let expect = expect![[
        r####"OK (("Route" :faces (php-function-call-traditional php-function-call-traditional php-function-call-traditional php-function-call-traditional php-function-call-traditional) :string nil :comment nil) ("enum" :faces (php-keyword php-keyword php-keyword php-keyword) :string nil :comment nil) ("Status" :faces (font-lock-type-face font-lock-type-face font-lock-type-face font-lock-type-face font-lock-type-face font-lock-type-face) :string nil :comment nil) ("string" :faces (font-lock-type-face font-lock-type-face font-lock-type-face font-lock-type-face font-lock-type-face font-lock-type-face) :string nil :comment nil) ("readonly" :faces (php-keyword php-keyword php-keyword php-keyword php-keyword php-keyword php-keyword php-keyword) :string nil :comment nil) ("public" :faces (php-keyword php-keyword php-keyword php-keyword php-keyword php-keyword) :string nil :comment nil) ("?Customer" :faces (font-lock-type-face font-lock-type-face font-lock-type-face font-lock-type-face font-lock-type-face font-lock-type-face font-lock-type-face font-lock-type-face font-lock-type-face) :string nil :comment nil) ("$customer" :faces (php-variable-sigil php-variable-name php-variable-name php-variable-name php-variable-name php-variable-name php-variable-name php-variable-name php-variable-name) :string nil :comment nil) ("normalize" :faces (font-lock-comment-face font-lock-comment-face font-lock-comment-face font-lock-comment-face font-lock-comment-face font-lock-comment-face font-lock-comment-face font-lock-comment-face font-lock-comment-face) :string nil :comment t) ("|>" :faces (php-pipe-op php-pipe-op) :string nil :comment nil) ("|>" :faces (php-pipe-op php-pipe-op) :string nil :comment nil) ("trim" :faces (php-function-call-traditional php-function-call-traditional php-function-call-traditional php-function-call-traditional) :string nil :comment nil) ("strtolower" :faces (php-function-call-traditional php-function-call-traditional php-function-call-traditional php-function-call-traditional php-function-call-traditional php-function-call-traditional php-function-call-traditional php-function-call-traditional php-function-call-traditional php-function-call-traditional) :string nil :comment nil) ("$this->id" :faces (php-variable-name php-variable-name php-variable-name php-variable-name php-variable-name php-variable-name php-variable-name php-variable-name php-variable-name) :string t :comment nil))"####
    ]];
    ParityBatchCase::value(
        "php85_application_tokens_receive_precise_keyword_type_variable_string_and_operator_faces",
        elisp_form,
        expect,
    )
}

fn heredoc_comments_and_method_chains_preserve_syntax_state_and_backward_tokens() -> ParityBatchCase
{
    let elisp_form = r###"
(with-temp-buffer
  (insert "<?php
$query = <<<SQL
SELECT * FROM users WHERE name = '{$name}'
SQL;

// a quoted \"value\" remains a comment
$result = $client
    ->withHeader('X-Trace', $trace)
    ->send($query);
")
  (php-mode)
  (font-lock-ensure)
  (let ((states
         (mapcar
          (lambda (token)
            (save-excursion
              (goto-char (point-min))
              (search-forward token)
              (let ((state (syntax-ppss (- (point) (length token)))))
                (list token
                      :string (and (nth 3 state) t)
                      :comment (and (nth 4 state) t)
                      :face
                      (get-text-property
                       (- (point) (length token))
                       'face)))))
          '("SELECT" "$name" "quoted" "$result" "X-Trace"))))
    (goto-char (point-min))
    (search-forward "send($query")
    (list
     :states states
     :tokens (nreverse (php-leading-tokens 8))
     :pattern (php-get-pattern))))
"###;
    let expect = expect![[
        r####"OK (:states (("SELECT" :string t :comment nil :face php-string) ("$name" :string t :comment nil :face php-string) ("quoted" :string nil :comment t :face nil) ("$result" :string nil :comment nil :face font-lock-comment-face) ("X-Trace" :string t :comment nil :face php-method-call-traditional)) :tokens ("'X-Trace'" "," "$trace" ")" "->" "send" "(" "$query") :pattern "$query")"####
    ]];
    ParityBatchCase::value(
        "heredoc_comments_and_method_chains_preserve_syntax_state_and_backward_tokens",
        elisp_form,
        expect,
    )
}

fn imenu_builds_navigable_namespace_type_method_property_constant_and_function_groups()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert "<?php
namespace App\\Domain;

interface Identifiable
{
    public function id(): string;
}

trait Timestamps
{
    public function touchedAt(): DateTimeImmutable {}
}

enum State: string
{
    case Open = 'open';
}

final class Ticket implements Identifiable
{
    public const PREFIX = 'T';
    private string $id;

    public function __construct(string $id)
    {
        $this->id = $id;
    }

    public static function fromId(string $id): self
    {
        return new self($id);
    }
}

function ticket_label(Ticket $ticket): string
{
    return Ticket::PREFIX . $ticket->id();
}
")
  (php-mode)
  (font-lock-ensure)
  (php-mode-test-normalize-imenu (imenu--make-index-alist t)))
"###;
    let expect = expect![[
        r####"OK (("Namespace" ("App\\Domain" 7)) ("Classes" ("interface Identifiable" 30) ("trait Timestamps" 92) ("enum State" 168) ("final class Ticket" 216)) ("Functions" ("function ticket_label(Ticket $ticket): string" 496)) ("Constants" ("public const PREFIX = 'T';" 261)) ("Methods" ("public function id(): string;" 55) ("public function touchedAt(): DateTimeImmutable {}" 111) ("public function __construct(string $id)" 317) ("public static function fromId(string $id): self" 399) ("function ticket_label(Ticket $ticket): string" 496)))"####
    ]];
    ParityBatchCase::value(
        "imenu_builds_navigable_namespace_type_method_property_constant_and_function_groups",
        elisp_form,
        expect,
    )
}

fn namespace_class_method_context_supports_insertion_fqsen_copy_and_defun_navigation()
-> ParityBatchCase {
    let elisp_form = r###"
(let ((source "<?php
namespace App\\Domain;

final class TicketRepository
{
    public function find(string $id): ?Ticket
    {
        /*CURSOR*/
        return $this->storage[$id] ?? null;
    }
}
"))
  (with-temp-buffer
    (insert source)
    (php-mode)
    (goto-char (point-min))
    (search-forward "return $this")
    (let ((context
           (list
            :namespace
            (php-get-current-element php--re-namespace-pattern)
            :class
            (php-get-current-element php--re-classlike-pattern)
            :function
            (php-get-current-element php-beginning-of-defun-regexp)))
          (kill-ring nil))
      (php-copyit-fqsen)
      (goto-char (point-max))
      (let ((moved (php-beginning-of-defun))
            begin-line
            signature
            ended
            end-line)
        (setq begin-line (line-number-at-pos)
              signature
              (buffer-substring-no-properties
               (line-beginning-position) (line-end-position))
              ended (php-end-of-defun)
              end-line (line-number-at-pos))
        (list
         :context context
         :class-insertion
         (php-mode-test-command-insertion source #'php-current-class)
         :namespace-insertion
         (php-mode-test-command-insertion source #'php-current-namespace)
         :fqsen (car kill-ring)
         :navigation
         (list :moved moved
               :begin-line begin-line
               :signature signature
               :ended ended
               :end-line end-line))))))
"###;
    let expect = expect![[
        r####"OK (:context (:namespace "App\\Domain" :class "TicketRepository" :function "find") :class-insertion "TicketRepository::" :namespace-insertion "App\\Domain\\" :fqsen "App\\Domain\\TicketRepository::find()" :navigation (:moved t :begin-line 6 :signature "    public function find(string $id): ?Ticket" :ended nil :end-line 11))"####
    ]];
    ParityBatchCase::value(
        "namespace_class_method_context_supports_insertion_fqsen_copy_and_defun_navigation",
        elisp_form,
        expect,
    )
}

fn completion_offers_builtin_functions_for_calls_but_not_object_member_contexts() -> ParityBatchCase
{
    let elisp_form = r###"
(let* ((php-complete-function-modules '(core))
       (php-complete--functions-cache (make-hash-table :test #'equal))
       (core-functions
        (sort (copy-sequence (cdr (assq 'core php-defs-functions-alist)))
              #'string<)))
  (puthash php-complete-function-modules
           core-functions
           php-complete--functions-cache)
  (cl-labels
      ((probe
        (source)
        (with-temp-buffer
          (insert source)
          (php-mode)
          (let* ((capf (php-complete-complete-function))
                 (beg (nth 0 capf))
                 (end (nth 1 capf))
                 (table (nth 2 capf))
                 (prefix (buffer-substring-no-properties beg end))
                 (candidates (and table (all-completions prefix table)))
                 (annotation (plist-get (nthcdr 3 capf)
                                        :annotation-function)))
            (list
             :prefix prefix
             :table (and table t)
             :candidates candidates
             :category
             (and table
                  (completion-metadata-get
                   (completion-metadata prefix table nil)
                   'category))
             :annotation
             (and annotation (funcall annotation "str_replace")))))))
    (let ((functions-1 (php-complete--functions))
          (functions-2 (php-complete--functions)))
      (list
       :plain-call (probe "<?php\n$result = str_repl")
       :member-call (probe "<?php\n$result = $service->str_repl")
       :cache
       (list :same-object (eq functions-1 functions-2)
             :count (length functions-1)
             :sample (seq-filter
                      (lambda (name)
                        (string-prefix-p "str_repl" name))
                      functions-1))))))
"###;
    let expect = expect![[
        r####"OK (:plain-call (:prefix "str_repl" :table t :candidates ("str_replace") :category cape-keyword :annotation " PHP functions") :member-call (:prefix "str_repl" :table nil :candidates nil :category nil :annotation " PHP functions") :cache (:same-object t :count 774 :sample ("str_replace")))"####
    ]];
    ParityBatchCase::value(
        "completion_offers_builtin_functions_for_calls_but_not_object_member_contexts",
        elisp_form,
        expect,
    )
}

fn template_routing_handles_plain_php_html_fallback_blade_fallback_and_cache_invalidation()
-> ParityBatchCase {
    let elisp_form = r###"
(cl-labels
    ((derive
      (filename contents)
      (with-temp-buffer
        (setq buffer-file-name
              (expand-file-name filename
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
        (insert contents)
        (php-derivation-major-mode))))
  (let ((php-project-php-file-as-template 'auto)
        (php-default-major-mode 'php-mode)
        (php-html-template-major-mode 'php-mode-test-absent-web-mode)
        (php-blade-template-major-mode 'php-mode-test-absent-web-mode)
        (php-blade-template-major-mode-fallback
         '(php-mode-test-absent-web-mode html-mode))
        (php-template-mode-alist
         '(("\\.blade\\.php\\'" . php-mode-test-absent-web-mode)
           ("\\.tpl\\.php\\'" . html-mode)))
        warnings)
    (cl-letf (((symbol-function 'warn)
               (lambda (format-string &rest args)
                 (push (apply #'format format-string args) warnings))))
      (let ((routes
             (list
              :plain (derive "Controller.php" "<?php\nreturn 42;\n")
              :html-in-php
              (derive "page.php" "<div><?php echo 'hi'; ?></div>\n")
              :blade
              (derive "welcome.blade.php" "@extends('layout')\n")
              :explicit-template
              (derive "receipt.tpl.php" "<article>Paid</article>\n"))))
        (with-temp-buffer
          (insert "<div>first</div>\n")
          (let* ((first (php-buffer-has-html-tag))
                 (cache-1 php--buffer-has-html-tag-cache)
                 (second (php-buffer-has-html-tag))
                 (cache-2 php--buffer-has-html-tag-cache))
            (erase-buffer)
            (insert "<?php\nreturn 1;\n")
            (let ((third (php-buffer-has-html-tag))
                  (cache-3 php--buffer-has-html-tag-cache))
              (list
               :routes routes
               :warnings
               (mapcar
                (lambda (warning)
                  (replace-regexp-in-string (string 96) "'" warning))
                (nreverse warnings))
               :cache
               (list :first (and first t)
                     :second (and second t)
                     :same-cache-object (eq cache-1 cache-2)
                     :after-edit (and third t)
                     :tick-changed
                     (not (eql (car cache-2) (car cache-3))))))))))))
"###;
    let expect = expect![[
        r####"OK (:routes (:plain php-mode :html-in-php php-mode :blade html-mode :explicit-template html-mode) :warnings ("'php-mode-test-absent-web-mode' is not available for this Blade template; using 'html-mode' instead.\nInstall the 'web-mode' package for full Blade support.") :cache (:first t :second t :same-cache-object t :after-edit nil :tick-changed t))"####
    ]];
    ParityBatchCase::value(
        "template_routing_handles_plain_php_html_fallback_blade_fallback_and_cache_invalidation",
        elisp_form,
        expect,
    )
}

fn composer_project_detection_resolves_bootstraps_project_adapter_and_local_formatter()
-> ParityBatchCase {
    let elisp_form = r###"
(let* ((sandbox (file-name-as-directory
                 (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (root (expand-file-name "shop" sandbox))
       (source (expand-file-name "src/Domain" root))
       (vendor-bin (expand-file-name "vendor/bin" root))
       (default-directory source))
  (unwind-protect
      (progn
        (make-directory source t)
        (make-directory vendor-bin t)
        (make-directory (expand-file-name "config" root) t)
        (dolist (file '("composer.json"
                        ".php-cs-fixer.dist.php"
                        "vendor/autoload.php"
                        "vendor/bin/php-cs-fixer"
                        "config/bootstrap.php"))
          (with-temp-file (expand-file-name file root)
            (insert "{}\n")))
        (let ((php-project-root 'auto)
              (php-project-bootstrap-scripts
               '(composer (root . "config/bootstrap.php")))
              (php-format-command 'auto)
              (php-format-command-dir "vendor/bin"))
          (let* ((detected (php-project-get-root-dir))
                 (bootstraps (php-project-get-bootstrap-scripts))
                 (adapter (php-project-project-find-function source))
                 (command
                  (let ((default-directory detected))
                    (php-format--get-command-args))))
            (list
             :root (file-relative-name detected sandbox)
             :bootstraps
             (mapcar
              (lambda (path) (file-relative-name path sandbox))
              bootstraps)
             :project-adapter
             (list (car adapter)
                   (file-relative-name (cdr adapter) sandbox))
             :formatter
             (cons
              (file-relative-name (car command) sandbox)
              (cdr command))
             :formatter-selection php-format-command))))
    (delete-directory root t)))
"###;
    let expect = expect![[
        r####"OK (:root "shop/" :bootstraps ("shop/vendor/autoload.php" "shop/config/bootstrap.php") :project-adapter (transient "shop/") :formatter ("shop/vendor/bin/php-cs-fixer" "fix" "--show-progress=none") :formatter-selection php-cs-fixer)"####
    ]];
    ParityBatchCase::value(
        "composer_project_detection_resolves_bootstraps_project_adapter_and_local_formatter",
        elisp_form,
        expect,
    )
}

#[test]
fn php_mode_package_batch() {
    let cases = vec![
        php8_service_code_indents_namespaces_promoted_properties_match_and_chains(),
        php85_application_tokens_receive_precise_keyword_type_variable_string_and_operator_faces(),
        heredoc_comments_and_method_chains_preserve_syntax_state_and_backward_tokens(),
        imenu_builds_navigable_namespace_type_method_property_constant_and_function_groups(),
        namespace_class_method_context_supports_insertion_fqsen_copy_and_defun_navigation(),
        completion_offers_builtin_functions_for_calls_but_not_object_member_contexts(),
        template_routing_handles_plain_php_html_fallback_blade_fallback_and_cache_invalidation(),
        composer_project_detection_resolves_bootstraps_project_adapter_and_local_formatter(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed PHP Mode parity test");
    assert_oracle_batch_cases(php_mode_oracle(), test_name, "php_mode_parity", &cases);
}
