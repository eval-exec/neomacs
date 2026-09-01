//! Oracle parity tests for GNU `subr.el' `map-keymap-sorted'.

use crate::common::assert_oracle_parity;

#[test]
fn oracle_map_keymap_sorted_orders_events_and_preserves_bindings() {
    let form = r#"
(let ((symbol-map (make-sparse-keymap))
      (integer-map (make-sparse-keymap))
      (mixed-map (make-sparse-keymap))
      symbol-out integer-out mixed-out)
  (define-key symbol-map [z] 'sym-z)
  (define-key symbol-map [a] 'sym-a)
  (define-key symbol-map [b] 'sym-b)
  (map-keymap-sorted
   (lambda (key binding)
     (setq symbol-out (append symbol-out (list (list key binding)))))
   symbol-map)

  (define-key integer-map [2] 'int-2)
  (define-key integer-map [1] 'int-1)
  (define-key integer-map [3] 'int-3)
  (map-keymap-sorted
   (lambda (key binding)
     (setq integer-out (append integer-out (list (list key binding)))))
   integer-map)

  (define-key mixed-map [b] 'sym-b)
  (define-key mixed-map [a] 'sym-a)
  (define-key mixed-map [2] 'int-2)
  (define-key mixed-map [1] 'int-1)
  (define-key mixed-map [z] 'sym-z)
  (map-keymap-sorted
   (lambda (key binding)
     (setq mixed-out (append mixed-out (list (list key binding)))))
   mixed-map)

  (list symbol-out
        integer-out
        mixed-out
        (condition-case e
            (map-keymap-sorted 42 symbol-map)
          (error (list (car e) (cadr e))))
        (condition-case e
            (map-keymap-sorted (lambda (key binding) nil) 42)
          (error (list (car e) (cadr e) (caddr e))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((a sym-a) (b sym-b) (z sym-z)) ((1 int-1) (2 int-2) (3 int-3)) ((z sym-z) (1 int-1) (2 int-2) (a sym-a) (b sym-b)) (invalid-function 42) (wrong-type-argument keymapp 42))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
