//! Complex combo batch 75 — window configurations, frame configurations,
//! `with-current-buffer`/`with-temp-buffer` interactions, window live/update,
//! minibuffer interactions, and `current-window-configuration` round-trips.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx75_window_configuration_save_and_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 1 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((config (current-window-configuration))
       (orig-buffer (current-buffer)))
  (split-window)
  (let ((n-with-split (length (window-list))))
    (set-window-configuration config)
    (let ((n-restored (length (window-list))))
      (list n-with-split n-restored
            (eq orig-buffer (window-buffer (selected-window)))))))
"##,
        expect,
    );
}

#[test]
fn div_cx75_with_current_buffer_preserves_origin_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t \"in other\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((origin (current-buffer))
      (other (get-buffer-create " *neo-cx75-other*")))
  (with-current-buffer other
    (insert "in other"))
  (let ((during-origin (current-buffer)))
    (prog1 (list (eq during-origin origin)
                 (buffer-live-p other)
                 (with-current-buffer other (buffer-string)))
      (kill-buffer other))))
"##,
        expect,
    );
}

#[test]
fn div_cx75_with_temp_buffer_isolation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil \"isolated\" t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((origin (current-buffer)))
  (with-temp-buffer
    (insert "isolated")
    (let ((temp-buffer (current-buffer)))
      (list (eq origin (current-buffer))
            (buffer-string)
            (buffer-live-p temp-buffer)
            (eq origin (current-buffer))))))
"##,
        expect,
    );
}

#[test]
fn div_cx75_save_window_excursion_restores_split() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((n-before (length (window-list))))
  (save-window-excursion
    (split-window)
    (let ((n-inside (length (window-list))))
      (list n-before n-inside)))
  (let ((n-after (length (window-list))))
    n-after))
"##,
        expect,
    );
}

#[test]
fn div_cx75_get_buffer_window_and_buffer_windows() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#<window 1 on *scratch*> (#<window 1 on *scratch*>) (#<window 1 on *scratch*>) t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx75-gw*")))
  (set-window-buffer (selected-window) buf)
  (let ((win-of-buf (get-buffer-window buf))
        (wins-of-buf (get-buffer-window-list buf))
        (all-windows (window-list)))
    (prog1 (list win-of-buf wins-of-buf all-windows
                 (eq win-of-buf (selected-window))
                 (eq (window-buffer (selected-window)) buf))
      (kill-buffer buf))))
"##,
        expect,
    );
}

#[test]
fn div_cx75_window_dedicated_p_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (let ((ded-before (window-dedicated-p win)))
    (set-window-dedicated-p win t)
    (let ((ded-true (window-dedicated-p win)))
      (set-window-dedicated-p win nil)
      (let ((ded-false (window-dedicated-p win)))
        (set-window-dedicated-p win (if (numberp ded-before) 99 t))
        (list ded-before ded-true ded-false (window-dedicated-p win))))))
"##,
        expect,
    );
}

#[test]
fn div_cx75_minibuffer_setup_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-type-argument)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (active-minibuffer-window)
          (minibufferp (minibuffer-window))
          (window-minibuffer-p (minibuffer-window))
          (eq (window-buffer (minibuffer-window))
              (window-buffer (minibuffer-window))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx75_set_window_buffer_dont_change_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf-a (get-buffer-create " *neo-cx75-a*"))
      (buf-b (get-buffer-create " *neo-cx75-b*")))
  (with-current-buffer buf-a
    (erase-buffer)
    (insert "AAAA"))
  (with-current-buffer buf-b
    (erase-buffer)
    (insert "BBBB")
    (goto-char 3))
  (set-window-buffer (selected-window) buf-a)
  (set-window-buffer (selected-window) buf-b)
  (let ((p-in-b (window-point (selected-window))))
    (kill-buffer buf-a)
    (kill-buffer buf-b)
    p-in-b))
"##,
        expect,
    );
}

#[test]
fn div_cx75_window_start_end_and_set_window_start() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (50 800 100)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx75-ws*")))
  (with-current-buffer buf
    (erase-buffer)
    (insert (mapconcat #'identity (make-list 50 "line of content") "\n")))
  (set-window-buffer (selected-window) buf)
  (set-window-start (selected-window) 50)
  (let ((start-1 (window-start (selected-window)))
        (end-1 (window-end (selected-window) t)))
    (set-window-start (selected-window) 100)
    (let ((start-2 (window-start (selected-window))))
      (prog1 (list start-1 end-1 start-2)
        (kill-buffer buf)))))
"##,
        expect,
    );
}

#[test]
fn div_cx75_buffer_display_count_and_kill_buffer_queries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil #<killed buffer> t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx75-dc*")))
  (list (buffer-live-p buf)
        (buffer-modified-p buf)
        (get-buffer " *neo-cx75-dc*")
        (eq (get-buffer " *neo-cx75-dc*") buf)
        (progn (kill-buffer buf) (get-buffer " *neo-cx75-dc*"))))
"##,
        expect,
    );
}

#[test]
fn div_cx75_buffer_list_order_and_bury_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"*scratch*\" \" *Minibuf-0*\" \"*Messages*\" \" *neovm-oracle-form*\" \" *neo-cx75-ba*\" \" *neo-cx75-bb*\") (\"*scratch*\" \" *Minibuf-0*\" \"*Messages*\" \" *neovm-oracle-form*\" \" *neo-cx75-bb*\" \" *neo-cx75-ba*\") (\" *neo-cx75-ba*\" \" *neo-cx75-bb*\") (\" *neo-cx75-ba*\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf-a (get-buffer-create " *neo-cx75-ba*"))
      (buf-b (get-buffer-create " *neo-cx75-bb*")))
  (let ((list-before (mapcar #'buffer-name (buffer-list))))
    (bury-buffer buf-a)
    (let ((list-after (mapcar #'buffer-name (buffer-list))))
      (kill-buffer buf-a)
      (kill-buffer buf-b)
      (list list-before list-after
            (member " *neo-cx75-ba*" list-before)
            (member " *neo-cx75-ba*" list-after)))))
"##,
        expect,
    );
}

#[test]
fn div_cx75_window_config_save_excursion_marker_overlay_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx75-mega*")))
  (with-current-buffer buf
    (buffer-enable-undo)
    (insert "Window config test buffer content here")
    (put-text-property 1 5 'face 'bold)
    (put-text-property 8 12 'display "XX")
    (let ((m (set-marker (make-marker) 18))
          (ov (make-overlay 4 24)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (let ((config (current-window-configuration)))
        (set-window-buffer (selected-window) buf)
        (narrow-to-region 3 30)
        (let ((state (list (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (point-min) (point-max)
                           (text-properties-at 1)
                           (window-buffer (selected-window)))))
          (set-window-configuration config)
          (widen)
          (list state
                (eq (window-buffer (selected-window)) buf)
                (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (point-min) (point-max)))))
    (kill-buffer buf)))
"##,
        expect,
    );
}
