use expect_test::expect;

use super::ParityBatchCase;

fn nested_python_project_discovery_respects_marker_precedence_and_package_root() -> ParityBatchCase
{
    let elisp_form = r##"
(neomacs-elpy-test-with-root
 "elpy-project-discovery"
 (lambda (root)
   (let* ((monorepo (expand-file-name "monorepo/" root))
          (service (expand-file-name "services/payments/" monorepo))
          (source-root (expand-file-name "src/" service))
          (package (expand-file-name "payments/" source-root))
          (file (expand-file-name "api/checkout.py" package)))
     (make-directory (expand-file-name ".git/" monorepo) t)
     (neomacs-elpy-test-write (expand-file-name "pyproject.toml" service)
                              "[project]\nname = \"payments\"\n")
     (neomacs-elpy-test-write (expand-file-name "__init__.py" package) "")
     (neomacs-elpy-test-write (expand-file-name "api/__init__.py" package) "")
     (neomacs-elpy-test-write file "def checkout(cart):\n    return cart.total\n")
     (with-current-buffer (find-file-noselect file)
       (unwind-protect
           (let ((default-directory (file-name-directory file))
                 (elpy-project-root nil)
                 (elpy-project-root-finder-functions
                  '(elpy-project-find-python-root elpy-project-find-git-root)))
             (list
              :python-first
              (neomacs-elpy-test-relative (elpy-project-root) root)
              :cached (neomacs-elpy-test-relative (elpy-project-root) root)
              :python-marker
              (neomacs-elpy-test-relative (elpy-project-find-python-root) root)
              :git-marker
              (neomacs-elpy-test-relative (elpy-project-find-git-root) root)
              :library-root
              (neomacs-elpy-test-relative (elpy-library-root) root)
              :git-only
              (let ((elpy-project-root nil)
                    (elpy-project-root-finder-functions
                     '(elpy-project-find-git-root)))
                (neomacs-elpy-test-relative (elpy-project-root) root))))
         (set-buffer-modified-p nil)
         (kill-buffer (current-buffer)))))))
"##;
    let expect = expect![[
        r#"OK (:python-first "monorepo/services/payments/" :cached "monorepo/services/payments/" :python-marker "monorepo/services/payments/" :git-marker "monorepo/" :library-root "monorepo/services/payments/src/" :git-only "monorepo/")"#
    ]];
    ParityBatchCase::value(
        "nested_python_project_discovery_respects_marker_precedence_and_package_root",
        elisp_form,
        expect,
    )
}

fn python_navigation_and_region_editing_transform_a_real_method_body() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (python-mode)
  (insert
   "class Invoice:\n"
   "    def total(self, items):\n"
   "        subtotal = sum(items)\n"
   "        if subtotal > 100:\n"
   "            discount = 10\n"
   "        tax = 5\n"
   "        return subtotal + tax\n")
  (goto-char (point-min))
  (search-forward "if subtotal")
  (back-to-indentation)
  (let ((before (neomacs-elpy-test-buffer-state)))
    (elpy-nav-forward-block)
    (let ((forward (neomacs-elpy-test-buffer-state)))
      (elpy-nav-backward-block)
      (let ((backward (neomacs-elpy-test-buffer-state)))
        (goto-char (point-min))
        (search-forward "subtotal = sum(items)")
        (beginning-of-line)
        (set-mark (point))
        (forward-line 2)
        (setq mark-active t)
        (elpy-nav-indent-shift-right)
        (let ((shifted-right (neomacs-elpy-test-buffer-state)))
          (elpy-nav-indent-shift-left)
          (list
           :before before
           :forward forward
           :backward backward
           :shifted-right shifted-right
           :restored (neomacs-elpy-test-buffer-state)
           :import-reorder
           (with-temp-buffer
             (python-mode)
             (insert "from decimal import Decimal\n"
                     "from typing import Iterable\n\n")
             (goto-char (point-min))
             (forward-line 1)
             (elpy-nav-move-line-or-region-up)
             (let ((moved-up (neomacs-elpy-test-buffer-state)))
               (elpy-nav-move-line-or-region-down)
               (list :moved-up moved-up
                     :restored (neomacs-elpy-test-buffer-state))))))))))
"##;
    let expect = expect![[
        r#"OK (:before (:text "class Invoice:\n    def total(self, items):\n        subtotal = sum(items)\n        if subtotal > 100:\n            discount = 10\n        tax = 5\n        return subtotal + tax\n" :point 82 :line 4 :column 8 :mark nil :active nil :region nil) :forward (:text "class Invoice:\n    def total(self, items):\n        subtotal = sum(items)\n        if subtotal > 100:\n            discount = 10\n        tax = 5\n        return subtotal + tax\n" :point 135 :line 6 :column 8 :mark nil :active nil :region nil) :backward (:text "class Invoice:\n    def total(self, items):\n        subtotal = sum(items)\n        if subtotal > 100:\n            discount = 10\n        tax = 5\n        return subtotal + tax\n" :point 82 :line 4 :column 8 :mark nil :active nil :region nil) :shifted-right (:text "class Invoice:\n    def total(self, items):\n        subtotal = sum(items)\n        if subtotal > 100:\n                discount = 10\n        tax = 5\n        return subtotal + tax\n" :point 101 :line 5 :column 0 :mark 44 :active t :region "        subtotal = sum(items)\n        if subtotal > 100:\n") :restored (:text "class Invoice:\n    def total(self, items):\n        subtotal = sum(items)\n        if subtotal > 100:\n            discount = 10\n        tax = 5\n        return subtotal + tax\n" :point 101 :line 5 :column 0 :mark 44 :active t :region "        subtotal = sum(items)\n        if subtotal > 100:\n") :import-reorder (:moved-up (:text "from typing import Iterable\nfrom decimal import Decimal\n\n" :point 1 :line 1 :column 0 :mark nil :active nil :region nil) :restored (:text "from decimal import Decimal\nfrom typing import Iterable\n\n" :point 29 :line 2 :column 0 :mark nil :active nil :region nil)))"#
    ]];
    ParityBatchCase::value(
        "python_navigation_and_region_editing_transform_a_real_method_body",
        elisp_form,
        expect,
    )
}

fn live_multiedit_renames_exact_inventory_symbols_without_touching_prefixes() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (python-mode)
  (insert
   "inventory = load_inventory()\n"
   "inventory_count = len(inventory)\n"
   "publish(inventory)\n")
  (goto-char (point-min))
  (let ((elpy-multiedit-overlays nil))
    (unwind-protect
        (progn
          (elpy-multiedit)
          (let ((initial-ranges
                 (neomacs-elpy-test-overlay-ranges elpy-multiedit-overlays)))
            (insert "active_")
            (let ((edited (buffer-string))
                  (edited-ranges
                   (neomacs-elpy-test-overlay-ranges
                    elpy-multiedit-overlays)))
              (elpy-multiedit-stop)
              (list :initial-ranges initial-ranges
                    :edited edited
                    :edited-ranges edited-ranges
                    :stopped elpy-multiedit-overlays
                    :remaining-overlays (overlays-in (point-min) (point-max))))))
      (elpy-multiedit-stop))))
"##;
    let expect = expect![[
        r#"OK (:initial-ranges ((1 10 "inventory") (52 61 "inventory") (71 80 "inventory")) :edited "active_inventory = load_inventory()\ninventory_count = len(active_inventory)\npublish(active_inventory)\n" :edited-ranges ((1 17 "active_inventory") (59 75 "active_inventory") (85 101 "active_inventory")) :stopped nil :remaining-overlays nil)"#
    ]];
    ParityBatchCase::value(
        "live_multiedit_renames_exact_inventory_symbols_without_touching_prefixes",
        elisp_form,
        expect,
    )
}

fn test_at_point_builds_unittest_and_pytest_commands_for_a_real_test_method() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-elpy-test-with-root
 "elpy-test-planning"
 (lambda (root)
   (let* ((project (expand-file-name "checkout-service/" root))
          (test-file (expand-file-name "tests/test_checkout.py" project)))
     (neomacs-elpy-test-write (expand-file-name "pyproject.toml" project)
                              "[project]\nname = \"checkout-service\"\n")
     (neomacs-elpy-test-write (expand-file-name "tests/__init__.py" project) "")
     (neomacs-elpy-test-write
      test-file
      (concat
       "class CheckoutTests:\n"
       "    def test_declines_expired_card(self):\n"
       "        response = charge(card='expired')\n"
       "        assert response.status == 402\n"))
     (with-current-buffer (find-file-noselect test-file)
       (unwind-protect
           (progn
             (python-mode)
             (goto-char (point-min))
             (search-forward "response.status")
             (let* ((target (elpy-test-at-point))
                    (relative-target
                     (list (neomacs-elpy-test-relative (nth 0 target) root)
                           (neomacs-elpy-test-relative (nth 1 target) root)
                           (nth 2 target)
                           (nth 3 target)))
                    calls
                    (elpy-test-compilation-function
                     (lambda (command)
                       (push (list (neomacs-elpy-test-relative
                                    default-directory root)
                                   command)
                             calls))))
               (let ((elpy-test-runner 'elpy-test-discover-runner))
                 (elpy-test))
               (let ((elpy-test-runner 'elpy-test-pytest-runner))
                 (elpy-test))
               (list :target relative-target
                     :current-defun (python-info-current-defun)
                     :module (elpy-test--module-name-for-file
                              (nth 0 target) (nth 1 target))
                     :runner-flags
                     (mapcar #'elpy-test-runner-p
                             '(elpy-test-discover-runner
                               elpy-test-pytest-runner
                               ignore))
                     :calls (nreverse calls))))
         (set-buffer-modified-p nil)
         (kill-buffer (current-buffer)))))))
"##;
    let expect = expect![[
        r#"OK (:target ("checkout-service/" "checkout-service/tests/test_checkout.py" "tests.test_checkout" "CheckoutTests.test_declines_expired_card") :current-defun "CheckoutTests.test_declines_expired_card" :module "tests.test_checkout" :runner-flags (t t nil) :calls (("checkout-service/" "python -m unittest tests.test_checkout.CheckoutTests.test_declines_expired_card") ("checkout-service/" "py.test [ORACLE-SANDBOX]/elpy-test-planning/checkout-service/tests/test_checkout.py\\:\\:CheckoutTests\\:\\:test_declines_expired_card")))"#
    ]];
    ParityBatchCase::value(
        "test_at_point_builds_unittest_and_pytest_commands_for_a_real_test_method",
        elisp_form,
        expect,
    )
}

fn module_navigation_resolves_modules_packages_and_relative_imports_on_disk() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-elpy-test-with-root
 "elpy-module-navigation"
 (lambda (root)
   (let* ((source (expand-file-name "src/" root))
          (shop (expand-file-name "shop/" source))
          (api (expand-file-name "api/" shop))
          (file (expand-file-name "checkout.py" api))
          source-buffer
          opened-buffers)
     (neomacs-elpy-test-write (expand-file-name "__init__.py" shop) "")
     (neomacs-elpy-test-write (expand-file-name "__init__.py" api) "")
     (neomacs-elpy-test-write (expand-file-name "models/__init__.py" shop) "")
     (neomacs-elpy-test-write (expand-file-name "models/order.py" shop)
                              "class Order:\n    pass\n")
     (neomacs-elpy-test-write
      file
      (concat
       "from shop.models import order\n"
       "from shop.models import Order\n"
       "from ..models import order\n"
       "from shop.missing import value\n"))
     (setq source-buffer (find-file-noselect file))
     (with-current-buffer source-buffer
       (unwind-protect
           (save-window-excursion
             (let ((default-directory api))
               (cl-labels
                   ((visit-import
                     (line label)
                     (switch-to-buffer source-buffer)
                     (goto-char (point-min))
                     (forward-line line)
                     (let ((source-line
                            (buffer-substring-no-properties
                             (line-beginning-position) (line-end-position))))
                       (elpy-find-file t)
                       (push (current-buffer) opened-buffers)
                       (list
                        :case label
                        :source source-line
                        :file (neomacs-elpy-test-relative buffer-file-name root)
                        :mode major-mode
                        :text (buffer-string)
                        :point (point)))))
                 (list
                  :library-root
                  (neomacs-elpy-test-relative (elpy-library-root) root)
                  :visits
                  (list
                   (visit-import 0 'absolute-module)
                   (visit-import 1 'nearest-package)
                   (visit-import 2 'relative-module)
                   (visit-import 3 'missing-nearest-package))))))
         (dolist (buffer (delete-dups
                          (delq nil (cons source-buffer opened-buffers))))
           (when (buffer-live-p buffer)
             (with-current-buffer buffer
               (set-buffer-modified-p nil))
             (kill-buffer buffer))))))))
"##;
    let expect = expect![[
        r#"OK (:library-root "src/" :visits ((:case absolute-module :source "from shop.models import order" :file "src/shop/models/order.py" :mode python-mode :text "class Order:\n    pass\n" :point 1) (:case nearest-package :source "from shop.models import Order" :file "src/shop/models/__init__.py" :mode python-mode :text "" :point 1) (:case relative-module :source "from ..models import order" :file "src/shop/models/order.py" :mode python-mode :text "class Order:\n    pass\n" :point 1) (:case missing-nearest-package :source "from shop.missing import value" :file "src/shop/__init__.py" :mode python-mode :text "" :point 1)))"#
    ]];
    ParityBatchCase::value(
        "module_navigation_resolves_modules_packages_and_relative_imports_on_disk",
        elisp_form,
        expect,
    )
}

fn global_enable_and_disable_controls_python_buffers_hooks_keys_and_checks() -> ParityBatchCase {
    let elisp_form = r##"
(let ((elpy-modules nil)
      (elpy-enabled-p nil)
      (python-mode-hook (remove 'elpy-mode python-mode-hook))
      (pyvenv-post-activate-hooks
       (remove 'elpy-rpc--disconnect pyvenv-post-activate-hooks))
      (pyvenv-post-deactivate-hooks
       (remove 'elpy-rpc--disconnect pyvenv-post-deactivate-hooks))
      (inferior-python-mode-hook
       (remove 'elpy-shell--enable-output-filter inferior-python-mode-hook))
      (python-shell-first-prompt-hook
       (remove 'elpy-shell--send-setup-code python-shell-first-prompt-hook))
      (buffer (generate-new-buffer "invoice.py")))
  (unwind-protect
      (progn
        (elpy-enable)
        (with-current-buffer buffer
          (python-mode)
          (insert "def total(items):\n    return sum(items)\n")
          (let ((enabled
                 (list :global elpy-enabled-p
                       :local elpy-mode
                       :mode major-mode
                       :lighter (assq 'elpy-mode minor-mode-alist)
                       :test-key (key-binding (kbd "C-c C-t"))
                       :edit-key (key-binding (kbd "C-c C-e"))
                       :check python-check-command
                       :xref (and (boundp 'xref-backend-functions)
                                  (memq 'elpy--xref-backend
                                        xref-backend-functions))
                       :python-hook (memq 'elpy-mode python-mode-hook)
                       :activate-hook
                       (memq 'elpy-rpc--disconnect pyvenv-post-activate-hooks))))
            (elpy-disable)
            (list :enabled enabled
                  :disabled
                  (list :global elpy-enabled-p
                        :python-hook (memq 'elpy-mode python-mode-hook)
                        :activate-hook
                        (memq 'elpy-rpc--disconnect pyvenv-post-activate-hooks)
                        :existing-local elpy-mode)
                  :new-buffer
                  (with-temp-buffer
                    (python-mode)
                    (list major-mode elpy-mode))))))
    (when (buffer-live-p buffer)
      (kill-buffer buffer))
    (when elpy-enabled-p
      (elpy-disable))))
"##;
    let expect = expect![[
        r#"OK (:enabled (:global t :local t :mode python-mode :lighter (elpy-mode " Elpy") :test-key elpy-test :edit-key elpy-multiedit-python-symbol-at-point :check "flake8" :xref (elpy--xref-backend t) :python-hook (elpy-mode) :activate-hook (elpy-rpc--disconnect)) :disabled (:global nil :python-hook nil :activate-hook nil :existing-local t) :new-buffer (python-mode nil))"#
    ]];
    ParityBatchCase::value(
        "global_enable_and_disable_controls_python_buffers_hooks_keys_and_checks",
        elisp_form,
        expect,
    )
    .fresh_process()
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        nested_python_project_discovery_respects_marker_precedence_and_package_root(),
        python_navigation_and_region_editing_transform_a_real_method_body(),
        live_multiedit_renames_exact_inventory_symbols_without_touching_prefixes(),
        test_at_point_builds_unittest_and_pytest_commands_for_a_real_test_method(),
        module_navigation_resolves_modules_packages_and_relative_imports_on_disk(),
        global_enable_and_disable_controls_python_buffers_hooks_keys_and_checks(),
    ]
}
