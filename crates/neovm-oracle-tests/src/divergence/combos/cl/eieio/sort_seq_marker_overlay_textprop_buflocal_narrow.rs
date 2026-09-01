//! Combo: cl-eieio sort/seq operations + markers + overlays + textprop + buflocal + narrow + undo.
//! Tests sort, seq-filter, seq-map, seq-group-by with EIEIO object comparators on buffer content.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_sort_buffer_lines_with_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 29 35)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass line-item ()
    ((text :initarg :text :accessor li-text :initform "")
     (priority :initarg :priority :accessor li-priority :initform 0)
     (category :initarg :category :accessor li-category :initform "")))
  (let* ((buf (generate-new-buffer "st1"))
         (items nil)
         (i1 (line-item :text "cherry" :priority 3 :category "fruit"))
         (i2 (line-item :text "almond" :priority 1 :category "nut"))
         (i3 (line-item :text "banana" :priority 2 :category "fruit"))
         (i4 (line-item :text "date" :priority 4 :category "fruit"))
         (i5 (line-item :text "pecan" :priority 5 :category "nut")))
    (setq items (list i1 i2 i3 i4 i5))
    (with-current-buffer buf
      (insert "cherry\nalmond\nbanana\ndate\npecan\n")
      (put-text-property 1 7 'item i1)
      (put-text-property 8 14 'item i2)
      (put-text-property 15 22 'item i3)
      (put-text-property 23 28 'item i4)
      (put-text-property 29 35 'item i5)
      (setq-local my-items items)
      (let* ((ov (make-overlay 1 14))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (sorted nil)
             (by-cat nil))
        (undo-boundary)
        (setq sorted (sort (copy-sequence items)
                          (lambda (a b) (< (li-priority a) (li-priority b)))))
        (setq by-cat (seq-group-by (lambda (x) (li-category x)) items))
        (goto-char (point-max))
        (insert (format " | sorted=%s by-cat=%s"
                       (mapcar (lambda (x) (list (li-text x) (li-priority x))) sorted)
                       (mapcar (lambda (g) (list (car g)
                                                (mapcar (lambda (x) (li-text x)) (cdr g))))
                               by-cat)))
        (set-marker m 5)
        (put-text-property (1- (point-max)) (point-max) 'sort-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                my-items))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_seq_filter_map_with_buffer_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 80 96)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass event ()
    ((type :initarg :type :accessor ev-type :initform "")
     (timestamp :initarg :timestamp :accessor ev-ts :initform 0)
     (data :initarg :data :accessor ev-data :initform "")))
  (let* ((buf (generate-new-buffer "st2"))
         (events nil)
         (e1 (event :type "click" :timestamp 100 :data "btn1"))
         (e2 (event :type "hover" :timestamp 200 :data "link1"))
         (e3 (event :type "click" :timestamp 300 :data "btn2"))
         (e4 (event :type "scroll" :timestamp 400 :data "page1"))
         (e5 (event :type "click" :timestamp 500 :data "btn3"))
         (e6 (event :type "hover" :timestamp 600 :data "link2")))
    (setq events (list e1 e2 e3 e4 e5 e6))
    (with-current-buffer buf
      (insert "click-100-btn1\nhover-200-link1\nclick-300-btn2\nscroll-400-page1\nclick-500-btn3\nhover-600-link2\n")
      (put-text-property 1 15 'ev e1)
      (put-text-property 16 32 'ev e2)
      (put-text-property 33 47 'ev e3)
      (put-text-property 48 64 'ev e4)
      (put-text-property 65 79 'ev e5)
      (put-text-property 80 96 'ev e6)
      (setq-local my-events events)
      (let* ((ov (make-overlay 16 47))
             (_ (overlay-put ov 'priority 2))
             (m (make-marker))
             (_ (set-marker m 16))
             (clicks nil)
             (timestamps nil)
             (data-map nil))
        (undo-boundary)
        (setq clicks (seq-filter (lambda (e) (equal (ev-type e) "click")) events))
        (setq timestamps (seq-map (lambda (e) (ev-ts e)) clicks))
        (setq data-map (seq-map (lambda (e) (ev-data e)) clicks))
        (let* ((avg (/ (apply '+ timestamps) (length timestamps)))
               (hovers (seq-filter (lambda (e) (equal (ev-type e) "hover")) events))
               (hover-data (seq-map (lambda (e) (ev-data e)) hovers)))
          (goto-char (point-max))
          (insert (format " | clicks=%d avg=%d data=%s hover-data=%s"
                         (length clicks) avg data-map hover-data))
          (set-marker m 10)
          (put-text-property (1- (point-max)) (point-max) 'seq-log t)
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (bs (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os oe bs
                  (marker-position m)
                  (buffer-string)
                  my-events))))
    (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_sort_substring_by_object_slot() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass word-entry ()
    ((word :initarg :word :accessor we-word :initform "")
     (freq :initarg :freq :accessor we-freq :initform 0)
     (pos :initarg :pos :accessor we-pos :initform "")))
  (let* ((buf (generate-new-buffer "st3"))
         (entries nil)
         (w1 (word-entry :word "the" :freq 100 :pos "det"))
         (w2 (word-entry :word "cat" :freq 50 :pos "noun"))
         (w3 (word-entry :word "sat" :freq 30 :pos "verb"))
         (w4 (word-entry :word "on" :freq 80 :pos "prep"))
         (w5 (word-entry :word "mat" :freq 20 :pos "noun")))
    (setq entries (list w1 w2 w3 w4 w5))
    (with-current-buffer buf
      (insert "the cat sat on mat\n")
      (put-text-property 1 4 'entry w1)
      (put-text-property 5 8 'entry w2)
      (put-text-property 9 12 'entry w3)
      (put-text-property 13 15 'entry w4)
      (put-text-property 16 19 'entry w5)
      (setq-local my-entries entries)
      (let* ((ov (make-overlay 5 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 5))
             (by-freq nil)
             (by-pos nil)
             (top3 nil))
        (undo-boundary)
        (setq by-freq (sort (copy-sequence entries)
                           (lambda (a b) (> (we-freq a) (we-freq b)))))
        (setq by-pos (seq-group-by (lambda (x) (we-pos x)) entries))
        (setq top3 (seq-take by-freq 3))
        (let ((top3-words (mapcar (lambda (x) (we-word x)) top3))
              (noun-words (mapcar (lambda (x) (we-word x))
                                 (cdr (assoc "noun" by-pos)))))
          (goto-char (point-max))
          (insert (format " | top3=%s nouns=%s by-freq=%s"
                         top3-words noun-words
                         (mapcar (lambda (x) (list (we-word x) (we-freq x))) by-freq)))
          (set-marker m 3)
          (put-text-property (1- (point-max)) (point-max) 'word-log t)
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (bs (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os oe bs
                  (marker-position m)
                  (buffer-string)
                  my-entries))))
    (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_sort_with_buffer_narrow_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass segment ()
    ((label :initarg :label :accessor sg-label :initform "")
     (start-pos :initarg :start-pos :accessor sg-start :initform 1)
     (end-pos :initarg :end-pos :accessor sg-end :initform 1)
     (score :initarg :score :accessor sg-score :initform 0)))
  (let* ((buf (generate-new-buffer "st4"))
         (segs nil)
         (s1 (segment :label "A" :score 5))
         (s2 (segment :label "B" :score 3))
         (s3 (segment :label "C" :score 8))
         (s4 (segment :label "D" :score 1))
         (s5 (segment :label "E" :score 6)))
    (setq segs (list s1 s2 s3 s4 s5))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'seg s1)
      (put-text-property 6 10 'seg s2)
      (put-text-property 11 15 'seg s3)
      (put-text-property 16 20 'seg s4)
      (put-text-property 21 25 'seg s5)
      (setq-local my-segs segs)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 2))
             (m (make-marker))
             (_ (set-marker m 11))
             (narrow-segs nil)
             (narrow-sorted nil))
        (undo-boundary)
        (save-restriction
          (narrow-to-region 6 15)
          (let ((pos (point-min)))
            (while (< pos (point-max))
              (let ((val (get-text-property pos 'seg)))
                (when val
                  (push val narrow-segs))
                 (setq pos (or (next-single-property-change pos 'seg (current-buffer) (point-max))
                              (point-max))))))
        (setq narrow-segs (reverse narrow-segs))
        (setq narrow-sorted (sort (copy-sequence narrow-segs)
                                 (lambda (a b) (< (sg-score a) (sg-score b)))))
        (let ((full-sorted (sort (copy-sequence segs)
                                 (lambda (a b) (< (sg-score a) (sg-score b)))))
              (narrow-labels nil)
              (full-labels nil))
          (setq narrow-labels (mapcar (lambda (x) (list (sg-label x) (sg-score x))) narrow-sorted))
          (setq full-labels (mapcar (lambda (x) (list (sg-label x) (sg-score x))) full-sorted))
          (goto-char (point-max))
          (insert (format " | narrow=%s full=%s m=%d ov=[%d,%d]"
                         narrow-labels full-labels
                         (marker-position m) (overlay-start ov) (overlay-end ov)))
          (set-marker m 4)
          (put-text-property (1- (point-max)) (point-max) 'seg-log t)
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (bs (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os oe bs
                  (marker-position m)
                  (buffer-string)
                  my-segs))))
    (kill-buffer buf)))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_seq_reduce_accumulate_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 21 26)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass metric ()
    ((name :initarg :name :accessor mt-name :initform "")
     (value :initarg :value :accessor mt-value :initform 0)
     (unit :initarg :unit :accessor mt-unit :initform "")))
  (let* ((buf (generate-new-buffer "st5"))
         (m1 (metric :name "latency" :value 120 :unit "ms"))
         (m2 (metric :name "throughput" :value 500 :unit "req/s"))
         (m3 (metric :name "errors" :value 3 :unit "count"))
         (m4 (metric :name "latency" :value 80 :unit "ms"))
         (m5 (metric :name "throughput" :value 700 :unit "req/s")))
    (with-current-buffer buf
      (insert "L120-T500-E3-L80-T700\n")
      (put-text-property 1 5 'metric m1)
      (put-text-property 6 11 'metric m2)
      (put-text-property 12 15 'metric m3)
      (put-text-property 16 20 'metric m4)
      (put-text-property 21 26 'metric m5)
      (setq-local my-metrics (list m1 m2 m3 m4 m5))
      (let* ((ov (make-overlay 1 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 6))
             (all-metrics (list m1 m2 m3 m4 m5))
             (total-errors nil)
             (avg-latency nil)
             (grouped nil))
        (undo-boundary)
        (setq total-errors (seq-reduce
                           (lambda (acc mt) (if (equal (mt-name mt) "errors")
                                               (+ acc (mt-value mt)) acc))
                           all-metrics 0))
        (let* ((latencies (seq-filter (lambda (mt) (equal (mt-name mt) "latency")) all-metrics))
               (lat-sum (seq-reduce (lambda (acc mt) (+ acc (mt-value mt))) latencies 0))
               (lat-count (length latencies)))
          (setq avg-latency (/ lat-sum lat-count)))
        (setq grouped (seq-group-by (lambda (mt) (mt-name mt)) all-metrics))
        (let ((group-summary (mapcar
                             (lambda (g)
                               (list (car g)
                                    (length (cdr g))
                                    (seq-reduce (lambda (acc mt) (+ acc (mt-value mt)))
                                               (cdr g) 0)))
                             grouped)))
          (goto-char (point-max))
          (insert (format " | errors=%d avg-lat=%d groups=%s m=%d"
                         total-errors avg-latency group-summary
                         (marker-position m)))
          (set-marker m 3)
          (put-text-property (1- (point-max)) (point-max) 'metric-log t)
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (bs (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os oe bs
                  (marker-position m)
                  (buffer-string)
                  my-metrics))))
    (kill-buffer buf))))"#,
        expect,
    );
}
