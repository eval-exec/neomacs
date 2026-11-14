"""Default-disposition pipe child: EOF is observable only absent a shutdown HUP.

GNU Emacs 31.1 (src/emacs.c shut_down_emacs -> src/process.c
kill_buffer_processes) sends SIGHUP to a real pipe child before Emacs exits,
so a default-disposition child dies at kill-emacs and never observes its
stdin pipe closing.  If the editor instead exits without signalling, this
helper survives long enough to read EOF, record it, and exit cleanly, which
the supervisor reports as a divergence.
"""
import os
from pathlib import Path
import sys
import time

root = Path(sys.argv[1])
parent = os.getppid()
ready = root / "child.ready.tmp"
ready.write_text(f"{os.getpid()} {parent} {os.getpgrp()} {os.getsid(0)}\n")
ready.replace(root / "child.ready")
deadline = time.monotonic() + 30
# Before GO, a failed editor/setup must not leave this fixture running.
while not (root / "go").exists():
    if os.getppid() != parent or time.monotonic() >= deadline:
        sys.exit(0)
    time.sleep(0.01)
data = sys.stdin.buffer.read()
(root / "child.eof").write_text(f"{len(data)}\n")
# Linger bounded; the supervisor reaps through the pidfd it captured.
while os.getppid() == parent and time.monotonic() < deadline:
    time.sleep(0.01)
