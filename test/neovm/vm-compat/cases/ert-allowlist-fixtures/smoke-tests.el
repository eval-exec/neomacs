(require 'ert)

(ert-deftest neovm-allowlist-smoke-eq ()
  (should (eq 'alpha 'alpha)))

(ert-deftest neovm-allowlist-smoke-list ()
  (should (equal '(1 2 3) (list 1 2 3))))
