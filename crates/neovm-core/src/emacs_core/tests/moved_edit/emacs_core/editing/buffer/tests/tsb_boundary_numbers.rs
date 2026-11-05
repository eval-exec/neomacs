//! Expectations captured from GNU Emacs 31.1 through sandbox-run.sh.
#[test]
fn tsb_constrain_to_field_validates_lazy_property_probe_positions() {
    let form = r#"(with-temp-buffer
      (insert "héllo")
      (list (condition-case e (constrain-to-field most-positive-fixnum 1) (error e))
            (condition-case e (constrain-to-field 1 most-positive-fixnum) (error e))
            (constrain-to-field most-positive-fixnum most-positive-fixnum)
            (let ((inhibit-field-text-motion t)) (constrain-to-field most-positive-fixnum 1))
            (constrain-to-field 6 1)
            (progn (narrow-to-region 2 5)
                   (condition-case e (constrain-to-field 1 6) (error e)))))"#;
    assert_eq!(
        crate::test_utils::runtime_startup_eval_one(form),
        "OK ".to_owned() + include_str!("tsb-constrain-to-field.expect").trim_end()
    );
}
