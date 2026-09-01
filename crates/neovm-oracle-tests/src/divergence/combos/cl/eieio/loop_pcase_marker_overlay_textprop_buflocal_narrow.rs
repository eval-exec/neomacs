//! Combo: cl-eieio cl-loop/pcase with objects + marker + overlay + textprop + buflocal + undo.
//! Tests complex cl-loop and pcase patterns destructuring EIEIO objects with buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_cl_loop_collect_nunion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 25 29)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass tag-entry ()
    ((tag :initarg :tag :accessor te-tag :initform "")
     (score :initarg :score :accessor te-score :initform 0)
     (category :initarg :category :accessor te-category :initform "")))
  (let* ((buf (generate-new-buffer "lp1"))
         (entries (list (tag-entry :tag "a" :score 10 :category "x")
                       (tag-entry :tag "b" :score 20 :category "y")
                       (tag-entry :tag "c" :score 30 :category "x")
                       (tag-entry :tag "d" :score 40 :category "z")
                       (tag-entry :tag "e" :score 50 :category "y"))))
    (with-current-buffer buf
      (insert "TAGS:a10:b20:c30:d40:e50")
      (put-text-property 1 5 'field 'header)
      (put-text-property 6 9 'field 'a)
      (put-text-property 10 14 'field 'b)
      (put-text-property 15 19 'field 'c)
      (put-text-property 20 24 'field 'd)
      (put-text-property 25 29 'field 'e)
      (setq-local tag-entries entries)
      (let* ((ov (make-overlay 6 19))
             (_ (overlay-put ov 'priority 5))
             (m (make-marker))
             (_ (set-marker m 8))
             (categories (cl-loop for e in entries
                                  collect (te-category e) into cats
                                  finally return (sort (delete-dups (copy-sequence cats)) #'string<)))
             (total-score (cl-loop for e in entries sum (te-score e)))
             (by-cat (cl-loop for cat in categories
                              collect (cons cat (cl-loop for e in entries
                                                         when (equal (te-category e) cat)
                                                         sum (te-score e))))))
        (undo-boundary)
        (dolist (e entries)
          (setf (te-score e) (+ (te-score e) 100)))
        (let ((new-total (cl-loop for e in entries sum (te-score e)))
              (new-by-cat (cl-loop for cat in categories
                                   collect (cons cat (cl-loop for e in entries
                                                              when (equal (te-category e) cat)
                                                              sum (te-score e))))))
          (goto-char 6)
          (insert (format "total=%d->%d:by=%s->%s"
                         total-score new-total by-cat new-by-cat))
          (setf (marker-position m) 10)
          (put-text-property 6 (+ 6 (length (format "total=%d->%d:by=%s->%s"
                                                      total-score new-total by-cat new-by-cat)))
                            'loop-result t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (scores (mapcar (lambda (e) (te-score e)) tag-entries)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs scores categories
                (marker-position m)
                (buffer-string)
                tag-entries)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_pcase_destructure_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass command ()
    ((verb :initarg :verb :accessor cmd-verb :initform "")
     (args :initarg :args :accessor cmd-args :initform nil)
     (priority :initarg :priority :accessor cmd-priority :initform 0)))
  (let* ((buf (generate-new-buffer "lp2"))
         (cmds (list (command :verb "move" :args '(10 20) :priority 1)
                    (command :verb "rotate" :args '(90) :priority 2)
                    (command :verb "scale" :args '(2.0) :priority 1)
                    (command :verb "move" :args '(30 40) :priority 3)
                    (command :verb "delete" :args nil :priority 5)))
         (results nil))
    (with-current-buffer buf
      (insert "CMDS:move,rotate,scale,move,delete")
      (put-text-property 1 5 'field 'header)
      (put-text-property 6 34 'field 'verbs)
      (setq-local commands cmds)
      (let* ((ov (make-overlay 6 34))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (dolist (cmd cmds)
          (push (pcase (cmd-verb cmd)
                  ("move" (let ((args (cmd-args cmd)))
                           (format "mv(%d,%d)" (car args) (cadr args))))
                  ("rotate" (format "rot(%s)" (car (cmd-args cmd))))
                  ("scale" (format "scl(%.1f)" (car (cmd-args cmd))))
                  ("delete" "del")
                  (_ "unknown"))
                results))
        (let ((move-commands (cl-loop for c in cmds
                                       when (equal (cmd-verb c) "move")
                                       collect (cmd-args c)))
              (high-priority (cl-loop for c in cmds
                                      when (> (cmd-priority c) 2)
                                      collect (cons (cmd-verb c) (cmd-priority c)))))
          (setf (cmd-priority (nth 0 cmds)) 10)
          (goto-char 6)
          (insert (format "%s|moves=%s|high=%s"
                         (mapconcat #'identity (reverse results) ",")
                         move-commands high-priority))
          (setf (marker-position m) 12)
          (put-text-property 6 (+ 6 (length (format "%s|moves=%s|high=%s"
                                                      (mapconcat #'identity (reverse results) ",")
                                                      move-commands high-priority)))
                            'pcase-result t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (p0 (cmd-priority (car commands)))
              (verbs (mapcar (lambda (c) (cmd-verb c)) commands)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs p0 verbs
                (marker-position m)
                (buffer-string)
                commands)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_cl_loop_for_object_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 20 26)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass event ()
    ((type :initarg :type :accessor ev-type :initform "")
     (data :initarg :data :accessor ev-data :initform nil)
     (timestamp :initarg :timestamp :accessor ev-ts :initform 0)))
  (let* ((buf (generate-new-buffer "lp3"))
         (events (list (event :type "click" :data '(100 200) :timestamp 100)
                      (event :type "key" :data "enter" :timestamp 200)
                      (event :type "click" :data '(300 400) :timestamp 300)
                      (event :type "scroll" :data '(0 5) :timestamp 400)
                      (event :type "key" :data "escape" :timestamp 500)))
         (click-events (cl-loop for e in events
                                when (equal (ev-type e) "click")
                                collect e))
         (key-events (cl-loop for e in events
                              when (equal (ev-type e) "key")
                              collect (cons (ev-data e) (ev-ts e)))))
    (with-current-buffer buf
      (insert "EVENTS:5:clicks=2:keys=2")
      (put-text-property 1 7 'field 'header)
      (put-text-property 8 9 'field 'count)
      (put-text-property 10 19 'field 'clicks)
      (put-text-property 20 26 'field 'keys)
      (setq-local my-events events)
      (setq-local my-clicks click-events)
      (let* ((ov (make-overlay 10 26))
             (_ (overlay-put ov 'face 'region))
             (m (make-marker))
             (_ (set-marker m 12))
             (click-count (length click-events))
             (click-data (mapcar (lambda (e) (ev-data e)) click-events))
             (ts-range (cons (ev-ts (car events)) (ev-ts (car (last events))))))
        (undo-boundary)
        (dolist (e events)
          (setf (ev-ts e) (+ (ev-ts e) 1000)))
        (dolist (e click-events)
          (setf (ev-data e) (mapcar (lambda (x) (* x 2)) (ev-data e))))
        (let ((new-click-data (mapcar (lambda (e) (ev-data e)) my-clicks))
              (new-ts-range (cons (ev-ts (car my-events)) (ev-ts (car (last my-events)))))
              (key-data (cl-loop for e in my-events
                                 when (equal (ev-type e) "key")
                                 collect (cons (ev-data e) (ev-ts e)))))
          (goto-char 10)
          (insert (format "data=%s->%s|ts=%s->%s|keys=%s"
                         click-data new-click-data ts-range new-ts-range key-data))
          (setf (marker-position m) 15)
          (put-text-property 10 (+ 10 (length (format "data=%s->%s|ts=%s->%s|keys=%s"
                                                        click-data new-click-data ts-range new-ts-range key-data)))
                            'loop-result t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (ts0 (ev-ts (car my-events)))
              (cd0 (ev-data (car my-clicks))))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs ts0 cd0 click-count
                (marker-position m)
                (buffer-string)
                my-events my-clicks)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_pcase_guard_with_loop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass measurement ()
    ((sensor :initarg :sensor :accessor msr-sensor :initform "")
     (value :initarg :value :accessor msr-value :initform 0.0)
     (unit :initarg :unit :accessor msr-unit :initform "")
     (quality :initarg :quality :accessor msr-quality :initform 1.0)))
  (let* ((buf (generate-new-buffer "lp4"))
         (measurements (list (measurement :sensor "temp" :value 22.5 :unit "C" :quality 0.95)
                            (measurement :sensor "humidity" :value 65.0 :unit "%" :quality 0.80)
                            (measurement :sensor "temp" :value 23.1 :unit "C" :quality 0.99)
                            (measurement :sensor "pressure" :value 1013.2 :unit "hPa" :quality 0.70)
                            (measurement :sensor "temp" :value 21.8 :unit "C" :quality 0.90)))
         (classified nil))
    (with-current-buffer buf
      (insert "MEAS:5:temp=3:other=2")
      (put-text-property 1 5 'field 'header)
      (put-text-property 6 7 'field 'count)
      (put-text-property 8 14 'field 'temp)
      (put-text-property 15 22 'field 'other)
      (setq-local meas measurements)
      (let* ((ov (make-overlay 8 22))
             (_ (overlay-put ov 'face 'underline))
             (m (make-marker))
             (_ (set-marker m 10)))
        (undo-boundary)
        (dolist (msr measurements)
          (push (pcase msr
                  ((pred (lambda (m) (> (msr-quality m) 0.9)))
                   (list 'high-quality (msr-sensor msr) (msr-value msr)))
                  ((pred (lambda (m) (equal (msr-sensor m) "temp")))
                   (list 'temp-reading (msr-value msr) (msr-unit msr)))
                  (_ (list 'other (msr-sensor msr) (msr-value msr))))
                classified))
        (let ((avg-temp (let ((temps (cl-loop for m in measurements
                                               when (equal (msr-sensor m) "temp")
                                               collect (msr-value m))))
                          (if temps (/ (cl-loop for t in temps sum t) (length temps)) 0.0)))
              (high-q-count (cl-loop for m in measurements count (> (msr-quality m) 0.9))))
          (dolist (m measurements)
            (when (equal (msr-sensor m) "temp")
              (setf (msr-value m) (+ (msr-value m) 1.0))))
          (let ((new-avg (let ((temps (cl-loop for m in measurements
                                               when (equal (msr-sensor m) "temp")
                                               collect (msr-value m))))
                          (if temps (/ (cl-loop for t in temps sum t) (length temps)) 0.0))))
            (goto-char 8)
            (insert (format "avg=%.2f->%.2f:hq=%d" avg-temp new-avg high-q-count))
            (setf (marker-position m) 12)
            (put-text-property 8 (+ 8 (length (format "avg=%.2f->%.2f:hq=%d" avg-temp new-avg high-q-count)))
                              'pcase-result t)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (temp-vals (mapcar (lambda (m) (and (equal (msr-sensor m) "temp") (msr-value m)))
                                 meas)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs temp-vals (reverse classified)
                (marker-position m)
                (buffer-string)
                meas)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_cl_loop_hash_objects_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass group ()
    ((name :initarg :name :accessor grp-name :initform "")
     (members :initarg :members :accessor grp-members :initform nil)))
  (defclass person ()
    ((name :initarg :name :accessor per-name :initform "")
     (age :initarg :age :accessor per-age :initform 0)))
  (let* ((buf (generate-new-buffer "lp5"))
         (p1 (person :name "alice" :age 30))
         (p2 (person :name "bob" :age 25))
         (p3 (person :name "carol" :age 35))
         (p4 (person :name "dave" :age 28))
         (g1 (group :name "devs" :members (list p1 p2)))
         (g2 (group :name "ops" :members (list p3 p4)))
         (groups (list g1 g2)))
    (with-current-buffer buf
      (insert "GROUPS:devs:ops")
      (put-text-property 1 7 'field 'header)
      (put-text-property 8 12 'field 'g1)
      (put-text-property 13 16 'field 'g2)
      (setq-local my-groups groups)
      (setq-local my-people (list p1 p2 p3 p4))
      (let* ((ov (make-overlay 8 16))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 10))
             (group-summary (cl-loop for g in groups
                                     collect (cons (grp-name g)
                                                   (cl-loop for p in (grp-members g)
                                                            collect (cons (per-name p) (per-age p))))))
             (total-members (cl-loop for g in groups sum (length (grp-members g))))
             (avg-age (/ (cl-loop for g in groups
                                  sum (cl-loop for p in (grp-members g) sum (per-age p)))
                         total-members)))
        (undo-boundary)
        (dolist (p (list p1 p2 p3 p4))
          (setf (per-age p) (+ (per-age p) 1)))
        (push p3 (grp-members g1))
        (let ((new-summary (cl-loop for g in my-groups
                                     collect (cons (grp-name g)
                                                   (length (grp-members g)))))
              (new-avg (/ (cl-loop for g in my-groups
                                    sum (cl-loop for p in (grp-members g) sum (per-age p)))
                          (cl-loop for g in my-groups sum (length (grp-members g))))))
          (goto-char 8)
          (insert (format "%s->%s:avg=%.1f->%.1f"
                         group-summary new-summary avg-age new-avg))
          (setf (marker-position m) 12)
          (put-text-property 8 (+ 8 (length (format "%s->%s:avg=%.1f->%.1f"
                                                      group-summary new-summary avg-age new-avg)))
                            'nested-result t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (g1-count (length (grp-members (car my-groups))))
              (g2-count (length (grp-members (cadr my-groups))))
              (ages (mapcar (lambda (p) (per-age p)) my-people)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs g1-count g2-count ages
                (marker-position m)
                (buffer-string)
                my-groups my-people)))
      (kill-buffer buf))))"#,
        expect,
    );
}
