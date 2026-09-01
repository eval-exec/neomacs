//! Complex combo batch 247 — `process` filter with `set-process-filter` /
//! `process-filter` / `set-process-sentinel` / `process-sentinel` /
//! `set-process-plist` / `process-plist` deep queries.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx247_set_process_filter_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-process :name "neo-cx247-flt"
                        :command '("echo" "test")
                        :buffer (generate-new-buffer " *neo-cx247-flt*"))))
  (let ((my-filter (lambda (proc data) nil)))
    (set-process-filter p my-filter)
    (let ((got (process-filter p)))
      (set-process-filter p nil)
      (let ((after-clear (process-filter p)))
        (prog1 (list (eq got my-filter)
                     (null after-clear))
          (delete-process p)
          (kill-buffer (process-buffer p))))))
"##,
        expect,
    );
}

#[test]
fn div_cx247_set_process_sentinel_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-process :name "neo-cx247-sent"
                        :command '("echo" "test"))))
  (let ((my-sentinel (lambda (proc event) nil)))
    (set-process-sentinel p my-sentinel)
    (let ((got (process-sentinel p)))
      (set-process-sentinel p nil)
      (let ((after-clear (process-sentinel p)))
        (prog1 (list (eq got my-sentinel)
                     (null after-clear))
          (delete-process p)))))
"##,
        expect,
    );
}

#[test]
fn div_cx247_process_plist_set_get_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:val1 :val2 99 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-process :name "neo-cx247-plist" :command '("echo" "test"))))
  (process-put p 'neo-cx247-key1 :val1)
  (process-put p 'neo-cx247-key2 :val2)
  (process-put p 'neo-cx247-key3 99)
  (let ((v1 (process-get p 'neo-cx247-key1))
        (v2 (process-get p 'neo-cx247-key2))
        (v3 (process-get p 'neo-cx247-key3))
        (missing (process-get p 'missing)))
    (delete-process p)
    (list v1 v2 v3 missing)))
"##,
        expect,
    );
}

#[test]
fn div_cx247_process_query_before_and_after_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((run open listen connect stop) \"neo-cx247-q\" (\"sh\" \"-c\" \"echo start; exit 5\") t t nil exit 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-process :name "neo-cx247-q"
                        :command '("sh" "-c" "echo start; exit 5"))))
  (list (process-live-p p)
        (process-name p)
        (process-command p)
        (accept-process-output p 2)
        (sit-for 0.05)
        (process-live-p p)
        (process-status p)
        (process-exit-status p)))
"##,
        expect,
    );
}

#[test]
fn div_cx247_process_type_and_connection_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-process :name "neo-cx247-type" :command '("echo" "test"))))
  (list (processp p)
        (process-type p)
        (process-tty-name p)
        (process-contact p))
  (delete-process p))
"##,
        expect,
    );
}

#[test]
fn div_cx247_process_list_and_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((before (length (process-list))))
  (let ((p1 (make-process :name "neo-cx247-l1" :command '("echo" "1")))
        (p2 (make-process :name "neo-cx247-l2" :command '("echo" "2")))
        (p3 (make-process :name "neo-cx247-l3" :command '("echo" "3"))))
    (let ((after-add (length (process-list))))
      (dolist (p (list p1 p2 p3)) (delete-process p))
      (let ((after-del (length (process-list))))
        (list before after-add after-del
              (>= after-add (+ 3 before))
              (>= (1- after-add) after-del)))))
"##,
        expect,
    );
}

#[test]
fn div_cx247_process_coding_system_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (utf-8-unix . utf-8-unix)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((buf (get-buffer-create " *neo-cx247-cs*"))
       (p (make-process :name "neo-cx247-cs"
                        :command '("echo" "coding")
                        :buffer buf)))
  (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
  (let ((cs (process-coding-system p)))
    (delete-process p)
    (kill-buffer buf)
    cs))
"##,
        expect,
    );
}

#[test]
fn div_cx247_process_mark_position_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 39 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((buf (get-buffer-create " *neo-cx247-mark*"))
       (p (make-process :name "neo-cx247-mark"
                        :command '("echo" "mark")
                        :buffer buf)))
  (accept-process-output p 2)
  (sit-for 0.05)
  (let ((mark (process-mark p)))
    (prog1 (list (markerp mark)
                 (marker-position mark)
                 (eq (marker-buffer mark) buf))
      (delete-process p)
      (kill-buffer buf))))
"##,
        expect,
    );
}

#[test]
fn div_cx247_send_string_and_eof_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"alpha beta gamma\\n\\nProcess neo-cx247-eof finished\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((buf (get-buffer-create " *neo-cx247-eof*"))
       (p (make-process :name "neo-cx247-eof"
                        :command '("cat")
                        :buffer buf
                        :connection-type 'pipe)))
  (process-send-string p "alpha beta gamma\n")
  (process-send-eof p)
  (accept-process-output p 2)
  (sit-for 0.05)
  (let ((content (string-trim (with-current-buffer buf (buffer-string)))))
    (kill-buffer buf)
    content))
"##,
        expect,
    );
}

#[test]
fn div_cx247_process_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((buf (get-buffer-create " *neo-cx247-mega*")))
  (with-current-buffer buf
    (buffer-enable-undo)
    (insert "Process mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t))
    (narrow-to-region 2 18))
  (let ((p (make-process :name "neo-cx247-mega-p"
                         :command '("sh" "-c" "printf 'PROC'")
                         :buffer buf)))
    (process-put p 'neo-cx247-tag :mega)
    (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
    (accept-process-output p 1)
    (sit-for 0.05))
  (let ((content (with-current-buffer buf (buffer-string))))
    (with-current-buffer buf
      (widen)
      (let ((state (list content (length content)
                         (length (overlays-in 1 20))
                         (text-properties-at 1))))
        (undo)
        (kill-buffer buf)
        (list state (buffer-string)))))))
"##,
        expect,
    );
}
