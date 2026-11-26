//! GNU Emacs 31.1 `SIGTTOU` disposition inheritance parity.
//!
//! GNU Emacs never sets `SIGTTOU` to `SIG_IGN` globally (`src/sysdep.c:924-960`
//! and `src/term.c:4354-4356,4485-4489` only temporarily block `SIGTTOU` with
//! `pthread_sigmask` around `tcsetpgrp`, `TIOCNOTTY`, and `tgetent`). Both
//! Emacs itself and every subprocess (pipe and PTY) therefore inherit the
//! parent's `SIGTTOU` disposition unchanged: `SIG_DFL` when launched with
//! `SIG_DFL`, and `SIG_IGN` when launched with `SIG_IGN`.

#[cfg(unix)]
use crate::common::assert_oracle_parity_with_sigttou_expect;
#[cfg(unix)]
use expect_test::expect;
#[cfg(unix)]
use neomacs_test_env::SignalDisposition;

/// On Linux, `/proc/self/status` `SigIgn` bit 21 (`0x200000`, `SIGTTOU` = 22)
/// must be clear in `self`, a pipe child, and a PTY child when the parent
/// launches the editor with `SIGTTOU = SIG_DFL`, and set in all three when the
/// parent launches the editor with `SIGTTOU = SIG_IGN`.
#[cfg(target_os = "linux")]
#[test]
fn oracle_sigttou_proc_status_inheritance_default_and_ignored() {
    let form = r#"(let ((sigttou-bit-ignored
       (lambda (hex)
         (let ((trimmed (and (stringp hex) (string-trim hex))))
           (if (and trimmed (string-match-p "\\`[0-9a-f]+\\'" trimmed))
               (if (/= 0 (logand (string-to-number trimmed 16) (ash 1 21)))
                   'ign
                 'dfl)
             (list 'bad-hex hex)))))
      (read-self-sigign
       (lambda ()
         (with-temp-buffer
           (insert-file-contents "/proc/self/status")
           (goto-char (point-min))
           (when (re-search-forward "^SigIgn:\t\\([0-9a-f]+\\)$" nil t)
             (match-string 1)))))
      (read-child-sigign
       (lambda (conn-type)
         (let ((buf (generate-new-buffer " *sigttou-probe*")))
           (unwind-protect
               (let ((p (make-process
                         :name "sigttou-probe"
                         :buffer buf
                         :connection-type conn-type
                         :sentinel #'ignore
                         :command (list "/bin/sh" "-c"
                                        "awk '/^SigIgn:/{print $2}' /proc/self/status"))))
                 (neovm--oracle-settle-process p)
                 (while (accept-process-output p 0.05))
                 (with-current-buffer buf
                   (string-trim (buffer-string))))
             (kill-buffer buf))))))
  (list (funcall sigttou-bit-ignored (funcall read-self-sigign))
        (funcall sigttou-bit-ignored (funcall read-child-sigign 'pipe))
        (funcall sigttou-bit-ignored (funcall read-child-sigign 'pty))))"#;

    assert_oracle_parity_with_sigttou_expect(
        form,
        SignalDisposition::Default,
        expect![[r#""OK (dfl dfl dfl)""#]],
    );
    assert_oracle_parity_with_sigttou_expect(
        form,
        SignalDisposition::Ignore,
        expect![[r#""OK (ign ign ign)""#]],
    );
}

/// POSIX non-interactive `/bin/sh` refuses to trap a signal that was `SIG_IGN`
/// on entry. With parent `SIGTTOU = SIG_DFL`, both pipe and PTY shell children
/// must be able to trap `SIGTTOU`; with parent `SIGTTOU = SIG_IGN`, both must
/// ignore the trap.
#[cfg(unix)]
#[test]
fn oracle_sigttou_posix_sh_trap_inheritance_default_and_ignored() {
    let form = r#"(let ((run-sh
       (lambda (conn-type)
         (let ((buf (generate-new-buffer " *sigttou-sh*")))
           (unwind-protect
               (let ((p (make-process
                         :name "sigttou-sh"
                         :buffer buf
                         :connection-type conn-type
                         :sentinel #'ignore
                         :command (list "/bin/sh" "-c"
                                        "trap 'echo CAUGHT' TTOU; kill -TTOU $$; echo DONE"))))
                 (neovm--oracle-settle-process p)
                 (while (accept-process-output p 0.05))
                 (with-current-buffer buf
                   (split-string (string-trim (buffer-string)) "[\r\n]+" t)))
             (kill-buffer buf))))))
  (list (funcall run-sh 'pipe)
        (funcall run-sh 'pty)))"#;

    assert_oracle_parity_with_sigttou_expect(
        form,
        SignalDisposition::Default,
        expect![[r#""OK ((\"CAUGHT\" \"DONE\") (\"CAUGHT\" \"DONE\"))""#]],
    );
    assert_oracle_parity_with_sigttou_expect(
        form,
        SignalDisposition::Ignore,
        expect![[r#""OK ((\"DONE\") (\"DONE\"))""#]],
    );
}

/// Observable job-control parity in a PTY child: a background-pgrp grandchild
/// calling `tcsetpgrp(0, getpgrp())` on its controlling PTY must stop with
/// `SIGTTOU` when the editor was started with `SIGTTOU = SIG_DFL`, and succeed
/// without stopping when started with `SIGTTOU = SIG_IGN`.
#[cfg(unix)]
#[test]
fn oracle_sigttou_background_pgrp_tcsetpgrp_stops_under_default_only() {
    let form = r#"(let ((buf (generate-new-buffer " *sigttou-tcsetpgrp*"))
      (py (concat
           "import os, signal\n"
           "r, w = os.pipe()\n"
           "pid = os.fork()\n"
           "if pid == 0:\n"
           "    os.close(w)\n"
           "    os.read(r, 1)\n"
           "    os.close(r)\n"
           "    os.tcsetpgrp(0, os.getpgrp())\n"
           "    os._exit(0)\n"
           "else:\n"
           "    os.close(r)\n"
           "    os.setpgid(pid, pid)\n"
           "    os.write(w, b'x')\n"
           "    os.close(w)\n"
           "    _, st = os.waitpid(pid, os.WUNTRACED)\n"
           "    if os.WIFSTOPPED(st):\n"
           "        sig = os.WSTOPSIG(st)\n"
           "        os.kill(pid, signal.SIGKILL)\n"
           "        os.waitpid(pid, 0)\n"
           "        print('stopped:SIGTTOU' if sig == signal.SIGTTOU else f'stopped:{sig}')\n"
           "    elif os.WIFEXITED(st):\n"
           "        print(f'exited:{os.WEXITSTATUS(st)}')\n"
           "    else:\n"
           "        print(f'other:{st}')\n")))
  (unwind-protect
      (let ((p (make-process
                :name "sigttou-tcsetpgrp"
                :buffer buf
                :connection-type 'pty
                :sentinel #'ignore
                :command (list "python3" "-c" py))))
        (neovm--oracle-settle-process p)
        (while (accept-process-output p 0.05))
        (with-current-buffer buf
          (string-trim (buffer-string))))
    (kill-buffer buf)))"#;

    assert_oracle_parity_with_sigttou_expect(
        form,
        SignalDisposition::Default,
        expect![[r#""OK \"stopped:SIGTTOU\"""#]],
    );
    assert_oracle_parity_with_sigttou_expect(
        form,
        SignalDisposition::Ignore,
        expect![[r#""OK \"exited:0\"""#]],
    );
}
