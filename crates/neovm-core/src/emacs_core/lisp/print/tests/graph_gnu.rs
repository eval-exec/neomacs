//! GNU 31.1 printer graph regressions. Refresh with UPDATE_EXPECT=1.
//! Each evaluator and its graph belong to the calling mutator.
//! Campaign probes run the containing nextest driver through sandbox-run.sh;
//! NEOMACS_PROBE_SANDBOX optionally also wraps the GNU refresh subprocess.
use crate::emacs_core::print::print_value;
use std::path::PathBuf;

fn compare(name: &str, form: &str, saved: &str) {
    crate::test_utils::init_test_tracing();
    let expected = if std::env::var("UPDATE_EXPECT").as_deref() == Ok("1") {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let script = root.join(format!("../../tmp/graph-{name}-{}.el", std::process::id()));
        std::fs::write(
            &script,
            format!(
                "(let ((print-circle nil) (print-level nil) (print-length nil)) (prin1 {form}))"
            ),
        )
        .unwrap();
        let emacs = PathBuf::from(std::env::var_os("HOME").unwrap()).join(".local/bin/emacs");
        let mut command = match std::env::var_os("NEOMACS_PROBE_SANDBOX") {
            Some(sandbox) => {
                let mut command = std::process::Command::new(sandbox);
                command.arg(emacs);
                command
            }
            None => std::process::Command::new(emacs),
        };
        let output = command
            .args(["-Q", "--batch", "-l"])
            .arg(script)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "GNU: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected = String::from_utf8(output.stdout).unwrap();
        let fixture = root.join(format!(
            "src/emacs_core/lisp/print/tests/graph_{name}.expect"
        ));
        if std::fs::read_to_string(&fixture).ok().as_deref() != Some(expected.as_str()) {
            std::fs::write(fixture, &expected).unwrap();
        }
        expected
    } else {
        saved.to_owned()
    };
    let mut ctx = crate::test_utils::runtime_startup_context();
    let actual = ctx.eval_str(form).expect("printer observation");
    assert_eq!(print_value(&actual), expected.trim_end(), "{name}");
}

#[test]
fn char_table_graph_and_limits() {
    compare(
        "chartable",
        r##"(let ((ct (make-char-table 'foo)) (x (list 1)))
      (aset ct ?a x) (aset ct ?b x)
      (list (let ((print-circle t)) (prin1-to-string ct))
            (let ((print-length 3)) (prin1-to-string ct))
            (let ((print-length 70)) (length (prin1-to-string ct)))))"##,
        include_str!("graph_chartable.expect"),
    );
}

#[test]
fn char_table_self_containment() {
    compare(
        "self",
        r##"(let ((ct (make-char-table 'foo))) (aset ct ?a ct)
      (list (length (prin1-to-string ct))
            (let ((print-circle t)) (length (prin1-to-string ct)))))"##,
        include_str!("graph_self.expect"),
    );
}

#[test]
fn object_level_accounting() {
    compare(
        "level",
        r##"(list
      (let ((print-level 1)) (prin1-to-string (propertize "abc" 'face 'bold)))
      (let ((print-level 1)) (prin1-to-string '(\` (a (\, b)))))
      (let ((print-level 3)) (prin1-to-string '(\` (a (\, (b c))))))
      (let ((print-level -1)) (prin1-to-string '(a (b))))
      (let ((print-level 2)) (prin1-to-string (list (propertize "abc" 'face 'bold)))))"##,
        include_str!("graph_level.expect"),
    );
}

#[test]
fn closure_slot_limits() {
    compare(
        "closure",
        r##"(list
      (let ((print-length 1)) (prin1-to-string (car (read-from-string "#[(x y) ((list x) y) (t)]"))))
      (let ((print-level 1)) (prin1-to-string (car (read-from-string "#[(x) ((list x)) (t)]")))))"##,
        include_str!("graph_closure.expect"),
    );
}

#[test]
fn circular_tail_uses_cons_identity() {
    compare(
        "tail",
        r##"(list
      (let ((x (list 'a 'a 'a 'a))) (setcdr (nthcdr 3 x) (cdr x)) (prin1-to-string x))
      (let ((x (list 'a 'b 'a 'b 'a 'b))) (setcdr (nthcdr 5 x) (nthcdr 2 x)) (prin1-to-string x)))"##,
        include_str!("graph_tail.expect"),
    );
}

#[test]
fn depth_limit_matches_gnu() {
    compare(
        "depth",
        r##"(let (results)
      (dolist (kind '(list vector record backquote))
        (dolist (depth '(198 199 200))
          (let ((x nil))
            (dotimes (_ depth)
              (setq x (cond ((eq kind 'list) (list x))
                            ((eq kind 'vector) (vector x))
                            ((eq kind 'record) (record 'foo x))
                            (t (list '\` x)))))
            (push (condition-case e (length (prin1-to-string x)) (error e)) results))))
      (nreverse results))"##,
        include_str!("graph_depth.expect"),
    );
}

#[test]
fn number_table_survives_print() {
    compare(
        "numbers",
        r##"(list
          (let ((print-circle t) (print-continuous-numbering nil) (print-number-table nil) (x (list 1)))
            (prin1-to-string (list x x)) print-number-table)
          (let ((print-circle t) (print-continuous-numbering nil) (print-number-table nil) (x (list 1)))
            (prin1-to-string (list x x))
            (let ((retained print-number-table))
              (let ((print-circle nil)) (prin1-to-string 'x))
              (list retained print-number-table)))
          (let ((print-circle t) (print-continuous-numbering nil) (print-number-table nil))
            (prin1-to-string 17) print-number-table)
          (let ((print-circle t) (print-continuous-numbering nil) (print-number-table nil))
            (prin1-to-string []) print-number-table)
          (let ((print-circle t) (print-continuous-numbering nil) (print-number-table nil))
            (prin1-to-string (make-marker)) print-number-table)
          (let ((print-circle nil) (print-continuous-numbering nil) (print-number-table nil) (x (list 1)))
            (prin1-to-string (list x x) nil '((circle . t))) print-number-table)
          (let ((print-circle t) (print-continuous-numbering nil) (print-number-table '(prior)))
            (condition-case e (prin1-to-string '(1) nil '((bogus . t))) (error nil))
            print-number-table))"##,
        include_str!("graph_numbers.expect"),
    );
}

#[test]
fn standalone_circular_tail_uses_gnu_brent_identity() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::test_utils::runtime_startup_context();
    let fixture = ctx
        .eval_str(&format!("(quote {})", include_str!("graph_tail.expect")))
        .unwrap();
    let expected = crate::emacs_core::value::list_to_vec(&fixture).unwrap();
    for (form, expected) in [
        "(let ((x (list 'a 'a 'a 'a))) (setcdr (nthcdr 3 x) (cdr x)) x)",
        "(let ((x (list 'a 'b 'a 'b 'a 'b))) (setcdr (nthcdr 5 x) (nthcdr 2 x)) x)",
    ]
    .into_iter()
    .zip(expected)
    {
        let value = ctx.eval_str(form).unwrap();
        assert_eq!(
            print_value(&value),
            expected.as_lisp_string().unwrap().as_utf8_str().unwrap()
        );
    }
}

#[test]
fn closure_sharing_preserves_slot_identity() {
    compare(
        "closure_sharing",
        r###"(let ((print-circle t)) (prin1-to-string (car (read-from-string "(#1=(x) #[#1# ((list x)) (t)])"))))"###,
        include_str!("graph_closure_sharing.expect"),
    );
}

#[test]
fn hash_callbacks_read_live_physical_slots() {
    compare(
        "hash_mutation",
        r###"(list
  (let* ((h (make-hash-table :test 'eq))
         (print-unreadable-function (lambda (_o _e) (puthash 'second 'after h) "Q")))
    (puthash 'first (make-marker) h) (puthash 'second 'before h)
    (prin1-to-string h))
  (let* ((h (make-hash-table :test 'eq)) (key (make-marker))
         (print-unreadable-function (lambda (_o _e) (puthash key 'after h) (puthash 'inserted 'later h) "Q")))
    (puthash key 'before h)
    (prin1-to-string h))
  (let* ((h (make-hash-table :test 'eq)) (key (make-marker))
         (print-unreadable-function (lambda (o _e) (remhash o h) (setq key nil) (garbage-collect) "Q")))
    (puthash key 'before h)
    (prin1-to-string h))
  (let* ((h (make-hash-table :test 'eq))
         (print-unreadable-function (lambda (_o _e) (puthash 'inserted 'earlier-slot h) (puthash 'second 'after h) "Q")))
    (puthash 'vacant 'removed h) (puthash 'first (make-marker) h) (puthash 'second 'before h)
    (remhash 'vacant h)
    (prin1-to-string h)))"###,
        include_str!("graph_hash_mutation.expect"),
    );
}

// GNU print.c:1324, 1700-1733, 2611-2613: nested closure print has its
// own traversal while the unreadable callback is dynamically disabled.
// Append this fragment to tests/graph_gnu.rs; fixture comes only from GNU.
#[test]
fn closure_reentry_does_not_share_traversal_stack() {
    compare(
        "closure_reentry",
        r##"(mapcar
  (lambda (circle)
    (let* ((marker (make-marker))
           (closure (eval '(lambda () x) (list (cons 'x marker))))
           (nested nil)
           (print-circle circle)
           (print-unreadable-function
            (lambda (_object _escaped)
              ;; GNU binds this callback to nil during the recursive print.
              (setq nested (prin1-to-string closure))
              "Q")))
      (progn (prin1-to-string closure) nested)))
  '(nil t))"##,
        include_str!("graph_closure_reentry.expect"),
    );
}
