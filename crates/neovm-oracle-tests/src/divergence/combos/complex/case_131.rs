//! Complex combo batch 131 — `bookmark` persistence, `recentf` list,
//! `savehist` variables, `saveplace` locations, `desktop` save format.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx131_bookmark_set_get_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'bookmark)
      (with-temp-buffer
        (insert "bookmark test content here")
        (goto-char 10)
        (bookmark-set "neo-cx131-bm"))
      (let ((bm (bookmark-get-bookmark "neo-cx131-bm")))
        (list bm
              (bookmark-get-position "neo-cx131-bm")
              (bookmark-get-filename "neo-cx131-bm")
              (assoc "neo-cx131-bm" bookmark-alist))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx131_bookmark_record_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'bookmark)
      (bookmark-set "neo-cx131-record")
      (let* ((entry (bookmark-get-bookmark "neo-cx131-record"))
             (props (cdr entry)))
        (list (consp entry)
              (stringp (car entry))
              (consp props))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx131_recentf_add_file() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"/tmp/neo-cx131-recentf-test.txt\") t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'recentf)
      (let ((path "/tmp/neo-cx131-recentf-test.txt"))
        (recentf-add-file path)
        (list (member path recentf-list)
              (recentf-include-p path))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx131_savehist_save_variable_to_file_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'savehist)
      (list (fboundp 'savehist-save)
            (boundp 'savehist-coding-system)
            (boundp 'savehist-minibuffer-history-variables)
            (boundp 'savehist-additional-variables)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx131_saveplace_save_location_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'saveplace)
      (list (fboundp 'save-place-to-file)
            (boundp 'save-place-alist)
            (boundp 'save-place-version-control)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx131_desktop_save_buffer_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'desktop)
      (with-temp-buffer
        (insert "desktop test content")
        (let ((state (desktop-buffer 1 (current-buffer) "temp" nil nil nil)))
          (list state
                (fboundp 'desktop-save)
                (boundp 'desktop-buffer-mode-handlers)
                (boundp 'desktop-minor-mode-table))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx131_bookmark_default_file_path() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-variable)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (stringp bookmark-default-file)
          (boundp 'bookmark-version-control)
          (boundp 'bookmark-save-flag))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx131_history_variables_savehist_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (boundp 'savehist-minibuffer-history-variables)
          (boundp 'minibuffer-history)
          (boundp 'extended-command-history)
          (boundp 'file-name-history)
          (boundp 'command-history)
          (boundp 'search-ring)
          (boundp 'regexp-search-ring))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx131_saveplace_visited_buffers_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'saveplace)
      (with-temp-buffer
        (insert "buffer content")
        (goto-char 5)
        (let ((result (condition-case err
                          (save-place-mode 1)
                        (error :no-mode))))
          (list result
                (boundp 'save-place)
                (boundp 'save-place-loaded)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx131_bookmark_propagate_after_kill_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'bookmark)
      (let ((buf (get-buffer-create " *neo-cx131-prop*")))
        (with-current-buffer buf
          (insert "content for bookmark propagation test")
          (goto-char 8)
          (bookmark-set "neo-cx131-prop"))
        (let ((pos-before-kill (bookmark-get-position "neo-cx131-prop")))
          (kill-buffer buf)
          (let ((pos-after-kill (bookmark-get-position "neo-cx131-prop")))
            (list pos-before-kill pos-after-kill
                  (bookmark-get-bookmark "neo-cx131-prop"))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx131_bookmark_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'bookmark)
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "Bookmark mega test buffer content here")
        (put-text-property 1 8 'face 'bold)
        (let ((m (set-marker (make-marker) 12))
              (ov (make-overlay 4 18)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 25)
          (bookmark-set "neo-cx131-mega")
          (let ((state (list (bookmark-get-position "neo-cx131-mega")
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

#[test]
fn div_cx131_recentf_cleanup_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'recentf)
      (let ((fake-path "/tmp/neo-cx131-cleanup-file.txt"))
        (list (recentf-include-p fake-path)
              (fboundp 'recentf-cleanup)
              (boundp 'recentf-exclude)
              (boundp 'recentf-max-saved-items))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}
