mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

#[test]
fn compat_lread_special_binding_semantics_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping lread special binding audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let form = r#"(let ((symbols '(standard-input
                        read-circle
                        load-path
                        load-suffixes
                        module-file-suffix
                        dynamic-library-suffixes
                        load-file-rep-suffixes
                        after-load-alist
                        load-history
                        load-file-name
                        load-true-file-name
                        user-init-file
                        current-load-list
                        load-read-function
                        load-source-file-function
                        source-directory
                        preloaded-file-list
                        byte-boolean-vars
                        bytecomp-version-regexp
                        eval-buffer-list
                        lread--unescaped-character-literals
                        load-path-filter-function
                        internal--get-default-lexical-binding-function
                        read-symbol-shorthands
                        macroexp--dynvars
                        values)))
  (mapcar
   (lambda (sym)
     (let ((marker (list 'marker sym)))
       (list sym
             (boundp sym)
             (funcall
              (eval
               `(lambda ()
                  (let ((,sym ',marker))
                    (equal (symbol-value ',sym) ',marker)))
               t)))))
   symbols))"#;

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "lread special binding mismatch:\nGNU: {}\nNeoVM: {}",
        gnu, neovm
    );
}
