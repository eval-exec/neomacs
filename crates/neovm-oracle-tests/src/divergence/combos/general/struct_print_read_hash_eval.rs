//! Deep combo: cl-defstruct + print-circle + read-from-string + hash-table + equal.
//! Tests struct serialization roundtrips, circular references, and collection interop.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_cl_defstruct_print_read_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-defstruct (tester (:constructor make-tester) (:type list))\n\
         x y z)\n\
         (let ((a (make-tester :x 1 :y \\\"hello\\\" :z '(a b c))))\n\
         (let ((printed (prin1-to-string a)))\n\
         (let ((re-read (read-from-string printed)))\n\
         (list printed (car re-read) (cdr re-read)\n\
         (tester-x a) (tester-y a) (tester-z a))))))",
        expect,
    );
}

#[test]
fn deficiency_cl_defstruct_vector_type_with_hash_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-defstruct (vpoint (:type vector) (:constructor vpoint-create))\n\
         label x y)\n\
         (let ((p1 (vpoint-create :label 'origin :x 0 :y 0))\n\
         (p2 (vpoint-create :label 'target :x 10 :y 20))\n\
         (ht (make-hash-table :test 'equal)))\n\
         (puthash p1 'start ht)\n\
         (puthash p2 'end ht)\n\
         (list (gethash p1 ht)\n\
         (gethash p2 ht)\n\
         (vpoint-label p1)\n\
         (vpoint-x p2)\n\
         (vpoint-y p2)\n\
         (hash-table-count ht))))",
        expect,
    );
}

#[test]
fn deficiency_nested_structs_with_print_circle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-defstruct (node (:type list) (:constructor node-create))\n\
         value left right)\n\
         (let ((leaf1 (node-create :value 1))\n\
         (leaf2 (node-create :value 2)))\n\
         (let ((root (node-create :value 'root :left leaf1 :right leaf2)))\n\
         (let ((printed (let (print-circle) (prin1-to-string root))))\n\
         (let ((re-read (read printed)))\n\
         (list printed\n\
         (node-value root)\n\
         (node-value (node-left root))\n\
         (node-value (node-right root))))))))",
        expect,
    );
}

#[test]
fn deficiency_struct_equal_vs_type_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-defstruct (entry (:type list) (:constructor entry-create) (:copier entry-copy))\n\
         key value tag)\n\
         (let ((e1 (entry-create :key 'name :value \\\"bob\\\" :tag 1))\n\
         (e2 (entry-create :key 'name :value \\\"bob\\\" :tag 1))\n\
         (e3 (entry-copy e1)))\n\
         (setf (entry-tag e3) 99)\n\
         (list (equal e1 e2)\n\
         (equal e1 e3)\n\
         (entry-p e1)\n\
         (entry-p '(entry name \\\"bob\\\" 1))\n\
         (entry-key e1)\n\
         (entry-value e3)\n\
         (entry-tag e3))))",
        expect,
    );
}

#[test]
fn deficiency_cl_defstruct_with_boa_constructor() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-defstruct (range (:constructor make-range (min max))\n\
         (:type list))\n\
         min max label)\n\
         (let ((r1 (make-range 0 100))\n\
         (r2 (make-range 50 150)))\n\
         (setf (range-label r1) 'first)\n\
         (setf (range-label r2) 'second)\n\
         (let ((ht (make-hash-table :test 'equal)))\n\
         (puthash r1 'a ht)\n\
         (puthash r2 'b ht)\n\
         (list (range-min r1) (range-max r1) (range-label r1)\n\
         (range-min r2) (range-max r2) (range-label r2)\n\
         (gethash r1 ht) (gethash r2 ht))))))",
        expect,
    );
}

#[test]
fn deficiency_struct_alist_with_print_length_and_level() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-defstruct (item (:type list) (:constructor item-create))\n\
         name payload)\n\
         (let ((items (cl-loop for i from 1 to 8\n\
         collect (item-create :name (format \\\"item-%d\\\" i)\n\
         :payload (cl-loop for j from 1 to i collect j)))))\n\
         (let ((p1 (let ((print-length 3)) (prin1-to-string items)))\n\
         (p2 (let ((print-level 2)) (prin1-to-string items))))\n\
         (list p1 p2\n\
         (length items)\n\
         (item-name (nth 0 items))\n\
         (item-name (nth 7 items))\n\
         (length (item-payload (nth 4 items)))))))",
        expect,
    );
}

#[test]
fn deficiency_hash_table_with_struct_keys_update_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-defstruct (coord (:type vector) (:constructor coord))\n\
         x y)\n\
         (let ((ht (make-hash-table :test 'equal :size 10)))\n\
         (dotimes (x 4)\n\
         (dotimes (y 4)\n\
         (puthash (coord x y) (+ (* x 4) y) ht)))\n\
         (let ((before (hash-table-count ht)))\n\
         (remhash (coord 1 2) ht)\n\
         (remhash (coord 3 3) ht)\n\
         (let ((after (hash-table-count ht)))\n\
         (list before after\n\
         (gethash (coord 0 0) ht)\n\
         (gethash (coord 1 2) ht)\n\
         (gethash (coord 2 2) ht)\n\
         (gethash (coord 3 3) ht)\n\
         (cl-loop for k being the hash-keys of ht\n\
         when (and (= (coord-x k) 0))\n\
         collect (cons (coord-y k) (gethash k ht))))))))",
        expect,
    );
}

#[test]
fn deficiency_struct_in_cl_loop_collect_with_nested_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-defstruct (rec (:type list) (:constructor rec-create))\n\
         id data next)\n\
         (let* ((r3 (rec-create :id 3 :data \\\"gamma\\\"))\n\
         (r2 (rec-create :id 2 :data \\\"beta\\\" :next r3))\n\
         (r1 (rec-create :id 1 :data \\\"alpha\\\" :next r2)))\n\
         (let ((chain (cl-loop for r = r1 then (rec-next r)\n\
         while r\n\
         collect (list (rec-id r) (rec-data r)))))\n\
         (let ((printed (prin1-to-string chain)))\n\
         (list chain printed\n\
         (rec-data (rec-next r1))\n\
         (rec-data (rec-next (rec-next r1))))))))",
        expect,
    );
}

#[test]
fn deficiency_struct_map_and_filter_via_cl_loop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-defstruct (pair (:type list) (:constructor pair))\n\
         key val)\n\
         (let ((pairs (cl-loop for i from 1 to 10\n\
         collect (pair :key i :val (* i i)))))\n\
         (let ((evens (cl-loop for p in pairs\n\
         when (cl-evenp (pair-key p))\n\
         collect (pair-val p))))\n\
         (let ((sum (cl-loop for v in evens sum v)))\n\
         (let ((mapped (cl-loop for p in pairs\n\
         collect (pair :key (pair-key p)\n\
         :val (1+ (pair-val p))))))\n\
         (list evens sum\n\
         (pair-val (nth 0 mapped))\n\
         (pair-val (nth 4 mapped))\n\
         (pair-val (nth 9 mapped)))))))",
        expect,
    );
}

#[test]
fn deficiency_read_from_string_with_multiple_structs_and_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-defstruct (pt (:type list) (:constructor pt)) x y)\n\
         (let* ((s \\\"(pt 1 2) (pt 3 4) (pt 5 6)\\\")\n\
         (pos 0)\n\
         (results nil))\n\
         (while (< pos (length s))\n\
         (let ((r (read-from-string s pos)))\n\
         (push (list (pt-x (car r)) (pt-y (car r)) (cdr r)) results)\n\
         (setq pos (cdr r))\n\
         (skip-chars-forward \\\" \\\" s pos)\n\
         (when (< pos (length s))\n\
         (if (equal (aref s pos) ? )\n\
         (setq pos (1+ pos))))))\n\
         (nreverse results)))",
        expect,
    );
}
