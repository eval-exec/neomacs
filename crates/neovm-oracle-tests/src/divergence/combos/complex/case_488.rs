/// Batch 488: frame-config, frame-selected, frame-parameter deep, frame-geometry.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx488_frame_config_register() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (selected-frame)))
  (frame-configuration-to-register ?f)
  (jump-to-register ?f)
  (framep f))
"##,
        expect,
    );
}

#[test]
fn div_cx488_frame_parameters_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"F1\" 80 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (selected-frame)))
  (list (frame-parameter f 'name)
        (frame-parameter f 'width)
        (frame-parameter f 'height)))
"##,
        expect,
    );
}

#[test]
fn div_cx488_frame_text_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (80 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (selected-frame)))
  (list (frame-text-width f) (frame-text-height f)))
"##,
        expect,
    );
}

#[test]
fn div_cx488_frame_pixel_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (80 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (selected-frame)))
  (list (frame-pixel-width f) (frame-pixel-height f)))
"##,
        expect,
    );
}

#[test]
fn div_cx488_frame_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 . 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (selected-frame)))
  (condition-case e
      (frame-position f)
    (error (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx488_frame_iconified() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function frame-iconified-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (selected-frame)))
  (list (frame-visible-p f) (frame-iconified-p f)))
"##,
        expect,
    );
}

#[test]
fn div_cx488_frame_alpha() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (selected-frame)))
  (condition-case e
      (frame-parameter f 'alpha)
    (error (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx488_frame_size_hints() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (selected-frame)))
  (condition-case e
      (frame-size-hints-pixelwise f)
    (error (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx488_frame_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function frame-display)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (selected-frame)))
  (list (frame-display f) (frame-terminal f)))
"##,
        expect,
    );
}

#[test]
fn div_cx488_frame_live_child() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (#<window 1 on *scratch*> #<window 1 on *scratch*>)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (selected-frame)))
  (list (frame-root-window f) (frame-first-window f)))
"##,
        expect,
    );
}

#[test]
fn div_cx488_frame_parameter_names() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function frame-parameters-keys)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (selected-frame)))
  (list (frame-parameters-keys f) (frame-parameter-names f)))
"##,
        expect,
    );
}

#[test]
fn div_cx488_frame_restack() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK error""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (frame-restack (selected-frame) (selected-frame) nil)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx488_frame_after_make() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK error""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (make-frame-invisible (selected-frame))
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx488_frame_visible() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(frame-visible-p (selected-frame))
"##,
        expect,
    );
}

#[test]
fn div_cx488_frame_raise_lower() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (raise-frame (selected-frame))
  (error (car e)))
"##,
        expect,
    );
}
