//! GNU `Fplist_get`, `Fplist_put` and `Fplist_member` with a PREDICATE
//! (fns.c): XCDR (tail) is read again after each predicate call, and the
//! walk uses `FOR_EACH_TAIL` (`FOR_EACH_TAIL_SAFE` for plist-get), so a
//! circular plist signals `circular-list` with the same tail after the same
//! number of predicate calls. Refresh GNU expectations with
//! NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_plist_predicate_walks_reread_the_value_cell_after_the_call() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(list
              (let* ((pl (list 'a 1 'b 2))
                     (r (plist-get pl 'a (lambda (x y)
                                           (setcdr pl (list 99))
                                           (garbage-collect)
                                           (eq x y)))))
                (dotimes (_ 64) (list 1 2 3))
                (list r pl))
              (let* ((pl (list 'a 1 'b 2))
                     (r (plist-put pl 'a 42 (lambda (x y)
                                              (setcdr pl (list 99 'c 3))
                                              (garbage-collect)
                                              (eq x y)))))
                (list r pl))
              (let* ((pl (list 'a 1 'b 2))
                     (r (plist-member pl 'b (lambda (x y)
                                              (when (eq x 'a)
                                                (setcdr pl (list 7 'b 8))
                                                (garbage-collect))
                                              (eq x y)))))
                (list r pl))
              (let ((pl (list 'a 1 'b 2)))
                (condition-case err
                    (plist-put pl 'a 42 (lambda (x y) (setcdr pl nil) (eq x y)))
                  (error err)))
              (let ((pl (list 'a 1 'b 2)))
                (condition-case err
                    (plist-member pl 'q (lambda (_x _y) (setcdr pl 5) nil))
                  (error err)))
              (let ((pl (list 'a 1 'b 2)))
                (list (plist-member pl 'q (lambda (_x _y) (setcdr pl nil) nil)) pl))
              (plist-get (list 'a 1 'b 2) 'b #'eq)
              (let ((pl (list 'a 1))) (plist-put pl 'z 9 #'eq))
              (condition-case err (plist-put (list 'a 1 'b) 'z 9 #'eq) (error err))
              (condition-case err (plist-member (cons 'a (cons 1 5)) 'z #'eq) (error err)))"#;
    let expect = expect_test::expect![[
        r#""OK ((99 (a 99)) ((a 42 c 3) (a 42 c 3)) ((b 8) (a 7 b 8)) (wrong-type-argument consp nil) (wrong-type-argument plistp (a . 5)) (nil (a)) 2 (a 1 z 9) (wrong-type-argument plistp (a 1 b)) (wrong-type-argument plistp (a 1 . 5)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_plist_predicate_walks_on_circular_plists() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // Each entry: (plist-put plist-member plist-get) results for a plist of
    // N pairs whose last cdr points back at the head. A signal reports the
    // index of the tail it carries and the predicate call count.
    let form = r#"(let ((make (lambda (n)
                    (let ((l (make-list (* 2 n) nil)))
                      (dotimes (i (* 2 n))
                        (setcar (nthcdr i l)
                                (if (= 0 (% i 2)) (intern (format "k%d" (/ i 2))) i)))
                      (setcdr (last l) l)
                      l)))
                  (which (lambda (l obj)
                    (let ((i 0) (r nil))
                      (while (and (not r) (< i 64))
                        (when (eq (nthcdr i l) obj) (setq r i))
                        (setq i (1+ i)))
                      r))))
              (mapcar
               (lambda (n)
                 (let ((l (funcall make n)))
                   (mapcar
                    (lambda (op)
                      (let* ((calls 0)
                             (pred (lambda (a b) (setq calls (1+ calls)) (eq a b))))
                        (condition-case err
                            (list 'value (funcall op l pred) calls)
                          (circular-list
                           (list 'circular (funcall which l (cadr err)) calls)))))
                    (list (lambda (l pred) (and (plist-put l 'zz 9 pred) 'returned))
                          (lambda (l pred) (plist-member l 'zz pred))
                          (lambda (l pred) (plist-get l 'zz pred))))))
               '(1 2 3 5 8 13)))"#;
    let expect = expect_test::expect![[
        r#""OK (((circular 0 1) (circular 0 1) (value nil 1)) ((circular 0 4) (circular 0 4) (value nil 4)) ((circular 4 5) (circular 4 5) (value nil 5)) ((circular 2 11) (circular 2 11) (value nil 11)) ((circular 12 22) (circular 12 22) (value nil 22)) ((circular 2 27) (circular 2 27) (value nil 27)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_list_walks_detect_cycles_entered_late() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // GNU's FOR_EACH_TAIL Brent counter `q` is an unsigned short: cycles
    // entered past ~131K steps must still be found, by the same tail after
    // the same number of steps. Covers the PREDICATE and eq plist walks,
    // put on a circular symbol plist, and length.
    let form = r#"(let* ((make (lambda (pairs cycle-pairs)
                     (let ((l (make-list (* 2 pairs) 'v)) (i 0) (c nil))
                       (setq c l)
                       (while c
                         (setcar c (if (= 0 (% i 2)) (intern (format "k%d" (/ i 2))) i))
                         (setq c (cdr c) i (1+ i)))
                       (setcdr (last l) (nthcdr (* 2 (- pairs cycle-pairs)) l))
                       l)))
                    (index (lambda (l obj limit)
                      (let ((h (make-hash-table :test 'eq)) (i 0) (c l))
                        (while (< i limit) (puthash c i h) (setq c (cdr c) i (1+ i)))
                        (gethash obj h))))
                    (run (lambda (pairs cycle-pairs)
                      (let* ((l (funcall make pairs cycle-pairs)) (limit (* 2 pairs)) (calls 0)
                             (pred (lambda (a b) (setq calls (1+ calls)) (eq a b))))
                        (list pairs cycle-pairs
                         (condition-case err (progn (plist-put l 'zz 9 pred) 'returned)
                           (circular-list (prog1 (list 'circ (funcall index l (cadr err) limit) calls) (setq calls 0))))
                         (condition-case err (progn (plist-member l 'zz pred) 'returned)
                           (circular-list (prog1 (list 'circ (funcall index l (cadr err) limit) calls) (setq calls 0))))
                         (prog1 (list (plist-get l 'zz pred) calls) (setq calls 0))
                         (condition-case err (progn (plist-put l 'zz 9) 'returned)
                           (circular-list (list 'circ (funcall index l (cadr err) limit))))
                         (condition-case err (progn (plist-member l 'zz) 'returned)
                           (circular-list (list 'circ (funcall index l (cadr err) limit))))
                         (plist-get l 'zz)
                         (progn (setplist 'neovm--lc-sym l)
                                (prog1 (condition-case err (progn (put 'neovm--lc-sym 'zz 1) 'returned)
                                         (circular-list (list 'circ (funcall index l (cadr err) limit))))
                                  (setplist 'neovm--lc-sym nil)))
                         (let ((m (make-list (* 2 pairs) nil)))
                           (setcdr (last m) (nthcdr (* 2 (- pairs cycle-pairs)) m))
                           (condition-case err (length m)
                             (circular-list (list 'circ (funcall index m (cadr err) limit))))))))))
              (mapcar (lambda (spec) (apply run spec)) '((5 2) (131071 1) (140000 3000))))"#;
    let expect = expect_test::expect![[
        r#""OK ((5 2 (circ 8 8) (circ 8 8) (nil 8) (circ 8) (circ 8) nil (circ 8) (circ 6)) (131071 1 (circ 262140 131071) (circ 262140 131071) (nil 131071) (circ 262140) (circ 262140) nil (circ 262140) (circ 262140)) (140000 3000 (circ 278284 265142) (circ 278284 265142) (nil 265142) (circ 278284) (circ 278284) nil (circ 278284) (circ 278286)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
