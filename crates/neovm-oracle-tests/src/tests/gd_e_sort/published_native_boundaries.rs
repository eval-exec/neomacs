//! GNU sort.c:198-214 and eval.c:3194-3218 require the captured predicate
//! frame during signal hooks, including ordinary list and keyed-vector sorts.
use super::*;

#[test]
fn oracle_sort_published_native_frames_for_lists_and_keyed_vectors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
      (defun neovm--gde-published-nest (levels function)
        (if (= levels 0) (funcall function)
          (neovm--gde-published-nest (1- levels) function)))
      (let ((max-lisp-eval-depth 1000) out)
        (dolist (levels '(0 15))
          (dolist (pred '(< string<))
            (dolist (container '(list keyed-vector))
              (dolist (compiled '(nil t))
                (dolist (failed '(nil t))
                  (let* ((items (let (xs) (dotimes (i 50 (nreverse xs))
                                  (push (if (eq pred '<) (mod (+ (* i 17) 13) 50)
                                          (format "%04d" (mod (+ (* i 17) 13) 50))) xs))))
                         (seq (if (eq container 'list) items (vconcat items)))
                         (captured (indirect-function pred))
                         (calls 0) seen frame-args frame-seen
                         (key (and (eq container 'keyed-vector)
                                  (lambda (x) (setq calls (1+ calls)) x))))
                    (when failed
                      (if (eq container 'list)
                          (setcar (nthcdr 47 seq) '(bad))
                        (aset seq 47 '(bad))))
                    (let* ((sorter (lambda (sequence predicate key)
                                     (sort sequence :lessp predicate :key key :in-place t)))
                           (function (if compiled (byte-compile sorter) sorter))
                           (run (lambda () (funcall function seq pred key)))
                           (signal-hook-function
                            (lambda (condition data)
                              (when (eq condition 'wrong-type-argument)
                                (let ((frame (backtrace-frame 0 captured)))
                                  (setq frame-seen (not (null frame))
                                        frame-args (cddr frame)
                                        seen (secure-hash 'sha256 (prin1-to-string seq))))
                                (if (eq container 'list)
                                    (setcar seq (if (eq pred '<) 777 "zzzz"))
                                  (aset seq 0 (if (eq pred '<) 777 "zzzz")))
                                (garbage-collect)))))
                      (let ((result (condition-case err
                                      (neovm--gde-published-nest levels run)
                                    (error (car err)))))
                        (push (list levels pred container compiled failed
                                    (if failed result (eq result seq)) calls
                                    frame-seen frame-args seen
                                    (secure-hash 'sha256 (prin1-to-string seq))) out)))))))))
        (nreverse out)))"#;
    assert_oracle_parity_under_envs_expect(
        form,
        ENVS,
        expect_test::expect![[
            r#""OK ((0 < list nil nil t 0 nil nil nil \"7cfe7666f0602cd4d06e94363edbf347ea7938f16150b8b53276be84ca68bf72\") (0 < list nil t wrong-type-argument 0 t ((bad) 24) \"ea0c91f98d451df45c72a59f6c739e8575ed1672025ba0c276c40d44af90ada5\" \"44cb27ba699a80548baf131874ed9bee38c9c0de648d86974aecd1db25adbd4e\") (0 < list t nil t 0 nil nil nil \"7cfe7666f0602cd4d06e94363edbf347ea7938f16150b8b53276be84ca68bf72\") (0 < list t t wrong-type-argument 0 t ((bad) 24) \"ea0c91f98d451df45c72a59f6c739e8575ed1672025ba0c276c40d44af90ada5\" \"44cb27ba699a80548baf131874ed9bee38c9c0de648d86974aecd1db25adbd4e\") (0 < keyed-vector nil nil t 50 nil nil nil \"dd781f76e21d0d7ef51c6094ae6e854528ff607d340301d9fb0e9e69c1cd44a2\") (0 < keyed-vector nil t wrong-type-argument 50 t ((bad) 24) \"74d5441df6b02f897064203b4a51848cbeac43809cc997820b76e41eb1b51742\" \"67acc00e03f5c93ec875888e8304d42ad85fa7d4b4af0ad3622c6470963fb09e\") (0 < keyed-vector t nil t 50 nil nil nil \"dd781f76e21d0d7ef51c6094ae6e854528ff607d340301d9fb0e9e69c1cd44a2\") (0 < keyed-vector t t wrong-type-argument 50 t ((bad) 24) \"74d5441df6b02f897064203b4a51848cbeac43809cc997820b76e41eb1b51742\" \"67acc00e03f5c93ec875888e8304d42ad85fa7d4b4af0ad3622c6470963fb09e\") (0 string< list nil nil t 0 nil nil nil \"4945221a8fa864b2de1fa1632af531340bf44fdf9980adda92f3d3745292609c\") (0 string< list nil t wrong-type-argument 0 t ((bad) \"0024\") \"449e42c187f13d5f30e00cb9c92241d683fb065638c4edc28d596025d47da98f\" \"0ece654a03f626f50b0646b07519ecf04b9c51eb334a79a3b363af47d626d7f6\") (0 string< list t nil t 0 nil nil nil \"4945221a8fa864b2de1fa1632af531340bf44fdf9980adda92f3d3745292609c\") (0 string< list t t wrong-type-argument 0 t ((bad) \"0024\") \"449e42c187f13d5f30e00cb9c92241d683fb065638c4edc28d596025d47da98f\" \"0ece654a03f626f50b0646b07519ecf04b9c51eb334a79a3b363af47d626d7f6\") (0 string< keyed-vector nil nil t 50 nil nil nil \"c98b4c911abbe9e61b1f0b07d1cee4d02b0b2857d764633745990961827d8343\") (0 string< keyed-vector nil t wrong-type-argument 50 t ((bad) \"0024\") \"fddc483daa70f0fae8e17177c5cbdba2a05e07846b1274477ef67591aa7951d3\" \"dabe4ae875cddfe689692b48a518a1a0f577fe7963389517bc9cbdb152c5beb1\") (0 string< keyed-vector t nil t 50 nil nil nil \"c98b4c911abbe9e61b1f0b07d1cee4d02b0b2857d764633745990961827d8343\") (0 string< keyed-vector t t wrong-type-argument 50 t ((bad) \"0024\") \"fddc483daa70f0fae8e17177c5cbdba2a05e07846b1274477ef67591aa7951d3\" \"dabe4ae875cddfe689692b48a518a1a0f577fe7963389517bc9cbdb152c5beb1\") (15 < list nil nil t 0 nil nil nil \"7cfe7666f0602cd4d06e94363edbf347ea7938f16150b8b53276be84ca68bf72\") (15 < list nil t wrong-type-argument 0 t ((bad) 24) \"ea0c91f98d451df45c72a59f6c739e8575ed1672025ba0c276c40d44af90ada5\" \"44cb27ba699a80548baf131874ed9bee38c9c0de648d86974aecd1db25adbd4e\") (15 < list t nil t 0 nil nil nil \"7cfe7666f0602cd4d06e94363edbf347ea7938f16150b8b53276be84ca68bf72\") (15 < list t t wrong-type-argument 0 t ((bad) 24) \"ea0c91f98d451df45c72a59f6c739e8575ed1672025ba0c276c40d44af90ada5\" \"44cb27ba699a80548baf131874ed9bee38c9c0de648d86974aecd1db25adbd4e\") (15 < keyed-vector nil nil t 50 nil nil nil \"dd781f76e21d0d7ef51c6094ae6e854528ff607d340301d9fb0e9e69c1cd44a2\") (15 < keyed-vector nil t wrong-type-argument 50 t ((bad) 24) \"74d5441df6b02f897064203b4a51848cbeac43809cc997820b76e41eb1b51742\" \"67acc00e03f5c93ec875888e8304d42ad85fa7d4b4af0ad3622c6470963fb09e\") (15 < keyed-vector t nil t 50 nil nil nil \"dd781f76e21d0d7ef51c6094ae6e854528ff607d340301d9fb0e9e69c1cd44a2\") (15 < keyed-vector t t wrong-type-argument 50 t ((bad) 24) \"74d5441df6b02f897064203b4a51848cbeac43809cc997820b76e41eb1b51742\" \"67acc00e03f5c93ec875888e8304d42ad85fa7d4b4af0ad3622c6470963fb09e\") (15 string< list nil nil t 0 nil nil nil \"4945221a8fa864b2de1fa1632af531340bf44fdf9980adda92f3d3745292609c\") (15 string< list nil t wrong-type-argument 0 t ((bad) \"0024\") \"449e42c187f13d5f30e00cb9c92241d683fb065638c4edc28d596025d47da98f\" \"0ece654a03f626f50b0646b07519ecf04b9c51eb334a79a3b363af47d626d7f6\") (15 string< list t nil t 0 nil nil nil \"4945221a8fa864b2de1fa1632af531340bf44fdf9980adda92f3d3745292609c\") (15 string< list t t wrong-type-argument 0 t ((bad) \"0024\") \"449e42c187f13d5f30e00cb9c92241d683fb065638c4edc28d596025d47da98f\" \"0ece654a03f626f50b0646b07519ecf04b9c51eb334a79a3b363af47d626d7f6\") (15 string< keyed-vector nil nil t 50 nil nil nil \"c98b4c911abbe9e61b1f0b07d1cee4d02b0b2857d764633745990961827d8343\") (15 string< keyed-vector nil t wrong-type-argument 50 t ((bad) \"0024\") \"fddc483daa70f0fae8e17177c5cbdba2a05e07846b1274477ef67591aa7951d3\" \"dabe4ae875cddfe689692b48a518a1a0f577fe7963389517bc9cbdb152c5beb1\") (15 string< keyed-vector t nil t 50 nil nil nil \"c98b4c911abbe9e61b1f0b07d1cee4d02b0b2857d764633745990961827d8343\") (15 string< keyed-vector t t wrong-type-argument 50 t ((bad) \"0024\") \"fddc483daa70f0fae8e17177c5cbdba2a05e07846b1274477ef67591aa7951d3\" \"dabe4ae875cddfe689692b48a518a1a0f577fe7963389517bc9cbdb152c5beb1\"))""#
        ]],
    );
}
