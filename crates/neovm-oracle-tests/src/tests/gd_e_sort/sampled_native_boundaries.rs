use super::*;

#[test]
fn oracle_sort_native_frames_and_signals_inside_nested_calls() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
      (defun neovm--gde-stack-nest (levels function)
        (if (= levels 0) (funcall function)
          (neovm--gde-stack-nest (1- levels) function)))
      (let ((max-lisp-eval-depth 1000) out)
        (dolist (levels '(0 7 15 31))
          (dolist (pred '(< string<))
            (let ((v (make-vector 50 nil)))
              (dotimes (i 50)
                (aset v i (if (eq pred '<) (mod (+ (* i 17) 13) 50)
                  (format "%04d" (mod (+ (* i 17) 13) 50)))))
              (let ((returned (neovm--gde-stack-nest levels (lambda () (sort v pred)))))
                (push (list levels pred 'success (eq returned v)
                  (secure-hash 'sha256 (prin1-to-string v))) out)))
            (let ((v (make-vector 50 nil)) seen frame-seen
                  (captured (indirect-function pred)))
              (dotimes (i 50)
                (aset v i (if (eq pred '<) (mod (+ (* i 17) 13) 50)
                  (format "%04d" (mod (+ (* i 17) 13) 50)))))
              (aset v 47 (if (eq pred '<) 'bad '(bad)))
              (let ((signal-hook-function
                     (lambda (condition data)
                       (when (eq condition 'wrong-type-argument)
                         (setq seen (copy-sequence v)
                               frame-seen (not (null (backtrace-frame 0 captured))))))))
                (push (list levels pred 'signal
                  (condition-case err
                    (neovm--gde-stack-nest levels (lambda () (sort v pred)))
                    (error (car err)))
                  frame-seen (and seen (secure-hash 'sha256 (prin1-to-string seen)))
                  (secure-hash 'sha256 (prin1-to-string v))) out)))))
        (nreverse out)))"#;
    assert_oracle_parity_under_envs_expect(
        form,
        ENVS,
        expect_test::expect![[
            r#""OK ((0 < success t \"dd781f76e21d0d7ef51c6094ae6e854528ff607d340301d9fb0e9e69c1cd44a2\") (0 < signal wrong-type-argument t \"e1718fd32f8e03f9405d50479d4df617696218ad2aa673d62daf956dae6cdff6\" \"e1718fd32f8e03f9405d50479d4df617696218ad2aa673d62daf956dae6cdff6\") (0 string< success t \"c98b4c911abbe9e61b1f0b07d1cee4d02b0b2857d764633745990961827d8343\") (0 string< signal wrong-type-argument t \"fddc483daa70f0fae8e17177c5cbdba2a05e07846b1274477ef67591aa7951d3\" \"fddc483daa70f0fae8e17177c5cbdba2a05e07846b1274477ef67591aa7951d3\") (7 < success t \"dd781f76e21d0d7ef51c6094ae6e854528ff607d340301d9fb0e9e69c1cd44a2\") (7 < signal wrong-type-argument t \"e1718fd32f8e03f9405d50479d4df617696218ad2aa673d62daf956dae6cdff6\" \"e1718fd32f8e03f9405d50479d4df617696218ad2aa673d62daf956dae6cdff6\") (7 string< success t \"c98b4c911abbe9e61b1f0b07d1cee4d02b0b2857d764633745990961827d8343\") (7 string< signal wrong-type-argument t \"fddc483daa70f0fae8e17177c5cbdba2a05e07846b1274477ef67591aa7951d3\" \"fddc483daa70f0fae8e17177c5cbdba2a05e07846b1274477ef67591aa7951d3\") (15 < success t \"dd781f76e21d0d7ef51c6094ae6e854528ff607d340301d9fb0e9e69c1cd44a2\") (15 < signal wrong-type-argument t \"e1718fd32f8e03f9405d50479d4df617696218ad2aa673d62daf956dae6cdff6\" \"e1718fd32f8e03f9405d50479d4df617696218ad2aa673d62daf956dae6cdff6\") (15 string< success t \"c98b4c911abbe9e61b1f0b07d1cee4d02b0b2857d764633745990961827d8343\") (15 string< signal wrong-type-argument t \"fddc483daa70f0fae8e17177c5cbdba2a05e07846b1274477ef67591aa7951d3\" \"fddc483daa70f0fae8e17177c5cbdba2a05e07846b1274477ef67591aa7951d3\") (31 < success t \"dd781f76e21d0d7ef51c6094ae6e854528ff607d340301d9fb0e9e69c1cd44a2\") (31 < signal wrong-type-argument t \"e1718fd32f8e03f9405d50479d4df617696218ad2aa673d62daf956dae6cdff6\" \"e1718fd32f8e03f9405d50479d4df617696218ad2aa673d62daf956dae6cdff6\") (31 string< success t \"c98b4c911abbe9e61b1f0b07d1cee4d02b0b2857d764633745990961827d8343\") (31 string< signal wrong-type-argument t \"fddc483daa70f0fae8e17177c5cbdba2a05e07846b1274477ef67591aa7951d3\" \"fddc483daa70f0fae8e17177c5cbdba2a05e07846b1274477ef67591aa7951d3\"))""#
        ]],
    );
}
