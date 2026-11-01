//! Complex combo batch 428 — 18 more edge probes: process-property deep,
//! process-buffer with sentinel, process-command, process-coding-system
//! for non-process, network-interface-list, network-interface-info,
//! serial-process-configure, file-acl, file-selinux-context,
//! file-backup-file-names, file-remote-p, file-local-copy,
//! file-exists-p on broken symlink, executable-find, user-real-uid,
//! group-gid, user-real-login-name, user-full-name deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// process-command / process-buffer / process-property.
#[test]
fn div_cx428_process_command_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"echo\" \"hello\") nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((proc (make-process :name "neo-cx428-pc"
                          :command '("echo" "hello")
                          :connection-type 'pipe :buffer nil)))
  (accept-process-output proc 2)
  (prog1 (list (process-command proc)
               (process-buffer proc))
    (delete-process proc)))
"##,
        expect,
    );
}

/// network-interface-list / network-interface-info (may be stubbed).
#[test]
fn div_cx428_network_interface() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // The loopback interface is "lo" on GNU/Linux and "lo0" on Darwin, and
    // GNU answers nil for an interface name the host does not have:
    // `network_interface_info' asks the kernel about IFNAME and returns nil
    // when the ioctl finds no interface (src/process.c:4485; the DEFUN at
    // 4677-4696).  The name->answer pair is therefore a host fact: compare
    // Neo against the local GNU, never against a Linux literal.
    crate::common::assert_oracle_parity(
        r##"
(list (condition-case e (listp (network-interface-list)) (error (car e)))
      (condition-case e (network-interface-info "lo") (error (car e))))
"##,
    );
}

/// file-acl / file-selinux-context (may be stubbed).
#[test]
fn div_cx428_file_acl_selinux() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil (nil nil nil nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-cx428-acl-")))
  (unwind-protect
      (list (condition-case e (file-acl f) (error (car e)))
            (condition-case e (file-selinux-context f) (error (car e))))
    (delete-file f)))
"##,
        expect,
    );
}

/// file-backup-file-names / file-remote-p.
#[test]
fn div_cx428_file_backup_remote() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (file-backup-file-names "/tmp/test.el")
      (file-remote-p "/tmp/test.el"))
"##,
        expect,
    );
}

/// file-exists-p on symlink / file-symlink-p.
#[test]
fn div_cx428_file_symlink() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // Use a private directory: `file-equal-p' compares the complete
    // `file-attributes' lists of the two truenames (lisp/files.el:6858-6872),
    // so a shared directory such as /tmp answers nil whenever another process
    // changes its mtime between the two reads (the 2026-11-01 oracle shard 18
    // failure under many nextest workers).  Nothing else writes inside the
    // private tree between the calls, and the relative symlink keeps the
    // printed target host-independent.
    let expect = expect_test::expect![[r#""OK (t \"target\" t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((root (make-temp-file "neo-cx428-" t))
       (target (expand-file-name "target" root))
       (link (expand-file-name "link" root)))
  (unwind-protect
      (progn
        (make-directory target)
        (make-symbolic-link "target" link)
        (let ((true (file-truename link)))
          (list (file-exists-p link)
                (file-symlink-p link)
                (file-directory-p link)
                (file-equal-p link target)
                (file-equal-p link true)
                (equal true (file-truename true)))))
    (ignore-errors (delete-directory root t))))
"##,
        expect,
    );
}

/// executable-find: finding executables on PATH.
#[test]
fn div_cx428_executable_find() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t \"sh\" nil t \"echo\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((sh (executable-find "sh"))
      (missing (executable-find "nonexistent-command-cx428"))
      (echo (executable-find "echo")))
  (list (and sh (file-executable-p sh))
        (and sh (file-name-nondirectory sh))
        missing
        (and echo (file-executable-p echo))
        (and echo (file-name-nondirectory echo))))
"##,
        expect,
    );
}

/// user-real-uid / group-gid / user-real-login-name.
#[test]
fn div_cx428_user_real_uid_gid() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // A temporary file's group is inherited from its directory, so a setgid
    // or shared worktree can hold a different group than the process gid; the
    // old file-group relation asserted that layout, not GNU's group-gid.
    // Assert the process and file-owner contract that holds on every host and
    // compare Neo against the local GNU.
    crate::common::assert_oracle_parity(
        r##"
(let ((path (make-temp-file "neo-cx428-owner-")))
  (unwind-protect
      (let ((attributes (file-attributes path 'integer)))
        (list (= (user-real-uid) (user-uid))
              (= (user-real-uid) (file-attribute-user-id attributes))
              (integerp (group-gid))
              (stringp (user-real-login-name))))
    (delete-file path)))
"##,
    );
}

/// user-full-name / user-login-name deep.
#[test]
fn div_cx428_user_full_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (stringp (user-full-name))
      (stringp (user-login-name))
      (stringp (user-uid)))
"##,
        expect,
    );
}

/// read-kbd-macro / single-key-description edge.
#[test]
fn div_cx428_read_kbd_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ([3 6] [134217848])""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (read-kbd-macro "C-c C-f")
      (read-kbd-macro "M-x"))
"##,
        expect,
    );
}

/// define-key with key vector vs string.
#[test]
fn div_cx428_define_key_vector_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil elisp-byte-compile-buffer)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((map (make-sparse-keymap)))
  (define-key map [?\C-c ?\C-f] 'forward-char)
  (define-key map "\C-c\C-b" 'backward-char)
  (list (key-binding [?\C-c ?\C-f] nil nil map)
        (key-binding "\C-c\C-b" nil nil map)))
"##,
        expect,
    );
}

/// list-packages / package-installed-p (package system).
#[test]
fn div_cx428_list_packages() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'package)
  (list (boundp 'package-archives)
        (package-installed-p 'emacs)))
"##,
        expect,
    );
}

/// face-font: getting face font information.
#[test]
fn div_cx428_face_font() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // A build without a window system (--without-x, the macOS CI oracle
    // profile) makes GNU's `face-font' answer the frame-type sentinel "tty"
    // (or "ms-dos"/"w32term") instead of the font set on the face
    // (src/xfaces.c:4468-4472); a window-system build answers the realized
    // font or nil.  Normalize the sentinel so one contract holds against
    // either reference profile: the face is valid and its font is either nil
    // or a real font name.
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((f (make-face 'neo-cx428-ff))
       (font (progn (set-face-font f "Monospace-10") (face-font f))))
  (if (member font '("tty" "ms-dos" "w32term")) nil font))
"##,
        expect,
    );
}

/// char-displayable-p: checking char display capability.
#[test]
fn div_cx428_char_displayable_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t unicode unicode unicode)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (char-displayable-p ?a)
      (char-displayable-p ?é)
      (char-displayable-p ?世)
      (char-displayable-p #x1F600))
"##,
        expect,
    );
}

/// buffer-hash / sha1 of buffer content.
#[test]
fn div_cx428_buffer_hash_sha() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"8ad95ad9cf041daa6635bf314ae42a7a7a1f781e\" \"8ad95ad9cf041daa6635bf314ae42a7a7a1f781e\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "content to hash")
  (list (buffer-hash)
        (secure-hash 'sha1 (current-buffer))))
"##,
        expect,
    );
}

/// window-configuration-to-register / frame-configuration-to-register.
#[test]
fn div_cx428_window_config_register() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"test register\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "test register")
  (window-configuration-to-register ?w)
  (jump-to-register ?w)
  (buffer-string))
"##,
        expect,
    );
}

/// define-mail-abbrev / mail-abbrevs (mail aliases).
#[test]
fn div_cx428_mail_abbrevs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'mailabbrev)
  (list (boundp 'mail-abbrevs)
        (fboundp 'define-mail-abbrev)))
"##,
        expect,
    );
}

/// process-send-string with filter that inserts.
#[test]
fn div_cx428_process_filter_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"\\nProcess neo-cx428-pfi finished\\n[hello-from-filter\\n]\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *cx428-pfi*")))
  (let ((proc (make-process :name "neo-cx428-pfi"
                            :command '("echo" "hello-from-filter")
                            :connection-type 'pipe
                            :buffer buf
                            :filter (lambda (p s)
                                      (with-current-buffer (process-buffer p)
                                        (insert "[" s "]"))))))
    (set-process-query-on-exit-flag proc nil)
    (let ((i 0))
      (while (and (process-live-p proc) (< i 100))
        (accept-process-output proc 0.02)
        (setq i (1+ i))))
    (prog1 (with-current-buffer buf
             (string-trim-right (buffer-string)))
      (when (process-live-p proc)
        (delete-process proc))
      (kill-buffer buf))))
"##,
        expect,
    );
}
