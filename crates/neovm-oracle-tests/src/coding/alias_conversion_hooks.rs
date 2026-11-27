//! GNU Emacs 31.1 parity for `:post-read-conversion` and `:pre-write-conversion`
//! installed via `coding-system-put` (including on `define-coding-system-alias`
//! names, EOL subsidiaries, and dedicated codecs) and `CODING_FOR_UNIBYTE` hook
//! temp-buffer multibyteness.
//!
//! GNU Emacs 31.1 (`a360712c9d27`) source contract:
//! - `Fdefine_coding_system_alias` (`src/coding.c:11581-11618`): binds the alias
//!   (and its `-unix`/`-dos`/`-mac` subsidiaries) to the same shared `spec`
//!   entry in `Vcoding_system_hash_table`.
//! - `Fcoding_system_put` (`src/coding.c:11511-11578`): updates the typed
//!   attributes `coding_attr_post_read` (`:11560-11564`) and
//!   `coding_attr_pre_write` (`:11565-11569`) on the shared `attrs` vector
//!   (validating with `CHECK_SYMBOL` only for the keywords
//!   `:post-read-conversion` / `:pre-write-conversion`) and updates
//!   `CODING_ATTR_PLIST` via `plist_put` (`:11575-11576`).
//! - `decode_coding_object` (`src/coding.c:8133-8197`): when `dst_object` is
//!   `Qt` (string return), `coding->dst_multibyte = !CODING_FOR_UNIBYTE(coding)`
//!   (`:8137`); when `dst_object` is a buffer, `coding->dst_multibyte` comes
//!   from the destination buffer (`:8155-8156`); and `CODING_ATTR_POST_READ`
//!   runs after `decode_coding` (`:8180-8197`) regardless of codec family.

use crate::common::assert_oracle_parity_expect;
use expect_test::expect;

#[test]
fn coding_system_put_post_read_on_raw_text_alias_runs_hook() {
    let form = r#"(progn
  (defun t1-del (_len) (delete-region (point-min) (point-max)) 0)
  (define-coding-system-alias 't1-raw 'raw-text)
  (unwind-protect
      (progn
        (coding-system-put 't1-raw :post-read-conversion 't1-del)
        (list
         ;; Alias into unibyte buffer -> hook empties buffer, returns 0
         (with-temp-buffer
           (set-buffer-multibyte nil)
           (let ((r (decode-coding-string (unibyte-string #xc3 #xa9) 't1-raw nil (current-buffer))))
             (list r (string-to-list (buffer-string)) last-coding-system-used)))
         ;; Shared spec: base `raw-text` also runs the hook
         (with-temp-buffer
           (set-buffer-multibyte nil)
           (let ((r (decode-coding-string (unibyte-string #xc3 #xa9) 'raw-text nil (current-buffer))))
             (list r (string-to-list (buffer-string)) last-coding-system-used)))
         ;; Shared spec: EOL subsidiary `t1-raw-unix` also runs the hook
         (with-temp-buffer
           (set-buffer-multibyte nil)
           (let ((r (decode-coding-string (unibyte-string #xc3 #xa9) 't1-raw-unix nil (current-buffer))))
             (list r (string-to-list (buffer-string)) last-coding-system-used)))
         ;; Alias into multibyte buffer
         (with-temp-buffer
           (let ((r (decode-coding-string (unibyte-string #xc3 #xa9) 't1-raw nil (current-buffer))))
             (list r (string-to-list (buffer-string)))))
         ;; Alias returning string
         (string-to-list (decode-coding-string (unibyte-string #xc3 #xa9) 't1-raw))
         ;; decode-coding-region in-place in unibyte buffer
         (with-temp-buffer
           (set-buffer-multibyte nil)
           (insert (unibyte-string #xc3 #xa9))
           (let ((r (decode-coding-region (point-min) (point-max) 't1-raw)))
             (list r (point) (string-to-list (buffer-string)))))
         ;; Clearing hook on base restores normal raw-text decoding on alias
         (progn
           (coding-system-put 'raw-text :post-read-conversion nil)
           (with-temp-buffer
             (set-buffer-multibyte nil)
             (let ((r (decode-coding-string (unibyte-string #xc3 #xa9) 't1-raw nil (current-buffer))))
               (list r (string-to-list (buffer-string))))))))
    (coding-system-put 'raw-text :post-read-conversion nil)))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK ((0 nil t1-raw) (0 nil raw-text) (0 nil t1-raw-unix) (0 nil) nil (0 1 nil) (2 (195 169)))""#
        ]],
    );
}

#[test]
fn coding_system_put_post_read_for_unibyte_string_and_region_destinations() {
    let form = r#"(progn
  (defvar t2-obs nil)
  (defun t2-rec (len)
    (setq t2-obs (list len (- (point-max) (point-min))
                       enable-multibyte-characters
                       (string-to-list (buffer-string))))
    len)
  (define-coding-system-alias 't2-raw 'raw-text)
  (define-coding-system-alias 't2-nc 'no-conversion)
  (define-coding-system 't2-raw-nofu "d"
    :coding-type 'raw-text :mnemonic ?R :post-read-conversion 't2-rec)
  (define-coding-system 't2-utf8-fu "d"
    :coding-type 'utf-8 :mnemonic ?U :for-unibyte t :post-read-conversion 't2-rec)
  (unwind-protect
      (progn
        ;; Putting on a subsidiary propagates to the alias and base
        (coding-system-put 't2-raw-unix :post-read-conversion 't2-rec)
        (coding-system-put 't2-nc :post-read-conversion 't2-rec)
        (list
         ;; raw-text (for-unibyte=t) with string return -> unibyte temp buf & unibyte result
         (let ((s (decode-coding-string (unibyte-string #xc3 #xa9) 't2-raw)))
           (list (multibyte-string-p s) (string-to-list s) t2-obs))
         ;; raw-text into multibyte buffer -> multibyte hook buf, eight-bit chars
         (with-temp-buffer
           (let ((r (decode-coding-string (unibyte-string #xc3 #xa9) 't2-raw nil (current-buffer))))
             (list r (string-to-list (buffer-string)) t2-obs)))
         ;; decode-coding-region with DESTINATION=t on raw-text -> unibyte
         (with-temp-buffer
           (set-buffer-multibyte nil)
           (insert (unibyte-string #xc3 #xa9))
           (let ((s (decode-coding-region (point-min) (point-max) 't2-raw t)))
             (list (multibyte-string-p s) (string-to-list s) t2-obs)))
         ;; no-conversion alias with CRLF -> unibyte, no EOL conversion
         (let ((s (decode-coding-string (unibyte-string #xc3 #xa9 ?\r ?\n) 't2-nc)))
           (list (multibyte-string-p s) (string-to-list s) t2-obs))
         ;; Direct :coding-type raw-text WITHOUT :for-unibyte -> multibyte string return
         (let ((s (decode-coding-string (unibyte-string #xc3 #xa9) 't2-raw-nofu)))
           (list (multibyte-string-p s) (string-to-list s) t2-obs))
         ;; Direct :coding-type utf-8 WITH :for-unibyte t -> unibyte string return
         (let ((s (decode-coding-string (unibyte-string #xc3 #xa9) 't2-utf8-fu)))
           (list (multibyte-string-p s) (string-to-list s) t2-obs))))
    (coding-system-put 'raw-text :post-read-conversion nil)
    (coding-system-put 'no-conversion :post-read-conversion nil)))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK ((nil (195 169) (2 2 nil (195 169))) (2 (4194243 4194217) (2 2 t (4194243 4194217))) (nil (195 169) (2 2 nil (195 169))) (nil (195 169 13 10) (4 4 nil (195 169 13 10))) (t (4194243 4194217) (2 2 t (4194243 4194217))) (nil (195 169) (1 2 nil (195 169))))""#
        ]],
    );
}

#[test]
fn coding_system_put_post_read_and_pre_write_on_dedicated_and_aliased_codings() {
    let form = r#"(progn
  (defvar t3-obs nil)
  (defun t3-del (_len) (delete-region (point-min) (point-max)) 0)
  (defun t3-rec (len)
    (setq t3-obs (list len (- (point-max) (point-min))
                       enable-multibyte-characters
                       (string-to-list (buffer-string))))
    len)
  (defun t3-pre (from _to)
    (save-excursion (goto-char from) (insert "P:"))
    nil)
  (define-coding-system-alias 't3-raw 'raw-text)
  (define-coding-system-alias 't3-utf8 'utf-8)
  (define-coding-system-alias 't3-l1 'iso-latin-1)
  (define-coding-system-alias 't3-euc 'euc-jp)
  (define-coding-system-alias 't3-viqr 'vietnamese-viqr)
  (define-coding-system-alias 't3-u7 'utf-7)
  (let ((orig-u7 (coding-system-get 'utf-7 :post-read-conversion)))
    (unwind-protect
        (progn
          (coding-system-put 't3-utf8 :post-read-conversion 't3-del)
          (coding-system-put 't3-l1 :post-read-conversion 't3-del)
          (coding-system-put 't3-euc :post-read-conversion 't3-del)
          (coding-system-put 't3-raw :pre-write-conversion 't3-pre)
          (coding-system-put 't3-l1 :pre-write-conversion 't3-pre)
          (coding-system-put 't3-u7 :post-read-conversion 't3-rec)
          (list
           ;; utf-8 alias post-read hook deleting 2 unibyte bytes -> -1
           (with-temp-buffer
             (set-buffer-multibyte nil)
             (list (decode-coding-string (unibyte-string #xc3 #xa9) 't3-utf8 nil (current-buffer))
                   (string-to-list (buffer-string))))
           ;; iso-latin-1 (dedicated charset codec) alias post-read hook -> -1
           (with-temp-buffer
             (set-buffer-multibyte nil)
             (list (decode-coding-string (unibyte-string #xe9) 't3-l1 nil (current-buffer))
                   (string-to-list (buffer-string))))
           ;; euc-jp (dedicated EUC codec) alias post-read hook deleting 3 UTF-8 bytes -> -2
           (with-temp-buffer
             (set-buffer-multibyte nil)
             (list (decode-coding-string (unibyte-string #xa4 #xa2) 't3-euc nil (current-buffer))
                   (string-to-list (buffer-string))))
           ;; raw-text alias pre-write hook
           (string-to-list (encode-coding-string (string #xe9) 't3-raw))
           ;; iso-latin-1 (dedicated charset codec) alias pre-write hook
           (string-to-list (encode-coding-string (string #xe9) 't3-l1))
           ;; utf-7 alias with overridden post-read hook uses base utf-8 + custom hook
           (let ((s (decode-coding-string (unibyte-string #xc3 #xa9) 't3-u7)))
             (list (multibyte-string-p s) (string-to-list s) t3-obs))
           ;; vietnamese-viqr alias into unibyte and multibyte buffers
           (with-temp-buffer
             (set-buffer-multibyte nil)
             (list (decode-coding-string "Vie^.t" 't3-viqr nil (current-buffer))
                   (string-to-list (buffer-string))))
           (with-temp-buffer
             (list (decode-coding-string "Vie^.t" 't3-viqr nil (current-buffer))
                   (string-to-list (buffer-string))))))
      (coding-system-put 'utf-8 :post-read-conversion nil)
      (coding-system-put 'iso-latin-1 :post-read-conversion nil)
      (coding-system-put 'iso-latin-1 :pre-write-conversion nil)
      (coding-system-put 'euc-jp :post-read-conversion nil)
      (coding-system-put 'raw-text :pre-write-conversion nil)
      (coding-system-put 'utf-7 :post-read-conversion orig-u7))))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK ((-1 nil) (-1 nil) (-2 nil) (80 58 195 169) (80 58 233) (t (233) (1 1 t (233))) (4 (86 105 199 116)) (4 (86 105 7879 116)))""#
        ]],
    );
}

#[test]
fn coding_system_put_hook_plist_order_and_validation() {
    let form = r#"(progn
  (define-coding-system-alias 't4-raw 'raw-text)
  (unwind-protect
      (let* ((before-keys (let ((p (coding-system-plist 't4-raw)) (ks nil))
                            (while p (push (car p) ks) (setq p (cddr p)))
                            (nreverse ks)))
             (_ (coding-system-put 't4-raw :post-read-conversion 'identity))
             (after-keys (let ((p (coding-system-plist 't4-raw)) (ks nil))
                           (while p (push (car p) ks) (setq p (cddr p)))
                           (nreverse ks)))
             (bare-ok (coding-system-put 't4-raw 'post-read-conversion 123))
             (kw-err (condition-case e
                         (coding-system-put 't4-raw :post-read-conversion 123)
                       (error (list (car e) (cadr e))))))
        (list (equal after-keys (append before-keys '(:post-read-conversion)))
              bare-ok
              kw-err))
    (coding-system-put 'raw-text :post-read-conversion nil)
    (coding-system-put 'raw-text 'post-read-conversion nil)))"#;

    assert_oracle_parity_expect(
        form,
        expect![[r#""OK (t 123 (wrong-type-argument symbolp))""#]],
    );
}
