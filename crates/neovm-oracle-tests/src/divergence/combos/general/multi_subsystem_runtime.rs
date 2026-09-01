//! Large multi-subsystem combo parity (4-6 subsystems per form):
//! process->buffer->textprop->marker->undo, timer->buffer->overlay,
//! org->table->clock->property, process->decode->search->replace,
//! buffer->marker->narrow->undo->widen, hash->closure->cl-loop->sort,
//! string split->encode->upcase->sort->join, textprop->overlay->field.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn buffer_marker_narrow_undo_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"AB1234567CD\" \"0AB1234567CD89\" 5 9)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789")
  (let ((m1 (copy-marker 3)) (m2 (copy-marker 7 t)))
    (narrow-to-region 2 9)
    (goto-char (point-min)) (insert "AB") (undo-boundary)
    (goto-char (point-max)) (insert "CD") (undo-boundary)
    (let ((before (buffer-string)))
      (widen)
      (primitive-undo 1 buffer-undo-list)
      (list before (buffer-string) (marker-position m1) (marker-position m2)))))"##,
        expect,
    );
}

#[test]
fn hash_closure_loop_sort_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK ((\"apple\" . 3) (\"banana\" . 2) (\"cherry\" . 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(let ((h (make-hash-table :test 'equal)) (words '("apple" "banana" "apple" "cherry" "banana" "apple")))
  (dolist (w words) (cl-incf (gethash w h 0)))
  (let ((pairs nil))
    (maphash (lambda (k v) (push (cons k v) pairs)) h)
    (sort pairs (lambda (a b) (if (= (cdr a) (cdr b)) (string< (car a) (car b)) (> (cdr a) (cdr b)))))))"##,
        expect,
    );
}

#[test]
fn org_table_clock_property_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (user-error \"Not at a table\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (require 'org-clock)
  (with-temp-buffer (org-mode)
    (insert "* Project :work:\n")
    (org-entry-put (point-min) "Effort" "2:00")
    (goto-char (point-max))
    (insert "| item | cost |\n|------+------|\n| a | 10 |\n| b | 20 |\n|------+------|\n| sum |    |\n")
    (insert "#+TBLFM: @6$2=vsum(@3..@4)\n")
    (forward-line -1) (org-table-recalculate t) (org-table-align)
    (list (org-entry-get (point-min) "Effort")
          (org-get-tags)
          (string-match "30" (buffer-string)))))"##,
        expect,
    );
}

#[test]
fn proc_buffer_textprop_marker_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"X alpha beta gamma\\nProcess neo-cb1-xxx finished\\n\" 2 48 (face bold)) 9 bold nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((buf (generate-new-buffer " neo-cb1-xxx")))
  (with-current-buffer buf
    (buffer-enable-undo)
    (let ((proc (start-process "neo-cb1-xxx" buf "printf" "alpha beta gamma")))
      (set-process-query-on-exit-flag proc nil)
      (while (process-live-p proc) (accept-process-output proc 1))
      (goto-char (point-min))
      (put-text-property (point-min) (point-max) 'face 'bold)
      (let ((m (copy-marker 7)))
        (goto-char (point-min)) (insert "X ")
        (undo-boundary)
        (let ((r (list (buffer-string)
                       (marker-position m)
                       (get-text-property 3 'face)
                       (= (point-max) 19))))
          (prog1 r (kill-buffer buf)))))))"##,
        expect,
    );
}

#[test]
fn proc_decode_search_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"one\\nTwo\\nThree\" 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((acc ""))
  (let ((proc (make-process :name "neo-cb4-xxx" :command '("printf" "one\\ntwo\\nthree")
               :connection-type 'pipe :coding 'utf-8
               :filter (lambda (_p s) (setq acc (concat acc s))))))
    (set-process-query-on-exit-flag proc nil)
    (while (process-live-p proc) (accept-process-output proc 1))
    (with-temp-buffer
      (insert acc)
      (goto-char (point-min))
      (while (re-search-forward "^t" nil t) (replace-match "T"))
      (list (buffer-string) (count-lines (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn string_pipeline_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"Café\" \"Zürich\" \"Ños\") (5 7 4) (\"CAFÉ\" \"ZÜRICH\" \"ÑOS\") \"CAFÉ|ZÜRICH|ÑOS\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(let* ((input "Café, Zürich;Ños")
        (parts (split-string input "[,;] *"))
        (encoded (mapcar (lambda (s) (length (encode-coding-string s 'utf-8))) parts))
        (upped (mapcar #'upcase parts))
        (joined (mapconcat #'identity (sort (copy-sequence upped) #'string<) "|")))
  (list parts encoded upped joined))"##,
        expect,
    );
}

#[test]
fn textprop_overlay_field_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"bbbb\" 6 10 (#<overlay in no buffer>) italic)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "aaaa bbbb cccc")
  (put-text-property 1 5 'field 'f1)
  (put-text-property 6 10 'field 'f2)
  (let ((ov (make-overlay 6 10)))
    (overlay-put ov 'face 'italic)
    (goto-char 7)
    (list (field-string-no-properties) (field-beginning) (field-end)
          (overlays-at 7) (get-char-property 7 'face))))"##,
        expect,
    );
}

#[test]
fn timer_buffer_overlay_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"hello world test!\" 1 6 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((buf (generate-new-buffer " neo-cb2-xxx")) (fired nil))
  (with-current-buffer buf
    (insert "hello world test")
    (let ((ov (make-overlay 1 6)))
      (overlay-put ov 'priority 5)
      (run-with-timer 0.02 nil (lambda () (with-current-buffer buf (goto-char (point-max)) (insert "!") (setq fired t))))
      (let ((k 0)) (while (and (not fired) (< k 100)) (accept-process-output nil 0.02) (setq k (1+ k))))
      (prog1 (list (buffer-string) (overlay-start ov) (overlay-end ov) fired)
        (kill-buffer buf)))))"##,
        expect,
    );
}
