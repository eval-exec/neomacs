//! v0.0.21 #775: `(garbage-collect)` reports live objects, not lifetime
//! allocation totals.
//!
//! GNU's rows carry the last sweep's `gcstat` counts (`src/alloc.c:6038-6063`):
//! USED is the live count, `vectors` counts vector objects and `vector-slots`
//! their cells, and FREE is the free capacity of that kind. The probe
//! allocates 200 cons lists, strings and 1000-cell vectors that only a
//! key-weak table references, collects, and asserts the rows return near
//! their baseline (a cumulative row would grow by ~200k) while one live
//! 1000-cell vector keeps `vector-slots` above `vectors`.
//!
//! The unit runtime is pre-loadup, so the probe uses special forms and subrs.

use crate::emacs_core::eval::Context;
use neovm_value::value::Value;

#[test]
fn garbage_collect_rows_report_live_counts_not_lifetime_totals() {
    let mut context = Context::new(crate::test_utils::unit_runtime());
    let value = context
        .eval_str(
            r#"(progn
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
                     (if (and
                          ;; GNU row shapes: list4 / list3 / list3 / list3.
                          (= (length (assq 'conses before)) 4)
                          (= (length (assq 'string-bytes before)) 3)
                          (= (length (assq 'vectors before)) 3)
                          (= (length (assq 'buffers before)) 3)
                          ;; The dead objects really were collected.
                          (< (hash-table-count alive) 50)
                          ;; USED is a live count: a cumulative row would have
                          ;; grown by the full ~200k allocated.
                          (< (funcall du 'conses) 1000)
                          (< (funcall du 'strings) 100)
                          (< (funcall du 'string-bytes) 10000)
                          (< (funcall du 'vectors) 100)
                          (< (funcall du 'vector-slots) 10000)
                          ;; vectors counts objects, vector-slots counts cells.
                          (/= (nth 2 (assq 'vectors gc))
                              (nth 2 (assq 'vector-slots gc)))
                          ;; FREE is a real free count, not a literal 0.
                          (> (nth 3 (assq 'conses gc)) 0)
                          (> (nth 3 (assq 'strings gc)) 0))
                         t
                       nil))))"#,
        )
        .expect("garbage-collect rows");
    assert_eq!(
        value,
        Value::T,
        "garbage-collect must report live counts with GNU row shapes"
    );
}
