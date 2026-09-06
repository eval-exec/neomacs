//! The subrs org font-lock calls most (24.7K calls per operation) reach
//! their Rust implementation straight off the bytecode stack, the way GNU
//! `funcall_subr` dispatches `a0`..`a5` subrs and `exec_byte_code` runs
//! `Bpoint`..`Bwiden` inline — with no owned argument `Vec` per call.
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::tagged::header::SubrFn;

const HOT_FIXED_ARITY_SUBRS: [(&str, usize); 13] = [
    ("get-char-property", 3),
    ("match-beginning", 1),
    ("widen", 0),
    ("current-buffer", 0),
    ("put-text-property", 5),
    ("re-search-forward", 4),
    ("search-forward", 4),
    ("looking-at", 2),
    ("buffer-substring", 2),
    ("match-end", 1),
    ("get-text-property", 3),
    ("forward-line", 1),
    ("forward-char", 1),
];

#[test]
fn hot_subrs_dispatch_with_fixed_arity_like_gnu_funcall_subr() {
    crate::test_utils::init_test_tracing();
    let _eval = Context::new();
    for (name, arity) in HOT_FIXED_ARITY_SUBRS {
        let entry = crate::emacs_core::eval::lookup_global_subr_entry(intern(name))
            .unwrap_or_else(|| panic!("{name} must be a registered subr"));
        let fixed = match entry.function {
            Some(SubrFn::A0(_)) => Some(0),
            Some(SubrFn::A1(_)) => Some(1),
            Some(SubrFn::A2(_)) => Some(2),
            Some(SubrFn::A3(_)) => Some(3),
            Some(SubrFn::A4(_)) => Some(4),
            Some(SubrFn::A5(_)) => Some(5),
            _ => None,
        };
        assert_eq!(
            fixed,
            Some(arity),
            "{name} must take its arguments off the stack (GNU funcall_subr a{arity}), not an owned Vec"
        );
    }
}

/// Absent optionals and explicit nil must behave identically (GNU fills
/// missing arguments with nil before calling the subr), and arity errors
/// keep GNU's shape. Expectation taken from GNU Emacs 31.0.90 --batch.
#[test]
fn hot_subrs_keep_gnu_semantics_for_absent_and_nil_optionals() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(
            r#"(format "%S"
 (save-current-buffer
  (set-buffer (get-buffer-create " *fixed-arity*"))
  (insert "hello world\nsecond line\n")
  (put-text-property 1 6 'face 'bold)
  (put-text-property 7 12 'x 1 (current-buffer))
  (goto-char 1)
  (list (get-char-property 1 'face) (get-char-property 7 'x nil) (get-text-property 1 'face) (get-text-property 7 'x (current-buffer))
        (re-search-forward "wor" nil t) (match-beginning 0) (match-end 0)
        (progn (goto-char 1) (search-forward "o" nil t 2))
        (progn (goto-char 1) (looking-at "hel")) (looking-at "xyz" t)
        (buffer-substring 1 6) (progn (goto-char 1) (forward-line) (point)) (progn (goto-char 1) (forward-line nil) (point))
        (progn (goto-char 1) (forward-char) (point)) (progn (goto-char 1) (forward-char nil) (point)) (progn (goto-char 1) (forward-char 3) (point))
        (progn (narrow-to-region 1 6) (widen) (point-max)) (bufferp (current-buffer))
        (condition-case e (re-search-forward "zzz") (error (car e))) (progn (goto-char 1) (re-search-forward "l+" nil nil 2) (match-beginning 0))
        (condition-case e (widen 1) (error e)) (condition-case e (forward-line 1 2) (error e)) (condition-case e (match-beginning) (error e))
        (condition-case e (get-char-property 1) (error e)) (condition-case e (put-text-property 1 2 'a) (error e)) (condition-case e (buffer-substring 1) (error e)))))"#,
        )
        .expect("spot-check form evaluates");
    assert_eq!(
        result.as_utf8_str(),
        Some(
            "(bold 1 bold 1 10 7 10 9 t nil #(\"hello\" 0 5 (face bold)) 13 13 2 2 4 25 t search-failed 10 (wrong-number-of-arguments widen 1) (wrong-number-of-arguments forward-line 2) (wrong-number-of-arguments match-beginning 0) (wrong-number-of-arguments get-char-property 1) (wrong-number-of-arguments put-text-property 3) (wrong-number-of-arguments buffer-substring 1))"
        )
    );
}
