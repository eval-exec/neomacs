//! The subrs org font-lock calls most (24.7K calls per operation) reach
//! their Rust implementation straight off the bytecode stack, the way GNU
//! `funcall_subr` dispatches `a0`..`a5` subrs and `exec_byte_code` runs
//! `Bpoint`..`Bwiden` inline — with no owned argument `Vec` per call.
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::tagged::header::SubrFn;

const HOT_FIXED_ARITY_SUBRS: [(&str, usize); 71] = [
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
    ("byte-to-position", 1),
    ("position-bytes", 1),
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
    // eieio's generated code and nbody's float math.
    ("assoc", 3),
    ("plist-get", 3),
    ("copy-sequence", 1),
    ("sqrt", 1),
    ("sin", 1),
    ("cos", 1),
    ("tan", 1),
    ("asin", 1),
    ("acos", 1),
    ("exp", 1),
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

/// assoc, plist-get, copy-sequence and the one-argument float functions as
/// fixed-arity subrs, and memq/assq/rassq under `symbols-with-pos-enabled`
/// (the byte compiler's setting): absent and nil optionals agree, arity and
/// type errors keep GNU's shape, and a symbol with position is `eq` to its
/// bare symbol from either side only. Expectation taken from GNU Emacs
/// 31.0.90 --batch.
#[test]
fn third_batch_and_symbols_with_pos_list_searches_match_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(&format!("(format \"%S\" {})", THIRD_BATCH_FORM))
        .unwrap_or_else(|error| {
            panic!(
                "third batch form: {}",
                crate::emacs_core::error::format_eval_result(&Err(error))
            )
        });
    assert_eq!(result.as_utf8_str(), Some(THIRD_BATCH_GNU));
}

const THIRD_BATCH_FORM: &str = r#"(list (assoc "b" '(("a" . 1) ("b" . 2))) (assoc 2 '((1 . a) (2 . b)) nil) (assoc "B" '(("a" . 1) ("b" . 2)) (lambda (a b) (string-equal (upcase a) (upcase b))))
      (assoc 'x '(a (x . 1))) (plist-get '(:a 1 :b 2) :b) (plist-get '(:a 1 "b" 2) "b" #'equal) (plist-get '(:a 1) :c nil)
      (copy-sequence "abc") (copy-sequence [1 2]) (copy-sequence nil) (copy-sequence '(1 2))
      (sqrt 4) (sin 0) (cos 0) (tan 0) (asin 0) (acos 1) (exp 0) (sqrt 2.25)
      (condition-case e (sqrt) (error e)) (condition-case e (sqrt 1 2) (error e)) (condition-case e (sqrt 'a) (error e)) (condition-case e (exp "x") (error e))
      (condition-case e (assoc 1) (error e)) (condition-case e (assoc 1 2 3 4) (error e)) (condition-case e (plist-get '(a)) (error e)) (condition-case e (copy-sequence) (error e)) (condition-case e (copy-sequence 1) (error e))
      (condition-case e (assoc 1 '((1 . 2) . 3)) (error e)) (condition-case e (assoc 5 '((1 . 2) . 3)) (error e))
      (func-arity 'assoc) (func-arity 'plist-get) (func-arity 'sqrt) (func-arity 'copy-sequence)
      (let ((symbols-with-pos-enabled t) (p (position-symbol 'foo 3)) (n (position-symbol nil 1)))
        (list (length (memq 'foo (list 1 p 'bar))) (length (memq p (list 'foo))) (length (memq p (list 1 (position-symbol 'foo 9))))
              (length (memq nil (list 1 n))) (memq 'bar (list p)) (length (memq 3 (list p 3))) (memq "s" (list p "s"))
              (cdr (assq 'foo (list 5 (cons p 1)))) (cdr (assq p (list (cons 'foo 2)))) (cdr (assq nil (list (cons n 5)))) (cdr (assq 7 (list (cons p 1) (cons 7 8)))) (assq 'q (list (cons p 1)))
              (car (rassq 'foo (list (cons 1 p)))) (car (rassq p (list (cons 2 'foo)))) (rassq 'baz (list (cons 1 p))) (car (rassq nil (list 3 (cons 4 n))))
              (condition-case e (memq 'zz '(1 2 . 3)) (error e)) (condition-case e (assq 'zz '((1 . 2) . 3)) (error e)) (condition-case e (rassq 'zz '((1 . 2) . 3)) (error e))
              (let ((l (list 1 2 3))) (setcdr (nthcdr 2 l) l) (condition-case e (memq 'zz l) (error (car e))))
              (let ((l (list '(1) '(2) '(3)))) (setcdr (nthcdr 2 l) l) (list (condition-case e (assq 'zz l) (error (car e))) (condition-case e (rassq 'zz l) (error (car e)))))
              (memq p (list 'foo p)) (assoc p (list (cons 'foo 9))))))"#;

const THIRD_BATCH_GNU: &str = r#"(("b" . 2) (2 . b) ("b" . 2) (x . 1) 2 2 nil "abc" [1 2] nil (1 2) 2.0 0.0 1.0 0.0 0.0 0.0 1.0 1.5 (wrong-number-of-arguments sqrt 0) (wrong-number-of-arguments sqrt 2) (wrong-type-argument numberp a) (wrong-type-argument numberp "x") (wrong-number-of-arguments assoc 1) (wrong-number-of-arguments assoc 4) (wrong-number-of-arguments plist-get 1) (wrong-number-of-arguments copy-sequence 0) (wrong-type-argument sequencep 1) (1 . 2) (wrong-type-argument listp ((1 . 2) . 3)) (2 . 3) (2 . 3) (1 . 1) (1 . 1) (2 1 1 1 nil 1 nil 1 2 5 8 nil 1 2 nil 4 (wrong-type-argument listp (1 2 . 3)) (wrong-type-argument listp ((1 . 2) . 3)) (wrong-type-argument listp ((1 . 2) . 3)) circular-list (circular-list circular-list) (foo #<symbol foo at 3>) (foo . 9)))"#;

/// The `memq`, `assq` and `rassq` scans with no cycle check answer what the
/// exact algorithms do —
/// the same tail, the same `circular-list` data, the same improper-list
/// object — for every list shape: a cycle with the match inside it or before
/// it, a cycle with no match, an improper end, and a list longer than the
/// scan's budget with the match at its end. With `symbols-with-pos-enabled`
/// both on and off.
#[test]
fn memq_scan_answers_as_the_exact_algorithm() {
    use crate::emacs_core::builtins::cons_list::{builtin_memq_values, memq_exact_for_test};
    use crate::emacs_core::value::Value;
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let describe = |result: &crate::emacs_core::error::EvalResult| match result {
        Ok(v) => format!("ok {:#x}", v.bits()),
        Err(crate::emacs_core::error::Flow::Signal(sig)) => format!(
            "signal {} {:?}",
            sig.symbol_name(),
            sig.data.iter().map(|v| v.bits()).collect::<Vec<_>>()
        ),
        Err(other) => format!("{other:?}"),
    };
    let positioned = eval
        .eval_str("(list (position-symbol 'b 4) (position-symbol 'c 9))")
        .expect("positioned symbols");
    crate::emacs_core::eval::push_scratch_gc_root(positioned);
    // Built without evaluating anything, so nothing collects meanwhile.
    let seq = |n: i64| {
        (1..=n)
            .rev()
            .fold(Value::NIL, |l, i| Value::cons(Value::fixnum(i), l))
    };
    let nthcdr = |mut l: Value, k: usize| {
        for _ in 0..k {
            l = l.cons_cdr();
        }
        l
    };
    let cyc = |n: i64, k: usize| {
        let l = seq(n);
        nthcdr(l, n as usize - 1).set_cdr(nthcdr(l, k));
        l
    };
    let improper = Value::cons(
        Value::fixnum(1),
        Value::cons(Value::fixnum(2), Value::fixnum(3)),
    );
    let with_positions = Value::cons(
        Value::symbol("a"),
        Value::cons(
            positioned.cons_car(),
            Value::cons(Value::symbol("c"), Value::NIL),
        ),
    );
    let lists = [
        Value::NIL,
        seq(3),
        improper,
        Value::cons(Value::fixnum(1), Value::fixnum(2)),
        Value::fixnum(5),
        cyc(1, 0),
        cyc(2, 0),
        cyc(2, 1),
        cyc(7, 3),
        cyc(40, 39),
        seq(70000),
        with_positions,
    ];
    for &list in &lists {
        crate::emacs_core::eval::push_scratch_gc_root(list);
    }
    let targets = [1, 2, 3, 5, 7, 39, 40, 41, 70000, 70001]
        .into_iter()
        .map(Value::fixnum)
        .chain([
            Value::symbol("a"),
            Value::symbol("b"),
            Value::symbol("zz"),
            positioned.cons_cdr().cons_car(),
            Value::NIL,
        ])
        .collect::<Vec<_>>();
    // The same shapes as alists: each element is `(i . i)`.
    let pairs = |l: Value| {
        let mut out = Value::NIL;
        let mut tail = l;
        let mut seen = 0;
        while tail.is_cons() && seen < 200 {
            let v = tail.cons_car();
            out = Value::cons(Value::cons(v, v), out);
            tail = tail.cons_cdr();
            seen += 1;
        }
        out
    };
    let alists: Vec<Value> = lists.iter().map(|&l| pairs(l)).collect();
    let cyclic_alist = {
        let l = pairs(seq(6));
        nthcdr(l, 5).set_cdr(nthcdr(l, 2));
        l
    };
    let improper_alist = Value::cons(
        Value::cons(Value::fixnum(1), Value::fixnum(1)),
        Value::fixnum(9),
    );
    let with_positions_alist = Value::cons(
        Value::cons(positioned.cons_car(), Value::symbol("a")),
        Value::cons(
            Value::cons(Value::symbol("c"), positioned.cons_cdr().cons_car()),
            Value::NIL,
        ),
    );
    for &list in &alists {
        crate::emacs_core::eval::push_scratch_gc_root(list);
    }
    for &list in &[cyclic_alist, improper_alist, with_positions_alist] {
        crate::emacs_core::eval::push_scratch_gc_root(list);
    }
    let mut checked = 0;
    for &list in &lists {
        for &target in &targets {
            for swp in [false, true] {
                let got = builtin_memq_values(target, list, swp);
                let want = memq_exact_for_test(target, list, swp);
                assert_eq!(describe(&got), describe(&want), "memq swp {swp}");
                checked += 1;
            }
        }
    }
    for list in alists
        .iter()
        .copied()
        .chain([cyclic_alist, improper_alist, with_positions_alist])
    {
        for &target in &targets {
            for swp in [false, true] {
                let got =
                    crate::emacs_core::builtins::cons_list::builtin_assq_values(target, list, swp);
                let want =
                    crate::emacs_core::builtins::cons_list::assq_exact_for_test(target, list, swp);
                assert_eq!(describe(&got), describe(&want), "assq swp {swp}");
                eval.symbols_with_pos_enabled = swp;
                let got = crate::emacs_core::misc::builtin_rassq_2(&mut eval, target, list);
                let want = crate::emacs_core::misc::rassq_exact_for_test(target, list, swp);
                assert_eq!(describe(&got), describe(&want), "rassq swp {swp}");
                eval.symbols_with_pos_enabled = false;
                checked += 2;
            }
        }
    }
    assert_eq!(checked, 12 * 15 * 2 + 15 * 15 * 2 * 2);
}
