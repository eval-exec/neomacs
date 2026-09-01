//! Strict combo oracle probes, batch 214: face-remap + text-scale cookie
//! management. face-remap-add-relative returns a cookie, face-remap-remove-
//! relative undoes it, text-scale-set/text-scale-increase buffer-local scale,
//! and face-remap-reset-remapping.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_face_remap_add_relative_cookie_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'face-remap)
(with-current-buffer (get-buffer-create " *probe-fr*")
  (let ((cookie (face-remap-add-relative 'default :height 2.0)))
    (prog1
        (list (consp cookie)
              (car cookie)
              (memq cookie (get 'default 'face-remapping)))
      (face-remap-remove-relative cookie))))
"##;
    let expect = expect_test::expect![[r#""OK (t default nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_text_scale_set_increase_decrease_buffer_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'face-remap)
(with-current-buffer (get-buffer-create " *probe-ts*")
  (let ((before text-scale-mode))
    (text-scale-set 3)
    (let ((after-set text-scale-mode-amount))
      (text-scale-decrease 1)
      (let ((after-dec text-scale-mode-amount))
        (text-scale-increase 1)
        (let ((after-inc text-scale-mode-amount))
          (prog1
              (list before
                    after-set
                    after-dec
                    after-inc
                    text-scale-mode
                    (numberp text-scale-mode-amount))))))))
"##;
    let expect = expect_test::expect![[r#""OK (nil 3 2 3 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_face_remap_multiple_cookies_reset_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'face-remap)
(with-current-buffer (get-buffer-create " *probe-fr2*")
  (let ((c1 (face-remap-add-relative 'default :weight 'bold))
        (c2 (face-remap-add-relative 'default :slant 'italic)))
    (let ((count-before (length (get 'default 'face-remapping))))
      (face-remap-remove-relative c1)
      (face-remap-remove-relative c2)
      (let ((count-after (length (get 'default 'face-remapping))))
        (kill-buffer (current-buffer))
        (list (>= count-before 2)
              count-after)))))
"##;
    let expect = expect_test::expect![[r#""OK (nil 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
