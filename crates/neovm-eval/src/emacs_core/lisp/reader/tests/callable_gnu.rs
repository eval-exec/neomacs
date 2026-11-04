//! GNU source_function_get/unget (lread.c:486-515) and invalid_syntax_lisp
//! (lread.c:636-652). Fixtures are refreshed exclusively from GNU 31.1.
use crate::test_utils::runtime_startup_eval_one;
use std::path::PathBuf;

const CALLABLE: &str = r###"(progn (defvar gdj-input nil) (let ((gdj-input nil))
  (defun gdj-read-source (&optional c)
    (if c (setq gdj-input (cons c gdj-input))
      (let ((ch (car gdj-input))) (setq gdj-input (cdr gdj-input)) ch)))
  (list
    (mapcar (lambda (source)
      (setq gdj-input (string-to-list source))
      (list (read 'gdj-read-source) gdj-input))
      '("foo " "(a b) rest" "(x . y)" "[1 2]rest" "\"hello\"rest" "?a " "#x12_" "'foo " "#1=(a)x" ";comment\n(a)" "#(\"abc\" 0 1 (a b))rest"))
    (let ((gdj-input (string-to-list "λfoo ")))
      (list (read (lambda (&optional c) (gdj-read-source c))) gdj-input))
    (let ((gdj-input (string-to-list "(a b) ")))
      (list (read (byte-compile (lambda (&optional c) (gdj-read-source c)))) gdj-input))
    (condition-case e (read 'ignore) (error e))
    (condition-case e (read 1) (error e))
    (let ((n 0))
      (condition-case e (read (lambda () (setq n (1+ n)) (error "callback error")))
        (error (list e n)))))))"###;

const COLUMNS: &str = r###"(mapcar (lambda (spec)
  (with-temp-buffer
    (setq tab-width (car spec))
    (insert (cadr spec)) (goto-char 1)
    (condition-case e (while t (read (current-buffer)))
      (error (list e (point))))))
  '((8 "x\n\ty )") (3 "x\n\ty )") (8 "x\n中 )") (8 "x\nλ )") (8 "x\n  y )")))"###;

const TRACE: &str = r###"(list
 (mapcar (lambda (source)
   (let ((input (string-to-list source)) (trace nil))
     (let ((result (condition-case e
       (read (lambda (&optional c)
         (setq trace (cons c trace))
         (if c (setq input (cons c input))
           (let ((ch (car input))) (setq input (cdr input)) ch))))
       (error e))))
       (list (cond ((stringp result) (append result nil))
                   ((bool-vector-p result) (append result nil))
                   (t result)) (reverse trace) input))))
   '("foo " "(a)" "(a b)" "(a . b)" "(.a)" "(a .b)" "?a " "? " "?\\s " "?\\x41 " "?\\1 " "\"\\x41;x\"" "\"\\C-\\s!\"" ",a " ".?" "#x12_" "#^x" "#:" "#_ " "#&1\"a\""))
 (let ((input (string-to-list "(a b)")))
   (read (lambda (&optional c)
     (if c (setq input (cons (if (= c 32) 41 c) input))
       (let ((ch (car input))) (setq input (cdr input)) ch)))))
 (let ((input (string-to-list "(a)")))
   (condition-case e
     (read (lambda (&optional c)
       (if c (error "unget blocked")
         (let ((ch (car input))) (setq input (cdr input)) ch))))
     (error e)))
 (let ((input (string-to-list "gdj-old-name ")) (read-symbol-shorthands nil))
   (read (lambda (&optional c)
     (if c (progn (setq read-symbol-shorthands '(("gdj-old-" . "gdj-new-")))
                   (setq input (cons c input)))
       (let ((ch (car input))) (setq input (cdr input)) ch)))))
 (mapcar (lambda (stream)
   (let ((input stream) (trace nil))
     (let ((result (condition-case e
       (read (lambda (&optional c)
         (setq trace (cons c trace))
         (if c (setq input (cons c input))
           (let ((ch (car input))) (setq input (cdr input)) ch))))
       (error e))))
       (list (cond ((stringp result) (append result nil))
                   ((bool-vector-p result) (append result nil))
                   (t result)) (reverse trace) input))))
   '((40 97 nil 41) (40 97 nil nil) (40 97 nil 98 41)))
 (let ((input (string-to-list "(.a)")))
   (read (lambda (&optional c)
     (if c (setq input (cons (if (= c 97) 32 c) input))
       (let ((ch (car input))) (setq input (cdr input)) ch)))))
 (mapcar (lambda (mode)
   (let* ((name (if (eq mode 'get) "gdj-callback-get-uninterned" "gdj-callback-unget-uninterned"))
          (input (string-to-list (concat name " "))))
     (list
       (condition-case e
         (read (lambda (&optional c)
           (if c
             (if (eq mode 'unget) (error "unget blocked")
               (setq input (cons c input)))
             (let ((ch (car input)))
               (setq input (cdr input))
               (if (and (eq mode 'get) (eq ch 32)) (error "get blocked") ch)))))
         (error e))
       (intern-soft name))))
   '(get unget))
 )"###;

fn assert_gnu(name: &str, form: &str, fixture: &str) {
    crate::test_utils::init_test_tracing();
    let expected = if std::env::var("UPDATE_EXPECT").as_deref() == Ok("1") {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let script = root
            .join("../../tmp")
            .join(format!("callable-{name}-{}.el", std::process::id()));
        std::fs::create_dir_all(script.parent().unwrap()).unwrap();
        std::fs::write(&script, format!(";;; -*- lexical-binding: nil; -*-\n(require 'bytecomp)\n(let ((print-circle t)) (prin1 {form}))")).unwrap();
        let output = crate::test_utils::gnu_refresh_command()
            .args(["-Q", "--batch", "-l"])
            .arg(script)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = format!("OK {}", String::from_utf8(output.stdout).unwrap());
        let path = root
            .join("src/emacs_core/lisp/reader/tests")
            .join(format!("callable-{name}.expect"));
        let saved = format!("{text}\n");
        if std::fs::read_to_string(&path).ok().as_deref() != Some(saved.as_str()) {
            std::fs::write(path, saved).unwrap();
        }
        text
    } else {
        fixture.trim_end().to_owned()
    };
    assert_eq!(runtime_startup_eval_one(form), expected, "{name}");
}

#[test]
fn callable_reader_gnu_get_unget() {
    assert_gnu("streams", CALLABLE, include_str!("callable-streams.expect"));
}

#[test]
fn reader_error_columns_gnu_display_width() {
    assert_gnu("columns", COLUMNS, include_str!("callable-columns.expect"));
}

const LONG_GC: &str = r#"(let ((input (string-to-list (concat "(" (make-string 10000 ?\s) "a \"xyz\" [1 2]) rest"))) (calls 0))
      (list (read (lambda (&optional c)
          (if c (setq input (cons c input))
            (let ((ch (car input)))
              (setq calls (1+ calls) input (cdr input))
              (when (= (% calls 1000) 0) (garbage-collect)) ch))))
        (length input)))"#;

#[test]
fn callable_reader_long_stream_and_gc() {
    assert_gnu("long-gc", LONG_GC, include_str!("callable-long-gc.expect"));
}

#[test]
fn callable_reader_gnu_callback_trace() {
    assert_gnu("trace", TRACE, include_str!("callable-trace.expect"));
}

const CHARACTER_DOMAIN: &str = r###"(mapcar (lambda (code)
  (let ((input (list 63 code 32)))
    (condition-case e (read (lambda (&optional c)
      (if c (setq input (cons c input))
        (let ((ch (car input))) (setq input (cdr input)) ch)))) (error e))))
  '(0 127 128 4194303 4194304 -1 9000000000000000000 nil t 1.0 "a" (a) [97]))"###;

#[test]
fn callable_reader_gnu_character_domain() {
    assert_gnu(
        "domain",
        CHARACTER_DOMAIN,
        include_str!("callable-domain.expect"),
    );
}

const LABEL_GC: &str = r###"(let ((input (string-to-list "(#s(hash-table data (a #1=\"foo\" a \"bar\")) #1#)")) (collections 0))
  (let ((value (read (lambda (&optional c)
    (if c (setq input (cons c input))
      (let ((ch (car input)))
        (setq input (cdr input) collections (1+ collections))
        (garbage-collect) ch))))))
    (list (gethash 'a (car value)) (cadr value) (> collections 10))))"###;

#[test]
fn callable_reader_gnu_retains_discarded_label_values_across_gc() {
    assert_gnu(
        "label-gc",
        LABEL_GC,
        include_str!("callable-label-gc.expect"),
    );
}
