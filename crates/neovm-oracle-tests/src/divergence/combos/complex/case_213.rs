//! Complex combo batch 213 — `desktop` / `session` / `savehist` /
//! `recentf` / `saveplace` file-based persistence round-trips.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx213_desktop_save_read_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'desktop)
      (let* ((dir (make-temp-file "neo-cx213-dtop" t))
             (desktop-base-file-name "neo-cx213.desktop")
             (desktop-dirname dir)
             (desktop-restore-frames nil))
        (let ((buf (get-buffer-create " *neo-cx213-dtop*")))
          (with-current-buffer buf
            (insert "desktop test content")
            (goto-char 5)))
        (condition-case err
            (desktop-save dir)
          (error :save-err))
        (let ((saved (file-exists-p (expand-file-name desktop-base-file-name dir))))
          (kill-buffer buf)
          (delete-directory dir t)
          (list saved)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx213_session_save_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored file-missing)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'session)
      (list (fboundp 'session-save)
            (fboundp 'session-initialize)
            (boundp 'session-save-file)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx213_savehist_save_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'savehist)
      (list (fboundp 'savehist-save)
            (fboundp 'savehist-load)
            (boundp 'savehist-file)
            (boundp 'savehist-additional-variables)
            (boundp 'savehist-minibuffer-history-variables)
            (boundp 'savehist-coding-system)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx213_recentf_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'recentf)
      (list (fboundp 'recentf-mode)
            (fboundp 'recentf-save-list)
            (fboundp 'recentf-cleanup)
            (fboundp 'recentf-add-file)
            (fboundp 'recentf-include-p)
            (boundp 'recentf-list)
            (boundp 'recentf-max-saved-items)
            (boundp 'recentf-save-file)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx213_saveplace_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'saveplace)
      (list (fboundp 'save-place-mode)
            (fboundp 'save-place-to-file)
            (fboundp 'load-save-place-alist-from-file)
            (boundp 'save-place-file)
            (boundp 'save-place-alist)
            (boundp 'save-place-version-control)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx213_recentf_add_and_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"/tmp/neo-cx213-recentf.txt\") t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'recentf)
      (let ((test-path "/tmp/neo-cx213-recentf.txt"))
        (recentf-add-file test-path)
        (list (member test-path recentf-list)
              (recentf-include-p test-path))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx213_savehist_variable_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (boundp 'minibuffer-history)
          (boundp 'file-name-history)
          (boundp 'extended-command-history)
          (boundp 'command-history)
          (boundp 'search-ring)
          (boundp 'regexp-search-ring)
          (boundp 'kill-ring)
          (boundp 'shell-command-history))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx213_desktop_buffer_state_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'desktop)
      (list (boundp 'desktop-buffer-mode-handlers)
            (boundp 'desktop-minor-mode-table)
            (boundp 'desktop-locals-to-save)
            (boundp 'desktop-missing-file-warning)
            (boundp 'desktop-restore-frames)
            (boundp 'desktop-restore-in-desktop-display)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx213_saveplace_visited_buffers_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'saveplace)
      (list (boundp 'save-place)
            (boundp 'save-place-loaded)
            (boundp 'save-place-limit)
            (fboundp 'toggle-save-place)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx213_persistence_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'savehist)
      (require 'recentf)
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "Persistence mega test buffer content")
        (put-text-property 1 6 'face 'bold)
        (let ((m (set-marker (make-marker) 8))
              (ov (make-overlay 4 14)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 18)
          (let ((state (list (fboundp 'savehist-save)
                             (boundp 'savehist-file)
                             (boundp 'recentf-list)
                             (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1))))
            (undo)
            (widen)
            (list state (buffer-string) (marker-position m)
                  (overlay-start ov) (overlay-end ov)
                  (text-properties-at 1))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}
