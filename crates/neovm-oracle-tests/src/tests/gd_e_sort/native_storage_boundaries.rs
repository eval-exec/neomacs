use super::*;

#[test]
fn oracle_sort_native_storage_signal_and_lisp_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
      (require 'cl-lib)
      (defvar neovm--gde-native-vector nil)
      (defvar neovm--gde-native-seen nil)
      (defvar neovm--gde-native-frames nil)
      (defvar neovm--gde-native-function nil)
      (let (out)
        (dolist (pred '(< string<))
          (dolist (n '(50 1000))
            (setq neovm--gde-native-function (indirect-function pred)
                  neovm--gde-native-vector (make-vector n nil)
                  neovm--gde-native-seen nil neovm--gde-native-frames nil)
            (dotimes (i n)
              (aset neovm--gde-native-vector i
                (if (eq pred '<) (mod (+ (* i 17) 13) n)
                  (format "%04d" (mod (+ (* i 17) 13) n)))))
            (aset neovm--gde-native-vector (- n 3) (if (eq pred '<) 'bad '(bad)))
            (let ((signal-hook-function
              (lambda (condition data)
                (when (eq condition 'wrong-type-argument)
                  (setq neovm--gde-native-seen (copy-sequence neovm--gde-native-vector)
                        neovm--gde-native-frames
                          (not (null (backtrace-frame 0 neovm--gde-native-function))))
                  (aset neovm--gde-native-vector 0 (if (numberp (aref neovm--gde-native-vector 0)) 777 "zzzz"))
                  (garbage-collect)))))
              (push (list pred n
                (condition-case err (sort neovm--gde-native-vector pred) (error (car err)))
                (and neovm--gde-native-seen
                  (secure-hash 'sha256 (prin1-to-string neovm--gde-native-seen)))
                (secure-hash 'sha256 (prin1-to-string neovm--gde-native-vector))
                neovm--gde-native-frames) out))))
        (let ((v (vector 3 nil 2)))
          (aset v 1 v)
          (let ((result (condition-case err (sort v :in-place t) (error (car err)))))
            (push (list result (mapcar (lambda (x) (if (eq x v) 'self x)) v)) out)))
        (nreverse out)))"#;
    assert_oracle_parity_under_envs_expect(
        form,
        ENVS,
        expect_test::expect![[
            r#""OK ((< 50 wrong-type-argument \"e1718fd32f8e03f9405d50479d4df617696218ad2aa673d62daf956dae6cdff6\" \"3dfa6808cb33b78424444a035f48cc2d6d1854210948d1528e771fb3e9b34945\" t) (< 1000 wrong-type-argument \"c35e95e9953a0ce7abcf7841716259f2dca35c530cf540e453010182bc175faa\" \"4bef7c461b27e5df7496e3d33d6cf6a10f00145323f63fcba9dad0efaf1064ab\" t) (string< 50 wrong-type-argument \"fddc483daa70f0fae8e17177c5cbdba2a05e07846b1274477ef67591aa7951d3\" \"dabe4ae875cddfe689692b48a518a1a0f577fe7963389517bc9cbdb152c5beb1\" t) (string< 1000 wrong-type-argument \"14567989b463f05b90e980a8e7f84ae411ec329b3df600accd0ab092a4fd50da\" \"54a2f81a3c3e2d2140cb7896b619a8a23d1f469b1188b98cbb4ba7ef47243e06\" t) (type-mismatch (3 self 2)))""#
        ]],
    );
}
