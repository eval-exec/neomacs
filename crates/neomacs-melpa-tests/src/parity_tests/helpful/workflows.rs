use expect_test::expect;

use super::ParityBatchCase;

fn pretty_print_and_indent_are_stable() -> ParityBatchCase {
    ParityBatchCase::value(
        "pretty_print_and_indent_are_stable",
        r####"
(list :pp (helpful--pretty-print '(1 2 3))
      :indented (helpful--indent-rigidly "foo\nbar" 2)
      :kind-fn (helpful--kind-name 'car t)
      :kind-var (helpful--kind-name 'load-path nil))
"####,
        expect![[
            r#"OK (:pp "(1 2 3)" :indented "  foo\n  bar" :kind-fn "function" :kind-var "variable")"#
        ]],
    )
}

fn sort_symbols_and_canonical_alias_resolution() -> ParityBatchCase {
    ParityBatchCase::value(
        "sort_symbols_and_canonical_alias_resolution",
        r####"
(progn
  (defalias 'neomacs-helpful-alias-target #'identity)
  (defalias 'neomacs-helpful-alias 'neomacs-helpful-alias-target)
  (list :sorted
        (mapcar #'symbol-name
                (helpful--sort-symbols
                 '(zeta alpha middle)))
        :canonical
        (helpful--canonical-symbol 'neomacs-helpful-alias t)
        :aliases
        (mapcar #'symbol-name
                (helpful--aliases 'neomacs-helpful-alias-target t))))
"####,
        expect![[
            r#"OK (:sorted ("alpha" "middle" "zeta") :canonical identity :aliases ("cl--block-wrapper" "eieio--class-constructor" "identity" "neomacs-helpful-alias" "purecopy"))"#
        ]],
    )
}

fn buffer_name_and_heading_format() -> ParityBatchCase {
    ParityBatchCase::value(
        "buffer_name_and_heading_format",
        r####"
(let* ((buf (helpful--buffer 'car t))
       (heading (substring-no-properties (helpful--heading "Signature"))))
  (list :buffer-name (buffer-name buf)
        :heading heading
        :heading-ends-newline (and (string-suffix-p "\n" heading) t)))
"####,
        expect![[
            r#"OK (:buffer-name "*helpful function: car*" :heading "Signature\n" :heading-ends-newline t)"#
        ]],
    )
}

fn format_closure_rewrites_to_defun_shape() -> ParityBatchCase {
    ParityBatchCase::value(
        "format_closure_rewrites_to_defun_shape",
        r####"
(let ((form (helpful--format-closure
             'demo
             '(closure (t) (x) "doc" (+ x 1)))))
  (list :form form
        :car (car form)
        :name (nth 1 form)
        :args (nth 2 form)))
"####,
        expect![[
            r#"OK (:form (defun demo #1=(x) "doc" (+ x 1)) :car defun :name demo :args #1#)"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        pretty_print_and_indent_are_stable(),
        sort_symbols_and_canonical_alias_resolution(),
        buffer_name_and_heading_format(),
        format_closure_rewrites_to_defun_shape(),
    ]
}
