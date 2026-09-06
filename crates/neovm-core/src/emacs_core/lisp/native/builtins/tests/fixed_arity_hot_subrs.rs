//! The subrs org font-lock calls most (24.7K calls per operation) reach
//! their Rust implementation straight off the bytecode stack, the way GNU
//! `funcall_subr` dispatches `a0`..`a5` subrs and `exec_byte_code` runs
//! `Bpoint`..`Bwiden` inline — with no owned argument `Vec` per call.
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::tagged::header::SubrFn;

const HOT_FIXED_ARITY_SUBRS: [(&str, usize); 59] = [
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
    ("type-of", 1),
    ("parse-partial-sexp", 6),
    ("indirect-function", 2),
    ("intern-soft", 2),
    ("line-end-position", 1),
    ("skip-syntax-forward", 2),
    ("skip-syntax-backward", 2),
    ("char-after", 1),
    ("char-before", 1),
    ("set-syntax-table", 1),
    ("syntax-table", 0),
    ("subr-arity", 1),
    ("beginning-of-line", 1),
    ("end-of-line", 1),
    ("scan-sexps", 2),
    ("scan-lists", 3),
    ("text-property-not-all", 5),
    ("skip-chars-forward", 2),
    ("skip-chars-backward", 2),
    ("eolp", 0),
    ("eobp", 0),
    ("bolp", 0),
    ("bobp", 0),
    ("set-buffer", 1),
    ("match-data", 3),
    ("set-match-data", 2),
    ("line-beginning-position", 1),
    ("pos-bol", 1),
    ("pos-eol", 1),
    ("buffer-local-value", 2),
    ("add-text-properties", 4),
    ("next-single-property-change", 4),
    ("previous-single-property-change", 4),
    ("next-single-char-property-change", 4),
    ("narrow-to-region", 2),
    ("char-syntax", 1),
    ("delete-region", 2),
    ("buffer-modified-p", 1),
    ("marker-position", 1),
    ("set-marker", 3),
    ("text-properties-at", 2),
    ("remove-list-of-text-properties", 4),
    ("upcase", 1),
    ("downcase", 1),
    ("get-pos-property", 3),
    ("char-equal", 2),
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
            Some(SubrFn::A6(_)) => Some(6),
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

/// Second batch (syntax, navigation, symbols, markers, text properties):
/// absent optionals and explicit nil agree, and arity errors keep GNU's
/// shape. Expectation taken from GNU Emacs 31.0.90 --batch.
#[test]
fn second_batch_keeps_gnu_semantics_for_absent_and_nil_optionals() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(&format!("(format \"%S\" {})", FA2_FORM))
        .expect("spot-check form evaluates");
    assert_eq!(result.as_utf8_str(), Some(FA2_EXPECTED));
}

const FA2_FORM: &str = r#"(save-current-buffer
  (set-buffer (get-buffer-create " *fa2*"))
  (erase-buffer)
  (set-syntax-table (standard-syntax-table))
  (insert "(foo (bar) \"str\") ; c\nline2 word\n")
  (put-text-property 2 5 'face 'bold)
  (goto-char 1)
  (list
    (parse-partial-sexp 1 10)
    (equal (parse-partial-sexp 1 10) (parse-partial-sexp 1 10 nil nil nil nil))
    (scan-sexps 1 1) (scan-lists 1 1 0)
    (progn (goto-char 1) (skip-syntax-forward "(")) (progn (goto-char 1) (skip-syntax-forward "(" nil)) (progn (goto-char 1) (skip-chars-forward "(" nil) (point)) (progn (goto-char 2) (skip-chars-backward "(" nil))
    (progn (goto-char 3) (line-end-position)) (line-end-position nil) (pos-eol) (pos-bol nil) (progn (goto-char 5) (beginning-of-line) (point)) (progn (goto-char 5) (beginning-of-line nil) (point)) (progn (goto-char 5) (end-of-line) (point)) (progn (goto-char 5) (end-of-line nil) (point)) (line-beginning-position nil)
    (intern-soft "car") (intern-soft "car" nil) (intern-soft "no-such-symbol-xyz")
    (indirect-function 'car) (indirect-function 'car nil) (indirect-function 'no-such-fn-xyz)
    (type-of 1) (type-of "s") (subr-arity (symbol-function 'car))
    (progn (goto-char 1) (list (char-after) (char-after nil) (char-after 2) (char-before 2) (char-before nil) (char-before) (bolp) (eolp) (bobp) (eobp)))
    (progn (goto-char 1) (looking-at "(foo") (list (match-data) (match-data nil) (match-data t) (mapcar (lambda (m) (if (markerp m) (marker-position m) m)) (match-data)) (progn (set-match-data '(1 3)) (match-data)) (progn (set-match-data '(1 3) nil) (match-data))))
    (text-property-not-all 1 10 'face nil) (text-property-not-all 1 10 'face nil nil) (text-properties-at 2) (text-properties-at 2 nil)
    (progn (add-text-properties 6 8 '(x 1)) (add-text-properties 6 8 '(y 2) nil) (text-properties-at 6))
    (next-single-property-change 1 'face) (next-single-property-change 1 'face nil) (next-single-property-change 1 'face nil 3) (previous-single-property-change 10 'face) (previous-single-property-change 10 'face nil nil) (next-single-char-property-change 1 'face) (next-single-char-property-change 1 'face nil 3)
    (progn (remove-list-of-text-properties 6 8 '(x)) (text-properties-at 6)) (progn (remove-list-of-text-properties 6 8 '(y) nil) (text-properties-at 6))
    (get-pos-property 2 'face) (get-pos-property 2 'face nil)
    (let ((m (make-marker))) (list (marker-position m) (progn (set-marker m 3) (marker-position m)) (progn (set-marker m 4 nil) (marker-position m))))
    (buffer-modified-p) (buffer-modified-p nil) (buffer-local-value 'major-mode (current-buffer)) (char-syntax ?\() (char-syntax ?a) (eq (syntax-table) (standard-syntax-table))
    (upcase "ab") (downcase "AB") (upcase ?a) (char-equal ?a ?A) (char-equal ?a ?b)
    (progn (narrow-to-region 2 5) (prog1 (list (point-min) (point-max)) (widen)))
    (progn (delete-region 1 2) (buffer-substring-no-properties 1 5))
    (condition-case e (char-after 1 2) (error e)) (condition-case e (scan-sexps 1) (error e)) (condition-case e (parse-partial-sexp 1) (error e)) (condition-case e (intern-soft) (error e)) (condition-case e (set-marker (make-marker)) (error e))))"#;
const FA2_EXPECTED: &str = r#"((2 6 7 nil nil nil 0 nil nil (1 6) nil) t 18 18 1 1 2 -1 22 22 22 1 1 1 22 22 1 car car nil #<subr car> #<subr car> nil integer string (1 . 1) (40 40 102 40 nil nil t nil t nil) ((#<marker at 1 in  *fa2*> #<marker at 4 in  *fa2*>) (#<marker at 1 in  *fa2*> #<marker at 4 in  *fa2*>) (1 5 #<buffer  *fa2*>) (1 5) (1 3) (1 3)) 2 2 (face bold) (face bold) (y 2) 2 2 2 5 5 2 2 (y 2) nil nil nil (nil 3 4) t t fundamental-mode 40 119 t "AB" "ab" 65 t nil (2 5) "foo " (wrong-number-of-arguments char-after 2) (wrong-number-of-arguments scan-sexps 1) (wrong-number-of-arguments parse-partial-sexp 1) (wrong-number-of-arguments intern-soft 0) (wrong-number-of-arguments set-marker 1))"#;

/// The registry projects every plain builtin into the inline table at
/// registration; VM-special subrs and Lisp-only symbols stay out of it.
#[test]
fn inline_subr_table_mirrors_the_registry() {
    crate::test_utils::init_test_tracing();
    let _eval = Context::new();
    for name in [
        "car",
        "point",
        "widen",
        "get-char-property",
        "re-search-forward",
    ] {
        assert!(
            crate::emacs_core::eval::inline_subr_function(intern(name)).is_some(),
            "{name} is a plain builtin and must be inline-dispatchable"
        );
    }
    for name in crate::emacs_core::eval::VM_SPECIAL_BUILTIN_NAMES {
        assert!(
            crate::emacs_core::eval::inline_subr_function(intern(name)).is_none(),
            "{name} needs the VM-level implementation"
        );
    }
    assert!(crate::emacs_core::eval::inline_subr_function(intern("no-such-subr-xyz")).is_none());
}

/// Registration classifies each builtin the way GNU `exec_byte_code`
/// treats it: pure buffer reads run without a call, small fixed-arity subrs
/// are called straight off the stack, `aset`/`fillarray` keep the string
/// write-back path, and everything else takes the by-symbol dispatch.
#[test]
fn inline_subr_kinds_follow_the_gnu_inline_opcode_shape() {
    use crate::emacs_core::eval::{InlineSubrKind, inline_subr};
    crate::test_utils::init_test_tracing();
    let _eval = Context::new();
    let kind = |name: &str| inline_subr(intern(name)).kind;
    assert_eq!(kind("point"), InlineSubrKind::Point);
    assert_eq!(kind("point-min"), InlineSubrKind::PointMin);
    assert_eq!(kind("point-max"), InlineSubrKind::PointMax);
    assert_eq!(kind("current-buffer"), InlineSubrKind::CurrentBuffer);
    assert_eq!(kind("aset"), InlineSubrKind::Writeback);
    assert_eq!(kind("fillarray"), InlineSubrKind::Writeback);
    for name in [
        "car",
        "char-after",
        "goto-char",
        "widen",
        "bolp",
        "get-char-property",
    ] {
        assert_eq!(kind(name), InlineSubrKind::Direct, "{name}");
    }
    for name in [
        "re-search-forward",
        "put-text-property",
        "maphash",
        "garbage-collect",
        "no-such-subr-xyz",
    ] {
        assert_eq!(kind(name), InlineSubrKind::Generic, "{name}");
    }
}
