use crate::emacs_core::format_eval_result;

#[test]
fn backtrace_frame_internal_tracks_runtime_funcall_interactively_marker() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let results = ev.eval_str_each(
        r#"
        (progn
          (fset 'neovm--misc-bt-target
                (lambda ()
                  (interactive)
                  (let (frame)
                    (backtrace-frame--internal
                     (lambda (evald func args flags)
                       (setq frame (list evald func args flags)))
                     1
                     'neovm--misc-bt-target)
                    (nth 1 frame))))
          (unwind-protect
              (list
               (funcall-interactively 'neovm--misc-bt-target)
               (call-interactively 'neovm--misc-bt-target))
            (fmakunbound 'neovm--misc-bt-target)))
        "#,
    );
    assert_eq!(
        results.iter().map(format_eval_result).collect::<Vec<_>>(),
        vec!["OK (funcall-interactively funcall-interactively)"]
    );
}
