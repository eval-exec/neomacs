#!/usr/bin/env python3
"""Run an editor in a pty and wait for it to exit, capturing its output.

Ledger 195.  Companion to scripts/motion-parity-audit.el: gives the editor a
real terminal so `noninteractive' is nil and GNU's DISPLAY-ITERATOR arm of
`Fvertical_motion' (src/indent.c:2287) is the one under test.

  L195_COLS / L195_ROWS   pty size, default 160x50 (the neomacs-tui-tests
                          geometry, crates/neomacs-tui-tests/src/lib.rs:38-39)
  L195_TIMEOUT            seconds before SIGKILL, default 180
"""
import os, pty, sys, select, signal, time, struct, fcntl, termios

def main():
    prog = sys.argv[1]
    args = sys.argv[1:]
    timeout = float(os.environ.get("L195_TIMEOUT", "180"))
    pid, fd = pty.fork()
    if pid == 0:
        os.environ["TERM"] = "screen-256color"
        os.environ["COLUMNS"] = os.environ.get("L195_COLS", "160")
        os.environ["LINES"] = os.environ.get("L195_ROWS", "50")
        os.environ.pop("RUST_LOG", None)
        try:
            os.execvp(prog, args)
        except OSError:
            pass
        # execvp RAISES on failure rather than returning, so this line was
        # unreachable and a missing editor surfaced as a Python traceback with
        # exit 1 (ledger 210).  127 is the shell's answer and the useful one.
        os._exit(127)
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", int(os.environ.get("L195_ROWS","50")), int(os.environ.get("L195_COLS","160")), 0, 0))
    start = time.time()
    out = bytearray()
    reaped = None
    while True:
        if time.time() - start > timeout:
            os.kill(pid, signal.SIGKILL)
            sys.stderr.write("L195 TIMEOUT\n")
            os.waitpid(pid, 0)
            sys.exit(124)
        r, _, _ = select.select([fd], [], [], 1.0)
        if r:
            try:
                data = os.read(fd, 65536)
            except OSError:
                break
            if not data:
                break
            out.extend(data)
        wpid, status = os.waitpid(pid, os.WNOHANG)
        if wpid == pid:
            reaped = status
            # drain
            while True:
                r, _, _ = select.select([fd], [], [], 0.2)
                if not r:
                    break
                try:
                    data = os.read(fd, 65536)
                except OSError:
                    break
                if not data:
                    break
                out.extend(data)
            break
    if reaped is None:
        # The pty closed before the child was reaped -- wait for it, so the
        # status below is the EDITOR's and not this driver's optimism.
        _, reaped = os.waitpid(pid, 0)
    sys.stdout.buffer.write(bytes(out))
    # Ledger 210: this used to be `sys.exit(0)' unconditionally, so an editor
    # that crashed, refused its arguments or died on a signal was reported as a
    # successful sweep and only `lines=MISSING' downstream said otherwise.  A
    # driver that cannot fail is a false-green generator.
    code = os.waitstatus_to_exitcode(reaped)
    # A signal comes back negative; report it the way a shell does.
    sys.exit(128 - code if code < 0 else code)

main()
