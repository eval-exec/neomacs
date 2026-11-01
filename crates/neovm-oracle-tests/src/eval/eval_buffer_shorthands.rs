//! `eval-buffer`/`eval-region` honor the reader's `read-symbol-shorthands`.
//!
//! GNU's reader consults the special variable while reading (src/lread.c:
//! 4956-5010), and `readevalloop` evaluates each top-level form before
//! reading the next (src/lread.c:2340). A buffer-local or dynamically bound
//! value therefore rewrites symbols in `eval-buffer` and `eval-region`, not
//! only in file loads. Neomacs previously wired shorthands only into the
//! file-load path, so eval-buffer of gptel-openrouter.el left the package's
//! `oi-` symbols unexpanded (issue #658).

use crate::common::{
    assert_ok_eq, eval_oracle_and_neovm, return_if_neovm_enable_oracle_proptest_not_set,
};

#[test]
fn eval_buffer_and_region_apply_read_symbol_shorthands() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"(list
  (with-temp-buffer
    (setq-local read-symbol-shorthands '(("oi-" . "gptel-openrouter-")))
    (insert "(setq oi-oracle-local 1)")
    (eval-buffer)
    (list (boundp 'gptel-openrouter-oracle-local)
          (boundp 'oi-oracle-local)))
  (let ((read-symbol-shorthands '(("zz-" . "longzz-"))))
    (with-temp-buffer
      (insert "(setq zz-oracle-dynamic 2)")
      (eval-region (point-min) (point-max))
      (list (boundp 'longzz-oracle-dynamic)
            (boundp 'zz-oracle-dynamic))))
  (progn
    (setq read-symbol-shorthands '(("qq-" . "longqq-")))
    (prog1
        (with-temp-buffer
          (insert "(setq qq-oracle-global 3)")
          (eval-buffer)
          (list (boundp 'longqq-oracle-global)
                (boundp 'qq-oracle-global)))
      (setq read-symbol-shorthands nil)))
  (with-temp-buffer
    (setq-local read-symbol-shorthands '(("mb-" . "longmb-")))
    (insert "(setq mb-oracle-first 4)\n\
             (setq read-symbol-shorthands '((\"nb-\" . \"longnb-\")))\n\
             (setq nb-oracle-second 5)")
    (eval-buffer)
    (list (boundp 'longmb-oracle-first)
          (boundp 'mb-oracle-first)
          (boundp 'longnb-oracle-second)
          (boundp 'nb-oracle-second)))
  (with-temp-buffer
    (setq-local read-symbol-shorthands '(("aa-" . "longaa-")))
    (insert "(setq read-symbol-shorthands '((\"rd-\" . \"longrd-\")))\n\
             (setq gptel-oracle-captured (symbol-name (read)))\n\
             rd-oracle-live")
    (eval-buffer)
    gptel-oracle-captured)
  (let ((caller (generate-new-buffer " *shorthands-caller*"))
        (target (generate-new-buffer " *shorthands-target*")))
    (unwind-protect
        (progn
          (with-current-buffer caller
            (setq-local read-symbol-shorthands '(("cx-" . "longcx-"))))
          (with-current-buffer target
            (insert "(setq cx-oracle-cross 6)"))
          (with-current-buffer caller
            (eval-buffer target))
          (list (boundp 'longcx-oracle-cross)
                (boundp 'cx-oracle-cross)))
      (kill-buffer caller)
      (kill-buffer target))))"##;
    let (gnu, neomacs) = eval_oracle_and_neovm(form);
    assert_ok_eq(
        "((t nil) (t nil) (t nil) (t nil t nil) \"longrd-oracle-live\" (t nil))",
        &gnu,
        &neomacs,
    );
}
