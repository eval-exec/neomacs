use super::*;

#[test]
fn oracle_prop_gde_review_deletion_bulk_and_middle_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let (out)
(dolist (direction '(forward reverse))
(dolist (edit '(erase middle ends))
(with-temp-buffer
(insert (make-string 128 ?a))
(let ((i 0))
(dotimes (step 32)
(setq i (if (eq direction 'forward) step (- 31 step)))
(let ((o (if (eq edit 'ends) (make-overlay 1 (+ 2 (* i 4)))
(make-overlay (+ 1 (* i 4)) (+ 2 (* i 4)) nil (= (% i 3) 0) t))))
(overlay-put o 'tag i))))
(cond ((eq edit 'erase) (erase-buffer)) ((eq edit 'middle) (delete-region 33 97))
(t (delete-region 2 129)))
(let ((order (mapcar (lambda (o) (overlay-get o 'tag)) (car (overlay-lists)))))
(goto-char (point-min)) (insert "xy")
(push (list direction edit order
(mapcar (lambda (o) (overlay-get o 'tag)) (car (overlay-lists)))) out)))))
(nreverse out))"#;
    let expected = expect_test::expect![[
        r#""OK ((forward erase (0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31) (1 2 4 5 7 8 10 11 13 14 16 17 19 20 22 23 25 26 28 29 31)) (forward middle (0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31) (0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31)) (forward ends (31 30 29 28 27 26 25 24 23 22 21 20 19 18 17 16 15 14 13 12 11 10 9 8 7 6 5 4 3 2 1 0) (31 30 29 28 27 26 25 24 23 22 21 20 19 18 17 16 15 14 13 12 11 10 9 8 7 6 5 4 3 2 1 0)) (reverse erase (0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31) (1 2 4 5 7 8 10 11 13 14 16 17 19 20 22 23 25 26 28 29 31)) (reverse middle (0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31) (0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31)) (reverse ends (0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31) (0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expected);
}
