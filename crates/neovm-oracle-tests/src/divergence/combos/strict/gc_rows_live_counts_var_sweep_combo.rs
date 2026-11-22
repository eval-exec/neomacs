//! v0.0.21 #775: `(garbage-collect)` rows are live counts, not cumulative
//! allocation totals, and FREE is a real free count.
//!
//! GNU builds the rows from the last sweep's `gcstat` (`src/alloc.c:310-317`,
//! `6038-6063`): USED is live, `vectors` counts vector objects while
//! `vector-slots` counts their cells, and FREE is the free-list count. The
//! probe allocates 200 cons lists, strings and 1000-cell vectors reachable
//! only through a key-weak table, collects, and asserts the live rows return
//! near their baseline and FREE is nonzero. Absolute counts differ between
//! editors; every printed element is a boolean or a row length, so the oracle
//! comparison is exact.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_gc_rows_report_live_counts_and_free_capacity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (garbage-collect)
  (let ((before (garbage-collect))
        (alive (make-hash-table :test 'eq :weakness 'key))
        (live-vector (make-vector 1000 nil))
        (i 0))
    (while (< i 200)
      (puthash (make-list 1000 nil) t alive)
      (puthash (make-string 1000 ?x) t alive)
      (puthash (make-vector 1000 nil) t alive)
      (setq i (1+ i)))
    (let* ((gc (garbage-collect))
           (du (lambda (n)
                 (- (nth 2 (assq n gc))
                    (nth 2 (assq n before))))))
      (list
       ;; Row shapes: list4/list4/list4/list3/list3/list4/list4/list4/list3.
       (mapcar (lambda (n) (length (assq n gc)))
               '(conses symbols strings string-bytes vectors vector-slots
                 floats intervals buffers))
       ;; The tracked objects really were collected.
       (< (hash-table-count alive) 50)
       ;; USED is live: a cumulative row would have grown by ~200000.
       (< (funcall du 'conses) 1000)
       (< (funcall du 'strings) 100)
       (< (funcall du 'string-bytes) 10000)
       (< (funcall du 'vectors) 100)
       (< (funcall du 'vector-slots) 10000)
       ;; vectors counts objects, vector-slots their cells.
       (/= (nth 2 (assq 'vectors gc)) (nth 2 (assq 'vector-slots gc)))
       ;; FREE is a real free count, not a literal 0.
       (> (nth 3 (assq 'conses gc)) 0)
       (> (nth 3 (assq 'strings gc)) 0)))))
"##;
    let expect = expect_test::expect![[r#""OK ((4 4 4 3 3 4 4 4 3) t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
