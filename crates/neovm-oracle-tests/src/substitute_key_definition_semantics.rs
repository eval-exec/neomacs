//! Oracle parity tests for GNU `subr.el' `substitute-key-definition'.

use crate::common::assert_oracle_parity;

#[test]
fn oracle_substitute_key_definition_direct_nested_oldmap_and_menu_items() {
    let form = r#"
(let ((map (make-sparse-keymap))
      (oldmap (make-sparse-keymap)))
  (define-key map [a] 'old-cmd)
  (define-key map [b] 'other-cmd)
  (define-key map [x a] 'old-cmd)
  (define-key map [menu] '(menu-item "Old" old-cmd :enable t))
  (substitute-key-definition 'old-cmd 'new-cmd map)

  ;; When OLDMAP is supplied, GNU scans OLDMAP for keys whose old binding is
  ;; OLDDEF, then writes NEWDEF into MAP at those keys.
  (define-key map [c] 'current-cmd)
  (define-key oldmap [c] 'old-cmd)
  (substitute-key-definition 'old-cmd 'oldmap-new map oldmap)

  (list
   (lookup-key map [a])
   (lookup-key map [b])
   (lookup-key map [x a])
   (cdr (assq 'menu (cdr map)))
   (lookup-key map [c])
   (condition-case e
       (substitute-key-definition 'old-cmd 'new-cmd 42)
     (error (list (car e) (cadr e) (caddr e))))))"#;
    let expect = expect_test::expect![[
        r#""OK (new-cmd other-cmd new-cmd (menu-item \"Old\" new-cmd :enable t) oldmap-new (wrong-type-argument keymapp 42))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
