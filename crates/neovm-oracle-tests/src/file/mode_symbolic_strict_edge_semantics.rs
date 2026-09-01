//! Oracle parity tests for GNU symbolic file mode conversion helpers.
//!
//! GNU implements these helpers in `lisp/files.el`.  They are pure Lisp
//! functions used by `read-file-modes` and expose exact parsing, bit-mask, and
//! error semantics that Neomacs must preserve.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_file_mode_symbolic_conversion_helpers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((default-file-modes #o666))
  (list
   ;; `file-modes-char-to-who' maps each symbolic class to GNU's mask.
   (mapcar #'file-modes-char-to-who '(?u ?g ?o ?a))
   (condition-case err
       (file-modes-char-to-who ?z)
     (error (list (car err) (cdr err))))

   ;; `file-modes-char-to-right' includes mode-relative X/u/g/o cases.
   (list
    (file-modes-char-to-right ?r)
    (file-modes-char-to-right ?w)
    (file-modes-char-to-right ?x)
    (file-modes-char-to-right ?s)
    (file-modes-char-to-right ?t)
    (file-modes-char-to-right ?X #o644)
    (file-modes-char-to-right ?X #o755)
    (file-modes-char-to-right ?u #o4750)
    (file-modes-char-to-right ?g #o2750)
    (file-modes-char-to-right ?o #o1755))
   (condition-case err
       (file-modes-char-to-right ?q)
     (error (list (car err) (cdr err))))

   ;; RIGHTS parsing stops on a non operator and returns the accumulated mode.
   (list
    (file-modes-rights-to-number "+rx-w" #o777 #o600)
    (file-modes-rights-to-number "=rw,+x" #o700 #o777)
    (file-modes-rights-to-number nil #o777 #o640)
    (file-modes-rights-to-number "" #o777 #o640))

   ;; Number-to-symbolic must preserve file type and special-bit spelling.
   (mapcar #'file-modes-number-to-symbolic
           '(#o0000 #o0644 #o0755 #o4700 #o2750 #o1755 #o40755 #o120777 #o140777))
   (list
    (file-modes-number-to-symbolic #o755 ?d)
    (file-modes-number-to-symbolic #o755 ?l))
   (condition-case err
       (file-modes-number-to-symbolic "bad")
     (error (list (car err) (cdr err))))

   ;; Symbolic chmod syntax must match GNU's parser, including defaults.
   (list
    (file-modes-symbolic-to-number "u=rw,go=r" #o777)
    (file-modes-symbolic-to-number "a+X" #o644)
    (file-modes-symbolic-to-number "a+X" #o755)
    (file-modes-symbolic-to-number "g=u,o=g" #o4750)
    (file-modes-symbolic-to-number "+x" #o600)
    (file-modes-symbolic-to-number "u-s,g+s,o+t" #o7777))
   (condition-case err
       (file-modes-symbolic-to-number "u+q" #o644)
     (error (list (car err) (cdr err))))
   (condition-case err
       (file-modes-symbolic-to-number 42 #o644)
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((2496 1080 519 4095) (error (\"z: Bad ‘who’ character\")) (292 146 73 3072 512 0 73 2847 9709 37741) (error (\"q: Bad right character\")) (365 447 416 416) (\"----------\" \"-rw-r--r--\" \"-rwxr-xr-x\" \"-rws------\" \"-rwxr-s---\" \"-rwxr-xr-t\" \"drwxr-xr-x\" \"lrwxrwxrwx\" \"srwxrwxrwx\") (\"drwxr-xr-x\" \"lrwxr-xr-x\") (wrong-type-argument (integerp \"bad\")) (420 420 493 2523 457 2047) (error (\"Parse error in modes near ‘q’\")) (wrong-type-argument (stringp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
