//! Lisp predicate walks that collect inside the callback: assoc TESTFN keeps
//! the matched entry alive, and plist-get/put/member re-read the value cell
//! after the call like GNU fns.c. Expected values are GNU 31.1 results; the
//! oracle twins live in neovm-oracle-tests (assoc/tests/callback_seams.rs,
//! property_list_predicate_walks.rs).
use crate::emacs_core::{Context, format_eval_result};

/// Evaluate `form` in a bare Context, then churn the cons allocator so a
/// cell freed by the in-callback collection is handed out again before the
/// result is printed.
fn eval_after_churn(form: &str) -> String {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let src = format!(
        "(let ((result {form}) (i 0))
           (while (< i 256) (list i i i) (setq i (1+ i)))
           result)"
    );
    format_eval_result(&ev.eval_str(&src))
}

#[test]
fn assoc_testfn_keeps_the_unlinked_entry_alive_across_a_collection() {
    let got = eval_after_churn(
        "(let* ((alist (list (cons 'k 'v) (cons 'other 'w)))
                (r (assoc 'k alist
                          (lambda (a b) (setcar alist nil) (garbage-collect) (eq a b)))))
           (list r alist))",
    );
    assert_eq!(got, "OK ((k . v) (nil (other . w)))");
}

#[test]
fn plist_get_predicate_reads_the_replaced_value_cell() {
    let got = eval_after_churn(
        "(let* ((pl (list 'a 1 'b 2))
                (r (plist-get pl 'a
                              (lambda (x y) (setcdr pl (list 99)) (garbage-collect) (eq x y)))))
           (list r pl))",
    );
    assert_eq!(got, "OK (99 (a 99))");
}

#[test]
fn plist_put_predicate_writes_the_replaced_value_cell() {
    let got = eval_after_churn(
        "(let* ((pl (list 'a 1 'b 2))
                (r (plist-put pl 'a 42
                              (lambda (x y) (setcdr pl (list 99 'c 3)) (garbage-collect) (eq x y)))))
           (list r pl))",
    );
    assert_eq!(got, "OK ((a 42 c 3) (a 42 c 3))");
    let got = eval_after_churn(
        "(let ((pl (list 'a 1 'b 2)))
           (condition-case err
               (plist-put pl 'a 42 (lambda (x y) (setcdr pl nil) (eq x y)))
             (error err)))",
    );
    assert_eq!(got, "OK (wrong-type-argument consp nil)");
}

#[test]
fn plist_member_predicate_advances_through_the_replaced_value_cell() {
    let got = eval_after_churn(
        "(let* ((pl (list 'a 1 'b 2))
                (r (plist-member pl 'b
                                 (lambda (x y)
                                   (if (eq x 'a)
                                       (progn (setcdr pl (list 7 'b 8)) (garbage-collect)))
                                   (eq x y)))))
           (list r pl))",
    );
    assert_eq!(got, "OK ((b 8) (a 7 b 8))");
    let got = eval_after_churn(
        "(let ((pl (list 'a 1 'b 2)))
           (condition-case err
               (plist-member pl 'q (lambda (x y) (setcdr pl 5) nil))
             (error err)))",
    );
    assert_eq!(got, "OK (wrong-type-argument plistp (a . 5))");
}

#[test]
fn plist_predicate_walks_signal_or_stop_on_circular_plists() {
    // Two pairs whose last cdr points at the head: GNU signals with the
    // head after four predicate calls (plist-get stops quietly).
    let got = eval_after_churn(
        "(let* ((l (list 'k0 0 'k1 1))
                (calls 0)
                (pred (lambda (a b) (setq calls (1+ calls)) (eq a b))))
           (setcdr (nthcdr 3 l) l)
           (list
            (condition-case err (plist-put l 'zz 9 pred)
              (circular-list (list (eq (car (cdr err)) l) (prog1 calls (setq calls 0)))))
            (condition-case err (plist-member l 'zz pred)
              (circular-list (list (eq (car (cdr err)) l) (prog1 calls (setq calls 0)))))
            (list (plist-get l 'zz pred) calls)))",
    );
    assert_eq!(got, "OK ((t 4) (t 4) (nil 4))");
}

#[test]
fn list_walks_detect_a_cycle_entered_past_131k_steps() {
    // GNU's Brent counter is an unsigned short; with a wider one the
    // tortoise froze at step 131070 and these walks spun forever.
    let got = eval_after_churn(
        "(let ((m (make-list 280000 nil)))
           (setcdr (nthcdr 279999 m) (nthcdr 279998 m))
           (condition-case nil (length m) (circular-list 'circ)))",
    );
    assert_eq!(got, "OK circ");
}

#[test]
fn eq_plist_walks_and_put_signal_on_circular_plists() {
    // GNU plist_put and plist_member walk with FOR_EACH_TAIL.
    let got = eval_after_churn(
        "(let ((l (list 'a 1 'b 2)))
           (setcdr (nthcdr 3 l) l)
           (setplist 'neovm--circular-plist-sym l)
           (prog1
               (list
                (condition-case err (plist-put l 'zz 9) (circular-list (eq (car (cdr err)) l)))
                (condition-case err (plist-member l 'zz) (circular-list (eq (car (cdr err)) l)))
                (plist-get l 'zz)
                (condition-case err (put 'neovm--circular-plist-sym 'zz 9)
                  (circular-list (eq (car (cdr err)) l))))
             (setplist 'neovm--circular-plist-sym nil)))",
    );
    assert_eq!(got, "OK (t t nil t)");
}
