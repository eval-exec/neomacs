use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, ELIXIR_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const ELIXIR_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn elixir_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ELIXIR_MODE_MELPA_PIN, "elixir-mode.el")
        .expect("prepare pinned Elixir Mode source below ./tmp")
        .with_prelude(r##"(defvar byte-compile-current-file nil)"##)
        .with_timeout(ELIXIR_MODE_TEST_TIMEOUT)
}

fn checkout_pipeline_and_error_branches_are_indented_as_real_elixir() -> ParityBatchCase {
    ParityBatchCase::value(
        "checkout_pipeline_and_error_branches_are_indented_as_real_elixir",
        r##"
(with-temp-buffer
  (insert
   "defmodule Checkout do\n"
   "def total(lines) do\n"
   "lines\n"
   "|> Enum.reject(fn line ->\n"
   "line.voided?\n"
   "end)\n"
   "|> Enum.map(& &1.price)\n"
   "|> Enum.sum()\n"
   "end\n\n"
   "def charge(order) do\n"
   "with {:ok, total} <- total(order.lines),\n"
   "true <- total > 0 do\n"
   "{:ok, Payments.capture(total)}\n"
   "else\n"
   "false -> {:error, :empty}\n"
   "{:error, reason} -> {:error, reason}\n"
   "end\n"
   "end\n"
   "end\n")
  (elixir-mode)
  (setq-local indent-tabs-mode nil)
  (indent-region (point-min) (point-max))
  (list :mode major-mode
        :indent-function indent-line-function
        :source (buffer-string)))
"##,
        expect![[
            r##"OK (:mode elixir-mode :indent-function smie-indent-line :source "defmodule Checkout do\n  def total(lines) do\n    lines\n    |> Enum.reject(fn line ->\n      line.voided?\n    end)\n    |> Enum.map(& &1.price)\n    |> Enum.sum()\n  end\n\n  def charge(order) do\n    with {:ok, total} <- total(order.lines),\n         true <- total > 0 do\n      {:ok, Payments.capture(total)}\n    else\n      false -> {:error, :empty}\n    {:error, reason} -> {:error, reason}\n    end\n  end\nend\n")"##
        ]],
    )
}

fn production_module_fontification_distinguishes_elixir_semantic_roles() -> ParityBatchCase {
    ParityBatchCase::value(
        "production_module_fontification_distinguishes_elixir_semantic_roles",
        r##"
(with-temp-buffer
  (insert
   "defmodule Checkout do\n"
   "  @moduledoc \"\"\"\n"
   "  Checkout pricing.\n"
   "  \"\"\"\n"
   "  @tax_rate 20\n\n"
   "  def total(customer, lines) do\n"
   "    subtotal = Enum.sum(lines)\n"
   "    %{customer: customer.name,\n"
   "      total: subtotal * @tax_rate,\n"
   "      status: :ready}\n"
   "    |> inspect(label: \"order #{customer.id}\")\n"
   "  end\n"
   "end\n"
   "# audit complete\n")
  (elixir-mode)
  (font-lock-fontify-buffer)
  (cl-labels
      ((probe (label needle occurrence)
         (goto-char (point-min))
         (dotimes (_ occurrence)
           (search-forward needle))
         (let* ((start (match-beginning 0))
                (end (match-end 0))
                (state (syntax-ppss (min (1+ start) (point-max))))
                (context
                 (cond ((elixir-ppss-comment-depth state) 'comment)
                       ((elixir-ppss-string-terminator state) 'string)
                       (t 'code))))
           (list label
                 (buffer-substring-no-properties start end)
                 (get-text-property start 'face)
                 context))))
    (mapcar
     (lambda (fixture)
       (apply #'probe fixture))
     '((declaration "defmodule" 1)
       (module-name "Checkout" 1)
       (module-doc-attribute "@moduledoc" 1)
       (module-doc-text "Checkout pricing." 1)
       (custom-attribute "@tax_rate" 1)
       (integer "20" 1)
       (function-name "total" 1)
       (assigned-variable "subtotal" 1)
       (map-key "customer:" 1)
       (atom ":ready" 1)
       (pipeline "|>" 1)
       (string-body "order #" 1)
       (interpolated-variable "customer.id" 1)
       (comment "# audit complete" 1)))))
"##,
        expect![[
            r##"OK ((declaration "defmodule" font-lock-keyword-face code) (module-name "Checkout" font-lock-type-face code) (module-doc-attribute "@moduledoc" elixir-attribute-face code) (module-doc-text "Checkout pricing." font-lock-doc-face string) (custom-attribute "@tax_rate" elixir-attribute-face code) (integer "20" elixir-number-face code) (function-name "total" font-lock-function-name-face code) (assigned-variable "subtotal" font-lock-variable-name-face code) (map-key "customer:" elixir-atom-face code) (atom ":ready" elixir-atom-face code) (pipeline "|>" font-lock-keyword-face code) (string-body "order #" font-lock-string-face string) (interpolated-variable "customer.id" font-lock-variable-name-face string) (comment "# audit complete" font-lock-comment-delimiter-face comment))"##
        ]],
    )
}

fn imenu_and_defun_navigation_find_real_modules_functions_macros_guards_and_tests()
-> ParityBatchCase {
    ParityBatchCase::value(
        "imenu_and_defun_navigation_find_real_modules_functions_macros_guards_and_tests",
        r##"
(with-temp-buffer
  (insert
   "defmodule Shop.Checkout do\n"
   "  def total(lines) do\n"
   "    Enum.sum(lines)\n"
   "  end\n\n"
   "  def charge(order) do\n"
   "    Repo.fetch(order)\n"
   "  end\n\n"
   "  defp normalize(line) do\n"
   "    line\n"
   "  end\n\n"
   "  defmacro measured(do: block) do\n"
   "    block\n"
   "  end\n\n"
   "  defguard is_billable(order) when order.total > 0\n"
   "  defdelegate capture(order), to: Payments\n"
   "  def available?, do: true\n"
   "end\n\n"
   "defmodule Shop.CheckoutTest do\n"
   "  use ExUnit.Case\n"
   "  test \"charges valid order\" do\n"
   "    assert {:ok, _} = Shop.Checkout.charge(%{})\n"
   "  end\n"
   "end\n")
  (elixir-mode)
  (require 'imenu)
  (let* ((charge-probe
          (save-excursion
            (goto-char (point-min))
            (search-forward "Repo.fetch")
            (point)))
         (inline-probe
          (save-excursion
            (goto-char (point-min))
            (search-forward "available?")
            (point)))
         (navigation
          (mapcar
           (lambda (probe)
             (list
              :probe-line (line-number-at-pos probe)
              :begin
              (save-excursion
                (goto-char probe)
                (beginning-of-defun)
                (list (line-number-at-pos)
                      (buffer-substring-no-properties
                       (line-beginning-position)
                       (line-end-position))))
              :end
              (save-excursion
                (goto-char probe)
                (end-of-defun)
                (list (line-number-at-pos) (current-column)))))
           (list charge-probe inline-probe)))
         (index (imenu--make-index-alist t)))
    (cl-labels
        ((entry-value (entry)
           (let ((position (cdr entry)))
             (save-excursion
               (goto-char position)
               (list (car entry)
                     (line-number-at-pos)
                     (buffer-substring-no-properties
                      (line-beginning-position)
                      (line-end-position))))))
         (normalize-index (items)
           (delq
            nil
            (mapcar
             (lambda (item)
               (cond
                ((equal (car-safe item) "*Rescan*") nil)
                ((imenu--subalist-p item)
                 (cons (car item)
                       (mapcar #'entry-value (cdr item))))
                (t (entry-value item))))
             items))))
      (list :navigation navigation
            :index (normalize-index index)))))
"##,
        expect![[
            r##"OK (:navigation ((:probe-line 7 :begin (6 "  def charge(order) do") :end (9 0)) (:probe-line 20 :begin (20 "  def available?, do: true") :end (21 0))) :index (("Tests" ("charges valid order" 25 "  test \"charges valid order\" do")) ("Delegates" ("capture" 19 "  defdelegate capture(order), to: Payments")) ("Public Guards" ("is_billable" 18 "  defguard is_billable(order) when order.total > 0")) ("Public Macros" ("measured" 14 "  defmacro measured(do: block) do")) ("Private Functions" ("normalize" 10 "  defp normalize(line) do")) ("Public Functions" ("total" 2 "  def total(lines) do") ("charge" 6 "  def charge(order) do") ("available?" 20 "  def available?, do: true")) ("Modules" ("Shop.Checkout" 1 "defmodule Shop.Checkout do") ("Shop.CheckoutTest" 23 "defmodule Shop.CheckoutTest do"))))"##
        ]],
    )
}

fn filling_a_real_docstring_wraps_prose_without_disturbing_module_structure() -> ParityBatchCase {
    ParityBatchCase::value(
        "filling_a_real_docstring_wraps_prose_without_disturbing_module_structure",
        r##"
(with-temp-buffer
  (insert
   "defmodule Payments do\n"
   "  @doc \"\"\"\n"
   "  Creates a payment authorization from an order and returns the gateway reference for later capture.\n"
   "\n"
   "  The caller may retry a declined authorization after replacing the payment method.\n"
   "  \"\"\"\n"
   "  def authorize(order), do: Gateway.authorize(order)\n"
   "end\n")
  (elixir-mode)
  (setq-local fill-column 52)
  (goto-char (point-min))
  (search-forward "gateway reference")
  (let ((before (list (line-number-at-pos) (current-column))))
    (elixir-mode-fill-doc-string)
    (list :before before
          :after (list (line-number-at-pos) (current-column))
          :fill-column fill-column
          :source (buffer-substring-no-properties (point-min) (point-max)))))
"##,
        expect![[
            r##"OK (:before (3 81) :after (4 31) :fill-column 52 :source "defmodule Payments do\n  @doc \"\"\"\n  Creates a payment authorization from an order and\n  returns the gateway reference for later capture.\n\n  The caller may retry a declined authorization\n  after replacing the payment method.\n  \"\"\"\n  def authorize(order), do: Gateway.authorize(order)\nend\n")"##
        ]],
    )
}

fn formatter_workflow_applies_mix_output_runs_hooks_and_cleans_process_buffers() -> ParityBatchCase
{
    ParityBatchCase::value(
        "formatter_workflow_applies_mix_output_runs_hooks_and_cleans_process_buffers",
        r##"
(let* ((run-root
        (expand-file-name
         (format "elixir-mode-format-%d/" (emacs-pid))
         (getenv "TMPDIR")))
       (target-file (expand-file-name "checkout.ex" run-root))
       (buffer (generate-new-buffer " *elixir-format-checkout*"))
       (formatted
        "defmodule Checkout do\n  def charge(order) do\n    IO.puts(order.id)\n  end\nend\n")
       events)
  (unwind-protect
      (progn
        (make-directory run-root t)
        (with-current-buffer buffer
          (insert
           "defmodule Checkout do\n"
           "def charge(order) do\n"
           "IO.puts order.id\n"
           "end\n"
           "end\n")
          (setq buffer-file-name target-file)
          (elixir-mode)
          (goto-char (point-min))
          (search-forward "IO.puts")
          (let ((elixir-format-hook
                 (list
                  (lambda ()
                    (push
                     (list :hook
                           (buffer-substring-no-properties
                            (point-min) (point-max)))
                     events)))))
            (cl-letf
                (((symbol-function 'elixir-format--elixir-executable)
                  (lambda () "/bin/elixir"))
                 ((symbol-function 'elixir-format--mix-executable)
                  (lambda () "/bin/mix"))
                 ((symbol-function 'elixir-format--from-mix-root)
                  (lambda (mix _errors arguments)
                    (let ((temp-file (car (last arguments))))
                      (push
                       (list :mix
                             mix
                             (mapcar #'file-name-nondirectory arguments)
                             (with-temp-buffer
                               (insert-file-contents temp-file)
                               (buffer-string)))
                       events)
                      (with-temp-buffer
                        (insert formatted)
                        (write-region
                         (point-min) (point-max) temp-file nil 'silent)))
                    0)))
              (elixir-format nil))
            (list :source
                  (buffer-substring-no-properties (point-min) (point-max))
                  :point (list (line-number-at-pos) (current-column))
                  :message (current-message)
                  :events (nreverse events)
                  :target (file-name-nondirectory buffer-file-name)
                  :process-buffers
                  (list
                   (if (get-buffer "*elixir-format-output*") 'alive 'cleaned)
                   (if (get-buffer "*elixir-format-errors*") 'alive 'cleaned))))))
    (when (buffer-live-p buffer)
      (kill-buffer buffer))
    (dolist (name '("*elixir-format-output*" "*elixir-format-errors*"))
      (let ((process-buffer (get-buffer name)))
        (when process-buffer
          (kill-buffer process-buffer))))
    (when (file-directory-p run-root)
      (delete-directory run-root t))))
"##,
        expect![[
            r##"OK (:source "defmodule Checkout do\n  def charge(order) do\n    IO.puts(order.id)\n  end\nend\n" :point (2 0) :message nil :events ((:hook "defmodule Checkout do\ndef charge(order) do\nIO.puts order.id\nend\nend\n") (:mix "/bin/mix" ("format" "checkout-emacs-elixir-format.ex") "defmodule Checkout do\ndef charge(order) do\nIO.puts order.id\nend\nend\n")) :target "checkout.ex" :process-buffers (cleaned cleaned))"##
        ]],
    )
}

#[test]
fn elixir_mode_package_batch() {
    let cases = vec![
        checkout_pipeline_and_error_branches_are_indented_as_real_elixir(),
        production_module_fontification_distinguishes_elixir_semantic_roles(),
        imenu_and_defun_navigation_find_real_modules_functions_macros_guards_and_tests(),
        filling_a_real_docstring_wraps_prose_without_disturbing_module_structure(),
        formatter_workflow_applies_mix_output_runs_hooks_and_cleans_process_buffers(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed Elixir Mode parity test");
    assert_oracle_batch_cases(
        elixir_mode_oracle(),
        test_name,
        "elixir_mode_parity",
        &cases,
    );
}
