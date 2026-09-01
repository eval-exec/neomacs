mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};
use neovm_core::emacs_core::value_reader;

fn test_ob() -> neovm_core::emacs_core::symbol::Obarray {
    neovm_core::emacs_core::symbol::Obarray::new()
}

const EMPTY_OVERLAY_QUERIES_AND_CROSS_BUFFER_MOVE_FORM: &str = r#"(let ((a (get-buffer-create " *compat-overlay-a*"))
      (b (get-buffer-create " *compat-overlay-b*")))
  (unwind-protect
      (progn
        (with-current-buffer a
          (erase-buffer)
          (insert "abcdef"))
        (with-current-buffer b
          (erase-buffer)
          (insert "uvwxyz"))
        (let ((empty (make-overlay 2 2 a))
              (movable (make-overlay 2 4 a))
              (project
               (lambda (ovs)
                 (sort (mapcar (lambda (ov)
                                 (list (overlay-start ov) (overlay-end ov)))
                               ovs)
                       (lambda (lhs rhs)
                         (if (= (car lhs) (car rhs))
                             (< (cadr lhs) (cadr rhs))
                           (< (car lhs) (car rhs))))))))
          (move-overlay movable 1 3 b)
          (list
           (funcall project (overlays-in 2 2))
           (buffer-name (overlay-buffer movable))
           (overlay-start movable)
           (overlay-end movable)
           (with-current-buffer a
             (funcall project (overlays-in 1 6)))
           (with-current-buffer b
             (funcall project (overlays-in 1 3))))))
    (kill-buffer b)
    (kill-buffer a)))"#;

struct OverlayCase {
    name: &'static str,
    form: &'static str,
}

#[test]
fn compat_overlay_semantics_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping overlay semantics audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let cases = [
        OverlayCase {
            name: "priority_and_boundary_queries",
            form: r#"(let ((buf (get-buffer-create " *compat-overlay-priority*")))
  (unwind-protect
      (with-current-buffer buf
        (erase-buffer)
        (insert "abcdefgh")
        (let ((a (make-overlay 2 7 buf))
              (b (make-overlay 4 7 buf))
              (c (make-overlay 4 5 buf)))
          (overlay-put a 'face 'bold)
          (overlay-put a 'priority 1)
          (overlay-put b 'face 'italic)
          (overlay-put b 'priority '(1 . 2))
          (overlay-put c 'face 'underline)
          (overlay-put c 'priority '(1 . 3))
          (list
           (mapcar (lambda (ov)
                     (list (overlay-start ov)
                           (overlay-end ov)
                           (overlay-get ov 'face)
                           (overlay-get ov 'priority)))
                   (overlays-at 4 t))
           (let* ((pair (get-char-property-and-overlay 4 'face))
                  (ov (cdr pair)))
             (list (car pair)
                   (and ov (overlay-start ov))
                   (and ov (overlay-end ov))))
           (list (next-overlay-change 1)
                 (next-overlay-change 4)
                 (previous-overlay-change 7)))))
    (kill-buffer buf)))"#,
        },
        OverlayCase {
            name: "empty_overlay_queries_and_cross_buffer_move",
            form: EMPTY_OVERLAY_QUERIES_AND_CROSS_BUFFER_MOVE_FORM,
        },
        OverlayCase {
            name: "deleted_overlay_identity_and_plist",
            form: r#"(let ((buf (get-buffer-create " *compat-overlay-deleted*")))
  (unwind-protect
      (with-current-buffer buf
        (erase-buffer)
        (insert "abcd")
        (let ((ov (make-overlay 1 2 buf)))
          (overlay-put ov 'face 'bold)
          (let ((live (prin1-to-string ov)))
            (delete-overlay ov)
            (overlay-put ov 'face 'italic)
            (list
             (overlayp ov)
             (eq ov ov)
             live
             (prin1-to-string ov)
             (overlay-buffer ov)
             (overlay-start ov)
             (overlay-end ov)
             (overlay-get ov 'face)
             (overlay-properties ov)))))
    (kill-buffer buf)))"#,
        },
    ];

    for case in cases {
        let gnu = run_oracle_eval(case.form).expect("GNU Emacs evaluation");
        let neovm = run_neovm_eval(case.form).expect("NeoVM evaluation");
        assert_eq!(
            neovm, gnu,
            "overlay semantics mismatch for {}:\nGNU: {}\nNeoVM: {}",
            case.name, gnu, neovm
        );
    }
}

#[test]
fn empty_overlay_cross_buffer_form_parses_in_neovm() {
    // value_reader::read_all allocates cons cells on the tagged heap,
    // which requires a TaggedHeap to be active on the current thread.
    // The simplest way to install one for an integration test is to
    // construct an empty Context.
    let _eval = neovm_core::emacs_core::eval::Context::new();
    let forms =
        value_reader::read_all(EMPTY_OVERLAY_QUERIES_AND_CROSS_BUFFER_MOVE_FORM, &test_ob())
            .expect("overlay audit form should parse");
    assert_eq!(forms.len(), 1);
}
