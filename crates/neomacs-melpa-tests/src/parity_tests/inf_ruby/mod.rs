use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, INF_RUBY_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const INF_RUBY_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const INF_RUBY_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'inf-ruby)

(unless (fboundp 'ruby-compilation-mode)
  (define-derived-mode ruby-compilation-mode compilation-mode
    "Ruby-Compilation"))

(defun inf-ruby-test-write-file (file contents)
  (make-directory (file-name-directory file) t)
  (with-temp-file file
    (insert contents)))

(defun inf-ruby-test-completion-shape (expression)
  (with-temp-buffer
    (insert expression)
    (goto-char (point-max))
    (let ((expr-bounds (inf-ruby-completion-bounds-of-expr-at-point))
          (prefix-bounds (inf-ruby-completion-bounds-of-prefix)))
      (list
       expression
       (list (car expr-bounds) (cdr expr-bounds)
             (buffer-substring (car expr-bounds) (cdr expr-bounds)))
       (list (car prefix-bounds) (cdr prefix-bounds)
             (buffer-substring (car prefix-bounds) (cdr prefix-bounds)))
       (inf-ruby-completion-target-at-point)))))
"##;

fn inf_ruby_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(INF_RUBY_MELPA_PIN, "inf-ruby.el")
        .expect("prepare pinned inf-ruby source below ./tmp")
        .with_prelude(INF_RUBY_TEST_PRELUDE)
        .with_timeout(INF_RUBY_TEST_TIMEOUT)
}

fn repl_mode_configures_comint_and_tracks_real_prompt_transitions() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (inf-ruby-mode)
  (let ((mode-state
         (list
          :mode major-mode
          :derived (and (derived-mode-p 'comint-mode) t)
          :keys
          (mapcar
           (lambda (key)
             (list key (lookup-key inf-ruby-mode-map (kbd key))))
           '("C-c C-l" "C-c C-k" "C-x C-e" "TAB" "C-x C-q" "C-c C-z"))
          :comments (list comment-start comment-end comment-start-skip)
          :parser (list parse-sexp-ignore-comments
                        parse-sexp-lookup-properties)
          :hooks
          (list
           (and (memq 'inf-ruby-output-filter
                      comint-output-filter-functions) t)
           (and (memq 'inf-ruby-completion-at-point
                      completion-at-point-functions) t)
           compilation-shell-minor-mode)
          :old-input comint-get-old-input
          :errors (list (= (length compilation-error-regexp-alist) 2)
                        (equal compilation-error-regexp-alist
                               inf-ruby-error-regexp-alist))))
        (prompt-state
         (mapcar
          (lambda (output)
            (inf-ruby-output-filter output)
            (list output
                  inf-ruby-last-prompt
                  (and inf-ruby-at-top-level-prompt-p t)))
          '("booting IRB\nirb(main):001:0> "
            "irb(main):002:1* "
            "[1] pry(main)> "
            "(byebug) "))))
    (list
     :mode mode-state
     :prompts prompt-state
     :transcript
     (inf-ruby-remove-in-string
      (concat
       "irb(main):001:0> order = Order.find(417)\n"
       "irb(main):002:1*   order.items.map(&:sku)\n"
       "irb(main):003:0> order.capture!")
      inf-ruby-prompt-pattern))))
"##;
    let expect = expect![[
        r##"OK (:mode (:mode inf-ruby-mode :derived t :keys (("C-c C-l" ruby-load-file) ("C-c C-k" ruby-load-current-file) ("C-x C-e" ruby-send-last-stmt) ("TAB" completion-at-point) ("C-x C-q" inf-ruby-maybe-switch-to-compilation) ("C-c C-z" ruby-switch-to-last-ruby-buffer)) :comments ("# " "" "#+ *") :parser (t t) :hooks (t t t) :old-input inf-ruby-get-old-input :errors (t t)) :prompts (("booting IRB\nirb(main):001:0> " "irb(main):001:0> " t) ("irb(main):002:1* " "irb(main):002:1* " nil) ("[1] pry(main)> " "[1] pry(main)> " t) ("(byebug) " "(byebug) " t)) :transcript "order = Order.find(417)\norder.items.map(&:sku)\norder.capture!")"##
    ]];
    ParityBatchCase::value(
        "repl_mode_configures_comint_and_tracks_real_prompt_transitions",
        elisp_form,
        expect,
    )
}

fn source_dispatch_preserves_definition_context_file_lines_and_wire_escaping() -> ParityBatchCase {
    let elisp_form = r##"
(let ((source (generate-new-buffer " *inf-ruby-source*"))
      (output (generate-new-buffer " *inf-ruby-output*"))
      sent print-events marker)
  (unwind-protect
      (progn
        (with-current-buffer output
          (setq marker (copy-marker (point-max))))
        (with-current-buffer source
          (setq buffer-file-name "/workspace/checkout/app/models/order.rb")
          (insert
           "module Checkout\n"
           "  class Order\n"
           "    def total\n"
           "      items.sum { |item| item.price * item.quantity }\n"
           "    end\n"
           "  end\n"
           "end\n\n"
           "Order.find(417).capture!\n")
          (ruby-mode)
          (cl-letf (((symbol-function 'inf-ruby-proc) (lambda () 'ruby-process))
                    ((symbol-function 'process-buffer) (lambda (_proc) output))
                    ((symbol-function 'process-mark) (lambda (_proc) marker))
                    ((symbol-function 'process-tty-name) (lambda (_proc) nil))
                    ((symbol-function 'comint-send-string)
                     (lambda (_proc string) (push string sent)))
                    ((symbol-function 'ruby-print-result)
                     (lambda (&optional print) (push print print-events))))
            (goto-char (point-min))
            (search-forward "item.price")
            (ruby-send-definition)
            (goto-char (point-min))
            (search-forward "Order.find")
            (beginning-of-line)
            (ruby-send-line)))
        (list
         :sent (nreverse sent)
         :print-events (nreverse print-events)
         :output-buffer
         (with-current-buffer output
           (list (buffer-string) (marker-position marker)))))
    (when (buffer-live-p source) (kill-buffer source))
    (when (buffer-live-p output) (kill-buffer output))))
"##;
    let expect = expect![[
        r##"OK (:sent ("eval(\"module Checkout\\n  class Order\\n    def total\\n      items.sum { |item| item.price * item.quantity }\\n    end\\nend\\nend\\n\", (defined?(IRB.conf) && IRB.conf[:MAIN_CONTEXT] && IRB.conf[:MAIN_CONTEXT].workspace.binding) || (defined?(Pry) && Pry.toplevel_binding), \"/workspace/checkout/app/models/order.rb\", 1)\n" "eval(\"Order.find(417).capture!\", (defined?(IRB.conf) && IRB.conf[:MAIN_CONTEXT] && IRB.conf[:MAIN_CONTEXT].workspace.binding) || (defined?(Pry) && Pry.toplevel_binding), \"/workspace/checkout/app/models/order.rb\", 9)\n") :print-events (nil nil) :output-buffer ("\n\n" 3))"##
    ]];
    ParityBatchCase::value(
        "source_dispatch_preserves_definition_context_file_lines_and_wire_escaping",
        elisp_form,
        expect,
    )
}

fn completion_understands_chained_ruby_receivers_and_serves_capf_candidates() -> ParityBatchCase {
    let elisp_form = r##"
(let ((shapes
       (mapcar
        #'inf-ruby-test-completion-shape
        '("order.customer.na"
          "Checkout::Order.fi"
          "payload[:customer].em")))
      top-level nested)
  (with-temp-buffer
    (insert "order.customer.na")
    (goto-char (point-max))
    (cl-letf (((symbol-function 'inf-ruby-completions)
               (lambda (_prefix)
                 '("name" "namespace" "status"))))
      (let* ((inf-ruby-at-top-level-prompt-p t)
             (capf (inf-ruby-completion-at-point)))
        (setq top-level
              (list (car capf)
                    (cadr capf)
                    (all-completions "na" (nth 2 capf)))))
      (let* ((inf-ruby-at-top-level-prompt-p nil)
             (capf (inf-ruby-completion-at-point)))
        (setq nested (list (car capf) (cadr capf) (nth 2 capf))))))
  (list :shapes shapes :top-level top-level :nested nested))
"##;
    let expect = expect![[
        r##"OK (:shapes (("order.customer.na" (1 18 "order.customer.na") (16 18 "na") "order.customer.") ("Checkout::Order.fi" (1 19 "Checkout::Order.fi") (17 19 "fi") "Checkout::Order.") ("payload[:customer].em" (1 22 "payload[:customer].em") (20 22 "em") "payload[:customer].")) :top-level (16 18 ("name" "namespace")) :nested (16 18 nil))"##
    ]];
    ParityBatchCase::value(
        "completion_understands_chained_ruby_receivers_and_serves_capf_candidates",
        elisp_form,
        expect,
    )
}

fn project_discovery_builds_exact_rails_gem_and_hanami_console_launches() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (file-name-as-directory
              (expand-file-name "inf-ruby-projects"
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (rails (expand-file-name "checkout/" root))
       (rails-nested (expand-file-name "app/services/" rails))
       (gem (expand-file-name "billing-client/" root))
       (hanami (expand-file-name "warehouse/" root))
       (inf-ruby-console-environment "test")
       launches)
  (make-directory rails-nested t)
  (inf-ruby-test-write-file (expand-file-name "bin/rails" rails) "#!/usr/bin/env ruby\n")
  (dolist (env '("development" "production" "test"))
    (inf-ruby-test-write-file
     (expand-file-name (format "config/environments/%s.rb" env) rails)
     (format "Rails.application.configure { config.x.env = :%s }\n" env)))
  (inf-ruby-test-write-file
   (expand-file-name "billing-client.gemspec" gem)
   "Gem::Specification.new { |spec| spec.name = 'billing-client' }\n")
  (inf-ruby-test-write-file (expand-file-name "Gemfile" gem) "gemspec\n")
  (inf-ruby-test-write-file
   (expand-file-name "lib/billing/client.rb" gem)
   "module Billing; class Client; end; end\n")
  (inf-ruby-test-write-file
   (expand-file-name "config.ru" hanami)
   "require 'hanami/boot'\nrun Hanami.app\n")
  (inf-ruby-test-write-file (expand-file-name "Gemfile" hanami) "gem 'hanami'\n")
  (cl-letf (((symbol-function 'inf-ruby--irb-needs-nomultiline-p)
             (lambda (&optional _with-bundler) t))
            ((symbol-function 'inf-ruby-console-run)
             (lambda (command name)
               (push
                (list (file-relative-name default-directory root)
                      command name (getenv "HANAMI_ENV"))
                launches))))
    (let ((default-directory rails-nested))
      (inf-ruby-console-auto))
    (inf-ruby-console-gem gem)
    (let ((inf-ruby-console-environment "production"))
      (inf-ruby-console-hanami hanami)))
  (list
   :detected
   (mapcar
    (lambda (dir)
      (let ((default-directory dir))
        (inf-ruby-console-match dir)))
    (list rails gem hanami))
   :rails-root
   (file-relative-name
    (locate-dominating-file rails-nested #'inf-ruby-console-match)
    root)
   :rails-envs
   (let ((default-directory rails))
     (inf-ruby-console-rails-envs))
   :launches (nreverse launches)))
"##;
    let expect = expect![[
        r##"OK (:detected (rails gem hanami) :rails-root "checkout/" :rails-envs ("development" "production" "test") :launches (("checkout/" "bin/rails console -e test -- --nomultiline --noreadline" "rails" nil) ("billing-client/" "bundle exec irb -I lib --nomultiline -r billing/client --prompt default --noreadline -r irb/completion" "gem" nil) ("warehouse/" "bundle exec hanami console" "hanami" "production")))"##
    ]];
    ParityBatchCase::value(
        "project_discovery_builds_exact_rails_gem_and_hanami_console_launches",
        elisp_form,
        expect,
    )
}

fn debugger_breakpoint_round_trip_preserves_compilation_session_state() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (ruby-compilation-mode)
  (setq-local compilation-arguments '("bundle exec rspec" nil))
  (setq-local compilation-error-regexp-alist '(ruby-test-error))
  (let ((inhibit-read-only t))
    (insert "Failure in checkout capture\n(byebug) order\n"))
  (goto-char (point-min))
  (forward-line 1)
  (end-of-line)
  (let ((entered (inf-ruby-auto-enter))
        scheduled
        entered-state)
    (inf-ruby-output-filter "(byebug) ")
    (setq entered-state
          (list
           :matched (and entered t)
           :mode major-mode
           :original inf-ruby-orig-compilation-mode
           :arguments (copy-tree compilation-arguments)
           :input-filter
           (and (memq 'inf-ruby-auto-exit comint-input-filter-functions) t)
           :prompt (list inf-ruby-last-prompt
                         (and inf-ruby-at-top-level-prompt-p t))))
    (cl-letf (((symbol-function 'run-with-idle-timer)
               (lambda (&rest args) (setq scheduled args) 'test-timer)))
      (inf-ruby-auto-exit "quit\n"))
    (inf-ruby-switch-to-compilation)
    (list
     :entered entered-state
     :scheduled scheduled
     :restored
     (list major-mode
           (copy-tree compilation-arguments)
           compilation-error-regexp-alist))))
"##;
    let expect = expect![[
        r##"OK (:entered (:matched t :mode inf-ruby-mode :original ruby-compilation-mode :arguments ("bundle exec rspec" nil) :input-filter t :prompt ("(byebug) " t)) :scheduled (0 nil inf-ruby-maybe-switch-to-compilation) :restored (ruby-compilation-mode ("bundle exec rspec" nil) (ruby-test-error)))"##
    ]];
    ParityBatchCase::value(
        "debugger_breakpoint_round_trip_preserves_compilation_session_state",
        elisp_form,
        expect,
    )
}

#[test]
fn inf_ruby_package_batch() {
    let cases = vec![
        repl_mode_configures_comint_and_tracks_real_prompt_transitions(),
        source_dispatch_preserves_definition_context_file_lines_and_wire_escaping(),
        completion_understands_chained_ruby_receivers_and_serves_capf_candidates(),
        project_discovery_builds_exact_rails_gem_and_hanami_console_launches(),
        debugger_breakpoint_round_trip_preserves_compilation_session_state(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed inf-ruby parity test");
    assert_oracle_batch_cases(inf_ruby_oracle(), test_name, "inf_ruby_parity", &cases);
}
