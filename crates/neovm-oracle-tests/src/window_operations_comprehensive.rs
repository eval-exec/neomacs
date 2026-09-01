//! Oracle parity tests for window/buffer interaction primitives.
//!
//! Tests: selected-window, window-buffer, window-point, window-start,
//! window-end, set-window-point, window-list, window-dedicated-p,
//! window-parameter, set-window-parameter, window-parameters,
//! window-live-p, window-width, window-height, and combinations thereof.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// selected-window basic properties and identity checks
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_window_selected_window_basic_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((w (selected-window))
       (is-window (windowp w))
       (is-live (window-live-p w))
       (same-again (eq w (selected-window)))
       (buf (window-buffer w))
       (buf-is-current (eq buf (current-buffer)))
       (has-point (integerp (window-point w)))
       (point-matches (= (window-point w) (point))))
  (list is-window is-live same-again buf-is-current has-point point-matches))
"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// window-buffer with various argument types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_window_buffer_interactions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "temp content for window-buffer test")
  (let* ((orig-buf (window-buffer (selected-window)))
         (temp-buf (current-buffer))
         (temp-name (buffer-name temp-buf))
         (orig-name (buffer-name orig-buf))
         (different-bufs (not (eq orig-buf temp-buf)))
         (window-buf-type (type-of (window-buffer)))
         (nil-arg-same (eq (window-buffer) (window-buffer (selected-window)))))
    (list different-bufs
          (stringp temp-name)
          (stringp orig-name)
          (eq window-buf-type 'buffer)
          nil-arg-same
          (> (length temp-name) 0))))
"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// window-point and set-window-point with boundary conditions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_window_point_set_and_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "abcdefghijklmnopqrstuvwxyz\nline two\nline three\nline four")
  (let* ((w (selected-window))
         (initial-point (window-point w))
         (_ (set-window-point w 5))
         (after-set-5 (window-point w))
         (_ (set-window-point w 1))
         (at-start (window-point w))
         (_ (set-window-point w (point-max)))
         (at-end (window-point w))
         (end-val (point-max))
         (_ (set-window-point w 15))
         (at-15 (window-point w))
         (_ (goto-char 20))
         (point-after-goto (window-point w))
         (point-eq-point (= (window-point w) (point))))
    (list (integerp initial-point)
          (= after-set-5 5)
          (= at-start 1)
          (= at-end end-val)
          (= at-15 15)
          (= point-after-goto 20)
          point-eq-point)))
"#;
    let expect = expect_test::expect![[r#""OK (t nil t nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// window-start and window-end relative positioning
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_window_start_end_relationship() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "first line\nsecond line\nthird line\nfourth line\nfifth line")
  (let* ((w (selected-window))
         (start (window-start w))
         (start-is-int (integerp start))
         (start-positive (>= start 1))
         (start-in-range (<= start (point-max))))
    (list start-is-int
          start-positive
          start-in-range
          (= start (window-start)))))
"#;
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_window_end_never_exceeds_point_max() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(save-window-excursion
  (with-temp-buffer
    (insert "one line without a trailing newline")
    (set-window-buffer (selected-window) (current-buffer))
    (goto-char (point-max))
    (redisplay t)
    (let ((end (window-end nil t)))
      (list (integerp end)
            (<= (point-min) end)
            (<= end (point-max))))))
"#;
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_window_end_uses_character_positions_for_multibyte_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(save-window-excursion
  (with-temp-buffer
    (insert "//! Unicode — repro\nsecond line\n")
    (set-window-buffer (selected-window) (current-buffer))
    (goto-char (point-min))
    (redisplay t)
    (let ((end (window-end nil t)))
      (list (integerp end)
            (= end (point-max))
            (= (length (buffer-substring (point-min) end))
               (1- end))
            (= (char-after 13) #x2014)))))
"#;
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// window-list returns proper list of live windows
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_window_list_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((wl (window-list))
       (is-list (listp wl))
       (non-empty (> (length wl) 0))
       (all-windows (let ((ok t))
                      (dolist (w wl)
                        (unless (windowp w) (setq ok nil)))
                      ok))
       (all-live (let ((ok t))
                   (dolist (w wl)
                     (unless (window-live-p w) (setq ok nil)))
                   ok))
       (selected-in-list (let ((found nil))
                           (dolist (w wl)
                             (when (eq w (selected-window))
                               (setq found t)))
                           found))
       (all-have-buffers (let ((ok t))
                           (dolist (w wl)
                             (unless (bufferp (window-buffer w))
                               (setq ok nil)))
                           ok)))
  (list is-list non-empty all-windows all-live selected-in-list all-have-buffers))
"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// window-dedicated-p defaults and type
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_window_dedicated_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((w (selected-window))
       (ded (window-dedicated-p w))
       (ded-nil-arg (window-dedicated-p))
       (both-same (eq ded ded-nil-arg)))
  (list ded both-same (or (null ded) (eq ded t) t)))
"#;
    let expect = expect_test::expect![[r#""OK (nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// window-parameter / set-window-parameter / window-parameters round-trip
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_window_parameters_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((w (selected-window))
       ;; Save original parameters to restore later
       (orig-params (copy-sequence (window-parameters w)))
       (before-get (window-parameter w 'neovm-test-param-xyz))
       (_ (set-window-parameter w 'neovm-test-param-xyz 42))
       (after-set (window-parameter w 'neovm-test-param-xyz))
       (_ (set-window-parameter w 'neovm-test-param-xyz "hello"))
       (after-string (window-parameter w 'neovm-test-param-xyz))
       (params-alist (window-parameters w))
       (has-param (assq 'neovm-test-param-xyz params-alist))
       (_ (set-window-parameter w 'neovm-test-param-xyz nil))
       (after-nil (window-parameter w 'neovm-test-param-xyz))
       ;; Set multiple parameters
       (_ (set-window-parameter w 'neovm-test-a 100))
       (_ (set-window-parameter w 'neovm-test-b '(1 2 3)))
       (val-a (window-parameter w 'neovm-test-a))
       (val-b (window-parameter w 'neovm-test-b))
       ;; Cleanup
       (_ (set-window-parameter w 'neovm-test-param-xyz nil))
       (_ (set-window-parameter w 'neovm-test-a nil))
       (_ (set-window-parameter w 'neovm-test-b nil)))
  (list (null before-get)
        (= after-set 42)
        (equal after-string "hello")
        (not (null has-param))
        (null after-nil)
        (= val-a 100)
        (equal val-b '(1 2 3))
        (listp params-alist)))
"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// window-width and window-height return positive integers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_window_dimensions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((w (selected-window))
       (width (window-width w))
       (height (window-height w))
       (width-nil (window-width))
       (height-nil (window-height))
       (width-total (window-total-width w))
       (height-total (window-total-height w)))
  (list (integerp width)
        (integerp height)
        (> width 0)
        (> height 0)
        (= width width-nil)
        (= height height-nil)
        (integerp width-total)
        (integerp height-total)
        (>= width-total width)
        (>= height-total height)))
"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// window-live-p on selected vs nil and type dispatch
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_window_live_p_type_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list (window-live-p (selected-window))
      (window-live-p nil)
      (window-live-p t)
      (window-live-p 42)
      (window-live-p "hello")
      (window-live-p '(a b))
      (window-live-p (make-hash-table))
      (windowp (selected-window))
      (windowp nil)
      (windowp 42))
"#;
    let expect = expect_test::expect![[r#""OK (t nil nil nil nil nil nil t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: buffer switch + window-point preservation pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_window_buffer_switch_point_preservation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((orig-buf (current-buffer))
       (orig-point (point))
       (results nil))
  (with-temp-buffer
    (insert "alpha beta gamma delta epsilon")
    (goto-char 12)
    (let ((tb (current-buffer))
          (tp (point)))
      (push (list 'in-temp (buffer-name tb) tp) results)))
  ;; After with-temp-buffer we should be back
  (push (list 'back (eq (current-buffer) orig-buf) (= (point) orig-point)) results)
  ;; Build result
  (nreverse results))
"#;
    let expect = expect_test::expect![[r#""OK ((in-temp \" *temp*\" 12) (back t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: window-point across multiple save-excursion + set-window-point
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_window_point_nested_save_excursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "0123456789abcdefghijklmnopqrstuvwxyz")
  (goto-char 1)
  (let* ((w (selected-window))
         (p1 (window-point w))
         (_ (save-excursion
              (goto-char 10)
              (set-window-point w 20)))
         (p2 (window-point w))
         (_ (save-excursion
              (goto-char 5)
              (save-excursion
                (goto-char 30)
                (set-window-point w 15))))
         (p3 (window-point w)))
    (list (= p1 1)
          ;; set-window-point in save-excursion persists
          (= p2 20)
          (= p3 15))))
"#;
    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// window-buffer identity: every window in window-list has a buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_window_all_windows_have_valid_buffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((results nil))
  (dolist (w (window-list))
    (let* ((buf (window-buffer w))
           (name (buffer-name buf))
           (live (buffer-live-p buf))
           (pt (window-point w)))
      (push (list (windowp w)
                  (bufferp buf)
                  (stringp name)
                  live
                  (integerp pt)
                  (>= pt 1))
            results)))
  (list (> (length results) 0)
        ;; All entries should be (t t t t t t)
        (let ((ok t))
          (dolist (r results)
            (dolist (v r)
              (unless v (setq ok nil))))
          ok)))
"#;
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: window parameters as a mini key-value store
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_window_parameters_as_kv_store() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((w (selected-window))
      (keys '(neovm-kv-a neovm-kv-b neovm-kv-c neovm-kv-d neovm-kv-e))
      (vals '(1 "two" (3 3 3) [4 4] t)))
  (unwind-protect
      (progn
        ;; Store all key-value pairs
        (let ((ks keys) (vs vals))
          (while ks
            (set-window-parameter w (car ks) (car vs))
            (setq ks (cdr ks) vs (cdr vs))))
        ;; Read them back and verify
        (let ((retrieved (mapcar (lambda (k) (window-parameter w k)) keys))
              (params (window-parameters w)))
          (list
           ;; All values match
           (equal retrieved vals)
           ;; All keys present in window-parameters alist
           (let ((ok t))
             (dolist (k keys)
               (unless (assq k params) (setq ok nil)))
             ok)
           ;; Count of our params
           (length (seq-filter (lambda (p) (memq (car p) keys)) params)))))
    ;; Cleanup
    (dolist (k keys)
      (set-window-parameter w k nil))))
"#;
    let expect = expect_test::expect![[r#""OK (t t 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
