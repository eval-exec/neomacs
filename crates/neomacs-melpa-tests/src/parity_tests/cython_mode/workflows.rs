use expect_test::expect;

use super::ParityBatchCase;

fn mode_registers_compile_and_navigation_hooks() -> ParityBatchCase {
    ParityBatchCase::value(
        "mode_registers_compile_and_navigation_hooks",
        r####"
(neomacs-cython-mode-test-with-buffer
 (lambda ()
   (list :mode major-mode
         :parent (get major-mode 'derived-mode-parent)
         :compile-prefix
         (and (stringp compile-command)
              (string-prefix-p "cython -a " compile-command))
         :default-format cython-default-compile-format
         :bod beginning-of-defun-function
         :eod end-of-defun-function
         :add-log add-log-current-defun-function
         :which-func (and (memq 'cython-current-defun which-func-functions) t)
         :finish (and (memq 'cython-compilation-finish
                            compilation-finish-functions)
                      t))))
"####,
        expect![[
            r#"OK (:mode cython-mode :parent python-mode :compile-prefix t :default-format "cython -a %s" :bod cython-beginning-of-defun :eod cython-end-of-defun :add-log cython-current-defun :which-func t :finish t)"#
        ]],
    )
}

fn open_block_and_comment_helpers_classify_lines() -> ParityBatchCase {
    ParityBatchCase::value(
        "open_block_and_comment_helpers_classify_lines",
        r####"
(neomacs-cython-mode-test-with-buffer
 (lambda ()
   (goto-char (point-min))
   (search-forward "cdef class")
   (forward-line 0)
   (let ((class-open (and (cython-open-block-statement-p t) t)))
     (search-forward "cpdef double magnitude")
     (forward-line 0)
     (let ((cpdef-open (and (cython-open-block-statement-p t) t)))
       (search-forward "return (self.x")
       (forward-line 0)
       (let ((return-open (and (cython-open-block-statement-p t) t)))
         (goto-char (point-max))
         (forward-line -1)
         (list :class-open class-open
               :cpdef-open cpdef-open
               :return-open return-open
               :comment-line (and (cython-comment-line-p) t)
               :in-string
               (progn
                 (goto-char (point-min))
                 (search-forward "cdef public")
                 (and (cython-in-string/comment) t))))))))
"####,
        expect![
            "OK (:class-open t :cpdef-open nil :return-open nil :comment-line t :in-string nil)"
        ],
    )
}

fn beginning_of_defun_finds_class_and_methods() -> ParityBatchCase {
    ParityBatchCase::value(
        "beginning_of_defun_finds_class_and_methods",
        r####"
(neomacs-cython-mode-test-with-buffer
 (lambda ()
   (goto-char (point-min))
   (search-forward "return (self.x")
   (cython-beginning-of-defun)
   (let ((method (buffer-substring-no-properties
                  (line-beginning-position) (line-end-position))))
     (cython-beginning-of-defun)
     (let ((class (buffer-substring-no-properties
                   (line-beginning-position) (line-end-position))))
       (goto-char (point-min))
       (search-forward "return Point")
       (list :from-method (string-trim method)
             :from-method-to-class (string-trim class)
             :current-defun (cython-current-defun))))))
"####,
        expect![[
            r#"OK (:from-method "cpdef double magnitude(self):" :from-method-to-class "cdef public double y" :current-defun "make_point")"#
        ]],
    )
}

fn end_of_defun_advances_past_method_body() -> ParityBatchCase {
    ParityBatchCase::value(
        "end_of_defun_advances_past_method_body",
        r####"
(neomacs-cython-mode-test-with-buffer
 (lambda ()
   (goto-char (point-min))
   (search-forward "cpdef double magnitude")
   (forward-line 0)
   (let ((start (point)))
     (cython-end-of-defun)
     (list :moved (> (point) start)
           :after-line
           (string-trim
            (buffer-substring-no-properties
             (line-beginning-position) (line-end-position)))
           :past-return
           (save-excursion
             (goto-char start)
             (search-forward "return (self.x" nil t)
             (< (match-end 0) (point)))))))
"####,
        expect![[
            r#"OK (:moved t :after-line "def make_point(double x, double y):" :past-return nil)"#
        ]],
    )
}

fn auto_mode_alist_associates_pyx_extensions() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_mode_alist_associates_pyx_extensions",
        r####"
(list :pyx (cdr (assoc "\\.pyx\\'" auto-mode-alist))
      :pxd (cdr (assoc "\\.pxd\\'" auto-mode-alist))
      :pxi (cdr (assoc "\\.pxi\\'" auto-mode-alist)))
"####,
        expect!["OK (:pyx cython-mode :pxd cython-mode :pxi cython-mode)"],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        mode_registers_compile_and_navigation_hooks(),
        open_block_and_comment_helpers_classify_lines(),
        beginning_of_defun_finds_class_and_methods(),
        end_of_defun_advances_past_method_body(),
        auto_mode_alist_associates_pyx_extensions(),
    ]
}
