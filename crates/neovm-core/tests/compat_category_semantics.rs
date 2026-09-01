mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

struct CategoryCase {
    name: &'static str,
    form: &'static str,
}

#[test]
fn compat_category_semantics_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping category semantics audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let cases = [
        CategoryCase {
            name: "shape_and_nil_set_category_table",
            form: r#"(let ((orig (category-table))
      (tbl (make-category-table)))
  (list
   (category-table-p tbl)
   (bool-vector-p (char-table-range tbl nil))
   (length (char-table-extra-slot tbl 0))
   (char-table-extra-slot tbl 1)
   (eq (set-category-table nil) orig)))"#,
        },
        CategoryCase {
            name: "define_category_duplicate_and_unused_scan",
            form: r#"(let ((tbl (make-category-table)))
  (list
   (get-unused-category tbl)
   (progn
     (define-category ?! "bang" tbl)
     (category-docstring ?! tbl))
   (get-unused-category tbl)
   (condition-case err
       (progn (define-category ?! "dup" tbl) 'ok)
     (error (prin1-to-string err)))))"#,
        },
        CategoryCase {
            name: "modify_category_entry_uses_optional_table_and_copy_is_deep",
            form: r#"(let* ((tbl (make-category-table))
       (copy nil))
  (define-category ?! "bang" tbl)
  (modify-category-entry '(65 . 67) ?! tbl)
  (setq copy (copy-category-table tbl))
  (define-category ?\" "quote" copy)
  (modify-category-entry ?D ?! copy)
  (list
   (category-set-mnemonics (char-table-range tbl ?A))
   (category-set-mnemonics (char-table-range tbl ?D))
   (category-set-mnemonics (char-table-range copy ?D))
   (category-docstring ?\" tbl)
   (category-docstring ?\" copy)
   (eq (char-table-extra-slot tbl 0) (char-table-extra-slot copy 0))
   (eq (char-table-range tbl nil) (char-table-range copy nil))))"#,
        },
        CategoryCase {
            name: "char_category_set_follows_current_buffer_table",
            form: r#"(let ((buf (get-buffer-create " *compat-category-buffer*"))
      (tbl (make-category-table)))
  (unwind-protect
      (progn
        (define-category ?! "bang" tbl)
        (modify-category-entry ?A ?! tbl)
        (with-current-buffer buf
          (set-category-table tbl)
          (category-set-mnemonics (char-category-set ?A))))
    (kill-buffer buf)))"#,
        },
    ];

    for case in cases {
        let gnu = run_oracle_eval(case.form).expect("GNU Emacs evaluation");
        let neovm = run_neovm_eval(case.form).expect("NeoVM evaluation");
        assert_eq!(
            neovm, gnu,
            "category semantics mismatch for {}:\nGNU: {}\nNeoVM: {}",
            case.name, gnu, neovm
        );
    }
}
