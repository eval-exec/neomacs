//! Expectations captured from GNU Emacs 31.1 through sandbox-run.sh.
#[test]
fn tsb_face_height_validates_relative_scale_before_storage() {
    let form = r#"(progn
      (make-face 'tsb-height)
      (mapcar (lambda (height)
                (condition-case e
                    (progn (set-face-attribute 'tsb-height nil :height height)
                           (face-attribute 'tsb-height :height))
                  (error (list (car e) (cadr e)))))
              (list 1.0e+INF 0.0e+NaN -1.0e+INF 1.0e301
                    0.01 0.09 0.1 1.2
                    (/ (float most-positive-fixnum) 10)
                    2147483648 most-positive-fixnum 0 -1 4.7e17 -2.4e17)))"#;
    assert_eq!(
        crate::test_utils::runtime_startup_eval_one(form),
        "OK ".to_owned() + include_str!("tsb-face-height.expect").trim_end()
    );
}

#[test]
fn tsb_face_height_merge_uses_the_gnu_signed_payload_probe() {
    let form = r#"(mapcar (lambda (height)
      (condition-case e (merge-face-attribute :height height 10) (error e)))
      '(1.0e+INF 0.0e+NaN -1.0e+INF 1.0e301 1.0e18 4.7e17 -2.4e17 0.01 1.2))"#;
    assert_eq!(
        crate::test_utils::runtime_startup_eval_one(form),
        "OK ".to_owned() + include_str!("tsb-face-height-merge.expect").trim_end()
    );
}
