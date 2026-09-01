//! Strict combo oracle probes, batch 24: frame alists (default/initial/
//! minibuffer), window-system and system-type, buffer-local-variable set,
//! featurep/feature list, and global ring/mark state.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_f9_frame_alist_defaults() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 nil nil 0 ((width . 80) (height . 2)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (length default-frame-alist)
      (assq 'menu-bar-lines default-frame-alist)
      (assq 'tool-bar-lines default-frame-alist)
      (length initial-frame-alist)
      minibuffer-frame-alist)
"##,
        expect,
    );
}

#[test]
fn div_f9_window_system_and_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil void-function gnu/linux t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list window-system
      (condition-case err (window-system-version) (error (car err)))
      system-type
      (framep (selected-frame)))
"##,
        expect,
    );
}

#[test]
fn div_f9_window_system_is_a_dynamic_defvar_kboard_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t w32)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(eval
 '(let ((reader (lambda () window-system)))
    (let ((window-system 'w32))
      (list (special-variable-p 'window-system)
            (funcall reader))))
 t)
"##,
        expect,
    );
}

#[test]
fn div_f9_buffer_local_variables_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil t (buffer-read-only))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((bl (buffer-local-variables)))
    (list (> (length bl) 10)
          (assq 'fill-column bl)
          (local-variable-p 'fill-column)
          (local-variable-p 'buffer-file-name)
          (assq 'buffer-read-only bl))))
"##,
        expect,
    );
}

#[test]
fn div_f9_featurep_and_feature_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t (emacs) t nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (featurep 'emacs)
      (member 'emacs features)
      (> (length features) 50)
      (featurep 'png)
      (featurep 'jpeg)
      (featurep 'svg)
      (featurep 'rlimit))
"##,
        expect,
    );
}

#[test]
fn div_f9_featurep_x() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // The concrete value is backend-dependent: the reference GNU is built
    // with X11, while Neomacs uses winit/WGPU.  The portable contract is that
    // `featurep' agrees with membership in `features'.
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(eq (not (featurep 'x))
    (not (memq 'x features)))
"##,
        expect,
    );
}

#[test]
fn div_f9_global_ring_and_mode_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 5 43)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (default-value 'mark-ring)
      global-mark-ring
      kill-ring-yank-pointer
      (default-value 'global-mode-string)))
"##,
        expect,
    );
}

#[test]
fn div_f9_standard_alists_and_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (268 41 nil 8 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (length auto-mode-alist)
      (length interpreter-mode-alist)
      (assq "\\.el\\'" auto-mode-alist)
      (length minor-mode-map-alist)
      (consp (default-value 'write-file-functions)))
"##,
        expect,
    );
}
